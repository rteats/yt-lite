#![cfg(target_os = "windows")]
//! Standalone issue #2 prototype: one Slint window, one libmpv core, one GL render context.
//! Load a local test clip only. No YouTube extraction or distribution packaging here.

mod mpv;

use mpv::Player;
use slint::{ComponentHandle, GraphicsAPI, RenderingState, Timer, TimerMode};
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};
use yt_lite_native_player_proof::{command_for, PlaybackAction};

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
        in-out property <string> status: "Opening local clip...";
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clip = std::env::args_os().nth(1).map(PathBuf::from)
        .ok_or("Usage: yt-lite-native-player-proof <existing-local-video-file>")?;
    let clip = clip.canonicalize()?;
    if !clip.is_file() { return Err("Local video path must point to a file".into()); }
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
                if let Err(error) = video_session.borrow_mut().setup_render(video_window.clone(), &clip) {
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
    let clock = Timer::default();
    clock.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        if let Some(ui) = label_window.upgrade() {
            let player = label_player.borrow();
            let elapsed = player.property_double(c"time-pos").unwrap_or(0.0);
            let duration = player.property_double(c"duration").unwrap_or(0.0);
            let fraction = if duration > 0.0 { (elapsed / duration).clamp(0.0, 1.0) } else { 0.0 };
            ui.set_fraction(fraction as f32);
            ui.set_status(format!("{} / {}", time_label(elapsed), time_label(duration)).into());
            ui.set_paused(player.property_flag(c"pause").unwrap_or(false));
        }
    });

    window.run()?;
    drop(clock);
    drop(window);
    drop(player);
    Ok(())
}
