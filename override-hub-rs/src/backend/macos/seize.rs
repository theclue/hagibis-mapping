use std::ffi::CString;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

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
    running: Arc<Mutex<bool>>,
}

impl IOKitManager {
    pub fn new() -> Self {
        Self {
            manager: std::ptr::null_mut(),
            reports: Arc::new(Mutex::new(VecDeque::new())),
            ctx_ptr: std::ptr::null(),
            running: Arc::new(Mutex::new(false)),
        }
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

        // Register callback — pushes reports into our queue.
        let ctx = Arc::into_raw(self.reports.clone());
        self.ctx_ptr = ctx;
        unsafe {
            ffi::IOHIDManagerRegisterInputReportCallback(
                mgr,
                hid_report_collector,
                ctx as *mut std::ffi::c_void,
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

    if let Ok(mut q) = reports.lock() {
        q.push_back(report);
    }
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

fn cf_dict(vid: i32, pid: i32, up: i32, u: i32) -> ffi::CFDictionaryRef {
    let keys: [ffi::CFStringRef; 4] = [cf_string("VendorID"), cf_string("ProductID"), cf_string("PrimaryUsagePage"), cf_string("PrimaryUsage")];
    let vals: [ffi::CFNumberRef; 4] = [cf_number(vid), cf_number(pid), cf_number(up), cf_number(u)];
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

fn build_seize_array() -> Result<ffi::CFArrayRef, Error> {
    let dicts: [ffi::CFDictionaryRef; 3] = [
        cf_dict(0x05AC, 0x029C, 1, 6),
        cf_dict(0x05AC, 0x029C, 12, 1),
        cf_dict(0x0C76, 0x1710, 12, 1),
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
