use std::io::Read;
use std::path::Path;

use crate::error::Error;

use super::defaults;
use super::types::Config;

/// Reject a config file that is writable by group or others.
///
/// SECURITY: the engine runs with elevated privileges and injects the key
/// combinations defined in this file. A world- or group-writable config would
/// let an unprivileged local attacker inject arbitrary keystrokes (effectively
/// arbitrary commands) into the privileged session. We refuse such files.
///
/// The check runs on an already-open file descriptor (fstat), and the caller
/// reads from that SAME descriptor — so there is no time-of-check/time-of-use
/// window in which the path could be swapped (e.g. via a symlink) between the
/// permission check and the read.
#[cfg(unix)]
fn is_safe_file(file: &std::fs::File) -> bool {
    use std::os::unix::fs::MetadataExt;
    match file.metadata() {
        // Reject if group-writable (0o020) or world-writable (0o002)
        Ok(meta) => meta.mode() & 0o022 == 0,
        // Can't stat the open fd — be conservative and refuse.
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_safe_file(_file: &std::fs::File) -> bool { true }

/// Acquire an advisory exclusive lock on the config directory.
///
/// The lock file descriptor is held for the duration of the config operation
/// (load or save). If the lock cannot be acquired, another instance is
/// presumed active — callers SHOULD use built-in defaults and refuse to
/// read/write the config file.
fn lock_config_dir(path: &Path) -> Result<std::fs::File, std::io::Error> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let dir_file = std::fs::File::open(dir)?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = dir_file.as_raw_fd();
        unsafe extern "C" { fn flock(fd: i32, operation: i32) -> i32; }
        const LOCK_EX: i32 = 0x02;
        const LOCK_NB: i32 = 0x04;
        if unsafe { flock(fd, LOCK_EX | LOCK_NB) } == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(dir_file)
}

/// Load config from `path`.  If the file doesn't exist, write the
/// documented defaults (with inline comments) and return them.
pub fn load_or_default(path: &Path) -> Config {
    // Try to acquire the config directory lock. If another instance holds it,
    // use built-in defaults without touching the config file at all.
    let _lock = match lock_config_dir(path) {
        Ok(l) => Some(l),
        Err(_) => {
            eprintln!("another override-hub instance is running — using built-in defaults");
            crate::logging::warn_log("config", "directory lock held by another instance, using defaults");
            return toml::from_str::<Config>(&defaults::default_config_toml())
                .expect("default config TOML is valid");
        }
    };

    // Open ONCE; check permissions and read from the same descriptor.
    match std::fs::File::open(path) {
        Ok(mut file) => {
            if !is_safe_file(&file) {
                eprintln!(
                    "SECURITY: config file {} is group/world-writable — refusing to load it. \
                     Run: chmod 600 {}",
                    path.display(), path.display()
                );
                crate::logging::error_log("config", "refusing group/world-writable config");
                // Fall back to built-in defaults WITHOUT reading the untrusted
                // file and WITHOUT overwriting it.
                return toml::from_str::<Config>(&defaults::default_config_toml())
                    .expect("default config TOML is valid");
            }

            let mut content = String::new();
            match file.read_to_string(&mut content) {
                Ok(_) => match toml::from_str::<Config>(&content) {
                    Ok(cfg) => return cfg,
                    Err(e) => eprintln!("config parse error ({}), regenerating", e),
                },
                Err(e) => eprintln!("config read error ({}), regenerating", e),
            }
            // fall through to regenerate defaults below
        }
        // Missing or unreadable — generate defaults below.
        Err(_) => { /* generate below */ }
    }

    // Write the raw default TOML (with comments/docs) to disk.
    // Open with O_NOFOLLOW so a symlink at the config path cannot redirect the
    // write to an arbitrary file — the root-privileged engine must not be
    // tricked into overwriting system files via a symlink swap.
    let raw = defaults::default_config_toml();
    if let Err(e) = write_config_file(path, &raw) {
        // Silently skip — we already have the parsed defaults in memory; the
        // file just remains absent until the user creates it manually.
        eprintln!("failed to write default config ({}), using built-in defaults", e);
    } else {
        eprintln!("default config written to {}", path.display());
    }

    toml::from_str::<Config>(&raw).expect("default config TOML is valid")
}

/// Write `config` to `path` as TOML (used by GUI/config reload).
pub fn save(config: &Config, path: &Path) -> Result<(), Error> {
    let _lock = lock_config_dir(path).map_err(|e| {
        Error::Io(std::io::Error::new(std::io::ErrorKind::WouldBlock,
            format!("another override-hub instance is running: {}", e)))
    })?;
    let toml_str = toml::to_string_pretty(config).map_err(|e| Error::Config(e.to_string()))?;
    // `write_config_file` opens with O_NOFOLLOW on unix — symlink-safe.
    write_config_file(path, &toml_str).map_err(Error::Io)?;
    Ok(())
}

/// Write `content` to `path` using an atomic write-rename pattern:
/// 1. Write to a temporary file next to `path`
/// 2. fsync + fchmod the temp file
/// 3. Atomically rename the temp file over `path`
///
/// On unix the temp file is opened with O_NOFOLLOW | O_EXCL so a symlink at
/// the temp path cannot redirect the write. The file gets 0o600 permissions
/// via fchmod on the open fd (avoids a separate path-based TOCTOU).
#[cfg(unix)]
fn write_config_file(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    const O_NOFOLLOW: i32 = 0x0100;
    const O_EXCL: i32 = 0x0200;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_path = path.with_extension(format!("toml.{:x}.tmp", nanos));

    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .custom_flags(O_NOFOLLOW | O_EXCL)
            .open(&tmp_path)?;

        file.write_all(content.as_bytes())?;
        file.flush()?;
        file.sync_all()?;

        // fchmod on the open fd — no path-based TOCTOU.
        // Declared inline to avoid pulling in the libc crate.
        let fd = file.as_raw_fd();
        unsafe extern "C" { fn fchmod(filedes: i32, mode: u16) -> i32; }
        let _ = unsafe { fchmod(fd, 0o600) };
        drop(file);

        std::fs::rename(&tmp_path, path)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}

#[cfg(not(unix))]
fn write_config_file(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_path = path.with_extension(format!("toml.{:x}.tmp", nanos));

    let result = (|| {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        std::fs::rename(&tmp_path, path)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_or_default_creates_file() {
        let dir = std::env::temp_dir().join("override-hub-test-load");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let config = load_or_default(&path);
        // It parsed the built-in defaults
        assert!(!config.default.button_top_left.is_none());
        // File was created
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("override-hub-test-save");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        // Start from defaults
        let mut config = load_or_default(&path);
        config.logging.loglevel = "warn".into();
        save(&config, &path).unwrap();

        let loaded = load_or_default(&path);
        assert_eq!(loaded.logging.loglevel, "warn");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_safe_file_rejects_world_writable() {
        let dir = std::env::temp_dir().join("override-hub-test-perm");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.toml");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut file = std::fs::File::create(&path).unwrap();
            file.write_all(b"x").unwrap();
            file.flush().unwrap();
            // Set world-writable
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o622)).unwrap();

            let file = std::fs::File::open(&path).unwrap();
            assert!(!is_safe_file(&file));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_safe_file_accepts_owner_only() {
        let dir = std::env::temp_dir().join("override-hub-test-safe");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("good.toml");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut file = std::fs::File::create(&path).unwrap();
            file.write_all(b"x").unwrap();
            file.flush().unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

            let file = std::fs::File::open(&path).unwrap();
            assert!(is_safe_file(&file));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Atomic write: temp file cleanup ──────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn atomic_write_no_temp_leftover() {
        let dir = std::env::temp_dir().join("override-hub-test-atomic");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        let mut config = load_or_default(&path);
        config.logging.loglevel = "info".into();
        save(&config, &path).unwrap();

        // No .tmp files should remain after a successful save.
        for entry in std::fs::read_dir(&dir).unwrap() {
            let e = entry.unwrap();
            assert!(
                !e.file_name().to_string_lossy().ends_with(".tmp"),
                "stale tmp file left behind: {:?}",
                e.file_name()
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Atomic write: rename failure cleans up temp ──────────────────────

    #[cfg(unix)]
    #[test]
    fn atomic_write_rename_failure_cleans_up_temp() {
        let dir = std::env::temp_dir().join("override-hub-test-atomic-fail");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        // Make the destination a directory so rename fails (can't rename
        // a regular file over an existing directory).
        std::fs::create_dir(&path).unwrap();

        let result = write_config_file(&path, "# test\n");
        assert!(
            result.is_err(),
            "write_config_file should fail when rename target is a directory"
        );

        // No .tmp files should linger.
        for entry in std::fs::read_dir(&dir).unwrap() {
            let e = entry.unwrap();
            assert!(
                !e.file_name().to_string_lossy().ends_with(".tmp"),
                "tmp file should be cleaned up on error: {:?}",
                e.file_name()
            );
        }

        // The original directory must still exist (original state preserved).
        assert!(path.is_dir(), "original path must be intact after failed write");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── fchmod: config file gets mode 0o600 ──────────────────────────────

    #[cfg(unix)]
    #[test]
    fn save_config_sets_mode_600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join("override-hub-test-fchmod");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        let mut config = load_or_default(&path);
        config.logging.loglevel = "debug".into();
        save(&config, &path).unwrap();

        let meta = std::fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode();
        assert_eq!(
            mode & 0o777, 0o600,
            "config file should be mode 0600, got {:o}",
            mode & 0o777
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Directory lock: concurrent load falls back to defaults ───────────

    #[cfg(unix)]
    #[test]
    fn dir_lock_prevents_concurrent_config_read() {
        let dir = std::env::temp_dir().join("override-hub-test-dirlock-read");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        // Write a config with a value DIFFERENT from the built-in default.
        // Default loglevel is "debug"; save "warn" so we can detect the fallback.
        let mut config = load_or_default(&path);
        config.logging.loglevel = "warn".into();
        save(&config, &path).unwrap();

        // Hold the advisory directory lock explicitly.
        let _lock = lock_config_dir(&path).expect("should acquire directory lock");

        // load_or_default tries to acquire the lock (LOCK_NB → fails) and must
        // fall back to built-in defaults (which have loglevel = "debug").
        let loaded = load_or_default(&path);
        assert_eq!(
            loaded.logging.loglevel, "debug",
            "should fall back to defaults (loglevel=debug) when lock is held, got {}",
            loaded.logging.loglevel
        );

        drop(_lock);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Directory lock: concurrent save returns error ────────────────────

    #[cfg(unix)]
    #[test]
    fn dir_lock_prevents_concurrent_save() {
        let dir = std::env::temp_dir().join("override-hub-test-dirlock-save");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        let mut config = load_or_default(&path);
        config.logging.loglevel = "warn".into();
        save(&config, &path).unwrap();

        // Hold the directory lock.
        let _lock = lock_config_dir(&path).expect("should acquire directory lock");

        // save() tries to acquire the lock with LOCK_NB → fails.
        config.logging.loglevel = "debug".into();
        let result = save(&config, &path);
        assert!(
            result.is_err(),
            "save should return error when directory lock is held by another fd"
        );

        drop(_lock);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
