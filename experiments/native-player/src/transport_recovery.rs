//! Startup-only recovery policy. No URLs or provider messages are retained here.
use std::time::Duration;

pub const RECOVERY_BUDGET: Duration = Duration::from_secs(30);
pub const ATTEMPT_BUDGET: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction { Refresh, Muxed, Stop }

#[derive(Default)]
pub struct StartupRecovery {
    refreshed: bool,
    muxed: bool,
}

impl StartupRecovery {
    pub fn next(&mut self, elapsed: Duration, split: bool, muxed_available: bool) -> RecoveryAction {
        if elapsed >= RECOVERY_BUDGET { return RecoveryAction::Stop; }
        if !self.refreshed && !self.muxed && split {
            self.refreshed = true;
            return RecoveryAction::Refresh;
        }
        if !self.muxed && muxed_available {
            self.muxed = true;
            return RecoveryAction::Muxed;
        }
        RecoveryAction::Stop
    }
}

/// Classify explicit error phrases, not arbitrary status numbers in URLs.
/// No raw input is returned. The classifications are advisory evidence only.
pub fn transport_diagnostic(text: &str) -> Option<&'static str> {
    let s = text.to_ascii_lowercase();
    for (code, label) in [("401", "http_401"), ("403", "http_403"), ("404", "http_404"),
        ("410", "http_410"), ("416", "http_416"), ("429", "http_429"),
        ("500", "http_500"), ("502", "http_502"), ("503", "http_503"), ("504", "http_504")] {
        if s.contains(&format!("http error {code}")) || s.contains(&format!("http response {code}"))
            || s.contains(&format!("http/1.1 {code} ")) || s.contains(&format!("server returned {code}")) {
            return Some(label);
        }
    }
    if s.contains("connection reset") || s.contains("wsaeconnreset") || s.contains("forcibly closed") {
        Some("connection_reset")
    } else if s.contains("connection refused") {
        Some("connection_refused")
    } else if s.contains("timed out") || s.contains("connection timeout") || s.contains("read timeout") {
        Some("transport_timeout")
    } else if s.contains("unexpected eof") || s.contains("premature end") || s.contains("stream ends prematurely") {
        Some("premature_eof")
    } else if s.contains("tls handshake") || s.contains("ssl error") || s.contains("certificate verify failed") {
        Some("tls_error")
    } else if s.contains("invalid data found when processing input") || s.contains("error opening demuxer") {
        Some("demux_error")
    } else { None }
}


/// FILE_LOADED alone cannot establish A/V readiness. Missing flags or audio
/// output configuration are not success.
pub fn startup_av_ready(video: Option<f64>, audio: Option<f64>, seeking: Option<bool>,
    buffering: Option<bool>, paused: Option<bool>, output_hz: Option<f64>) -> bool {
    seeking == Some(false) && buffering == Some(false) && paused == Some(false)
        && output_hz.is_some_and(|hz| hz.is_finite() && hz > 0.0)
        && video.zip(audio).is_some_and(|(v,a)| v.is_finite() && a.is_finite() && (v-a).abs() <= 0.5)
}


/// Recognize mpv's terminal external-file open error only for the exact
/// current requested audio URL. Builds that redact the URL use the watchdog.
pub fn terminal_audio_open_failure(message: &str, expected_audio: &str) -> bool {
    !expected_audio.is_empty()
        && message.trim_end_matches(&['\r', '\n'][..]).strip_prefix("Can not open external file ")
            .and_then(|s| s.strip_suffix('.')) == Some(expected_audio)
}

pub fn missing_required_audio_after_open_failure(explicit_failure: bool, startup_pending: bool,
    file_loaded: bool, audio_pts: Option<f64>, output_hz: Option<f64>) -> bool {
    explicit_failure && startup_pending && file_loaded && audio_pts.is_none() && output_hz.is_none()
}
