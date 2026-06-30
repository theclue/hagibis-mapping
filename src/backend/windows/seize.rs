use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use crate::backend::traits::seize::HIDBackend;
use crate::error::Error;
use crate::hid::parser::parse;
use crate::hid::Report;

use super::ffi;

/// Windows seizure via RawInput — intercepts the hub's HID reports before
/// they become legacy keyboard messages, blocking originals from reaching
/// foreground applications (RIDEV_INPUTSINK + RIDEV_EXCLUDE).
///
/// NOTE: RegisterRawInputDevices registers by USAGE, not device.  This means
/// the raw-input callback fires for ALL keyboards/consumer-controls, not just
/// the hub.  The WndProc filters by device handle (gathered during seize) so
/// only hub reports are consumed; non-hub messages are passed to DefWindowProc.
pub struct WinHIDManager {
    reports: Arc<Mutex<VecDeque<Report>>>,
    window: ffi::HWND,
    /// Raw-input device handles belonging to the hub (HID keyboard + consumer).
    hub_devices: Vec<ffi::HANDLE>,
    running: Arc<Mutex<bool>>,
    /// Tracks whether the physical hub device is still connected.
    /// Updated by WM_DEVICECHANGE messages in the wndproc.
    device_present: Arc<AtomicBool>,
    /// Low-level keyboard hook handle, or 0 when not installed.
    hook_handle: ffi::HHOOK,
    /// Device notification handle for WM_DEVICECHANGE, or null.
    dev_notify_handle: ffi::HANDLE,
}

impl WinHIDManager {
    pub fn new() -> Self {
        Self {
            reports: Arc::new(Mutex::new(VecDeque::new())),
            window: std::ptr::null_mut(),
            hub_devices: Vec::new(),
            running: Arc::new(Mutex::new(false)),
            device_present: Arc::new(AtomicBool::new(false)),
            hook_handle: 0,
            dev_notify_handle: std::ptr::null_mut(),
        }
    }
}

impl Drop for WinHIDManager {
    /// Releases the message window even if the owning thread unwinds before
    /// `release()` is reached. `release()` is idempotent.
    fn drop(&mut self) {
        self.release();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Global state shared with the window procedure.
//
// Raw-input messages arrive on the same thread that created the window, but we
// use a Mutex rather than `static mut` so the code is sound under edition 2024
// (which denies references to `static mut`) and so `release()` can clear it,
// avoiding an Arc/buffer leak across start/stop cycles.
// ─────────────────────────────────────────────────────────────────────────────

static G_REPORTS: Mutex<Option<Arc<Mutex<VecDeque<Report>>>>> = Mutex::new(None);

/// Shared with the wndproc so WM_DEVICECHANGE can set the flag without
/// borrowing the WinHIDManager (which lives on the stack of another thread).
static G_DEVICE_PRESENT: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// VK codes currently pressed on the hub, mapped to their last-seen instant.
/// Populated by the wndproc when a hub keyboard report arrives; consumed by
/// `low_level_keyboard_hook` to suppress events from reaching foreground apps.
static G_HUB_VKCODES: LazyLock<Mutex<HashMap<u16, std::time::Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const CLASS_NAME: &str = "HagibisHubMapper\0";

/// Map a USB HID keyboard-page usage ID (page 0x07) to a Windows VK code.
/// Covers all standard usages 0x04–0x56 (letters, numbers, punctuation,
/// function keys, navigation, keypad).
fn hid_kbd_usage_to_vk(usage: u8) -> Option<u16> {
    match usage {
        // Letters A–Z: HID 0x04–0x1D ⇒ VK_A–VK_Z (0x41–0x5A)
        0x04..=0x1D => Some((usage as u16) + 0x3D),
        // Numbers 1–9: HID 0x1E–0x26 ⇒ VK_1–VK_9 (0x31–0x39)
        0x1E..=0x26 => Some((usage as u16) + 0x13),
        // Number 0
        0x27 => Some(0x30),                        // VK_0
        // Enter, Escape, Backspace, Tab, Spacebar
        0x28 => Some(0x0D),                        // VK_RETURN
        0x29 => Some(0x1B),                        // VK_ESCAPE
        0x2A => Some(0x08),                        // VK_BACK
        0x2B => Some(0x09),                        // VK_TAB
        0x2C => Some(0x20),                        // VK_SPACE
        // Hyphen / Equals
        0x2D => Some(0xBD),                        // VK_OEM_MINUS
        0x2E => Some(0xBB),                        // VK_OEM_PLUS
        // Left / Right bracket, Backslash
        0x2F => Some(0xDB),                        // VK_OEM_4  ( [ )
        0x30 => Some(0xDD),                        // VK_OEM_6  ( ] )
        0x31 => Some(0xDC),                        // VK_OEM_5  ( \ )
        // Semicolon, Apostrophe, Grave
        0x33 => Some(0xBA),                        // VK_OEM_1  ( ; )
        0x34 => Some(0xDE),                        // VK_OEM_7  ( ' )
        0x35 => Some(0xC0),                        // VK_OEM_3  ( ` )
        // Comma, Period, Slash
        0x36 => Some(0xBC),                        // VK_OEM_COMMA
        0x37 => Some(0xBE),                        // VK_OEM_PERIOD
        0x38 => Some(0xBF),                        // VK_OEM_2  ( / )
        // Caps Lock
        0x39 => Some(0x14),                        // VK_CAPITAL
        // Function keys F1–F24
        0x3A => Some(0x70), 0x3B => Some(0x71),    // VK_F1 – VK_F2
        0x3C => Some(0x72), 0x3D => Some(0x73),    // VK_F3 – VK_F4
        0x3E => Some(0x74), 0x3F => Some(0x75),    // VK_F5 – VK_F6
        0x40 => Some(0x76), 0x41 => Some(0x77),    // VK_F7 – VK_F8
        0x42 => Some(0x78), 0x43 => Some(0x79),    // VK_F9 – VK_F10
        0x44 => Some(0x7A), 0x45 => Some(0x7B),    // VK_F11 – VK_F12
        // Print Screen, Scroll Lock, Pause
        0x46 => Some(0x2C),                        // VK_SNAPSHOT
        0x47 => Some(0x91),                        // VK_SCROLL
        0x48 => Some(0x13),                        // VK_PAUSE
        // Insert, Home, PageUp
        0x49 => Some(0x2D),                        // VK_INSERT
        0x4A => Some(0x24),                        // VK_HOME
        0x4B => Some(0x21),                        // VK_PRIOR
        // Delete, End, PageDown
        0x4C => Some(0x2E),                        // VK_DELETE
        0x4D => Some(0x23),                        // VK_END
        0x4E => Some(0x22),                        // VK_NEXT
        // Arrow keys
        0x4F => Some(0x27),                        // VK_RIGHT
        0x50 => Some(0x25),                        // VK_LEFT
        0x51 => Some(0x28),                        // VK_DOWN
        0x52 => Some(0x26),                        // VK_UP
        // Keypad Number Lock and basic navigation
        0x53 => Some(0x62),                        // VK_NUMPAD2 (down)
        0x54 => Some(0x64),                        // VK_NUMPAD4 (left)
        0x55 => Some(0x66),                        // VK_NUMPAD6 (right)
        0x56 => Some(0x68),                        // VK_NUMPAD8 (up)
        _ => None,
    }
}

/// Low-level keyboard hook (`WH_KEYBOARD_LL`) that suppresses VK codes
/// the hub recently emitted.  Returns 1 (blocked) for hub-originated keys,
/// or calls `CallNextHookEx` to pass unmodified events through.
unsafe extern "system" fn low_level_keyboard_hook(
    code: i32,
    w_param: usize,
    l_param: isize,
) -> isize {
    if code == ffi::HC_ACTION {
        let info = &*(l_param as *const ffi::KBDLLHOOKSTRUCT);

        // Events injected by SendInput (our own synthetic keys) must pass
        // through — we only want to suppress genuine hardware events that
        // originated from the hub.
        if (info.flags & ffi::LLKHF_INJECTED) != 0
            || (info.flags & ffi::LLKHF_LOWER_IL_INJECTED) != 0
        {
            return ffi::CallNextHookEx(0, code, w_param, l_param);
        }

        // If the hub recently emitted this VK code, suppress the event
        // by returning a non-zero value.
        let vk = info.vkCode as u16;
        if let Ok(guard) = G_HUB_VKCODES.lock() {
            if guard.contains_key(&vk) {
                return 1;
            }
        }
    }

    ffi::CallNextHookEx(0, code, w_param, l_param)
}

unsafe extern "system" fn wndproc(
    hwnd: ffi::HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    // ── WM_DEVICECHANGE: detect physical device arrival / removal ──────
    // We check lParam for DBT_DEVTYP_DEVICEINTERFACE and filter by VID to
    // avoid false positives from unrelated HID devices being plugged.
    // Security: only trust VID/PID that match our known hub identifiers.
    if msg == ffi::WM_DEVICECHANGE {
        let wparam_u32 = wparam as u32;
        if wparam_u32 == ffi::DBT_DEVICEREMOVECOMPLETE || wparam_u32 == ffi::DBT_DEVICEARRIVAL {
            let mut matched = false;
            if lparam != 0 {
                let hdr = unsafe { &*(lparam as *const ffi::DEV_BROADCAST_HDR) };
                if hdr.dbcc_devicetype == ffi::DBT_DEVTYP_DEVICEINTERFACE {
                    let di = unsafe {
                        &*(lparam as *const ffi::DEV_BROADCAST_DEVICEINTERFACE_W)
                    };
                    // Read the null-terminated wide device name up to dbcc_size.
                    let name_ptr = &di.dbcc_name as *const u16;
                    let max_chars =
                        (di.dbcc_size as usize - core::mem::offset_of!(ffi::DEV_BROADCAST_DEVICEINTERFACE_W, dbcc_name))
                            / 2;
                    let mut name_utf16: Vec<u16> = Vec::new();
                    for i in 0..max_chars.min(256) {
                        let c = unsafe { *name_ptr.add(i) };
                        if c == 0 {
                            break;
                        }
                        name_utf16.push(c);
                    }
                    let name = String::from_utf16_lossy(&name_utf16);
                    // VID filtering: only react to our known hub VIDs so
                    // unrelated device add/remove events don't spuriously
                    // transition the engine to SEEKING.
                    matched = name.contains("VID_05AC") || name.contains("VID_0C76");
                }
            }
            if wparam_u32 == ffi::DBT_DEVICEREMOVECOMPLETE && matched {
                if let Ok(guard) = G_DEVICE_PRESENT.lock() {
                    if let Some(ref dp) = *guard {
                        dp.store(false, Ordering::Relaxed);
                    }
                }
            } else if wparam_u32 == ffi::DBT_DEVICEARRIVAL && matched {
                if let Ok(guard) = G_DEVICE_PRESENT.lock() {
                    if let Some(ref dp) = *guard {
                        dp.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
        return ffi::DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    if msg != ffi::WM_INPUT {
        if msg == ffi::WM_DESTROY {
            ffi::PostQuitMessage(0);
            return 0;
        }
        return ffi::DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    // Get the size of the raw input data
    let mut size: u32 = 0;
    unsafe {
        ffi::GetRawInputData(
            lparam as ffi::HANDLE,
            ffi::RID_INPUT,
            std::ptr::null_mut(),
            &mut size,
            std::mem::size_of::<ffi::RAWINPUTHEADER>() as u32,
        );
    }
    if size == 0 {
        return 0;
    }

    let mut buf: Vec<u8> = vec![0u8; size as usize];
    let copied = unsafe {
        ffi::GetRawInputData(
            lparam as ffi::HANDLE,
            ffi::RID_INPUT,
            buf.as_mut_ptr() as *mut std::ffi::c_void,
            &mut size,
            std::mem::size_of::<ffi::RAWINPUTHEADER>() as u32,
        )
    };
    if copied == u32::MAX || copied == 0 {
        return 0;
    }

    // Parse the RAWINPUTHEADER
    let header = unsafe { &*(buf.as_ptr() as *const ffi::RAWINPUTHEADER) };
    if header.dwType != ffi::RIM_TYPEHID {
        return 0;
    }

    // Filter: only process reports from our hub devices.
    // RawInput registers by usage, so we get messages from ALL keyboards
    // and consumer controls.  Query the device info to check VID/PID.
    {
        let mut info = ffi::RID_DEVICE_INFO {
            cbSize: std::mem::size_of::<ffi::RID_DEVICE_INFO>() as u32,
            dwType: 0,
            u: ffi::RID_DEVICE_INFO_UNION {
                hid: ffi::RID_DEVICE_INFO_HID {
                    dwVendorId: 0, dwProductId: 0, dwVersionNumber: 0,
                    usUsagePage: 0, usUsage: 0,
                },
            },
        };
        let mut info_size = info.cbSize;
        let ret = unsafe {
            ffi::GetRawInputDeviceInfoW(
                header.hDevice,
                ffi::RIDI_DEVICEINFO,
                &mut info as *mut _ as *mut std::ffi::c_void,
                &mut info_size,
            )
        };
        if ret == u32::MAX || ret == 0 {
            return 0;
        }
        let is_hub = unsafe {
            info.u.hid.dwVendorId == 0x05AC && info.u.hid.dwProductId == 0x029C
                || info.u.hid.dwVendorId == 0x0C76 && info.u.hid.dwProductId == 0x1710
        };
        if !is_hub {
            // Not our hub — let the system handle this message normally
            return ffi::DefWindowProcW(hwnd, msg, wparam, lparam);
        }
    }

    // HID data follows the header
    let hid_offset = std::mem::size_of::<ffi::RAWINPUTHEADER>();
    if hid_offset + 8 > buf.len() {
        return 0;
    }
    // RAWHID: dwSizeHid (u32) + dwCount (u32) + bRawData[]
    let dw_size_hid = u32::from_ne_bytes([buf[hid_offset], buf[hid_offset + 1], buf[hid_offset + 2], buf[hid_offset + 3]]);
    let dw_count = u32::from_ne_bytes([buf[hid_offset + 4], buf[hid_offset + 5], buf[hid_offset + 6], buf[hid_offset + 7]]);

    if dw_size_hid == 0 || dw_count == 0 {
        return 0;
    }

    let data_begin = hid_offset + 8;
    let data_len = (dw_size_hid as usize) * (dw_count as usize);
    if data_begin + data_len > buf.len() {
        return 0;
    }

    let hid_data = &buf[data_begin..data_begin + data_len];
    let report_id = hid_data[0] as u32;
    let report = parse(hid_data, report_id, hid_data.len());

    // ── Track hub VK codes for the low-level keyboard hook ──────────
    // The hook (low_level_keyboard_hook) runs during PeekMessageW and
    // checks G_HUB_VKCODES to decide which events to suppress.  We
    // populate the map here so it reflects the latest hub state.
    if let Report::Keyboard(ref kb) = report {
        if let Ok(mut guard) = G_HUB_VKCODES.lock() {
            guard.clear();
            let now = std::time::Instant::now();
            // Regular keycodes (HID keyboard-page usage IDs → VK)
            for kc in &kb.keycodes {
                if let Some(vk) = hid_kbd_usage_to_vk(kc.0) {
                    guard.insert(vk, now);
                }
            }
            // Modifier bits (both left and right variants mapped to
            // the same VK because WH_KEYBOARD_LL reports them identically).
            if kb.modifier & 0x01 != 0 { guard.insert(ffi::VK_CONTROL, now); }
            if kb.modifier & 0x10 != 0 { guard.insert(ffi::VK_CONTROL, now); }
            if kb.modifier & 0x02 != 0 { guard.insert(ffi::VK_SHIFT, now); }
            if kb.modifier & 0x20 != 0 { guard.insert(ffi::VK_SHIFT, now); }
            if kb.modifier & 0x04 != 0 { guard.insert(ffi::VK_MENU, now); }
            if kb.modifier & 0x40 != 0 { guard.insert(ffi::VK_MENU, now); }
            if kb.modifier & 0x08 != 0 { guard.insert(ffi::VK_LWIN, now); }
            if kb.modifier & 0x80 != 0 { guard.insert(ffi::VK_RWIN, now); }
        }
    }

    // Clone the Arc out under the lock, then release the global lock before
    // touching the inner queue.
    let reports = G_REPORTS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(reports) = reports {
        if let Ok(mut q) = reports.lock() {
            q.push_back(report);
        }
    }

    // Return 0 — prevents DefWindowProc from generating legacy WM_KEYDOWN.
    // Together with RIDEV_EXCLUDE, this blocks the original events from the
    // foreground application.
    0
}

impl HIDBackend for WinHIDManager {
    fn seize(&mut self) -> Result<(), Error> {
        *G_REPORTS.lock().unwrap_or_else(|e| e.into_inner()) = Some(self.reports.clone());

        let class_name: Vec<u16> = CLASS_NAME.encode_utf16().collect();
        let wc = ffi::WNDCLASSEXW {
            cbSize: std::mem::size_of::<ffi::WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: wndproc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: std::ptr::null_mut(),
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };

        let atom = unsafe { ffi::RegisterClassExW(&wc) };
        if atom == 0 {
            return Err(Error::Seize("RegisterClassExW failed".into()));
        }

        let hwnd = unsafe {
            ffi::CreateWindowExW(
                0,
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0, 0, 0, 0,
                ffi::HWND_MESSAGE as ffi::HWND,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if hwnd.is_null() {
            // Unregister the class we just registered, else a later seize()
            // fails with ERROR_CLASS_ALREADY_EXISTS and never restarts.
            unsafe { ffi::UnregisterClassW(class_name.as_ptr(), std::ptr::null_mut()) };
            return Err(Error::Seize("CreateWindowExW failed".into()));
        }
        self.window = hwnd;

        // Register for raw input from keyboard + consumer interfaces
        let devices = [
            ffi::RAWINPUTDEVICE {
                usUsagePage: 1,  // Generic Desktop
                usUsage: 6,      // Keyboard
                dwFlags: ffi::RIDEV_INPUTSINK | ffi::RIDEV_EXCLUDE,
                hwndTarget: hwnd,
            },
            ffi::RAWINPUTDEVICE {
                usUsagePage: 12, // Consumer
                usUsage: 1,      // Consumer Control
                dwFlags: ffi::RIDEV_INPUTSINK | ffi::RIDEV_EXCLUDE,
                hwndTarget: hwnd,
            },
        ];

        let reg_ret = unsafe {
            ffi::RegisterRawInputDevices(
                devices.as_ptr(),
                devices.len() as u32,
                std::mem::size_of::<ffi::RAWINPUTDEVICE>() as u32,
            )
        };
        if reg_ret == 0 {
            unsafe {
                ffi::DestroyWindow(hwnd);
                // Also unregister the class so a later seize() can re-register.
                ffi::UnregisterClassW(class_name.as_ptr(), std::ptr::null_mut());
            }
            self.window = std::ptr::null_mut();
            return Err(Error::Seize("RegisterRawInputDevices failed".into()));
        }

        // Install a low-level keyboard hook to suppress hub-originated key
        // events from reaching the foreground application.  The hook checks
        // G_HUB_VKCODES, which the wndproc populates as it processes hub
        // keyboard reports.
        self.hook_handle = unsafe {
            ffi::SetWindowsHookExW(
                ffi::WH_KEYBOARD_LL,
                low_level_keyboard_hook,
                ffi::GetModuleHandleW(std::ptr::null()),
                0,
            )
        };
        if self.hook_handle == 0 {
            // Clean up raw-input registration and window before returning.
            self.device_present.store(false, Ordering::Relaxed);
            *G_DEVICE_PRESENT.lock().unwrap_or_else(|e| e.into_inner()) = None;
            unsafe { ffi::DestroyWindow(hwnd) };
            unsafe { ffi::UnregisterClassW(class_name.as_ptr(), std::ptr::null_mut()) };
            self.window = std::ptr::null_mut();
            return Err(Error::Seize("SetWindowsHookExW(WH_KEYBOARD_LL) failed".into()));
        }

        // Register for device-interface change notifications so the wndproc
        // receives WM_DEVICECHANGE when the hub is plugged or unplugged.
        let notify_filter = ffi::DEV_BROADCAST_DEVICEINTERFACE_W {
            dbcc_size: std::mem::size_of::<ffi::DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
            dbcc_devicetype: ffi::DBT_DEVTYP_DEVICEINTERFACE,
            dbcc_reserved: 0,
            dbcc_classguid: ffi::GUID_DEVINTERFACE_HID,
            dbcc_name: [0u16; 1],
        };
        self.dev_notify_handle = unsafe {
            ffi::RegisterDeviceNotificationW(
                self.window as ffi::HANDLE,
                &notify_filter,
                ffi::DEVICE_NOTIFY_WINDOW_HANDLE,
            )
        };

        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = true;
        // Share the device_present flag with the wndproc so WM_DEVICECHANGE
        // messages can update it, then mark the device as present — seize
        // succeeded so the hub is connected right now.
        *G_DEVICE_PRESENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(self.device_present.clone());
        self.device_present.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn run_once(&mut self, timeout_ms: u32) -> Result<Option<Report>, Error> {
        if self.window.is_null() {
            return Err(Error::Seize("seize() first".into()));
        }

        // ── Drain pending WM_INPUT messages ────────────────────────────
        // The low-level keyboard hook (WH_KEYBOARD_LL) fires during the
        // PeekMessageW call below.  By processing any already-queued hub
        // WM_INPUT messages first, we ensure G_HUB_VKCODES is up to date
        // before the hook checks it.
        //
        // Bound the drain to 16 iterations per run_once() call so that
        // sustained input (e.g. knob rotation) cannot livelock the engine
        // loop and starve focus/status updates.
        const MAX_DRAIN_ITERATIONS: usize = 16;
        for _ in 0..MAX_DRAIN_ITERATIONS {
            // MSG is a POD struct — zeroed init is sound.
            let mut peek_msg: ffi::MSG = unsafe { std::mem::zeroed() };
            let has = unsafe {
                ffi::PeekMessageW(
                    &mut peek_msg,
                    self.window,
                    ffi::WM_INPUT,
                    ffi::WM_INPUT,
                    1, // PM_REMOVE
                )
            };
            if has == 0 {
                break;
            }
            if peek_msg.message == ffi::WM_QUIT {
                *self.running.lock().unwrap_or_else(|e| e.into_inner()) = false;
                return Ok(None);
            }
            unsafe {
                ffi::TranslateMessage(&peek_msg);
                ffi::DispatchMessageW(&peek_msg);
            }
        }

        let mut msg = ffi::MSG {
            hwnd: std::ptr::null_mut(),
            message: 0,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt_x: 0,
            pt_y: 0,
        };

        let has_msg =
            unsafe { ffi::PeekMessageW(&mut msg, self.window, 0, 0, 1 /* PM_REMOVE */) };
        if has_msg != 0 {
            if msg.message == ffi::WM_QUIT {
                *self.running.lock().unwrap_or_else(|e| e.into_inner()) = false;
                return Ok(None);
            }
            unsafe {
                ffi::TranslateMessage(&msg);
                ffi::DispatchMessageW(&msg);
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(timeout_ms as u64));
        }

        let mut q = self.reports.lock().unwrap_or_else(|e| e.into_inner());
        Ok(q.pop_front())
    }

    fn is_device_present(&self) -> bool {
        self.device_present.load(Ordering::Relaxed)
    }

    fn release(&mut self) {
        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = false;

        // Uninstall the low-level keyboard hook so our thread no longer
        // intercepts keyboard events system-wide.
        if self.hook_handle != 0 {
            unsafe { ffi::UnhookWindowsHookEx(self.hook_handle) };
            self.hook_handle = 0;
        }
        // Clear the hub VK code tracker so even if the wndproc fires one
        // last time it doesn't leave stale entries.
        if let Ok(mut guard) = G_HUB_VKCODES.lock() {
            guard.clear();
        }

        // Unregister the device notification before destroying the window.
        if !self.dev_notify_handle.is_null() {
            unsafe { ffi::UnregisterDeviceNotification(self.dev_notify_handle) };
            self.dev_notify_handle = std::ptr::null_mut();
        }

        if !self.window.is_null() {
            unsafe { ffi::DestroyWindow(self.window) };
            self.window = std::ptr::null_mut();
            // Unregister the window class so a later seize() can re-register it.
            // Without this, the second RegisterClassExW fails with
            // ERROR_CLASS_ALREADY_EXISTS and the engine never restarts.
            let class_name: Vec<u16> = CLASS_NAME.encode_utf16().collect();
            unsafe { ffi::UnregisterClassW(class_name.as_ptr(), std::ptr::null_mut()) };
        }
        // Drop our reference to the report queue so it isn't leaked across cycles.
        *G_REPORTS.lock().unwrap_or_else(|e| e.into_inner()) = None;
        // Clear the device present flag so the wndproc doesn't hold a stale Arc.
        *G_DEVICE_PRESENT.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
