//! Raw FFI bindings to IOKit, CoreFoundation, and CoreGraphics.

#![allow(non_camel_case_types, dead_code)]

// ═══════════════════════════════════════════════════════════════════════════════
// Opaque types
// ═══════════════════════════════════════════════════════════════════════════════

pub type CFAllocatorRef = *const std::ffi::c_void;
pub type CFStringRef = *const std::ffi::c_void;
pub type CFNumberRef = *const std::ffi::c_void;
pub type CFDictionaryRef = *const std::ffi::c_void;
pub type CFArrayRef = *const std::ffi::c_void;
pub type CFRunLoopRef = *const std::ffi::c_void;
pub type IOHIDManagerRef = *mut std::ffi::c_void;

/// HID input report callback signature.
pub type IOHIDReportCallback = unsafe extern "C" fn(
    context: *mut std::ffi::c_void,
    result: i32,
    sender: *mut std::ffi::c_void,
    report_type: u32,
    report_id: u32,
    report: *const u8,
    report_length: isize,
);

// ═══════════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════════

pub const IOHID_OPTIONS_TYPE_SEIZE_DEVICE: u32 = 1;
pub const CF_NUMBER_SINT32_TYPE: i32 = 3;
pub const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

// CG event tap locations (CGEventTapLocation enum)
//   kCGHIDEventTap = 0, kCGSessionEventTap = 1, kCGAnnotatedSessionEventTap = 2
pub const CG_HID_EVENT_TAP: u32 = 0;
pub const CG_SESSION_EVENT_TAP: u32 = 1;

// CGEventSourceStateID — HID system state.
pub const CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: i32 = 1;

// CG event flag masks
pub const CG_EVENT_FLAG_MASK_CONTROL: u64   = 0x40000;
pub const CG_EVENT_FLAG_MASK_SHIFT: u64     = 0x20000;
pub const CG_EVENT_FLAG_MASK_ALTERNATE: u64 = 0x80000;
pub const CG_EVENT_FLAG_MASK_COMMAND: u64   = 0x100000;

// ═══════════════════════════════════════════════════════════════════════════════
// IOKit HID Manager
// ═══════════════════════════════════════════════════════════════════════════════

unsafe extern "C" {
    pub fn IOHIDManagerCreate(allocator: CFAllocatorRef, options: u32) -> IOHIDManagerRef;
    pub fn IOHIDManagerOpen(manager: IOHIDManagerRef, options: u32) -> i32;
    pub fn IOHIDManagerClose(manager: IOHIDManagerRef) -> i32;
    pub fn IOHIDManagerScheduleWithRunLoop(manager: IOHIDManagerRef, rl: CFRunLoopRef, mode: CFStringRef);
    pub fn IOHIDManagerUnscheduleFromRunLoop(manager: IOHIDManagerRef, rl: CFRunLoopRef, mode: CFStringRef);
    pub fn IOHIDManagerSetDeviceMatching(manager: IOHIDManagerRef, matching: CFDictionaryRef);
    pub fn IOHIDManagerSetDeviceMatchingMultiple(manager: IOHIDManagerRef, matching: CFArrayRef);
    pub fn IOHIDManagerRegisterInputReportCallback(manager: IOHIDManagerRef, cb: IOHIDReportCallback, ctx: *mut std::ffi::c_void);
}

// ═══════════════════════════════════════════════════════════════════════════════
// CoreFoundation
// ═══════════════════════════════════════════════════════════════════════════════

unsafe extern "C" {
    pub fn CFStringCreateWithCString(alloc: CFAllocatorRef, c_str: *const std::ffi::c_char, encoding: u32) -> CFStringRef;
    pub fn CFNumberCreate(alloc: CFAllocatorRef, number_type: i32, value_ptr: *const std::ffi::c_void) -> CFNumberRef;
    pub fn CFDictionaryCreate(alloc: CFAllocatorRef, keys: *const *const std::ffi::c_void, values: *const *const std::ffi::c_void, num: isize, kcb: *const std::ffi::c_void, vcb: *const std::ffi::c_void) -> CFDictionaryRef;
    pub fn CFArrayCreate(alloc: CFAllocatorRef, values: *const *const std::ffi::c_void, num: isize, cb: *const std::ffi::c_void) -> CFArrayRef;
    pub fn CFRelease(cf: *const std::ffi::c_void);
    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source_handled: u8) -> i32;
    pub fn CFRunLoopStop(run_loop: CFRunLoopRef);
}

/// `CFRunLoopDefaultMode` — loaded from CoreFoundation symbol table.
pub fn cf_run_loop_default_mode() -> CFStringRef {
    unsafe extern "C" {
        #[link_name = "kCFRunLoopDefaultMode"]
        static MODE: CFStringRef;
    }
    unsafe { MODE }
}

// ───────────────────────────────────────────────────────────────────────────────
// CF container callbacks
//
// CFDictionaryCreate / CFArrayCreate take pointers to callback structs that tell
// CoreFoundation how to retain/release the elements. Passing NULL means "do not
// retain" — which would leave the container holding dangling pointers once we
// release our own references. We instead pass the standard kCFType*CallBacks
// globals so CF retains the keys/values/elements for us.
//
// The structs are opaque here: we only need their ADDRESS, CF reads the real
// layout (which it defined) at that address.
// ───────────────────────────────────────────────────────────────────────────────

#[repr(C)]
pub struct CFCallbacksOpaque {
    _private: [u8; 0],
}

unsafe extern "C" {
    #[link_name = "kCFTypeDictionaryKeyCallBacks"]
    pub static CF_TYPE_DICTIONARY_KEY_CALLBACKS: CFCallbacksOpaque;
    #[link_name = "kCFTypeDictionaryValueCallBacks"]
    pub static CF_TYPE_DICTIONARY_VALUE_CALLBACKS: CFCallbacksOpaque;
    #[link_name = "kCFTypeArrayCallBacks"]
    pub static CF_TYPE_ARRAY_CALLBACKS: CFCallbacksOpaque;
}

pub fn cf_dictionary_key_callbacks() -> *const std::ffi::c_void {
    unsafe { &CF_TYPE_DICTIONARY_KEY_CALLBACKS as *const _ as *const std::ffi::c_void }
}
pub fn cf_dictionary_value_callbacks() -> *const std::ffi::c_void {
    unsafe { &CF_TYPE_DICTIONARY_VALUE_CALLBACKS as *const _ as *const std::ffi::c_void }
}
pub fn cf_array_callbacks() -> *const std::ffi::c_void {
    unsafe { &CF_TYPE_ARRAY_CALLBACKS as *const _ as *const std::ffi::c_void }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CoreGraphics
// ═══════════════════════════════════════════════════════════════════════════════

// ── CGPoint helper type (must be #[repr(C)] to match CoreGraphics ABI) ──────

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

unsafe extern "C" {
    pub fn CGEventSourceCreate(state_id: i32) -> *mut std::ffi::c_void;
    pub fn CGEventCreateKeyboardEvent(source: *mut std::ffi::c_void, vk: u16, key_down: bool) -> *mut std::ffi::c_void;
    pub fn CGEventCreateMouseEvent(
        source: *mut std::ffi::c_void,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> *mut std::ffi::c_void;
    pub fn CGEventCreateScrollWheelEvent(
        source: *mut std::ffi::c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> *mut std::ffi::c_void;
    pub fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
    pub fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
    pub fn CGWarpMouseCursorPosition(new_cursor_position: CGPoint) -> u32;
}

// CG mouse event types
pub const CG_EVENT_MOUSE_MOVED: u32 = 5;
pub const CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
pub const CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
pub const CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
pub const CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
pub const CG_EVENT_OTHER_MOUSE_DOWN: u32 = 25;
pub const CG_EVENT_OTHER_MOUSE_UP: u32 = 26;

// CG mouse button constants (CGMouseButton)
pub const CG_BUTTON_LEFT: u32 = 0;
pub const CG_BUTTON_RIGHT: u32 = 1;
pub const CG_BUTTON_CENTER: u32 = 2;

// CG scroll wheel units
pub const CG_SCROLL_UNIT_LINE: u32 = 1;
pub const CG_SCROLL_UNIT_PIXEL: u32 = 0;

// ═══════════════════════════════════════════════════════════════════════════════
// NSEvent helpers (compiled from nsevent_helper.m)
// ═══════════════════════════════════════════════════════════════════════════════

unsafe extern "C" {
    pub fn hagibis_post_media_key(key_code: i32, key_down: i32);
    pub fn hagibis_post_system_event(subtype: i32, data: i32);
    pub fn hagibis_focused_app(
        bundle_id_out: *mut std::ffi::c_char,
        bundle_id_cap: i32,
        name_out: *mut std::ffi::c_char,
        name_cap: i32,
    ) -> i32;
}
