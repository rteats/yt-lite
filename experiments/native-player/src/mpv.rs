//! Minimal dynamic libmpv bridge for Windows. The C declarations match mpv's
//! ISC-licensed public client.h, render.h and render_gl.h ABI.
//!
//! Keep this bridge isolated: it must not be reused for release packaging until
//! frame pacing, GL state preservation and hardware decode have been measured.

use crate::PlayerWindow;
use yt_lite_native_player_proof::{video_margin_ratios, NETWORK_STREAM_OPTIONS};
use yt_lite_extraction_proof::HttpPlaybackHeaders;
use libloading::Library;
use slint::{ComponentHandle, Weak};
use std::{
    ffi::{c_char, c_int, c_void, CStr, CString},
    path::Path,
    ptr,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
};

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct OpenGlInitParams {
    get_proc_address: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void>,
    get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
struct OpenGlFbo {
    fbo: c_int,
    width: c_int,
    height: c_int,
    internal_format: c_int,
}

type RenderUpdate = Option<unsafe extern "C" fn(*mut c_void)>;

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEndFile {
    reason: c_int,
    error: c_int,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackEvent {
    StartFile,
    FileLoaded,
    PlaybackRestart,
    LoadFailed(i32),
    ExternalAudioAdded,
    ExternalAudioFailed(i32),
    Ended,
}

// mpv/client.h: START_FILE=6, END_FILE=7, FILE_LOADED=8,
// COMMAND_REPLY=5; MPV_END_FILE_REASON_ERROR=4.
const MPV_EVENT_NONE: c_int = 0;
const MPV_EVENT_COMMAND_REPLY: c_int = 5;
const MPV_EVENT_START_FILE: c_int = 6;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_FILE_LOADED: c_int = 8;
const MPV_EVENT_PLAYBACK_RESTART: c_int = 21;
const MPV_END_FILE_REASON_ERROR: c_int = 4;
const LOADFILE_REPLY_ID: u64 = 0x594c_4f41_44;
const EXTERNAL_AUDIO_REPLY_ID: u64 = 0x5941_5544_494f;

#[derive(Clone, Copy)]
struct Api {
    create: unsafe extern "C" fn() -> *mut c_void,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut c_void),
    set_option_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    set_property_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    command_async: unsafe extern "C" fn(*mut c_void, u64, *const *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    render_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *const RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(*mut c_void),
    render_render: unsafe extern "C" fn(*mut c_void, *const RenderParam) -> c_int,
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    render_callback: unsafe extern "C" fn(*mut c_void, RenderUpdate, *mut c_void),
    wait_event: unsafe extern "C" fn(*mut c_void, f64) -> *const MpvEvent,
}

impl Api {
    unsafe fn load(library: &Library) -> Result<Self, String> {
        unsafe fn get<T: Copy>(library: &Library, name: &[u8]) -> Result<T, String> {
            library.get::<T>(name).map(|symbol| *symbol).map_err(|err| format!("libmpv symbol missing: {err}"))
        }
        Ok(Self {
            create: get(library, b"mpv_create\0")?,
            initialize: get(library, b"mpv_initialize\0")?,
            terminate_destroy: get(library, b"mpv_terminate_destroy\0")?,
            set_option_string: get(library, b"mpv_set_option_string\0")?,
            set_property_string: get(library, b"mpv_set_property_string\0")?,
            command_async: get(library, b"mpv_command_async\0")?,
            get_property: get(library, b"mpv_get_property\0")?,
            render_create: get(library, b"mpv_render_context_create\0")?,
            render_free: get(library, b"mpv_render_context_free\0")?,
            render_render: get(library, b"mpv_render_context_render\0")?,
            render_update: get(library, b"mpv_render_context_update\0")?,
            render_callback: get(library, b"mpv_render_context_set_update_callback\0")?,
            wait_event: get(library, b"mpv_wait_event\0")?,
        })
    }
}

struct Wake {
    window: Mutex<Weak<PlayerWindow>>,
    queued: Arc<AtomicBool>,
}

// Called by mpv from its own thread. Never access Slint or libmpv directly here.
unsafe extern "C" fn redraw_callback(ctx: *mut c_void) {
    if ctx.is_null() { return; }
    let wake = &*(ctx.cast::<Wake>());
    if wake.queued.swap(true, Ordering::AcqRel) { return; }
    let weak = match wake.window.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return,
    };
    let queued = wake.queued.clone();
    if weak.upgrade_in_event_loop(move |ui| {
        queued.store(false, Ordering::Release);
        ui.window().request_redraw();
    }).is_err() {
        wake.queued.store(false, Ordering::Release);
    }
}

#[link(name = "opengl32")]
extern "system" {
    fn wglGetProcAddress(name: *const c_char) -> *mut c_void;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleA(name: *const c_char) -> *mut c_void;
    fn GetProcAddress(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

// The desktop OpenGL backend is explicitly required by the Slint app.
// Some symbols are exported only from opengl32.dll, not wglGetProcAddress.
unsafe extern "C" fn gl_proc(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    if name.is_null() { return ptr::null_mut(); }
    let address = wglGetProcAddress(name);
    if !address.is_null() && (address as usize) > 3 && (address as isize) != -1 {
        return address;
    }
    let module = GetModuleHandleA(c"opengl32.dll".as_ptr());
    if module.is_null() { ptr::null_mut() } else { GetProcAddress(module, name) }
}

pub struct Player {
    api: Api,
    handle: *mut c_void,
    context: *mut c_void,
    wake: Option<Box<Wake>>,
    loaded: bool,
    last_video_margins: Option<(u64, u64)>,
    _lib: Library,
}

impl Player {
    pub fn open() -> Result<Self, String> {
        // Accept both names used by Windows libmpv builds, without searching PATH.
        let executable = std::env::current_exe().map_err(|err| err.to_string())?;
        let directory = executable.parent().ok_or("Missing executable directory")?;
        let dll = ["libmpv-2.dll", "mpv-2.dll"].into_iter()
            .map(|name| directory.join(name))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| format!(
                "libmpv not found in {}. Put the x64 libmpv-2.dll (or mpv-2.dll) from a trusted Windows mpv-dev build next to the executable.",
                directory.display()
            ))?;
        let lib = unsafe { Library::new(&dll) }.map_err(|err| format!(
            "Cannot load {}: {err}. Check that it is an x64 DLL and that its dependent DLLs are installed or provided alongside it.",
            dll.display()
        ))?;
        let api = unsafe { Api::load(&lib)? };
        let handle = unsafe { (api.create)() };
        if handle.is_null() { return Err("mpv_create failed".into()); }
        let player = Self { api, handle, context: ptr::null_mut(), wake: None, loaded: false, last_video_margins: None, _lib: lib };
        // Network HTTP media is streamed through libmpv with bounded memory,
        // no disk cache, and no full-file download before playback.
        for (key, value) in [
            ("config", "no"),
            ("vo", "libmpv"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("osc", "no"),
            ("hwdec", "auto-safe"),
        ].into_iter().chain(NETWORK_STREAM_OPTIONS.iter().copied()) {
            let key = CString::new(key).unwrap();
            let value = CString::new(value).unwrap();
            let status = unsafe { (player.api.set_option_string)(player.handle, key.as_ptr(), value.as_ptr()) };
            if status < 0 { return Err(format!("mpv rejected option {}", key.to_string_lossy())); }
        }
        let code = unsafe { (player.api.initialize)(player.handle) };
        if code < 0 { return Err(format!("mpv_initialize failed: {code}")); }
        Ok(player)
    }

    pub fn setup_render(&mut self, ui: Weak<PlayerWindow>, clip: Option<&Path>) -> Result<(), String> {
        if !self.context.is_null() { return Ok(()); }
        let mut ctx = ptr::null_mut();
        let init = OpenGlInitParams { get_proc_address: Some(gl_proc), get_proc_address_ctx: ptr::null_mut() };
        let params = [
            RenderParam { kind: 1, data: c"opengl".as_ptr() as *mut c_void },
            RenderParam { kind: 2, data: (&init as *const OpenGlInitParams).cast_mut().cast() },
            RenderParam { kind: 0, data: ptr::null_mut() },
        ];
        let status = unsafe { (self.api.render_create)(&mut ctx, self.handle, params.as_ptr()) };
        if status < 0 || ctx.is_null() { return Err(format!("mpv_render_context_create failed: {status}")); }
        self.context = ctx;
        let wake = Box::new(Wake { window: Mutex::new(ui), queued: Arc::new(AtomicBool::new(false)) });
        unsafe { (self.api.render_callback)(ctx, Some(redraw_callback), (&*wake as *const Wake).cast_mut().cast()) };
        self.wake = Some(wake);
        if !self.loaded {
            if let Some(clip) = clip {
                self.send_loadfile(clip.to_string_lossy().as_ref())?;
                self.loaded = true;
            }
        }
        Ok(())
    }

    pub fn is_render_ready(&self) -> bool {
        !self.context.is_null()
    }

    pub fn load_network_stream(
        &mut self,
        url: &str,
        headers: &HttpPlaybackHeaders,
    ) -> Result<(), String> {
        if !self.is_render_ready() {
            return Err("libmpv render context is not ready yet".into());
        }
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err("Extractor returned a non-HTTP streaming URL".into());
        }
        // yt-dlp can supply request-specific headers for signed URLs. Forward
        // only user agent and referer, never cookies or authorization tokens.
        if let Some(ref agent) = headers.user_agent {
            self.set_string_property(c"user-agent", agent)?;
        }
        if let Some(ref referer) = headers.referer {
            self.set_string_property(c"referrer", referer)?;
        }
        self.send_loadfile(url)?;
        self.loaded = true;
        Ok(())
    }

    fn set_string_property(&self, name: &CStr, value: &str) -> Result<(), String> {
        let cvalue = CString::new(value)
            .map_err(|_| "Extractor returned an invalid HTTP header".to_string())?;
        let rc = unsafe { (self.api.set_property_string)(self.handle, name.as_ptr(), cvalue.as_ptr()) };
        if rc < 0 {
            return Err(format!("mpv rejected HTTP request setting (code {rc})"));
        }
        Ok(())
    }

    /// Add a separate HTTP audio track after the video FILE_LOADED event.
    /// The two network streams share compatible User-Agent / Referer headers.
    /// This remains one mpv decoder session; no local download or muxing.
    pub fn add_external_audio(&self, url: &str) -> Result<(), String> {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err("Extractor returned a non-HTTP audio stream".into());
        }
        self.send_command(
            &["audio-add".into(), url.into(), "select".into()],
            EXTERNAL_AUDIO_REPLY_ID,
        )
    }

    /// mpv renders to the full GL framebuffer; reserve Slint chrome as
    /// margins so the *video picture* is confined to the visible center.
    /// Avoid setting properties on every frame; update only on geometry/DPI
    /// or when the video metadata row changes.
    pub fn update_video_margins(
        &mut self, height_px: u32, scale_factor: f32, has_metadata: bool,
    ) -> Result<(), String> {
        if height_px == 0 { return Ok(()); }
        let (top, bottom) = video_margin_ratios(height_px, scale_factor, has_metadata);
        let key = (top.to_bits(), bottom.to_bits());
        if self.last_video_margins == Some(key) { return Ok(()); }
        self.set_string_property(c"video-margin-ratio-top", &format!("{top:.7}"))?;
        self.set_string_property(c"video-margin-ratio-bottom", &format!("{bottom:.7}"))?;
        self.last_video_margins = Some(key);
        Ok(())
    }

    pub fn send_strings(&self, args: &[String]) -> Result<(), String> {
        self.send_command(args, 0)
    }

    fn send_loadfile(&self, target: &str) -> Result<(), String> {
        self.send_command(&["loadfile".into(), target.into(), "replace".into()], LOADFILE_REPLY_ID)
    }

    fn send_command(&self, args: &[String], reply: u64) -> Result<(), String> {
        let strings: Vec<_> = args.iter().map(|s| CString::new(s.as_str())
            .map_err(|_| "Command contains an embedded NUL".to_string())).collect::<Result<_, _>>()?;
        let mut pointers: Vec<*const c_char> = strings.iter().map(|s| s.as_ptr()).collect();
        pointers.push(ptr::null());
        let status = unsafe { (self.api.command_async)(self.handle, reply, pointers.as_ptr()) };
        if status < 0 { Err(format!("mpv_command_async rejected command: {status}")) } else { Ok(()) }
    }

    /// Nonblocking mpv event processing. Never include event payload filenames
    /// or streaming URLs in diagnostics; only expose numeric error codes.
    pub fn poll_events(&self) -> Vec<PlaybackEvent> {
        let mut events = Vec::new();
        for _ in 0..64 {
            let ptr = unsafe { (self.api.wait_event)(self.handle, 0.0) };
            if ptr.is_null() { break; }
            let event = unsafe { &*ptr };
            if event.event_id == MPV_EVENT_NONE { break; }
            match event.event_id {
                MPV_EVENT_START_FILE => events.push(PlaybackEvent::StartFile),
                MPV_EVENT_FILE_LOADED => events.push(PlaybackEvent::FileLoaded),
                MPV_EVENT_PLAYBACK_RESTART => events.push(PlaybackEvent::PlaybackRestart),
                MPV_EVENT_COMMAND_REPLY if event.reply_userdata == LOADFILE_REPLY_ID && event.error < 0 => {
                    events.push(PlaybackEvent::LoadFailed(event.error));
                }
                MPV_EVENT_COMMAND_REPLY if event.reply_userdata == EXTERNAL_AUDIO_REPLY_ID => {
                    if event.error < 0 {
                        events.push(PlaybackEvent::ExternalAudioFailed(event.error));
                    } else {
                        events.push(PlaybackEvent::ExternalAudioAdded);
                    }
                }
                MPV_EVENT_END_FILE if !event.data.is_null() => {
                    let end = unsafe { &*event.data.cast::<MpvEndFile>() };
                    if end.reason == MPV_END_FILE_REASON_ERROR {
                        events.push(PlaybackEvent::LoadFailed(end.error));
                    } else if end.reason == 0 {
                        events.push(PlaybackEvent::Ended);
                    }
                }
                _ => {}
            }
        }
        events
    }

    pub fn property_double(&self, name: &CStr) -> Option<f64> {
        let mut value = 0.0f64;
        let status = unsafe { (self.api.get_property)(self.handle, name.as_ptr(), 5, (&mut value as *mut f64).cast()) };
        (status >= 0 && value.is_finite()).then_some(value)
    }

    pub fn property_flag(&self, name: &CStr) -> Option<bool> {
        let mut value: c_int = 0;
        let status = unsafe { (self.api.get_property)(self.handle, name.as_ptr(), 3, (&mut value as *mut c_int).cast()) };
        (status >= 0).then_some(value != 0)
    }

    pub fn draw(&mut self, width: u32, height: u32) {
        if self.context.is_null() || width == 0 || height == 0 { return; }
        let fbo = OpenGlFbo { fbo: 0, width: width as c_int, height: height as c_int, internal_format: 0 };
        let flip: c_int = 1;
        let params = [
            RenderParam { kind: 3, data: (&fbo as *const OpenGlFbo).cast_mut().cast() },
            RenderParam { kind: 4, data: (&flip as *const c_int).cast_mut().cast() },
            RenderParam { kind: 0, data: ptr::null_mut() },
        ];
        unsafe {
            (self.api.render_update)(self.context);
            (self.api.render_render)(self.context, params.as_ptr());
        }
    }

    // Called on Slint RenderingTeardown while the render context is still valid.
    pub fn teardown_render(&mut self) {
        if !self.context.is_null() {
            unsafe {
                (self.api.render_callback)(self.context, None, ptr::null_mut());
                (self.api.render_free)(self.context);
            }
            self.context = ptr::null_mut();
            self.wake = None;
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if !self.context.is_null() {
            // If Slint did not send RenderingTeardown, free the context before the core.
            self.teardown_render();
        }
        unsafe { (self.api.terminate_destroy)(self.handle) };
    }
}
