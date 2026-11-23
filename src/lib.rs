//! Feasibility harness for the yt-lite extraction adapter seam (issue #3).
//! No stream URLs, cookies or provider stderr are included in error messages.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, time::Duration};
use tokio::{process::Command, time::timeout};
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Rust,
    YtDlp,
    Auto,
}

impl Provider {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "rust" => Some(Self::Rust),
            "yt-dlp" => Some(Self::YtDlp),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "items", rename_all = "snake_case")]
pub enum Capability<T> {
    Available(Vec<T>),
    Unavailable,
    Unknown,
}

/// Only forwarding non-secret HTTP compatibility fields. Never store or
/// relay cookies or Authorization headers from extractor results.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HttpPlaybackHeaders {
    pub user_agent: Option<String>,
    pub referer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stream {
    pub format_id: String,
    pub url: String,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub fps: Option<f64>,
    pub has_video: bool,
    pub has_audio: bool,
    #[serde(default)]
    pub http_headers: HttpPlaybackHeaders,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    pub language: String,
    pub extension: Option<String>,
    pub url: String,
    pub automatic: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub start_seconds: f64,
    pub end_seconds: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub provider: Provider,
    pub video_id: String,
    pub title: String,
    pub channel: Option<String>,
    pub duration_seconds: Option<u64>,
    pub thumbnail: Option<String>,
    pub streams: Capability<Stream>,
    pub captions: Capability<Caption>,
    pub chapters: Capability<Chapter>,
}

/// Metadata-only capability report. Safe to publish: it contains no signed URLs,
/// captions, titles, thumbnails, cookies or provider stderr.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Available,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CapabilityCount {
    pub status: CapabilityStatus,
    pub count: usize,
}

impl<T> From<&Capability<T>> for CapabilityCount {
    fn from(capability: &Capability<T>) -> Self {
        match capability {
            Capability::Available(items) => Self {
                status: CapabilityStatus::Available,
                count: items.len(),
            },
            Capability::Unavailable => Self {
                status: CapabilityStatus::Unavailable,
                count: 0,
            },
            Capability::Unknown => Self {
                status: CapabilityStatus::Unknown,
                count: 0,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExtractionSummary {
    pub provider: Provider,
    pub duration_seconds: Option<u64>,
    pub streams: CapabilityCount,
    pub video_streams: usize,
    pub muxed_streams: usize,
    pub video_only_streams: usize,
    pub audio_only_streams: usize,
    pub captions: CapabilityCount,
    pub automatic_caption_tracks: usize,
    pub chapters: CapabilityCount,
}

/// Public reporting seam; drops sensitive provider URLs rather than redacting
/// serialized JSON after the fact.
pub fn summarize_extraction(result: &ExtractionResult) -> ExtractionSummary {
    let (video_streams, muxed_streams, video_only_streams, audio_only_streams) = match &result.streams {
        Capability::Available(streams) => (
            streams.iter().filter(|stream| stream.has_video).count(),
            streams.iter().filter(|stream| stream.has_video && stream.has_audio).count(),
            streams.iter().filter(|stream| stream.has_video && !stream.has_audio).count(),
            streams.iter().filter(|stream| stream.has_audio && !stream.has_video).count(),
        ),
        _ => (0, 0, 0, 0),
    };
    let automatic_caption_tracks = match &result.captions {
        Capability::Available(captions) => captions.iter().filter(|c| c.automatic).count(),
        _ => 0,
    };
    ExtractionSummary {
        provider: result.provider,
        duration_seconds: result.duration_seconds,
        streams: (&result.streams).into(),
        video_streams,
        muxed_streams,
        video_only_streams,
        audio_only_streams,
        captions: (&result.captions).into(),
        automatic_caption_tracks,
        chapters: (&result.chapters).into(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeError {
    pub code: String,
    pub message: String,
    pub action: String,
}

impl ProbeError {
    fn new(code: &str, message: &str, action: &str) -> Self {
        Self { code: code.into(), message: message.into(), action: action.into() }
    }
}

pub fn canonical_video_url(input: &str) -> Result<(String, String), ProbeError> {
    let url = Url::parse(input).map_err(|_| ProbeError::new(
        "invalid_url", "Expected an absolute YouTube video URL.", "Paste a https:// YouTube watch, Shorts, live, or youtu.be link."
    ))?;
    if url.scheme() != "https" {
        return Err(ProbeError::new("invalid_url", "Only HTTPS links are accepted.", "Use the HTTPS YouTube URL."));
    }
    let host = url.host_str().unwrap_or_default();
    let id = match host {
        "youtube.com" | "www.youtube.com" | "m.youtube.com" => {
            let segments: Vec<_> = url.path_segments().into_iter().flatten().collect();
            match segments.as_slice() {
                ["watch"] => url.query_pairs().find(|(key, _)| key == "v").map(|(_, v)| v.into_owned()),
                ["shorts", id] | ["live", id] => Some((*id).to_owned()),
                _ => None,
            }
        }
        "youtu.be" | "www.youtu.be" => {
            let segments: Vec<_> = url.path_segments().into_iter().flatten().collect();
            match segments.as_slice() { [id] => Some((*id).to_owned()), _ => None }
        }
        _ => None,
    };
    let id = id.filter(|id| id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
        .ok_or_else(|| ProbeError::new("invalid_video_id", "Unsupported YouTube URL or video ID.", "Use a direct video URL with a valid 11-character ID."))?;
    Ok((id.clone(), format!("https://www.youtube.com/watch?v={id}")))
}

fn at<'a>(root: &'a Value, keys: &[&str]) -> &'a Value {
    keys.iter().find_map(|key| root.get(*key)).unwrap_or(&Value::Null)
}
fn string(value: &Value) -> Option<String> {
    value.as_str().filter(|s| !s.trim().is_empty()).map(str::to_owned)
}
fn integer(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .or_else(|| value.as_f64().filter(|v| *v >= 0.0).map(|v| v as u64))
}
fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

fn safe_http_header(value: &Value) -> Option<String> {
    string(value).filter(|s| s.len() <= 512 && !s.chars().any(char::is_control))
}

fn playback_http_headers(root: &Value, format: &Value) -> HttpPlaybackHeaders {
    let root_fields = at(root, &["http_headers", "httpHeaders"]);
    let format_fields = at(format, &["http_headers", "httpHeaders"]);
    let get = |key: &str| safe_http_header(&format_fields[key])
        .or_else(|| safe_http_header(&root_fields[key]));
    HttpPlaybackHeaders {
        user_agent: get("User-Agent"),
        referer: get("Referer"),
    }
}

fn formats(root: &Value) -> Capability<Stream> {
    let Some(entries) = root.get("formats").and_then(Value::as_array) else { return Capability::Unknown };
    let items = entries.iter().filter_map(|v| {
        let url = string(&v["url"])?;
        let format_id = string(at(v, &["format_id", "formatId"]))
            .or_else(|| integer(&v["itag"]).map(|n| n.to_string()))?;
        let has_video = at(v, &["hasVideo", "has_video"]).as_bool()
            .unwrap_or_else(|| v.get("vcodec").and_then(Value::as_str).map(|c| c != "none")
                .unwrap_or_else(|| integer(&v["height"]).is_some()));
        let has_audio = at(v, &["hasAudio", "has_audio"]).as_bool()
            .unwrap_or_else(|| v.get("acodec").and_then(Value::as_str).map(|c| c != "none")
                .unwrap_or(false));
        // yt-dlp includes storyboard/image formats under "formats". Those
        // must not be advertised as selectable audio/video streams.
        if !has_video && !has_audio { return None; }
        Some(Stream { format_id, url, width: integer(&v["width"]), height: integer(&v["height"]),
            fps: number(&v["fps"]), has_video, has_audio,
            http_headers: playback_http_headers(root, v) })
    }).collect::<Vec<_>>();
    if items.is_empty() { Capability::Unavailable } else { Capability::Available(items) }
}

fn captions(root: &Value) -> Capability<Caption> {
    let reported = root.get("subtitles").is_some() || root.get("automatic_captions").is_some();
    if !reported { return Capability::Unknown }
    let mut items = Vec::new();
    for (field, automatic) in [("subtitles", false), ("automatic_captions", true)] {
        if let Some(languages) = root.get(field).and_then(Value::as_object) {
            for (language, tracks) in languages {
                if let Some(tracks) = tracks.as_array() {
                    for track in tracks {
                        if let Some(url) = string(&track["url"]) {
                            items.push(Caption { language: language.to_owned(),
                                extension: string(&track["ext"]), url, automatic });
                        }
                    }
                }
            }
        }
    }
    if items.is_empty() { Capability::Unavailable } else { Capability::Available(items) }
}

fn chapters(root: &Value) -> Capability<Chapter> {
    let Some(entries) = root.get("chapters").and_then(Value::as_array) else { return Capability::Unknown };
    let items = entries.iter().filter_map(|v| {
        let title = string(&v["title"])?;
        let start_seconds = number(at(v, &["start_time", "startTime", "start_seconds"]))?;
        if !start_seconds.is_finite() || start_seconds < 0.0 { return None }
        let end_seconds = number(at(v, &["end_time", "endTime", "end_seconds"]))
            .filter(|end| end.is_finite() && *end >= start_seconds);
        Some(Chapter { title, start_seconds, end_seconds })
    }).collect::<Vec<_>>();
    if items.is_empty() { Capability::Unavailable } else { Capability::Available(items) }
}

/// Offline-testable public adapter boundary. No IO and no provider-specific types escape.
pub fn parse_provider_json(provider: Provider, expected_id: &str, json: &str) -> Result<ExtractionResult, ProbeError> {
    if provider == Provider::Auto {
        return Err(ProbeError::new("invalid_provider", "Auto is a routing policy, not a JSON format.", "Select rust or yt-dlp for a fixture."));
    }
    let value: Value = serde_json::from_str(json).map_err(|_| ProbeError::new(
        "malformed_response", "Provider returned invalid JSON.", "Update the extractor or inspect local logs without sharing stream URLs."
    ))?;
    if !value.is_object() {
        return Err(ProbeError::new("malformed_response", "Provider returned an unexpected document.", "Use a single video URL."));
    }
    let details = if provider == Provider::Rust { at(&value, &["video_details", "videoDetails"]) } else { &value };
    let title = string(&details["title"]).ok_or_else(|| ProbeError::new(
        "missing_metadata", "Extractor could not read the video title.", "Retry or switch extraction provider."
    ))?;
    let returned_id = string(at(details, &["video_id", "videoId", "id"]));
    if let Some(id) = returned_id {
        if id != expected_id {
            return Err(ProbeError::new("id_mismatch", "Provider returned a different video.", "Check the URL and retry."));
        }
    }
    let thumbnail = string(&details["thumbnail"]).or_else(||
        at(details, &["thumbnails"]).as_array().and_then(|a| a.last()).and_then(|item| string(&item["url"])));
    let channel = string(at(details, &["channel", "uploader", "ownerChannelName", "owner_channel_name", "author"]));
    let duration_seconds = integer(at(details, &["duration", "lengthSeconds", "length_seconds", "durationSeconds"]));
    Ok(ExtractionResult {
        provider, video_id: expected_id.into(), title, channel, duration_seconds, thumbnail,
        streams: formats(&value), captions: captions(&value), chapters: chapters(details),
    })
}

async fn rust_extract(url: &str, id: &str) -> Result<ExtractionResult, ProbeError> {
    let video = rusty_ytdl::Video::new(url.to_owned()).map_err(|_| ProbeError::new(
        "rust_extractor", "Rust provider could not initialize.", "Retry with yt-dlp provider."
    ))?;
    let info = timeout(Duration::from_secs(45), video.get_info()).await.map_err(|_| ProbeError::new(
        "timeout", "Rust provider exceeded its time limit.", "Retry or choose yt-dlp."
    ))?.map_err(|_| ProbeError::new("rust_extractor", "Rust extraction failed or access was restricted.", "Retry with yt-dlp or open the link in a browser."))?;
    let json = serde_json::to_string(&info).map_err(|_| ProbeError::new(
        "malformed_response", "Could not serialize Rust provider output.", "Report the extractor version."
    ))?;
    parse_provider_json(Provider::Rust, id, &json)
}

/// yt-dlp's projected output preserves all media formats and root HTTP
/// compatibility headers, but deliberately omits large caption/chapters data.
/// This is an opt-in playback experiment; the normal extractor remains full.
pub const YT_DLP_PLAYBACK_FORMAT_ARGS: &[&str] = &[
    "--print", "%(.{id,title,duration,thumbnail,channel,http_headers,formats})j",
];

fn yt_dlp_output_args(lean_metadata: bool) -> &'static [&'static str] {
    if lean_metadata { YT_DLP_PLAYBACK_FORMAT_ARGS } else { &["--dump-single-json"] }
}

async fn yt_dlp_extract(
    url: &str,
    id: &str,
    helper: &Path,
    lean_metadata: bool,
) -> Result<ExtractionResult, ProbeError> {
    let mut cmd = Command::new(helper);
    cmd.kill_on_drop(true)
        .args(["--ignore-config", "--no-playlist", "--no-warnings", "--skip-download"])
        .args(yt_dlp_output_args(lean_metadata))
        .args(["--socket-timeout", "10", "--", url]);
    let output = timeout(Duration::from_secs(45), cmd.output()).await
        .map_err(|_| ProbeError::new("timeout", "yt-dlp exceeded its time limit.", "Retry or choose Rust provider."))?
        .map_err(|_| ProbeError::new("helper_missing", "yt-dlp could not be started.", "Supply --yt-dlp PATH to a compatible local helper."))?;
    if !output.status.success() {
        return Err(ProbeError::new("yt_dlp_failed", "yt-dlp could not resolve the video.", "Update the helper or open the video in a browser."));
    }
    let json = std::str::from_utf8(&output.stdout).map_err(|_| ProbeError::new(
        "malformed_response", "yt-dlp returned non-UTF8 output.", "Update yt-dlp and retry."
    ))?;
    parse_provider_json(Provider::YtDlp, id, json)
}

/// Full capability extraction used by the public harness. Never silently
/// lose captions or chapters in this API.
pub async fn extract(url: &str, selected: Provider, helper: &Path) -> Result<ExtractionResult, ProbeError> {
    extract_playback(url, selected, helper, false).await
}

/// Playback-specific extraction can opt into a smaller yt-dlp response.
/// Rust provider handling and auto fallback semantics remain unchanged.
/// Omitted caption/chapter metadata is reported as Unknown, not Unavailable.
pub async fn extract_playback(
    url: &str,
    selected: Provider,
    helper: &Path,
    lean_metadata: bool,
) -> Result<ExtractionResult, ProbeError> {
    let (id, canonical) = canonical_video_url(url)?;
    match selected {
        Provider::Rust => rust_extract(&canonical, &id).await,
        Provider::YtDlp => yt_dlp_extract(&canonical, &id, helper, lean_metadata).await,
        Provider::Auto => match rust_extract(&canonical, &id).await {
            Ok(result) if matches!(&result.streams, Capability::Available(_)) => Ok(result),
            _ => yt_dlp_extract(&canonical, &id, helper, lean_metadata).await,
        },
    }
}

/// Read-only video search result. The public watch URL is constructed from
/// video_id later; this type cannot expose signed media URLs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub video_id: String,
    pub title: String,
    pub channel: Option<String>,
    pub duration_seconds: Option<u64>,
}

/// Validate a search before launching any helper or performing network IO.
pub fn validated_search_query(input: &str) -> Result<String, ProbeError> {
    let query = input.trim();
    if query.is_empty() || query.chars().count() > 120 || query.chars().any(char::is_control) {
        return Err(ProbeError::new(
            "invalid_search", "Search must be 1–120 characters on one line.",
            "Enter a short video search phrase.",
        ));
    }
    Ok(query.to_owned())
}

/// Parse yt-dlp's flat playlist search response; skip non-video entries and
/// malformed IDs rather than allowing an arbitrary URL into playback.
pub fn parse_search_json(json: &str) -> Result<Vec<SearchHit>, ProbeError> {
    let root: Value = serde_json::from_str(json).map_err(|_| ProbeError::new(
        "search_malformed", "Search provider returned malformed data.", "Retry the search or update yt-dlp."
    ))?;
    let entries = root.get("entries").and_then(Value::as_array).ok_or_else(|| ProbeError::new(
        "search_malformed", "Search results could not be read.", "Retry the search or update yt-dlp."
    ))?;
    Ok(entries.iter().filter_map(|item| {
        if item.get("_type").and_then(Value::as_str) == Some("playlist") { return None; }
        let id = string(&item["id"])?;
        if id.len() != 11 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return None;
        }
        let title = string(&item["title"])?;
        Some(SearchHit {
            video_id: id,
            title,
            channel: string(at(item, &["channel", "uploader"])),
            duration_seconds: integer(&item["duration"]),
        })
    }).take(8).collect())
}

/// Search via the existing on-device yt-dlp helper without extra proxy
/// services, authentication, downloads, or per-result video extraction.
/// The flat result's metadata is intentionally best-effort.
pub async fn search_youtube(query: &str, helper: &Path) -> Result<Vec<SearchHit>, ProbeError> {
    let query = validated_search_query(query)?;
    let target = format!("ytsearch8:{query}");
    let mut cmd = Command::new(helper);
    cmd.kill_on_drop(true).args([
        "--ignore-config", "--no-warnings", "--flat-playlist",
        "--yes-playlist", "--skip-download", "--dump-single-json",
        "--socket-timeout", "10", "--", &target,
    ]);
    let output = timeout(Duration::from_secs(40), cmd.output()).await
        .map_err(|_| ProbeError::new("search_timeout", "Search timed out.", "Retry with a shorter query."))?
        .map_err(|_| ProbeError::new("helper_missing", "Search helper could not start.", "Install or configure local yt-dlp."))?;
    if !output.status.success() {
        return Err(ProbeError::new("search_failed", "YouTube search failed.", "Retry or update the local yt-dlp helper."));
    }
    let json = std::str::from_utf8(&output.stdout).map_err(|_| ProbeError::new(
        "search_malformed", "Search helper returned invalid text.", "Update yt-dlp and retry."
    ))?;
    parse_search_json(json)
}
