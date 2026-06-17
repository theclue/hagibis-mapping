use crate::error::Error;

/// Abstraction over platform-specific event injection.
pub trait Injector {
    // ── Keyboard ──────────────────────────────────────────────────────────

    /// Inject a simple key-down (`down = true`) or key-up (`down = false`).
    fn inject_key(&self, virtual_key: u16, down: bool) -> Result<(), Error>;

    /// Inject a key combination with modifier(s) held.
    ///
    /// `modifiers` bitmask uses USB HID values:
    ///   1 = Ctrl, 2 = Shift, 4 = Alt/Option, 8 = GUI (Cmd on macOS, Win on Windows).
    ///
    /// Each platform maps these to the correct virtual-key codes.
    fn inject_key_combo(&self, virtual_key: u16, modifiers: u8) -> Result<(), Error>;

    // ── Media keys (OSD) ──────────────────────────────────────────────────

    /// Inject a system media key (shows OSD on macOS/Windows).
    ///
    /// `key_type` is an `NX_KEYTYPE_*` constant (0=VolUp, 1=VolDown,
    /// 7=Mute, 16=Play, etc.).
    fn inject_media_key(&self, key_type: u8) -> Result<(), Error>;

    // ── System events ─────────────────────────────────────────────────────

    /// Inject a system-level event (sleep, restart, shutdown, eject…).
    ///
    /// `subtype` is an `NX_SUBTYPE_*` constant.
    /// `data` is subtype-dependent (typically 0).
    fn inject_system_event(&self, subtype: u16, data: i32) -> Result<(), Error>;

    // ── Mouse ─────────────────────────────────────────────────────────────

    /// Move the mouse cursor by a relative amount.
    ///
    /// `(dx, dy)` are in screen points.
    fn inject_mouse_move(&self, dx: f64, dy: f64) -> Result<(), Error>;

    /// Click a mouse button at the current cursor position.
    ///
    /// `button`: 1 = left, 2 = right, 3 = middle.
    /// If `x` and `y` are `Some`, the cursor is moved to that absolute
    /// position first.
    fn inject_mouse_click(
        &self, button: u8, x: Option<f64>, y: Option<f64>,
    ) -> Result<(), Error>;

    /// Scroll the mouse wheel.
    ///
    /// `dy`: vertical scroll (positive = up).
    /// `dx`: horizontal scroll (positive = right).
    fn inject_mouse_scroll(&self, dx: f64, dy: f64) -> Result<(), Error>;
}
