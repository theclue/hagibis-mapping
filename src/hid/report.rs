use super::keycode::{ConsumerBits, KeyCode};

/// A parsed keyboard report from Interface 0 or Interface 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardReport {
    /// HID modifier byte (bitmask: Ctrl, Shift, Alt, GUI)
    pub modifier: u8,
    /// Currently pressed keycodes (up to 6 simultaneous, 0x00 = no key)
    pub keycodes: Vec<KeyCode>,
}

/// A parsed consumer report from Interface 1 (knob) or audio chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumerReport {
    pub bits: ConsumerBits,
}

/// Media key flags from report ID 0x52.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaKeysReport {
    pub play: bool,
    pub fast_forward: bool,
    pub rewind: bool,
    pub next_track: bool,
    pub prev_track: bool,
}

/// All possible HID input report types received from the hub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    Keyboard(KeyboardReport),
    Consumer(ConsumerReport),
    MediaKeys(MediaKeysReport),
    /// Vendor-defined (report ID 0x3F) — ignored.
    Vendor,
    /// Unsupported or unknown format.
    Unknown,
}
