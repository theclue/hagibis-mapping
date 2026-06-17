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

/// Live config — loaded once at start, reloadable via `hagibis_reload_config()`.
/// The engine thread reads from this on every poll cycle, so a reload is
/// picked up immediately without restarting the device seizure.
static CONFIG: Mutex<Option<Config>> = Mutex::new(None);

static STATUS: Mutex<Status> = Mutex::new(Status {
    running: false, seized: false,
    consumer: 0, consumer_sticky: 0, consumer_hold: 0,
    keyboard_keys: vec![],
    keyboard_modifiers: 0,
    focused_app_id: String::new(),
    focused_app_name: String::new(),
    btn_tl: String::new(), btn_tl_hold: String::new(),
    btn_br: String::new(), btn_br_hold: String::new(),
    knob_cw: String::new(), knob_ccw: String::new(),
    knob_click: String::new(), play_pause: String::new(),
    engine_present: false,
    error: String::new(),
});

struct Engine {
    thread: JoinHandle<()>,
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    finished: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

// ── Poison-tolerant lock helpers ─────────────────────────────────────────

fn status() -> std::sync::MutexGuard<'static, Status> {
    STATUS.lock().unwrap_or_else(|e| e.into_inner())
}

fn engine() -> std::sync::MutexGuard<'static, Option<Engine>> {
    ENGINE.lock().unwrap_or_else(|e| e.into_inner())
}

fn config_lock() -> std::sync::MutexGuard<'static, Option<Config>> {
    CONFIG.lock().unwrap_or_else(|e| e.into_inner())
}

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
    knob_cw: String,
    knob_ccw: String,
    knob_click: String,
    play_pause: String,
    engine_present: bool,
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
            knob_cw: "—".into(), knob_ccw: "—".into(),
            knob_click: "—".into(), play_pause: "—".into(),
            engine_present: false,
            error: String::new(),
        }
    }
}

// ── real_home / config_dir / log_dir ─────────────────────────────────────

#[cfg(target_os = "macos")]
fn real_home() -> std::path::PathBuf {
    use std::os::unix::fs::MetadataExt;

    if unsafe { libc_geteuid() } != 0 {
        if let Ok(h) = std::env::var("HOME") {
            if !h.is_empty() { return std::path::PathBuf::from(h); }
        }
    }

    if let Ok(meta) = std::fs::metadata("/dev/console") {
        let uid = meta.uid();
        if uid != 0 {
            if let Some(home) = home_for_uid(uid) {
                return home;
            }
        }
    }

    std::env::var("HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    #[link_name = "geteuid"]
    fn libc_geteuid() -> u32;
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

// ── C FFI functions ──────────────────────────────────────────────────────

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

    let level = match config.logging.loglevel.as_str() {
        "debug" => logging::LogLevel::Debug,
        _ => logging::LogLevel::Info,
    };
    let _ = logging::init(level, log_dir());

    *config_lock() = Some(config);

    let mut engine_guard = engine();
    match engine_guard.as_ref() {
        Some(e) if !e.finished.load(std::sync::atomic::Ordering::Relaxed) => {
            return 0;
        }
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

    let handle = std::thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine_loop(&stop);
        }));
        if outcome.is_err() {
            logging::error_log("ffi", "engine thread panicked; device released via Drop");
            let mut st = status();
            *st = Status::idle();
            st.error = "engine panicked".into();
        }
        finished_thread.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    *engine_guard = Some(Engine { thread: handle, stop_flag, finished });
    0
}

fn engine_loop(stop: &std::sync::atomic::AtomicBool) {
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        // ── SEEKING ──────────────────────────────────────────────────────
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

        loop {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                let mut st = status();
                *st = Status::idle();
                return;
            }

            match seize_backend.seize() {
                Ok(()) => break,
                Err(e) => {
                    let mut st = status();
                    *st = Status::idle();
                    st.engine_present = true;
                    st.error = format!("{}", e);
                    #[cfg(target_os = "macos")]
                    { use crate::backend::macos; seize_backend = macos::IOKitManager::new(); }
                    #[cfg(target_os = "windows")]
                    { use crate::backend::windows; seize_backend = windows::WinHIDManager::new(); }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }

        // ── ACTIVE ──────────────────────────────────────────────────────
        let mut dispatcher = Dispatcher::new();

        loop {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                seize_backend.release();
                let mut st = status();
                *st = Status::idle();
                return;
            }

            #[cfg(target_os = "macos")]
            {
                if !seize_backend.is_device_present() {
                    seize_backend.release();
                    let mut st = status();
                    *st = Status::idle();
                    st.engine_present = true;
                    break;
                }
            }

            {
                let focused = focus_query.focused_app();

                let cfg = config_lock();
                let config = match cfg.as_ref() {
                    Some(c) => c,
                    None => {
                        drop(cfg);
                        let _ = seize_backend.run_once(50);
                        continue;
                    }
                };

            let mut st = status();
            st.running = true;
            st.seized = true;
            st.engine_present = true;
            st.error.clear();

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
                st.knob_cw = mapping.knob_cw.as_ref().map(|e| e.label()).unwrap_or("—").into();
                st.knob_ccw = mapping.knob_ccw.as_ref().map(|e| e.label()).unwrap_or("—").into();
                st.knob_click = mapping.knob_click.as_ref().map(|e| e.label()).unwrap_or("—").into();
                st.play_pause = mapping.play_pause.as_ref().map(|e| e.label()).unwrap_or("—").into();
            }

            match seize_backend.run_once(50) {
                Ok(Some(report)) => {
                    let focused = focus_query.focused_app();
                    let app_id = focused.as_ref().map(|a| a.id.as_str());
                    let cfg = config_lock();
                    if let Some(config) = cfg.as_ref() {
                        if let Err(e) = dispatcher.dispatch(&report, config, app_id, &injector) {
                            let mut st = status();
                            st.error = format!("{}", e);
                        }
                    }

                    let mut st = status();
                    match &report {
                        Report::Consumer(c) => {
                            st.consumer = c.bits.0;
                            if c.bits.0 != 0 {
                                st.consumer_sticky = c.bits.0;
                                st.consumer_hold = 3;
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
    }

    let mut st = status();
    *st = Status::idle();
}

/// Stop the engine and release devices.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_stop() {
    ffi_guard((), hagibis_stop_impl)
}

fn hagibis_stop_impl() {
    let mut guard = engine();
    if let Some(engine) = guard.take() {
        engine.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = engine.thread.join();
    }
}

/// Write the current engine status as JSON into `buf`.
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

/// Load the current config as a JSON string (from the live CONFIG global).
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_config_json(buf: *mut c_char, buf_size: i32) -> i32 {
    ffi_guard(0, || hagibis_config_json_impl(buf, buf_size))
}

fn hagibis_config_json_impl(buf: *mut c_char, buf_size: i32) -> i32 {
    if buf.is_null() || buf_size < 1 {
        return 0;
    }
    let config = config_lock().clone().unwrap_or_else(|| {
        let path = config_dir().join("config.toml");
        manager::load_or_default(&path)
    });
    let json = match serde_json::to_string(&config) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    write_cstr(&json, buf, buf_size)
}

/// Reload config from disk into the live CONFIG global.
/// Call after `hagibis_save_config_json` so the engine picks up changes
/// immediately.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_reload_config() -> i32 {
    ffi_guard(-1, hagibis_reload_config_impl)
}

fn hagibis_reload_config_impl() -> i32 {
    let path = config_dir().join("config.toml");
    let config = manager::load_or_default(&path);
    *config_lock() = Some(config);
    0
}

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
/// Also updates the live CONFIG global so the engine sees changes instantly.
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
    let path = config_dir().join("config.toml");
    if let Err(e) = manager::save(&config, &path) {
        logging::error_log("ffi", &format!("config save: {}", e));
        return -1;
    }
    // Update live CONFIG so the engine picks up changes without restart.
    *config_lock() = Some(config);
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
        Some(e) if !e.finished.load(std::sync::atomic::Ordering::Relaxed) => 1,
        _ => 0,
    }
}
