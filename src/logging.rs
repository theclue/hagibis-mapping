//! File-based structured logging.
//!
//! Writes to `~/Library/Logs/override-hub/` on macOS (readable by Console.app)
//! and `%LOCALAPPDATA%\override-hub\logs\` on Windows (eligible for Storage Sense cleanup).
//!
//! Thread-safe via a global `Mutex<LogState>`.
//!
//! # Log rotation
//!
//! On init(), if the existing log file exceeds `MAX_LOG_BYTES`, it is renamed
//! to `hagibis.log.old` (overwriting any previous `.old` file) and a fresh log
//! is started. This prevents unbounded disk usage from a compromised or
//! long-running process.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use crate::error::Error;

/// Maximum log file size before rotation (10 MB).
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

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
    #[cfg(unix)]
    let old_umask = unsafe { umask(0o077) };
    std::fs::create_dir_all(&dir).map_err(Error::Io)?;
    #[cfg(unix)]
    unsafe { umask(old_umask); }

    let log_path = dir.join("hagibis.log");

    // ── Rotate if log file exceeds MAX_LOG_BYTES ─────────────────────────
    // Prevents unbounded disk growth from a long-running or compromised
    // process. Simple rename-to-.old strategy (keeps at most two files: the
    // current file and one .old archive).
    let rotated = fs::metadata(&log_path)
        .ok()
        .filter(|m| m.len() >= MAX_LOG_BYTES)
        .and_then(|_| {
            let old = dir.join("hagibis.log.old");
            fs::rename(&log_path, &old).ok()
        })
        .is_some();

    #[cfg(unix)]
    let file = {
        use std::os::fd::AsRawFd;
        const O_NOFOLLOW: i32 = 0x0100;
        let f = OpenOptions::new()
            .create(true)
            .append(true)
            .custom_flags(O_NOFOLLOW)
            .open(&log_path)
            .map_err(Error::Io)?;
        let fd = f.as_raw_fd();
        if unsafe { flock(fd, LOCK_EX | LOCK_NB) } == -1 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        f
    };
    #[cfg(not(unix))]
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

    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner())
            .as_ref().map(|s| s.file.as_raw_fd());
        if let Some(fd) = fd {
            let _ = unsafe { fchmod(fd, 0o644) };
        }
    }

    info_log("logging", &format!("log started — level={:?}", level));
    if rotated {
        info_log("logging", "old log rotated → hagibis.log.old");
    }
    Ok(())
}

#[cfg(unix)]
unsafe extern "C" {
    fn fchmod(fd: i32, mode: u16) -> i32;
    fn flock(fd: i32, operation: i32) -> i32;
    fn umask(mode: u16) -> u16;
}

#[cfg(unix)]
const LOCK_EX: i32 = 0x02;
#[cfg(unix)]
const LOCK_NB: i32 = 0x04;

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

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── O_NOFOLLOW ───────────────────────────────────────────────────────
    // init() opens the log file with O_NOFOLLOW on unix. If hagibis.log is a
    // symlink, the open must fail (prevents symlink redirect attacks).

    #[cfg(unix)]
    #[test]
    fn init_rejects_symlink_log_path() {
        let dir = std::env::temp_dir().join("override-hub-test-nofollow");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Place a symlink at hagibis.log pointing to a real file.
        let real = dir.join("real.log");
        std::fs::File::create(&real).unwrap();
        let link = dir.join("hagibis.log");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let result = init(LogLevel::Info, dir.clone());
        assert!(
            result.is_err(),
            "init should reject symlink log path due to O_NOFOLLOW"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── flock (advisory exclusive lock) ──────────────────────────────────
    // After init() acquires LOCK_EX on the log file, a second open+flock on
    // the same file must fail with LOCK_NB.

    #[cfg(unix)]
    #[test]
    fn log_file_is_exclusively_locked() {
        use std::os::fd::AsRawFd;

        let dir = std::env::temp_dir().join("override-hub-test-flock");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        init(LogLevel::Info, dir.clone()).expect("init should succeed");

        // Open the same log file from a separate fd.
        let log_path = dir.join("hagibis.log");
        let file = std::fs::File::open(&log_path).expect("log file should exist");
        let fd = file.as_raw_fd();

        let rc = unsafe { flock(fd, LOCK_EX | LOCK_NB) };
        assert_eq!(
            rc, -1,
            "flock should fail because log file is already exclusively locked"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── umask: log directory created with restrictive permissions ───────
    // init() sets umask(0o077) before create_dir_all, so the log directory
    // (and any parent directories created) must be mode 0o700.

    #[cfg(unix)]
    #[test]
    fn init_creates_log_dir_with_mode_700() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join("override-hub-test-log-umask");
        let _ = std::fs::remove_dir_all(&dir);
        // Do NOT pre-create the directory — let init() do it via create_dir_all.

        init(LogLevel::Info, dir.clone()).expect("init should succeed");

        let meta = std::fs::metadata(&dir).expect("log dir should exist");
        let mode = meta.permissions().mode();
        assert_eq!(
            mode & 0o777, 0o700,
            "log directory should be mode 0700 (umask 0077), got {:o}",
            mode & 0o777
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Log rotation ─────────────────────────────────────────────────────
    // When the existing hagibis.log exceeds MAX_LOG_BYTES (10 MB), init()
    // must rename it to hagibis.log.old and start a fresh log file.

    #[test]
    fn init_rotates_when_log_exceeds_max_size() {
        let dir = std::env::temp_dir().join("override-hub-test-rotate");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Create a log file > 10 MB via set_len (sparse — fast).
        let log_path = dir.join("hagibis.log");
        let f = std::fs::File::create(&log_path).unwrap();
        f.set_len(11 * 1024 * 1024).unwrap(); // 11 MB
        drop(f);

        init(LogLevel::Info, dir.clone()).expect("init should succeed");

        // The old file should have been renamed to .old
        assert!(
            dir.join("hagibis.log.old").exists(),
            "old log should be rotated to hagibis.log.old"
        );
        let old_meta = std::fs::metadata(dir.join("hagibis.log.old")).unwrap();
        assert_eq!(
            old_meta.len(), 11 * 1024 * 1024,
            "old rotated log should still be 11 MB"
        );

        // A fresh hagibis.log should exist and be much smaller
        assert!(dir.join("hagibis.log").exists(), "new log file should exist");
        let new_meta = std::fs::metadata(&log_path).unwrap();
        assert!(
            new_meta.len() < 10 * 1024 * 1024,
            "new log should be well under 10 MB, got {} bytes",
            new_meta.len()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_does_not_rotate_small_log() {
        let dir = std::env::temp_dir().join("override-hub-test-no-rotate");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Create a small log file
        let log_path = dir.join("hagibis.log");
        std::fs::write(&log_path, "tiny log\n").unwrap();

        init(LogLevel::Info, dir.clone()).expect("init should succeed");

        // No .old file should have been created
        assert!(
            !dir.join("hagibis.log.old").exists(),
            "small log should not trigger rotation"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Basic sanity ─────────────────────────────────────────────────────

    #[test]
    fn init_creates_log_file() {
        let dir = std::env::temp_dir().join("override-hub-test-init");
        let _ = std::fs::remove_dir_all(&dir);

        init(LogLevel::Info, dir.clone()).expect("init should succeed");
        assert!(dir.join("hagibis.log").exists(), "log file should exist after init");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
