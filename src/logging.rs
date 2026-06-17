//! File-based structured logging.
//!
//! Writes to `~/Library/Logs/override-hub/` on macOS (readable by Console.app)
//! and `%LOCALAPPDATA%\override-hub\logs\` on Windows (eligible for Storage Sense cleanup).
//!
//! Thread-safe via a global `Mutex<LogState>`.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 0,
    Warn  = 1,
    Info  = 2,
    Debug = 3,
}

static LOG_STATE: Mutex<Option<LogState>> = Mutex::new(None);

struct LogState {
    level: LogLevel,
    file: File,
    start: std::time::Instant,
}

pub fn init(level: LogLevel, dir: PathBuf) -> Result<(), Error> {
    std::fs::create_dir_all(&dir).map_err(Error::Io)?;
    let log_path = dir.join("hagibis.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(Error::Io)?;

    let mut guard = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(LogState {
        level,
        file,
        start: std::time::Instant::now(),
    });
    drop(guard); // release before calling info_log which also locks

    info_log("logging", &format!("log started — level={:?}", level));
    Ok(())
}

pub fn debug(cat: &str, msg: &str) {
    write_log(LogLevel::Debug, cat, msg);
}

pub fn info_log(cat: &str, msg: &str) {
    write_log(LogLevel::Info, cat, msg);
}

pub fn warn_log(cat: &str, msg: &str) {
    write_log(LogLevel::Warn, cat, msg);
}

pub fn error_log(cat: &str, msg: &str) {
    write_log(LogLevel::Error, cat, msg);
}

fn write_log(level: LogLevel, cat: &str, msg: &str) {
    if let Ok(mut guard) = LOG_STATE.lock() {
        if let Some(ref mut state) = *guard {
            if level > state.level {
                return;
            }
            let elapsed = state.start.elapsed();
            let ms = elapsed.as_millis();
            let level_str = match level {
                LogLevel::Error => "ERROR",
                LogLevel::Warn  => "WARN ",
                LogLevel::Info  => "INFO ",
                LogLevel::Debug => "DEBUG",
            };
            let line = format!("{:>8}.{:03} [{}] {}: {}\n", ms / 1000, ms % 1000, level_str, cat, msg);
            let _ = state.file.write_all(line.as_bytes());
            let _ = state.file.flush();
        }
    }
}

// ── Format helpers ────────────────────────────────────────────────────────

pub fn debug_fmt(cat: &str, args: fmt::Arguments) {
    let msg = format!("{}", args);
    debug(cat, &msg);
}

#[macro_export]
macro_rules! log_debug {
    ($cat:expr, $($arg:tt)*) => {
        $crate::logging::debug_fmt($cat, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($cat:expr, $($arg:tt)*) => {
        $crate::logging::info_log($cat, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($cat:expr, $($arg:tt)*) => {
        $crate::logging::warn_log($cat, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($cat:expr, $($arg:tt)*) => {
        $crate::logging::error_log($cat, &format!($($arg)*))
    };
}
