use std::ffi::CString;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::backend::traits::seize::HIDBackend;
use crate::error::Error;
use crate::hid::parser::parse;
use crate::hid::Report;
use crate::log_debug;
use crate::log_info;

use super::ffi;

/// IOKit HIDManager backend (macOS).
pub struct IOKitManager {
    manager: ffi::IOHIDManagerRef,
    /// Queue of reports collected by the HID callback.
    reports: Arc<Mutex<VecDeque<Report>>>,
    /// Raw `Arc` clone handed to the C callback as its context. Stored so we can
    /// reclaim it with `Arc::from_raw` in `release()` and avoid leaking.
    ctx_ptr: *const Mutex<VecDeque<Report>>,
    /// Context pointer for the device-removal callback.
    removal_ctx_ptr: *const AtomicBool,
    /// Set to false by the removal callback; checked by `is_device_present()`.
    device_present: Arc<AtomicBool>,
    running: Arc<Mutex<bool>>,
}

impl IOKitManager {
    pub fn new() -> Self {
        Self {
            manager: std::ptr::null_mut(),
            reports: Arc::new(Mutex::new(VecDeque::new())),
            ctx_ptr: std::ptr::null(),
            removal_ctx_ptr: std::ptr::null(),
            device_present: Arc::new(AtomicBool::new(true)),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Returns false once IOKit reports that the device has been unplugged.
    pub fn is_device_present(&self) -> bool {
        self.device_present.load(Ordering::Relaxed)
    }
}

impl Drop for IOKitManager {
    /// Guarantees the device is released even if the owning thread unwinds
    /// (panic) before `release()` is reached. Without this, a panic anywhere in
    /// the engine loop would leave the hub seized until physical unplug — the
    /// classic "zombie process" failure mode. `release()` is idempotent, so a
    /// normal stop followed by this drop is a no-op.
    fn drop(&mut self) {
        self.release();
    }
}

impl HIDBackend for IOKitManager {
    fn seize(&mut self) -> Result<(), Error> {
        log_debug!("seize", "creating IOHIDManager...");
        let mgr = unsafe { ffi::IOHIDManagerCreate(std::ptr::null(), 0) };
        if mgr.is_null() {
            return Err(Error::Seize("IOHIDManagerCreate NULL".into()));
        }
        self.manager = mgr;
        log_debug!("seize", "manager={:p}", mgr);

        // Schedule with run loop
        let rl = unsafe { ffi::CFRunLoopGetCurrent() };
        let mode = ffi::cf_run_loop_default_mode();
        unsafe { ffi::IOHIDManagerScheduleWithRunLoop(mgr, rl, mode) };
        log_debug!("seize", "scheduled on run loop");

        // Set matching BEFORE open — the matched devices are the ones seized.
        let matching = build_seize_array()?;
        unsafe { ffi::IOHIDManagerSetDeviceMatchingMultiple(mgr, matching) };
        // IOKit copies the matching criteria; release our reference.
        unsafe { ffi::CFRelease(matching) };
        log_debug!("seize", "matching array set");

        // Open with seize
        let ret = unsafe { ffi::IOHIDManagerOpen(mgr, ffi::IOHID_OPTIONS_TYPE_SEIZE_DEVICE) };
        if ret != 0 {
            return Err(Error::Seize(format!("IOHIDManagerOpen failed: 0x{:08X}", ret)));
        }
        log_info!("seize", "IOHIDManagerOpen OK (ret=0x{:08X})", ret);

        // Register input report callback — pushes reports into our queue.
        // Both callback registration calls (input + removal) return void, so
        // there is no error to check. The Arc::into_raw() is paired with
        // Arc::from_raw() in release() which is called from Drop, so the Arc
        // is always reclaimed even on panic.
        let ctx = Arc::into_raw(self.reports.clone());
        self.ctx_ptr = ctx;
        unsafe {
            ffi::IOHIDManagerRegisterInputReportCallback(
                mgr,
                hid_report_collector,
                ctx as *mut std::ffi::c_void,
            )
        };

        // Register device-removal callback so engine_loop can detect unplug
        // and transition back to SEEKING without polling.
        self.device_present.store(true, Ordering::Relaxed);
        let removal_ctx = Arc::into_raw(self.device_present.clone());
        self.removal_ctx_ptr = removal_ctx;
        unsafe {
            ffi::IOHIDManagerRegisterDeviceRemovalCallback(
                mgr,
                device_removal_callback,
                removal_ctx as *mut std::ffi::c_void,
            )
        };

        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = true;
        Ok(())
    }

    fn run_once(&mut self, timeout_ms: u32) -> Result<Option<Report>, Error> {
        if self.manager.is_null() {
            return Err(Error::Seize("seize() first".into()));
        }
        let mode = ffi::cf_run_loop_default_mode();
        // Run the run loop for a short time slice
        unsafe { ffi::CFRunLoopRunInMode(mode, timeout_ms as f64 / 1000.0, 1) };

        // Drain collected reports
        let mut q = self.reports.lock().unwrap_or_else(|e| e.into_inner());
        Ok(q.pop_front())
    }

    fn release(&mut self) {
        // NOTE: this uses CFRunLoopGetCurrent(), so it MUST run on the same
        // thread that scheduled the manager. The struct is confined to one
        // thread (engine_loop, or the CLI main thread) and is dropped there,
        // including during an unwind, so the run loop is always the right one.
        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = false;
        if !self.manager.is_null() {
            let rl = unsafe { ffi::CFRunLoopGetCurrent() };
            let mode = ffi::cf_run_loop_default_mode();
            unsafe {
                ffi::IOHIDManagerUnscheduleFromRunLoop(self.manager, rl, mode);
                ffi::IOHIDManagerClose(self.manager);
                // Balance the +1 retain from IOHIDManagerCreate (Create Rule).
                // Without this, every start/stop cycle leaks one IOHIDManager
                // and its retained device references.
                ffi::CFRelease(self.manager as *const std::ffi::c_void);
            }
            self.manager = std::ptr::null_mut();
        }
        // Reclaim the Arc clone handed to the callback. Safe now that the manager
        // is closed and no further callbacks can fire.
        if !self.ctx_ptr.is_null() {
            unsafe { drop(Arc::from_raw(self.ctx_ptr)) };
            self.ctx_ptr = std::ptr::null();
        }
        if !self.removal_ctx_ptr.is_null() {
            unsafe { drop(Arc::from_raw(self.removal_ctx_ptr)) };
            self.removal_ctx_ptr = std::ptr::null();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// HID Report collector (callback → queue)
// ═══════════════════════════════════════════════════════════════════════════════

unsafe extern "C" fn hid_report_collector(
    context: *mut std::ffi::c_void,
    _result: i32,
    _sender: *mut std::ffi::c_void,
    rtype: u32,
    report_id: u32,
    report_ptr: *const u8,
    report_len: isize,
) {
    if rtype != 0 || report_ptr.is_null() || report_len < 1 {
        return;
    }
    let reports = unsafe { &*(context as *const Mutex<VecDeque<Report>>) };
    let data = unsafe { std::slice::from_raw_parts(report_ptr, report_len as usize) };

    // ── Debug: raw bytes ──────────────────────────────────────────────────
    let mut hex = String::with_capacity(data.len() * 3);
    for b in data {
        use std::fmt::Write;
        let _ = write!(hex, " {:02X}", b);
    }
    crate::logging::debug(
        "cb",
        &format!("report_id=0x{:02X} len={} raw={}", report_id, report_len, hex),
    );

    let report = parse(data, report_id, report_len as usize);
    crate::logging::debug("cb", &format!("parsed → {:?}", report));

    // Recover on poison for consistency with the rest of the codebase; a panic
    // cannot corrupt the VecDeque<Report> itself, so recovering is safe.
    reports
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push_back(report);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Device removal callback
// ═══════════════════════════════════════════════════════════════════════════════

unsafe extern "C" fn device_removal_callback(
    context: *mut std::ffi::c_void,
    _result: i32,
    _sender: *mut std::ffi::c_void,
    _device: *mut std::ffi::c_void,
) {
    let present = unsafe { &*(context as *const AtomicBool) };
    present.store(false, Ordering::Relaxed);
    crate::logging::info_log("seize", "device removed");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Matching dict builder
// ═══════════════════════════════════════════════════════════════════════════════

fn cf_string(s: &str) -> ffi::CFStringRef {
    let c = CString::new(s).unwrap();
    unsafe { ffi::CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), ffi::CF_STRING_ENCODING_UTF8) }
}

fn cf_number(v: i32) -> ffi::CFNumberRef {
    unsafe { ffi::CFNumberCreate(std::ptr::null(), ffi::CF_NUMBER_SINT32_TYPE, &v as *const i32 as *const std::ffi::c_void) }
}

/// Build a 4-key matching dictionary (VendorID, ProductID, PrimaryUsagePage, PrimaryUsage).
fn cf_dict_4(vid: i32, pid: i32, up: i32, u: i32) -> ffi::CFDictionaryRef {
    let keys: [ffi::CFStringRef; 4] = [cf_string("VendorID"), cf_string("ProductID"), cf_string("PrimaryUsagePage"), cf_string("PrimaryUsage")];
    let vals: [*const std::ffi::c_void; 4] = [cf_number(vid) as *const _, cf_number(pid) as *const _, cf_number(up) as *const _, cf_number(u) as *const _];
    // Pass the standard CFType callbacks so CF retains keys/values. With NULL
    // callbacks CF would store our pointers without retaining, and the CFRelease
    // calls below would leave the dictionary holding dangling pointers.
    let d = unsafe {
        ffi::CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr() as *const *const std::ffi::c_void,
            vals.as_ptr() as *const *const std::ffi::c_void,
            4,
            ffi::cf_dictionary_key_callbacks(),
            ffi::cf_dictionary_value_callbacks(),
        )
    };
    for i in 0..4 { unsafe { ffi::CFRelease(keys[i] as *const std::ffi::c_void); ffi::CFRelease(vals[i] as *const std::ffi::c_void) }; }
    d
}

/// Build a 5-key matching dictionary that also includes the USB product string.
/// This prevents a spoofed device with the same VID:PID from being seized —
/// the product name must match as well.
///
/// # Limitations
///
/// `kIOHIDProductKey` matching depends on USB driver enumeration order. If
/// the `Product` string is not yet populated in the `IOHIDDevice` properties
/// at the time `IOHIDManagerOpen` evaluates the matching dictionaries, IOKit
/// silently ignores the key and matches only on VID:PID:usage — the product
/// string filter is effectively a no-op in that scenario.
///
/// A more robust approach would be a post-match callback that reads the
/// property from each matched device via `IOHIDDeviceGetProperty`, but that
/// is left as future work. This is a defense-in-depth measure — it may help
/// on some macOS versions or firmware combinations even if not universally
/// effective.
fn cf_dict_5(vid: i32, pid: i32, up: i32, u: i32, product: &str) -> ffi::CFDictionaryRef {
    let keys: [ffi::CFStringRef; 5] = [cf_string("VendorID"), cf_string("ProductID"), cf_string("PrimaryUsagePage"), cf_string("PrimaryUsage"), cf_string("Product")];
    let vals: [*const std::ffi::c_void; 5] = [
        cf_number(vid) as *const _,
        cf_number(pid) as *const _,
        cf_number(up) as *const _,
        cf_number(u) as *const _,
        cf_string(product) as *const _,
    ];
    let d = unsafe {
        ffi::CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr() as *const *const std::ffi::c_void,
            vals.as_ptr() as *const *const std::ffi::c_void,
            5,
            ffi::cf_dictionary_key_callbacks(),
            ffi::cf_dictionary_value_callbacks(),
        )
    };
    for i in 0..5 { unsafe { ffi::CFRelease(keys[i] as *const std::ffi::c_void); ffi::CFRelease(vals[i] as *const std::ffi::c_void) }; }
    d
}

fn build_seize_array() -> Result<ffi::CFArrayRef, Error> {
    // ── Matching dictionary 1-2: Apple built-in keyboard/trackpad ─────
    // These devices are internal USB and cannot be easily spoofed by an
    // external device. We use VID+PID+usage matching which is sufficient
    // for internal hardware.
    //
    // ── Matching dictionary 3: 3rd-party hub (UC-1102AG) ──────────────
    // This device is external USB and could be spoofed by any device
    // claiming the same VID:PID. We add the "Product" string to the
    // matching dictionary so IOKit requires the product name to match too.
    let dicts: [ffi::CFDictionaryRef; 3] = [
        cf_dict_4(0x05AC, 0x029C, 1, 6),
        cf_dict_4(0x05AC, 0x029C, 12, 1),
        cf_dict_5(0x0C76, 0x1710, 12, 1, "UC-1102AG"),
    ];
    // Standard CFType array callbacks so the array retains its dictionaries.
    let arr = unsafe {
        ffi::CFArrayCreate(
            std::ptr::null(),
            dicts.as_ptr() as *const *const std::ffi::c_void,
            3,
            ffi::cf_array_callbacks(),
        )
    };
    for i in 0..3 { unsafe { ffi::CFRelease(dicts[i] as *const std::ffi::c_void) }; }
    if arr.is_null() { Err(Error::Seize("CFArrayCreate NULL".into())) } else { Ok(arr) }
}
