use crate::backend::traits::inject::Injector;
use crate::error::Error;
use crate::log_debug;

use super::ffi;

/// CGEvent-based injection (macOS).
pub struct CGEventInjector {
    /// HID-system event source, created once and reused. May be NULL — a NULL
    /// source is still accepted by CGEventCreateKeyboardEvent.
    source: *mut std::ffi::c_void,
}

impl CGEventInjector {
    pub fn new() -> Self {
        let source = unsafe { ffi::CGEventSourceCreate(ffi::CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE) };
        Self { source }
    }
}

impl Drop for CGEventInjector {
    fn drop(&mut self) {
        if !self.source.is_null() {
            unsafe { ffi::CFRelease(self.source) };
            self.source = std::ptr::null_mut();
        }
    }
}

impl Injector for CGEventInjector {
    fn inject_key(&self, vk: u16, down: bool) -> Result<(), Error> {
        let event = unsafe { ffi::CGEventCreateKeyboardEvent(self.source, vk, down) };
        if event.is_null() {
            return Err(Error::Inject("CGEventCreateKeyboardEvent returned NULL".into()));
        }
        // CGEventPost returns void; success is implied by a non-NULL event.
        unsafe { ffi::CGEventPost(ffi::CG_SESSION_EVENT_TAP, event) };
        unsafe { ffi::CFRelease(event) };
        Ok(())
    }

    fn inject_key_combo(&self, vk: u16, modifiers: u8) -> Result<(), Error> {
        let flags = modifier_mask_to_cgflags(modifiers);
        log_debug!(
            "inject",
            "combo vk=0x{:02X} hid_mods=0x{:02X} cgflags=0x{:08X} source={:p}",
            vk, modifiers, flags, self.source
        );

        // Key-down with modifier flags set via CGEventSetFlags.
        let event = unsafe { ffi::CGEventCreateKeyboardEvent(self.source, vk, true) };
        if event.is_null() {
            return Err(Error::Inject("CGEventCreateKeyboardEvent returned NULL".into()));
        }
        if flags != 0 {
            unsafe { ffi::CGEventSetFlags(event, flags) };
        }
        unsafe { ffi::CGEventPost(ffi::CG_SESSION_EVENT_TAP, event) };
        unsafe { ffi::CFRelease(event) };

        // Key-up
        let up_event = unsafe { ffi::CGEventCreateKeyboardEvent(self.source, vk, false) };
        if !up_event.is_null() {
            if flags != 0 {
                unsafe { ffi::CGEventSetFlags(up_event, flags) };
            }
            unsafe { ffi::CGEventPost(ffi::CG_SESSION_EVENT_TAP, up_event) };
            unsafe { ffi::CFRelease(up_event) };
        }

        Ok(())
    }

    fn inject_media_key(&self, key_type: u8) -> Result<(), Error> {
        // NSSystemDefined event via NSEvent+AppKit helper (nsevent_helper.c).
        // This is the only reliable way to inject volume/media controls on macOS
        // because the WindowServer processes these via the special NSEvent pathway.
        let code: i32 = match key_type {
            0  => 0,  // NX_KEYTYPE_SOUND_UP
            1  => 1,  // NX_KEYTYPE_SOUND_DOWN
            7  => 7,  // NX_KEYTYPE_MUTE
            16 => 16, // NX_KEYTYPE_PLAY
            _ => return Err(Error::Inject(format!("unknown media key_type: {}", key_type))),
        };
        unsafe {
            ffi::hagibis_post_media_key(code, 1); // key-down
            ffi::hagibis_post_media_key(code, 0); // key-up
        }
        Ok(())
    }

    fn inject_system_event(&self, subtype: u16, data: i32) -> Result<(), Error> {
        match subtype {
            // Brightness uses the media-key pathway
            53 => {
                // data = 2 (up) or 3 (down) → NX_KEYTYPE_BRIGHTNESS_UP/DOWN
                unsafe {
                    ffi::hagibis_post_media_key(data, 1);
                    ffi::hagibis_post_media_key(data, 0);
                }
            }
            // Sleep / Restart / Shutdown / Eject via NSSystemDefined
            10 | 11 | 12 | 13 => {
                unsafe { ffi::hagibis_post_system_event(subtype as i32, 0) };
            }
            _ => return Err(Error::Inject(format!("unknown system subtype: {}", subtype))),
        }
        Ok(())
    }

    fn inject_mouse_move(&self, dx: f64, dy: f64) -> Result<(), Error> {
        let pos = ffi::CGPoint { x: dx, y: dy };
        let event = unsafe {
            ffi::CGEventCreateMouseEvent(self.source, ffi::CG_EVENT_MOUSE_MOVED, pos, 0)
        };
        if event.is_null() {
            return Err(Error::Inject("CGEventCreateMouseEvent returned NULL".into()));
        }
        unsafe { ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, event) };
        unsafe { ffi::CFRelease(event) };
        Ok(())
    }

    fn inject_mouse_click(&self, btn: u8, x: Option<f64>, y: Option<f64>) -> Result<(), Error> {
        if let (Some(px), Some(py)) = (x, y) {
            let abs = ffi::CGPoint { x: px, y: py };
            unsafe { ffi::CGWarpMouseCursorPosition(abs) };
        }

        let (down_type, up_type) = match btn {
            1 => (ffi::CG_EVENT_LEFT_MOUSE_DOWN, ffi::CG_EVENT_LEFT_MOUSE_UP),
            2 => (ffi::CG_EVENT_RIGHT_MOUSE_DOWN, ffi::CG_EVENT_RIGHT_MOUSE_UP),
            3 => (ffi::CG_EVENT_OTHER_MOUSE_DOWN, ffi::CG_EVENT_OTHER_MOUSE_UP),
            _ => return Err(Error::Inject(format!("unknown mouse button: {}", btn))),
        };

        let button = match btn {
            1 => ffi::CG_BUTTON_LEFT,
            2 => ffi::CG_BUTTON_RIGHT,
            3 => ffi::CG_BUTTON_CENTER,
            _ => 0,
        };

        let pos = ffi::CGPoint { x: 0.0, y: 0.0 };
        let down = unsafe { ffi::CGEventCreateMouseEvent(self.source, down_type, pos, button) };
        if down.is_null() {
            return Err(Error::Inject("CGEventCreateMouseEvent (down) returned NULL".into()));
        }
        unsafe { ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, down) };
        unsafe { ffi::CFRelease(down) };

        let up = unsafe { ffi::CGEventCreateMouseEvent(self.source, up_type, pos, button) };
        if !up.is_null() {
            unsafe { ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, up) };
            unsafe { ffi::CFRelease(up) };
        }

        Ok(())
    }

    fn inject_mouse_scroll(&self, dx: f64, dy: f64) -> Result<(), Error> {
        // Clamp dx/dy to i32 range to prevent truncation overflow from
        // extreme f64 values (V-6). The -1000..1000 window is generous
        // enough for any realistic scroll gesture; config validation in
        // TargetEvent::MouseScroll could tighten it further.
        let clamp = |v: f64| -> i32 {
            if v.is_nan() || v.is_infinite() {
                return 0;
            }
            if v >= i32::MAX as f64 { i32::MAX }
            else if v <= i32::MIN as f64 { i32::MIN }
            else { v as i32 }
        };
        let dy_i32 = clamp(dy);
        let dx_i32 = clamp(dx);

        // dx = horizontal scroll, dy = vertical scroll
        let event = unsafe {
            ffi::CGEventCreateScrollWheelEvent(
                self.source,
                ffi::CG_SCROLL_UNIT_LINE,
                2,          // wheel count (2 = vertical + horizontal)
                dy_i32,     // wheel1 = vertical delta
                dx_i32,     // wheel2 = horizontal delta
                0,          // wheel3 = unused
            )
        };
        if event.is_null() {
            return Err(Error::Inject("CGEventCreateScrollWheelEvent returned NULL".into()));
        }
        unsafe { ffi::CGEventPost(ffi::CG_HID_EVENT_TAP, event) };
        unsafe { ffi::CFRelease(event) };
        Ok(())
    }
}

/// Convert USB HID modifier bits (1=Ctrl,2=Shift,4=Alt,8=Super/Cmd)
/// to CGEventFlags for use with CGEventSetFlags.
fn modifier_mask_to_cgflags(mask: u8) -> u64 {
    let mut flags = 0u64;
    if mask & 1 != 0 { flags |= ffi::CG_EVENT_FLAG_MASK_CONTROL; }
    if mask & 2 != 0 { flags |= ffi::CG_EVENT_FLAG_MASK_SHIFT; }
    if mask & 4 != 0 { flags |= ffi::CG_EVENT_FLAG_MASK_ALTERNATE; }
    if mask & 8 != 0 { flags |= ffi::CG_EVENT_FLAG_MASK_COMMAND; }
    flags
}
