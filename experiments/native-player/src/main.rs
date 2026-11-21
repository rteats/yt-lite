#![cfg(target_os = "windows")]
//! Standalone issue #2 prototype: one Slint window, one libmpv core, one GL render context.
//! Experimental local-file and YouTube URL playback, not a release-ready watch page.

mod mpv;

use mpv::Player;
use slint::{ComponentHandle, GraphicsAPI, RenderingState, Timer, TimerMode};
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::mpsc::{self, Receiver, TryRecvError}, time::Duration};
use yt_lite_extraction_proof::{canonical_video_url, extract, Provider};
use yt_lite_native_player_proof::{command_for, select_muxed_stream, PlaybackAction};

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

type StreamResolution = Result<(String, String), String>;

/// Resolve network media off the UI thread, returning a single signed,
/// muxed (audio+video) stream. No URL is logged or shown in errors.
fn spawn_extraction(video_url: String, helper: PathBuf) -> Receiver<StreamResolution> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> StreamResolution {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "Could not initialize network extraction runtime".to_string())?;
            runtime.block_on(async {
                let primary = extract(&video_url, Provider::Rust, &helper).await;
                if let Ok(ref info) = primary {
                    if let Some(stream) = select_muxed_stream(info) {
                        return Ok((stream.url.clone(), info.title.clone()));
                    }
                }
                let fallback = extract(&video_url, Provider::YtDlp, &helper).await
                    .map_err(|error| format!("{} {}", error.message, error.action))?;
                let stream = select_muxed_stream(&fallback).ok_or_else(|| {
                    "No combined video+audio stream available; separate-track playback is not implemented in this proof".to_string()
                })?;
                Ok((stream.url.clone(), fallback.title.clone()))
            })
        })();
        let _ = sender.send(result);
    });
    receiver
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or(
        "Usage: yt-lite-native-player-proof <local-video-file | YouTube-video-URL> [--yt-dlp PATH]"
    )?;
    let mut helper = PathBuf::from("yt-dlp");
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--yt-dlp" => {
                helper = PathBuf::from(args.next().ok_or("Missing --yt-dlp PATH")?);
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
    let remote_receiver = remote_url.map(|url| spawn_extraction(url, helper));
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
    let mut playback_error: Option<String> = None;
    let mut active_title: Option<String> = None;
    let clock = Timer::default();
    clock.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        if let Some(rx) = receiver.as_ref() {
            match rx.try_recv() {
                Ok(response) => {
                    pending = Some(response);
                    receiver = None;
                }
                Err(TryRecvError::Disconnected) => {
                    receiver = None;
                    resolving = false;
                    playback_error = Some("Extractor worker exited without a result".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        if let Some(response) = pending.take() {
            if !label_player.borrow().is_render_ready() {
                // The Slint rendering notifier must establish mpv's GL context
                // before we hand its playback core a signed media URL.
                pending = Some(response);
            } else {
                resolving = false;
                match response {
                    Ok((stream_url, title)) => {
                        match label_player.borrow_mut().load_network_stream(&stream_url) {
                            Ok(()) => active_title = Some(title),
                            Err(error) => playback_error = Some(error),
                        }
                    }
                    Err(error) => playback_error = Some(error),
                }
            }
        }

        if let Some(ui) = label_window.upgrade() {
            if let Some(ref error) = playback_error {
                ui.set_status(format!("Playback setup failed: {error}").into());
                return;
            }
            if resolving {
                ui.set_status("Resolving YouTube media...".into());
                return;
            }
            let player = label_player.borrow();
            let elapsed = player.property_double(c"time-pos").unwrap_or(0.0);
            let duration = player.property_double(c"duration").unwrap_or(0.0);
            let fraction = if duration > 0.0 { (elapsed / duration).clamp(0.0, 1.0) } else { 0.0 };
            ui.set_fraction(fraction as f32);
            let title = active_title.as_deref().unwrap_or("Local video");
            ui.set_status(format!("{title}  |  {} / {}", time_label(elapsed), time_label(duration)).into());
            ui.set_paused(player.property_flag(c"pause").unwrap_or(false));
        }
    });

    window.run()?;
    drop(clock);
    drop(window);
    drop(player);
    Ok(())
}
