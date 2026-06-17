#!/usr/bin/env python3
"""USB Hub HID Monitor — IOKit HIDManager passive monitor.

Monitors keyboard and consumer-control events from all hub interfaces
via IOKit HIDManager (shared read mode) — the hub works normally.

Usage:
    source .venv/bin/activate
    python monitor.py
"""

import ctypes
import os
import re
import signal
import sys
import threading
import time
from collections import deque
from dataclasses import dataclass

import objc
from Foundation import NSDictionary, NSNumber, NSArray

# ═══════════════════════════════════════════════════════════════════════════════
# Constants
# ═══════════════════════════════════════════════════════════════════════════════

LOG_FILE = "/tmp/hub-monitor.log"

# Consumer key types
KEYTYPE = {
    0:  ("Vol+",        "vol_up"),
    1:  ("Vol-",        "vol_down"),
    3:  ("Mute",        "mute"),
    14: ("Eject",       "eject"),
    16: ("Play/Pause",  "play"),
    17: ("Next Track",  "next_track"),
    18: ("Prev Track",  "prev_track"),
    19: ("Stop",        "stop"),
    20: ("FF",          "ff"),
    21: ("Rew",         "rew"),
}

# HID modifier bit → CGEvent-style flag mask (for display compatibility)
_HID_MOD_TO_CG = {
    0x01: 0x040000,  # Left Control
    0x02: 0x020000,  # Left Shift
    0x04: 0x080000,  # Left Alt/Option
    0x08: 0x100000,  # Left GUI/Command
    0x10: 0x040000,  # Right Control
    0x20: 0x020000,  # Right Shift
    0x40: 0x080000,  # Right Alt
    0x80: 0x100000,  # Right GUI
}

# Modifier mask → symbol
MOD_SYMBOLS = [
    (0x100000, "\u2318"),   # Command
    (0x020000, "\u21E7"),   # Shift
    (0x080000, "\u2325"),   # Option/Alt
    (0x040000, "\u2303"),   # Control
    (0x800000, "fn"),       # Function
]

# ANSI
CLEAR = "\033[2J"
HOME = "\033[H"
HIDE_CURSOR = "\033[?25l"
SHOW_CURSOR = "\033[?25h"
GREEN = "\033[1;32m"
DIM = "\033[2;37m"
YELLOW = "\033[1;33m"
CYAN = "\033[1;36m"
RESET = "\033[0m"
ON = f"{GREEN}\u25A0{RESET}"
OFF = f"{DIM}\u25A1{RESET}"


def _hid_mod_to_cg(mod_byte: int) -> int:
    """Convert HID modifier byte to CGEvent-style flags mask."""
    flags = 0
    for hid_bit, cg_flag in _HID_MOD_TO_CG.items():
        if mod_byte & hid_bit:
            flags |= cg_flag
    return flags


def _log(msg: str):
    """Append a timestamped message to the debug log."""
    try:
        with open(LOG_FILE, "a") as f:
            ts = time.strftime("%H:%M:%S", time.localtime())
            f.write(f"[{ts}] {msg}\n")
    except Exception:
        pass


# ═══════════════════════════════════════════════════════════════════════════════
# IOKit HID Manager — passive monitoring of all hub interfaces
# ═══════════════════════════════════════════════════════════════════════════════
# hidapi steals exclusive access, IOKit HIDManager with kIOHIDOptionsTypeNone
# allows shared read — macOS still receives and processes the events.

# Load frameworks
_iokit = ctypes.cdll.LoadLibrary("/System/Library/Frameworks/IOKit.framework/IOKit")
_cf = ctypes.cdll.LoadLibrary("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")

# Type aliases
_CFAllocatorRef = ctypes.c_void_p
_CFStringRef = ctypes.c_void_p
_CFNumberRef = ctypes.c_void_p
_CFDictionaryRef = ctypes.c_void_p
_CFRunLoopRef = ctypes.c_void_p
_CFRunLoopSourceRef = ctypes.c_void_p
_IOHIDManagerRef = ctypes.c_void_p
_IOReturn = ctypes.c_int32
_IOOptionBits = ctypes.c_uint32
_CFIndex = ctypes.c_long
_UInt8 = ctypes.c_uint8
_UInt32 = ctypes.c_uint32

# Constants
_kIOHIDOptionsTypeNone = 0
_kCFAllocatorDefault = _CFAllocatorRef(0)
_kCFRunLoopDefaultMode = _CFStringRef.in_dll(_cf, "kCFRunLoopDefaultMode")

# Matching keys (C-string constants)
_HID_VENDOR_ID_KEY = ctypes.c_char_p(b"VendorID")
_HID_PRODUCT_ID_KEY = ctypes.c_char_p(b"ProductID")
_HID_PRIMARY_USAGE_PAGE_KEY = ctypes.c_char_p(b"PrimaryUsagePage")
_HID_PRIMARY_USAGE_KEY = ctypes.c_char_p(b"PrimaryUsage")

# Target device
_HUB_VID = 0x05AC
_HUB_PID = 0x029C
_AUDIO_VID = 0x0C76
_AUDIO_PID = 0x1710

# Function prototypes
_iokit.IOHIDManagerCreate.restype = _IOHIDManagerRef
_iokit.IOHIDManagerCreate.argtypes = [_CFAllocatorRef, _IOOptionBits]

_iokit.IOHIDManagerSetDeviceMatching.argtypes = [_IOHIDManagerRef, _CFDictionaryRef]

# IOHIDManagerSetDeviceMatchingMultiple takes a CFArray of CFDictionary
_iokit.IOHIDManagerSetDeviceMatchingMultiple.argtypes = [_IOHIDManagerRef, ctypes.c_void_p]

_iokit.IOHIDManagerOpen.restype = _IOReturn
_iokit.IOHIDManagerOpen.argtypes = [_IOHIDManagerRef, _IOOptionBits]

_iokit.IOHIDManagerClose.restype = _IOReturn
_iokit.IOHIDManagerClose.argtypes = [_IOHIDManagerRef]

_iokit.IOHIDManagerScheduleWithRunLoop.argtypes = [_IOHIDManagerRef, _CFRunLoopRef, _CFStringRef]
_iokit.IOHIDManagerUnscheduleFromRunLoop.argtypes = [_IOHIDManagerRef, _CFRunLoopRef, _CFStringRef]

_iokit.IOHIDManagerRegisterInputReportCallback.argtypes = [_IOHIDManagerRef, ctypes.c_void_p, ctypes.c_void_p]
_iokit.IOHIDManagerRegisterInputReportCallback.restype = None

# Input report callback type
_HIDReportCallback = ctypes.CFUNCTYPE(
    None,
    ctypes.c_void_p,       # context
    _IOReturn,              # result
    ctypes.c_void_p,        # sender
    _UInt32,                # type (kIOHIDReportTypeInput = 0)
    _UInt32,                # report_id
    ctypes.c_void_p,        # report data (uint8_t *)
    _CFIndex,               # report_length
)

# CoreFoundation helpers
_cf.CFNumberCreate.restype = _CFNumberRef
_cf.CFNumberCreate.argtypes = [_CFAllocatorRef, _UInt32, ctypes.c_void_p]
_cf.CFRelease.argtypes = [_CFDictionaryRef]
_cf.CFStringCreateWithCString.restype = _CFStringRef
_cf.CFStringCreateWithCString.argtypes = [_CFAllocatorRef, ctypes.c_char_p, _UInt32]
_cf.CFRunLoopGetCurrent.restype = _CFRunLoopRef
_cf.CFRunLoopRun.argtypes = []
_cf.CFRunLoopRunInMode.argtypes = [_CFStringRef, ctypes.c_double, ctypes.c_bool]
_cf.CFRunLoopRunInMode.restype = ctypes.c_int32
_cf.CFRunLoopStop.argtypes = [_CFRunLoopRef]

_kCFStringEncodingUTF8 = 0x08000100

# Module-level holder to prevent GC
_hid_callback_ref = None
_hid_running_ctrl_ref = None
_hid_state_ref = None
_hid_match_ref = None

# Previous keyboard state for change detection: report_key → (mod_byte, frozenset(keycodes))
_prev_kb_state = {}


def _parse_keyboard_report(data: bytes, report_key: int, state: "SharedState", now: float):
    """Parse a keyboard HID report and append KeyboardEvents for changes.

    Called with state._lock already held.
    """
    global _prev_kb_state

    if report_key == 0x01:
        # Interface 2 composite keyboard — 9 bytes (mod, reserved, 6 keycodes)
        if len(data) < 9:
            return
        mod_byte = data[0]
        keycodes = [b for b in data[2:8] if b != 0]
    elif report_key == 0:
        # Interface 0 keyboard 6KRO — 8 bytes (mod, reserved, 6 keycodes)
        if len(data) < 8:
            return
        mod_byte = data[0]
        keycodes = [b for b in data[2:8] if b != 0]
    else:
        return

    prev = _prev_kb_state.get(report_key, (0, frozenset()))
    prev_mod, prev_keys = prev

    flags = _hid_mod_to_cg(mod_byte)

    if mod_byte != prev_mod:
        state.kb.append(KeyboardEvent(
            keycode=0,
            kind="flags",
            flags=flags,
            ts=now,
        ))
        state.event_count += 1

    curr_keys = frozenset(keycodes)

    for kc in sorted(curr_keys - prev_keys):
        state.kb.append(KeyboardEvent(
            keycode=kc,
            kind="down",
            flags=flags,
            ts=now,
        ))
        state.event_count += 1
        _log(f"HID key DOWN  keycode=0x{kc:02X} mod=0x{mod_byte:02X} interface={report_key}")
        # Update button state for known buttons
        label = KEY_LABELS.get(kc)
        if label:
            state.buttons[label] = now

    for kc in sorted(prev_keys - curr_keys):
        state.kb.append(KeyboardEvent(
            keycode=kc,
            kind="up",
            flags=flags,
            ts=now,
        ))
        state.event_count += 1
        _log(f"HID key UP    keycode=0x{kc:02X} mod=0x{mod_byte:02X} interface={report_key}")

    _prev_kb_state[report_key] = (mod_byte, curr_keys)


@_HIDReportCallback
def _hid_input_callback(context, result, sender, rtype, report_id, report_ptr, report_len):
    """Called from IOKit thread when a HID input report arrives."""
    if _hid_running_ctrl_ref is None or not _hid_running_ctrl_ref[0]:
        return
    try:
        if rtype != 0:  # Only input reports
            return
        if report_ptr is None or report_len < 1:
            return
        data = ctypes.string_at(report_ptr, report_len)
        now = time.time()

        if _hid_state_ref is None:
            return

        with _hid_state_ref._lock:
            # Case E: Vendor data — log and skip
            if report_id == 0x3F:
                hex_str = " ".join(f"{data[i]:02X}" for i in range(min(report_len, 32)))
                _log(f"HID VENDOR report_id=0x3F len={report_len} first_bytes={hex_str}")
                return

            # Case D: Interface 2 Media Keys (report ID 0x52)
            if report_id == 0x52:
                if report_len >= 2:
                    b = data[1]  # byte 1 = flags (byte 0 is report ID 0x52)
                    if b & 0x01:
                        _hid_state_ref.consumer["play"] = now
                    if b & 0x02:
                        _hid_state_ref.consumer["ff"] = now
                    if b & 0x04:
                        _hid_state_ref.consumer["rew"] = now
                    if b & 0x08:
                        _hid_state_ref.consumer["next_track"] = now
                    if b & 0x10:
                        _hid_state_ref.consumer["prev_track"] = now
                    _hid_state_ref.event_count += 1
                    _log(f"HID media keys report_id=0x52 raw={b:02X}")
                return

            # Case C: Interface 2 Composite Keyboard (report ID 0x01)
            if report_id == 0x01:
                _parse_keyboard_report(data, 0x01, _hid_state_ref, now)
                return

            # No report ID — distinguish by byte length
            if report_id == 0:
                if report_len >= 8:
                    # Case A: Interface 0 Keyboard 6KRO (8 bytes)
                    _parse_keyboard_report(data, 0, _hid_state_ref, now)
                    return
                elif report_len >= 4:
                    # Case B: Interface 1 Consumer/Knob (4 bytes)
                    b0 = data[0]
                    if b0 & 0x01:
                        _hid_state_ref.consumer["vol_up"] = now
                    if b0 & 0x02:
                        _hid_state_ref.consumer["vol_down"] = now
                    for mask, attr in [(0x04, "mute"), (0x10, "stop"), (0x20, "next_track"), (0x40, "play"), (0x80, "prev_track")]:
                        if b0 & mask:
                            _hid_state_ref.consumer[attr] = now
                    _hid_state_ref.event_count += 1
                    if b0:
                        _log(f"HID knob raw={b0:02X} {bin(b0)}")
                    return

            # Catch-all: log unrecognized reports to debug
            hex_str = " ".join(f"{data[i]:02X}" for i in range(min(report_len, 32)))
            _log(f"HID UNRECOGNIZED report_id={report_id} len={report_len} data={hex_str}")

    except Exception:
        pass


def _make_dict(vid: int, pid: int):
    """Create a single matching NSDictionary for a VID/PID pair."""
    return NSDictionary.dictionaryWithObjects_forKeys_(
        [NSNumber.numberWithInt_(vid), NSNumber.numberWithInt_(pid)],
        ["VendorID", "ProductID"],
    )


def _make_matching_array():
    """Create CFArray of matching dictionaries for all hub devices.
    
    Matches both the HID controller (knob + keyboard) and the audio chip (media keys).
    IOHIDManagerSetDeviceMatchingMultiple uses OR semantics.
    """
    dicts = [
        _make_dict(_HUB_VID, _HUB_PID),
        _make_dict(_AUDIO_VID, _AUDIO_PID),
    ]
    return NSArray.arrayWithArray_(dicts)


def _start_hid_monitor(state: "SharedState", running_ctrl: list) -> threading.Thread:
    """Launch a background thread that reads HID reports from all hub interfaces.

    Uses IOHIDManager with kIOHIDOptionsTypeNone so macOS also receives events.
    """

    def _hid_thread():
        global _hid_callback_ref, _hid_running_ctrl_ref, _hid_state_ref, _hid_match_ref
        manager = None
        try:
            manager = _iokit.IOHIDManagerCreate(_kCFAllocatorDefault, 0)
            if not manager:
                _log("HID: IOHIDManagerCreate returned NULL")
                return

            # Schedule on this thread's run loop BEFORE opening
            loop = _cf.CFRunLoopGetCurrent()
            _iokit.IOHIDManagerScheduleWithRunLoop(manager, loop, _kCFRunLoopDefaultMode)

            ret = _iokit.IOHIDManagerOpen(manager, _kIOHIDOptionsTypeNone)
            if ret != 0:
                _log(f"HID: IOHIDManagerOpen failed with 0x{ret:08X}")
                return

            # Set device matching — match HID controller + audio chip (OR logic)
            match_array = _make_matching_array()
            _iokit.IOHIDManagerSetDeviceMatchingMultiple(manager, ctypes.c_void_p(objc.pyobjc_id(match_array)))
            # Keep references alive to prevent GC
            _hid_state_ref = state
            _hid_running_ctrl_ref = running_ctrl
            _hid_match_ref = match_array
            _hid_match_ref = match_array

            # Register the input report callback
            _iokit.IOHIDManagerRegisterInputReportCallback(manager, _hid_input_callback, None)
            _log("HID: IOHIDManager started, monitoring all hub interfaces")

            # Run this thread's run loop until stopped
            while running_ctrl[0]:
                _cf.CFRunLoopRunInMode(_kCFRunLoopDefaultMode, 0.1, 1)

        except Exception as e:
            _log(f"HID thread error: {e}")
        finally:
            if manager:
                _iokit.IOHIDManagerClose(manager)
                _cf.CFRelease(manager)
            _log("HID thread stopped.")

    t = threading.Thread(target=_hid_thread, daemon=True)
    return t


# ═══════════════════════════════════════════════════════════════════════════════
# Shared state (thread-safe)
# ═══════════════════════════════════════════════════════════════════════════════


@dataclass
class KeyboardEvent:
    keycode: int
    kind: str       # "down" | "up" | "flags"
    flags: int
    ts: float
    kb_type: int = 0  # 0 = unknown (HID-sourced, not CGEvent)


# Known hub button keycodes → display label
KEY_LABELS = {
    0x14: "Ctrl+Q",
    0x0F: "Cmd+R",
    0x20: "Ctrl+3",
    0x46: "Cmd+F13",
}


class SharedState:
    def __init__(self):
        self._lock = threading.Lock()
        self.kb: deque[KeyboardEvent] = deque(maxlen=12)
        self.consumer: dict[str, float] = {attr: 0.0 for _, attr in KEYTYPE.values()}
        self.event_count = 0
        # Track button state: label → timestamp (500ms timeout for LED)
        self.buttons: dict[str, float] = {label: 0.0 for label in KEY_LABELS.values()}

    def snapshot(self):
        with self._lock:
            now = time.time()
            consumer_active = {
                attr: (now - ts) < 0.500
                for attr, ts in self.consumer.items()
            }
            btn_active = {
                label: (now - ts) < 0.500
                for label, ts in self.buttons.items()
            }
            return consumer_active, btn_active, list(self.kb), self.event_count


_state = SharedState()

# ═══════════════════════════════════════════════════════════════════════════════
# Display
# ═══════════════════════════════════════════════════════════════════════════════


_ANSI_RE = re.compile(r"\033\[[0-9;]*m")


def _visible_len(s: str) -> int:
    return len(_ANSI_RE.sub("", s))


def _pad(s: str, width: int) -> str:
    return s + " " * max(0, width - _visible_len(s))


def _fmt_flags(flags: int) -> str:
    parts = []
    for mask, sym in MOD_SYMBOLS:
        if flags & mask:
            parts.append(sym)
    return "".join(parts) if parts else "\u2014"


def _build_display(consumer: dict, btn_states: dict, kb_events: list[KeyboardEvent], count: int) -> str:
    try:
        w = min(os.get_terminal_size().columns, 90)
    except OSError:
        w = 80
    inner = w - 2  # minus border chars

    def row(left: str, right: str, content: str) -> str:
        return f"{CYAN}{left}{RESET} {_pad(content, inner - 2)} {CYAN}{right}{RESET}"

    out = CLEAR + HOME

    # Header
    out += row("\u250C", "\u2510", "USB HUB MONITOR -- HID Manager (passive)")
    out += "\n"
    out += row("\u2502", "\u2502", f"Events: {count}    Ctrl+C to exit")
    out += "\n"
    out += row("\u251C", "\u2524", "\u2500" * (inner - 2))
    out += "\n"

    # Knob & Media
    out += row("\u2502", "\u2502", "[Knob & Media Keys]  All interfaces")
    out += "\n"

    parts = []
    for label, attr in [("Vol+", "vol_up"), ("Vol-", "vol_down"), ("Mute", "mute")]:
        state = consumer.get(attr)
        parts.append(f"{ON} {label}" if state else f"{OFF} {label}")
    out += row("\u2502", "\u2502", "  Knob:  " + "  ".join(parts))
    out += "\n"

    parts = []
    for label, attr in [("Play", "play")]:
        parts.append(f"{ON} {label}" if consumer.get(attr) else f"{OFF} {label}")
    out += row("\u2502", "\u2502", "  Media: " + "  ".join(parts))
    out += "\n"

    out += row("\u2502", "\u2502", "")
    out += "\n"

    # Keyboard Buttons
    out += row("\u2502", "\u2502", "[Keyboard Buttons]  Physical buttons (state)")
    out += "\n"

    # Show two rows of buttons
    btn = btn_states
    row1 = "  ".join(f"{ON} {l}" if btn.get(l) else f"{OFF} {l}" for l in ["Ctrl+Q", "Cmd+R"])
    row2 = "  ".join(f"{ON} {l}" if btn.get(l) else f"{OFF} {l}" for l in ["Ctrl+3", "Cmd+F13"])
    out += row("\u2502", "\u2502", "  " + row1)
    out += "\n"
    out += row("\u2502", "\u2502", "  " + row2)
    out += "\n"

    # Fill remaining space
    for _ in range(10):
        out += row("\u2502", "\u2502", "")
        out += "\n"

    # Footer
    out += row("\u2514", "\u2518", "\u2500" * (inner - 2))
    out += "\n"
    out += f"  {ON}=active  {OFF}=inactive  (500ms hold)\n"

    return out


# ═══════════════════════════════════════════════════════════════════════════════
# Main
# ═══════════════════════════════════════════════════════════════════════════════


def main():
    running_ctrl = [True]

    def _stop(sig, frame):
        running_ctrl[0] = False

    signal.signal(signal.SIGINT, _stop)

    _log("STARTUP — monitor.py launched")

    # ── Start HID monitor for all hub interfaces ──────────────────────
    hid_thread = _start_hid_monitor(_state, running_ctrl)
    hid_thread.start()
    _log("STARTUP — HID monitor thread started")

    # ── Display thread ───────────────────────────────────────────────────
    def _display_loop():
        sys.stdout.write(HIDE_CURSOR)
        sys.stdout.flush()
        while running_ctrl[0]:
            consumer, btn_states, kb_events, count = _state.snapshot()
            sys.stdout.write(_build_display(consumer, btn_states, kb_events, count))
            sys.stdout.flush()
            time.sleep(0.040)
        sys.stdout.write(SHOW_CURSOR)
        sys.stdout.write("\n")
        sys.stdout.flush()

    display_thread = threading.Thread(target=_display_loop, daemon=True)
    display_thread.start()

    # ── Wait for Ctrl+C ──────────────────────────────────────────────────
    sys.stdout.write(CLEAR)
    sys.stdout.write("HID monitor active — monitoring all hub interfaces. Press Ctrl+C to exit.\n")
    sys.stdout.flush()

    try:
        while running_ctrl[0]:
            time.sleep(0.1)
    except KeyboardInterrupt:
        pass

    # ── Cleanup ──────────────────────────────────────────────────────────
    running_ctrl[0] = False
    if hid_thread:
        hid_thread.join(timeout=1.5)
    display_thread.join(timeout=0.5)
    print(f"Done. {_state.snapshot()[3]} events captured.")


if __name__ == "__main__":
    main()
