#![cfg(target_os = "windows")]
//! Native watch-shell increment: one Slint window and one libmpv core.
//! Experimental local-file and YouTube URL playback, not a release-ready watch page.

mod mpv;
mod seek_benchmark;

use mpv::{PlaybackEvent, Player};
use yt_lite_native_player_proof::transport_recovery::{missing_required_audio_after_open_failure, startup_av_ready, StartupRecovery, RecoveryAction, RECOVERY_BUDGET, ATTEMPT_BUDGET};
use yt_lite_extraction_proof::persistent::PersistentWorker;
use slint::{ComponentHandle, GraphicsAPI, Model, RenderingState, Timer, TimerMode};
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::mpsc::{self, Receiver, TryRecvError}, time::{Duration, Instant}};
use yt_lite_extraction_proof::{canonical_video_url, extract_playback_attempt, trace_event, trace_id, search_youtube_with_limit, validated_search_query, HttpPlaybackHeaders, Provider, SearchHit, SearchPage, SEARCH_BATCH_SIZE, SEARCH_MAX_RESULTS};
use yt_lite_native_player_proof::{command_for, fallback_after_playback_error, preferred_playback_provider, present_watch_metadata, public_youtube_thumbnail_url, select_muxed_stream, select_playback_streams, AudioContinuity, PlaybackAction, PlaybackStreamSelection, WatchMetadata};

slint::slint! {
    import { Button, HorizontalBox, VerticalBox, LineEdit, ListView, Palette } from "std-widgets.slint";

    export struct SearchRow {
        id: string,
        title: string,
        details: string,
        thumbnail: image,
        has-thumbnail: bool,
    }

    export component PlayerWindow inherits Window {
        title: "yt-lite / watch";
        preferred-width: 1000px;
        // 560 logical px leaves room for window decorations/taskbar at 150% DPI.
        preferred-height: 560px;
        min-width: 660px;
        min-height: 480px;
        background: transparent;

        in-out property <bool> paused: false;
        in-out property <float> fraction: 0;
        in-out property <string> status: "Paste a YouTube URL above to begin."; 
        in-out property <string> url-text: "";
        in-out property <bool> has-url: false;
        in-out property <bool> has-media: false;
        in-out property <bool> dark-mode: Palette.color-scheme == ColorScheme.dark;
        in-out property <bool> has-video-info: false;
        in-out property <string> video-title: "";
        in-out property <string> video-subtitle: "";
        in-out property <image> video-thumbnail;
        in-out property <bool> has-thumbnail: false;
        in-out property <bool> can-retry: false;
        callback retry();
        in-out property <bool> show-search: false;
        in-out property <string> search-status: "";
        in-out property <[SearchRow]> search-results;
        in-out property <bool> has-more-results: false;
        callback load-more();
        callback search(string);
        callback submit-input(string);
        callback open-result(string);
        callback close-search();
        callback open-url(string);
        callback open-browser();
        callback toggle-pause();
        callback seek-by(int);
        callback seek-to(float);
        callback toggle-fullscreen();

        // An interactive native watch header over the video render surface.
        // LineEdit offers keyboard focus/paste, and Enter also opens the URL.
        Rectangle {
            x: 0px;
            y: 0px;
            width: parent.width;
            height: 64px;
            background: root.dark-mode ? #ee1b202b : #f1f4f6;
            HorizontalBox {
                padding: 10px;
                spacing: 8px;
                Text {
                    text: "yt-lite";
                    color: root.dark-mode ? white : #19242f;
                    font-size: 18px;
                    vertical-alignment: center;
                }
                LineEdit {
                    horizontal-stretch: 1;
                    text <=> root.url-text;
                    placeholder-text: "Paste video URL or search phrase (Enter)...";
                    accepted(text) => { root.submit-input(text); }
                }
                Button {
                    text: "Open";
                    clicked => { root.open-url(root.url-text); }
                }
                Button {
                    text: "Search";
                    clicked => { root.search(root.url-text); }
                }
                Button {
                    text: "Browser";
                    enabled: root.has-url;
                    clicked => { root.open-browser(); }
                }
                Button {
                    text: root.dark-mode ? "Light" : "Dark";
                    clicked => { Palette.color-scheme = root.dark-mode ? ColorScheme.light : ColorScheme.dark; }
                }
            }
        }

        // Read-only details above the native video surface; image data arrives
        // asynchronously and only from the public YouTube thumbnail origin.
        Rectangle {
            x: 0px;
            y: 64px;
            width: parent.width;
            height: 54px;
            visible: root.has-video-info;
            background: root.dark-mode ? #d51b202b : #edf1f5;
            HorizontalBox {
                padding: 8px;
                spacing: 10px;
                Image {
                    source: root.video-thumbnail;
                    visible: root.has-thumbnail;
                    width: 64px;
                    height: 36px;
                    image-fit: contain;
                }
                Text {
                    text: root.video-title;
                    color: root.dark-mode ? white : #19242f;
                    font-size: 15px;
                    vertical-alignment: center;
                    horizontal-stretch: 1;
                    overflow: elide;
                }
                Text {
                    text: root.video-subtitle;
                    color: root.dark-mode ? #d2d7e2 : #34485f;
                    font-size: 12px;
                    vertical-alignment: center;
                    overflow: elide;
                }
            }
        }

        // Reserve real space for the controls. Fixed pixel positions avoid
        // Slint's vertical stretch distributing a row below the viewport
        // at 125–200% Windows DPI scaling.
        Rectangle {
            width: parent.width;
            height: 116px;
            x: 0px;
            y: parent.height - self.height;
            background: root.dark-mode ? #ed1b202b : #eef1f4;
            HorizontalBox {
                x: 12px;
                y: 7px;
                width: parent.width - 24px;
                height: 28px;
                spacing: 8px;
                Text {
                    text: root.status;
                    color: root.dark-mode ? white : #19242f;
                    font-size: 13px;
                    vertical-alignment: center;
                    horizontal-stretch: 1;
                    overflow: elide;
                }
                Button {
                    text: "Retry";
                    visible: root.can-retry;
                    clicked => { root.retry(); }
                }
            }
            Rectangle {
                x: 12px;
                y: 44px;
                width: parent.width - 24px;
                height: 9px;
                background: #677381;
                border-radius: 4px;
                Rectangle {
                    x: 0px;
                    y: 0px;
                    width: parent.width * root.fraction;
                    height: parent.height;
                    background: #59baff;
                    border-radius: 4px;
                }
                TouchArea {
                    clicked => { root.seek-to(self.mouse-x / self.width); }
                }
            }
            HorizontalBox {
                x: 12px;
                y: 63px;
                width: parent.width - 24px;
                height: 42px;
                spacing: 10px;
                Button {
                    enabled: root.has-media;
                    text: root.paused ? "Play" : "Pause";
                    clicked => { root.toggle-pause(); }
                }
                Button {
                    enabled: root.has-media;
                    text: "-10 s";
                    clicked => { root.seek-by(-10); }
                }
                Button {
                    enabled: root.has-media;
                    text: "+10 s";
                    clicked => { root.seek-by(10); }
                }
                Rectangle { horizontal-stretch: 1; background: transparent; }
                Button {
                    text: "Fullscreen";
                    clicked => { root.toggle-fullscreen(); }
                }
            }
        }

        // Native search results remain a separate browsing overlay; searching
        // never pauses, unloads or reconstructs the current libmpv session.
        Rectangle {
            visible: root.show-search;
            x: parent.width - self.width - 10px;
            y: 120px;
            width: 350px;
            height: parent.height - 242px;
            background: root.dark-mode ? #f21b202b : #f5f8fb;
            border-radius: 8px;
            VerticalBox {
                padding: 10px;
                spacing: 8px;
                HorizontalBox {
                    spacing: 6px;
                    Text {
                        text: root.search-status;
                        color: root.dark-mode ? white : #19242f;
                        font-size: 13px;
                        horizontal-stretch: 1;
                        vertical-alignment: center;
                        overflow: elide;
                    }
                    Button {
                        text: "Close";
                        clicked => { root.close-search(); }
                    }
                }
                ListView {
                    vertical-stretch: 1;
                    for hit in root.search-results : Rectangle {
                        width: parent.width;
                        height: 76px;
                        background: root.dark-mode ? #243043 : #e9eef3;
                        border-radius: 4px;
                        Rectangle {
                            x: 7px;
                            y: 10px;
                            width: 93px;
                            height: 53px;
                            background: root.dark-mode ? #34455b : #d5dfe6;
                            Image {
                                source: hit.thumbnail;
                                visible: hit.has-thumbnail;
                                width: parent.width;
                                height: parent.height;
                                image-fit: contain;
                            }
                        }
                        Text {
                            text: hit.title;
                            x: 108px;
                            y: 7px;
                            width: parent.width - 116px;
                            height: 35px;
                            color: root.dark-mode ? white : #19242f;
                            font-size: 13px;
                            overflow: elide;
                            vertical-alignment: center;
                        }
                        Text {
                            text: hit.details;
                            x: 108px;
                            y: 43px;
                            width: parent.width - 116px;
                            height: 22px;
                            color: root.dark-mode ? #d2d7e2 : #34485f;
                            font-size: 11px;
                            overflow: elide;
                            vertical-alignment: center;
                        }
                        TouchArea { clicked => { root.open-result(hit.id); } }
                    }
                }
                Button {
                    text: "Load more results";
                    visible: root.has-more-results;
                    clicked => { root.load-more(); }
                }
            }
        }
    }
}

fn time_label(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[derive(Debug)]
struct ResolvedStream {
    url: String,
    audio_url: Option<String>,
    audio_direct_http: bool,
    fallback_muxed: Option<(String, HttpPlaybackHeaders)>,
    selected_height: Option<u64>,
    used_native_ejs: bool,
    used_fast_extraction: bool,
    used_persistent: bool,
    trace: Option<u64>,
    video_format: String,
    audio_format: Option<String>,
    watch: WatchMetadata,
    provider: Provider,
    headers: HttpPlaybackHeaders,
    extraction_ms: u128,
}

type StreamResolution = Result<ResolvedStream, String>;

// Diagnostic resolver only: both generations use the actual pending-media
// and recovery paths, without a YouTube request or arbitrary host.
fn startup_fixture_resolution(port: u16, attempt: u8) -> Receiver<StreamResolution> {
    let (tx, rx) = mpsc::channel();
    let origin = format!("http://127.0.0.1:{port}/attempt/{attempt}");
    let _ = tx.send(Ok(ResolvedStream {
        url: format!("{origin}/video.mp4"), audio_url: Some(format!("{origin}/audio.m4a")),
        audio_direct_http: false, fallback_muxed: None, selected_height: None, used_native_ejs: false,
        used_fast_extraction: false, used_persistent: false, trace: Some(trace_id()),
        video_format: "fixture-video".into(), audio_format: Some("fixture-audio".into()),
        watch: WatchMetadata { title: "HTTP startup fixture".into(), subtitle: "Loopback split media".into(), thumbnail_url: None },
        provider: Provider::YtDlp, headers: HttpPlaybackHeaders::default(), extraction_ms: 0,
    }));
    trace_event("startup_fixture_resolved", format_args!("attempt={attempt}"));
    rx
}

/// Fetch metadata off the UI thread, preferring a muxed stream. If the
/// video only offers separate A/V formats, keep both URLs inside this worker
/// response for libmpv to stream independently. No video is downloaded.
fn spawn_extraction(video_url: String, helper: PathBuf, provider: Provider, lean_metadata: bool, native_ejs: Option<PathBuf>, profile_timings: bool, fast_extraction: bool, persistent: Option<&PersistentWorker>) -> Receiver<StreamResolution> {
    // Submit on the caller thread to preserve the actual selection order.
    let ticket = persistent.filter(|_| provider == Provider::YtDlp)
        .map(|worker| worker.submit(&video_url));
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let extraction_started = Instant::now();
        let result = (|| -> StreamResolution {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "Could not initialize network extraction runtime".to_string())?;
            runtime.block_on(async {
                let mut used_native_ejs = provider == Provider::YtDlp && native_ejs.is_some();
                let mut used_persistent = ticket.is_some();
                let mut used_fast_extraction = provider == Provider::YtDlp && (fast_extraction || used_persistent);
                let solver = if used_native_ejs { native_ejs.as_deref() } else { None };
                let mut trace = if profile_timings { Some(ticket.as_ref().map(|t| t.id()).unwrap_or_else(trace_id)) } else { None };
                let mut attempt = if let Some(t) = ticket.as_ref() {
                    let result = t.resolve();
                    if !t.is_current() { return Err("Extraction superseded".into()); }
                    result
                } else {
                    extract_playback_attempt(&video_url, provider, &helper, lean_metadata, solver, trace, used_fast_extraction).await
                };
                // A zero exit code is not sufficient: a solver failure may
                // leave only unusable formats in the returned metadata.
                if (used_native_ejs || used_fast_extraction) && !attempt.as_ref().ok().is_some_and(|info| select_playback_streams(info).is_some()) {
                    eprintln!("[yt-lite] experimental extraction failed or returned no playable pair; retrying standard yt-dlp");
                    used_native_ejs = false;
                    used_fast_extraction = false;
                    used_persistent = false;
                    trace = profile_timings.then(trace_id);
                    attempt = extract_playback_attempt(&video_url, provider, &helper, lean_metadata, None, trace, false).await;
                }
                let info = attempt.map_err(|error| format!("{} {}", error.message, error.action))?;
                let selected = select_playback_streams(&info).ok_or_else(|| {
                    "No playable audio+video tracks with compatible HTTP settings. Retry or open in Browser.".to_string()
                })?;
                let (video_format, audio_format) = match &selected {
                    PlaybackStreamSelection::Muxed(stream) => (stream.format_id.clone(), None),
                    PlaybackStreamSelection::Separate { video, audio } => (video.format_id.clone(), Some(audio.format_id.clone())),
                };
                let audio_direct_http = matches!(&selected, PlaybackStreamSelection::Separate { audio, .. } if matches!(audio.protocol.as_deref(), Some("https" | "http")));
                let (url, audio_url, headers, selected_height, fallback_muxed) = match selected {
                    PlaybackStreamSelection::Muxed(stream) => (
                        stream.url.clone(), None, stream.http_headers.clone(),
                        stream.height, None,
                    ),
                    PlaybackStreamSelection::Separate { video, audio } => (
                        video.url.clone(), Some(audio.url.clone()), video.http_headers.clone(),
                        video.height,
                        select_muxed_stream(&info).map(|muxed|
                            (muxed.url.clone(), muxed.http_headers.clone())
                        ),
                    ),
                };
                Ok(ResolvedStream {
                    url,
                    audio_url,
                    audio_direct_http,
                    fallback_muxed,
                    selected_height,
                    used_native_ejs,
                    used_fast_extraction,
                    used_persistent,
                    trace,
                    video_format,
                    audio_format,
                    watch: present_watch_metadata(&info),
                    provider,
                    headers,
                    extraction_ms: extraction_started.elapsed().as_millis(),
                })
            })
        })();
        let _ = sender.send(result);
    });
    receiver
}

/// Public, fixed-origin thumbnail fetch on a separate worker. It never blocks
/// media extraction or the Slint UI thread and never follows cross-origin redirects.
/// Reject oversized responses without reading the complete payload into memory.
fn spawn_thumbnail(video_id: String) -> Receiver<Option<slint::SharedPixelBuffer<slint::Rgb8Pixel>>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Option<slint::SharedPixelBuffer<slint::Rgb8Pixel>> {
            let url = public_youtube_thumbnail_url(&video_id)?;
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
            let bytes = runtime.block_on(async {
                let client = reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(6)).build().ok()?;
                let mut response = client.get(&url).send().await.ok()?;
                if !response.status().is_success() { return None; }
                let content_type = response.headers().get(reqwest::header::CONTENT_TYPE)?
                    .to_str().ok()?;
                if !content_type.starts_with("image/jpeg") && !content_type.starts_with("image/png") {
                    return None;
                }
                const MAX_BYTES: usize = 512 * 1024;
                if response.content_length().is_some_and(|len| len > MAX_BYTES as u64) {
                    return None;
                }
                let mut bytes = Vec::new();
                loop {
                    match response.chunk().await {
                        Ok(Some(chunk)) => {
                            if chunk.len() > MAX_BYTES - bytes.len() { return None; }
                            bytes.extend_from_slice(&chunk);
                        }
                        Ok(None) => break,
                        Err(_) => return None,
                    }
                }
                (!bytes.is_empty()).then_some(bytes)
            })?;
            // Decode on the worker, not in Slint's render/event loop.
            let rgb = image::load_from_memory(&bytes).ok()?.to_rgb8();
            let (width, height) = rgb.dimensions();
            if width == 0 || height == 0 || width > 1024 || height > 1024 {
                return None;
            }
            Some(slint::SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(
                rgb.as_raw(), width, height,
            ))
        })();
        let _ = sender.send(result);
    });
    receiver
}

type SearchResolution = Result<SearchPage, String>;

/// Search runs on its own worker. The current video keeps playing while
/// yt-dlp reads a small, flat result list (no per-result video extraction).
fn spawn_search(query: String, helper: PathBuf, count: usize) -> Receiver<SearchResolution> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let response = (|| -> SearchResolution {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all().build().map_err(|_| "Could not initialize search runtime".to_string())?;
            runtime.block_on(async {
                search_youtube_with_limit(&query, &helper, count).await
                    .map_err(|error| format!("{} {}", error.message, error.action))
            })
        })();
        let _ = sender.send(response);
    });
    receiver
}

fn search_result_details(hit: &SearchHit) -> String {
    let mut pieces = Vec::new();
    if let Some(channel) = hit.channel.as_deref() { pieces.push(channel.to_owned()); }
    if let Some(seconds) = hit.duration_seconds {
        pieces.push(if seconds >= 3600 {
            format!("{}:{:02}:{:02}", seconds / 3600, (seconds % 3600) / 60, seconds % 60)
        } else { format!("{}:{:02}", seconds / 60, seconds % 60) });
    }
    pieces.join(" · ")
}

fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Rust => "Rust",
        Provider::YtDlp => "yt-dlp",
        Provider::Auto => "auto",
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Flags work with or without a startup video; users can paste the URL
    // later in the watch bar. Reject ambiguous multiple media sources.
    let mut args = std::env::args().skip(1);
    let mut source: Option<String> = None;
    let mut helper = PathBuf::from("yt-dlp");
    let mut requested_provider = Provider::Auto;
    let mut profile_timings = false;
    let mut lean_metadata = false;
    let mut fast_extraction = false;
    let mut preattach_audio = false;
    let mut audio_cache = false;
    let mut seek_benchmark_mode = None;
    let mut seek_probe = false;
    let mut seek_recovery = false;
    let mut seek_fault_marker: Option<PathBuf> = None;
    let mut seek_fixture_audio: Option<PathBuf> = None;
    let mut seek_cache_expanded = false;
    let mut observed_state = false;
    let mut transport_recovery = false;
    let mut seek_http_fixture: Option<u16> = None;
    let mut startup_http_fixture: Option<u16> = None;
    let mut seek_http_edl = false;
    let mut persistent_extraction = false;
    let mut worker_python = PathBuf::from("python");
    let mut worker_script = PathBuf::from("experiments/extraction-latency/worker.py");
    let mut native_ejs: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--timings" => profile_timings = true,
            "--lean-metadata" => lean_metadata = true,
            "--fast-extraction" => fast_extraction = true,
            "--preattach-audio" => preattach_audio = true,
            "--audio-cache" => { audio_cache = true; profile_timings = true; },
            "--seek-cache-expanded" => seek_cache_expanded = true,
            "--observed-state" => observed_state = true,
            "--transport-recovery" => { transport_recovery = true; profile_timings = true; }
            "--startup-http-fixture" => {
                let port: u16 = args.next().ok_or("Missing --startup-http-fixture PORT")?.parse()?;
                if port == 0 { return Err("Fixture port must be nonzero".into()); }
                startup_http_fixture = Some(port);
                transport_recovery = true;
                profile_timings = true;
                observed_state = true;
                preattach_audio = true;
            }
            "--seek-http-edl" => seek_http_edl = true,
            "--seek-probe" => seek_probe = true,
            "--seek-recovery" => { seek_recovery = true; seek_probe = true; },
            "--seek-fault-marker" => seek_fault_marker = Some(PathBuf::from(args.next().ok_or("Missing --seek-fault-marker PATH")?)),
            "--seek-http-fixture" => {
                let port: u16 = args.next().ok_or("Missing --seek-http-fixture PORT")?.parse()?;
                if port == 0 { return Err("Fixture port must be nonzero".into()); }
                seek_http_fixture = Some(port);
            }
            "--seek-fixture-audio" => {
                let path = PathBuf::from(args.next().ok_or("Missing --seek-fixture-audio PATH")?).canonicalize()?;
                if !path.is_file() { return Err("Audio fixture must be a local file".into()); }
                seek_fixture_audio = Some(path);
            }
            "--seek-benchmark" => {
                let mode = args.next().ok_or("Missing --seek-benchmark exact|keyframes")?;
                seek_benchmark_mode = Some(yt_lite_native_player_proof::SeekBenchmarkMode::parse(&mode)
                    .ok_or("Seek benchmark mode must be exact or keyframes")?);
                profile_timings = true;
            }
            "--persistent-extraction" => persistent_extraction = true,
            "--worker-python" => worker_python = PathBuf::from(args.next().ok_or("Missing --worker-python PATH")?),
            "--worker-script" => worker_script = PathBuf::from(args.next().ok_or("Missing --worker-script PATH")?),
            "--native-ejs" => {
                let path = PathBuf::from(args.next().ok_or("Missing --native-ejs PATH")?);
                let path = path.canonicalize().map_err(|_| "Native EJS executable does not exist")?;
                if !path.is_file() { return Err("Native EJS path must be a file".into()); }
                native_ejs = Some(path);
            }
            "--yt-dlp" => {
                helper = PathBuf::from(args.next().ok_or("Missing --yt-dlp PATH")?);
            }
            "--provider" => {
                let selection = args.next().ok_or("Missing --provider VALUE")?;
                requested_provider = Provider::parse(&selection)
                    .ok_or("Provider must be auto, rust or yt-dlp")?;
            }
            _ if arg.starts_with("--") => return Err(format!("Unknown option: {arg}").into()),
            _ if source.is_none() => source = Some(arg),
            _ => return Err("Only one media source may be supplied".into()),
        }
    }
    if persistent_extraction && (fast_extraction || native_ejs.is_some() || lean_metadata || requested_provider == Provider::Rust) {
        return Err("Benchmark --persistent-extraction alone with auto or yt-dlp provider".into());
    }
    if fast_extraction && native_ejs.is_some() {
        return Err("Benchmark --fast-extraction and --native-ejs separately".into());
    }
    if audio_cache && (!preattach_audio || startup_http_fixture.is_some() || seek_http_fixture.is_some() || seek_fixture_audio.is_some()) {
        return Err("--audio-cache requires --preattach-audio and remote playback (no fixtures)".into());
    }
    if audio_cache { eprintln!("[yt-lite] experimental bounded audio prefetch enabled (4 MiB, immediate direct fallback)"); }
    if preattach_audio { eprintln!("[yt-lite] experimental preattached audio enabled (serial fallback retained)"); }
    if fast_extraction {
        eprintln!("[yt-lite] experimental fast extraction enabled (standard fallback retained)");
    }
    // Reduced output is experimental and affects yt-dlp metadata only.
    // Full output remains the default for captions/chapters compatibility.
    if lean_metadata {
        eprintln!("[yt-lite] experimental projected playback metadata enabled");
    }
    if native_ejs.is_some() {
        eprintln!("[yt-lite] experimental native EJS runtime enabled for yt-dlp (standard fallback retained)");
    }
    if startup_http_fixture.is_some() && (source.is_some() || seek_benchmark_mode.is_some() || persistent_extraction || fast_extraction || native_ejs.is_some()) {
        return Err("Startup HTTP fixture must run alone, without a source, seek benchmark or extraction flags".into());
    }
    let input = source.unwrap_or_default();
    let (clip, remote_url) = if input.is_empty() {
        (None, None)
    } else if input.starts_with("https://") || input.starts_with("http://") {
        let (_, canonical) = canonical_video_url(&input)
            .map_err(|error| format!("{} {}", error.message, error.action))?;
        (None, Some(canonical))
    } else {
        let path = PathBuf::from(input).canonicalize()?;
        if !path.is_file() { return Err("Local video path must point to a file".into()); }
        (Some(path), None)
    };
    if audio_cache && clip.is_some() { return Err("--audio-cache supports remote playback only".into()); }
    if (seek_cache_expanded || seek_fixture_audio.is_some() || seek_http_fixture.is_some()) && seek_benchmark_mode.is_none() {
        return Err("Fixture and cache options require --seek-benchmark".into());
    }
    if seek_benchmark_mode.is_some() && remote_url.is_none() && clip.is_none() {
        return Err("Seek benchmark requires a startup watch URL or local fixture".into());
    }
    if seek_http_edl && seek_http_fixture.is_none() {
        return Err("--seek-http-edl requires --seek-http-fixture".into());
    }
    if seek_http_fixture.is_some() && (clip.is_none() || seek_fixture_audio.is_some()) {
        return Err("HTTP fixture requires a local video path as fixture marker and no --seek-fixture-audio".into());
    }
    if seek_recovery && (seek_http_fixture.is_none() || seek_http_edl || seek_benchmark_mode != Some(yt_lite_native_player_proof::SeekBenchmarkMode::Exact)) {
        return Err("--seek-recovery requires exact external-audio loopback seek benchmark".into());
    }
    if seek_probe && (seek_benchmark_mode.is_none() || !observed_state) { return Err("--seek-probe requires --seek-benchmark and --observed-state".into()); }
    if seek_fault_marker.is_some() && (seek_http_fixture.is_none() || seek_benchmark_mode.is_none() || seek_http_edl) {
        return Err("--seek-fault-marker requires the external audio HTTP seek fixture".into());
    }
    if transport_recovery && seek_benchmark_mode.is_some() {
        return Err("Test startup recovery separately from the seek benchmark".into());
    }
    if transport_recovery { eprintln!("[yt-lite] experimental startup recovery: fresh extraction once, 30s recovery budget"); }
    if seek_fixture_audio.is_some() && clip.is_none() {
        return Err("Audio fixture requires a local video file".into());
    }
    if seek_benchmark_mode.is_some() {
        eprintln!("[yt-lite] seek-bench config source={} cache={} forward_secs=8",
            if seek_http_edl { "loopback_http_edl" } else if seek_http_fixture.is_some() { "loopback_http_split" } else if seek_fixture_audio.is_some() { "local_split" } else if clip.is_some() { "local_muxed" } else { "remote" },
            if seek_cache_expanded { "64MiB/32MiB" } else { "16MiB/4MiB" });
    }
    let initial_provider = preferred_playback_provider(requested_provider);
    let persistent = if persistent_extraction {
        eprintln!("[yt-lite] experimental persistent extraction enabled (standard fallback retained)");
        Some(PersistentWorker::start(worker_python, worker_script, profile_timings))
    } else { None };
    let remote_receiver = if let Some(port) = startup_http_fixture {
        Some(startup_fixture_resolution(port, 1))
    } else { remote_url.clone().map(|url| spawn_extraction(url, helper.clone(), initial_provider, lean_metadata, native_ejs.clone(), profile_timings, fast_extraction, persistent.as_ref())) };
    // mpv's OpenGL proc resolver uses WGL; do not silently choose Direct3D or software.
    slint::BackendSelector::new().backend_name("winit".into())
        .renderer_name("femtovg".into()).require_opengl().select()?;
    let window = PlayerWindow::new()?;
    window.set_has_url(remote_url.is_some());
    window.set_has_media(clip.is_some());
    if let Some(ref url) = remote_url {
        window.set_url_text(url.as_str().into());
    }

    // UI callbacks only enqueue a request. The timer handles validation,
    // cancellation and lifecycle; no synchronous network IO runs in Slint.
    let requested_url = Rc::new(RefCell::new(None::<String>));
    let new_request = requested_url.clone();
    window.on_open_url(move |input| {
        *new_request.borrow_mut() = Some(input.to_string());
    });
    let search_requested = Rc::new(RefCell::new(None::<String>));
    let new_search = search_requested.clone();
    window.on_search(move |input| {
        *new_search.borrow_mut() = Some(input.to_string());
    });
    let more_search_requested = Rc::new(RefCell::new(false));
    let requested_more = more_search_requested.clone();
    window.on_load_more(move || { *requested_more.borrow_mut() = true; });
    // Enter in the shared field follows its intent: URL -> watch, phrase ->
    // search. Explicit Open and Search buttons remain available to everyone.
    let enter_open = requested_url.clone();
    let enter_search = search_requested.clone();
    window.on_submit_input(move |input| {
        let value = input.to_string();
        if value.starts_with("http://") || value.starts_with("https://")
            || value.contains("youtube.com/") || value.contains("youtu.be/")
        {
            *enter_open.borrow_mut() = Some(value);
        } else {
            *enter_search.borrow_mut() = Some(value);
        }
    });

    let result_request = requested_url.clone();
    let result_window = window.as_weak();
    window.on_open_result(move |id| {
        let url = format!("https://www.youtube.com/watch?v={id}");
        if let Ok((_, canonical)) = canonical_video_url(&url) {
            *result_request.borrow_mut() = Some(canonical);
            if let Some(ui) = result_window.upgrade() {
                ui.set_show_search(false);
            }
        }
    });

    let close_window = window.as_weak();
    window.on_close_search(move || {
        if let Some(ui) = close_window.upgrade() { ui.set_show_search(false); }
    });

    // Browser fallback gets only a canonicalized URL; never a signed media
    // URL, arbitrary URI scheme or extractor response.
    let browser_url = Rc::new(RefCell::new(remote_url.clone()));
    let external_url = browser_url.clone();
    window.on_open_browser(move || {
        if let Some(url) = external_url.borrow().clone() {
            std::thread::spawn(move || {
                if webbrowser::open(&url).is_err() {
                    eprintln!("[yt-lite] Could not open the system browser");
                }
            });
        }
    });
    // Retry uses the same canonical URL and worker path as a normal Open.
    let retry_request = requested_url.clone();
    let retry_url = browser_url.clone();
    window.on_retry(move || {
        if let Some(url) = retry_url.borrow().clone() {
            *retry_request.borrow_mut() = Some(url);
        }
    });

    let clip_source = Rc::new(RefCell::new(clip));
    let player = Rc::new(RefCell::new(if seek_cache_expanded { Player::open_with_cache(true)? } else { Player::open()? }));
    if profile_timings { player.borrow().enable_diagnostics(); }
    if observed_state { player.borrow().enable_observed_state(); }

    let seek_started = Rc::new(RefCell::new(None::<(u64, Instant)>));

    let controls = player.clone();
    window.on_toggle_pause(move || {
        if let Some(command) = command_for(PlaybackAction::TogglePause) {
            if let Err(error) = controls.borrow().send_strings(&command) {
                eprintln!("Pause error: {error}");
            }
        }
    });

    let controls = player.clone();
    let relative_seek_started = seek_started.clone();
    window.on_seek_by(move |seconds| {
        if let Some(command) = command_for(PlaybackAction::SeekRelative(seconds.into())) {
            let requested = Instant::now();
            let before = if profile_timings { controls.borrow().property_double(c"time-pos") } else { None };
            match controls.borrow().send_strings(&command) {
                Ok(()) if profile_timings => {
                    let id = trace_id();
                    if let Some((old, _)) = relative_seek_started.borrow_mut().replace((id, requested)) {
                        trace_event("seek_superseded", format_args!("id={old} next={id}"));
                    }
                    trace_event("seek_requested", format_args!("id={id} mode=relative from={before:?} seconds={seconds} dispatch_us={}", requested.elapsed().as_micros()));
                }
                Err(error) => eprintln!("Seek error: {error}"),
                _ => {}
            }
        }
    });

    let controls = player.clone();
    let absolute_seek_started = seek_started.clone();
    window.on_seek_to(move |fraction| {
        let duration = controls.borrow().property_double(c"duration").unwrap_or(0.0);
        if let Some(command) = command_for(PlaybackAction::SeekFraction {
            fraction: f64::from(fraction), duration_seconds: duration,
        }) {
            let requested = Instant::now();
            let before = if profile_timings { controls.borrow().property_double(c"time-pos") } else { None };
            match controls.borrow().send_strings(&command) {
                Ok(()) if profile_timings => {
                    let id = trace_id();
                    if let Some((old, _)) = absolute_seek_started.borrow_mut().replace((id, requested)) {
                        trace_event("seek_superseded", format_args!("id={old} next={id}"));
                    }
                    trace_event("seek_requested", format_args!("id={id} mode=absolute from={before:?} fraction={fraction} dispatch_us={}", requested.elapsed().as_micros()));
                }
                Err(error) => eprintln!("Scrub error: {error}"),
                _ => {}
            }
        }
    });

    let fullscreen = window.as_weak();
    window.on_toggle_fullscreen(move || {
        if let Some(window) = fullscreen.upgrade() {
            let state = window.window().is_fullscreen();
            window.window().set_fullscreen(!state);
        }
    });

    let video_window = window.as_weak();
    let video_session = player.clone();
    let rendering_clip = clip_source.clone();
    let rendering_audio = seek_fixture_audio;
    let mut local_split_started = false;
    let mut margin_warning_logged = false;
    window.window().set_rendering_notifier(move |phase, api| {
        if !matches!(api, GraphicsAPI::NativeOpenGL { .. }) { return; }
        match phase {
            RenderingState::RenderingSetup => {
                let mut session = video_session.borrow_mut();
                let clip = rendering_clip.borrow();
                let setup = session.setup_render(video_window.clone(), if rendering_audio.is_some() || seek_http_fixture.is_some() { None } else { clip.as_deref() });
                if let Err(error) = setup {
                    eprintln!("Video initialization error: {error}");
                } else if !local_split_started {
                    if let Some(port) = seek_http_fixture.filter(|_| clip.is_some()) {
                        let video = format!("http://127.0.0.1:{port}/video.mp4");
                        let audio = format!("http://127.0.0.1:{port}/audio.m4a");
                        trace_event("http_fixture_load_requested", format_args!("composition={}", if seek_http_edl { "edl" } else { "external_audio" }));
                        let result = if seek_http_edl { session.load_loopback_edl(port) }
                            else { session.load_network_with_audio(&video, &audio, &HttpPlaybackHeaders::default()) };
                        match result {
                            Ok(()) => local_split_started = true,
                            Err(_) => eprintln!("[yt-lite] loopback HTTP fixture failed to load"),
                        }
                    } else if let (Some(video), Some(audio)) = (clip.as_deref(), rendering_audio.as_deref()) {
                        match session.load_local_split(video, audio) {
                            Ok(()) => local_split_started = true,
                            Err(_) => eprintln!("[yt-lite] local split fixture failed to load"),
                        }
                    }
                }
            }
            RenderingState::BeforeRendering => {
                if let Some(ui) = video_window.upgrade() {
                    let size = ui.window().size();
                    let mut player = video_session.borrow_mut();
                    if let Err(error) = player.update_video_margins(
                        size.width, size.height, ui.window().scale_factor(),
                        ui.get_has_video_info(), ui.get_show_search(),
                    ) {
                        if !margin_warning_logged {
                            eprintln!("[yt-lite] Could not reserve video viewport: {error}");
                            margin_warning_logged = true;
                        }
                    }
                    player.draw(size.width, size.height);
                }
            }
            RenderingState::RenderingTeardown => video_session.borrow_mut().teardown_render(),
            _ => {}
        }
    })?;

    let label_window = window.as_weak();
    let label_player = player.clone();
    let pending_seek_timing = seek_started.clone();
    let mut receiver = remote_receiver;
    let mut search_receiver: Option<Receiver<SearchResolution>> = None;
    let mut active_search_query: Option<String> = None;
    let mut search_count = SEARCH_BATCH_SIZE;
    let mut search_rows: Option<Rc<slint::VecModel<SearchRow>>> = None;
    let mut result_thumbnail_receivers: Vec<(usize, Receiver<Option<slint::SharedPixelBuffer<slint::Rgb8Pixel>>>)> = Vec::new();
    let mut thumbnail_receiver: Option<Receiver<Option<slint::SharedPixelBuffer<slint::Rgb8Pixel>>>> = None;
    let mut remote_url = remote_url;
    let mut pending: Option<StreamResolution> = None;
    let mut resolving = receiver.is_some();
    let mut media_title: Option<String> = None;
    let mut active_provider = initial_provider;
    let mut playback_error: Option<String> = None;
    let mut load_started: Option<Instant> = None;
    let mut file_loaded = false;
    let mut file_loaded_at: Option<Instant> = None;
    let mut playback_advanced = false;
    let mut has_network_media = false;
    let mut pending_external_audio: Option<String> = None;
    // Retain only the current split pair until the first audio clock is observed.
    let mut preattach_retry: Option<(String, String, HttpPlaybackHeaders)> = None;
    let mut media_load_dispatched: Option<Instant> = None;
    let mut startup_recovery = StartupRecovery::default();
    let mut recovery_started: Option<Instant> = None;
    let mut startup_complete = false;
    let mut startup_split = false;
    // At most one experimental audio-cache attempt per explicitly selected watch.
    let mut audio_cache_attempted = false;
    let mut explicit_audio_open_failure = false;
    let mut fallback_muxed: Option<(String, HttpPlaybackHeaders)> = None;
    let mut used_native_ejs = false;
    let mut used_fast_extraction = false;
    let mut audio_monitor = AudioContinuity::default();
    let mut audio_warning = false;
    let mut next_audio_sample = Instant::now();
    let mut next_audio_log = Instant::now();
    let mut active_trace: Option<u64> = None;
    let mut audio_started: Option<Instant> = None;
    let mut first_audio = false;
    let mut awaiting_seek_audio: Option<(u64, Instant)> = None;
    let mut previous_wait_state = None;
    let mut last_tick = Instant::now();
    let mut seek_benchmark = seek_benchmark_mode.map(|mode| seek_benchmark::SeekBenchmark::new(mode, seek_probe, seek_fault_marker, seek_recovery));
    struct TickDuration(Option<Instant>);
    impl Drop for TickDuration {
        fn drop(&mut self) {
            if let Some(started) = self.0 {
                let us = started.elapsed().as_micros();
                if us >= 20_000 { trace_event("ui_tick_slow", format_args!("elapsed_us={us}")); }
            }
        }
    }
    let clock = Timer::default();
    let tick_ms = if seek_benchmark.is_some() { 50 } else { 250 };
    clock.start(TimerMode::Repeated, Duration::from_millis(tick_ms), move || {
        let _tick_duration = TickDuration(profile_timings.then(Instant::now));
        let now = Instant::now();
        if profile_timings && now.duration_since(last_tick) > Duration::from_millis(500) {
            trace_event("ui_timer_gap", format_args!("gap_ms={}", now.duration_since(last_tick).as_millis()));
        }
        last_tick = now;
        if let Some(query) = search_requested.borrow_mut().take() {
            // Old thumbnails may finish late; never attach to new search rows.
            result_thumbnail_receivers.clear();
            search_rows = None;
            active_search_query = None;
            search_count = SEARCH_BATCH_SIZE;
            *more_search_requested.borrow_mut() = false;
            if let Some(ui) = label_window.upgrade() {
                ui.set_show_search(true);
                ui.set_has_more_results(false);
                ui.set_search_results(slint::ModelRc::new(slint::VecModel::from(Vec::<SearchRow>::new())));
                match validated_search_query(&query) {
                    Ok(clean) => {
                        ui.set_search_status("Searching YouTube...".into());
                        search_receiver = Some(spawn_search(clean.clone(), helper.clone(), search_count));
                        active_search_query = Some(clean);
                    }
                    Err(error) => {
                        ui.set_search_status(error.message.into());
                        search_receiver = None;
                    }
                }
            }
        }

        if std::mem::take(&mut *more_search_requested.borrow_mut()) &&
            search_receiver.is_none() && search_count < SEARCH_MAX_RESULTS
        {
            if let Some(ref query) = active_search_query {
                search_count = (search_count + SEARCH_BATCH_SIZE).min(SEARCH_MAX_RESULTS);
                search_receiver = Some(spawn_search(query.clone(), helper.clone(), search_count));
                if let Some(ui) = label_window.upgrade() {
                    ui.set_has_more_results(false);
                    ui.set_search_status("Loading more results...".into());
                }
            }
        }

        if let Some(rx) = search_receiver.as_ref() {
            match rx.try_recv() {
                Ok(response) => {
                    search_receiver = None;
                    if let Some(ui) = label_window.upgrade() {
                        match response {
                            Ok(page) => {
                                let results = page.hits;
                                // ytsearchN re-fetches a longer prefix instead
                                // of returning a cursor. Append only unseen IDs
                                // to the existing model, preserving scroll
                                // position and already fetched thumbnails.
                                if search_rows.is_none() {
                                    let model = Rc::new(slint::VecModel::from(Vec::<SearchRow>::new()));
                                    ui.set_search_results(model.clone().into());
                                    search_rows = Some(model);
                                }
                                if let Some(ref model) = search_rows {
                                    let mut existing_ids = (0..model.row_count())
                                        .filter_map(|index| model.row_data(index))
                                        .map(|row| row.id.to_string())
                                        .collect::<std::collections::HashSet<_>>();
                                    for hit in &results {
                                        if !existing_ids.insert(hit.video_id.clone()) {
                                            continue;
                                        }
                                        let index = model.row_count();
                                        model.push(SearchRow {
                                            id: hit.video_id.clone().into(),
                                            title: hit.title.clone().into(),
                                            details: search_result_details(hit).into(),
                                            thumbnail: slint::Image::default(),
                                            has_thumbnail: false,
                                        });
                                        result_thumbnail_receivers.push((
                                            index, spawn_thumbnail(hit.video_id.clone()),
                                        ));
                                    }
                                }
                                let shown = search_rows.as_ref().map(|model| model.row_count())
                                    .unwrap_or(0);
                                let status = if shown == 0 {
                                    "No videos found.".to_string()
                                } else {
                                    format!("{shown} video results")
                                };
                                // Flat YouTube search can return fewer
                                // *usable* entries than requested. Allow one
                                // explicit expansion from the initial prefix
                                // even when the provider reports a short page.
                                // Subsequent batches respect provider exhaustion.
                                ui.set_has_more_results(
                                    page.has_more ||
                                    (search_count == SEARCH_BATCH_SIZE && !results.is_empty())
                                );
                                ui.set_search_status(status.into());
                            }
                            Err(error) => {
                                ui.set_search_status(format!("Search failed: {error}").into());
                                // Keep previous results when loading more fails.
                                ui.set_has_more_results(
                                    search_rows.is_some() && search_count < SEARCH_MAX_RESULTS
                                );
                            }
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    search_receiver = None;
                    if let Some(ui) = label_window.upgrade() {
                        ui.set_search_status("Search worker stopped. Retry.".into());
                    }
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        // Apply image updates to the current search model only. Workers
        // download/decode public thumbnails off the UI and video threads.
        let mut pending_images = Vec::new();
        for (index, receiver) in std::mem::take(&mut result_thumbnail_receivers) {
            match receiver.try_recv() {
                Ok(Some(pixels)) => {
                    if let Some(model) = search_rows.as_ref() {
                        if let Some(mut row) = model.row_data(index) {
                            row.thumbnail = slint::Image::from_rgb8(pixels);
                            row.has_thumbnail = true;
                            model.set_row_data(index, row);
                        }
                    }
                }
                Err(TryRecvError::Empty) => pending_images.push((index, receiver)),
                Ok(None) | Err(TryRecvError::Disconnected) => {}
            }
        }
        result_thumbnail_receivers = pending_images;

        // A new watch URL replaces the current playback session but does not
        // construct a new libmpv core. In-flight extraction results are dropped.
        if let Some(input) = requested_url.borrow_mut().take() {
            if startup_http_fixture.is_some() { eprintln!("[yt-lite] startup fixture ignores watch replacement"); return; }
            if let Some(bench) = seek_benchmark.as_mut() { bench.abort("video_replaced"); }
            match canonical_video_url(input.trim()) {
                Ok((_, canonical)) => {
                    let _ = label_player.borrow().send_strings(&["stop".into()]);
                    *pending_seek_timing.borrow_mut() = None;
                    search_receiver = None;
                    active_search_query = None;
                    search_count = SEARCH_BATCH_SIZE;
                    search_rows = None;
                    result_thumbnail_receivers.clear();
                    if let Some(ui) = label_window.upgrade() { ui.set_show_search(false); }
                    // An explicit user Open action should start the new video
                    // even if the previous video was left paused.
                    let _ = label_player.borrow().send_strings(&["set".into(), "pause".into(), "no".into()]);
                    active_provider = preferred_playback_provider(requested_provider);
                    receiver = Some(spawn_extraction(canonical.clone(), helper.clone(), active_provider, lean_metadata, native_ejs.clone(), profile_timings, fast_extraction, persistent.as_ref()));
                    pending = None;
                    resolving = true;
                    remote_url = Some(canonical.clone());
                    explicit_audio_open_failure = false;
                    startup_recovery = StartupRecovery::default();
                    recovery_started = None;
                    startup_complete = false;
                    startup_split = false;
                    audio_cache_attempted = false;
                    *browser_url.borrow_mut() = Some(canonical.clone());
                    clip_source.borrow_mut().take();
                    media_title = None;
                    thumbnail_receiver = None;
                    playback_error = None;
                    load_started = None;
                    file_loaded = false;
                    file_loaded_at = None;
                    playback_advanced = false;
                    has_network_media = false;
                    pending_external_audio = None;
                    preattach_retry = None;
                    media_load_dispatched = None;
                    fallback_muxed = None;
                    used_native_ejs = false;
                    used_fast_extraction = false;
                    audio_monitor = AudioContinuity::default();
                    audio_warning = false;
                    active_trace = None;
                    audio_started = None;
                    first_audio = false;
                    awaiting_seek_audio = None;
                    previous_wait_state = None;
                    if let Some(ui) = label_window.upgrade() {
                        ui.set_has_url(true);
                        ui.set_has_media(false);
                        ui.set_has_video_info(false);
                        ui.set_has_thumbnail(false);
                        ui.set_video_thumbnail(slint::Image::default());
                        ui.set_can_retry(false);
                        ui.set_url_text(canonical.into());
                        ui.set_fraction(0.0);
                    }
                }
                Err(error) => {
                    playback_error = Some(format!("{} {}", error.message, error.action));
                    if let Some(ui) = label_window.upgrade() { ui.set_can_retry(false); }
                }
            }
        }
        let mut failure: Option<String> = None;

        // Unlike the earlier prototype, consume the mpv event queue. An
        // accepted asynchronous loadfile command does NOT mean media loaded.
        for event in label_player.borrow().poll_events() {
            if let Some(bench) = seek_benchmark.as_mut() { bench.on_event(event); }
            if profile_timings {
                trace_event("mpv_event", format_args!("load={active_trace:?} seek={:?} kind={event:?}", pending_seek_timing.borrow().as_ref().map(|(id, _)| *id)));
            }
            match event {
                PlaybackEvent::Diagnostic("terminal_audio_open_failure") => {
                    if transport_recovery && recovery_started.is_some() && !startup_complete && preattach_retry.is_some() {
                        explicit_audio_open_failure = true;
                    }
                }
                PlaybackEvent::Diagnostic(_) | PlaybackEvent::Seek | PlaybackEvent::AudioReconfig | PlaybackEvent::VideoReconfig | PlaybackEvent::SeekAudioReloadReply(_) => {}
                PlaybackEvent::StartFile => {
                    // This may also fire when retrying; do not interpret as playback.
                }
                PlaybackEvent::PlaybackRestart => {
                    if profile_timings {
                        if let Some((id, start)) = pending_seek_timing.borrow_mut().take() {
                            let audio_pts = label_player.borrow().property_double(c"audio-pts");
                            trace_event("seek_restart_observed", format_args!("id={id} elapsed_ms={} audio_pts={audio_pts:?}", start.elapsed().as_millis()));
                            awaiting_seek_audio = Some((id, start));
                            eprintln!("[yt-lite] timing seek-to-playback-restart={}ms", start.elapsed().as_millis());
                        }
                    }
                }
                PlaybackEvent::FileLoaded => {
                    if remote_url.is_none() && clip_source.borrow().is_some() { file_loaded = true; }
                    let gated_startup = transport_recovery && recovery_started.is_some() && !startup_complete;
                    if !gated_startup && ((remote_url.is_none() && startup_http_fixture.is_none()) || load_started.is_some()) && label_window.upgrade().is_some() {
                        if let Some(ui) = label_window.upgrade() {
                            ui.set_has_media(true);
                        }
                    }
                    if let Some(start) = load_started.take() {
                        if profile_timings {
                            eprintln!("[yt-lite] timing libmpv-load={}ms", start.elapsed().as_millis());
                        }
                        file_loaded = true;
                        file_loaded_at = Some(Instant::now());
                        if let Some(audio_url) = pending_external_audio.take() {
                            // Add external audio after FILE_LOADED to avoid racing
                            // the asynchronous replace/loadfile operation.
                            audio_started = Some(Instant::now());
                            if profile_timings { trace_event("audio_add_requested", format_args!("load={active_trace:?}")); }
                            if let Err(err) = label_player.borrow().add_external_audio(&audio_url) {
                                failure.get_or_insert(err);
                            } else {
                                eprintln!("[yt-lite] loading separate audio track (incremental HTTP)");
                            }
                        }
                        if gated_startup { eprintln!("[yt-lite] media opened; waiting for synchronized A/V"); }
                        else { eprintln!("[yt-lite] {} media loaded; streaming", provider_label(active_provider)); }
                    }
                }
                PlaybackEvent::ExternalAudioAdded => {
                    if profile_timings { trace_event("audio_add_reply", format_args!("load={active_trace:?} elapsed_ms={:?}", audio_started.map(|t| t.elapsed().as_millis()))); }
                    eprintln!("[yt-lite] separate audio track accepted by libmpv");
                }
                PlaybackEvent::ExternalAudioFailed(code) => {
                    failure.get_or_insert_with(|| format!("Separate audio track failed to load (code {code})"));
                }
                PlaybackEvent::LoadFailed(code) => {
                    if load_started.is_some() || (has_network_media && file_loaded) {
                        failure.get_or_insert_with(|| format!("mpv media load failed (code {code})"));
                    } else if remote_url.is_none() && startup_http_fixture.is_none() {
                        playback_error = Some(format!("Local media failed to load (mpv code {code})"));
                        if let Some(bench) = seek_benchmark.as_mut() { bench.abort("playback_failure"); }
                    }
                }
                PlaybackEvent::Ended => {}
            }
        }

        if transport_recovery && recovery_started.is_some() && !startup_complete && file_loaded && failure.is_none() {
            let player = label_player.borrow();
            if startup_av_ready(player.property_double(c"time-pos"), player.property_double(c"audio-pts"),
                player.property_flag(c"seeking"), player.property_flag(c"paused-for-cache"),
                player.property_flag(c"pause"), player.property_double(c"audio-out-params/samplerate"))
                && recovery_started.is_some_and(|t| t.elapsed() < RECOVERY_BUDGET)
                && media_load_dispatched.is_some_and(|t| t.elapsed() < ATTEMPT_BUDGET) {
                startup_complete = true;
                if let Some(start) = recovery_started { trace_event("startup_recovery_ready", format_args!("elapsed_ms={}", start.elapsed().as_millis())); }
                if let Some(ui) = label_window.upgrade() { ui.set_has_media(true); }
                if startup_http_fixture.is_some() {
                    eprintln!("[yt-lite] startup-fixture done status=ready");
                }
            }
        }
        if transport_recovery && explicit_audio_open_failure && !startup_complete && preattach_retry.is_some() && file_loaded {
            let player = label_player.borrow();
            if missing_required_audio_after_open_failure(true, recovery_started.is_some(), file_loaded,
                player.property_double(c"audio-pts"), player.property_double(c"audio-out-params/samplerate")) {
                trace_event("startup_terminal_audio_failure", format_args!("load={active_trace:?} since_load_ms={:?}", media_load_dispatched.map(|t| t.elapsed().as_millis())));
                failure.get_or_insert_with(|| "Required external audio could not open".into());
            }
        }
        if transport_recovery && !startup_complete && recovery_started.is_some()
            && media_load_dispatched.is_some_and(|t| t.elapsed() >= ATTEMPT_BUDGET) {
            failure.get_or_insert_with(|| "Startup attempt did not establish synchronized A/V within 10 seconds".into());
        }
        if let Some(start) = load_started {
            let limit = if transport_recovery && !startup_complete { ATTEMPT_BUDGET } else { Duration::from_secs(30) };
            if start.elapsed() > limit {
                failure.get_or_insert_with(|| format!("mpv did not load the network stream in {} seconds", limit.as_secs()));
            }
        }

        // FILE_LOADED alone does not prove media decoding. If an allegedly
        // playable stream never advances, retry rather than showing 00:00.
        if let Some(start) = file_loaded_at {
            let position = label_player.borrow().property_double(c"time-pos").unwrap_or(0.0);
            if position > 0.1 { playback_advanced = true; }
            let pause_state = label_player.borrow().property_flag(c"pause");
            let manually_paused = pause_state.unwrap_or(false);
            let unpaused = if observed_state {
                pause_state == Some(false) || (pause_state.is_none() && start.elapsed() > Duration::from_secs(10))
            } else { !manually_paused };
            if preattach_retry.is_some() {
                if label_player.borrow().property_double(c"audio-pts").is_some() {
                    preattach_retry = None;
                    if profile_timings { trace_event("preattached_audio_ready", format_args!("load={active_trace:?} since_load_ms={:?}", media_load_dispatched.map(|t| t.elapsed().as_millis()))); }
                } else if unpaused && start.elapsed() > Duration::from_secs(5) {
                    failure.get_or_insert_with(|| "Preattached audio clock did not become available".into());
                }
            }
            if !playback_advanced && unpaused && start.elapsed() > Duration::from_secs(20) {
                failure.get_or_insert_with(|| "No decoded playback progress within 20 seconds".into());
            }
        }

        if let Some(rx) = receiver.as_ref() {
            match rx.try_recv() {
                Ok(response) => {
                    pending = Some(response);
                    receiver = None;
                }
                Err(TryRecvError::Disconnected) => {
                    receiver = None;
                    failure.get_or_insert_with(|| "Extractor worker exited without a result".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        // Late responses from a superseded video are discarded with its receiver.
        // Decode small JPEG/PNG thumbnails only after they reach the UI thread.
        if let Some(rx) = thumbnail_receiver.as_ref() {
            match rx.try_recv() {
                Ok(Some(pixels)) => {
                    if let Some(ui) = label_window.upgrade() {
                        ui.set_video_thumbnail(slint::Image::from_rgb8(pixels));
                        ui.set_has_thumbnail(true);
                    }
                    thumbnail_receiver = None;
                }
                Ok(None) | Err(TryRecvError::Disconnected) => thumbnail_receiver = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        if transport_recovery && !startup_complete && recovery_started.is_some_and(|t| t.elapsed() >= RECOVERY_BUDGET) {
            receiver = None;
            pending = None;
            failure = Some("Startup recovery budget exhausted".into());
        }
        if let Some(response) = if transport_recovery && failure.is_some() { None } else { pending.take() } {
            let ready = { label_player.borrow().is_render_ready() };
            if !ready {
                // Wait for Slint to establish the libmpv GL context.
                pending = Some(response);
            } else {
                resolving = false;
                match response {
                    Ok(stream) => {
                        active_trace = stream.trace;
                        audio_started = None;
                        first_audio = false;
                        awaiting_seek_audio = None;
                        previous_wait_state = None;
                        active_provider = stream.provider;
                        used_native_ejs = stream.used_native_ejs;
                        used_fast_extraction = stream.used_fast_extraction;
                        audio_monitor = AudioContinuity::default();
                        audio_warning = false;
                        next_audio_sample = Instant::now();
                        next_audio_log = Instant::now();
                        if profile_timings {
                            // Format IDs are short provider labels, not URLs.
                            let safe = |id: &str| -> String {
                                if id.len() <= 48 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.') {
                                    id.to_owned()
                                } else { "redacted".into() }
                            };
                            eprintln!("[yt-lite] format video={} audio={}", safe(&stream.video_format),
                                stream.audio_format.as_deref().map(safe).unwrap_or_else(|| "muxed".into()));
                        }
                        if profile_timings {
                            let label = if stream.used_persistent { "yt-dlp-persistent" } else if used_fast_extraction { "yt-dlp-fast" } else if used_native_ejs { "yt-dlp-native-ejs-requested" } else { provider_label(stream.provider) };
                            eprintln!("[yt-lite] timing {}-extract={}ms", label, stream.extraction_ms);
                        }
                        media_title = Some(stream.watch.title.clone());
                        if let Some(ui) = label_window.upgrade() {
                            ui.set_video_title(stream.watch.title.clone().into());
                            ui.set_video_subtitle(stream.watch.subtitle.clone().into());
                            ui.set_has_video_info(true);
                            ui.set_can_retry(false);
                        }
                        if stream.watch.thumbnail_url.is_some() {
                            // The watch metadata contains only a validated public ID.
                            if let Ok((id, _)) = canonical_video_url(remote_url.as_deref().unwrap_or("")) {
                                thumbnail_receiver = Some(spawn_thumbnail(id));
                            }
                        }
                        if let Some(height) = stream.selected_height {
                            let mode = if stream.audio_url.is_some() { "separate A/V" } else { "combined A/V" };
                            eprintln!("[yt-lite] selected {height}p {mode} for automatic quality");
                        }
                        eprintln!("[yt-lite] loading {} network stream (incremental HTTP)", provider_label(stream.provider));
                        fallback_muxed = stream.fallback_muxed;
                        preattach_retry = None;
                        explicit_audio_open_failure = false;
                        media_load_dispatched = Some(Instant::now());
                        if transport_recovery && !startup_complete {
                            recovery_started.get_or_insert_with(Instant::now);
                            startup_split = stream.audio_url.is_some();
                        }
                        if preattach_audio {
                            if let Some(audio) = stream.audio_url.as_ref() {
                                preattach_retry = Some((stream.url.clone(), audio.clone(), stream.headers.clone()));
                                audio_started = media_load_dispatched;
                            }
                        }
                        let preattached = preattach_retry.is_some();
                        if profile_timings { trace_event("load_requested", format_args!("load={active_trace:?} preattached={preattached}")); }
                        let result = if let Some((video, audio, headers)) = preattach_retry.as_ref() {
                            // Retry bookkeeping retains the original CDN URL, never the local endpoint.
                            if audio_cache && !audio_cache_attempted && stream.audio_direct_http {
                                audio_cache_attempted = true;
                                label_player.borrow_mut().load_network_with_cached_audio(video, audio, headers, active_trace.unwrap_or_else(trace_id))
                            } else {
                                if audio_cache { trace_event("audio_cache_skip", format_args!("load={active_trace:?} reason={}", if audio_cache_attempted { "already_attempted" } else { "unsupported_protocol" })); }
                                label_player.borrow_mut().load_network_with_audio(video, audio, headers)
                            }
                        } else {
                            label_player.borrow_mut().load_network_stream(&stream.url, &stream.headers)
                        };
                        match result {
                            Ok(()) => {
                                pending_external_audio = if preattached { None } else { stream.audio_url };
                                load_started = Some(Instant::now());
                                file_loaded = false;
                                file_loaded_at = None;
                                playback_advanced = false;
                                has_network_media = true;
                            }
                            Err(err) => failure = Some(err),
                        }
                    }
                    Err(err) => failure = Some(err),
                }
            }
        }

        if let Some(err) = failure {
            if let Some(bench) = seek_benchmark.as_mut() { bench.abort("playback_failure"); }
            // If high-resolution split A/V failed, use the already-extracted
            // combined stream before contacting another extractor. Never
            // persist or log signed CDN URLs.
            audio_monitor = AudioContinuity::default();
            audio_warning = false;
            audio_started = None;
            first_audio = false;
            awaiting_seek_audio = None;
            previous_wait_state = None;
            if profile_timings { trace_event("playback_failure", format_args!("load={active_trace:?}")); }
            if transport_recovery && !startup_complete && recovery_started.is_some() && (remote_url.is_some() || startup_http_fixture.is_some()) {
                let elapsed = recovery_started.unwrap().elapsed();
                explicit_audio_open_failure = false;
                let action = startup_recovery.next(elapsed, startup_split, fallback_muxed.is_some());
                trace_event("startup_recovery_action", format_args!("action={action:?} elapsed_ms={}", elapsed.as_millis()));
                let _ = label_player.borrow().send_strings(&["stop".into()]);
                receiver = None;
                pending = None;
                resolving = false;
                load_started = None;
                file_loaded = false;
                file_loaded_at = None;
                playback_advanced = false;
                has_network_media = false;
                pending_external_audio = None;
                preattach_retry = None;
                media_load_dispatched = None;
                used_native_ejs = false;
                used_fast_extraction = false;
                if let Some(ui) = label_window.upgrade() { ui.set_has_media(false); ui.set_can_retry(false); }
                match action {
                    RecoveryAction::Refresh => {
                        // Standard fresh extraction: do not reuse the failed split URLs.
                        // Keep the old muxed candidate until a fresh response replaces it.
                        receiver = if let Some(port) = startup_http_fixture {
                            Some(startup_fixture_resolution(port, 2))
                        } else {
                            let url = remote_url.as_ref().unwrap().clone();
                            Some(spawn_extraction(url, helper.clone(), Provider::YtDlp, false, None, profile_timings, false, None))
                        };
                        resolving = true;
                        playback_error = None;
                    }
                    RecoveryAction::Muxed => {
                        startup_split = false;
                        let (url, headers) = fallback_muxed.take().unwrap();
                        media_load_dispatched = Some(Instant::now());
                        match label_player.borrow_mut().load_network_stream(&url, &headers) {
                            Ok(()) => { load_started = media_load_dispatched; has_network_media = true; playback_error = None; }
                            Err(_) => { pending = Some(Err("Combined stream fallback failed".into())); resolving = true; }
                        }
                    }
                    RecoveryAction::Stop => {
                        recovery_started = None;
                        startup_complete = true; // terminal: don't re-arm on later ticks
                        fallback_muxed = None;
                        playback_error = Some("Startup recovery failed. Retry or open in Browser.".into());
                        if startup_http_fixture.is_some() { eprintln!("[yt-lite] startup-fixture done status=failed"); }
                        if let Some(ui) = label_window.upgrade() { ui.set_can_retry(remote_url.is_some()); }
                    }
                }
            } else if let Some((video, audio, headers)) = preattach_retry.take() {
                eprintln!("[yt-lite] preattached audio startup failed; retrying the same split streams serially");
                if profile_timings { trace_event("preattached_audio_fallback", format_args!("load={active_trace:?} since_load_ms={:?}", media_load_dispatched.map(|t| t.elapsed().as_millis()))); }
                let _ = label_player.borrow().send_strings(&["stop".into()]);
                media_load_dispatched = Some(Instant::now());
                if profile_timings { trace_event("load_requested", format_args!("load={active_trace:?} preattached=false retry=true")); }
                let result = label_player.borrow_mut().load_network_stream(&video, &headers);
                load_started = None;
                file_loaded = false;
                file_loaded_at = None;
                playback_advanced = false;
                has_network_media = false;
                pending_external_audio = None;
                if let Some(ui) = label_window.upgrade() { ui.set_has_media(false); }
                match result {
                    Ok(()) => {
                        load_started = media_load_dispatched;
                        pending_external_audio = Some(audio);
                        has_network_media = true;
                        playback_error = None;
                    }
                    Err(error) => {
                        pending = Some(Err(error));
                        resolving = true;
                    }
                }
            } else if let Some((url, headers)) = fallback_muxed.take() {
                eprintln!("[yt-lite] high-quality stream failed: {err}; retrying compatible combined A/V");
                pending_external_audio = None;
                let _ = label_player.borrow().send_strings(&["stop".into()]);
                if profile_timings { trace_event("muxed_fallback_requested", format_args!("load={active_trace:?}")); }
                media_load_dispatched = Some(Instant::now());
                match label_player.borrow_mut().load_network_stream(&url, &headers) {
                    Ok(()) => {
                        load_started = Some(Instant::now());
                        file_loaded = false;
                        file_loaded_at = None;
                        playback_advanced = false;
                        has_network_media = true;
                        playback_error = None;
                    }
                    Err(error) => {
                        let message = format!("Combined stream fallback failed: {error}");
                        load_started = None;
                        file_loaded = false;
                        has_network_media = false;
                        if used_native_ejs || used_fast_extraction {
                            // A synchronous load failure must also reach the
                            // ordinary-runtime retry, not bypass it.
                            pending = Some(Err(message));
                            resolving = true;
                        } else {
                            playback_error = Some(message);
                        }
                        if let Some(ui) = label_window.upgrade() {
                            ui.set_has_media(false);
                            ui.set_can_retry(!used_native_ejs && !used_fast_extraction && remote_url.is_some());
                        }
                    }
                }
            // Runtime failures (HTTP 403, expired signatures, etc.) are
            // distinct from extraction success; other provider is last resort.
            } else if (used_native_ejs || used_fast_extraction) && remote_url.is_some() {
                // Split/muxed candidates produced by the experimental solver
                // failed to open. Resolve once using the ordinary runtime.
                used_native_ejs = false;
                used_fast_extraction = false;
                let url = remote_url.as_ref().unwrap().clone();
                eprintln!("[yt-lite] experimental media failed; retrying standard yt-dlp");
                let _ = label_player.borrow().send_strings(&["stop".into()]);
                receiver = Some(spawn_extraction(url, helper.clone(), Provider::YtDlp, lean_metadata, None, profile_timings, false, None));
                pending = None;
                resolving = true;
                load_started = None;
                file_loaded = false;
                file_loaded_at = None;
                playback_advanced = false;
                has_network_media = false;
                pending_external_audio = None;
                preattach_retry = None;
                media_load_dispatched = None;
                fallback_muxed = None;
                if let Some(ui) = label_window.upgrade() {
                    ui.set_has_media(false);
                    ui.set_can_retry(false);
                }
            } else if let Some(next) = fallback_after_playback_error(active_provider, requested_provider) {
                if let Some(ref url) = remote_url {
                    eprintln!("[yt-lite] {} failed: {err}; retrying via {}", provider_label(active_provider), provider_label(next));
                    let _ = label_player.borrow().send_strings(&["stop".into()]);
                    receiver = Some(spawn_extraction(url.clone(), helper.clone(), next, lean_metadata, None, profile_timings, false, None));
                    active_provider = next;
                    pending = None;
                    resolving = true;
                    media_title = None;
                    thumbnail_receiver = None;
                    load_started = None;
                    file_loaded = false;
                    file_loaded_at = None;
                    playback_advanced = false;
                    has_network_media = false;
                    pending_external_audio = None;
                    preattach_retry = None;
                    media_load_dispatched = None;
                    fallback_muxed = None;
                    used_native_ejs = false;
                    used_fast_extraction = false;
                    if let Some(ui) = label_window.upgrade() {
                        ui.set_has_media(false);
                        ui.set_can_retry(false);
                        ui.set_has_video_info(false);
                        ui.set_has_thumbnail(false);
                    }
                } else {
                    playback_error = Some(err);
                }
            } else {
                eprintln!("[yt-lite] playback failure: {err}");
                playback_error = Some(err);
                resolving = false;
                load_started = None;
                file_loaded_at = None;
                pending_external_audio = None;
                if let Some(ui) = label_window.upgrade() {
                    ui.set_has_media(false);
                    ui.set_can_retry(remote_url.is_some());
                }
            }
        }

        if let Some(bench) = seek_benchmark.as_mut() {
            bench.tick(&label_player.borrow(), (has_network_media || clip_source.borrow().is_some()) && file_loaded && !resolving && playback_error.is_none());
            if bench.take_recovery_failure() {
                playback_error = Some("Seek recovery failed: required audio unavailable.".into());
                file_loaded = false;
                has_network_media = false;
                if let Some(ui) = label_window.upgrade() { ui.set_has_media(false); ui.set_can_retry(false); }
            }
        }

        // During startup/seek, sample at the UI tick; during
        // steady playback report transitions and sparse samples only.
        if profile_timings && has_network_media {
            let player = label_player.borrow();
            let seeking = player.property_flag(c"seeking");
            let buffering = player.property_flag(c"paused-for-cache");
            let paused = player.property_flag(c"pause");
            let audio_pts = player.property_double(c"audio-pts");
            let state = (seeking, buffering, paused, audio_pts.is_some());
            let changed = previous_wait_state != Some(state);
            if pending_seek_timing.borrow().is_some() { awaiting_seek_audio = None; }
            if let Some((id, start)) = awaiting_seek_audio {
                if audio_pts.is_some() && seeking == Some(false) {
                    trace_event("seek_audio_pts_observed", format_args!("id={id} elapsed_ms={} pts={audio_pts:?}", start.elapsed().as_millis()));
                    awaiting_seek_audio = None;
                } else if start.elapsed() > Duration::from_secs(30) {
                    trace_event("seek_audio_pts_unavailable", format_args!("id={id} elapsed_ms={}", start.elapsed().as_millis()));
                    awaiting_seek_audio = None;
                }
            }
            let waiting = load_started.is_some() || pending_seek_timing.borrow().is_some()
                || awaiting_seek_audio.is_some() || seeking == Some(true) || buffering == Some(true) || !first_audio;
            if !first_audio && audio_pts.is_some() && file_loaded {
                first_audio = true;
                trace_event("audio_pts_first_observed", format_args!("load={active_trace:?} since_load_ms={:?} since_add_ms={:?} pts={audio_pts:?}", media_load_dispatched.map(|t| t.elapsed().as_millis()), audio_started.map(|t| t.elapsed().as_millis())));
            }
            if changed || waiting || (audio_warning && Instant::now() >= next_audio_log) {
                trace_event("wait_state", format_args!("load={active_trace:?} seek={:?} pos={:?} audio_pts={audio_pts:?} seeking={seeking:?} buffering={buffering:?} paused={paused:?} cache_s={:?} cache_pct={:?} input_bytes_s={:?} avsync={:?}",
                    pending_seek_timing.borrow().as_ref().map(|(id, _)| *id),
                    player.property_double(c"time-pos"),
                    player.property_double(c"demuxer-cache-duration"),
                    player.property_double(c"cache-buffering-state"),
                    player.property_double(c"cache-speed"),
                    player.property_double(c"avsync")));
            }
            previous_wait_state = Some(state);
        }

        if let Some(ui) = label_window.upgrade() {
            if let Some(ref err) = playback_error {
                ui.set_status(format!("Open failed: {err}").into());
                return;
            }
            if remote_url.is_none() && clip_source.borrow().is_none() && startup_http_fixture.is_none() {
                ui.set_status("Paste a YouTube URL above and press Enter.".into());
                ui.set_fraction(0.0);
                return;
            }
            if resolving {
                ui.set_status(format!("Resolving {} media...", provider_label(active_provider)).into());
                return;
            }
            if load_started.is_some() {
                ui.set_status(format!("Connecting to {} stream (not downloading video)...", provider_label(active_provider)).into());
                return;
            }
            if transport_recovery && recovery_started.is_some() && !startup_complete {
                ui.set_status("Waiting for synchronized audio and video...".into());
                return;
            }
            let player = label_player.borrow();
            let position = player.property_double(c"time-pos");
            let length = player.property_double(c"duration");
            let elapsed = position.unwrap_or(0.0);
            let duration = length.unwrap_or(0.0);
            let fraction = if duration > 0.0 { (elapsed / duration).clamp(0.0, 1.0) } else { 0.0 };
            if !observed_state || (position.is_some() && length.is_some()) { ui.set_fraction(fraction as f32); }
            let title = media_title.as_deref().unwrap_or("Local video");
            let buffering = file_loaded && player.property_flag(c"paused-for-cache").unwrap_or(false);
            // Inspect clock/decoder/output state at most once per second,
            // outside the rendering callback. No raw mpv logs or signed URLs.
            if has_network_media && file_loaded && Instant::now() >= next_audio_sample {
                next_audio_sample = Instant::now() + Duration::from_secs(1);
                let audio_pts = player.property_double(c"audio-pts");
                let decoder_rate = player.property_double(c"audio-params/samplerate");
                let output_rate = player.property_double(c"audio-out-params/samplerate");
                let paused = player.property_flag(c"pause").unwrap_or(false);
                let seeking = player.property_flag(c"seeking").unwrap_or(false);
                let muted = player.property_flag(c"mute").unwrap_or(false);
                let eof = player.property_flag(c"eof-reached").unwrap_or(false);
                let near_end = duration > 0.0 && duration - elapsed < 3.0;
                let old_warning = audio_warning;
                let flags_known = !observed_state || [c"pause", c"seeking", c"paused-for-cache", c"mute", c"eof-reached"]
                    .iter().all(|name| player.property_flag(name).is_some());
                audio_warning = audio_monitor.sample(if observed_state { position } else { Some(elapsed) }, audio_pts,
                    flags_known && !paused && !seeking && !buffering && !muted && !eof && !near_end);
                if old_warning != audio_warning {
                    eprintln!("[yt-lite] audio clock {} at {:.1}s", if audio_warning { "stalled" } else { "warning cleared" }, elapsed);
                    ui.set_can_retry(audio_warning && remote_url.is_some());
                }
                if profile_timings && (old_warning != audio_warning || Instant::now() >= next_audio_log) {
                    next_audio_log = Instant::now() + Duration::from_secs(10);
                    eprintln!("[yt-lite] av-state pos={elapsed:.2} audio_pts={audio_pts:?} decoder_hz={decoder_rate:?} output_hz={output_rate:?} seeking={seeking} buffering={buffering} paused={paused} muted={muted} eof={eof}");
                }
            }
            let prefix = if buffering { "Buffering | " } else if audio_warning { "Audio clock stalled — Retry | " } else { "" };
            if !observed_state || (position.is_some() && length.is_some()) {
                ui.set_status(format!("{prefix}{title}  |  {} / {}", time_label(elapsed), time_label(duration)).into());
            }
            if let Some(paused) = player.property_flag(c"pause") { ui.set_paused(paused); }
            else if !observed_state { ui.set_paused(false); }
        }
    });

    window.run()?;
    drop(clock);
    drop(window);
    drop(player);
    Ok(())
}
