---
title: "Getting Started"
description: "Setup instructions, build steps, and first-run guide for Override Hub"
category: "root"
source_files:
  - "README.md"
  - "Makefile.in"
  - "Makefile"
  - "configure"
  - "Cargo.toml"
  - "Cargo.toml.in"
  - "src/cli.rs"
  - "src/logging.rs"
  - "src/config/manager.rs"
  - "src/config/defaults.rs"
  - "src/tui.rs"
  - "src/lib.rs"
  - "gui/entitlements.plist"
  - "gui/Info.plist"
created: "2026-06-25"
last_updated: "2026-06-25"
---

# Getting Started

Override Hub is a cross-platform application that seizes a Hagibis UC-1102AG USB hub and remaps its physical buttons, rotary knob, and Play/Pause key to arbitrary keyboard shortcuts, media keys, and mouse gestures.

This page guides you through prerequisites, building from source, running for the first time, and understanding the project's conventions.

## Prerequisites

| Dependency | Required For | Notes |
|------------|-------------|-------|
| Rust toolchain (edition 2024) | All builds | Install via [rustup](https://rustup.rs). The crate uses edition 2024. |
| GNU autotools (autoconf-style `./configure`) | All builds | The build system uses a shell-based `./configure` script (not autoconf-generated). |
| Xcode Command Line Tools | macOS builds | Provides `swiftc` and `clang`. Install with `xcode-select --install`. |
| MSVC build tools | Windows builds | Required for Rust on Windows; use the Visual Studio Build Tools installer. |

The `./configure` script checks for these tools at runtime. If `swiftc` is missing on macOS, the build proceeds without the [GUI](../components/swift-gui.md) target and prints a warning.

## Building

All builds start with the `./configure` step, which detects the target platform, locates tools, and generates the `Makefile`, `Cargo.toml`, and (on macOS) `gui/Info.plist` from their `.in` templates.

```bash
./configure
```

Set `RELEASE=1` in the environment for an optimised build:

```bash
RELEASE=1 ./configure
```

### Make Targets

#### `make build`

Compiles the Rust crate only. Produces the [CLI](../modules/cli.md) binary at `target/debug/hagibis_hub_mapper` (or `target/release/hagibis_hub_mapper` with `RELEASE=1`).

```bash
make build
```

#### `make` (default target)

Runs `make build` first, then on macOS (if `swiftc` is available) also builds the native GUI binary at `gui/HagibisMapping`.

```bash
make
```

#### `make gui`

Builds the Rust crate as a static library (`libhagibis_hub_mapper.a`), then compiles the macOS [Swift GUI](../components/swift-gui.md) (`gui/main.swift`) and links it against the Rust library, Objective-C helper objects, and system frameworks (IOKit, CoreGraphics, Foundation, AppKit, Security).

```bash
make gui
```

#### `make dist`

Runs `make gui` first, then packages the GUI binary into a signed `.app` bundle with hardened runtime, generates icons, clears the quarantine attribute, resets TCC database entries, and produces a DMG archive at `gui/HagibisMapping-<version>.dmg`.

```bash
make dist
```

This target is macOS-only.

#### `make install`

On macOS, copies the `.app` bundle to `/Applications`. On other platforms, installs the CLI binary to the configured `$bindir` (default `/usr/local/bin`).

```bash
make install
```

#### `make uninstall`

On macOS, removes `/Applications/HagibisMapping.app`. On other platforms, removes `hagibis_hub_mapper` from `$bindir`.

```bash
make uninstall
```

#### `make clean`

Runs `cargo clean` and removes all GUI build artifacts (binary, `.app` bundle, icons, DMG, object files).

#### `make distclean`

Runs `make clean` then removes the generated `Makefile`, `Cargo.toml`, and `gui/Info.plist`.

#### `make check` / `make test`

Runs `cargo test`.

## Running Locally

### [CLI](../modules/cli.md) / [TUI](../modules/tui.md) Mode

The terminal interface provides a real-time view of HID state with ANSI-rendered borders, bullet indicators for active signals, modifier flags, and the currently focused application.

```bash
./target/debug/hagibis_hub_mapper
```

The TUI shows:

- **App** — name of the currently focused application (resolved from the active per-app profile)
- **Knob & Media** — live consumer report bits for Vol+, Vol-, Mute, and Play/Pause with green/red bullet indicators
- **Keyboard Buttons** — physical state of the hub's two buttons (top-left, bottom-right) and their hold variants, with active keycode detection
- **Modifiers** — active modifier flags (Ctrl, Shift, Option/Alt, Cmd) using Unicode symbols
- **Status** — `SEIZED` (remapping active) or `monitoring` (passive)

Press `Ctrl+C` to stop. The cursor is hidden during TUI rendering and restored on exit.

### First Run

The first time you launch the application on macOS, several steps are required:

1. **Administrator password** — the GUI uses an elevation helper (`gui/elevate.c`) to obtain root privileges needed for USB device seizing.
2. **USB entitlement** — the `.app` bundle includes `com.apple.security.device.usb` in its hardened runtime entitlements.
3. **TCC accessibility permission** — the app injects keyboard and mouse events via CGEvent, which requires Accessibility access in System Settings > Privacy & Security > Accessibility.

The build system attempts to reset the TCC database on each `make dist` to clear stale permission entries:

```bash
tccutil reset Accessibility com.gabrielebaldassarre.override-hub
```

### Logging

Logging is configured through the `[logging]` section of the config file. The log level can be set to `"debug"`, `"info"`, or `"warn"`.

```
[logging]
loglevel = "debug"
```

Log files are written to platform-specific locations:

| Platform | Log Directory |
|----------|---------------|
| macOS | `~/Library/Logs/override-hub/hagibis.log` |
| Windows | `%LOCALAPPDATA%\override-hub\logs\hagibis.log` |

On macOS, the log file is readable by Console.app. On Windows, the log directory is eligible for Storage Sense cleanup.

## Permissions

### macOS

The application requires two distinct sets of privileges:

1. **Root elevation** — seizing a USB HID device from the kernel requires superuser privileges. The GUI uses an `elevate.c` helper invoked before the Swift application starts. On the CLI, the binary must be run as root (or via `sudo`).
2. **TCC Accessibility permission** — CGEvent keyboard/mouse injection requires the `com.apple.security.device.usb` entitlement and the Accessibility permission in System Settings.

The `make dist` target signs the bundle with hardened runtime and the USB entitlement, then resets TCC records to prompt the user fresh.

### Windows

Seizing HID devices and injecting input via `SendInput` requires administrator privileges. The binary must be run in an elevated command prompt or configured to always run as administrator.

## Configuration

The [configuration file](../modules/config.md) is auto-generated on first run with documented defaults that include inline comments explaining every field.

### Location

| Platform | Config Directory |
|----------|-----------------|
| macOS | `~/Library/Application Support/override-hub/config.toml` |
| Windows | `%APPDATA%\override-hub\config.toml` |

### Structure

The [config file](../config/config-toml.md) has four sections:

- `[device]` — USB HID device identification and seizing rules (VID:PID, usage page, usage)
- `[default]` — default key bindings for all hub controls (button presses, holds, knob, Play/Pause)
- `[[profiles]]` — per-application profile overrides (optional, advanced)
- `[logging]` — log level and output configuration

Each binding entry must specify a `type`:

| Type | Description |
|------|-------------|
| `Keyboard` | Single keystroke or modifier+key combination (e.g. `"Shift+Cmd+N"`) |
| `MediaKey` | Media key by numeric code (0=Vol+, 1=Vol-, 7=Mute, 16=Play/Pause) |
| `SystemEvent` | System events by numeric subtype (11=Sleep, 12=Restart, 13=Shutdown) |
| `MouseMove` | Relative mouse movement (dx, dy floats) |
| `MouseClick` | Mouse button click with optional absolute coordinates |
| `MouseScroll` | Scroll wheel delta (dx, dy floats) |

### Security

On Unix systems, the config file must not be group- or world-writable. The `load_or_default` function in `src/config/manager.rs` checks file permissions via `fstat` and refuses to load any config with mode bits `0o022` set. This prevents an unprivileged local attacker from injecting arbitrary keystrokes into the privileged session.

File writes use `O_NOFOLLOW` to prevent symlink-based redirection and `fchmod` to set `0o600` atomically on the open file descriptor, avoiding TOCTOU races.

### Hot-Reload

The configuration is loaded once at startup in CLI mode. To apply changes, restart the application. The GUI editor (click any pill in the Show Monitor window) saves changes to the config file.

## Project Conventions

- **Build system** — autotools-style `./configure` + `make` pattern. The `configure` script is a plain POSIX shell script, not autoconf-generated.
- **Crate type** — the Rust crate builds as a library (`lib`, `staticlib`, `cdylib`) so the macOS GUI can link against it statically. The `staticlib` is consumed by the Swift GUI; the `cdylib` is available for other FFI consumers.
- **Panic strategy** — both debug and release profiles pin `panic = "unwind"`. This is a safety requirement: the device-seizing logic (`IOKitManager` Drop handler) and engine thread use `catch_unwind` to ensure the USB device is always released, even on panics. Aborting would leave the device seized until physical unplug.
- **Platform backends** — platform-specific logic lives in `src/backend/<platform>/` behind trait abstractions in `src/backend/traits/`. Each platform provides a seize manager, an injector, and a focus query implementation.
- **Error handling** — a unified `Error` enum in `src/error.rs` covers seize, inject, config, I/O, and generic errors, with `From<std::io::Error>` conversion.
- **Logging** — custom file-based logging (not `env_logger`/`log` crate). Thread-safe via a global `Mutex<LogState>`. Supports four levels (Error, Warn, Info, Debug) with structured timestamp prefixes.
- **TUI rendering** — the terminal UI uses raw ANSI escape codes (not a TUI framework like ratatui). It renders box-drawing characters, colored bullets, and modifier Unicode symbols.
- **Versioning** — version is derived from the most recent git tag (stripped of `v` prefix) during `./configure`, with `VERSION` environment variable override.
