use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

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
}

impl WinHIDManager {
    pub fn new() -> Self {
        Self {
            reports: Arc::new(Mutex::new(VecDeque::new())),
            window: std::ptr::null_mut(),
            hub_devices: Vec::new(),
            running: Arc::new(Mutex::new(false)),
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

const CLASS_NAME: &str = "HagibisHubMapper\0";

unsafe extern "system" fn wndproc(
    hwnd: ffi::HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
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

        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = true;
        Ok(())
    }

    fn run_once(&mut self, timeout_ms: u32) -> Result<Option<Report>, Error> {
        if self.window.is_null() {
            return Err(Error::Seize("seize() first".into()));
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

    fn release(&mut self) {
        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = false;
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
    }
}
