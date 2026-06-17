//! Raw FFI bindings to Windows APIs (kernel32, user32).

#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case, dead_code)]

pub type HANDLE = *mut std::ffi::c_void;
pub type HWND = *mut std::ffi::c_void;
pub type HDEVINFO = *mut std::ffi::c_void;
pub type DWORD = u32;
pub type WORD = u16;
pub type BOOL = i32;
pub type LPCWSTR = *const u16;
pub type LPWSTR = *mut u16;
pub type ULONG_PTR = usize;
pub type LPOVERLAPPED = *mut std::ffi::c_void;

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

extern "system" {
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
pub const HWND_MESSAGE: isize = -3;

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

pub const GENERIC_READ: u32 = 0x80000000;
pub const GENERIC_WRITE: u32 = 0x40000000;
pub const FILE_SHARE_READ: u32 = 0x00000001;
pub const FILE_SHARE_WRITE: u32 = 0x00000002;
pub const OPEN_EXISTING: u32 = 3;
pub const FILE_FLAG_OVERLAPPED: u32 = 0x40000000;
pub const INVALID_HANDLE_VALUE: isize = -1;
pub const WAIT_OBJECT_0: u32 = 0;
pub const INFINITE: u32 = 0xFFFFFFFF;

extern "system" {
    pub fn CreateFileW(
        lpFileName: LPCWSTR,
        dwDesiredAccess: DWORD,
        dwShareMode: DWORD,
        lpSecurityAttributes: *mut std::ffi::c_void,
        dwCreationDisposition: DWORD,
        dwFlagsAndAttributes: DWORD,
        hTemplateFile: HANDLE,
    ) -> HANDLE;
    pub fn ReadFile(
        hFile: HANDLE,
        lpBuffer: *mut u8,
        nNumberOfBytesToRead: DWORD,
        lpNumberOfBytesRead: *mut DWORD,
        lpOverlapped: *mut std::ffi::c_void,
    ) -> BOOL;
    pub fn CloseHandle(hObject: HANDLE) -> BOOL;
    pub fn CreateEventW(
        lpEventAttributes: *mut std::ffi::c_void,
        bManualReset: BOOL,
        bInitialState: BOOL,
        lpName: LPCWSTR,
    ) -> HANDLE;
    pub fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
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
pub const VK_VOLUME_DOWN: u16 = 0xAE;
pub const VK_VOLUME_MUTE: u16 = 0xAD;
pub const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;

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

extern "system" {
    pub fn ExitWindowsEx(uFlags: u32, dwReason: u32) -> i32;
    pub fn SetSuspendState(
        bHibernate: i32,
        bForce: i32,
        bWakeupEventsDisabled: i32,
    ) -> i32;
}

// ═══════════════════════════════════════════════════════════════════════════════
// SetupAPI
// ═══════════════════════════════════════════════════════════════════════════════

pub const DIGCF_PRESENT: DWORD = 0x00000002;
pub const DIGCF_DEVICEINTERFACE: DWORD = 0x00000010;

#[repr(C)]
pub struct SP_DEVICE_INTERFACE_DATA {
    pub cbSize: DWORD,
    pub InterfaceClassGuid: [u8; 16],  // GUID = 16 bytes
    pub Flags: DWORD,
    pub Reserved: ULONG_PTR,
}

#[repr(C)]
pub struct SP_DEVICE_INTERFACE_DETAIL_DATA_W {
    pub cbSize: DWORD,
    pub DevicePath: [u16; 1],  // variable-length WCHAR array
}

extern "system" {
    pub fn SetupDiGetClassDevsW(
        ClassGuid: *const u8,
        Enumerator: LPCWSTR,
        hwndParent: HWND,
        Flags: DWORD,
    ) -> HDEVINFO;
    pub fn SetupDiEnumDeviceInterfaces(
        DeviceInfoSet: HDEVINFO,
        DeviceInfoData: *mut std::ffi::c_void,
        InterfaceClassGuid: *const u8,
        MemberIndex: DWORD,
        DeviceInterfaceData: *mut SP_DEVICE_INTERFACE_DATA,
    ) -> BOOL;
    pub fn SetupDiGetDeviceInterfaceDetailW(
        DeviceInfoSet: HDEVINFO,
        DeviceInterfaceData: *mut SP_DEVICE_INTERFACE_DATA,
        DeviceInterfaceDetailData: *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W,
        DeviceInterfaceDetailDataSize: DWORD,
        RequiredSize: *mut DWORD,
        DeviceInfoData: *mut std::ffi::c_void,
    ) -> BOOL;
    pub fn SetupDiDestroyDeviceInfoList(DeviceInfoSet: HDEVINFO) -> BOOL;
}

// HID device interface GUID: {4D1E55B2-F16F-11CF-88CB-001111000030}
pub const GUID_DEVINTERFACE_HID: [u8; 16] = [
    0xB2, 0x55, 0x1E, 0x4D, 0x6F, 0xF1, 0xCF, 0x11,
    0x88, 0xCB, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30,
];

// ═══════════════════════════════════════════════════════════════════════════════
// Hid.dll
// ═══════════════════════════════════════════════════════════════════════════════

#[repr(C)]
pub struct HIDD_ATTRIBUTES {
    pub Size: DWORD,
    pub VendorID: u16,
    pub ProductID: u16,
    pub VersionNumber: u16,
}

extern "system" {
    pub fn HidD_GetAttributes(HidDeviceObject: HANDLE, Attributes: *mut HIDD_ATTRIBUTES) -> BOOL;
    pub fn HidD_GetPreparsedData(HidDeviceObject: HANDLE, PreparsedData: *mut *mut std::ffi::c_void) -> BOOL;
    pub fn HidD_FreePreparsedData(PreparsedData: *mut std::ffi::c_void) -> BOOL;
}
