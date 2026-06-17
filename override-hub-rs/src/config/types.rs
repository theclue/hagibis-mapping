use serde::{Deserialize, Serialize};

/// Target event to inject for a hub button.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TargetEvent {
    /// A keyboard event: either a single key (`"A"`, `"Space"`) or a
    /// combo with modifiers (`"Ctrl+Q"`, `"Shift+Cmd+4"`).
    ///
    /// The binding string is parsed at runtime — modifiers are optional.
    /// This replaces the old `Combo`, `Key`, and `KeyWithMods` variants.
    Keyboard {
        /// Human-readable binding like `"Ctrl+Q"` or just `"A"`.
        binding: String,
    },
    /// A system media key (shows OSD on macOS, nothing on Windows).
    MediaKey {
        /// NX_KEYTYPE_* value (0=VolUp, 1=VolDown, 7=Mute, 16=Play, etc.)
        key_type: u8,
        #[serde(default)]
        label: String,
    },
    /// A system-level event (sleep, restart, shutdown, brightness, eject…).
    SystemEvent {
        subtype: u16,
        data: i32,
        #[serde(default)]
        label: String,
    },
    /// Move the mouse cursor relative to its current position.
    MouseMove {
        dx: f64,
        dy: f64,
        #[serde(default)]
        label: String,
    },
    /// Mouse button click.
    MouseClick {
        /// 1 = left, 2 = right, 3 = middle.
        button: u8,
        /// Absolute screen coordinates (optional).
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
        #[serde(default)]
        label: String,
    },
    /// Scroll wheel event.
    MouseScroll {
        dx: f64,
        dy: f64,
        #[serde(default)]
        label: String,
    },
}

impl TargetEvent {
    pub fn label(&self) -> &str {
        match self {
            TargetEvent::Keyboard { binding } => binding,
            TargetEvent::MediaKey { key_type, label } => {
                if !label.is_empty() { label }
                else {
                    match key_type {
                        0 => "Vol+", 1 => "Vol-", 7 => "Mute",
                        16 => "Play/Pause", 17 => "Next", 18 => "Prev",
                        _ => "MediaKey",
                    }
                }
            }
            TargetEvent::SystemEvent { subtype, label, .. } => {
                if !label.is_empty() { label }
                else {
                    match subtype {
                        10 => "Eject", 11 => "Sleep", 12 => "Restart",
                        13 => "Shutdown", 53 => "Brightness",
                        _ => "SystemEvent",
                    }
                }
            }
            TargetEvent::MouseMove { label, .. }
            | TargetEvent::MouseClick { label, .. }
            | TargetEvent::MouseScroll { label, .. } => {
                if !label.is_empty() { label } else { "Mouse" }
            }
        }
    }
}

impl Default for TargetEvent {
    fn default() -> Self {
        TargetEvent::Keyboard { binding: String::new() }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// NX_SUBTYPE_* constants (from IOLLEvent.h)
// ═══════════════════════════════════════════════════════════════════════════════

impl TargetEvent {
    pub const SUBTYPE_POWER_KEY: u16      = 1;
    pub const SUBTYPE_EJECT_KEY: u16      = 10;
    pub const SUBTYPE_SLEEP: u16          = 11;
    pub const SUBTYPE_RESTART: u16        = 12;
    pub const SUBTYPE_SHUTDOWN: u16       = 13;
    pub const SUBTYPE_BRIGHTNESS: u16     = 53; // synthetic — dispatched as key_type 2/3
}

// ═══════════════════════════════════════════════════════════════════════════════
// NX_KEYTYPE_* constants (from ev_keymap.h)
// ═══════════════════════════════════════════════════════════════════════════════

impl TargetEvent {
    pub const KEYTYPE_SOUND_UP: u8         = 0;
    pub const KEYTYPE_SOUND_DOWN: u8       = 1;
    pub const KEYTYPE_BRIGHTNESS_UP: u8    = 2;
    pub const KEYTYPE_BRIGHTNESS_DOWN: u8  = 3;
    pub const KEYTYPE_CAPS_LOCK: u8        = 4;
    pub const KEYTYPE_HELP: u8             = 5;
    pub const KEYTYPE_POWER: u8            = 6;
    pub const KEYTYPE_MUTE: u8             = 7;
    pub const KEYTYPE_CONTRAST_UP: u8      = 11;
    pub const KEYTYPE_CONTRAST_DOWN: u8    = 12;
    pub const KEYTYPE_LAUNCH_PANEL: u8     = 13;
    pub const KEYTYPE_EJECT: u8            = 14;
    pub const KEYTYPE_VIDMIRROR: u8        = 15;
    pub const KEYTYPE_PLAY: u8             = 16;
    pub const KEYTYPE_NEXT: u8             = 17;
    pub const KEYTYPE_PREVIOUS: u8         = 18;
    pub const KEYTYPE_FAST: u8             = 19;
    pub const KEYTYPE_REWIND: u8           = 20;
    pub const KEYTYPE_ILLUM_UP: u8         = 21;
    pub const KEYTYPE_ILLUM_DOWN: u8       = 22;
    pub const KEYTYPE_ILLUM_TOGGLE: u8     = 23;
}

// ═══════════════════════════════════════════════════════════════════════════════
// Config data types
// ═══════════════════════════════════════════════════════════════════════════════

/// USB HID device matching criteria.
///
/// Used to identify which physical devices to seize.
/// All fields are ANDed together (must ALL match).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMatch {
    /// USB Vendor ID as a hex string, e.g. `"0x05AC"`.
    pub vendor_id: String,
    /// USB Product ID as a hex string, e.g. `"0x029C"`.
    pub product_id: String,
    /// HID Usage Page (optional — if omitted, matches any usage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_page: Option<u16>,
    /// HID Usage (optional — if omitted, matches any usage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<u16>,
}

impl DeviceMatch {
    /// Parse the hex vendor ID into a u16.
    pub fn vendor_id_u16(&self) -> Option<u16> {
        parse_hex_u16(&self.vendor_id)
    }

    /// Parse the hex product ID into a u16.
    pub fn product_id_u16(&self) -> Option<u16> {
        parse_hex_u16(&self.product_id)
    }
}

fn parse_hex_u16(s: &str) -> Option<u16> {
    let s = s.trim().strip_prefix("0x").unwrap_or(s);
    u16::from_str_radix(s, 16).ok()
}

/// Device identification configuration.
///
/// Defines which HID devices/interfaces the application seizes.
/// Whitelist-only: only devices matching a `seize` entry are targeted.
/// No other device is ever touched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Criteria for devices/interfaces to seize.
    /// Multiple entries use OR logic.
    #[serde(default)]
    pub seize: Vec<DeviceMatch>,
}

/// Mapping set for all hub controls.
///
/// In the `[default]` section all fields are mandatory (the app needs a complete
/// fallback).  In `[[profiles]]` sections every field is optional — missing
/// fields inherit the default mapping automatically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonMappingSet {
    #[serde(default)]
    pub button_bottom_right:      Option<TargetEvent>,
    #[serde(default)]
    pub button_bottom_right_hold: Option<TargetEvent>,
    #[serde(default)]
    pub button_top_left:          Option<TargetEvent>,
    #[serde(default)]
    pub button_top_left_hold:     Option<TargetEvent>,
    #[serde(default)]
    pub play_pause:               Option<TargetEvent>,
    #[serde(default)]
    pub knob_cw:                  Option<TargetEvent>,
    #[serde(default)]
    pub knob_ccw:                 Option<TargetEvent>,
    #[serde(default)]
    pub knob_click:               Option<TargetEvent>,
}

impl ButtonMappingSet {
    /// Fill every `None` field from `fallback`, returning a complete set.
    pub fn merge(&self, fallback: &ButtonMappingSet) -> ButtonMappingSet {
        ButtonMappingSet {
            button_bottom_right:      self.button_bottom_right.clone().or_else(|| fallback.button_bottom_right.clone()),
            button_bottom_right_hold: self.button_bottom_right_hold.clone().or_else(|| fallback.button_bottom_right_hold.clone()),
            button_top_left:          self.button_top_left.clone().or_else(|| fallback.button_top_left.clone()),
            button_top_left_hold:     self.button_top_left_hold.clone().or_else(|| fallback.button_top_left_hold.clone()),
            play_pause:               self.play_pause.clone().or_else(|| fallback.play_pause.clone()),
            knob_cw:                  self.knob_cw.clone().or_else(|| fallback.knob_cw.clone()),
            knob_ccw:                 self.knob_ccw.clone().or_else(|| fallback.knob_ccw.clone()),
            knob_click:               self.knob_click.clone().or_else(|| fallback.knob_click.clone()),
        }
    }
}

/// Per-app remapping profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub app_id: String,
    pub app_name: String,
    pub mappings: ButtonMappingSet,
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Minimum log level: `"debug"` or `"info"`.
    #[serde(default = "default_loglevel")]
    pub loglevel: String,
}

fn default_loglevel() -> String { "info".into() }

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { loglevel: default_loglevel() }
    }
}

/// Top-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Device identification (which USB HID devices to seize).
    pub device: DeviceConfig,
    /// Mapping used when no per-app profile matches.
    pub default: ButtonMappingSet,
    /// Application-specific overrides (checked in order).
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Logging control.
    #[serde(default)]
    pub logging: LoggingConfig,
}
