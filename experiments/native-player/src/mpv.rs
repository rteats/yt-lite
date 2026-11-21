//! Minimal dynamic libmpv bridge for Windows. The C declarations match mpv's
//! ISC-licensed public client.h, render.h and render_gl.h ABI.
//!
//! Keep this bridge isolated: it must not be reused for release packaging until
//! frame pacing, GL state preservation and hardware decode have been measured.

use crate::PlayerWindow;
use libloading::Library;
use slint::Weak;
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

#[derive(Clone, Copy)]
struct Api {
    create: unsafe extern "C" fn() -> *mut c_void,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut c_void),
    set_option_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    command_async: unsafe extern "C" fn(*mut c_void, u64, *const *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    render_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *const RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(*mut c_void),
    render_render: unsafe extern "C" fn(*mut c_void, *const RenderParam) -> c_int,
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    render_callback: unsafe extern "C" fn(*mut c_void, RenderUpdate, *mut c_void),
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
            command_async: get(library, b"mpv_command_async\0")?,
            get_property: get(library, b"mpv_get_property\0")?,
            render_create: get(library, b"mpv_render_context_create\0")?,
            render_free: get(library, b"mpv_render_context_free\0")?,
            render_render: get(library, b"mpv_render_context_render\0")?,
            render_update: get(library, b"mpv_render_context_update\0")?,
            render_callback: get(library, b"mpv_render_context_set_update_callback\0")?,
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
    _lib: Library,
}

impl Player {
    pub fn open() -> Result<Self, String> {
        // Deliberately load only an explicitly bundled DLL. Do not search PATH.
        let dll = std::env::current_exe().map_err(|err| err.to_string())?
            .parent().ok_or("Missing executable directory")?.join("mpv-2.dll");
        let lib = unsafe { Library::new(&dll) }
            .map_err(|err| format!("Missing compatible bundled {}: {err}", dll.display()))?;
        let api = unsafe { Api::load(&lib)? };
        let handle = unsafe { (api.create)() };
        if handle.is_null() { return Err("mpv_create failed".into()); }
        let mut player = Self { api, handle, context: ptr::null_mut(), wake: None, _lib: lib };
        for (key, value) in [
            ("config", "no"),
            ("vo", "libmpv"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("osc", "no"),
            ("hwdec", "auto-safe"),
        ] {
            let key = CString::new(key).unwrap();
            let value = CString::new(value).unwrap();
            let status = unsafe { (player.api.set_option_string)(player.handle, key.as_ptr(), value.as_ptr()) };
            if status < 0 { return Err(format!("mpv rejected option {}", key.to_string_lossy())); }
        }
        let code = unsafe { (player.api.initialize)(player.handle) };
        if code < 0 { return Err(format!("mpv_initialize failed: {code}")); }
        Ok(player)
    }

    pub fn setup_render(&mut self, ui: Weak<PlayerWindow>, clip: &Path) -> Result<(), String> {
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
        self.send_strings(&["loadfile".into(), clip.to_string_lossy().into_owned(), "replace".into()])?;
        Ok(())
    }

    pub fn send_strings(&self, args: &[String]) -> Result<(), String> {
        let strings: Vec<_> = args.iter().map(|s| CString::new(s.as_str())
            .map_err(|_| "Command contains an embedded NUL".to_string())).collect::<Result<_, _>>()?;
        let mut pointers: Vec<*const c_char> = strings.iter().map(|s| s.as_ptr()).collect();
        pointers.push(ptr::null());
        let status = unsafe { (self.api.command_async)(self.handle, 0, pointers.as_ptr()) };
        if status < 0 { Err(format!("mpv_command_async rejected command: {status}")) } else { Ok(()) }
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
