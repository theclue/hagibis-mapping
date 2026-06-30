//! Raw FFI bindings to Windows APIs (kernel32, user32).

#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case, dead_code)]

pub type HANDLE = *mut std::ffi::c_void;
pub type HWND = *mut std::ffi::c_void;
pub type DWORD = u32;
pub type WORD = u16;
pub type BOOL = i32;
pub type LPCWSTR = *const u16;
pub type LPWSTR = *mut u16;

// ═══════════════════════════════════════════════════════════════════════════════
// SendInput structures
// ═══════════════════════════════════════════════════════════════════════════════

pub const INPUT_KEYBOARD: u32 = 1;
pub const INPUT_MOUSE: u32 = 0;

pub const KEYEVENTF_KEYUP: u32 = 0x0002;
pub const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;

// MOUSEEVENTF_* flags
pub const MOUSEEVENTF_MOVE: u32 = 0x0001;
pub const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
pub const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
pub const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
pub const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
pub const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
pub const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;
pub const MOUSEEVENTF_WHEEL: u32 = 0x0800;
pub const MOUSEEVENTF_HWHEEL: u32 = 0x1000;
pub const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;

#[repr(C)]
pub struct MOUSEINPUT {
    pub dx: i32,
    pub dy: i32,
    pub mouseData: u32,
    pub dwFlags: u32,
    pub time: u32,
    pub dwExtraInfo: usize,
}

#[repr(C)]
pub struct KEYBDINPUT {
    pub wVk: u16,
    pub wScan: u16,
    pub dwFlags: u32,
    pub time: u32,
    pub dwExtraInfo: usize,
}

#[repr(C)]
pub union INPUT_UNION {
    pub ki: KEYBDINPUT,
    pub mi: MOUSEINPUT,
}

#[repr(C)]
pub struct INPUT {
    pub type_: u32,
    pub u: INPUT_UNION,
}

// ═══════════════════════════════════════════════════════════════════════════════
// user32
// ═══════════════════════════════════════════════════════════════════════════════

unsafe extern "system" {
    pub fn SendInput(cInputs: u32, pInputs: *const INPUT, cbSize: i32) -> u32;
    pub fn GetForegroundWindow() -> HWND;
    pub fn GetWindowThreadProcessId(hWnd: HWND, lpdwProcessId: *mut DWORD) -> DWORD;
    pub fn RegisterRawInputDevices(
        pRawInputDevices: *const RAWINPUTDEVICE,
        uiNumDevices: u32,
        cbSize: u32,
    ) -> i32;
    pub fn GetRawInputData(
        hRawInput: HANDLE,
        uiCommand: u32,
        pData: *mut std::ffi::c_void,
        pcbSize: *mut u32,
        cbSizeHeader: u32,
    ) -> u32;
    pub fn GetRawInputDeviceInfoW(
        hDevice: HANDLE,
        uiCommand: u32,
        pData: *mut std::ffi::c_void,
        pcbSize: *mut u32,
    ) -> u32;
    pub fn RegisterClassExW(lpWndClass: *const WNDCLASSEXW) -> u16;
    pub fn UnregisterClassW(lpClassName: *const u16, hInstance: HANDLE) -> i32;
    pub fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: u32,
        X: i32,
        Y: i32,
        nWidth: i32,
        nHeight: i32,
        hWndParent: HWND,
        hMenu: HANDLE,
        hInstance: HANDLE,
        lpParam: *mut std::ffi::c_void,
    ) -> HWND;
    pub fn DestroyWindow(hWnd: HWND) -> i32;
    pub fn DefWindowProcW(hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> isize;
    pub fn GetMessageW(
        lpMsg: *mut MSG,
        hWnd: HWND,
        wMsgFilterMin: u32,
        wMsgFilterMax: u32,
    ) -> i32;
    pub fn PeekMessageW(
        lpMsg: *mut MSG,
        hWnd: HWND,
        wMsgFilterMin: u32,
        wMsgFilterMax: u32,
        wRemoveMsg: u32,
    ) -> i32;
    pub fn TranslateMessage(lpMsg: *const MSG) -> i32;
    pub fn DispatchMessageW(lpMsg: *const MSG) -> i32;
    pub fn PostQuitMessage(nExitCode: i32);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Low-level keyboard hook (WH_KEYBOARD_LL)
// ═══════════════════════════════════════════════════════════════════════════════

pub const WH_KEYBOARD_LL: i32 = 13;
pub const HC_ACTION: isize = 0;
pub const LLKHF_INJECTED: u32 = 0x10;
pub const LLKHF_LOWER_IL_INJECTED: u32 = 0x02;

#[repr(C)]
pub struct KBDLLHOOKSTRUCT {
    pub vkCode: u32,
    pub scanCode: u32,
    pub flags: u32,
    pub time: u32,
    pub dwExtraInfo: usize,
}

pub type HHOOK = isize;
pub type HOOKPROC = unsafe extern "system" fn(code: i32, wParam: usize, lParam: isize) -> isize;

unsafe extern "system" {
    pub fn SetWindowsHookExW(
        idHook: i32,
        lpfn: HOOKPROC,
        hmod: HANDLE,
        dwThreadId: u32,
    ) -> HHOOK;
    pub fn UnhookWindowsHookEx(hhk: HHOOK) -> BOOL;
    pub fn CallNextHookEx(hhk: HHOOK, nCode: i32, wParam: usize, lParam: isize) -> isize;
    pub fn GetModuleHandleW(lpModuleName: *const u16) -> HANDLE;
}

// ── RawInput types ────────────────────────────────────────────────────────

#[repr(C)]
pub struct RAWINPUTDEVICE {
    pub usUsagePage: u16,
    pub usUsage: u16,
    pub dwFlags: u32,
    pub hwndTarget: HWND,
}

#[repr(C)]
pub struct RAWINPUTHEADER {
    pub dwType: u32,
    pub dwSize: u32,
    pub hDevice: HANDLE,
    pub wParam: usize,
}

#[repr(C)]
pub struct RAWHID {
    pub dwSizeHid: u32,
    pub dwCount: u32,
    pub bRawData: [u8; 1],
}

pub const RIDEV_INPUTSINK: u32 = 0x00000100;
pub const RIDEV_EXCLUDE: u32 = 0x00000020;
pub const RID_INPUT: u32 = 0x10000003;
pub const RIM_TYPEHID: u32 = 2;

// ── GetRawInputDeviceInfo ────────────────────────────────────────────────

pub const RIDI_DEVICEINFO: u32 = 0x2000000B;

#[repr(C)]
pub struct RID_DEVICE_INFO_HID {
    pub dwVendorId: u32,
    pub dwProductId: u32,
    pub dwVersionNumber: u32,
    pub usUsagePage: u16,
    pub usUsage: u16,
}

#[repr(C)]
pub union RID_DEVICE_INFO_UNION {
    pub hid: RID_DEVICE_INFO_HID,
}

#[repr(C)]
pub struct RID_DEVICE_INFO {
    pub cbSize: u32,
    pub dwType: u32,
    pub u: RID_DEVICE_INFO_UNION,
}

// ── Additional user32 functions ──────────────────────────────────────────

// ── Window message constants ──────────────────────────────────────────────

pub const WM_INPUT: u32 = 0x00FF;
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_QUIT: u32 = 0x0012;
pub const WM_DEVICECHANGE: u32 = 0x0219;
pub const HWND_MESSAGE: isize = -3;

// ── WM_DEVICECHANGE sub‑codes (wParam) ────────────────────────────────────

pub const DBT_DEVICEARRIVAL: u32 = 0x8000;
pub const DBT_DEVICEREMOVECOMPLETE: u32 = 0x8004;
pub const DBT_DEVTYP_DEVICEINTERFACE: u32 = 5;

// ── WM_DEVICECHANGE device-notification structures ────────────────────────

#[repr(C)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[repr(C)]
pub struct DEV_BROADCAST_HDR {
    pub dbcc_size: u32,
    pub dbcc_devicetype: u32,
    pub dbcc_reserved: u32,
}

#[repr(C)]
pub struct DEV_BROADCAST_DEVICEINTERFACE_W {
    pub dbcc_size: u32,
    pub dbcc_devicetype: u32,
    pub dbcc_reserved: u32,
    pub dbcc_classguid: GUID,
    pub dbcc_name: [u16; 1],
}

unsafe extern "system" {
    pub fn RegisterDeviceNotificationW(
        hRecipient: HANDLE,
        NotificationFilter: *const DEV_BROADCAST_DEVICEINTERFACE_W,
        Flags: DWORD,
    ) -> HANDLE;
    pub fn UnregisterDeviceNotification(Handle: HANDLE) -> BOOL;
}

/// Use with a window handle recipient (DEVICE_NOTIFY_WINDOW_HANDLE = 0).
pub const DEVICE_NOTIFY_WINDOW_HANDLE: DWORD = 0x00000000;

// ── Window class types ────────────────────────────────────────────────────

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: unsafe extern "system" fn(HWND, u32, usize, isize) -> isize,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HANDLE,
    pub hIcon: HANDLE,
    pub hCursor: HANDLE,
    pub hbrBackground: HANDLE,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: HANDLE,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: usize,
    pub lParam: isize,
    pub time: u32,
    pub pt_x: i32,
    pub pt_y: i32,
}

// ═══════════════════════════════════════════════════════════════════════════════
// kernel32
// ═══════════════════════════════════════════════════════════════════════════════

unsafe extern "system" {
    pub fn CloseHandle(hObject: HANDLE) -> BOOL;
    pub fn OpenProcess(dwDesiredAccess: DWORD, bInheritHandle: BOOL, dwProcessId: DWORD) -> HANDLE;
    pub fn QueryFullProcessImageNameW(
        hProcess: HANDLE,
        dwFlags: DWORD,
        lpExeName: LPWSTR,
        lpdwSize: *mut DWORD,
    ) -> BOOL;
}

pub const PROCESS_QUERY_LIMITED_INFORMATION: DWORD = 0x1000;

// ═══════════════════════════════════════════════════════════════════════════════
// Windows Virtual-Key codes (user32)
// ═══════════════════════════════════════════════════════════════════════════════

pub const VK_CONTROL: u16 = 0x11;
pub const VK_SHIFT: u16 = 0x10;
pub const VK_MENU: u16 = 0x12;    // Alt
pub const VK_LWIN: u16 = 0x5B;    // Left Windows key

pub const VK_VOLUME_UP: u16 = 0xAF;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;
pub const VK_RWIN: u16 = 0x5C;
pub const VK_VOLUME_DOWN: u16 = 0xAE;
pub const VK_VOLUME_MUTE: u16 = 0xAD;
pub const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;
pub const VK_MEDIA_NEXT_TRACK: u16 = 0xB0;
pub const VK_MEDIA_PREV_TRACK: u16 = 0xB1;
pub const VK_MEDIA_STOP: u16 = 0xB2;

// ═══════════════════════════════════════════════════════════════════════════════
// Win32 system event helpers (sleep, restart, shutdown)
// ═══════════════════════════════════════════════════════════════════════════════

pub const EWX_LOGOFF: u32 = 0;
pub const EWX_SHUTDOWN: u32 = 1;
pub const EWX_REBOOT: u32 = 2;
pub const EWX_FORCE: u32 = 4;
pub const SHTDN_REASON_MAJOR_OTHER: u32 = 0x00000000;
pub const SHTDN_REASON_MINOR_OTHER: u32 = 0x00000000;
pub const SHTDN_REASON_FLAG_PLANNED: u32 = 0x80000000;

unsafe extern "system" {
    pub fn ExitWindowsEx(uFlags: u32, dwReason: u32) -> i32;
    pub fn SetSuspendState(
        bHibernate: i32,
        bForce: i32,
        bWakeupEventsDisabled: i32,
    ) -> i32;
}

// HID device interface GUID: {4D1E55B2-F16F-11CF-88CB-001111000030}
pub const GUID_DEVINTERFACE_HID: GUID = GUID {
    data1: 0x4D1E55B2,
    data2: 0xF16F,
    data3: 0x11CF,
    data4: [0x88, 0xCB, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30],
};
