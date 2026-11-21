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
        PlaybackAction::SeekRelative(seconds) if seconds.checked_abs().is_some_and(|magnitude| magnitude <= 3600) => {
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


/// Pick the highest-resolution HTTP(S) stream that already contains both
/// video and audio. Never select video-only tracks without a matching audio
/// input: feeding only that URL to mpv would produce silent playback.
pub fn select_muxed_stream(
    info: &yt_lite_extraction_proof::ExtractionResult,
) -> Option<&yt_lite_extraction_proof::Stream> {
    match &info.streams {
        yt_lite_extraction_proof::Capability::Available(streams) => streams
            .iter()
            .filter(|stream| {
                stream.has_video && stream.has_audio
                    && (stream.url.starts_with("https://") || stream.url.starts_with("http://"))
            })
            .max_by_key(|stream| (stream.height.unwrap_or(0), stream.width.unwrap_or(0))),
        _ => None,
    }
}
