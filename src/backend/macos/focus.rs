use std::ffi::CStr;

use crate::backend::traits::focus::{FocusQuery, FocusedApp};

use super::ffi;

/// NSWorkspace-based frontmost application query (macOS).
pub struct NSWorkspaceFocus;

impl NSWorkspaceFocus {
    pub fn new() -> Self { Self }
}

impl FocusQuery for NSWorkspaceFocus {
    fn focused_app(&self) -> Option<FocusedApp> {
        let mut bid_buf = [0i8; 256];
        let mut name_buf = [0i8; 256];

        let ok = unsafe {
            ffi::hagibis_focused_app(
                bid_buf.as_mut_ptr(),
                bid_buf.len() as i32,
                name_buf.as_mut_ptr(),
                name_buf.len() as i32,
            )
        };

        if ok == 0 {
            return None;
        }

        let id = unsafe { CStr::from_ptr(bid_buf.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let name = unsafe { CStr::from_ptr(name_buf.as_ptr()) }
            .to_string_lossy()
            .into_owned();

        Some(FocusedApp { id, name })
    }
}
