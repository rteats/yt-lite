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


/// A bounded in-memory libmpv demuxer cache for HTTP streaming. This is not
/// a full-file download: playback begins as soon as enough data is buffered.
pub const NETWORK_STREAM_OPTIONS: &[(&str, &str)] = &[
    ("cache", "yes"),
    ("cache-on-disk", "no"),
    ("cache-secs", "8"),
    ("demuxer-max-bytes", "16MiB"),
    ("demuxer-max-back-bytes", "4MiB"),
    ("network-timeout", "20"),
];

/// Prefer the provider that actually played a YouTube video on Windows.
/// Rust extraction is retained as an optional backup and an explicit choice.
pub fn preferred_playback_provider(
    selected: yt_lite_extraction_proof::Provider,
) -> yt_lite_extraction_proof::Provider {
    use yt_lite_extraction_proof::Provider;
    match selected {
        Provider::Auto => Provider::YtDlp,
        selected => selected,
    }
}

/// Only automatic playback retries through the alternative provider, once.
/// Explicit provider choices must remain strict, including privacy settings.
pub fn fallback_after_playback_error(
    failed: yt_lite_extraction_proof::Provider,
    requested: yt_lite_extraction_proof::Provider,
) -> Option<yt_lite_extraction_proof::Provider> {
    use yt_lite_extraction_proof::Provider;
    match (failed, requested) {
        (Provider::YtDlp, Provider::Auto) => Some(Provider::Rust),
        _ => None,
    }
}

/// Metadata the native watch page can display before the decoder starts.
/// Does not contain signed stream URLs, credentials, or extractor JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchMetadata {
    pub title: String,
    pub subtitle: String,
    pub thumbnail_url: Option<String>,
}

/// Build a predictable public thumbnail URL from a validated YouTube ID.
/// We never fetch an arbitrary URL returned by an extraction provider.
pub fn public_youtube_thumbnail_url(id: &str) -> Option<String> {
    if id.len() != 11 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return None;
    }
    Some(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"))
}

fn format_watch_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{secs:02}")
    } else {
        format!("{mins}:{secs:02}")
    }
}

/// Present the same basic details for full and opt-in lean extraction.
pub fn present_watch_metadata(info: &yt_lite_extraction_proof::ExtractionResult) -> WatchMetadata {
    let duration = info.duration_seconds.map(format_watch_duration)
        .unwrap_or_else(|| "Duration unavailable".into());
    let subtitle = match info.channel.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(channel) => format!("{channel} · {duration}"),
        None => duration,
    };
    WatchMetadata {
        title: info.title.clone(),
        subtitle,
        thumbnail_url: public_youtube_thumbnail_url(&info.video_id),
    }
}
