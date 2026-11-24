//! Opt-in Python extraction process. The UI only submits work; one supervisor
//! owns the child, bounds replies, rejects stale IDs and restarts on bad IPC.
use crate::{canonical_video_url, parse_provider_json, trace_event, trace_id, ExtractionResult, ProbeError, Provider};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{atomic::{AtomicU64, Ordering}, mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

const REQUEST_BUDGET: Duration = Duration::from_secs(12);
const STARTUP_BUDGET: Duration = Duration::from_secs(8);
const POLL: Duration = Duration::from_millis(40);
const MAX_REPLY: u64 = 16 * 1024 * 1024;
const SHUTDOWN: u64 = u64::MAX;

type Reply = Result<ExtractionResult, ProbeError>;
struct Request {
    id: u64,
    url: String,
    started: Instant,
    response: mpsc::Sender<Reply>,
}

pub struct PersistentWorker {
    requests: Option<mpsc::Sender<Request>>,
    latest: Arc<AtomicU64>,
    supervisor: Option<thread::JoinHandle<()>>,
}

pub struct Ticket {
    id: u64,
    latest: Arc<AtomicU64>,
    response: mpsc::Receiver<Reply>,
}

fn error(code: &str) -> ProbeError {
    ProbeError {
        code: code.into(),
        message: "Persistent extraction did not return a usable response.".into(),
        action: "Retry using standard yt-dlp.".into(),
    }
}

impl Ticket {
    pub fn id(&self) -> u64 { self.id }
    pub fn is_current(&self) -> bool { self.latest.load(Ordering::Acquire) == self.id }
    pub fn resolve(&self) -> Reply {
        self.response.recv_timeout(REQUEST_BUDGET + Duration::from_secs(2))
            .unwrap_or_else(|_| Err(error("persistent_supervisor_timeout")))
    }
}

impl PersistentWorker {
    pub fn start(python: PathBuf, script: PathBuf, timings: bool) -> Self {
        let (requests, receiver) = mpsc::channel();
        let latest = Arc::new(AtomicU64::new(0));
        let generation = latest.clone();
        let supervisor = thread::spawn(move || supervise(&python, &script, timings, receiver, generation));
        Self { requests: Some(requests), latest, supervisor: Some(supervisor) }
    }

    pub fn submit(&self, url: &str) -> Ticket {
        let id = trace_id();
        self.latest.store(id, Ordering::Release);
        let (response, receiver) = mpsc::channel();
        if let Some(requests) = &self.requests {
            let _ = requests.send(Request { id, url: url.into(), started: Instant::now(), response });
        }
        Ticket { id, latest: self.latest.clone(), response: receiver }
    }
}

impl Drop for PersistentWorker {
    fn drop(&mut self) {
        self.latest.store(SHUTDOWN, Ordering::Release);
        drop(self.requests.take());
        if let Some(supervisor) = self.supervisor.take() { let _ = supervisor.join(); }
    }
}

struct Process {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Vec<u8>>,
    requests_served: u64,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Process {
    fn spawn(python: &Path, script: &Path) -> Result<Self, &'static str> {
        let mut command = Command::new(python);
        command.arg("-u").arg(script)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().map_err(|_| "persistent_spawn")?;
        let input = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, output) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                // A malformed or unbounded frame must not consume arbitrary memory.
                let read = (&mut reader).take(MAX_REPLY + 1).read_until(b'\n', &mut line);
                match read {
                    Ok(n) if n > 0 && n as u64 <= MAX_REPLY && line.last() == Some(&b'\n') => {
                        if tx.send(line).is_err() { break; }
                    }
                    _ => break,
                }
            }
        });
        Ok(Self { child, input, output, requests_served: 0 })
    }

    fn receive(&self, deadline: Instant, current: impl Fn() -> bool) -> Result<Vec<u8>, &'static str> {
        loop {
            if !current() { return Err("persistent_cancelled"); }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { return Err("persistent_timeout"); }
            match self.output.recv_timeout(POLL.min(remaining)) {
                Ok(line) => {
                    if !current() { return Err("persistent_cancelled"); }
                    return Ok(line);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err("persistent_pipe"),
            }
        }
    }

    fn request(&mut self, request: &Request, latest: &AtomicU64, timings: bool) -> Result<Reply, &'static str> {
        let (video_id, url) = match canonical_video_url(&request.url) {
            Ok(value) => value,
            Err(err) => return Ok(Err(err)),
        };
        let current = || latest.load(Ordering::Acquire) == request.id;
        if !current() { return Err("persistent_cancelled"); }
        let deadline = request.started + REQUEST_BUDGET;
        if Instant::now() >= deadline { return Err("persistent_timeout"); }
        let sent = Instant::now();
        let mut line = serde_json::to_vec(&json!({"id": request.id, "url": url})).map_err(|_| "persistent_protocol")?;
        line.push(b'\n');
        self.input.write_all(&line).and_then(|_| self.input.flush()).map_err(|_| "persistent_pipe")?;
        let bytes = self.receive(deadline, current)?;
        let wire_ms = sent.elapsed().as_millis();
        let parse_started = Instant::now();
        let response: Value = serde_json::from_slice(&bytes).map_err(|_| "persistent_protocol")?;
        if response["id"].as_u64() != Some(request.id) { return Err("persistent_id_mismatch"); }
        let outcome = match response["ok"].as_bool() {
            Some(true) => {
                let payload = response["payload"].as_str().ok_or("persistent_protocol")?;
                parse_provider_json(Provider::YtDlp, &video_id, payload)
            }
            Some(false) => Err(error("persistent_extraction_failed")),
            None => return Err("persistent_protocol"),
        };
        if timings {
            trace_event("persistent_reply", format_args!(
                "id={} total_ms={} wire_ms={} extract_ms={} serialize_ms={} parse_us={} bytes={} retries={} resets={} timeouts={} solver_failures={} success={}",
                request.id, request.started.elapsed().as_millis(), wire_ms,
                response["extract_ms"].as_u64().unwrap_or(0), response["serialize_ms"].as_u64().unwrap_or(0),
                parse_started.elapsed().as_micros(), bytes.len(),
                response["retries"].as_u64().unwrap_or(0), response["resets"].as_u64().unwrap_or(0),
                response["timeouts"].as_u64().unwrap_or(0), response["solver_failures"].as_u64().unwrap_or(0), outcome.is_ok()));
        }
        if !current() { return Err("persistent_cancelled"); }
        self.requests_served += 1;
        Ok(outcome)
    }
}

fn launch(python: &Path, script: &Path, deadline: Instant, current: impl Fn() -> bool, timings: bool) -> Result<Process, &'static str> {
    let started = Instant::now();
    let process = Process::spawn(python, script)?;
    let ready = process.receive(deadline, current)?;
    let message: Value = serde_json::from_slice(&ready).map_err(|_| "persistent_protocol")?;
    if message["ready"].as_u64() != Some(1) { return Err("persistent_protocol"); }
    if timings { trace_event("persistent_ready", format_args!("elapsed_ms={}", started.elapsed().as_millis())); }
    Ok(process)
}

fn supervise(python: &Path, script: &Path, timings: bool, receiver: mpsc::Receiver<Request>, latest: Arc<AtomicU64>) {
    // Prewarm asynchronously. Failure is recoverable; the first request may retry.
    let mut process = launch(python, script, Instant::now() + STARTUP_BUDGET,
        || latest.load(Ordering::Acquire) != SHUTDOWN, timings).ok();
    while let Ok(request) = receiver.recv() {
        let current = || latest.load(Ordering::Acquire) == request.id;
        if !current() {
            let _ = request.response.send(Err(error("persistent_cancelled")));
            continue;
        }
        if timings { trace_event("persistent_begin", format_args!("id={} queue_ms={} ready={} reuse={}", request.id, request.started.elapsed().as_millis(), process.is_some(), process.as_ref().is_some_and(|p| p.requests_served > 0))); }
        let result = (|| {
            if process.is_none() {
                let deadline = (Instant::now() + STARTUP_BUDGET).min(request.started + REQUEST_BUDGET);
                process = Some(launch(python, script, deadline, current, timings)?);
            }
            process.as_mut().expect("worker started").request(&request, &latest, timings)
        })();
        let response = match result {
            Ok(reply) => reply,
            Err(code) => {
                // A timed-out/stale/malformed reply cannot contaminate the next request.
                process = None;
                if timings { trace_event("persistent_recovery", format_args!("id={} reason={} elapsed_ms={}", request.id, code, request.started.elapsed().as_millis())); }
                Err(error(code))
            }
        };
        let _ = request.response.send(response);
    }
}
