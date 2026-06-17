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

    // Write the raw default TOML (with comments/docs) to disk
    let raw = defaults::default_config_toml();
    if let Err(e) = std::fs::write(path, &raw) {
        eprintln!("failed to write default config: {}", e);
    } else {
        // Tighten permissions to owner-only (0o600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        eprintln!("default config written to {}", path.display());
    }

    toml::from_str::<Config>(&raw).expect("default config TOML is valid")
}

/// Write `config` to `path` as TOML (used by GUI/config reload).
pub fn save(config: &Config, path: &Path) -> Result<(), Error> {
    let toml_str = toml::to_string_pretty(config).map_err(|e| Error::Config(e.to_string()))?;
    std::fs::write(path, toml_str).map_err(Error::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}
