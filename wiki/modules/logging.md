---
title: "Logging"
description: "File-based structured logging with level filtering, thread-safe log writing, and platform-specific security hardening"
category: "modules"
source_files:
  - "src/logging.rs"
created: "2026-06-25"
last_updated: "2026-06-30"
---

# Logging

## Purpose

Provides a lightweight, file-based structured logging system for the application. Logs are written to platform-appropriate locations — readable by Console.app on macOS and eligible for Storage Sense cleanup on Windows. All log writing is thread-safe via a global mutex, and log levels are enforced at write time so lower-priority messages are discarded without formatting cost. On Unix, filesystem-level security hardening (umask, O_NOFOLLOW, flock, fchmod) prevents unauthorized access and symlink attacks.

## Key Files

| File | Role |
|------|------|
| `src/logging.rs` | Full implementation: log level enum, global state, init, write functions, export macros, and security hardening |

## Public API

### `LogLevel` enum

Ordered by increasing verbosity (derives `PartialOrd` for level comparison):

| Variant | Value | Description |
|---------|-------|-------------|
| `Error` | 0 | Critical errors |
| `Warn`  | 1 | Warning conditions |
| `Info`  | 2 | Informational messages |
| `Debug` | 3 | Diagnostic detail |

### `init(level: LogLevel, dir: PathBuf) -> Result<(), Error>`

Initializes the logging system with platform-specific security hardening and automatic log rotation. On Unix, it performs a four-step hardening sequence before writing any log entries:

1. **`umask(0o077)`** — Sets the process umask to `0o077` before calling `create_dir_all`, ensuring the log directory is created with mode `0o700` (owner-only access). The previous umask is restored immediately after directory creation.

2. **`O_NOFOLLOW`** — Opens (or creates) `hagibis.log` with the `O_NOFOLLOW` flag via `OpenOptionsExt::custom_flags()`. This causes the open to fail with `ELOOP` if the path is a symlink, preventing symlink redirect attacks that could trick the logger into writing to an attacker-chosen file.

3. **`flock(LOCK_EX|LOCK_NB)`** — Acquires an exclusive, non-blocking advisory lock on the log file descriptor. If another process already holds the lock, `init()` returns an error. This prevents concurrent write sharing across processes.

4. **`fchmod(0o644)`** — After the log file is opened and the global state is populated, the file descriptor permissions are set to `0o644` (owner read-write, world readable). This makes the log file accessible to Console.app on macOS while keeping the parent directory at `0o700`.

Log rotation is performed before opening the log file: if the existing `hagibis.log` exceeds `MAX_LOG_BYTES` (10 MB), it is renamed to `hagibis.log.old` (overwriting any previous `.old` file), and a fresh log is started. This prevents unbounded disk growth.

The function stores the enabled level, file handle, and startup timestamp in the global state, then writes a startup message at the `Info` level. If rotation occurred, a second message notes the rotation.

```rust
pub fn init(level: LogLevel, dir: PathBuf) -> Result<(), Error> {
    // ── Step 1: umask (unix only) ──
    #[cfg(unix)]
    let old_umask = unsafe { umask(0o077) };
    std::fs::create_dir_all(&dir).map_err(Error::Io)?;
    #[cfg(unix)]
    unsafe { umask(old_umask); }

    // ── Log rotation ──
    let rotated = /* rename if >= MAX_LOG_BYTES */;

    // ── Steps 2+3: O_NOFOLLOW + flock (unix only) ──
    #[cfg(unix)]
    let file = {
        let f = OpenOptions::new()
            .create(true).append(true)
            .custom_flags(O_NOFOLLOW)
            .open(&log_path)?;
        if unsafe { flock(f.as_raw_fd(), LOCK_EX | LOCK_NB) } == -1 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        f
    };
    #[cfg(not(unix))]
    let file = OpenOptions::new()
        .create(true).append(true)
        .open(&log_path)?;

    // Store state, release mutex
    // ── Step 4: fchmod (unix only) ──
    #[cfg(unix)]
    {
        let _ = unsafe { fchmod(fd, 0o644) };
    }

    info_log("logging", &format!("log started — level={:?}", level));
    Ok(())
}
```

### Level-checked write functions

Each function takes a category string (`cat`) and a message string (`msg`). The category is written into each log line for easy grepping. Messages at a level higher than the configured `LogLevel` (i.e. less severe) are silently dropped.

| Function | Writes at level |
|----------|----------------|
| `error_log(cat, msg)` | `Error` |
| `warn_log(cat, msg)` | `Warn` |
| `info_log(cat, msg)` | `Info` |
| `debug(cat, msg)` | `Debug` |

### `debug_fmt(cat: &str, args: fmt::Arguments)`

Avoids allocating the message string when debug logging is disabled. Used internally by the `log_debug!` macro.

### Export macros

Convenience macros that accept `format_args!`-style formatting, avoiding the need to call `format!` manually:

```rust
log_error!("category", "something failed: {}", err);
log_warn!("category", "retry attempt {}", n);
log_info!("category", "started with config={:?}", config);
log_debug!("category", "state={:?}", state);
```

Each macro calls the corresponding function (`error_log`, `warn_log`, `info_log`, `debug_fmt` respectively).

## Log Destinations per Platform

The file path is supplied by the caller to `init()`, but the module's doc comments document the intended locations:

| Platform | Directory | Notes |
|----------|-----------|-------|
| macOS | `~/Library/Logs/override-hub/` | Dir mode `0o700`; log file mode `0o644` (world-readable for Console.app) |
| Windows | `%LOCALAPPDATA%\override-hub\logs\` | Eligible for Storage Sense automatic cleanup |

On macOS the log file is readable by Console.app (made possible by the `fchmod(0o644)` call after init). The parent directory is restricted to owner-only (`0o700`) via `umask(0o077)`.

## Log Format

Each log line uses elapsed time since initialization (not wall-clock time):

```
{:>8}.{:03} [LEVEL] cat: msg
```

| Component | Example | Description |
|-----------|---------|-------------|
| Elapsed   | `  12.345` | Seconds and milliseconds since `init()` |
| Level     | `[ERROR]`, `[WARN ]`, `[INFO ]`, `[DEBUG]` | Padded to 5 characters |
| Category  | `logging` | Arbitrary string identifying the subsystem |
| Message   | `log started — level=Info` | The formatted message |

### Example line

```
   0.000 [INFO ] logging: log started — level=Info
```

## Log Rotation

On `init()`, if the existing `hagibis.log` file exceeds `MAX_LOG_BYTES` (10 MB), it is renamed to `hagibis.log.old` (overwriting any previous `.old` file) and a fresh log file is created. This simple two-file strategy prevents unbounded disk growth from a compromised or long-running process.

## Thread Safety

A single global `Mutex<Option<LogState>>` guards the log file handle and enabled level. All write functions lock this mutex before accessing state.

```rust
static LOG_STATE: Mutex<Option<LogState>> = Mutex::new(None);
```

- On **poisoning**, the mutex lock uses `into_inner()` to recover the state (in `init`) or simply returns without writing (in `write_log`).
- The lock is released before writing the startup message in `init` to avoid a double-lock deadlock.
- Each write calls `flush()` so logs survive a crash.

On Unix, an additional filesystem-level exclusive lock (`flock` with `LOCK_EX|LOCK_NB`) provides cross-process enforcement: only one process can hold the lock on the log file at a time, preventing concurrent write corruption even if multiple instances of the application are running.

## Security Hardening (Unix only)

These hardening measures mirror the [configuration security model](../concepts/config-security.md) used for config files: both `O_NOFOLLOW` (symlink prevention), `flock` (concurrent-access guard), and `fchmod` (owner-only permissions) are applied identically. The logging module adds `umask(0o077)` on the log *directory* for additional defence-in-depth.

Hardening uses **inline FFI declarations** (no `libc` crate dependency) for three POSIX syscalls:

```rust
#[cfg(unix)]
unsafe extern "C" {
    fn fchmod(fd: i32, mode: u16) -> i32;
    fn flock(fd: i32, operation: i32) -> i32;
    fn umask(mode: u16) -> u16;
}
```

These are called directly in `init()` as part of the four-step process described above. The `O_NOFOLLOW` flag uses `std::os::unix::fs::OpenOptionsExt` (a standard library extension trait, no FFI needed). All hardening is conditionally compiled with `#[cfg(unix)]`, so Windows builds are unaffected.

## Dependencies

- `crate::error::Error` — used to wrap I/O errors via `Error::Io`.
- `std::fs`, `std::io`, `std::path`, `std::sync::Mutex` — standard library only; no external logging crate.
- `std::os::unix::fs::OpenOptionsExt` — for `custom_flags(O_NOFOLLOW)` on Unix (conditional via `#[cfg(unix)]`).

The log level is configured in the [`config.toml`](../config/config-toml.md) file and managed by the [configuration system](../modules/config.md).
