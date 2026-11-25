//! Controlled, opt-in seek experiment on the actual libmpv playback session.
use crate::mpv::{PlaybackEvent, Player};
use std::time::{Duration, Instant};
use yt_lite_native_player_proof::seek_recovery::{SeekEvidence, SeekLossDetector};
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
    probes: u8,
    loss: SeekLossDetector,
    last_confirmation: Option<Instant>,
    recovery_started: Option<Instant>,
    reload_reply: Option<i32>,
    reseek_dispatched: bool,
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
    probe: bool,
    fault_marker: Option<std::path::PathBuf>,
    recovery: bool,
    recovery_failure: bool,
}

impl SeekBenchmark {
    pub fn new(mode: SeekBenchmarkMode, probe: bool, fault_marker: Option<std::path::PathBuf>, recovery: bool) -> Self {
        eprintln!("[yt-lite] seek-bench begin mode={} samples={} settle_ms=2500 poll_ms=50",
            mode.label(), TARGETS.len());
        Self { mode, next: 0, wrong_landings: 0, duration: None, active: None, ready_since: None,
            phase_started: Instant::now(), done: false, probe, fault_marker, recovery, recovery_failure: false }
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

    pub fn take_recovery_failure(&mut self) -> bool { std::mem::take(&mut self.recovery_failure) }

    fn fail_recovery(&mut self, player: &Player, reason: &'static str) {
        self.recovery_failure = true;
        let _ = player.send_strings(&["stop".into()]);
        eprintln!("[yt-lite] seek-recovery done status=failed reason={reason}");
        self.abort(reason);
    }

    pub fn on_event(&mut self, event: PlaybackEvent) {
        if let Some(sample) = self.active.as_mut() {
            match event {
                PlaybackEvent::SeekAudioReloadReply(code) if sample.recovery_started.is_some() && !sample.reseek_dispatched => {
                    sample.reload_reply = Some(code);
                }
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
            if self.active.as_ref().is_some_and(|s| s.recovery_started.is_some_and(|t| t.elapsed() >= Duration::from_secs(12)) || (s.recovery_started.is_some() && s.started.elapsed() >= SEEK_TIMEOUT)) {
                self.fail_recovery(player, "seek_recovery_timeout");
            } else if self.phase_started.elapsed() >= START_TIMEOUT { self.abort("startup_timeout"); }
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
            if self.recovery {
                if let Some(started) = sample.recovery_started {
                    if started.elapsed() >= Duration::from_secs(12) || sample.started.elapsed() >= SEEK_TIMEOUT {
                        self.fail_recovery(player, "seek_recovery_timeout"); return;
                    }
                    if !sample.reseek_dispatched {
                        match sample.reload_reply {
                            Some(code) if code < 0 => { self.fail_recovery(player, "seek_audio_reload_failed"); return; }
                            Some(_) => {
                                sample.seek_ms = None;
                                sample.restart_ms = None;
                                sample.landing = None;
                                sample.probes = 0;
                                sample.reseek_dispatched = true;
                                eprintln!("[yt-lite] seek-recovery reseek sample={} target_s={:.3} elapsed_ms={}", sample.index, sample.target, sample.started.elapsed().as_millis());
                                if player.send_strings(&self.mode.command(sample.target).expect("valid target")).is_err() {
                                    self.fail_recovery(player, "seek_recovery_dispatch_failed");
                                }
                                return;
                            }
                            None => return,
                        }
                    }
                } else {
                    let qualifies = sample.loss.update(sample.started.elapsed(), SeekEvidence {
                        restarted: sample.restart_ms.is_some(), position, audio, seeking,
                        buffering: player.property_flag(c"paused-for-cache"), paused: player.property_flag(c"pause"),
                        eof: player.property_flag(c"eof-reached"),
                    });
                    let confirmed = player.take_seek_missing_confirmation(sample.index);
                    if qualifies && confirmed {
                        sample.recovery_started = Some(Instant::now());
                        eprintln!("[yt-lite] seek-recovery reload sample={} target_s={:.3} elapsed_ms={}", sample.index, sample.target, sample.started.elapsed().as_millis());
                        if player.reload_seek_audio().is_err() { self.fail_recovery(player, "seek_audio_reload_dispatch_failed"); }
                        return;
                    }
                    if qualifies && sample.last_confirmation.map_or(true, |t| t.elapsed() >= Duration::from_millis(500)) {
                        sample.last_confirmation = Some(Instant::now());
                        player.request_seek_probe(sample.index, "recovery_confirm", sample.target);
                    }
                }
            }
            if self.probe {
                let elapsed = sample.started.elapsed();
                for (bit, due, phase) in [(1, 1, "after_1s"), (2, 5, "after_5s"), (4, 15, "after_15s")] {
                    if elapsed >= Duration::from_secs(due) && sample.probes & bit == 0 {
                        sample.probes |= bit;
                        player.request_seek_probe(sample.index, phase, sample.target);
                    }
                }
            }
            if self.probe && sample.restart_ms.is_some() && !ready && sample.probes & 8 == 0 {
                sample.probes |= 8;
                player.request_seek_probe(sample.index, "restart_unready", sample.target);
            }
            if sample.restart_ms.is_some() && sample.landing.is_none() && seeking == Some(false) {
                sample.landing = position;
            }
            if sample.restart_ms.is_some() && ready {
                if self.probe { player.request_seek_probe(sample.index, "verdict", sample.target); }
                let elapsed = sample.started.elapsed().as_millis();
                let landing_error = sample.landing.map(|p| p - sample.target);
                let status = if self.mode == SeekBenchmarkMode::Exact && !exact_seek_landing_ok(sample.target, sample.landing) {
                    self.wrong_landings += 1;
                    "wrong_landing"
                } else { "ok" };
                eprintln!("[yt-lite] seek-bench result mode={} sample={} status={status} seek_ms={:?} restart_ms={:?} av_ready_ms={} target_s={:.3} landing_s={:?} landing_error_s={:?} video_s={:?} audio_s={:?}",
                    self.mode.label(), sample.index, sample.seek_ms, sample.restart_ms, elapsed,
                    sample.target, sample.landing, landing_error, position, audio);
                if let Some(started) = sample.recovery_started {
                    if status != "ok" { self.fail_recovery(player, "seek_recovery_wrong_landing"); return; }
                    eprintln!("[yt-lite] seek-recovery done status=ready sample={} elapsed_ms={elapsed} recovery_ms={}", sample.index, started.elapsed().as_millis());
                }
                self.active = None;
                self.next += 1;
                self.ready_since = None;
                self.phase_started = Instant::now();
                if self.next == TARGETS.len() {
                    self.abort(if self.wrong_landings == 0 { "complete" } else { "incorrect_landings" });
                }
            } else if sample.started.elapsed() >= SEEK_TIMEOUT {
                if self.probe { player.request_seek_probe(sample.index, "timeout", sample.target); }
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
        if index == 1 {
            if let Some(marker) = self.fault_marker.as_ref() {
                // Explicit local diagnostic only, never used during remote playback.
                if std::fs::OpenOptions::new().write(true).create_new(true).open(marker).is_err() {
                    self.abort("fault_arm_failed"); return;
                }
                eprintln!("[yt-lite] seek-bench fault armed sample=1");
            }
        }
        let command = self.mode.command(target).expect("valid benchmark target");
        let started = Instant::now();
        if player.send_strings(&command).is_err() {
            self.abort("dispatch_failed");
            return;
        }
        self.active = Some(Sample { index, target, started, seek_ms: None, restart_ms: None, landing: None, probes: 0,
            loss: SeekLossDetector::default(), last_confirmation: None, recovery_started: None,
            reload_reply: None, reseek_dispatched: false });
        self.ready_since = None;
    }
}

impl Drop for SeekBenchmark {
    fn drop(&mut self) {
        self.abort("closed_before_completion");
    }
}
