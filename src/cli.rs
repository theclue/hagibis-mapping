use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::backend::{FocusQuery, HIDBackend};
use crate::config::manager;
use crate::engine::key_mapper::ConfigKeyMapper;
use crate::engine::Dispatcher;
use crate::error::Error;
use crate::log_debug;
use crate::log_info;
use crate::log_warn;
use crate::logging::LogLevel;
use crate::tui::TuiState;

fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    { Some(std::env::var("HOME").ok()?.into()).map(|h: PathBuf| h.join("Library/Application Support/override-hub")) }
    #[cfg(target_os = "windows")]
    { std::env::var("APPDATA").ok().map(|p| PathBuf::from(p).join("override-hub")) }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    { None }
}

fn log_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home: PathBuf = std::env::var("HOME").unwrap_or_else(|_| ".".into()).into();
        home.join("Library/Logs/override-hub")
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("override-hub/logs")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    { PathBuf::from(".") }
}

pub fn run() -> Result<(), Error> {
    let config_dir = config_dir().unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&config_dir).ok();
    let config_path = config_dir.join("config.toml");
    let config = Arc::new(Mutex::new(manager::load_or_default(&config_path)));
    log_info!("cli", "config loaded");

    {
        let cfg = config.lock().unwrap_or_else(|e| e.into_inner());
        let level = match cfg.logging.loglevel.as_str() {
            "debug" => LogLevel::Debug,
            "info"  => LogLevel::Info,
            "warn"  => LogLevel::Warn,
            _       => LogLevel::Info,
        };
        crate::logging::init(level, log_dir())?;
    }

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

    seize_backend.seize()?;
    log_info!("cli", "hub interfaces seized");

    let mut tui = TuiState::new();
    tui.seized = true;
    let mut dispatcher = Dispatcher::new();
    let mut stdout = std::io::stdout();

    // Hide cursor
    write!(stdout, "{}", crate::tui::HIDE)?;
    stdout.flush()?;

    while running() {
        // Resolve focused app and active mapping
        let focused = focus_query.focused_app();
        let (active_mapping, app_label) = {
            let cfg = config.lock().unwrap_or_else(|e| e.into_inner());
            let mapping = ConfigKeyMapper::resolve(&cfg, focused.as_ref().map(|a| a.id.as_str()));
            let label = focused.as_ref().map(|a| a.name.as_str()).unwrap_or("default");
            (mapping.clone(), label.to_string())
        };

        // Update TUI labels from the resolved mapping (always fully populated)
        tui.focused = app_label;
        tui.btn_tl = active_mapping.button_top_left.as_ref().map(|e| e.label()).unwrap_or("?").to_string();
        tui.btn_tl_hold = active_mapping.button_top_left_hold.as_ref().map(|e| e.label()).unwrap_or("?").to_string();
        tui.btn_br = active_mapping.button_bottom_right.as_ref().map(|e| e.label()).unwrap_or("?").to_string();
        tui.btn_br_hold = active_mapping.button_bottom_right_hold.as_ref().map(|e| e.label()).unwrap_or("?").to_string();
        tui.knob_cw = active_mapping.knob_cw.as_ref().map(|e| e.label()).unwrap_or("?").to_string();
        tui.knob_ccw = active_mapping.knob_ccw.as_ref().map(|e| e.label()).unwrap_or("?").to_string();
        tui.knob_click = active_mapping.knob_click.as_ref().map(|e| e.label()).unwrap_or("?").to_string();
        tui.play_pause = active_mapping.play_pause.as_ref().map(|e| e.label()).unwrap_or("?").to_string();

        match seize_backend.run_once(50)? {
            Some(report) => {
                tui.update(&report);
                log_debug!("cli", "report: {:?}", report);
                let app_id = focused.as_ref().map(|a| a.id.as_str());
                let cfg = config.lock().unwrap_or_else(|e| e.into_inner());
                if let Err(e) = dispatcher.dispatch(&report, &cfg, app_id, &injector) {
                    log_warn!("cli", "dispatch error: {}", e);
                }
            }
            None => {}
        }
        write!(stdout, "{}", tui.render())?;
        stdout.flush()?;
    }

    // Restore cursor
    write!(stdout, "{}\n", crate::tui::SHOW)?;
    stdout.flush()?;

    seize_backend.release();
    log_info!("cli", "seize released");
    println!("Seize released.");
    Ok(())
}

fn running() -> bool {
    static FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Don't panic if the handler can't be installed — just warn and fall
        // back to "stop with kill/SIGTERM". The IOKitManager Drop still releases
        // the device on exit, so this is non-fatal.
        if let Err(e) =
            ctrlc::set_handler(|| FLAG.store(false, std::sync::atomic::Ordering::Relaxed))
        {
            log_warn!("cli", "could not install Ctrl+C handler: {} (use kill to stop)", e);
        }
    });
    FLAG.load(std::sync::atomic::Ordering::Relaxed)
}
