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

// Prefer a direct HTTP demuxer input over manifest-backed streams when
// choosing otherwise comparable tracks. Unknown protocol is not rejected.
fn directness(stream: &yt_lite_extraction_proof::Stream) -> u8 {
    match stream.protocol.as_deref() {
        Some("http" | "https") => 2,
        None => 1,
        Some(_) => 0,
    }
}

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
                .filter(|audio| audio.http_headers == video.http_headers)
                // yt-dlp orders formats worst to best; max_by_key retains
                // the last candidate on ties, preserving provider ranking.
                .max_by_key(|audio| directness(audio))?;
            Some((video, audio))
        })
        .max_by_key(|(video, _)| {
            let height = video.height.unwrap_or(0);
            (height > 0 && height <= AUTO_PREFERRED_MAX_HEIGHT, height, directness(video), video.width.unwrap_or(0))
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

/// Clock-based diagnostic only. Detect sustained video progress without
/// corresponding audio-clock progress, after audio has actually initialized.
/// Never infer silence from a muted/paused/seeking/buffering player or EOF.
#[derive(Default)]
pub struct AudioContinuity {
    previous: Option<(f64, Option<f64>)>,
    seen_audio: bool,
    stagnant: f64,
    warning: bool,
}

impl AudioContinuity {
    pub fn sample(&mut self, video: Option<f64>, audio: Option<f64>, active: bool) -> bool {
        if !active || video.is_none() {
            self.previous = None;
            self.stagnant = 0.0;
            return self.warning;
        }
        let video = video.unwrap();
        let current = (video, audio);
        if let Some((last_video, last_audio)) = self.previous.replace(current) {
            let advance = video - last_video;
            let audio_advanced = matches!((last_audio, audio),
                (Some(a), Some(b)) if b - a > 0.05);
            if audio_advanced { self.seen_audio = true; self.warning = false; }
            // Large jumps and backwards movement are seeks or discontinuities.
            if !(0.05..=2.0).contains(&advance) || audio_advanced {
                self.stagnant = 0.0;
            } else if self.seen_audio {
                self.stagnant += advance;
            }
        }
        if self.stagnant >= 3.0 { self.warning = true; }
        self.warning
    }
}


/// mpv >= 0.38 loadfile per-file options. The byte-counted quote keeps
/// commas, percent escapes and other URL bytes out of the option grammar.
/// The append action consumes exactly one path rather than a path list.
pub fn preattached_audio_command(video: &str, audio: &str) -> Option<Vec<String>> {
    let valid = |url: &str| (url.starts_with("https://") || url.starts_with("http://"))
        && !url.contains('\0');
    if !valid(video) || !valid(audio) { return None; }
    Some(vec!["loadfile".into(), video.into(), "replace".into(), "-1".into(),
        format!("audio-files-append=%{}%{}", audio.len(), audio)])
}


/// Classify the requested time against the actual seekable ranges reported by
/// mpv's main demuxer. External audio is not covered by this classification.
pub fn main_cache_membership(ranges: Option<&[(f64, f64)]>, target: f64) -> &'static str {
    match ranges {
        Some(ranges) if target.is_finite() => {
            if ranges.iter().any(|&(start, end)| start <= target && target <= end) {
                "inside"
            } else {
                "outside"
            }
        }
        _ => "unknown",
    }
}

/// Explicit seek precision for the diagnostic A/B only. Normal UI commands
/// retain the current default precision and behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekBenchmarkMode {
    Exact,
    Keyframes,
}

impl SeekBenchmarkMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "exact" => Some(Self::Exact),
            "keyframes" => Some(Self::Keyframes),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Keyframes => "keyframes",
        }
    }

    pub fn command(self, target: f64) -> Option<Vec<String>> {
        if !target.is_finite() || target < 0.0 { return None; }
        Some(vec!["seek".into(), format!("{target:.3}"), format!("absolute+{}", self.label())])
    }
}


/// Diagnostic tolerance, deliberately larger than normal frame/50-ms polling
/// error. This is a benchmark verdict, not a production seek policy.
pub fn exact_seek_landing_ok(target: f64, landing: Option<f64>) -> bool {
    target.is_finite() && landing.is_some_and(|p| p.is_finite() && (p - target).abs() <= 0.35)
}

/// Only fixed classifications leave the mpv bridge. An explicit MIME hint is
/// not proof that every request by this demuxer belongs to the same stream.
pub fn diagnostic_stream_hint(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    let audio = lower.contains("mime=audio") || lower.contains("mime%3daudio");
    let video = lower.contains("mime=video") || lower.contains("mime%3dvideo");
    match (audio, video) {
        (true, false) => "audio",
        (false, true) => "video",
        _ => "unknown",
    }
}

pub mod observed_state;

pub mod transport_recovery;


/// EDL experiment uses fixed loopback fixture paths only.
pub fn loopback_edl_source(port: u16) -> Option<String> {
    if port == 0 { return None; }
    let video = format!("http://127.0.0.1:{port}/video.mp4");
    let audio = format!("http://127.0.0.1:{port}/audio.m4a");
    Some(format!("edl://!new_stream;!no_clip;%{}%{};!new_stream;!no_clip;%{}%{}",
        video.len(), video, audio.len(), audio))
}
