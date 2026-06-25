---
title: "CLI Entry Point"
description: "Main terminal application entry point — orchestration of config loading, backend initialization, event loop, and graceful shutdown"
category: "modules"
source_files:
  - "src/cli.rs"
created: "2026-06-25"
last_updated: "2026-06-25"
---

# CLI Entry Point

## Purpose

The CLI module is the main entry point for the override-hub terminal application. It orchestrates the full application lifecycle: loading [configuration](../modules/config.md), initialising platform-specific [backends](../modules/backend.md) (device capture, input injection, and focus tracking), entering the [engine](../modules/engine.md)'s main event loop that polls for hardware reports and dispatches mapped actions, and performing a clean shutdown on termination. It is the glue layer that ties together all other subsystems.

## Key Files

| File | Role |
|------|------|
| `src/cli.rs` | Contains the public `run()` function and the private `running()` guard for Ctrl+C handling and the `config_dir()`/`log_dir()` path helpers |

## Public API

### `pub fn run() -> Result<(), Error>`

The sole public entry point. Called from the library root (`lib::run()`). It does not return until the user sends SIGINT (Ctrl+C). On success it returns `Ok(())`; on failure it returns a crate-level [`Error`](../modules/error.md).

### Private helpers

| Helper | Description |
|--------|-------------|
| `config_dir() -> Option<PathBuf>` | Resolves the platform-specific configuration directory (`~/Library/Application Support/override-hub` on macOS, `%APPDATA%/override-hub` on Windows, current directory on other platforms) |
| `log_dir() -> PathBuf` | Resolves the log directory (`~/Library/Logs/override-hub` on macOS, `%LOCALAPPDATA%/override-hub/logs` on Windows, current directory on other platforms) |
| `running() -> bool` | Atomic flag guarded by a `Once`-installed Ctrl+C handler; returns `true` until SIGINT is received |

## Lifecycle

The `run()` function follows a strict sequence of phases:

1. **Config resolution** — determine `config_dir()` from platform conventions, create it if absent, load or create the default `config.toml`.
2. **[Logging](../modules/logging.md) initialisation** — read the configured log level from config, initialise the logging subsystem with `log_dir()`.
3. **Backend instantiation** — create platform-specific HID backend, event injector, and focus-query objects via conditional compilation.
4. **Device seize** — call `seize_backend.seize()` to capture the HID device.
5. **[TUI](../modules/tui.md) setup** — create a `TuiState`, set initial `seized = true`, hide the terminal cursor.
6. **Main loop** — poll for hardware reports at ~20 Hz (50 ms interval), resolve the focused application, map reports through the config, dispatch actions, render TUI.
7. **Shutdown** — restore the terminal cursor, release the HID device via `seize_backend.release()`.

## Main Loop

The core loop runs while `running()` returns `true`:

```
loop (at ~50 ms intervals):
  resolve focused app from FocusQuery
  lock config → resolve active ConfigKeyMapper mapping
  update TUI labels (button mappings, knob mappings, focused app name)
  call seize_backend.run_once(50)
  match report:
    Some(report):
      update TUI state from report
      lock config → call dispatcher.dispatch(report, config, app_id, injector)
      log dispatch errors as warnings
    None: (no event this poll cycle)
  write TUI render to stdout
  flush stdout
```

The poll interval is 50 ms, yielding approximately 20 iterations per second. Each iteration re-resolves the focused application and re-reads the configuration, so mapping changes take effect without a restart.

## Ctrl+C Handler

The `running()` function installs a Ctrl+C handler exactly once via `std::sync::Once`:

```rust
static FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static ONCE: std::sync::Once = std::sync::Once::new();
ONCE.call_once(|| {
    if let Err(e) =
        ctrlc::set_handler(|| FLAG.store(false, std::sync::atomic::Ordering::Relaxed))
    {
        log_warn!("cli", "could not install Ctrl+C handler: {} (use kill to stop)", e);
    }
});
FLAG.load(std::sync::atomic::Ordering::Relaxed)
```

Key properties:

- The handler is registered only on the first call to `running()`; subsequent calls are no-ops.
- The `FLAG` atomic bool is set to `false` on SIGINT; the main loop exits on the next poll cycle.
- If the handler cannot be installed (e.g., non-interactive terminal), a warning is logged and the process must be stopped via `kill`/SIGTERM.
- The [`IOKitManager`'s `Drop` implementation](../concepts/anti-zombie.md) still releases the device on abnormal exit, so a missing Ctrl+C handler is non-fatal.

## Backend Instantiation

Backend objects are created via `#[cfg]` conditional compilation, guaranteeing that only the current platform's types are compiled:

### macOS

```rust
#[cfg(target_os = "macos")]
let (mut seize_backend, injector, focus_query) = {
    use crate::backend::macos;
    (macos::IOKitManager::new(), macos::CGEventInjector::new(), macos::NSWorkspaceFocus::new())
};
```

- `IOKitManager` — seizes the HID device via IOKit.
- `CGEventInjector` — injects input events via Core Graphics.
- `NSWorkspaceFocus` — queries the currently focused application via NSWorkspace.

### Windows

```rust
#[cfg(target_os = "windows")]
let (mut seize_backend, injector, focus_query) = {
    use crate::backend::windows;
    (windows::WinHIDManager::new(), windows::SendInputInjector::new(), windows::Win32Focus::new())
};
```

- `WinHIDManager` — seizes the HID device via Windows HID APIs.
- `SendInputInjector` — injects input events via `SendInput`.
- `Win32Focus` — queries the focused window via the Win32 API.

### Other platforms

Unsupported platforms do not compile; there is no fallback implementation.

## Dependencies

```
cli → backend     (HIDBackend trait, FocusQuery trait, platform-specific structs)
cli → config      (manager::load_or_default, resolved config)
cli → engine      (ConfigKeyMapper, Dispatcher)
cli → tui         (TuiState, HIDE/SHOW escape codes)
cli → logging     (init, LogLevel, log_info/log_debug/log_warn macros)
cli → error       (Error type)
```

- **Internal**: backend (`crate::backend`), config (`crate::config`), engine (`crate::engine`), tui (`crate::tui`), logging (`crate::logging`), error (`crate::error`)
- **External**: `ctrlc` (signal handler installation), `std::sync::atomic` (AtomicBool for shutdown flag), `std::sync::{Arc, Mutex, Once}` (shared config and one-shot initialisation)

## Usage Example

The CLI entry point is invoked from the library root:

```rust
// in lib.rs (conceptual)
pub fn run() -> Result<(), crate::error::Error> {
    cli::run()
}
```

The function is the single entry path for the `override-hub` binary; there is no alternative mode of operation.
