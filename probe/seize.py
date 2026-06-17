#!/usr/bin/env python3
"""USB Hub HID Seize — remap hub buttons to A/B/C/D.

Seizes Interface 0 (keyboard) in exclusive mode via IOKit.
Requires sudo on macOS Tahoe:  sudo ./seize.sh

Usage:
    source .venv/bin/activate
    sudo python seize.py
"""

import ctypes
import os
import signal
import sys

import objc
import Quartz
from Foundation import NSDictionary, NSNumber, NSArray

# ═══════════════════════════════════════════════════════════════════════════════
# Constants
# ═══════════════════════════════════════════════════════════════════════════════

_HUB_VID = 0x05AC
_HUB_PID = 0x029C
_KBD_USAGE_PAGE = 0x0001
_KBD_USAGE = 0x0006

_AUDIO_VID = 0x0C76
_AUDIO_PID = 0x1710
_AUDIO_USAGE_PAGE = 0x000C  # Consumer
_AUDIO_USAGE = 0x0001       # Consumer Control

REMAPPING = {
    0x14: 0x00,  # Ctrl+Q  → A
    0x0F: 0x0B,  # Cmd+R   → B
    0x20: 0x08,  # Ctrl+3  → C
    0x46: 0x02,  # Cmd+F13 → D
}
CONSUMER_TO_VK = {
    0x01: 0x03,  # Vol+ (bit 0)       → F (kVK_ANSI_F)
    0x02: 0x05,  # Vol- (bit 1)       → G (kVK_ANSI_G)
    0x04: 0x04,  # Mute (bit 2)       → H (kVK_ANSI_H)
    0x40: 0x0E,  # Play/Pause (bit 6) → E (kVK_ANSI_E)
}

# ═══════════════════════════════════════════════════════════════════════════════
# IOKit ctypes
# ═══════════════════════════════════════════════════════════════════════════════

_iokit = ctypes.cdll.LoadLibrary("/System/Library/Frameworks/IOKit.framework/IOKit")
_cf    = ctypes.cdll.LoadLibrary("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")

CFAllocatorRef   = ctypes.c_void_p
CFStringRef      = ctypes.c_void_p
CFDictionaryRef  = ctypes.c_void_p
CFRunLoopRef     = ctypes.c_void_p
IOHIDManagerRef  = ctypes.c_void_p
IOReturn         = ctypes.c_int32
IOOptionBits     = ctypes.c_uint32
CFIndex          = ctypes.c_long

kCFAllocatorDefault = CFAllocatorRef(0)
kIOHIDSeizeDevice   = 1

_mode = CFStringRef.in_dll(_cf, "kCFRunLoopDefaultMode")

_iokit.IOHIDManagerCreate.restype = IOHIDManagerRef
_iokit.IOHIDManagerCreate.argtypes = [CFAllocatorRef, IOOptionBits]
_iokit.IOHIDManagerSetDeviceMatching.argtypes = [IOHIDManagerRef, CFDictionaryRef]
_iokit.IOHIDManagerSetDeviceMatchingMultiple.argtypes = [IOHIDManagerRef, ctypes.c_void_p]
_iokit.IOHIDManagerOpen.restype = IOReturn
_iokit.IOHIDManagerOpen.argtypes = [IOHIDManagerRef, IOOptionBits]
_iokit.IOHIDManagerClose.restype = IOReturn
_iokit.IOHIDManagerClose.argtypes = [IOHIDManagerRef]
_iokit.IOHIDManagerScheduleWithRunLoop.argtypes = [IOHIDManagerRef, CFRunLoopRef, CFStringRef]
_iokit.IOHIDManagerUnscheduleFromRunLoop.argtypes = [IOHIDManagerRef, CFRunLoopRef, CFStringRef]
_iokit.IOHIDManagerRegisterInputReportCallback.argtypes = [IOHIDManagerRef, ctypes.c_void_p, ctypes.c_void_p]
_iokit.IOHIDManagerRegisterInputReportCallback.restype = None

_cf.CFRunLoopGetCurrent.restype = CFRunLoopRef
_cf.CFRunLoopRunInMode.argtypes = [CFStringRef, ctypes.c_double, ctypes.c_bool]
_cf.CFRunLoopRunInMode.restype = ctypes.c_int32
_cf.CFRunLoopStop.argtypes = [CFRunLoopRef]
_cf.CFRelease.argtypes = [ctypes.c_void_p]


def _make_dict(vid, pid, usage_page, usage):
    return NSDictionary.dictionaryWithObjects_forKeys_(
        [NSNumber.numberWithInt_(v) for v in (vid, pid, usage_page, usage)],
        ["VendorID", "ProductID", "PrimaryUsagePage", "PrimaryUsage"],
    )


def _make_matching_array():
    """Match hub keyboard + hub knob + audio chip consumer."""
    return NSArray.arrayWithArray_([
        _make_dict(_HUB_VID, _HUB_PID, _KBD_USAGE_PAGE, _KBD_USAGE),
        _make_dict(_AUDIO_VID, _AUDIO_PID, _AUDIO_USAGE_PAGE, _AUDIO_USAGE),
        _make_dict(_HUB_VID, _HUB_PID, _AUDIO_USAGE_PAGE, _AUDIO_USAGE),
    ])

# ═══════════════════════════════════════════════════════════════════════════════
# State
# ═══════════════════════════════════════════════════════════════════════════════

_prev_keys: set[int] = set()
_prev_consumer: int = 0

ABSOLUTE_BITS  = {0x01, 0x02}  # Vol+, Vol-: held while knob turns
RELATIVE_BITS  = {0x04}        # Mute: each click toggles


def _inject_key(vk: int, down: bool):
    """Post a keyboard event. Uses HID event source with session tap."""
    src = Quartz.CGEventSourceCreate(Quartz.kCGEventSourceStateHIDSystemState)
    if src is None:
        return
    ev = Quartz.CGEventCreateKeyboardEvent(src, vk, down)
    if ev is None:
        return
    Quartz.CGEventPost(Quartz.kCGSessionEventTap, ev)


def _parse_and_remap(data: bytes):
    global _prev_keys
    if len(data) < 8:
        return
    current_keys = {b for b in data[2:8] if b != 0}

    for kc in sorted(current_keys - _prev_keys):
        vk = REMAPPING.get(kc)
        if vk is not None:
            _inject_key(vk, True)

    for kc in sorted(_prev_keys - current_keys):
        vk = REMAPPING.get(kc)
        if vk is not None:
            _inject_key(vk, False)

    _prev_keys = current_keys


def _parse_consumer(data: bytes):
    """Parse consumer report (knob / audio chip) and inject remapped keys."""
    global _prev_consumer
    b0 = data[0]
    prev = _prev_consumer

    for mask, vk in CONSUMER_TO_VK.items():
        was = bool(prev & mask)
        now = bool(b0 & mask)

        if mask in ABSOLUTE_BITS:
            if now and not was:
                _inject_key(vk, True)
            elif was and not now:
                _inject_key(vk, False)

        elif mask in RELATIVE_BITS:
            if now and not was:
                _inject_key(vk, True)
                _inject_key(vk, False)

        else:
            # Play/Pause: press = tap
            if now and not was:
                _inject_key(vk, True)
                _inject_key(vk, False)

    _prev_consumer = b0


# ═══════════════════════════════════════════════════════════════════════════════
# Callback
# ═══════════════════════════════════════════════════════════════════════════════

_HIDReportCallback = ctypes.CFUNCTYPE(
    None,
    ctypes.c_void_p, ctypes.c_int32, ctypes.c_void_p,
    ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_long,
)

_cb_ref = None
_running = None


@_HIDReportCallback
def _hid_callback(context, result, sender, rtype, rid, report_ptr, rlen):
    if _running is None or not _running[0]:
        return
    try:
        if rtype != 0 or report_ptr is None:
            return
        data = ctypes.string_at(report_ptr, rlen)

        if rid == 0 and rlen >= 8:
            # Keyboard report (Interface 0) — no report ID, 8 bytes
            _parse_and_remap(data)
        elif rid == 0 and rlen >= 4:
            # Consumer report (Interface 1 / audio chip) — no report ID, 4 bytes
            _parse_consumer(data)
    except Exception:
        pass


# ═══════════════════════════════════════════════════════════════════════════════
# Main
# ═══════════════════════════════════════════════════════════════════════════════

def main():
    global _cb_ref, _running

    running = [True]
    _running = running

    if os.geteuid() != 0:
        print("ERROR: This tool requires root (sudo) to seize the HID device.", file=sys.stderr)
        print("Run:  sudo ./seize.sh", file=sys.stderr)
        sys.exit(1)

    def _stop(sig, frame):
        running[0] = False
        try:
            _cf.CFRunLoopStop(_cf.CFRunLoopGetCurrent())
        except Exception:
            pass

    signal.signal(signal.SIGINT, _stop)
    signal.signal(signal.SIGTERM, _stop)

    mgr = _iokit.IOHIDManagerCreate(kCFAllocatorDefault, 0)
    if not mgr:
        print("ERROR: IOHIDManagerCreate failed", file=sys.stderr)
        sys.exit(1)

    loop = _cf.CFRunLoopGetCurrent()
    _iokit.IOHIDManagerScheduleWithRunLoop(mgr, loop, _mode)

    # Set matching BEFORE open — the devices matched are the ones seized
    match_array = _make_matching_array()
    _iokit.IOHIDManagerSetDeviceMatchingMultiple(mgr, ctypes.c_void_p(objc.pyobjc_id(match_array)))

    # Open with SEIZE — requires root
    ret = _iokit.IOHIDManagerOpen(mgr, kIOHIDSeizeDevice)
    if ret != 0:
        print(f"ERROR: IOHIDManagerOpen with seize failed (0x{ret:08X})", file=sys.stderr)
        _iokit.IOHIDManagerClose(mgr)
        _cf.CFRelease(mgr)
        sys.exit(1)

    _cb_ref = _hid_callback
    _iokit.IOHIDManagerRegisterInputReportCallback(mgr, _cb_ref, None)

    print("SEIZE active — hub controls remapped")
    print("  Ctrl+Q        → A")
    print("  Cmd+R         → B")
    print("  Ctrl+3        → C")
    print("  Cmd+F13       → D")
    print("  Play/Pause    → E")
    print("  Vol+          → F")
    print("  Vol-          → G")
    print("  Mute/Unmute   → H")
    print()
    print("Press Ctrl+C to stop.")
    sys.stdout.flush()
    sys.stderr.flush()

    try:
        while running[0]:
            _cf.CFRunLoopRunInMode(_mode, 0.1, True)
    except KeyboardInterrupt:
        pass

    # ── Cleanup ───────────────────────────────────────────────────────────
    running[0] = False
    _cb_ref = None
    if mgr:
        try:
            _iokit.IOHIDManagerUnscheduleFromRunLoop(mgr, loop, _mode)
        except Exception:
            pass
        try:
            _iokit.IOHIDManagerClose(mgr)
        except Exception:
            pass
        try:
            _cf.CFRelease(mgr)
        except Exception:
            pass
    print("Seize released. Hub buttons restored.")
    sys.stdout.flush()


if __name__ == "__main__":
    main()
