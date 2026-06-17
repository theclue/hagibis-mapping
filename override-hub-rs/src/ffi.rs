//! C FFI bridge for native GUI integration (Swift, C#, etc.).
//!
//! Functions are #[no_mangle] extern "C" so they can be called from
//! any language that supports the C ABI.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Mutex;
use std::thread::JoinHandle;

use crate::backend::{FocusQuery, HIDBackend};
use crate::config::manager;
use crate::config::Config;
use crate::engine::key_mapper::ConfigKeyMapper;
use crate::engine::Dispatcher;
use crate::hid::Report;
use crate::logging;

// ── Global engine state ──────────────────────────────────────────────────

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
static STATUS: Mutex<Status> = Mutex::new(Status {
    running: false, seized: false,
    consumer: 0, consumer_sticky: 0, consumer_hold: 0,
    keyboard_keys: vec![],
    keyboard_modifiers: 0,
    focused_app_id: String::new(),
    focused_app_name: String::new(),
    btn_tl: String::new(), btn_tl_hold: String::new(),
    btn_br: String::new(), btn_br_hold: String::new(),
    error: String::new(),
});

struct Engine {
    thread: JoinHandle<()>,
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Set true by the engine thread right before it exits, for ANY reason
    /// (normal stop, seize failure, run-loop error, or panic). Read via atomic
    /// so callers can detect a self-terminated engine WITHOUT locking ENGINE —
    /// the thread must never lock ENGINE itself, or it would deadlock against
    /// `hagibis_stop()` which holds ENGINE while joining the thread.
    finished: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

// ── Poison-tolerant lock helpers ─────────────────────────────────────────
//
// A panic while a Mutex is held normally "poisons" it, after which every
// `.lock().unwrap()` panics too. Because some of these locks are taken inside
// `extern "C"` functions (e.g. `hagibis_status_json`, polled every 150 ms by
// the Swift timer), a propagated panic would unwind across the FFI boundary and
// abort the whole process — leaving the device seized. We therefore recover the
// inner value on poison instead of unwinding.

fn status() -> std::sync::MutexGuard<'static, Status> {
    STATUS.lock().unwrap_or_else(|e| e.into_inner())
}

fn engine() -> std::sync::MutexGuard<'static, Option<Engine>> {
    ENGINE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Run an FFI body, catching any panic so it never unwinds across the
/// `extern "C"` boundary (which would abort the whole privileged process and
/// leave the device seized). Returns `default` if the body panics.
fn ffi_guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            logging::error_log("ffi", "panic caught at FFI boundary");
            default
        }
    }
}

#[derive(Clone, serde::Serialize)]
struct Status {
    running: bool,
    seized: bool,
    consumer: u8,
    consumer_sticky: u8,
    consumer_hold: u32,
    keyboard_keys: Vec<u8>,
    keyboard_modifiers: u8,
    focused_app_id: String,
    focused_app_name: String,
    btn_tl: String,
    btn_tl_hold: String,
    btn_br: String,
    btn_br_hold: String,
    error: String,
}

impl Status {
    fn idle() -> Self {
        Self {
            running: false, seized: false,
            consumer: 0, consumer_sticky: 0, consumer_hold: 0,
            keyboard_keys: vec![],
            keyboard_modifiers: 0,
            focused_app_id: String::new(),
            focused_app_name: "—".into(),
            btn_tl: "—".into(), btn_tl_hold: "—".into(),
            btn_br: "—".into(), btn_br_hold: "—".into(),
            error: String::new(),
        }
    }
}

// ── C FFI functions ──────────────────────────────────────────────────────

/// Resolve the real (logged-in) user's home directory.
///
/// When the app is elevated to root, `$HOME` becomes `/var/root`, which would
/// point to a different config than the one the user edits. We resolve the
/// console user's home (owner of /dev/console) so the privileged engine and the
/// user-facing config stay in sync.
#[cfg(target_os = "macos")]
fn real_home() -> std::path::PathBuf {
    use std::os::unix::fs::MetadataExt;

    // If not root, HOME is already correct.
    if unsafe { libc_geteuid() } != 0 {
        if let Ok(h) = std::env::var("HOME") {
            if !h.is_empty() { return std::path::PathBuf::from(h); }
        }
    }

    // Root: find the console user via /dev/console owner, then look up their home.
    if let Ok(meta) = std::fs::metadata("/dev/console") {
        let uid = meta.uid();
        if uid != 0 {
            if let Some(home) = home_for_uid(uid) {
                return home;
            }
        }
    }

    // Fallback to HOME or current dir
    std::env::var("HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    #[link_name = "geteuid"]
    fn libc_geteuid() -> u32;
    // Thread-safe reentrant variant: caller supplies the struct + scratch buffer
    // (plain getpwuid() returns a pointer into a shared static buffer).
    #[link_name = "getpwuid_r"]
    fn libc_getpwuid_r(
        uid: u32,
        pwd: *mut Passwd,
        buf: *mut std::ffi::c_char,
        buflen: usize,
        result: *mut *mut Passwd,
    ) -> i32;
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct Passwd {
    pw_name: *const std::ffi::c_char,
    pw_passwd: *const std::ffi::c_char,
    pw_uid: u32,
    pw_gid: u32,
    pw_change: i64,
    pw_class: *const std::ffi::c_char,
    pw_gecos: *const std::ffi::c_char,
    pw_dir: *const std::ffi::c_char,
    pw_shell: *const std::ffi::c_char,
    pw_expire: i64,
}

#[cfg(target_os = "macos")]
fn home_for_uid(uid: u32) -> Option<std::path::PathBuf> {
    // SAFETY: `pwd` and `buf` are caller-owned and stay alive while we read
    // `pwd.pw_dir`, which points into `buf`. getpwuid_r fills `result` with a
    // pointer to `pwd` on success, or leaves it NULL if the user isn't found.
    let mut pwd: Passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as std::ffi::c_char; 4096];
    let mut result: *mut Passwd = std::ptr::null_mut();
    let rc = unsafe {
        libc_getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result)
    };
    if rc != 0 || result.is_null() || pwd.pw_dir.is_null() {
        return None;
    }
    let dir = unsafe { std::ffi::CStr::from_ptr(pwd.pw_dir) }
        .to_string_lossy()
        .into_owned();
    if dir.is_empty() { None } else { Some(std::path::PathBuf::from(dir)) }
}

#[cfg(not(target_os = "macos"))]
fn real_home() -> std::path::PathBuf {
    std::env::var("HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

fn config_dir() -> std::path::PathBuf {
    real_home().join("Library/Application Support/override-hub")
}

fn log_dir() -> std::path::PathBuf {
    real_home().join("Library/Logs/override-hub")
}

/// Start the engine (seize + poll loop in a background thread).
/// Returns 0 on success, -1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_start() -> i32 {
    ffi_guard(-1, hagibis_start_impl)
}

fn hagibis_start_impl() -> i32 {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).ok();
    let config_path = dir.join("config.toml");
    let config = manager::load_or_default(&config_path);

    // Init logging
    let level = match config.logging.loglevel.as_str() {
        "debug" => logging::LogLevel::Debug,
        _ => logging::LogLevel::Info,
    };
    let _ = logging::init(level, log_dir());

    let mut engine_guard = engine();
    match engine_guard.as_ref() {
        // A live engine — nothing to do.
        Some(e) if !e.finished.load(std::sync::atomic::Ordering::Relaxed) => {
            return 0; // already running
        }
        // A stale engine that terminated on its own (seize failure, run-loop
        // error, or panic). Reap it by joining the finished thread before we
        // start a fresh one; otherwise ENGINE would stay non-None forever and
        // block every restart.
        Some(_) => {
            if let Some(old) = engine_guard.take() {
                let _ = old.thread.join();
            }
        }
        None => {}
    }

    let stop_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop = stop_flag.clone();
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished_thread = finished.clone();

    // Mark running BEFORE spawning the thread. The two STATUS writes (this one
    // and the thread's own update / seize-failure idle-reset) are serialized by
    // the mutex but otherwise unordered; doing this first guarantees the thread's
    // write is the LATER one. Otherwise a fast seize() failure could have its
    // idle-reset clobbered here, leaving running=true stuck forever (GUI shows
    // "Running" while hagibis_is_running() reports stopped).
    {
        let mut st = status();
        st.running = true;
        st.error.clear();
    }

    let handle = std::thread::spawn(move || {
        // Catch any panic from the loop so it never escapes the thread without
        // cleanup. The HID manager is a local inside `engine_loop`, so unwinding
        // runs its Drop and releases the device before we get here.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine_loop(&config, &stop);
        }));
        if outcome.is_err() {
            logging::error_log("ffi", "engine thread panicked; device released via Drop");
            let mut st = status();
            *st = Status::idle();
            st.error = "engine panicked".into();
        }
        // Signal termination LAST and via atomic only. Never lock ENGINE here:
        // hagibis_stop() holds ENGINE while joining this thread, so touching it
        // would deadlock.
        //
        // Setting `finished` last is intentional: observing `finished == true`
        // therefore GUARANTEES the device is already released (release() ran in
        // engine_loop). This leaves a sub-millisecond window where
        // hagibis_is_running() may report 1 while STATUS.running is already
        // false — that is the correct, safe direction (the engine really is
        // still finishing), and it converges to "stopped". Do NOT reorder these
        // to "fix" it: setting finished before release() would let a reap/restart
        // observe finished while the device is still seized.
        finished_thread.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    *engine_guard = Some(Engine { thread: handle, stop_flag, finished });
    0
}

/// The engine's seize → poll → dispatch loop. Runs on the background thread.
///
/// All OS resources (HID manager, injector, focus query) are created here as
/// locals, so an unwind drops them — releasing the device — before control
/// returns to the spawning closure.
fn engine_loop(config: &Config, stop: &std::sync::atomic::AtomicBool) {
    #[cfg(target_os = "macos")]
    let (mut seize_backend, injector, focus_query) = {
        use crate::backend::macos;
        (macos::IOKitManager::new(), macos::CGEventInjector::new(), macos::NSWorkspaceFocus::new())
    };
    #[cfg(target_os = "windows")]
    let (mut seize_backend, injector, focus_query) = {
        use crate::backend::windows;
        (windows::WinHIDManager::new(), windows::SendInputInjector::new(), windows::Win32Focus::new())
    };

    if let Err(e) = seize_backend.seize() {
        // Reset to idle (NOT just set error): hagibis_start() optimistically set
        // running=true before spawning us. Leaving it true would desync the GUI
        // (panel shows "Running", menu shows "Stop") against hagibis_is_running().
        let mut st = status();
        *st = Status::idle();
        st.error = format!("{}", e);
        return;
    }

    let mut dispatcher = Dispatcher::new();

    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        // Always update status (even on timeout, to reflect focused app changes)
        {
            // Resolve focus BEFORE locking STATUS: focused_app() makes a
            // synchronous NSWorkspace IPC call; holding STATUS across it would
            // stall the Swift status poll if the WindowServer is slow.
            let focused = focus_query.focused_app();

            let mut st = status();
            st.running = true;
            st.seized = true;
            st.error.clear();

            // Sticky consumer: hold non-zero bits visible for a few cycles
            if st.consumer_hold > 0 {
                st.consumer_hold -= 1;
                if st.consumer_hold == 0 {
                    st.consumer_sticky = 0;
                }
            }
            if let Some(app) = focused {
                st.focused_app_id = app.id.clone();
                st.focused_app_name = app.name.clone();
            }
            let app_id_opt: Option<&str> = if st.focused_app_id.is_empty() {
                None
            } else {
                Some(&st.focused_app_id)
            };
            let mapping = ConfigKeyMapper::resolve(config, app_id_opt);
            st.btn_tl = mapping.button_top_left.as_ref().map(|e| e.label()).unwrap_or("—").into();
            st.btn_tl_hold = mapping.button_top_left_hold.as_ref().map(|e| e.label()).unwrap_or("—").into();
            st.btn_br = mapping.button_bottom_right.as_ref().map(|e| e.label()).unwrap_or("—").into();
            st.btn_br_hold = mapping.button_bottom_right_hold.as_ref().map(|e| e.label()).unwrap_or("—").into();
        }

        match seize_backend.run_once(50) {
            Ok(Some(report)) => {
                let focused = focus_query.focused_app();
                let app_id = focused.as_ref().map(|a| a.id.as_str());
                if let Err(e) = dispatcher.dispatch(&report, config, app_id, &injector) {
                    let mut st = status();
                    st.error = format!("{}", e);
                }

                let mut st = status();
                match &report {
                    Report::Consumer(c) => {
                        st.consumer = c.bits.0;
                        if c.bits.0 != 0 {
                            st.consumer_sticky = c.bits.0;
                            st.consumer_hold = 3; // hold for ~3 engine cycles (~150ms)
                        }
                    }
                    Report::Keyboard(kb) => {
                        st.keyboard_keys = kb.keycodes.iter().map(|k| k.0).collect();
                        st.keyboard_modifiers = kb.modifier;
                    }
                    _ => {}
                }
            }
            Ok(None) => {}
            Err(e) => {
                let mut st = status();
                st.error = format!("{}", e);
                break;
            }
        }
    }

    seize_backend.release();
    let mut st = status();
    *st = Status::idle();
}

/// Stop the engine and release devices.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_stop() {
    ffi_guard((), hagibis_stop_impl)
}

fn hagibis_stop_impl() {
    // Poison-tolerant: if a previous panic poisoned ENGINE we must still be able
    // to stop and release the device, otherwise it stays seized.
    let mut guard = engine();
    if let Some(engine) = guard.take() {
        engine.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        // The thread never locks ENGINE, so joining while holding it is safe.
        let _ = engine.thread.join();
    }
}

/// Write the current engine status as JSON into `buf` (max `buf_size` bytes).
/// Returns the number of bytes written (excluding null terminator), or 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_status_json(buf: *mut c_char, buf_size: i32) -> i32 {
    ffi_guard(0, || hagibis_status_json_impl(buf, buf_size))
}

fn hagibis_status_json_impl(buf: *mut c_char, buf_size: i32) -> i32 {
    if buf.is_null() || buf_size < 1 {
        return 0;
    }
    let status = status().clone();
    let json = match serde_json::to_string(&status) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    write_cstr(&json, buf, buf_size)
}

/// Load the current config as a JSON string.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_config_json(buf: *mut c_char, buf_size: i32) -> i32 {
    ffi_guard(0, || hagibis_config_json_impl(buf, buf_size))
}

fn hagibis_config_json_impl(buf: *mut c_char, buf_size: i32) -> i32 {
    if buf.is_null() || buf_size < 1 {
        return 0;
    }
    let dir = config_dir();
    let config_path = dir.join("config.toml");
    let config = manager::load_or_default(&config_path);
    let json = match serde_json::to_string(&config) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    write_cstr(&json, buf, buf_size)
}

/// Copy `s` into the C buffer as a NUL-terminated string, truncating on a UTF-8
/// char boundary so we never emit a split multibyte sequence (which would make
/// the Swift side fail to decode and silently drop the update). Caller must have
/// already checked `buf` non-null and `buf_size >= 1`. Returns bytes written
/// (excluding the NUL terminator).
fn write_cstr(s: &str, buf: *mut c_char, buf_size: i32) -> i32 {
    let max = buf_size as usize - 1;
    let mut copy_len = s.len().min(max);
    while copy_len > 0 && !s.is_char_boundary(copy_len) {
        copy_len -= 1;
    }
    let bytes = s.as_bytes();
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, copy_len);
        *buf.add(copy_len) = 0;
    }
    copy_len as i32
}

/// Save config from a JSON string. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_save_config_json(json_ptr: *const c_char) -> i32 {
    ffi_guard(-1, || hagibis_save_config_json_impl(json_ptr))
}

fn hagibis_save_config_json_impl(json_ptr: *const c_char) -> i32 {
    if json_ptr.is_null() {
        return -1;
    }
    let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_string_lossy();
    let config: Config = match serde_json::from_str(&json_str) {
        Ok(c) => c,
        Err(e) => {
            logging::error_log("ffi", &format!("config parse: {}", e));
            return -1;
        }
    };
    let dir = config_dir();
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join("config.toml");
    let toml_str = match toml::to_string_pretty(&config) {
        Ok(s) => s,
        Err(e) => {
            logging::error_log("ffi", &format!("config serialize: {}", e));
            return -1;
        }
    };
    if let Err(e) = std::fs::write(&path, toml_str) {
        logging::error_log("ffi", &format!("config write: {}", e));
        return -1;
    }
    0
}

/// Returns 1 if the engine is running, 0 otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_is_running() -> i32 {
    ffi_guard(0, hagibis_is_running_impl)
}

fn hagibis_is_running_impl() -> i32 {
    let guard = engine();
    match guard.as_ref() {
        // Present but self-terminated (panic / seize error) counts as NOT
        // running, so the GUI can offer "Start" again.
        Some(e) if !e.finished.load(std::sync::atomic::Ordering::Relaxed) => 1,
        _ => 0,
    }
}
