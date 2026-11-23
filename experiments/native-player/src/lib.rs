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


/// The muxed stream is preferred because it has a single network input.
/// Videos with only separate streams can still play without downloading or
/// merging files, by adding the audio track to the same libmpv session.
pub enum PlaybackStreamSelection<'a> {
    Muxed(&'a yt_lite_extraction_proof::Stream),
    Separate {
        video: &'a yt_lite_extraction_proof::Stream,
        audio: &'a yt_lite_extraction_proof::Stream,
    },
}

/// Automatic playback targets at most 1080p when that resolution is
/// available. The decoder can still use a larger track if it is the only
/// separate-video candidate; manual quality controls belong to issue #10.
pub const AUTO_PREFERRED_MAX_HEIGHT: u64 = 1080;

pub fn select_playback_streams(
    info: &yt_lite_extraction_proof::ExtractionResult,
) -> Option<PlaybackStreamSelection<'_>> {
    let muxed = select_muxed_stream(info);
    let yt_lite_extraction_proof::Capability::Available(streams) = &info.streams else {
        return muxed.map(PlaybackStreamSelection::Muxed);
    };
    let http = |s: &&yt_lite_extraction_proof::Stream| {
        s.url.starts_with("https://") || s.url.starts_with("http://")
    };
    // Find a complete compatible pair first; an attractive video-only
    // format without matching audio must not suppress valid lower formats.
    let split = streams.iter()
        .filter(|s| s.has_video && !s.has_audio)
        .filter(http)
        .filter_map(|video| {
            let audio = streams.iter()
                .filter(|s| s.has_audio && !s.has_video)
                .filter(http)
                .find(|audio| audio.http_headers == video.http_headers)?;
            Some((video, audio))
        })
        .max_by_key(|(video, _)| {
            let height = video.height.unwrap_or(0);
            (height <= AUTO_PREFERRED_MAX_HEIGHT, height, video.width.unwrap_or(0))
        });
    match (muxed, split) {
        (Some(muxed), Some((video, audio)))
            if video.height.unwrap_or(0) > muxed.height.unwrap_or(0) =>
        {
            Some(PlaybackStreamSelection::Separate { video, audio })
        }
        (Some(muxed), _) => Some(PlaybackStreamSelection::Muxed(muxed)),
        (None, Some((video, audio))) => Some(PlaybackStreamSelection::Separate { video, audio }),
        (None, None) => None,
    }
}

/// Return mpv's video margins as fractions of the physical window height.
/// The Slint UI is defined in logical px; this conversion preserves the
/// visible video area under Windows display scaling, resize and fullscreen.
/// If the window is extremely short, keep a small nonzero video viewport.
pub fn video_margin_ratios(height_px: u32, scale_factor: f32, has_metadata: bool) -> (f64, f64) {
    let height = f64::from(height_px.max(1));
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        f64::from(scale_factor)
    } else { 1.0 };
    let top = (64.0 + if has_metadata { 54.0 } else { 0.0 }) * scale / height;
    let bottom = 116.0 * scale / height;
    let sum = top + bottom;
    if sum > 0.9 {
        (top / sum * 0.9, bottom / sum * 0.9)
    } else {
        (top, bottom)
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

/// Search results occupy the right 350 logical px plus a 10px gap.
/// Constrain the viewport margin at small window widths.
pub fn search_margin_ratio(width_px: u32, scale_factor: f32, show_search: bool) -> f64 {
    if !show_search || width_px == 0 { return 0.0; }
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        f64::from(scale_factor)
    } else { 1.0 };
    (370.0 * scale / f64::from(width_px)).min(0.65)
}
