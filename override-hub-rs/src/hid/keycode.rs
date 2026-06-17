/// USB HID keyboard keycodes (keyboard page 0x07).
///
/// Only the codes emitted by our specific hub are listed here.
/// Full table: https://www.usb.org/sites/default/files/hut1_21.pdf
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode(pub u8);

impl KeyCode {
    /// Top-left button, short press
    pub const BTN_TOP_LEFT: Self = Self(0x0F);
    /// Top-left button, hold (>500ms)
    pub const BTN_TOP_LEFT_HOLD: Self = Self(0x14);
    /// Bottom-right button, short press
    pub const BTN_BOTTOM_RIGHT: Self = Self(0x46);
    /// Bottom-right button, hold (>500ms)
    pub const BTN_BOTTOM_RIGHT_HOLD: Self = Self(0x20);
}

/// Consumer page bits (page 0x0C) — used by knob and audio chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumerBits(pub u8);

impl ConsumerBits {
    pub fn has_vol_up(&self) -> bool   { self.0 & 0x01 != 0 }
    pub fn has_vol_down(&self) -> bool { self.0 & 0x02 != 0 }
    pub fn has_mute(&self) -> bool     { self.0 & 0x04 != 0 }
    pub fn has_play_pause(&self) -> bool { self.0 & 0x40 != 0 }
}
