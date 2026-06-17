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
    #[link_name = "getpwuid"]
    fn libc_getpwuid(uid: u32) -> *const Passwd;
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
    unsafe {
        let pw = libc_getpwuid(uid);
        if pw.is_null() || (*pw).pw_dir.is_null() {
            return None;
        }
        let dir = std::ffi::CStr::from_ptr((*pw).pw_dir).to_string_lossy().into_owned();
        if dir.is_empty() { None } else { Some(std::path::PathBuf::from(dir)) }
    }
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

    let mut engine_guard = ENGINE.lock().unwrap();
    if engine_guard.is_some() {
        return 0; // already running
    }

    let stop_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop = stop_flag.clone();

    let handle = std::thread::spawn(move || {
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
            let mut st = STATUS.lock().unwrap();
            st.error = format!("{}", e);
            return;
        }

        let mut dispatcher = Dispatcher::new();

        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            // Always update status (even on timeout, to reflect focused app changes)
            {
                let mut st = STATUS.lock().unwrap();
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
                let focused = focus_query.focused_app();
                if let Some(app) = focused {
                    st.focused_app_id = app.id.clone();
                    st.focused_app_name = app.name.clone();
                }
                let app_id_opt: Option<&str> = if st.focused_app_id.is_empty() {
                    None
                } else {
                    Some(&st.focused_app_id)
                };
                let mapping = ConfigKeyMapper::resolve(&config, app_id_opt);
                st.btn_tl = mapping.button_top_left.as_ref().map(|e| e.label()).unwrap_or("—").into();
                st.btn_tl_hold = mapping.button_top_left_hold.as_ref().map(|e| e.label()).unwrap_or("—").into();
                st.btn_br = mapping.button_bottom_right.as_ref().map(|e| e.label()).unwrap_or("—").into();
                st.btn_br_hold = mapping.button_bottom_right_hold.as_ref().map(|e| e.label()).unwrap_or("—").into();
            }

            match seize_backend.run_once(50) {
                Ok(Some(report)) => {
                    let focused = focus_query.focused_app();
                    let app_id = focused.as_ref().map(|a| a.id.as_str());
                    if let Err(e) = dispatcher.dispatch(&report, &config, app_id, &injector) {
                        let mut st = STATUS.lock().unwrap();
                        st.error = format!("{}", e);
                    }

                    let mut st = STATUS.lock().unwrap();
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
                    let mut st = STATUS.lock().unwrap();
                    st.error = format!("{}", e);
                    break;
                }
            }
        }

        seize_backend.release();
        let mut st = STATUS.lock().unwrap();
        *st = Status::idle();
    });

    *engine_guard = Some(Engine { thread: handle, stop_flag });
    let mut st = STATUS.lock().unwrap();
    st.running = true;
    st.error.clear();
    0
}

/// Stop the engine and release devices.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_stop() {
    if let Ok(mut guard) = ENGINE.lock() {
        if let Some(engine) = guard.take() {
            engine.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = engine.thread.join();
        }
    }
}

/// Write the current engine status as JSON into `buf` (max `buf_size` bytes).
/// Returns the number of bytes written (excluding null terminator), or 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_status_json(buf: *mut c_char, buf_size: i32) -> i32 {
    if buf.is_null() || buf_size < 1 {
        return 0;
    }
    let status = STATUS.lock().unwrap().clone();
    let json = match serde_json::to_string(&status) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let bytes = json.as_bytes();
    let copy_len = bytes.len().min(buf_size as usize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, copy_len);
        *buf.add(copy_len) = 0;
    }
    copy_len as i32
}

/// Load the current config as a JSON string.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_config_json(buf: *mut c_char, buf_size: i32) -> i32 {
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
    let bytes = json.as_bytes();
    let copy_len = bytes.len().min(buf_size as usize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, copy_len);
        *buf.add(copy_len) = 0;
    }
    copy_len as i32
}

/// Save config from a JSON string. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_save_config_json(json_ptr: *const c_char) -> i32 {
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
    if let Ok(guard) = ENGINE.lock() {
        if guard.is_some() { 1 } else { 0 }
    } else {
        0
    }
}
