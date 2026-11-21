#![cfg(target_os = "windows")]
//! Standalone issue #2 prototype: one Slint window, one libmpv core, one GL render context.
//! Experimental local-file and YouTube URL playback, not a release-ready watch page.

mod mpv;

use mpv::{PlaybackEvent, Player};
use slint::{ComponentHandle, GraphicsAPI, RenderingState, Timer, TimerMode};
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::mpsc::{self, Receiver, TryRecvError}, time::{Duration, Instant}};
use yt_lite_extraction_proof::{canonical_video_url, extract, Provider};
use yt_lite_native_player_proof::{command_for, fallback_after_playback_error, select_muxed_stream, PlaybackAction};

slint::slint! {
    import { Button, HorizontalBox, VerticalBox } from "std-widgets.slint";

    export component PlayerWindow inherits Window {
        title: "yt-lite / native playback proof";
        preferred-width: 1000px;
        preferred-height: 640px;
        min-width: 560px;
        min-height: 300px;
        background: transparent;

        in-out property <bool> paused: false;
        in-out property <float> fraction: 0;
        in-out property <string> status: "Opening source...";
        callback toggle-pause();
        callback seek-by(int);
        callback seek-to(float);
        callback toggle-fullscreen();

        // The transparent window is deliberate: libmpv draws the GL underlay
        // before Slint paints these controls over it.
        Rectangle {
            width: parent.width;
            height: 106px;
            x: 0px;
            y: parent.height - self.height;
            background: #d51b202b;
            VerticalBox {
                padding: 10px;
                spacing: 9px;
                Text {
                    text: root.status;
                    color: white;
                    font-size: 13px;
                    vertical-alignment: center;
                }
                Rectangle {
                    height: 9px;
                    background: #677381;
                    border-radius: 4px;
                    Rectangle {
                        // Slint centers implicitly positioned children. Anchor the
                        // fill at the left edge so progress grows left-to-right.
                        x: 0px;
                        y: 0px;
                        width: parent.width * root.fraction;
                        height: parent.height;
                        background: #59baff;
                        border-radius: 4px;
                    }
                    TouchArea {
                        clicked => {
                            root.seek-to(self.mouse-x / self.width);
                        }
                    }
                }
                HorizontalBox {
                    spacing: 10px;
                    Button {
                        text: root.paused ? "Play" : "Pause";
                        clicked => { root.toggle-pause(); }
                    }
                    Button {
                        text: "-10 s";
                        clicked => { root.seek-by(-10); }
                    }
                    Button {
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
    title: String,
    provider: Provider,
}

type StreamResolution = Result<ResolvedStream, String>;

/// Fetch metadata on a worker and return one muxed HTTP stream for direct,
/// incremental libmpv playback. No media file is downloaded or logged.
fn spawn_extraction(video_url: String, helper: PathBuf, provider: Provider) -> Receiver<StreamResolution> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> StreamResolution {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "Could not initialize network extraction runtime".to_string())?;
            runtime.block_on(async {
                let info = extract(&video_url, provider, &helper).await
                    .map_err(|error| format!("{} {}", error.message, error.action))?;
                let stream = select_muxed_stream(&info).ok_or_else(|| {
                    "No combined audio+video HTTP stream available from this provider".to_string()
                })?;
                Ok(ResolvedStream {
                    url: stream.url.clone(),
                    title: info.title,
                    provider,
                })
            })
        })();
        let _ = sender.send(result);
    });
    receiver
}

fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Rust => "Rust",
        Provider::YtDlp => "yt-dlp",
        Provider::Auto => "auto",
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or(
        "Usage: yt-lite-native-player-proof <local-video-file | YouTube-video-URL> [--provider auto|rust|yt-dlp] [--yt-dlp PATH]"
    )?;
    let mut helper = PathBuf::from("yt-dlp");
    let mut requested_provider = Provider::Auto;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--yt-dlp" => {
                helper = PathBuf::from(args.next().ok_or("Missing --yt-dlp PATH")?);
            }
            "--provider" => {
                let selection = args.next().ok_or("Missing --provider VALUE")?;
                requested_provider = Provider::parse(&selection)
                    .ok_or("Provider must be auto, rust or yt-dlp")?;
            }
            _ => return Err(format!("Unknown option: {flag}").into()),
        }
    }
    let (clip, remote_url) = if input.starts_with("https://") || input.starts_with("http://") {
        let (_, canonical) = canonical_video_url(&input)
            .map_err(|error| format!("{} {}", error.message, error.action))?;
        (None, Some(canonical))
    } else {
        let path = PathBuf::from(input).canonicalize()?;
        if !path.is_file() { return Err("Local video path must point to a file".into()); }
        (Some(path), None)
    };
    let initial_provider = if requested_provider == Provider::Auto { Provider::Rust } else { requested_provider };
    let remote_receiver = remote_url.clone().map(|url| spawn_extraction(url, helper.clone(), initial_provider));
    // mpv's OpenGL proc resolver uses WGL; do not silently choose Direct3D or software.
    slint::BackendSelector::new().backend_name("winit".into())
        .renderer_name("femtovg".into()).require_opengl().select()?;
    let window = PlayerWindow::new()?;
    let player = Rc::new(RefCell::new(Player::open()?));

    let controls = player.clone();
    window.on_toggle_pause(move || {
        if let Some(command) = command_for(PlaybackAction::TogglePause) {
            if let Err(error) = controls.borrow().send_strings(&command) {
                eprintln!("Pause error: {error}");
            }
        }
    });

    let controls = player.clone();
    window.on_seek_by(move |seconds| {
        if let Some(command) = command_for(PlaybackAction::SeekRelative(seconds.into())) {
            if let Err(error) = controls.borrow().send_strings(&command) {
                eprintln!("Seek error: {error}");
            }
        }
    });

    let controls = player.clone();
    window.on_seek_to(move |fraction| {
        let duration = controls.borrow().property_double(c"duration").unwrap_or(0.0);
        if let Some(command) = command_for(PlaybackAction::SeekFraction {
            fraction: f64::from(fraction), duration_seconds: duration,
        }) {
            if let Err(error) = controls.borrow().send_strings(&command) {
                eprintln!("Scrub error: {error}");
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
    window.window().set_rendering_notifier(move |phase, api| {
        if !matches!(api, GraphicsAPI::NativeOpenGL { .. }) { return; }
        match phase {
            RenderingState::RenderingSetup => {
                if let Err(error) = video_session.borrow_mut().setup_render(video_window.clone(), clip.as_deref()) {
                    eprintln!("Video initialization error: {error}");
                }
            }
            RenderingState::BeforeRendering => {
                if let Some(ui) = video_window.upgrade() {
                    let size = ui.window().size();
                    video_session.borrow_mut().draw(size.width, size.height);
                }
            }
            RenderingState::RenderingTeardown => video_session.borrow_mut().teardown_render(),
            _ => {}
        }
    })?;

    let label_window = window.as_weak();
    let label_player = player.clone();
    let mut receiver = remote_receiver;
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
    let clock = Timer::default();
    clock.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        let mut failure: Option<String> = None;

        // Unlike the earlier prototype, consume the mpv event queue. An
        // accepted asynchronous loadfile command does NOT mean media loaded.
        for event in label_player.borrow().poll_events() {
            match event {
                PlaybackEvent::StartFile => {
                    // This may also fire when retrying; do not interpret as playback.
                }
                PlaybackEvent::FileLoaded => {
                    if load_started.is_some() {
                        load_started = None;
                        file_loaded = true;
                        file_loaded_at = Some(Instant::now());
                        eprintln!("[yt-lite] {} media loaded; streaming", provider_label(active_provider));
                    }
                }
                PlaybackEvent::LoadFailed(code) => {
                    if load_started.is_some() || (has_network_media && file_loaded) {
                        failure.get_or_insert_with(|| format!("mpv media load failed (code {code})"));
                    } else if remote_url.is_none() {
                        playback_error = Some(format!("Local media failed to load (mpv code {code})"));
                    }
                }
                PlaybackEvent::Ended => {}
            }
        }

        if let Some(start) = load_started {
            if start.elapsed() > Duration::from_secs(30) {
                failure.get_or_insert_with(|| "mpv did not load the network stream in 30 seconds".into());
            }
        }

        // FILE_LOADED alone does not prove media decoding. If an allegedly
        // playable stream never advances, retry rather than showing 00:00.
        if let Some(start) = file_loaded_at {
            let position = label_player.borrow().property_double(c"time-pos").unwrap_or(0.0);
            if position > 0.1 { playback_advanced = true; }
            let manually_paused = label_player.borrow().property_flag(c"pause").unwrap_or(false);
            if !playback_advanced && !manually_paused && start.elapsed() > Duration::from_secs(20) {
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

        if let Some(response) = pending.take() {
            let ready = { label_player.borrow().is_render_ready() };
            if !ready {
                // Wait for Slint to establish the libmpv GL context.
                pending = Some(response);
            } else {
                resolving = false;
                match response {
                    Ok(stream) => {
                        active_provider = stream.provider;
                        media_title = Some(stream.title);
                        eprintln!("[yt-lite] loading {} network stream (incremental HTTP)", provider_label(stream.provider));
                        match label_player.borrow_mut().load_network_stream(&stream.url) {
                            Ok(()) => {
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
            // Runtime failures (HTTP 403, expired signatures, etc.) are distinct
            // from extraction success. Try the other extractor exactly once.
            if let Some(next) = fallback_after_playback_error(active_provider, requested_provider) {
                if let Some(ref url) = remote_url {
                    eprintln!("[yt-lite] {} failed: {err}; retrying via {}", provider_label(active_provider), provider_label(next));
                    let _ = label_player.borrow().send_strings(&["stop".into()]);
                    receiver = Some(spawn_extraction(url.clone(), helper.clone(), next));
                    active_provider = next;
                    pending = None;
                    resolving = true;
                    media_title = None;
                    load_started = None;
                    file_loaded = false;
                    file_loaded_at = None;
                    playback_advanced = false;
                    has_network_media = false;
                } else {
                    playback_error = Some(err);
                }
            } else {
                eprintln!("[yt-lite] playback failure: {err}");
                playback_error = Some(err);
                resolving = false;
                load_started = None;
                file_loaded_at = None;
            }
        }

        if let Some(ui) = label_window.upgrade() {
            if let Some(ref err) = playback_error {
                ui.set_status(format!("Playback failed: {err}").into());
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
            let player = label_player.borrow();
            let elapsed = player.property_double(c"time-pos").unwrap_or(0.0);
            let duration = player.property_double(c"duration").unwrap_or(0.0);
            let fraction = if duration > 0.0 { (elapsed / duration).clamp(0.0, 1.0) } else { 0.0 };
            ui.set_fraction(fraction as f32);
            let title = media_title.as_deref().unwrap_or("Local video");
            let buffering = file_loaded && player.property_flag(c"paused-for-cache").unwrap_or(false);
            let prefix = if buffering { "Buffering | " } else { "" };
            ui.set_status(format!("{prefix}{title}  |  {} / {}", time_label(elapsed), time_label(duration)).into());
            ui.set_paused(player.property_flag(c"pause").unwrap_or(false));
        }
    });

    window.run()?;
    drop(clock);
    drop(window);
    drop(player);
    Ok(())
}
