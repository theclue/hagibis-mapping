---
title: Configuration File Reference
category: config
source_files:
  - src/config/types.rs
  - src/config/defaults.rs
  - src/config/manager.rs
  - src/config/combo.rs
created: 2026-06-25
last_updated: 2026-06-25
---

# Configuration File Reference

The `config.toml` file is the primary way to configure override-hub. It is managed by the [configuration system](../modules/config.md) and controls which USB HID devices are seized, what each hub button and knob does by default, optional per-application overrides, and logging behaviour.

## File Location

The config file is auto-created on startup if it does not exist, using the documented defaults with inline comments. The path depends on the platform:

| Platform | Config directory | Log directory |
|----------|------------------|---------------|
| macOS | `~/Library/Application Support/override-hub/config.toml` | `~/Library/Logs/override-hub/hagibis.log` |
| Windows | `%APPDATA%\override-hub\config.toml` | `%LOCALAPPDATA%\override-hub\logs\hagibis.log` |

The application creates the directory and writes the default `config.toml` on first run. Subsequent runs read the file; parse errors cause a fallback to built-in defaults (the file is not overwritten).

## [Security](../concepts/config-security.md)

The engine runs with elevated privileges (required for HID device access and key injection). The config file is therefore security-sensitive:

- **Permission check (Unix):** On load, the file is rejected if it is group-writable or world-writable (mode bits `0o022`). If rejected, the application falls back to built-in defaults and prints an error message instructing the user to run `chmod 600 config.toml`.
- **O_NOFOLLOW (Unix):** Both reads and writes use `O_NOFOLLOW`, preventing a symlink at the config path from redirecting I/O to an arbitrary file.
- **fd-based check:** The permission check runs on an open file descriptor (`fstat`), and the file content is read from the same descriptor — there is no TOCTOU (time-of-check/time-of-use) window.
- **fchmod:** When writing the default file, permissions are set to `0o600` via `fchmod` on the open fd (no path-based race).

## Top-Level Structure

```toml
[device]
# Device seizing rules (whitelist)

[default]
# Default button and knob mappings

[[profiles]]
# Per-application overrides (optional, repeatable)

[logging]
# Logging configuration
```

## The `[device]` Section

Identifies which USB HID devices and interfaces the application should seize. This is a whitelist — only devices matching at least one `[[device.seize]]` entry are ever touched.

### `[[device.seize]]` entries

Each entry is a `DeviceMatch` struct. Multiple entries use **OR** logic — if any entry matches, the interface is seized. Fields within a single entry are **AND**ed (all must match).

| Field | Type | Description |
|-------|------|-------------|
| `vendor_id` | String (hex) | USB Vendor ID, e.g. `"0x05AC"`. The `0x` prefix is optional. |
| `product_id` | String (hex) | USB Product ID, e.g. `"0x029C"`. The `0x` prefix is optional. |
| `usage_page` | u16 (optional) | HID Usage Page to match. Omit to match any usage page. |
| `usage` | u16 (optional) | HID Usage to match. Omit to match any usage. |

**Default configuration** seizes three interfaces:

1. **Hub Interface 0** (keyboard combos): `vendor_id = "0x05AC"`, `product_id = "0x029C"`, `usage_page = 1` (Generic Desktop), `usage = 6` (Keyboard).
2. **Hub Interface 1** (knob rotate + click): `vendor_id = "0x05AC"`, `product_id = "0x029C"`, `usage_page = 12` (Consumer), `usage = 1` (Consumer Control).
3. **Audio chip** (Play/Pause): `vendor_id = "0x0C76"`, `product_id = "0x1710"`, `usage_page = 12` (Consumer), `usage = 1` (Consumer Control).

## The `[default]` Section

Defines the fallback mapping for every button and knob on the hub. These apply whenever no per-app profile matches the currently focused application. All eight fields are present in the default config.

| Field | Purpose | Default |
|-------|---------|---------|
| `button_top_left` | Tap top-left button | `Shift+Cmd+N` (macOS screenshot) |
| `button_top_left_hold` | Hold top-left button | `Cmd+T` (new tab) |
| `button_bottom_right` | Tap bottom-right button | `Shift+Cmd+4` (macOS area screenshot) |
| `button_bottom_right_hold` | Hold bottom-right button | `Shift+Cmd+3` (macOS full screenshot) |
| `knob_cw` | Rotate knob clockwise | Vol+ (MediaKey `key_type = 0`) |
| `knob_ccw` | Rotate knob counter-clockwise | Vol- (MediaKey `key_type = 1`) |
| `knob_click` | Press knob | Mute (MediaKey `key_type = 7`) |
| `play_pause` | Dedicated play/pause (audio chip) | Play/Pause (MediaKey `key_type = 16`) |

Each field is a TOML table containing a `type` discriminator and type-specific fields. See TargetEvent variants below.

### Example `[default]` section

```toml
[default]

[default.button_top_left]
type = "Keyboard"
binding = "Shift+Cmd+N"

[default.button_top_left_hold]
type = "Keyboard"
binding = "Cmd+T"

[default.button_bottom_right]
type = "Keyboard"
binding = "Shift+Cmd+4"

[default.button_bottom_right_hold]
type = "Keyboard"
binding = "Shift+Cmd+3"

[default.play_pause]
type = "MediaKey"
key_type = 16

[default.knob_cw]
type = "MediaKey"
key_type = 0

[default.knob_ccw]
type = "MediaKey"
key_type = 1

[default.knob_click]
type = "MediaKey"
key_type = 7
```

## TargetEvent Variants

Every button mapping is a `TargetEvent` — a tagged enum serialized with `#[serde(tag = "type")]`. The `type` field is always a string discriminator; additional fields depend on the variant.

### `Keyboard`

Simulates a keyboard shortcut.

```toml
type = "Keyboard"
binding = "Ctrl+Shift+Q"
label = "Quit"     # optional display label
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `binding` | String | Yes | Key combo string. See [Keyboard Binding Syntax](../modules/config-combo.md). |
| `label` | String | No | Human-readable label for display in GUI/TUI. Falls back to `binding` if empty. |

**Modifier names** (case-insensitive):
- `Ctrl` / `Control` — Control modifier
- `Shift` — Shift modifier
- `Alt` / `Option` — Alt/Option modifier
- `Cmd` / `Command` / `Super` / `Meta` / `Win` — Command/Super modifier

**Supported keys** (case-insensitive):
- Single letters: `A`–`Z`
- Digits: `0`–`9`
- Function keys: `F1`–`F15`
- Named keys: `Space`, `Enter`, `Escape`, `Backspace`, `Tab`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, `ArrowLeft`, `ArrowRight`, `ArrowUp`, `ArrowDown`
- Aliases: `Return` (same as `Enter`), `Esc` (same as `Escape`), `Del` (same as `Delete`), `Left`/`Right`/`Up`/`Down` (same as arrow keys)

Whitespace around `+` separators is allowed: `"Ctrl + Shift + N"` parses identically to `"Ctrl+Shift+N"`.

### `MediaKey`

Sends a system media key event (shows OSD on macOS, no visual feedback on Windows).

```toml
type = "MediaKey"
key_type = 16       # Play/Pause
label = "Play/Pause" # optional display label
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `key_type` | u8 | Yes | NX_KEYTYPE constant (see table below) |
| `label` | String | No | Human-readable label. Falls back to a known name if empty. |

**Common `key_type` values:**

| Value | Label | Description |
|-------|-------|-------------|
| `0` | Vol+ | Sound Up |
| `1` | Vol- | Sound Down |
| `2` | Brightness+ | Brightness Up |
| `3` | Brightness- | Brightness Down |
| `4` | Caps Lock | Caps Lock |
| `5` | Help | Help |
| `6` | Power | Power key |
| `7` | Mute | Sound Mute |
| `11` | Contrast+ | Display Contrast Up |
| `12` | Contrast- | Display Contrast Down |
| `13` | Launch Panel | Launch Panel |
| `14` | Eject | Eject |
| `15` | Video Mirror | Video Mirror Toggle |
| `16` | Play/Pause | Play / Pause |
| `17` | Next | Next Track |
| `18` | Previous | Previous Track |
| `19` | Fast | Fast Forward |
| `20` | Rewind | Rewind |
| `21` | Illum+ | Keyboard Illumination Up |
| `22` | Illum- | Keyboard Illumination Down |
| `23` | Illum Toggle | Keyboard Illumination Toggle |

### `SystemEvent`

Sends a system-level power or display event.

```toml
type = "SystemEvent"
subtype = 11       # Sleep
data = 0           # data parameter (used by Brightness)
label = "Sleep"    # optional display label
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `subtype` | u16 | Yes | NX_SUBTYPE constant |
| `data` | i32 | Yes | Additional data (used by Brightness: `data=2` up, `data=3` down) |
| `label` | String | No | Human-readable label. Falls back to a known name if empty. |

**Common `subtype` values:**

| Value | Label | Description |
|-------|-------|-------------|
| `1` | Power Key | Power button press |
| `10` | Eject | Eject media |
| `11` | Sleep | Sleep system |
| `12` | Restart | Restart system |
| `13` | Shutdown | Shut down system |
| `53` | Brightness | Display brightness. `data=2` increases, `data=3` decreases. |

### `MouseMove`

Moves the mouse cursor relative to its current position.

```toml
type = "MouseMove"
dx = 10.0
dy = -5.0
label = "Mouse"    # optional display label
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `dx` | f64 | Yes | Horizontal delta in device-specific units |
| `dy` | f64 | Yes | Vertical delta in device-specific units |
| `label` | String | No | Human-readable label. Falls back to `"Mouse"` if empty. |

### `MouseClick`

Simulates a mouse button click at an optional absolute screen position.

```toml
type = "MouseClick"
button = 1         # left click
x = 200.0          # optional absolute X
y = 300.0          # optional absolute Y
label = "Click"    # optional display label
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `button` | u8 | Yes | Button identifier: `1` = left, `2` = right, `3` = middle |
| `x` | f64 | No | Optional absolute screen X coordinate |
| `y` | f64 | No | Optional absolute screen Y coordinate |
| `label` | String | No | Human-readable label. Falls back to `"Mouse"` if empty. |

### `MouseScroll`

Simulates a scroll wheel event.

```toml
type = "MouseScroll"
dx = 0.0
dy = 3.0
label = "Scroll"   # optional display label
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `dx` | f64 | Yes | Horizontal scroll delta |
| `dy` | f64 | Yes | Vertical scroll delta (positive = scroll up, negative = scroll down) |
| `label` | String | No | Human-readable label. Falls back to `"Mouse"` if empty. |

## The `[[profiles]]` Section

Per-application overrides that remap hub buttons only when a specific application is in the foreground. The profiles list is checked **in order** — the first profile whose `app_id` matches the frontmost application is used.

### Profile matching

A profile matches when the currently focused application's bundle identifier (on macOS) or executable name (on Windows) equals the `app_id` field.

| Field | Type | Description |
|-------|------|-------------|
| `app_id` | String | Application identifier. On macOS this is the bundle ID (e.g. `"com.apple.Terminal"`). On Windows this is the executable name. |
| `app_name` | String | Human-readable name for display purposes only (not used for matching). |
| `mappings` | Table | A `ButtonMappingSet` with only the fields to override. |

### Override semantics

Within a `[[profiles]]` section, the `mappings` table follows the same structure as `[default]` but every field is **optional**. Missing fields **inherit** from `[default]` automatically.

The merge logic works as follows:
1. Start with the complete `[default]` mapping set.
2. For the matching profile, take every field that is present (not `None` in the TOML) and replace it.
3. Fields omitted from the profile keep their default value.

This means a profile can override a single button while leaving all others unchanged.

### Example profile

```toml
[[profiles]]
app_id = "com.apple.Terminal"
app_name = "Terminal"

[profiles.mappings]
button_top_left = { type = "Keyboard", binding = "Cmd+N" }

[[profiles]]
app_id = "com.google.Chrome"
app_name = "Google Chrome"

[profiles.mappings]
knob_cw = { type = "MediaKey", key_type = 16 }      # Play/Pause
knob_ccw = { type = "MediaKey", key_type = 17 }     # Next track
```

## The `[logging]` Section

Controls logging verbosity and destination.

```toml
[logging]
loglevel = "debug"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `loglevel` | String | `"info"` | Minimum log level. Accepted values: `"debug"` (all events), `"info"` (key events), `"warn"` (problems only). Case-sensitive. |

The runtime interprets values as follows:
- `"debug"` — `LogLevel::Debug`: most verbose, includes all HID events.
- `"info"` — `LogLevel::Info`: default, shows key events only.
- `"warn"` — `LogLevel::Warn`: only problems and errors.
- Any other value defaults to `LogLevel::Info`.

## Complete Annotated Example

```toml
# override-hub configuration
#
# This file controls:
#   1. Which USB HID devices/interfaces to seize — [device] section
#   2. What each hub button does — [default] section
#   3. Per-application overrides — [[profiles]] (advanced)
#   4. Logging behaviour — [logging] section

# ── Device identification (whitelist) ────────────────────────────────────────
# Three interfaces are seized:
#   1. Hub Interface 0  — keyboard (button combos)
#   2. Hub Interface 1  — consumer control (knob rotate + click)
#   3. Audio chip        — consumer control (Play/Pause)
#
# Each [[device.seize]] uses the hub's VID:PID (0x05AC:0x029C) but targets
# a different HID usage page/collection. The audio chip uses a different
# VID:PID pair (0x0C76:0x1710).

[device]

[[device.seize]]
vendor_id = "0x05AC"
product_id = "0x029C"
usage_page = 1      # Generic Desktop
usage = 6           # Keyboard

[[device.seize]]
vendor_id = "0x05AC"
product_id = "0x029C"
usage_page = 12     # Consumer
usage = 1           # Consumer Control

[[device.seize]]
vendor_id = "0x0C76"
product_id = "0x1710"
usage_page = 12     # Consumer
usage = 1           # Consumer Control

# ── Default key bindings ────────────────────────────────────────────────────
# These apply when no per-app profile matches.

[default]

[default.button_top_left]
type = "Keyboard"
binding = "Shift+Cmd+N"

[default.button_top_left_hold]
type = "Keyboard"
binding = "Cmd+T"

[default.button_bottom_right]
type = "Keyboard"
binding = "Shift+Cmd+4"

[default.button_bottom_right_hold]
type = "Keyboard"
binding = "Shift+Cmd+3"

[default.play_pause]
type = "MediaKey"
key_type = 16       # Play/Pause

[default.knob_cw]
type = "MediaKey"
key_type = 0        # Vol+

[default.knob_ccw]
type = "MediaKey"
key_type = 1        # Vol-

[default.knob_click]
type = "MediaKey"
key_type = 7        # Mute

# ── Logging ──────────────────────────────────────────────────────────────────
# level: "debug" (all), "info" (key events), "warn" (problems only).

[logging]
loglevel = "debug"

# ── Per-app profiles (advanced) ──────────────────────────────────────────────
# Only override the buttons you want to change — missing fields
# automatically inherit the [default] mapping.
#
# Example: Terminal.app gets plain Cmd+N instead of Shift+Cmd+N:
#
# [[profiles]]
# app_id = "com.apple.Terminal"
# app_name = "Terminal"
#
# [profiles.mappings]
# button_top_left = { type = "Keyboard", binding = "Cmd+N" }
```
