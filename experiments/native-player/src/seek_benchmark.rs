//! Controlled, opt-in seek experiment on the actual libmpv playback session.
use crate::mpv::{PlaybackEvent, Player};
use std::time::{Duration, Instant};
use yt_lite_native_player_proof::{exact_seek_landing_ok, main_cache_membership, SeekBenchmarkMode};

const TARGETS: [f64; 6] = [0.55, 0.55, 0.25, 0.25, 0.75, 0.75];
const SETTLE: Duration = Duration::from_millis(2500);
const SEEK_TIMEOUT: Duration = Duration::from_secs(25);
const START_TIMEOUT: Duration = Duration::from_secs(90);

struct Sample {
    index: usize,
    target: f64,
    started: Instant,
    seek_ms: Option<u128>,
    restart_ms: Option<u128>,
    landing: Option<f64>,
}

pub struct SeekBenchmark {
    mode: SeekBenchmarkMode,
    next: usize,
    wrong_landings: usize,
    duration: Option<f64>,
    active: Option<Sample>,
    ready_since: Option<Instant>,
    phase_started: Instant,
    done: bool,
}

impl SeekBenchmark {
    pub fn new(mode: SeekBenchmarkMode) -> Self {
        eprintln!("[yt-lite] seek-bench begin mode={} samples={} settle_ms=2500 poll_ms=50",
            mode.label(), TARGETS.len());
        Self { mode, next: 0, wrong_landings: 0, duration: None, active: None, ready_since: None,
            phase_started: Instant::now(), done: false }
    }

    pub fn abort(&mut self, reason: &'static str) {
        if self.done { return; }
        if let Some(sample) = self.active.take() {
            eprintln!("[yt-lite] seek-bench result mode={} sample={} status={} elapsed_ms={} seek_ms={:?} restart_ms={:?}",
                self.mode.label(), sample.index, reason, sample.started.elapsed().as_millis(),
                sample.seek_ms, sample.restart_ms);
        }
        self.done = true;
        eprintln!("[yt-lite] seek-bench done mode={} completed={} wrong_landings={} status={reason}", self.mode.label(), self.next, self.wrong_landings);
    }

    pub fn on_event(&mut self, event: PlaybackEvent) {
        if let Some(sample) = self.active.as_mut() {
            match event {
                PlaybackEvent::Seek if sample.seek_ms.is_none() => {
                    sample.seek_ms = Some(sample.started.elapsed().as_millis());
                }
                PlaybackEvent::PlaybackRestart if sample.seek_ms.is_some() && sample.restart_ms.is_none() => {
                    sample.restart_ms = Some(sample.started.elapsed().as_millis());
                }
                _ => {}
            }
        }
    }

    pub fn tick(&mut self, player: &Player, loaded: bool) {
        if self.done { return; }
        if !loaded {
            if self.phase_started.elapsed() >= START_TIMEOUT { self.abort("startup_timeout"); }
            return;
        }
        let position = player.property_double(c"time-pos");
        let audio = player.property_double(c"audio-pts");
        let seeking = player.property_flag(c"seeking");
        let ready = seeking == Some(false)
            && player.property_flag(c"pause") == Some(false)
            && player.property_flag(c"paused-for-cache") == Some(false)
            && position.zip(audio).is_some_and(|(p, a)| (p - a).abs() <= 0.5);
        if let Some(sample) = self.active.as_mut() {
            if sample.restart_ms.is_some() && sample.landing.is_none() && seeking == Some(false) {
                sample.landing = position;
            }
            if sample.restart_ms.is_some() && ready {
                let elapsed = sample.started.elapsed().as_millis();
                let landing_error = sample.landing.map(|p| p - sample.target);
                let status = if self.mode == SeekBenchmarkMode::Exact && !exact_seek_landing_ok(sample.target, sample.landing) {
                    self.wrong_landings += 1;
                    "wrong_landing"
                } else { "ok" };
                eprintln!("[yt-lite] seek-bench result mode={} sample={} status={status} seek_ms={:?} restart_ms={:?} av_ready_ms={} target_s={:.3} landing_s={:?} landing_error_s={:?} video_s={:?} audio_s={:?}",
                    self.mode.label(), sample.index, sample.seek_ms, sample.restart_ms, elapsed,
                    sample.target, sample.landing, landing_error, position, audio);
                self.active = None;
                self.next += 1;
                self.ready_since = None;
                self.phase_started = Instant::now();
                if self.next == TARGETS.len() {
                    self.abort(if self.wrong_landings == 0 { "complete" } else { "incorrect_landings" });
                }
            } else if sample.started.elapsed() >= SEEK_TIMEOUT {
                self.abort("seek_timeout");
            }
            return;
        }
        if self.phase_started.elapsed() >= START_TIMEOUT {
            self.abort(if self.next == 0 && audio.is_none() { "startup_audio_unavailable" } else { "settle_timeout" });
            return;
        }
        if !ready { self.ready_since = None; return; }
        let ready_since = *self.ready_since.get_or_insert_with(Instant::now);
        if ready_since.elapsed() < SETTLE { return; }
        let duration = match self.duration.or_else(|| player.property_double(c"duration")) {
            Some(d) if d >= 60.0 => d,
            Some(_) => { self.abort("requires_video_at_least_60_seconds"); return; }
            None => return, // An observed duration may not have arrived yet.
        };
        self.duration = Some(duration);
        let target = (duration * TARGETS[self.next] * 1000.0).round() / 1000.0;
        let ranges = player.main_cache_ranges();
        let membership = main_cache_membership(ranges.as_deref(), target);
        let cache_s = player.property_double(c"demuxer-cache-duration");
        let index = self.next + 1;
        eprintln!("[yt-lite] seek-bench request mode={} sample={} target_s={target:.3} from_s={position:?} main_cache={membership} ranges={ranges:?} cache_s={cache_s:?}",
            self.mode.label(), index);
        let command = self.mode.command(target).expect("valid benchmark target");
        let started = Instant::now();
        if player.send_strings(&command).is_err() {
            self.abort("dispatch_failed");
            return;
        }
        self.active = Some(Sample { index, target, started, seek_ms: None, restart_ms: None, landing: None });
        self.ready_since = None;
    }
}

impl Drop for SeekBenchmark {
    fn drop(&mut self) {
        self.abort("closed_before_completion");
    }
}
