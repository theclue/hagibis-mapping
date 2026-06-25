---
title: "Logging"
description: "File-based structured logging with level filtering and thread-safe log writing"
category: "modules"
source_files:
  - "src/logging.rs"
created: "2026-06-25"
last_updated: "2026-06-25"
---

# Logging

## Purpose

Provides a lightweight, file-based structured logging system for the application. Logs are written to platform-appropriate locations — readable by Console.app on macOS and eligible for Storage Sense cleanup on Windows. All log writing is thread-safe via a global mutex, and log levels are enforced at write time so lower-priority messages are discarded without formatting cost.

## Key Files

| File | Role |
|------|------|
| `src/logging.rs` | Full implementation: log level enum, global state, init, write functions, and export macros |

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

Initializes the logging system. Creates the log directory if it does not exist, then opens (or creates) `hagibis.log` inside it in append mode. Stores the enabled level and file handle in the global state. Writes a startup message at the Info level.

```rust
pub fn init(level: LogLevel, dir: PathBuf) -> Result<(), Error> {
    std::fs::create_dir_all(&dir).map_err(Error::Io)?;
    let log_path = dir.join("hagibis.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(Error::Io)?;
    // ...
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

| Platform | Directory |
|----------|-----------|
| macOS | `~/Library/Logs/override-hub/` |
| Windows | `%LOCALAPPDATA%\override-hub\logs\` |

On macOS the log file is readable by Console.app. On Windows the directory falls under Storage Sense automatic cleanup.

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

## Thread Safety

A single global `Mutex<Option<LogState>>` guards the log file handle and enabled level. All write functions lock this mutex before accessing state.

```rust
static LOG_STATE: Mutex<Option<LogState>> = Mutex::new(None);
```

- On **poisoning**, the mutex lock uses `into_inner()` to recover the state (in `init`) or simply returns without writing (in `write_log`).
- The lock is released before writing the startup message in `init` to avoid a double-lock deadlock.
- Each write calls `flush()` so logs survive a crash.

## Dependencies

- `crate::error::Error` — used to wrap I/O errors via `Error::Io`.
- `std::fs`, `std::io`, `std::path`, `std::sync::Mutex` — standard library only; no external logging crate.

The log level is configured in the [`config.toml`](../config/config-toml.md) file and managed by the [configuration system](../modules/config.md).
