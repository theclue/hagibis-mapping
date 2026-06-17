/// Returns a TOML string with the production default configuration.
pub fn default_config_toml() -> String {
    r##"# override-hub configuration
#
# This file controls:
#   1. Which USB HID devices/interfaces to seize — [device] section
#   2. What each hub button does — [default] section
#   3. Per-application overrides — [[profiles]] (advanced)
#   4. Logging behaviour — [logging] section
#
# ═══════════════════════════════════════════════════════════════════════════════
# TARGET EVENT TYPES
# ═══════════════════════════════════════════════════════════════════════════════
#
# Every button/knob entry must be one of these types:
#
#   type = "Keyboard"
#       binding = "A"           single keystroke (letters A-Z, digits 0-9,
#       binding = "Ctrl+Q"       F1-F15, Space, Enter, Escape, Backspace,
#       binding = "Shift+Cmd+4"  Tab, Delete, Home, End, PageUp, PageDown,
#                                ArrowLeft/Right/Up/Down)
#       Modifiers: Ctrl, Shift, Alt/Option, Cmd/Super/Win/Meta
#
#   type = "MediaKey"
#       key_type = 0            Sound Up (0), Sound Down (1), Mute (7)
#       key_type = 16           Play/Pause (16), Next (17), Previous (18)
#
#   type = "SystemEvent"
#       subtype = 11            Sleep (11), Restart (12), Shutdown (13),
#       subtype = 10            Eject (10)
#       subtype = 53            Brightness — data=2 (up), data=3 (down)
#       data = ...              (only used by Brightness)
#
#   type = "MouseMove"
#       dx = 10.0               horizontal delta
#       dy = -5.0               vertical delta
#
#   type = "MouseClick"
#       button = 1              left (1), right (2), middle (3)
#       x = 200.0               optional absolute screen X
#       y = 300.0               optional absolute screen Y
#
#   type = "MouseScroll"
#       dx = 0.0                horizontal scroll delta
#       dy = 3.0                vertical scroll delta
#
# ═══════════════════════════════════════════════════════════════════════════════

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
# Log file location:
#   macOS:   ~/Library/Logs/override-hub/hagibis.log  (readable by Console.app)
#   Windows: %LOCALAPPDATA%\override-hub\logs\hagibis.log  (Storage Sense aware)

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
"##
    .to_string()
}
