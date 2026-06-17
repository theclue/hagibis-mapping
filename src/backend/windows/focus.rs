use crate::backend::traits::focus::{FocusQuery, FocusedApp};

use super::ffi;

pub struct Win32Focus;

impl Win32Focus {
    pub fn new() -> Self { Self }
}

impl FocusQuery for Win32Focus {
    fn focused_app(&self) -> Option<FocusedApp> {
        let hwnd = unsafe { ffi::GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }

        let mut pid: u32 = 0;
        unsafe { ffi::GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == 0 {
            return None;
        }

        let process = unsafe {
            ffi::OpenProcess(ffi::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)
        };
        if process.is_null() {
            return None;
        }

        let mut buf = [0u16; 260]; // MAX_PATH WCHAR
        let mut size: u32 = buf.len() as u32;
        let ok = unsafe {
            ffi::QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut size)
        };
        unsafe { ffi::CloseHandle(process) };

        if ok != 0 && size > 0 {
            let len = size as usize;
            let path: String = String::from_utf16_lossy(&buf[..len]);
            // Extract the executable name from the path
            let name = std::path::Path::new(&path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            Some(FocusedApp {
                id: name.clone(),
                name,
            })
        } else {
            None
        }
    }
}
