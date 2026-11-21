//! Minimal playback action seam for the Windows Slint/libmpv feasibility prototype.
//! This module contains only commands that affect the existing playback session.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackAction {
    TogglePause,
    SeekRelative(i64),
    SeekFraction { fraction: f64, duration_seconds: f64 },
}

/// Convert a user action into mpv's command arguments. Invalid seek positions are rejected.
pub fn command_for(action: PlaybackAction) -> Option<Vec<String>> {
    match action {
        PlaybackAction::TogglePause => Some(vec!["cycle".into(), "pause".into()]),
        PlaybackAction::SeekRelative(seconds) if seconds.abs() <= 3600 => {
            Some(vec!["seek".into(), seconds.to_string(), "relative".into()])
        }
        PlaybackAction::SeekFraction { fraction, duration_seconds }
            if fraction.is_finite() && (0.0..=1.0).contains(&fraction)
                && duration_seconds.is_finite() && duration_seconds > 0.0 =>
        {
            Some(vec!["seek".into(), format!("{:.3}", fraction * duration_seconds), "absolute".into()])
        }
        _ => None,
    }
}
