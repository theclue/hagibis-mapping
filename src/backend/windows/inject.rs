use crate::backend::traits::inject::Injector;
use crate::error::Error;

use super::ffi;

pub struct SendInputInjector;

impl SendInputInjector {
    pub fn new() -> Self { Self }
}

fn send_key(vk: u16, flags: u32) {
    let input = ffi::INPUT {
        type_: ffi::INPUT_KEYBOARD,
        u: ffi::INPUT_UNION {
            ki: ffi::KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe { ffi::SendInput(1, &input, std::mem::size_of::<ffi::INPUT>() as i32) };
}

impl Injector for SendInputInjector {
    fn inject_key(&self, vk: u16, down: bool) -> Result<(), Error> {
        let flags = if down { 0 } else { ffi::KEYEVENTF_KEYUP };
        send_key(vk, flags);
        Ok(())
    }

    fn inject_key_combo(&self, vk: u16, modifiers: u8) -> Result<(), Error> {
        // 1. Press modifiers
        if modifiers & 1 != 0 { send_key(ffi::VK_CONTROL, 0); }
        if modifiers & 2 != 0 { send_key(ffi::VK_SHIFT, 0); }
        if modifiers & 4 != 0 { send_key(ffi::VK_MENU, 0); }
        if modifiers & 8 != 0 { send_key(ffi::VK_LWIN, 0); }

        // 2. Press and release main key
        send_key(vk, 0);
        send_key(vk, ffi::KEYEVENTF_KEYUP);

        // 3. Release modifiers (reverse order)
        if modifiers & 8 != 0 { send_key(ffi::VK_LWIN, ffi::KEYEVENTF_KEYUP); }
        if modifiers & 4 != 0 { send_key(ffi::VK_MENU, ffi::KEYEVENTF_KEYUP); }
        if modifiers & 2 != 0 { send_key(ffi::VK_SHIFT, ffi::KEYEVENTF_KEYUP); }
        if modifiers & 1 != 0 { send_key(ffi::VK_CONTROL, ffi::KEYEVENTF_KEYUP); }

        Ok(())
    }

    fn inject_media_key(&self, key_type: u8) -> Result<(), Error> {
        let vk = match key_type {
            0  => ffi::VK_VOLUME_UP,
            1  => ffi::VK_VOLUME_DOWN,
            7  => ffi::VK_VOLUME_MUTE,
            16 => ffi::VK_MEDIA_PLAY_PAUSE,
            _ => return Err(Error::Inject(format!("unknown media key_type: {}", key_type))),
        };
        send_key(vk, 0);
        send_key(vk, ffi::KEYEVENTF_KEYUP);
        Ok(())
    }

    fn inject_system_event(&self, subtype: u16, data: i32) -> Result<(), Error> {
        match subtype {
            // Brightness — not easily available on Windows without WMI.
            // Use media key injection as a fallback (key_type = data).
            53 => {
                self.inject_media_key(data as u8)?;
            }
            // Sleep
            11 => {
                unsafe { ffi::SetSuspendState(0, 0, 0) };
            }
            // Restart
            12 => {
                unsafe {
                    ffi::ExitWindowsEx(
                        ffi::EWX_REBOOT | ffi::EWX_FORCE,
                        ffi::SHTDN_REASON_MAJOR_OTHER | ffi::SHTDN_REASON_MINOR_OTHER | ffi::SHTDN_REASON_FLAG_PLANNED,
                    );
                }
            }
            // Shutdown
            13 => {
                unsafe {
                    ffi::ExitWindowsEx(
                        ffi::EWX_SHUTDOWN | ffi::EWX_FORCE,
                        ffi::SHTDN_REASON_MAJOR_OTHER | ffi::SHTDN_REASON_MINOR_OTHER | ffi::SHTDN_REASON_FLAG_PLANNED,
                    );
                }
            }
            _ => return Err(Error::Inject(format!("system event subtype {} not supported on Windows", subtype))),
        }
        Ok(())
    }

    fn inject_mouse_move(&self, dx: f64, dy: f64) -> Result<(), Error> {
        let input = ffi::INPUT {
            type_: ffi::INPUT_MOUSE,
            u: ffi::INPUT_UNION {
                mi: ffi::MOUSEINPUT {
                    dx: dx as i32,
                    dy: dy as i32,
                    mouseData: 0,
                    dwFlags: ffi::MOUSEEVENTF_MOVE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe { ffi::SendInput(1, &input, std::mem::size_of::<ffi::INPUT>() as i32) };
        Ok(())
    }

    fn inject_mouse_click(&self, btn: u8, _x: Option<f64>, _y: Option<f64>) -> Result<(), Error> {
        let (down_flag, up_flag) = match btn {
            1 => (ffi::MOUSEEVENTF_LEFTDOWN, ffi::MOUSEEVENTF_LEFTUP),
            2 => (ffi::MOUSEEVENTF_RIGHTDOWN, ffi::MOUSEEVENTF_RIGHTUP),
            3 => (ffi::MOUSEEVENTF_MIDDLEDOWN, ffi::MOUSEEVENTF_MIDDLEUP),
            _ => return Err(Error::Inject(format!("unknown mouse button: {}", btn))),
        };

        let input_down = ffi::INPUT {
            type_: ffi::INPUT_MOUSE,
            u: ffi::INPUT_UNION {
                mi: ffi::MOUSEINPUT {
                    dx: 0, dy: 0, mouseData: 0,
                    dwFlags: down_flag,
                    time: 0, dwExtraInfo: 0,
                },
            },
        };
        unsafe { ffi::SendInput(1, &input_down, std::mem::size_of::<ffi::INPUT>() as i32) };

        let input_up = ffi::INPUT {
            type_: ffi::INPUT_MOUSE,
            u: ffi::INPUT_UNION {
                mi: ffi::MOUSEINPUT {
                    dx: 0, dy: 0, mouseData: 0,
                    dwFlags: up_flag,
                    time: 0, dwExtraInfo: 0,
                },
            },
        };
        unsafe { ffi::SendInput(1, &input_up, std::mem::size_of::<ffi::INPUT>() as i32) };

        Ok(())
    }

    fn inject_mouse_scroll(&self, dx: f64, dy: f64) -> Result<(), Error> {
        // Vertical scroll (WHEEL)
        if dy != 0.0 {
            let input = ffi::INPUT {
                type_: ffi::INPUT_MOUSE,
                u: ffi::INPUT_UNION {
                    mi: ffi::MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (dy * 120.0) as u32, // WHEEL_DELTA = 120
                        dwFlags: ffi::MOUSEEVENTF_WHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            unsafe { ffi::SendInput(1, &input, std::mem::size_of::<ffi::INPUT>() as i32) };
        }
        // Horizontal scroll (HWHEEL) — Vista+
        if dx != 0.0 {
            let input = ffi::INPUT {
                type_: ffi::INPUT_MOUSE,
                u: ffi::INPUT_UNION {
                    mi: ffi::MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (dx * 120.0) as u32,
                        dwFlags: ffi::MOUSEEVENTF_HWHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            unsafe { ffi::SendInput(1, &input, std::mem::size_of::<ffi::INPUT>() as i32) };
        }
        Ok(())
    }
}
