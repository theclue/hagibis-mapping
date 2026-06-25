---
title: "Configuration Security"
description: "Security model protecting the config file from tampering and disclosure in the elevated-privilege engine"
category: "concepts"
source_files:
  - "src/config/manager.rs"
  - "src/ffi.rs"
created: "2026-06-25"
last_updated: "2026-06-25"
---

# Configuration Security

## Threat Model

The override-hub engine runs with [elevated privileges](../concepts/anti-zombie.md) (root on macOS) in order to seize HID devices and inject keystrokes. This creates a critical security surface: the configuration file defines which keystroke combinations the engine injects. An attacker who can modify the config file can inject arbitrary keystrokes — effectively arbitrary commands — into the privileged session.

Three attack vectors are defended against:

1. **Symlink redirection**: A user-writable symlink at the config path could redirect writes to an attacker-chosen file (e.g. `/etc/sudoers`), overwriting system files with root privileges.
2. **Permission-based tampering**: A group- or world-writable config file lets any unprivileged local user inject keystrokes into the root-privileged session.
3. **Disclosure**: The config reveals key bindings; other users on the system should not be able to read it.

All three are mitigated at the filesystem level in the [config module](../modules/config.md)'s `manager.rs`.

## Mitigation 1: O_NOFOLLOW on Writes

When the engine writes a config file (either the initial default or a save from the GUI), it opens the file with the `O_NOFOLLOW` flag (value `0x0100` on macOS, BSD, and Linux). If the path resolves through a symlink, the `open()` call fails instead of following the link.

```rust
const O_NOFOLLOW: i32 = 0x0100;

let mut file = std::fs::OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)
    .custom_flags(O_NOFOLLOW)
    .open(path)?;
```

This prevents a classic TOCTOU / symlink-swap attack: an unprivileged user cannot plant a symlink at `~/Library/Application Support/override-hub/config.toml` pointing to a system file (the [`config.toml`](../config/config-toml.md) file stores all mappings) and wait for the root engine to overwrite it.

The flag is set via `custom_flags` (from `std::os::unix::fs::OpenOptionsExt`), which is a unix-only extension. On non-unix platforms, `write_config_file` falls back to plain `std::fs::write`.

**Source**: `src/config/manager.rs`, lines 94–119.

## Mitigation 2: fchmod 0o600 on New Config Files

After writing and flushing the config file content, the engine sets its permissions to `0o600` (owner read/write only) using `fchmod` on the **open file descriptor** — not on the path. This avoids a path-based TOCTOU race where the file at the path could be swapped between the write and the permission set.

```rust
let fd = file.as_raw_fd();
unsafe extern "C" { fn fchmod(filedes: i32, mode: u16) -> i32; }
let _ = unsafe { fchmod(fd, 0o600) };
```

The `fchmod` call is declared inline as an `unsafe extern "C"` FFI function to avoid pulling in the `libc` crate as a dependency. The `0o600` mode ensures that only the file's owner (root) can read or write the config, preventing both tampering and disclosure by other users.

**Source**: `src/config/manager.rs`, lines 113–117.

## Mitigation 3: is_safe_file() Permission Check on Reads

Every time the engine loads the config via `load_or_default()`, it opens the file and immediately checks its permissions using `is_safe_file()`. This function returns `false` if the file has group-writable (`0o020`) or world-writable (`0o002`) bits set:

```rust
fn is_safe_file(file: &std::fs::File) -> bool {
    match file.metadata() {
        Ok(meta) => meta.mode() & 0o022 == 0,
        Err(_) => false,
    }
}
```

If the file fails the check, the engine prints a security warning, logs the event, and **falls back to built-in defaults** — it does not read the untrusted file and does not overwrite it. This is a deliberate fail-closed design.

The check operates on an **already-open file descriptor** via `fstat`, and the caller reads from that same descriptor. This eliminates the TOCTOU window between checking the path and reading the content: the file at the path could be swapped, but the open descriptor still refers to the original inode.

```rust
match std::fs::File::open(path) {
    Ok(mut file) => {
        if !is_safe_file(&file) {
            // refuse to load — use defaults
            return toml::from_str::<Config>(&defaults::default_config_toml())
                .expect("default config TOML is valid");
        }
        // read from the SAME file handle
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        // parse and return
    }
}
```

On non-unix platforms, `is_safe_file` unconditionally returns `true` (the permission model is not applicable).

**Source**: `src/config/manager.rs`, lines 20–51.

## Permission Checking Flow

The complete flow when the engine starts:

```
hagibis_start()
  └─ config_dir() ──→ real_home() + "/Library/Application Support/override-hub"
       └─ load_or_default(path)
            ├─ File::open(path) ──→ OPEN file descriptor
            ├─ is_safe_file(&file) ──→ fstat on OPEN fd
            │    ├─ PASS ──→ read config from fd, parse TOML
            │    └─ FAIL ──→ log warning, return built-in defaults
            └─ File not found ──→ write_config_file(path)
                 ├─ O_NOFOLLOW open
                 ├─ write + flush
                 └─ fchmod 0o600
```

Both writes (initial default creation and `save()` from the GUI) go through `write_config_file`, which always uses `O_NOFOLLOW` + `fchmod 0o600`. Both reads go through `load_or_default`, which gates on `is_safe_file`.

## real_home() Utility

When the engine runs as root, `$HOME` resolves to `/var/root`, not the logged-in user's home. This would cause config and log paths to diverge between the GUI (running as the user) and the engine (running as root). The `real_home()` function in the [FFI module](../modules/ffi.md) (`src/ffi.rs`) solves this (macOS only):

```rust
fn real_home() -> std::path::PathBuf {
    if unsafe { libc_geteuid() } != 0 {
        // Not root — use HOME env var directly
        if let Ok(h) = std::env::var("HOME") {
            if !h.is_empty() { return std::path::PathBuf::from(h); }
        }
    }
    // Running as root — find the real user via /dev/console owner
    if let Ok(meta) = std::fs::metadata("/dev/console") {
        let uid = meta.uid();
        if uid != 0 {
            if let Some(home) = home_for_uid(uid) {
                return home;
            }
        }
    }
    // Final fallback
    std::env::var("HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}
```

The algorithm:

1. If the effective UID is not root, return `$HOME` directly.
2. If root, stat `/dev/console` to get the owning UID of the currently-logged-in console user.
3. Call `getpwuid_r` (via raw FFI) to look up that UID's password database entry and extract `pw_dir`.
4. If any step fails, fall back to `$HOME` or `"."`.

Config and log directories are derived from this:

```rust
fn config_dir() -> std::path::PathBuf {
    real_home().join("Library/Application Support/override-hub")
}

fn log_dir() -> std::path::PathBuf {
    real_home().join("Library/Logs/override-hub")
}
```

On non-macOS platforms, `real_home()` simply returns `$HOME` or `"."`.

**Source**: `src/ffi.rs`, lines 119–200.

## Summary

| Mitigation | Mechanism | Scope | Attack Prevented |
|---|---|---|---|
| `O_NOFOLLOW` | `custom_flags(0x0100)` on write | Unix | Symlink redirection of writes |
| `fchmod 0o600` | `fchmod(fd, 0o600)` on open fd | Unix | Permission-based tampering and disclosure |
| `is_safe_file()` | `mode & 0o022 == 0` on read | Unix | Loading a compromised config |
| `real_home()` | `getpwuid_r` on `/dev/console` owner | macOS | Config path mismatch when running as root |

All unix-specific mitigations are gated behind `#[cfg(unix)]` and are no-ops on other platforms. The design preference is to operate on open file descriptors rather than paths, eliminating TOCTOU races at every step.
