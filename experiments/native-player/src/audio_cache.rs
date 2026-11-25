//! Opt-in, session-scoped small-audio prefetch. Video remains in libmpv.
//! The loopback endpoint serves a complete validated audio object or redirects
//! immediately to the original URL. No general-purpose proxy, disk or cookies.
use reqwest::{header, Client, StatusCode, Url};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, atomic::{AtomicBool, AtomicUsize, Ordering}},
    thread,
    time::{Duration, Instant},
};
use yt_lite_extraction_proof::{trace_event, HttpPlaybackHeaders};

pub const MAX_AUDIO_BYTES: usize = 4 * 1024 * 1024;
const PREFETCH_TIMEOUT: Duration = Duration::from_secs(8);
const IO_POLL: Duration = Duration::from_millis(100);
const MAX_CLIENTS: usize = 4;
// Retiring sessions hold the lease until their last worker releases it.
// Rapid replacement skips this experiment rather than multiplying payload caps.
static ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn eligible_size(raw: &str) -> Result<usize, &'static str> {
    if !raw.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err("invalid_url");
    }
    let url = Url::parse(raw).map_err(|_| "invalid_url")?;
    let host = url.host_str().unwrap_or("");
    if url.scheme() != "https" || url.port_or_known_default() != Some(443)
        || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some()
        || !host.ends_with(".googlevideo.com") || url.path() != "/videoplayback" {
        return Err("unsupported_origin");
    }
    // clen is only a candidate size: the actual response must agree exactly.
    let mut lengths = url.query_pairs().filter(|(k, _)| k == "clen");
    let (_, value) = lengths.next().ok_or("unknown_size")?;
    if lengths.next().is_some() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid_size");
    }
    let size: u64 = value.parse().map_err(|_| "invalid_size")?;
    if size == 0 { return Err("invalid_size"); }
    if size > MAX_AUDIO_BYTES as u64 { return Err("over_budget"); }
    Ok(size as usize)
}

struct Payload {
    bytes: Vec<u8>,
    mime: &'static str,
}

enum State {
    Pending,
    Ready(Arc<Payload>),
    Bypass,
}

struct Session {
    id: u64,
    origin: String,
    path: String,
    size: usize,
    stopped: AtomicBool,
    cancel_fetch: AtomicBool,
    clients: AtomicUsize,
    received_bytes: AtomicUsize,
    state: Mutex<State>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // Drop the stored payload before releasing the process-wide lease.
        if let Ok(state) = self.state.get_mut() { *state = State::Bypass; }
        ACTIVE.store(false, Ordering::Release);
    }
}

pub struct AudioCache {
    url: String,
    session: Arc<Session>,
}

impl AudioCache {
    pub fn start(origin: &str, headers: &HttpPlaybackHeaders, id: u64) -> Option<Self> {
        let size = match eligible_size(origin) {
            Ok(size) => size,
            Err(reason) => {
                trace_event("audio_cache_skip", format_args!("load={id} reason={reason}"));
                return None;
            }
        };
        Self::start_known(origin, size, headers, id)
    }

    fn start_known(origin: &str, size: usize, headers: &HttpPlaybackHeaders, id: u64) -> Option<Self> {
        if size == 0 || size > MAX_AUDIO_BYTES { return None; }
        // Bind before taking the lease; no URL or token is ever logged.
        let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
        listener.set_nonblocking(true).ok()?;
        let port = listener.local_addr().ok()?.port();
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).ok()?;
        let token: String = random.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join("");
        let path = format!("/{token}/audio");
        if ACTIVE.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            trace_event("audio_cache_skip", format_args!("load={id} reason=retiring_session"));
            return None;
        }
        let session = Arc::new(Session {
            id, origin: origin.into(), path: path.clone(), size,
            stopped: AtomicBool::new(false), cancel_fetch: AtomicBool::new(false),
            clients: AtomicUsize::new(0), received_bytes: AtomicUsize::new(0), state: Mutex::new(State::Pending),
        });
        let cache = Self { url: format!("http://127.0.0.1:{port}{path}"), session: session.clone() };
        let serving = session.clone();
        if thread::Builder::new().name("audio-cache-http".into()).spawn(move || serve(listener, serving)).is_err() {
            return None;
        }
        let fetching = session.clone();
        let headers = headers.clone();
        if thread::Builder::new().name("audio-cache-fetch".into()).spawn(move || prefetch(fetching, headers)).is_err() {
            return None;
        }
        Some(cache)
    }

    pub fn url(&self) -> &str { &self.url }
}

impl Drop for AudioCache {
    fn drop(&mut self) {
        // Nonblocking on UI thread. Workers release the lease after cancellation.
        self.session.stopped.store(true, Ordering::Release);
        self.session.cancel_fetch.store(true, Ordering::Release);
        trace_event("audio_cache_retire", format_args!("load={}", self.session.id));
    }
}

async fn fetch(session: &Session, headers: HttpPlaybackHeaders) -> Result<Payload, &'static str> {
    let builder = Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(PREFETCH_TIMEOUT);
    #[cfg(test)]
    let builder = builder.no_proxy();
    let client = builder.build().map_err(|_| "client")?;
    let mut request = client.get(session.origin.as_str())
        .header(header::RANGE, format!("bytes=0-{}", session.size - 1))
        .header(header::ACCEPT_ENCODING, "identity");
    if let Some(agent) = headers.user_agent { request = request.header(header::USER_AGENT, agent); }
    if let Some(referer) = headers.referer { request = request.header(header::REFERER, referer); }
    let request_started = Instant::now();
    let mut response = request.send().await.map_err(|_| "request")?;
    trace_event("audio_cache_headers", format_args!("load={} status={} elapsed_ms={}", session.id, response.status().as_u16(), request_started.elapsed().as_millis()));
    let h = response.headers();
    let expected_range = format!("bytes 0-{}/{}", session.size - 1, session.size);
    let range = h.get(header::CONTENT_RANGE).and_then(|v| v.to_str().ok());
    let valid_status = match response.status() {
        StatusCode::PARTIAL_CONTENT => range == Some(expected_range.as_str()),
        StatusCode::OK => !h.contains_key(header::CONTENT_RANGE),
        _ => false,
    };
    if !valid_status || h.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok()) != Some(session.size) {
        return Err("response_size_or_status");
    }
    if h.contains_key(header::CONTENT_ENCODING)
        && h.get(header::CONTENT_ENCODING).and_then(|v| v.to_str().ok()) != Some("identity") {
        return Err("encoding");
    }
    let mime = match h.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next()).map(str::trim) {
        Some("audio/webm") => "audio/webm",
        Some("audio/mp4") => "audio/mp4",
        Some("audio/mpeg") => "audio/mpeg",
        Some("audio/ogg") => "audio/ogg",
        Some("application/octet-stream") => "application/octet-stream",
        _ => return Err("content_type"),
    };
    let mut bytes = Vec::with_capacity(session.size);
    while let Some(chunk) = response.chunk().await.map_err(|_| "body")? {
        if chunk.len() > session.size - bytes.len() { return Err("oversized_body"); }
        let first = bytes.is_empty() && !chunk.is_empty();
        bytes.extend_from_slice(&chunk);
        session.received_bytes.store(bytes.len(), Ordering::Release);
        if first { trace_event("audio_cache_first_body", format_args!("load={} bytes={} elapsed_ms={}", session.id, bytes.len(), request_started.elapsed().as_millis())); }
    }
    if bytes.len() != session.size { return Err("short_body"); }
    Ok(Payload { bytes, mime })
}

fn prefetch(session: Arc<Session>, headers: HttpPlaybackHeaders) {
    let began = Instant::now();
    trace_event("audio_cache_begin", format_args!("load={} bytes={} cap={MAX_AUDIO_BYTES}", session.id, session.size));
    let result = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => {
            let result = runtime.block_on(async {
            let future = fetch(&session, headers);
            tokio::pin!(future);
            loop {
                if session.cancel_fetch.load(Ordering::Acquire) { break Err("cancelled"); }
                if began.elapsed() >= PREFETCH_TIMEOUT { break Err("timeout"); }
                if let Ok(result) = tokio::time::timeout(IO_POLL, future.as_mut()).await { break result; }
            }
            });
            // A platform DNS lookup may outlive the async request. Do not hold
            // the cache lease waiting indefinitely for blocking runtime work.
            runtime.shutdown_timeout(Duration::from_millis(200));
            result
        }
        Err(_) => Err("runtime"),
    };
    let mut state = session.state.lock().unwrap();
    if !matches!(*state, State::Pending) || session.cancel_fetch.load(Ordering::Acquire) {
        trace_event("audio_cache_end", format_args!("load={} status=cancelled received_bytes={} elapsed_ms={}", session.id, session.received_bytes.load(Ordering::Acquire), began.elapsed().as_millis()));
        return;
    }
    match result {
        Ok(payload) => {
            trace_event("audio_cache_end", format_args!("load={} status=ready bytes={} elapsed_ms={}", session.id, payload.bytes.len(), began.elapsed().as_millis()));
            *state = State::Ready(Arc::new(payload));
        }
        Err(reason) => {
            trace_event("audio_cache_end", format_args!("load={} status=bypass reason={reason} received_bytes={} elapsed_ms={}", session.id, session.received_bytes.load(Ordering::Acquire), began.elapsed().as_millis()));
            *state = State::Bypass;
        }
    }
}

fn serve(listener: TcpListener, session: Arc<Session>) {
    while !session.stopped.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                if session.clients.fetch_add(1, Ordering::AcqRel) >= MAX_CLIENTS {
                    session.clients.fetch_sub(1, Ordering::AcqRel);
                    continue;
                }
                let client = session.clone();
                if thread::Builder::new().name("audio-cache-range".into()).spawn(move || {
                    handle(stream, &client);
                    client.clients.fetch_sub(1, Ordering::AcqRel);
                }).is_err() {
                    session.clients.fetch_sub(1, Ordering::AcqRel);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(10)),
            Err(_) => break,
        }
    }
}

fn read_request(stream: &mut TcpStream, session: &Session) -> Option<String> {
    stream.set_read_timeout(Some(IO_POLL)).ok()?;
    let start = Instant::now();
    let mut bytes = Vec::with_capacity(1024);
    let mut buf = [0u8; 1024];
    while start.elapsed() < Duration::from_secs(2) && !session.stopped.load(Ordering::Acquire) {
        match stream.read(&mut buf) {
            Ok(0) => return None,
            Ok(n) => {
                if bytes.len() + n > 8192 { return None; }
                bytes.extend_from_slice(&buf[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    return String::from_utf8(bytes[..end].to_vec()).ok();
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted) => {}
            Err(_) => return None,
        }
    }
    None
}

// Single range only; inclusive end. Invalid or unsatisfiable -> 416.
pub fn byte_range(range: Option<&str>, size: usize) -> Option<(usize, usize, bool)> {
    if size == 0 { return None; }
    let Some(range) = range else { return Some((0, size-1, false)); };
    let (a,b) = range.strip_prefix("bytes=")?.split_once('-')?;
    fn num(s: &str) -> Option<usize> {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) { return None; }
        s.parse().ok()
    }
    if a.is_empty() {
        let n = num(b)?;
        return (n > 0).then(|| (size.saturating_sub(n), size-1, true));
    }
    let lo = num(a)?;
    let hi = if b.is_empty() { size-1 } else { num(b)?.min(size-1) };
    (lo <= hi && lo < size).then_some((lo,hi,true))
}

fn send(stream: &mut TcpStream, session: &Session, mut bytes: &[u8]) -> bool {
    let mut progress = Instant::now();
    while !bytes.is_empty() && !session.stopped.load(Ordering::Acquire)
        && progress.elapsed() < Duration::from_secs(10) {
        match stream.write(&bytes[..bytes.len().min(65536)]) {
            Ok(0) => return false,
            Ok(n) => { bytes = &bytes[n..]; progress = Instant::now(); }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted) => {}
            Err(_) => return false,
        }
    }
    bytes.is_empty()
}

fn handle(mut stream: TcpStream, session: &Session) {
    let Some(request) = read_request(&mut stream, session) else { return; };
    if stream.set_write_timeout(Some(IO_POLL)).is_err() { return; }
    let mut lines = request.split("\r\n");
    let mut first = lines.next().unwrap_or("").split_ascii_whitespace();
    let method = first.next().unwrap_or("");
    let path = first.next().unwrap_or("");
    let version = first.next().unwrap_or("");
    if path != session.path || !matches!(method, "GET" | "HEAD")
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1") || first.next().is_some() {
        send(&mut stream, session, b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    }
    let mut range = None;
    for line in lines {
        if let Some((key,value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("range") {
                if range.is_some() { return; }
                range = Some(value.trim());
            }
        }
    }
    // First demand wins. Never hold playback waiting for the speculative GET.
    // Once bypassed, all later requests redirect and the fetch is cancelled.
    let payload = {
        let mut state = session.state.lock().unwrap();
        match &*state {
            State::Ready(payload) => Some(payload.clone()),
            _ => {
                *state = State::Bypass;
                session.cancel_fetch.store(true, Ordering::Release);
                None
            }
        }
    };
    let Some(payload) = payload else {
        trace_event("audio_cache_request", format_args!("load={} result=redirect method={method}", session.id));
        let reply = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {}\r\nCache-Control: no-store\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", session.origin);
        send(&mut stream, session, reply.as_bytes());
        return;
    };
    let Some((lo,hi,partial)) = byte_range(range, payload.bytes.len()) else {
        let reply = format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", payload.bytes.len());
        send(&mut stream, session, reply.as_bytes());
        return;
    };
    let size = hi-lo+1;
    let status = if partial { "206 Partial Content" } else { "200 OK" };
    let content_range = if partial { format!("Content-Range: bytes {lo}-{hi}/{}\r\n", payload.bytes.len()) } else { String::new() };
    let reply = format!("HTTP/1.1 {status}\r\nContent-Type: {}\r\nAccept-Ranges: bytes\r\nContent-Length: {size}\r\n{content_range}Cache-Control: no-store\r\nConnection: close\r\n\r\n", payload.mime);
    let ok = send(&mut stream, session, reply.as_bytes())
        && (method == "HEAD" || send(&mut stream, session, &payload.bytes[lo..=hi]));
    trace_event("audio_cache_request", format_args!("load={} result=hit method={method} requested_bytes={size} complete={ok}", session.id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligibility_requires_exact_small_size_and_googlevideo_https() {
        assert_eq!(eligible_size("https://r1.googlevideo.com/videoplayback?clen=4194304"), Ok(MAX_AUDIO_BYTES));
        assert_eq!(eligible_size("https://r1.googlevideo.com/videoplayback?clen=4194305"), Err("over_budget"));
        assert_eq!(eligible_size("https://r1.googlevideo.com/videoplayback"), Err("unknown_size"));
        assert_eq!(eligible_size("https://r1.googlevideo.com/videoplayback?clen=1&clen=2"), Err("invalid_size"));
        assert_eq!(eligible_size("https://googlevideo.com.evil.test/videoplayback?clen=1"), Err("unsupported_origin"));
        assert_eq!(eligible_size("https://user@r1.googlevideo.com/videoplayback?clen=1"), Err("unsupported_origin"));
        assert_eq!(eligible_size("http://r1.googlevideo.com/videoplayback?clen=1"), Err("unsupported_origin"));
    }

    #[test]
    fn ranges_preserve_exact_inclusive_boundaries() {
        assert_eq!(byte_range(None, 10), Some((0,9,false)));
        assert_eq!(byte_range(Some("bytes=2-4"), 10), Some((2,4,true)));
        assert_eq!(byte_range(Some("bytes=-3"), 10), Some((7,9,true)));
        assert_eq!(byte_range(Some("bytes=5-"), 10), Some((5,9,true)));
        assert_eq!(byte_range(Some("bytes=10-"), 10), None);
        assert_eq!(byte_range(Some("bytes=0-1,4-5"), 10), None);
    }

    // Actual HTTP seam: private constructor bypasses the CDN allowlist only
    // for a fixed local test origin. No live network or libmpv required.
    #[test]
    fn cached_ranges_and_immediate_miss_redirect_are_session_scoped() {
        use std::sync::mpsc;
        let origin = TcpListener::bind(("127.0.0.1",0)).unwrap();
        let upstream = format!("http://{}/audio", origin.local_addr().unwrap());
        let (release_tx, release_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut conn, _) = origin.accept().unwrap();
            conn.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut request = Vec::new();
            let mut b = [0; 512];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = conn.read(&mut b).unwrap();
                if n == 0 { return; }
                request.extend_from_slice(&b[..n]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.contains("range: bytes=0-9\r\n"));
            assert!(request.contains("user-agent: test-audio-cache\r\n"));
            assert!(request.contains("referer: https://www.youtube.com/\r\n"));
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            let _ = conn.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-9/10\r\nContent-Length: 10\r\nContent-Type: audio/mp4\r\nConnection: close\r\n\r\n0123456789");
        });
        let headers = HttpPlaybackHeaders { user_agent: Some("test-audio-cache".into()), referer: Some("https://www.youtube.com/".into()) };
        let cache = AudioCache::start_known(&upstream,10,&headers,1).unwrap();
        release_tx.send(()).unwrap();
        let deadline = Instant::now()+Duration::from_secs(4);
        while !matches!(*cache.session.state.lock().unwrap(), State::Ready(_)) {
            assert!(Instant::now()<deadline, "prefetch did not become ready");
            thread::sleep(Duration::from_millis(10));
        }
        fn request(url: &str, method: &str, range: &str) -> String {
            let url=Url::parse(url).unwrap();
            let mut conn=TcpStream::connect(("127.0.0.1",url.port().unwrap())).unwrap();
            conn.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            write!(conn,"{method} {} HTTP/1.1\r\nHost: localhost\r\n{range}Connection: close\r\n\r\n",url.path()).unwrap();
            let mut reply=String::new();
            conn.read_to_string(&mut reply).unwrap();
            reply
        }
        let reply=request(cache.url(),"GET","Range: bytes=2-4\r\n");
        assert!(reply.starts_with("HTTP/1.1 206"));
        assert!(reply.contains("Content-Range: bytes 2-4/10"));
        assert!(reply.ends_with("\r\n\r\n234"));
        let head=request(cache.url(),"HEAD","Range: bytes=-3\r\n");
        assert!(head.starts_with("HTTP/1.1 206") && head.ends_with("\r\n\r\n"));
        assert!(request(cache.url(),"GET","Range: bytes=99-\r\n").starts_with("HTTP/1.1 416"));
        assert!(request(&format!("{}/wrong",cache.url()),"GET","").starts_with("HTTP/1.1 404"));
        assert!(AudioCache::start_known(&upstream,10,&HttpPlaybackHeaders::default(),2).is_none());
        drop(cache);
        worker.join().unwrap();
        let deadline=Instant::now()+Duration::from_secs(3);
        while ACTIVE.load(Ordering::Acquire) { assert!(Instant::now()<deadline); thread::sleep(Duration::from_millis(10)); }

        // Origin deliberately never replies before first demand; redirect must
        // not wait for the upstream prefetch timeout.
        let origin=TcpListener::bind(("127.0.0.1",0)).unwrap();
        let upstream=format!("http://{}/audio",origin.local_addr().unwrap());
        let cache=AudioCache::start_known(&upstream,10,&HttpPlaybackHeaders::default(),3).unwrap();
        let began=Instant::now();
        let response=request(cache.url(),"GET","Range: bytes=0-\r\n");
        assert!(response.starts_with("HTTP/1.1 307"));
        assert!(response.contains(&format!("Location: {upstream}\r\n")));
        assert!(began.elapsed()<Duration::from_secs(2));
        assert!(request(cache.url(),"GET","").starts_with("HTTP/1.1 307"));
        drop(cache);
        drop(origin);
    }
}
