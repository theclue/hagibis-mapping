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

/// Load config from `path`.  If the file doesn't exist, write the
/// documented defaults (with inline comments) and return them.
pub fn load_or_default(path: &Path) -> Config {
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
    let toml_str = toml::to_string_pretty(config).map_err(|e| Error::Config(e.to_string()))?;
    // `write_config_file` opens with O_NOFOLLOW on unix — symlink-safe.
    write_config_file(path, &toml_str).map_err(Error::Io)?;
    Ok(())
}

/// Write `content` to `path`, using O_NOFOLLOW on unix so a symlink at the
/// path cannot redirect the write. Sets 0o600 permissions atomically via fchmod
/// on the open fd (avoids a separate path-based set_permissions race).
#[cfg(unix)]
fn write_config_file(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    // O_NOFOLLOW = 0x0100 on macOS / BSD / Linux.
    const O_NOFOLLOW: i32 = 0x0100;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)?;

    file.write_all(content.as_bytes())?;
    file.flush()?;

    // fchmod on the open fd — no path-based TOCTOU.
    // Declared inline to avoid pulling in the libc crate.
    let fd = file.as_raw_fd();
    unsafe extern "C" { fn fchmod(filedes: i32, mode: u16) -> i32; }
    let _ = unsafe { fchmod(fd, 0o600) };
    Ok(())
}

#[cfg(not(unix))]
fn write_config_file(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)
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
}
