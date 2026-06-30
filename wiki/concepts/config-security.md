---
title: "Configuration Security"
description: "Security model protecting the config file from tampering and disclosure in the elevated-privilege engine"
category: "concepts"
source_files:
  - "src/config/manager.rs"
  - "src/ffi.rs"
  - "src/logging.rs"
created: "2026-06-25"
last_updated: "2026-06-30"
---

# Configuration Security

## Threat Model

The override-hub engine runs with [elevated privileges](../concepts/anti-zombie.md) (root on macOS) in order to seize HID devices and inject keystrokes — see [architecture](../architecture.md) for how this fits into the overall system design. This creates a critical security surface: the configuration file defines which keystroke combinations the engine injects. An attacker who can modify the config file can inject arbitrary keystrokes — effectively arbitrary commands — into the privileged session.

Four attack vectors are defended against:

1. **Symlink redirection**: A user-writable symlink at the config path could redirect writes to an attacker-chosen file (e.g. `/etc/sudoers`), overwriting system files with root privileges.
2. **Permission-based tampering**: A group- or world-writable config file lets any unprivileged local user inject keystrokes into the root-privileged session.
3. **Disclosure**: The config reveals key bindings; other users on the system should not be able to read it.
4. **Concurrent instance conflict**: Two engine instances racing on the same config file could interleave reads and writes (e.g. one instance reads a partially-written config from another), leading to corrupted state, lost keystroke mappings, or a window where the engine falls back to defaults while another instance still believes its config is active.

All four are mitigated at the filesystem level in the [config module](../modules/config.md)'s `manager.rs`. Additional hardening applies to log files in the [logging module](../modules/logging.md).

## Mitigation 1: O_NOFOLLOW on Writes

When the engine writes a config file (either the initial default or a save from the GUI), it opens the file with the `O_NOFOLLOW` flag (value `0x0100` on macOS, BSD, and Linux). If the path resolves through a symlink, the `open()` call fails instead of following the link.

```rust
const O_NOFOLLOW: i32 = 0x0100;
const O_EXCL: i32 = 0x0200;

let mut file = std::fs::OpenOptions::new()
    .write(true)
    .create(true)
    .custom_flags(O_NOFOLLOW | O_EXCL)
    .open(&tmp_path)?;
```

The write is now performed to a **temporary file** (opened with both `O_NOFOLLOW` and `O_EXCL` — see [Mitigation 6](#mitigation-6-o_excl-on-temp-files)), then atomically renamed over the target. This prevents a classic TOCTOU / symlink-swap attack: an unprivileged user cannot plant a symlink at `~/Library/Application Support/override-hub/config.toml` pointing to a system file (the [`config.toml`](../config/config-toml.md) file stores all mappings) and wait for the root engine to overwrite it.

The flags are set via `custom_flags` (from `std::os::unix::fs::OpenOptionsExt`), which is a unix-only extension. On non-unix platforms, `write_config_file` falls back to a plain atomic write-rename without the flag guards.

**Source**: `src/config/manager.rs`, lines 138–178.

## Mitigation 2: fchmod 0o600 on New Config Files

After writing and flushing the config file content, the engine sets its permissions to `0o600` (owner read/write only) using `fchmod` on the **open file descriptor** — not on the path. This avoids a path-based TOCTOU race where the file at the path could be swapped between the write and the permission set.

```rust
let fd = file.as_raw_fd();
unsafe extern "C" { fn fchmod(filedes: i32, mode: u16) -> i32; }
let _ = unsafe { fchmod(fd, 0o600) };
```

The `fchmod` call is declared inline as an `unsafe extern "C"` FFI function to avoid pulling in the `libc` crate as a dependency. The `0o600` mode ensures that only the file's owner (root) can read or write the config, preventing both tampering and disclosure by other users.

This now operates on the **temporary file** before the atomic rename — see [Mitigation 5](#mitigation-5-atomic-write-rename-pattern) for the full sequence.

**Source**: `src/config/manager.rs`, lines 164–168.

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

## Mitigation 4: Advisory Directory Lock

Before any config read or write, the engine acquires an advisory exclusive lock on the config **directory** using `flock(LOCK_EX | LOCK_NB)`:

```rust
fn lock_config_dir(path: &Path) -> Result<std::fs::File, std::io::Error> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let dir_file = std::fs::File::open(dir)?;
    #[cfg(unix)]
    {
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
```

The lock is **non-blocking** (`LOCK_NB`): if another instance already holds the lock, the call returns immediately with an error instead of waiting. This prevents two engine instances from racing on config reads and writes entirely:

- In `load_or_default()`, a lock failure causes the engine to **use built-in defaults** without touching the config file at all. It logs: `"another override-hub instance is running — using built-in defaults"`.
- In `save()`, a lock failure returns an `Error::Io` with `WouldBlock`, preventing the GUI from overwriting config that another instance may be reading or writing.

The lock file descriptor is held for the duration of the entire config operation (load or save), then dropped — releasing the lock for the next caller.

**Source**: `src/config/manager.rs`, lines 40–55 (lock), 62–70 (usage in `load_or_default`), 120–123 (usage in `save`).

## Mitigation 5: Atomic Write-Rename Pattern

All config writes go through an atomic write-rename pattern that replaces the legacy direct-overwrite approach. The full sequence:

```
1. Open a temporary file next to the target path (e.g. config.toml.<hex-nanos>.tmp)
     with O_NOFOLLOW | O_EXCL
2. Write all content to the temp file
3. flush() — ensure all buffered data is written to the kernel
4. sync_all() (fsync) — force the kernel to flush to disk
5. fchmod(fd, 0o600) — set owner-only permissions on the open fd
6. drop(file) — close the temp file descriptor
7. rename(tmp_path, target_path) — atomically replace the target
```

Step 7 (`rename`) is atomic on POSIX filesystems: any reader at the target path sees either the complete old file or the complete new file — never a partial write. The `fsync` before rename (step 4) ensures the new content is durable on disk before the rename commits, preventing data loss on a crash between rename and the kernel's lazy writeback.

```rust
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

    let fd = file.as_raw_fd();
    unsafe extern "C" { fn fchmod(filedes: i32, mode: u16) -> i32; }
    let _ = unsafe { fchmod(fd, 0o600) };
    drop(file);

    std::fs::rename(&tmp_path, path)
})();
```

On non-unix platforms, the same pattern is followed without the `O_NOFOLLOW | O_EXCL` flags and without `fchmod`.

**Source**: `src/config/manager.rs`, lines 130–202.

## Mitigation 6: O_EXCL on Temp Files

The temporary file used in the atomic write-rename is opened with `O_EXCL` in addition to `O_NOFOLLOW`:

```rust
const O_EXCL: i32 = 0x0200;

.custom_flags(O_NOFOLLOW | O_EXCL)
```

The `O_EXCL` flag (combined with `O_CREAT`, which `create(true)` enables) causes `open()` to fail with `EEXIST` if the temporary file already exists. The temp filename includes a high-resolution timestamp (`<hex-nanos>`), making collision astronomically unlikely, but `O_EXCL` provides a kernel-level guarantee: if an attacker could somehow predict or race the filename, the open still fails rather than opening an existing file (potentially a symlink or hard-link plant).

Together, `O_NOFOLLOW` prevents symlink traversal and `O_EXCL` prevents opening a pre-existing file at the temp path — the write target is guaranteed to be a freshly-created regular file in the config directory.

**Source**: `src/config/manager.rs`, lines 145–158.

## Mitigation 7: Temp File Cleanup

If the atomic write-rename sequence fails at any point (write error, flush error, rename failure), the temporary file is cleaned up:

```rust
if result.is_err() {
    let _ = std::fs::remove_file(&tmp_path);
}
result
```

This prevents stale `.tmp` files from accumulating in the config directory if writes fail (e.g. disk full, permission error, or the rename target being a directory). On success, the `rename` removes the temp file atomically as part of the operation — no cleanup is needed.

Tests verify both scenarios: successful saves leave no `.tmp` artifacts, and rename failures (target is a directory) clean up the temp file while preserving the original state.

**Source**: `src/config/manager.rs`, lines 174–178, tests at lines 288–346.

## Logging Security Hardening

The logging module (`src/logging.rs`) applies similar filesystem-level hardening to log files, which are written with elevated privileges:

| Hardening | Mechanism | Attack Prevented |
|---|---|---|
| `O_NOFOLLOW` on log open | `custom_flags(O_NOFOLLOW)` in `OpenOptions` | Symlink redirection of log writes |
| `flock(LOCK_EX \| LOCK_NB)` | Advisory exclusive lock on the log file descriptor | Concurrent log writes from multiple instances |
| `umask(0o077)` on log directory | Set umask before `create_dir_all`, restore after | Accidental group/other access to `~/Library/Logs/override-hub/` |

The umask ensures the log directory is created with mode `0o700` (owner-only), preventing other users from reading log entries that may contain keystroke mapping information or internal engine state.

**Source**: `src/logging.rs`, lines 46–50 (umask), 70–81 (O_NOFOLLOW + flock).

## Permission Checking Flow

The complete flow when the engine starts:

```
hagibis_start()
  └─ config_dir() ──→ real_home() + "/Library/Application Support/override-hub"
       └─ load_or_default(path)
            ├─ lock_config_dir(path) ──→ flock(LOCK_EX | LOCK_NB) on directory
            │    ├─ ACQUIRED ──→ hold lock for duration
            │    └─ FAILED ──→ log warning, return built-in defaults
            ├─ File::open(path) ──→ OPEN file descriptor
            ├─ is_safe_file(&file) ──→ fstat on OPEN fd
            │    ├─ PASS ──→ read config from fd, parse TOML
            │    └─ FAIL ──→ log warning, return built-in defaults
            └─ File not found ──→ write_config_file(path)
                 ├─ open temp file with O_NOFOLLOW | O_EXCL
                 ├─ write content + flush + sync_all (fsync)
                 ├─ fchmod(fd, 0o600) on temp fd
                 ├─ drop temp fd
                 ├─ rename(temp, path) ──→ atomic replacement
                 └─ on error: remove_file(temp) cleanup
```

Both writes (initial default creation and `save()` from the GUI) go through `write_config_file`, which always uses the full atomic sequence. Both reads go through `load_or_default`, which gates on the directory lock first, then `is_safe_file`.

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
| Advisory directory lock | `flock(LOCK_EX \| LOCK_NB)` on config directory | Unix | Concurrent instance conflicts |
| `O_NOFOLLOW` on writes | `custom_flags(0x0100)` on temp file open | Unix | Symlink redirection of writes |
| `O_EXCL` on temp files | `custom_flags(0x0200)` combined with `O_NOFOLLOW` | Unix | Predicted/raced temp filename attacks |
| Atomic write-rename | Write temp → fsync → fchmod → rename | All | Partial writes, crash-consistency |
| `fchmod 0o600` | `fchmod(fd, 0o600)` on temp fd before rename | Unix | Permission-based tampering and disclosure |
| `is_safe_file()` | `mode & 0o022 == 0` on read | Unix | Loading a compromised config |
| Temp file cleanup | `remove_file(tmp_path)` on write failure | All | Stale temp file accumulation |
| `real_home()` | `getpwuid_r` on `/dev/console` owner | macOS | Config path mismatch when running as root |

All unix-specific mitigations are gated behind `#[cfg(unix)]` and are no-ops on other platforms. The design preference is to operate on open file descriptors rather than paths, eliminating TOCTOU races at every step.

Log files are similarly hardened: `O_NOFOLLOW` prevents symlink redirection of log writes, `flock(LOCK_EX | LOCK_NB)` prevents concurrent log writes, and `umask(0o077)` ensures the log directory is created with mode `0o700`.
