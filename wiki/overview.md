---
title: "Override Hub"
description: "Cross-platform application that seizes a Hagibis UC-1102AG USB-C hub's physical controls and remaps them to arbitrary keyboard shortcuts, media keys, and mouse gestures."
category: "root"
source_files:
  - "src/lib.rs"
  - "src/main.rs"
  - "src/cli.rs"
  - "src/ffi.rs"
  - "src/error.rs"
  - "src/config/mod.rs"
  - "src/config/types.rs"
  - "src/config/manager.rs"
  - "src/config/defaults.rs"
  - "src/config/combo.rs"
  - "src/hid/mod.rs"
  - "src/hid/parser.rs"
  - "src/hid/report.rs"
  - "src/hid/keycode.rs"
  - "src/engine/mod.rs"
  - "src/engine/dispatcher.rs"
  - "src/engine/key_mapper.rs"
  - "src/engine/consumer.rs"
  - "src/engine/injector_util.rs"
  - "src/engine/keyboard.rs"
  - "src/backend/mod.rs"
  - "src/backend/traits/mod.rs"
  - "src/backend/traits/seize.rs"
  - "src/backend/traits/inject.rs"
  - "src/backend/traits/focus.rs"
  - "src/backend/macos/mod.rs"
  - "src/backend/macos/seize.rs"
  - "src/backend/macos/inject.rs"
  - "src/backend/macos/focus.rs"
  - "src/backend/macos/ffi.rs"
  - "src/backend/windows/mod.rs"
  - "src/backend/windows/seize.rs"
  - "src/backend/windows/inject.rs"
  - "src/backend/windows/focus.rs"
  - "src/backend/windows/ffi.rs"
  - "src/tui.rs"
  - "src/logging.rs"
  - "Cargo.toml"
  - "gui/main.swift"
  - "gui/bridge.h"
  - "gui/elevate.c"
  - "gui/Info.plist"
  - "gui/entitlements.plist"
created: "2026-06-25"
last_updated: "2026-06-25"
---

# Override Hub

Override Hub is a cross-platform Rust application that takes exclusive control of a **Hagibis UC-1102AG** USB-C hub's physical inputs (two configurable buttons, a rotary knob with push-to-click, and a Play/Pause button) and remaps them to arbitrary keyboard shortcuts, media keys, and mouse gestures. It supports per-application profiles so different programs can have different mappings, and ships with both a [macOS menu-bar GUI (Swift/AppKit)](../components/swift-gui.md) and a [terminal TUI](../modules/tui.md) for monitoring and configuration.

## Key Features

- **Seize and remap** — Takes exclusive [HID](../modules/hid.md) control of the hub's physical controls (2 buttons, rotary knob, Play/Pause) away from the OS and reinterprets every input.
- **Per-app profiles** — Different key mappings for different foreground applications, resolved automatically based on the focused window.
- **[macOS menu-bar GUI](../components/swift-gui.md)** — Native Swift/AppKit application with a live monitor overlay showing current HID state and an inline editor for mappings.
- **[CLI](../modules/cli.md) + [TUI](../modules/tui.md)** — Terminal interface built with ANSI escape codes that renders real-time [HID report](../concepts/hid-protocol.md) state and binding labels; suitable for remote monitoring or headless environments.
- **[C FFI bridge](../modules/ffi.md)** — Exposes the Rust engine to foreign language frontends via `extern "C"` functions (`hagibis_start`, `hagibis_stop`, `hagibis_status_json`, `hagibis_config_json`, `hagibis_save_config_json`, `hagibis_reload_config`).
- **[Platform-specific backends](../modules/backend.md)** — Uses IOKit + CoreGraphics on macOS and RawInput + SendInput on Windows, both abstracted behind a shared trait interface.
- **[TOML configuration](../modules/config.md)** — [Config file](../config/config-toml.md) generated on first run, hot-reloadable without restarting the device seizure.
- **[Safe unwinding](../concepts/anti-zombie.md)** — Explicit `panic = "unwind"` across all profiles to guarantee device release via Drop on panic.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (edition 2024) |
| Build system | Cargo + GNU autotools (`configure` + `make`) |
| Serialization | serde, serde_json, toml |
| Signal handling | ctrlc |
| macOS HID seize | IOKit (`IOKitManager`) |
| macOS key injection | CoreGraphics (`CGEventInjector`) |
| macOS focus detection | NSWorkspace (`NSWorkspaceFocus`) |
| macOS FFI helpers | Objective-C (`nsevent_helper.m`) |
| Windows HID seize | RawInput (`WinHIDManager`) |
| Windows key injection | SendInput (`SendInputInjector`) |
| Windows focus detection | Win32 API (`Win32Focus`) |
| macOS GUI | Swift / AppKit |
| Privilege elevation | C (`elevate.c`) with AuthorizationExecuteWithPrivileges |

## System Overview

```mermaid
graph TD
    subgraph CLI["CLI Entry"]
        main["main.rs"] --> run["lib::run()"]
        run --> cli["cli::run()"]
    end

    subgraph Config["Config Layer"]
        config_mod["config::Config"]
        manager["config::manager"]
        types["config::types"]
        defaults["config::defaults"]
        combo["config::combo"]
        config_mod --> manager
        config_mod --> types
        config_mod --> defaults
        config_mod --> combo
        manager -->|reads/writes| config_toml["config.toml"]
    end

    subgraph HID["HID Layer"]
        hid_mod["hid::Report"]
        parser["hid::parser"]
        report_types["hid::ConsumerReport<br/>hid::KeyboardReport"]
        keycode["hid::KeyCode"]
        hid_mod --> parser
        hid_mod --> report_types
        hid_mod --> keycode
    end

    subgraph Engine["Engine Layer"]
        dispatcher["engine::Dispatcher"]
        key_mapper["engine::key_mapper"]
        consumer["engine::consumer"]
        injector_util["engine::injector_util"]
        keyboard["engine::keyboard"]
        dispatcher --> key_mapper
        dispatcher --> consumer
        dispatcher --> injector_util
        dispatcher --> keyboard
    end

    subgraph Backend["Backend Layer"]
        traits["backend::traits"]
        seize_trait["HIDBackend"]
        inject_trait["Injector"]
        focus_trait["FocusQuery"]
        macos["backend::macos"]
        windows["backend::windows"]
        traits --> seize_trait
        traits --> inject_trait
        traits --> focus_trait
        macos --> traits
        windows --> traits
    end

    subgraph GUI["GUI Layer"]
        ffi["ffi::FFI bridge"]
        swift["Swift AppKit GUI"]
        elevate["elevate.c"]
        ffi --> swift
        ffi --> elevate
    end

    subgraph TUI["Terminal UI"]
        tui_mod["tui::TuiState"]
        logging["logging"]
        error["error::Error"]
    end

    cli --> config_mod
    cli --> hid_mod
    cli --> dispatcher
    cli --> seize_trait
    cli --> inject_trait
    cli --> focus_trait
    cli --> tui_mod
    cli --> logging

    ffi --> config_mod
    ffi --> hid_mod
    ffi --> dispatcher
    ffi --> seize_trait
    ffi --> inject_trait
    ffi --> focus_trait
    ffi --> logging

    dispatcher --> config_mod
    dispatcher --> inject_trait
    dispatcher --> hid_mod
    key_mapper --> config_mod

    parse_config["HID reports"] --> parser
    parser --> hid_mod
    hid_mod --> dispatcher

    seize_trait -->|poll| hid_mod
```

The diagram above shows the layered architecture: the **[CLI](../modules/cli.md)** (`cli::run`) and **[FFI bridge](../modules/ffi.md)** (`ffi.rs`) are the two entry points. Both load the **[Config layer](../modules/config.md)** (TOML-based), instantiate the **[Backend](../modules/backend.md)** (platform-specific seize, inject, focus traits), and run the **[Engine](../modules/engine.md)** (report dispatching and key mapping). The **[HID layer](../modules/hid.md)** parses [raw USB reports](../concepts/hid-protocol.md) into typed structs. The **[GUI](../components/swift-gui.md)** (Swift/AppKit) communicates with the Rust core exclusively through the FFI bridge.

## Project Structure

| Directory / File | Description |
|---|---|
| `Cargo.toml` | Rust package manifest — crate `hagibis_hub_mapper`, edition 2024, targets `lib`/`staticlib`/`cdylib` |
| `configure` / `Makefile.in` | Autotools build system wrapping Cargo, GUI bundling, and DMG creation |
| `src/` | Rust source tree |
| `src/lib.rs` | Library root — re-exports all modules, provides `run()` entry point |
| `src/main.rs` | Binary entry point — calls `lib::run()`, exits on error |
| `src/cli.rs` | CLI/TUI event loop — seizes device, polls HID, dispatches mappings, renders TUI |
| `src/ffi.rs` | C FFI bridge — global engine state, `extern "C"` functions for GUI integration |
| `src/error.rs` | Unified `Error` enum — `Seize`, `Inject`, `Config`, `Io`, `Other` variants |
| `src/logging.rs` | [File-based logger](../modules/logging.md) with log levels (Debug, Info, Warn, Error) |
| `src/config/` | Config layer — TOML file management, data types, keyboard combo parsing |
| `src/hid/` | HID layer — raw report parsing, typed report structs, USB HID keycode definitions |
| `src/engine/` | Engine layer — report dispatching, key mapping resolution, consumer key handling |
| `src/backend/` | Backend layer — abstract traits and platform-specific implementations |
| `src/backend/traits/` | Shared traits: `HIDBackend` (seize/poll), `Injector` (key injection), `FocusQuery` (foreground app) |
| `src/backend/macos/` | macOS backend — IOKit seizure, CGEvent injection, NSWorkspace focus, ObjC helpers |
| `src/backend/windows/` | Windows backend — RawInput seizure, SendInput injection, Win32 focus |
| `src/tui.rs` | Terminal UI renderer — ANSI box-drawing, real-time HID state visualization |
| `gui/` | macOS native GUI (Swift/AppKit) — menu-bar app, live monitor overlay, mapping editor |
| `gui/main.swift` | Swift AppKit application — imports Rust FFI symbols, builds menu bar, editor window, monitor overlay |
| `gui/bridge.h` | C header declaring the FFI functions exported by the Rust library |
| `gui/elevate.c` | Privilege elevation helper using `AuthorizationExecuteWithPrivileges` |
| `gui/Info.plist` / `gui/entitlements.plist` | macOS application bundle metadata and entitlements |
| `gui/AppIcon.icns` / `gui/hub.png` / `gui/icon.png` | Application icons and assets |
| `DEVICE.md` | Technical reference for the Hagibis UC-1102AG USB HID device — chip identification, USB topology, report descriptors, and control behavior |

## Quick Links

- [Architecture](../architecture.md) — system architecture and design decisions
- [Getting Started](../getting-started.md) — setup instructions and build guide
- Modules — detailed documentation for config, HID, engine, backend, and FFI modules
- Glossary — terminology reference
