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
use crate::log_info;
use crate::log_warn;

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
pub(crate) fn real_home() -> std::path::PathBuf {
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
    #[cfg(unix)]
    {
        unsafe extern "C" { fn umask(mode: u16) -> u16; }
        let old = unsafe { umask(0o077) };
        std::fs::create_dir_all(&dir).ok();
        unsafe { umask(old); }
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(&dir).ok();
    let config_path = dir.join("config.toml");
    let loaded = manager::load_or_default(&config_path);

    // ── Init logging BEFORE config validation ────────────────────────────
    // Must come first so that warn_log() calls in the validation block below
    // are not silently swallowed (LOG_STATE would still be None).
    let level = match loaded.logging.loglevel.as_str() {
        "debug" => logging::LogLevel::Debug,
        _ => logging::LogLevel::Info,
    };
    let _ = logging::init(level, log_dir());

    // ── Validate config; fall back to defaults on failure ────────────────
    // A damaged/hostile config.toml should not prevent the engine from
    // starting. Log a warning and use built-in defaults if validation fails.
    let config = if let Err(bad) = validate_bindings(&loaded) {
        logging::warn_log("ffi", &format!(
            "config has invalid binding '{}', falling back to defaults", bad
        ));
        toml::from_str(&crate::config::defaults::default_config_toml())
            .expect("default config TOML is valid")
    } else if !loaded.allow_destructive {
        if let Some(name) = find_destructive_binding(&loaded) {
            logging::warn_log("ffi", &format!(
                "destructive event '{}' bound but allow_destructive is false, falling back to defaults", name
            ));
            toml::from_str(&crate::config::defaults::default_config_toml())
                .expect("default config TOML is valid")
        } else {
            loaded
        }
    } else {
        loaded
    };

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

        log_info!("engine", "hub seized → ACTIVE");

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
                    log_info!("engine", "device unplugged → SEEKING");
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
                    log_warn!("engine", "run_once error → SEEKING: {}", e);
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

    // ── Semantic validation — reject reload with invalid config ──────────
    if let Err(bad) = validate_bindings(&config) {
        logging::error_log("ffi", &format!("config reload rejected: invalid binding '{}'", bad));
        return -1;
    }
    if !config.allow_destructive {
        if let Some(name) = find_destructive_binding(&config) {
            logging::error_log("ffi",
                &format!("destructive event '{}' bound but allow_destructive is false", name));
            return -1;
        }
    }

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

    // ── Semantic validation of keyboard bindings ─────────────────────────
    // Iterate all button mappings and validate each Keyboard binding string
    // via parse_combo(). Reject the entire save if any binding is invalid.
    // This catches typos or garbage that serde's structural validation
    // would accept, and which would silently become no-ops at runtime.
    if let Err(bad) = validate_bindings(&config) {
        logging::error_log("ffi", &format!("config rejected: invalid binding '{}'", bad));
        return -1;
    }

    // ── Destructive event warning ───────────────────────────────────────
    // If any SystemEvent (Sleep/Restart/Shutdown) is bound and
    // allow_destructive is false, reject the save.
    if !config.allow_destructive {
        if let Some(name) = find_destructive_binding(&config) {
            logging::error_log("ffi",
                &format!("destructive event '{}' bound but allow_destructive is false", name));
            return -1;
        }
    }

    let path = config_dir().join("config.toml");
    if let Err(e) = manager::save(&config, &path) {
        logging::error_log("ffi", &format!("config save: {}", e));
        return -1;
    }
    // Update live CONFIG so the engine picks up changes without restart.
    *config_lock() = Some(config);
    0
}

/// Validate all keyboard bindings in the config. Returns Err(first invalid binding).
fn validate_bindings(config: &Config) -> Result<(), String> {
    for mapping in mappings_iter(config) {
        if let Some(crate::config::types::TargetEvent::Keyboard { binding, .. }) = mapping {
            if !binding.is_empty() && crate::config::combo::parse_combo(binding).is_none() {
                return Err(binding.clone());
            }
        }
    }
    Ok(())
}

/// Find the first destructive binding label. Returns None if none found.
fn find_destructive_binding(config: &Config) -> Option<String> {
    for mapping in mappings_iter(config) {
        if let Some(evt) = mapping {
            if evt.is_destructive() {
                return Some(evt.label().to_string());
            }
        }
    }
    None
}

/// Iterate all non-None button mappings across default + profiles.
fn mappings_iter(config: &Config) -> impl Iterator<Item = &Option<crate::config::types::TargetEvent>> + '_ {
    let all = [
        &config.default.button_top_left,
        &config.default.button_top_left_hold,
        &config.default.button_bottom_right,
        &config.default.button_bottom_right_hold,
        &config.default.play_pause,
        &config.default.knob_cw,
        &config.default.knob_ccw,
        &config.default.knob_click,
    ];
    let profile_mappings: Vec<&Option<crate::config::types::TargetEvent>> = config.profiles
        .iter()
        .flat_map(|p| [
            &p.mappings.button_top_left,
            &p.mappings.button_top_left_hold,
            &p.mappings.button_bottom_right,
            &p.mappings.button_bottom_right_hold,
            &p.mappings.play_pause,
            &p.mappings.knob_cw,
            &p.mappings.knob_ccw,
            &p.mappings.knob_click,
        ])
        .collect();
    all.into_iter().chain(profile_mappings.into_iter())
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

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::*;

    fn empty_config() -> Config {
        Config {
            device: DeviceConfig { seize: vec![] },
            default: ButtonMappingSet {
                button_top_left: None,
                button_top_left_hold: None,
                button_bottom_right: None,
                button_bottom_right_hold: None,
                play_pause: None,
                knob_cw: None,
                knob_ccw: None,
                knob_click: None,
            },
            profiles: vec![],
            logging: crate::config::types::LoggingConfig { loglevel: "info".into() },
            allow_destructive: false,
        }
    }

    fn mk_kb(binding: &str) -> TargetEvent {
        TargetEvent::Keyboard { binding: binding.into(), label: "".into() }
    }

    // ── validate_bindings ───────────────────────────────────────────────

    #[test]
    fn validate_accepts_valid_bindings() {
        let mut config = empty_config();
        config.default.button_top_left = Some(mk_kb("Ctrl+Shift+Q"));
        config.default.button_top_left_hold = Some(mk_kb("Alt+Tab"));
        config.default.button_bottom_right = Some(mk_kb("Enter"));
        assert!(validate_bindings(&config).is_ok());
    }

    #[test]
    fn validate_rejects_unknown_key() {
        let mut config = empty_config();
        config.default.button_top_left = Some(mk_kb("BogusKey"));
        let err = validate_bindings(&config).unwrap_err();
        assert_eq!(err, "BogusKey");
    }

    #[test]
    fn validate_rejects_unknown_modifier() {
        let mut config = empty_config();
        config.default.button_top_left = Some(mk_kb("Ctrl+BogusModifier"));
        let err = validate_bindings(&config).unwrap_err();
        assert_eq!(err, "Ctrl+BogusModifier");
    }

    #[test]
    fn validate_rejects_multiple_invalid() {
        let mut config = empty_config();
        config.default.button_top_left = Some(mk_kb("NotAKey+NotAThing"));
        let err = validate_bindings(&config).unwrap_err();
        assert_eq!(err, "NotAKey+NotAThing");
    }

    #[test]
    fn validate_skips_empty_binding() {
        let mut config = empty_config();
        config.default.button_top_left = Some(TargetEvent::Keyboard { binding: "".into(), label: "".into() });
        assert!(validate_bindings(&config).is_ok());
    }

    #[test]
    fn validate_ignores_non_keyboard_events() {
        let mut config = empty_config();
        config.default.button_top_left = Some(TargetEvent::MediaKey { key_type: 0, label: "".into() });
        config.default.button_top_left_hold = Some(TargetEvent::SystemEvent { subtype: 11, data: 0, label: "".into() });
        config.default.button_bottom_right = Some(TargetEvent::MouseMove { dx: 10.0, dy: 0.0, label: "".into() });
        config.default.button_bottom_right_hold = Some(TargetEvent::MouseClick { button: 1, x: None, y: None, label: "".into() });
        assert!(validate_bindings(&config).is_ok());
    }

    // ── find_destructive_binding ────────────────────────────────────────

    #[test]
    fn find_destructive_finds_shutdown() {
        let mut config = empty_config();
        config.default.button_top_left = Some(TargetEvent::SystemEvent {
            subtype: 13, data: 0, label: "".into(),
        });
        let found = find_destructive_binding(&config);
        assert!(found.is_some());
        assert!(found.unwrap().contains("Shutdown"));
    }

    #[test]
    fn find_destructive_finds_sleep() {
        let mut config = empty_config();
        config.default.button_bottom_right = Some(TargetEvent::SystemEvent {
            subtype: 11, data: 0, label: "Zzz".into(),
        });
        let found = find_destructive_binding(&config);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), "Zzz");
    }

    #[test]
    fn find_destructive_returns_none_for_safe_config() {
        let mut config = empty_config();
        config.default.button_top_left = Some(mk_kb("Ctrl+Q"));
        config.default.button_top_left_hold = Some(TargetEvent::MediaKey { key_type: 16, label: "".into() });
        config.default.knob_cw = Some(TargetEvent::MouseMove { dx: 5.0, dy: 0.0, label: "".into() });
        assert!(find_destructive_binding(&config).is_none());
    }

    #[test]
    fn find_destructive_returns_none_for_eject() {
        let mut config = empty_config();
        config.default.button_top_left = Some(TargetEvent::SystemEvent {
            subtype: 10, data: 0, label: "".into(),
        });
        assert!(find_destructive_binding(&config).is_none());
    }

    #[test]
    fn find_destructive_scans_profiles_too() {
        let mut config = empty_config();
        config.profiles = vec![Profile {
            app_id: "com.example".into(),
            app_name: "Example".into(),
            mappings: ButtonMappingSet {
                button_top_left: Some(TargetEvent::SystemEvent {
                    subtype: 12, data: 0, label: "Restart".into(),
                }),
                button_top_left_hold: None, button_bottom_right: None,
                button_bottom_right_hold: None, play_pause: None,
                knob_cw: None, knob_ccw: None, knob_click: None,
            },
        }];
        let found = find_destructive_binding(&config);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), "Restart");
    }
}

/// Log a message into the engine's log file.
/// level: 0=Error, 1=Warn, 2=Info, 3=Debug.
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_log(level: i32, cat_ptr: *const c_char, msg_ptr: *const c_char) {
    if cat_ptr.is_null() || msg_ptr.is_null() { return }
    let cat = unsafe { CStr::from_ptr(cat_ptr) }.to_string_lossy();
    let msg = unsafe { CStr::from_ptr(msg_ptr) }.to_string_lossy();
    match level {
        0 => logging::error_log(&cat, &msg),
        1 => logging::warn_log(&cat, &msg),
        2 => logging::info_log(&cat, &msg),
        _ => logging::debug(&cat, &msg),
    }
}
