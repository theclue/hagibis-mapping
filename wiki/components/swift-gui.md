---
title: "Swift GUI"
description: "macOS menu-bar application for Hagibis hub binding configuration and live monitoring"
category: "components"
source_files:
  - "gui/main.swift"
  - "gui/bridge.h"
  - "gui/elevate.c"
  - "gui/entitlements.plist"
  - "gui/Info.plist"
  - "gui/Info.plist.in"
  - "gui/hub.png"
  - "gui/icon.png"
  - "gui/AppIcon.icns"
created: "2026-06-25"
last_updated: "2026-06-25"
---

# Swift GUI

## Purpose

Provides a native macOS menu-bar application that lets users monitor and configure the HID hub binding mappings in real time. The app runs with root privileges (via `AuthorizationExecuteWithPrivileges`) to access [IOKit](../modules/backend-macos.md)'s exclusive device seizure, and communicates with the Rust engine through [C FFI calls](../modules/ffi.md).

## Key Files

| File | Role |
|------|------|
| `gui/main.swift` | Entire Swift/AppKit application (618 lines) — AppDelegate, monitor window, binding editor, FFI declarations |
| `gui/elevate.c` | C helper that calls `AuthorizationExecuteWithPrivileges` to re-launch the binary as root |
| `gui/bridge.h` | C header documenting the FFI functions exported by the Rust library |
| `gui/entitlements.plist` | Entitlements XML: `com.apple.security.device.usb` (required for IOKit seize in AEWP context) |
| `gui/Info.plist` | Bundle metadata with `LSUIElement=true` (no Dock icon), bundle identifier `com.gabrielebaldassarre.override-hub` |
| `gui/Info.plist.in` | Template version of Info.plist with `@APP_VERSION@` placeholder substituted at build time |
| `gui/hub.png` | Image of the Hagibis hub device displayed in the monitor window behind the pill overlays |
| `gui/icon.png` | Source icon (512x512) used to generate `AppIcon.icns` via iconutil during `make dist` |
| `gui/AppIcon.icns` | Generated icon set for the `.app` bundle |

## AppDelegate and Status Bar

The application entry point in `main.swift` creates an `NSApplication`, sets its activation policy to `.accessory` (menu-bar only, no Dock icon), and assigns an `AppDelegate` instance.

### Elevation check at startup

Before the app delegate runs, the top-level code checks the effective user ID:

```swift
if geteuid() != 0 {
    if let execPath = Bundle.main.executablePath {
        _ = execPath.withCString { hagibis_elevate($0) }
    }
    exit(0)
}
```

If not already root, it calls `hagibis_elevate` (defined in `elevate.c`) to spawn a privileged copy of itself and then exits. The child process inherits the same binary path and starts with `euid == 0`.

### Status item and menu

In `applicationDidFinishLaunching`, the delegate creates an `NSStatusItem` with a button displaying `"HM"` in monospaced bold font, and attaches an `NSMenu` with:

- **Toggle** (Start/Stop) — calls `hagibis_start()` / `hagibis_stop()` via FFI
- **Show Monitor** — opens the monitor overlay window
- **About** — standard macOS About panel via `NSApp.orderFrontStandardAboutPanel`
- **Quit** — stops the engine and terminates

### Status polling loop

A `Timer` fires every 150ms calling `updateStatus()`, which queries the Rust engine via `hagibis_status_json`. The returned JSON is deserialized with `JSONSerialization` (converting `Int` values through `UInt8(truncatingIfNeeded:)` to avoid fragile `NSNumber` casting). The parsed fields include:

- **running** — engine state (green/red indicator in menu bar)
- **focused_app_id / focused_app_name** — frontmost application
- **consumer_sticky / consumer** — active consumer HID usage (bitfield for knob and play/pause)
- **keyboard_keys** — array of active HID key codes currently pressed on the hub
- **btn_tl, btn_tl_hold, btn_br, btn_br_hold, knob_cw, knob_ccw, knob_click, play_pause** — human-readable binding labels per-control

Each pill in the monitor window is updated with an on/off indicator (`"●"` / `"○"`) and the label string. Active controls are colored `systemGreen`, inactive ones get a dark gray.

When the engine is stopped, the display falls back to hardcoded developer defaults (`devTL = "Cmd+R"`, `devCW = "Vol+"`, etc.).

## Monitor Window with Pill Overlays

The `createWindow()` method builds an `NSWindow` (titled, closable, miniaturizable) containing an `NSImageView` that loads `hub.png` from the app bundle's Resources or from the executable directory. The image is scaled proportionally to a display width of 300pt.

### Pill placement

The `makePill` helper positions eight `NSTextField` labels over the hub image at relative coordinates matching the physical HID controls:

| Pill | Relative Position | Usage |
|------|-------------------|-------|
| `btn_tl` | left 10%, top 12% | Top-left button (tap) |
| `btn_tl_hold` | left 10%, top 22% | Top-left button (hold) |
| `btn_br` | right 10%, top 78% | Bottom-right button (tap) |
| `btn_br_hold` | right 10%, top 88% | Bottom-right button (hold) |
| `knob_ccw` | left 22%, center 48% | Knob counter-clockwise |
| `knob_cw` | left 54%, center 48% | Knob clockwise |
| `knob_click` | left 38%, 58% | Knob press |
| `play_pause` | right 10%, top 12% | Play/pause button |

Each pill consists of a semi-transparent white background `NSView` (cornerRadius 4, 80% opacity) and a monospaced `NSTextField` displaying the on/off indicator and label.

### Click handling

A single `NSClickGestureRecognizer` on the image view handles pill clicks. In the `pillClicked` action, it iterates the image view's subviews in reverse (front-to-back) looking for a background view whose `toolTip` matches a control key name. On hit, it sets `edKey` and opens the binding editor.

### Window lifecycle

The window is created lazily on first "Show Monitor" and held in the `window` property (`isReleasedWhenClosed = false`). Closing the window calls `windowShouldClose` which orders it out and resets the activation policy to `.accessory`.

## Binding Editor

The editor is a modal-like `NSWindow` opened by `openEditor()` when a pill is clicked. It reads the full config from the Rust engine via `hagibis_config_json`, deserializes the JSON into a `[String: Any]` dictionary, and presents three input rows for editing a single binding.

### App profile selector

A `NSPopUpButton` lists "Default" plus all running applications with `.regular` activation policy and non-empty bundle identifiers. The list is supplemented with apps that have profiles in the saved config (in case they are not currently running). On initial open, the dropdown auto-selects the currently focused app if it has a profile, otherwise "Default".

### Event type radio buttons

Three `NSButton(radioButtonWithTitle:...)` controls for the binding type:

- **Keyboard Shortcut** — activates a `KeyCaptureField`
- **Media Key** — activates a dropdown of 9 media key types (Vol+, Vol−, Mute, Play/Pause, Next Track, Previous Track, Brightness+, Brightness−, Eject)
- **Mouse Gesture** — activates a dropdown of 5 gestures (Left Click, Right Click, Middle Click, Scroll Up, Scroll Down)

Only one radio is selectable at a time via the `edTypeChanged` action, which manually toggles the others to `.off` and enables/disables the associated input controls.

### KeyCaptureField

A custom `NSTextField` subclass that intercepts `performKeyEquivalent`, `keyDown`, and `flagsChanged` to capture arbitrary keyboard combos. The `interpret` method builds a combo string from:

- Modifier flags: Ctrl, Shift, Alt, Cmd (in that order)
- Named keys via `keyCode` lookup: Enter, Tab, Space, Backspace, Escape, Delete, Home, End, PageUp, PageDown, F1–F15, ArrowUp/Down/Left/Right
- Single-character keys from `charactersIgnoringModifiers`

The resulting combo string (e.g. `"Cmd+Shift+A"`) is set as the field's `stringValue` and passed to the `onKeyCombo` closure.

### Save logic

The `saveAndCloseEditor` method constructs an event dictionary with the selected type and value, then writes it into the in-memory `edConfig` dictionary — either under the `"default"` key or inside the appropriate per-app profile (creating a new profile if none exists). The updated config is [persisted](../config/config-toml.md) via `hagibis_save_config_json` followed by `hagibis_reload_config`.

## Elevation and Entitlements

### Elevation mechanism

The file `elevate.c` implements the `hagibis_elevate` function:

1. Creates an `AuthorizationRef` with `AuthorizationCreate`
2. Requests the `kAuthorizationRightExecute` right with interaction allowed and pre-authorization
3. Calls `AuthorizationExecuteWithPrivileges` with the binary path and arguments `["OverrideHub", "--elevated"]` (the `--elevated` flag is reserved for future use)
4. Returns 0 on success, -1 on failure

The function carries an intentional `#pragma clang diagnostic ignored "-Wdeprecated-declarations"` because `AuthorizationExecuteWithPrivileges` is deprecated in macOS 10.15+ but still required for this approach.

### [Security](../concepts/config-security.md): no shell injection

Unlike an earlier `osascript`-based approach, `elevate.c` passes the executable path as a direct argument to the Authorization Services API — never through a shell. This eliminates command injection even if the `.app` bundle is located at a path containing shell metacharacters.

### Entitlements

The `entitlements.plist` file contains a single entitlement:

```xml
<key>com.apple.security.device.usb</key>
<true/>
```

The app is explicitly **not** sandboxed (`com.apple.security.app-sandbox` is absent) because App Sandbox blocks `kIOHIDOptionsTypeSeizeDevice`. The USB entitlement is sufficient to grant IOKit HID exclusive access when the process runs in the AEWP (AuthorizationExecuteWithPrivileges) security context.

### Code signing

The `Makefile` `dist` target signs the bundle with:

```
codesign --force --deep --sign - --options runtime --entitlements gui/entitlements.plist
```

- `--sign -` uses ad-hoc signing (no Apple Developer ID certificate required)
- `--options runtime` enables Hardened Runtime, required for IOKit access from AEWP-elevated processes on modern macOS
- `--deep` signs all nested binaries and libraries

After signing, the build verifies that `codesign --display --verbose=4` shows `flags=...runtime` and fails the build if the hardened runtime flag is missing.

### Post-build steps

The `dist` target also:

- Removes quarantine attributes (`xattr -dr com.apple.quarantine`)
- Resets TCC permissions via `tccutil reset` for `ListenEvent`, `Accessibility`, and `All` categories under the bundle identifier

### Residual risk

On macOS 11+ a [root process](../concepts/anti-zombie.md) may not connect to the user session's WindowServer, which would prevent the menu bar from appearing. If this occurs, the recommended architecture is a user-level GUI communicating with a privileged helper via SMJobBless / LaunchDaemon IPC.

## FFI Integration

### Rust library bridge

The Swift code imports the [Rust static library](../dependencies/external-dependencies.md) (`libhagibis_hub_mapper.a`) via `@_silgen_name` declarations that map to C functions documented in `bridge.h`:

| Swift FFI declaration | bridge.h signature | Purpose |
|---|---|---|
| `hagibis_start()` | `int hagibis_start(void)` | Start engine (seize + polling) |
| `hagibis_stop()` | `void hagibis_stop(void)` | Stop engine, release devices |
| `hagibis_is_running()` | `int hagibis_is_running(void)` | Check engine state |
| `hagibis_status_json(buf, size)` | `int hagibis_status_json(char*, int)` | Get status as JSON string |
| `hagibis_config_json(buf, size)` | `int hagibis_config_json(char*, int)` | Get config as JSON string |
| `hagibis_save_config_json(json)` | `int hagibis_save_config_json(const char*)` | Persist config from JSON |
| `hagibis_reload_config()` | (not in bridge.h) | Reload config after save |
| `hagibis_elevate(path)` | (in elevate.c) | Re-launch as root via AEWP |
| `hagibis_log(level, cat, msg)` | (not in bridge.h) | Write to engine log |

### JSON buffer pattern

All string-exchange functions follow the same pattern: Swift allocates a fixed-size `[CChar]` buffer (8192 bytes for status, 65536 for config), passes it as an `UnsafeMutablePointer<CChar>` to the C function, then converts the written bytes to a `String` and parses it with `JSONSerialization`.

```swift
var buf = [CChar](repeating: 0, count: 8192)
let len = buf.withUnsafeMutableBufferPointer { ptr -> Int32 in
    guard let addr = ptr.baseAddress else { return 0 }
    return hagibis_status_json(addr, 8192)
}
```

### Static linking security

The Rust library is linked statically (`.a` file) rather than dynamically (`.dylib`). This eliminates dylib hijacking: because the app runs as root, a user-writable `.dylib` in the bundle would be a privilege-escalation vector. The resulting binary is self-contained (7.9 MB) with no load-time dependencies on the Rust library, verified with `otool -L`.

The linker command in the Makefile combines the Swift object, the compiled `elevate.o`, the Rust static library, and an optional Objective-C helper `.o` file (from a build artifact path) with the required system frameworks: IOKit, CoreGraphics, Foundation, AppKit, and Security.

## Build and Packaging

### Build targets

The Makefile defines two relevant targets:

- **`make gui`** — compiles `elevate.c` with clang, then links everything with `swiftc` into a standalone binary at `gui/HagibisMapping`
- **`make dist`** — runs `make gui`, then assembles the `.app` bundle and optionally creates a `.dmg` disk image

### Bundle structure

```
HagibisMapping.app/
├── Contents/
│   ├── Info.plist              # LSUIElement=true, CFBundleIdentifier, etc.
│   ├── MacOS/HagibisMapping    # Static binary (7.9 MB)
│   ├── Resources/
│   │   ├── AppIcon.icns        # App icon
│   │   └── hub.png             # Hub image for monitor overlay
│   └── _CodeSignature/         # Ad-hoc signature with entitlements
```

### Info.plist properties

- `LSUIElement = true` — no Dock icon, menu-bar only
- `CFBundleIdentifier = com.gabrielebaldassarre.override-hub`
- `CFBundleName = Hagibis Mapping`
- `CFBundleExecutable = HagibisMapping`

The `Info.plist.in` template substitutes `@APP_VERSION@` at configure time.

### DMG creation

The `dist` target builds a read-only compressed DMG (`UDZO` format) by staging the `.app` bundle alongside a symlink to `/Applications`, producing `gui/HagibisMapping-0.1.1.dmg`.

### Install locations

- macOS: `cp -R HagibisMapping.app /Applications/`
- Other platforms: the Rust CLI binary is installed to `$bindir` directly

### Version

Current version: `0.1.1`, sourced from `APP_VERSION` in the Makefile and embedded in both the Rust crate and the Info.plist bundle.
