//! Minimal dynamic libmpv bridge for Windows. The C declarations match mpv's
//! ISC-licensed public client.h, render.h and render_gl.h ABI.
//!
//! Keep this bridge isolated: it must not be reused for release packaging until
//! frame pacing, GL state preservation and hardware decode have been measured.

use crate::PlayerWindow;
use yt_lite_native_player_proof::observed_state::{ObservedState, ObservedValue};
use yt_lite_native_player_proof::{diagnostic_stream_hint, preattached_audio_command, search_margin_ratio, video_margin_ratios, NETWORK_STREAM_OPTIONS};
use yt_lite_extraction_proof::HttpPlaybackHeaders;
use libloading::Library;
use slint::{ComponentHandle, Weak};
use std::{
    cell::{Cell, RefCell},
    ffi::{c_char, c_int, c_void, CStr, CString},
    path::Path,
    ptr,
    time::Instant,
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

// Public client.h ABI for reading the seekable-ranges node. Only numerical
// fields are copied; no strings, media filenames or URLs leave the bridge.
#[repr(C)]
union MpvNodeValue {
    double: f64,
    int64: i64,
    list: *mut MpvNodeList,
}

#[repr(C)]
struct MpvNode {
    value: MpvNodeValue,
    format: c_int,
}

#[repr(C)]
struct MpvNodeList {
    num: c_int,
    values: *mut MpvNode,
    keys: *mut *mut c_char,
}

impl MpvNode {
    // SAFETY: only called on a live mpv-owned node before free_node_contents.
    unsafe fn entries(&self, format: c_int) -> Option<(&MpvNodeList, &[MpvNode])> {
        if self.format != format { return None; }
        let list = self.value.list.as_ref()?;
        if !(0..=4096).contains(&list.num) { return None; }
        let values = if list.num == 0 {
            &[]
        } else {
            if list.values.is_null() { return None; }
            std::slice::from_raw_parts(list.values, list.num as usize)
        };
        Some((list, values))
    }

    unsafe fn field(&self, name: &[u8]) -> Option<&MpvNode> {
        let (list, values) = self.entries(8)?; // MPV_FORMAT_NODE_MAP
        if list.keys.is_null() { return None; }
        for (i, value) in values.iter().enumerate() {
            let key = *list.keys.add(i);
            if !key.is_null() && CStr::from_ptr(key).to_bytes() == name {
                return Some(value);
            }
        }
        None
    }

    unsafe fn finite_double(&self) -> Option<f64> {
        if self.format != 5 { return None; } // MPV_FORMAT_DOUBLE
        let n = self.value.double;
        n.is_finite().then_some(n)
    }
}


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
    Seek,
    AudioReconfig,
    VideoReconfig,
    Diagnostic(&'static str),
}

// mpv/client.h: START_FILE=6, END_FILE=7, FILE_LOADED=8,
// COMMAND_REPLY=5; MPV_END_FILE_REASON_ERROR=4.
const MPV_EVENT_NONE: c_int = 0;
const MPV_EVENT_LOG_MESSAGE: c_int = 2;
const MPV_EVENT_VIDEO_RECONFIG: c_int = 17;
const MPV_EVENT_AUDIO_RECONFIG: c_int = 18;
const MPV_EVENT_SEEK: c_int = 20;
const MPV_EVENT_PROPERTY_CHANGE: c_int = 22;
const MPV_EVENT_QUEUE_OVERFLOW: c_int = 24;
const MPV_FORMAT_NONE: c_int = 0;
const MPV_FORMAT_FLAG: c_int = 3;
const MPV_FORMAT_DOUBLE: c_int = 5;
const MPV_FORMAT_NODE: c_int = 6;

#[repr(C)]
struct MpvEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

const OBSERVED_PROPERTIES: &[(&str, &CStr, c_int)] = &[
    ("time-pos", c"time-pos", MPV_FORMAT_DOUBLE),
    ("duration", c"duration", MPV_FORMAT_DOUBLE),
    ("audio-pts", c"audio-pts", MPV_FORMAT_DOUBLE),
    ("pause", c"pause", MPV_FORMAT_FLAG),
    ("seeking", c"seeking", MPV_FORMAT_FLAG),
    ("paused-for-cache", c"paused-for-cache", MPV_FORMAT_FLAG),
    ("demuxer-cache-duration", c"demuxer-cache-duration", MPV_FORMAT_DOUBLE),
    ("cache-buffering-state", c"cache-buffering-state", MPV_FORMAT_DOUBLE),
    ("cache-speed", c"cache-speed", MPV_FORMAT_DOUBLE),
    ("avsync", c"avsync", MPV_FORMAT_DOUBLE),
    ("audio-params/samplerate", c"audio-params/samplerate", MPV_FORMAT_DOUBLE),
    ("audio-out-params/samplerate", c"audio-out-params/samplerate", MPV_FORMAT_DOUBLE),
    ("mute", c"mute", MPV_FORMAT_FLAG),
    ("eof-reached", c"eof-reached", MPV_FORMAT_FLAG),
    ("demuxer-cache-state", c"demuxer-cache-state", MPV_FORMAT_NODE),
];

#[repr(C)]
struct MpvLogMessage {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

// Classify only; do not print any raw message (may contain signed URLs).
fn safe_mpv_log(text: &str) -> Option<&'static str> {
    let s = text.to_ascii_lowercase();
    if s.contains("timed out") || s.contains("timeout") { Some("network_timeout") }
    else if s.contains("http error") || s.contains("http response") { Some("http_error_or_response") }
    else if s.contains("reconnect") { Some("reconnect") }
    else if s.contains("end of file") || s.contains("eof") { Some("eof_marker") }
    else if s.contains("seek") && (s.contains("fail") || s.contains("error")) { Some("seek_error") }
    else if s.contains("seek") { Some("seek_marker") }
    else if s.contains("audio") && (s.contains("fail") || s.contains("error")) { Some("audio_error") }
    else if s.contains("buffer") || s.contains("cache") { Some("cache_marker") }
    else if s.contains("error") || s.contains("failed") { Some("other_error") }
    else { None }
}
const MPV_EVENT_COMMAND_REPLY: c_int = 5;
const MPV_EVENT_START_FILE: c_int = 6;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_FILE_LOADED: c_int = 8;
const MPV_EVENT_PLAYBACK_RESTART: c_int = 21;
const MPV_END_FILE_REASON_ERROR: c_int = 4;
const FIRST_REPLY_ID: u64 = 1;

#[derive(Clone, Copy)]
struct Api {
    create: unsafe extern "C" fn() -> *mut c_void,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut c_void),
    set_option_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    set_property_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    command_async: unsafe extern "C" fn(*mut c_void, u64, *const *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    free_node_contents: unsafe extern "C" fn(*mut MpvNode),
    observe_property: unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int,
    unobserve_property: unsafe extern "C" fn(*mut c_void, u64) -> c_int,
    render_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *const RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(*mut c_void),
    render_render: unsafe extern "C" fn(*mut c_void, *const RenderParam) -> c_int,
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    render_callback: unsafe extern "C" fn(*mut c_void, RenderUpdate, *mut c_void),
    wait_event: unsafe extern "C" fn(*mut c_void, f64) -> *const MpvEvent,
    request_log_messages: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
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
            free_node_contents: get(library, b"mpv_free_node_contents\0")?,
            observe_property: get(library, b"mpv_observe_property\0")?,
            unobserve_property: get(library, b"mpv_unobserve_property\0")?,
            render_create: get(library, b"mpv_render_context_create\0")?,
            render_free: get(library, b"mpv_render_context_free\0")?,
            render_render: get(library, b"mpv_render_context_render\0")?,
            render_update: get(library, b"mpv_render_context_update\0")?,
            render_callback: get(library, b"mpv_render_context_set_update_callback\0")?,
            wait_event: get(library, b"mpv_wait_event\0")?,
            request_log_messages: get(library, b"mpv_request_log_messages\0")?,
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
    profile_calls: Cell<bool>,
    observed: RefCell<Option<ObservedState>>,
    observed_file_loaded: Cell<bool>,
    next_reply: Cell<u64>,
    pending_load_reply: Cell<Option<u64>>,
    pending_audio_reply: Cell<Option<u64>>,
    last_video_margins: Option<(u64, u64, u64)>,
    _lib: Library,
}

impl Player {
    pub fn open() -> Result<Self, String> { Self::open_with_cache(false) }

    pub fn open_with_cache(expanded: bool) -> Result<Self, String> {
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
        let player = Self {
            api,
            handle,
            context: ptr::null_mut(),
            wake: None,
            loaded: false,
            profile_calls: Cell::new(false),
            observed: RefCell::new(None),
            observed_file_loaded: Cell::new(false),
            next_reply: Cell::new(FIRST_REPLY_ID),
            pending_load_reply: Cell::new(None),
            pending_audio_reply: Cell::new(None),
            last_video_margins: None,
            _lib: lib,
        };
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
            // Isolate retained cache capacity; do not change forward time target.
            let value = if expanded { match key {
                "demuxer-max-bytes" => "64MiB",
                "demuxer-max-back-bytes" => "32MiB",
                _ => value,
            } } else { value };
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

    /// Diagnostic-only local split fixture, same mpv per-file audio option.
    pub fn load_local_split(&mut self, video: &Path, audio: &Path) -> Result<(), String> {
        if !self.is_render_ready() { return Err("render context not ready".into()); }
        let video = video.to_str().ok_or("fixture path is not UTF-8")?;
        let audio = audio.to_str().ok_or("fixture path is not UTF-8")?;
        self.send_load_command(&["loadfile".into(), video.into(), "replace".into(), "-1".into(),
            format!("audio-files-append=%{}%{}", audio.len(), audio)])?;
        self.loaded = true;
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
        self.load_network_media(url, None, headers)
    }

    /// Experimental: ask mpv to attach the audio during the initial load.
    /// Per-file options are restored by mpv when the file is replaced.
    pub fn load_network_with_audio(&mut self, url: &str, audio: &str, headers: &HttpPlaybackHeaders) -> Result<(), String> {
        self.load_network_media(url, Some(audio), headers)
    }

    fn load_network_media(&mut self, url: &str, audio: Option<&str>, headers: &HttpPlaybackHeaders) -> Result<(), String> {
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
        if let Some(audio) = audio {
            let args = preattached_audio_command(url, audio).ok_or("Invalid split HTTP media URL")?;
            self.send_load_command(&args)?;
        } else {
            self.send_loadfile(url)?;
        }
        self.loaded = true;
        Ok(())
    }

    fn set_string_property(&self, name: &CStr, value: &str) -> Result<(), String> {
        let cvalue = CString::new(value)
            .map_err(|_| "Extractor returned an invalid HTTP header".to_string())?;
        let started = self.profile_calls.get().then(Instant::now);
        let rc = unsafe { (self.api.set_property_string)(self.handle, name.as_ptr(), cvalue.as_ptr()) };
        self.report_slow_call(started, "set_property", name);
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
        let reply = self.allocate_reply();
        self.send_command(&["audio-add".into(), url.into(), "select".into()], reply)?;
        self.pending_audio_reply.set(Some(reply));
        Ok(())
    }

    /// mpv renders to the full GL framebuffer; reserve Slint chrome as
    /// margins so the *video picture* is confined to the visible center.
    /// Avoid setting properties on every frame; update only on geometry/DPI
    /// or when the video metadata row changes.
    pub fn update_video_margins(
        &mut self, width_px: u32, height_px: u32, scale_factor: f32,
        has_metadata: bool, show_search: bool,
    ) -> Result<(), String> {
        if width_px == 0 || height_px == 0 { return Ok(()); }
        let (top, bottom) = video_margin_ratios(height_px, scale_factor, has_metadata);
        let right = search_margin_ratio(width_px, scale_factor, show_search);
        let key = (top.to_bits(), bottom.to_bits(), right.to_bits());
        if self.last_video_margins == Some(key) { return Ok(()); }
        self.set_string_property(c"video-margin-ratio-top", &format!("{top:.7}"))?;
        self.set_string_property(c"video-margin-ratio-bottom", &format!("{bottom:.7}"))?;
        self.set_string_property(c"video-margin-ratio-right", &format!("{right:.7}"))?;
        self.last_video_margins = Some(key);
        Ok(())
    }

    pub fn send_strings(&self, args: &[String]) -> Result<(), String> {
        self.send_command(args, 0)
    }

    fn send_loadfile(&self, target: &str) -> Result<(), String> {
        self.send_load_command(&["loadfile".into(), target.into(), "replace".into()])
    }

    fn allocate_reply(&self) -> u64 {
        let id = self.next_reply.get();
        self.next_reply.set(id.wrapping_add(1).max(FIRST_REPLY_ID));
        id
    }

    fn send_load_command(&self, args: &[String]) -> Result<(), String> {
        let reply = self.allocate_reply();
        self.send_command(args, reply)?;
        self.pending_load_reply.set(Some(reply));
        Ok(())
    }

    fn send_command(&self, args: &[String], reply: u64) -> Result<(), String> {
        let strings: Vec<_> = args.iter().map(|s| CString::new(s.as_str())
            .map_err(|_| "Command contains an embedded NUL".to_string())).collect::<Result<_, _>>()?;
        let mut pointers: Vec<*const c_char> = strings.iter().map(|s| s.as_ptr()).collect();
        pointers.push(ptr::null());
        let status = unsafe { (self.api.command_async)(self.handle, reply, pointers.as_ptr()) };
        if status < 0 {
            return Err(format!("mpv_command_async rejected command: {status}"));
        }
        // An audio-add reply can arrive after the playing file is replaced.
        if matches!(args.first().map(String::as_str), Some("stop" | "loadfile")) {
            self.pending_audio_reply.set(None);
            self.pending_load_reply.set(None);
            self.observed_file_loaded.set(false);
            self.reset_observations(false);
        }
        Ok(())
    }

    /// Nonblocking mpv event processing. Never include event payload filenames
    /// or streaming URLs in diagnostics; only expose numeric error codes.
    pub fn poll_events(&self) -> Vec<PlaybackEvent> {
        let started = self.profile_calls.get().then(Instant::now);
        let mut events = Vec::new();
        // Property events are coalesced behind queued lifecycle/log events.
        // Bound work while allowing the observed mode to drain a larger burst.
        let budget = if self.observed.borrow().is_some() { 256 } else { 64 };
        for i in 0..budget {
            let ptr = unsafe { (self.api.wait_event)(self.handle, 0.0) };
            if ptr.is_null() { break; }
            let event = unsafe { &*ptr };
            if event.event_id == MPV_EVENT_NONE { break; }
            if i + 1 == budget && self.profile_calls.get() {
                yt_lite_extraction_proof::trace_event("mpv_event_budget", format_args!("events={budget}"));
            }
            match event.event_id {
                MPV_EVENT_PROPERTY_CHANGE if !event.data.is_null() => {
                    self.apply_property_event(event.reply_userdata, unsafe { &*event.data.cast::<MpvEventProperty>() });
                }
                MPV_EVENT_QUEUE_OVERFLOW => {
                    if self.profile_calls.get() { yt_lite_extraction_proof::trace_event("mpv_queue_overflow", format_args!("observed={}", self.observed.borrow().is_some())); }
                    self.reset_observations(self.observed_file_loaded.get());
                }
                MPV_EVENT_LOG_MESSAGE if !event.data.is_null() => {
                    let msg = unsafe { &*event.data.cast::<MpvLogMessage>() };
                    if !msg.text.is_null() {
                        let text = unsafe { CStr::from_ptr(msg.text) }.to_string_lossy();
                        if let Some(kind) = safe_mpv_log(&text) {
                            if self.profile_calls.get() {
                                let prefix = if msg.prefix.is_null() { "".into() } else { unsafe { CStr::from_ptr(msg.prefix) }.to_string_lossy() };
                                let module = if prefix.contains("ffmpeg") || prefix.contains("lavf") { "ffmpeg" }
                                    else if prefix.contains("demux") { "demux" }
                                    else if prefix.contains("audio") || prefix.contains("ao/") { "audio" }
                                    else if prefix.contains("video") || prefix.contains("vd/") { "video" }
                                    else { "other" };
                                yt_lite_extraction_proof::trace_event("mpv_diagnostic", format_args!("module={module} stream_hint={} kind={kind}", diagnostic_stream_hint(&text)));
                            }
                            events.push(PlaybackEvent::Diagnostic(kind));
                        }
                    }
                }
                MPV_EVENT_SEEK => {
                    if self.observed_file_loaded.get() { self.reset_observations(true); }
                    events.push(PlaybackEvent::Seek);
                }
                MPV_EVENT_AUDIO_RECONFIG => events.push(PlaybackEvent::AudioReconfig),
                MPV_EVENT_VIDEO_RECONFIG => events.push(PlaybackEvent::VideoReconfig),
                MPV_EVENT_START_FILE => {
                    self.observed_file_loaded.set(false);
                    self.reset_observations(false);
                    events.push(PlaybackEvent::StartFile);
                }
                MPV_EVENT_FILE_LOADED => {
                    self.observed_file_loaded.set(true);
                    self.reset_observations(true);
                    events.push(PlaybackEvent::FileLoaded);
                }
                MPV_EVENT_PLAYBACK_RESTART => {
                    if self.observed_file_loaded.get() { self.reset_observations(true); }
                    events.push(PlaybackEvent::PlaybackRestart);
                }
                MPV_EVENT_COMMAND_REPLY if self.pending_load_reply.get() == Some(event.reply_userdata) => {
                    self.pending_load_reply.set(None);
                    if event.error < 0 { events.push(PlaybackEvent::LoadFailed(event.error)); }
                }
                MPV_EVENT_COMMAND_REPLY if self.pending_audio_reply.get() == Some(event.reply_userdata) => {
                    self.pending_audio_reply.set(None);
                    if event.error < 0 {
                        events.push(PlaybackEvent::ExternalAudioFailed(event.error));
                    } else {
                        events.push(PlaybackEvent::ExternalAudioAdded);
                    }
                }
                MPV_EVENT_END_FILE if !event.data.is_null() => {
                    let end = unsafe { &*event.data.cast::<MpvEndFile>() };
                    self.observed_file_loaded.set(false);
                    self.reset_observations(false);
                    if end.reason == MPV_END_FILE_REASON_ERROR {
                        events.push(PlaybackEvent::LoadFailed(end.error));
                    } else if end.reason == 0 {
                        events.push(PlaybackEvent::Ended);
                    }
                }
                _ => {}
            }
        }
        self.report_slow_call(started, "poll_events", c"event-queue");
        events
    }

    pub fn enable_observed_state(&self) {
        *self.observed.borrow_mut() = Some(ObservedState::default());
        eprintln!("[yt-lite] experimental observed playback state enabled");
    }

    fn reset_observations(&self, active: bool) {
        let mut observed = self.observed.borrow_mut();
        let Some(state) = observed.as_mut() else { return; };
        let started = self.profile_calls.get().then(Instant::now);
        if let Some(old) = state.active_id() {
            unsafe { (self.api.unobserve_property)(self.handle, old); }
        }
        if let Some(id) = state.reset(active) {
            for &(name, c_name, format) in OBSERVED_PROPERTIES {
                let rc = unsafe { (self.api.observe_property)(self.handle, id, c_name.as_ptr(), format) };
                if rc < 0 { eprintln!("[yt-lite] observe unavailable property={name} code={rc}"); }
            }
        }
        self.report_slow_call(started, "observe_reset", c"playback-state");
    }

    fn apply_property_event(&self, id: u64, property: &MpvEventProperty) {
        let mut observed = self.observed.borrow_mut();
        let Some(state) = observed.as_mut() else { return; };
        if state.active_id() != Some(id) || property.name.is_null() { return; }
        let name = unsafe { CStr::from_ptr(property.name) };
        let Some(&(key, _, expected)) = OBSERVED_PROPERTIES.iter().find(|entry| entry.1 == name) else { return; };
        // mpv owns event data until the next wait_event. Copy scalars and
        // only numerical cache ranges before returning; never free event data.
        let value = if property.format == MPV_FORMAT_NONE || property.data.is_null() || property.format != expected {
            None
        } else { unsafe {
            match expected {
                MPV_FORMAT_DOUBLE => Some(ObservedValue::Double(*property.data.cast::<f64>())),
                MPV_FORMAT_FLAG => Some(ObservedValue::Flag(*property.data.cast::<c_int>() != 0)),
                MPV_FORMAT_NODE => Self::copy_cache_ranges(&*property.data.cast::<MpvNode>()).map(ObservedValue::Ranges),
                _ => None,
            }
        } };
        state.update(id, key, value);
    }

    pub fn enable_diagnostics(&self) {
        self.profile_calls.set(true);
        let rc = unsafe { (self.api.request_log_messages)(self.handle, c"v".as_ptr()) };
        if rc < 0 { eprintln!("[yt-lite] mpv verbose diagnostics unavailable code={rc}"); }
    }

    /// Main demuxer only, as documented by mpv. Missing/invalid nodes are
    /// unknown, not evidence of a cache miss. Read only before benchmark seeks.
    pub fn main_cache_ranges(&self) -> Option<Vec<(f64, f64)>> {
        if let Some(state) = self.observed.borrow().as_ref() { return state.ranges(); }
        let mut node = MpvNode { value: MpvNodeValue { int64: 0 }, format: 0 };
        unsafe {
            let started = self.profile_calls.get().then(Instant::now);
            let rc = (self.api.get_property)(self.handle, c"demuxer-cache-state".as_ptr(), 6,
                (&mut node as *mut MpvNode).cast());
            self.report_slow_call(started, "get_node", c"demuxer-cache-state");
            if rc < 0 { return None; }
            let result = Self::copy_cache_ranges(&node);
            (self.api.free_node_contents)(&mut node);
            result
        }
    }

    unsafe fn copy_cache_ranges(node: &MpvNode) -> Option<Vec<(f64, f64)>> {
        let (_, entries) = node.field(b"seekable-ranges")?.entries(7)?;
        entries.iter().map(|entry| {
            let start = entry.field(b"start")?.finite_double()?;
            let end = entry.field(b"end")?.finite_double()?;
            (start <= end).then_some((start, end))
        }).collect::<Option<Vec<_>>>()
    }

    fn report_slow_call(&self, started: Option<Instant>, operation: &str, name: &CStr) {
        if let Some(started) = started {
            let us = started.elapsed().as_micros();
            if us >= 10_000 {
                // Names are compile-time property names from the bridge, not values.
                yt_lite_extraction_proof::trace_event("mpv_slow_call",
                    format_args!("op={operation} property={} elapsed_us={us}", name.to_string_lossy()));
            }
        }
    }

    pub fn property_double(&self, name: &CStr) -> Option<f64> {
        if let Some(state) = self.observed.borrow().as_ref() { return state.double(name.to_str().ok()?); }
        let mut value = 0.0f64;
        let started = self.profile_calls.get().then(Instant::now);
        let status = unsafe { (self.api.get_property)(self.handle, name.as_ptr(), 5, (&mut value as *mut f64).cast()) };
        self.report_slow_call(started, "get_double", name);
        (status >= 0 && value.is_finite()).then_some(value)
    }

    pub fn property_flag(&self, name: &CStr) -> Option<bool> {
        if let Some(state) = self.observed.borrow().as_ref() { return state.flag(name.to_str().ok()?); }
        let mut value: c_int = 0;
        let started = self.profile_calls.get().then(Instant::now);
        let status = unsafe { (self.api.get_property)(self.handle, name.as_ptr(), 3, (&mut value as *mut c_int).cast()) };
        self.report_slow_call(started, "get_flag", name);
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

#[cfg(test)]
mod tests {
    use super::{PlaybackEvent, Player};
    use std::{thread, time::{Duration, Instant}};

    #[test]
    #[ignore = "requires mpv-2.dll or libmpv-2.dll alongside the test executable"]
    fn stopping_playback_discards_only_the_superseded_audio_reply() {
        let player = Player::open().unwrap();
        // With no file loaded, each audio command produces a failure reply.
        let url = "http://127.0.0.1:1/unused";
        player.add_external_audio(url).unwrap();
        player.send_strings(&["stop".into()]).unwrap();
        player.add_external_audio(url).unwrap();

        let deadline = Instant::now() + Duration::from_secs(1);
        let mut failures = 0;
        while Instant::now() < deadline {
            failures += player.poll_events().into_iter()
                .filter(|event| matches!(event, PlaybackEvent::ExternalAudioFailed(_)))
                .count();
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(failures, 1, "only the current audio command can fail playback");
    }
}
