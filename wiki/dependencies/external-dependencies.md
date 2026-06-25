---
title: "External Dependencies"
description: "Third-party crate dependencies and build-time tooling required by the project"
category: "dependencies"
source_files:
  - "Cargo.toml"
  - "Cargo.toml.in"
  - "build.rs"
created: "2026-06-25"
last_updated: "2026-06-25"
---

# External Dependencies

The project maintains a deliberately minimal external dependency footprint. Platform-specific work (HID access, event injection, GUI introspection) is implemented entirely through raw FFI to OS frameworks rather than third-party crates.

## Runtime Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `serde` | 1 | Data model serialization/deserialization (derive feature) |
| `serde_json` | 1 | JSON serialization for configuration files |
| `toml` | 0.8 | TOML parsing for configuration files |
| `ctrlc` | 3 | SIGINT (Ctrl+C) signal handler for graceful shutdown |

## Build Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `cc` | 1 | C/Objective-C compiler wrapper, compiles `nsevent_helper.m` on macOS |

## Dependency Rationale

### serde + serde_json

The configuration subsystem reads and writes device mapping definitions. `serde` provides the derive macro for turning Rust structs into serializable models, and `serde_json` handles the JSON wire format. These are the standard choices in the Rust ecosystem — no lighter-weight alternative exists that offers the same derive-based ergonomics for nested configuration structs.

### toml

The primary application configuration file (`config.toml`) uses TOML as its format. The `toml` crate is the reference implementation in the Rust ecosystem, maintained by the same team that maintains `serde`.

### ctrlc

The daemon process needs to release USB device capture on shutdown so the device is not left in a seized state. `ctrlc` installs a signal handler for SIGINT that triggers the engine's teardown sequence. Alternatives considered and rejected:

- **Manual `libc::signal()` binding**: Equivalent in functionality but requires unsafe code and careful signal-safety reasoning that `ctrlc` encapsulates.
- **`signal-hook`**: More feature-rich (multi-handler, arbitrary signal types) but heavier than needed for a single SIGINT handler.

## Platform FFI (No Crate Dependency)

All platform-specific hardware interaction is done through [raw FFI bindings](../modules/ffi.md) declared inline in the Rust source, with no wrapper crates.

### [macOS Frameworks](../modules/backend-macos.md)

Linked in `build.rs` under `#[cfg(target_os = "macos")]`:

| Framework | Used For |
|-----------|----------|
| `IOKit` | USB device discovery, matching, and capture via IO Kit's I/O registry |
| `CoreGraphics` | Event injection (CGEventPost), display properties |
| `Foundation` | Objective-C runtime bridging, NSString/NSArray wrappers |
| `AppKit` | Accessibility permissions, application-level event taps |

### [Windows Libraries](../modules/backend-windows.md)

Linked in `build.rs` under `#[cfg(target_os = "windows")]`:

| Library | Used For |
|---------|----------|
| `user32` | Window message loop, input simulation (SendInput) |
| `kernel32` | Process/thread management, console control handlers |
| `setupapi` | HID device enumeration (SetupDiGetClassDevs) |
| `hid` | HID report descriptors, device path resolution (HidD_GetAttributes, HidD_GetHidGuid) |

## Notable Absences

### No HID Crate

The Rust ecosystem offers `hidapi` and `hid-rs` for cross-platform HID access. These were not used because:

- **HIDAPI**: Wraps the C `hidapi` library, adding a native build dependency and potential linking issues. The project only needs basic device matching and capture, which `IOKit` (macOS) and `setupapi` + `hid` (Windows) provide with a small amount of unsafe FFI glue.
- **hid-rs / rusb**: Heavier abstractions (USB transfer management, async) that are unnecessary for this project's synchronous device enumeration model.

### No Event Injection Crate

No crate wraps `CGEvent` creation or `SendInput` with the precision needed for Input Map overrides. The project creates synthetic events from scratch with specific field-level control (e.g., `CGEventSetIntegerValueField` for scroll wheel deltas) that no existing crate exposes.

### No GUI / Accessibility Crate

Window focus management and foreground application detection use direct `AppKit` calls (`NSWorkspace.shared.runningApplications`, `NSRunningApplication`) and CoreGraphics calls (`CGWindowListCopyWindowInfo`). No wrapper crate (e.g., `accesskit`) is imported because the scope is limited to reading the active application bundle identifier — not full accessibility tree traversal.
