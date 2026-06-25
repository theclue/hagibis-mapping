---
title: "Hub HID Protocol"
description: "Complete reference document for the Hagibis UC-1102AG USB hub's HID protocol — describing USB topology, interface layout, report descriptors, button keycode maps, consumer control bits, knob behavior, and known firmware issues"
category: "concepts"
source_files:
  - "DEVICE.md"
  - "src/hid/report.rs"
  - "src/hid/parser.rs"
  - "src/hid/keycode.rs"
created: "2026-06-25"
last_updated: "2026-06-25"
---

# Hub HID Protocol

## Overview

The Hagibis UC-1102AG USB-C Hub contains two independent HID-capable devices that together implement the physical controls (four buttons, one rotary knob with click, one RGB LED ring). The primary HID controller is a custom microcontroller (vendor `0x05AC`, product `0x029C`) that exposes three HID interfaces covering keyboard keystrokes, consumer volume/mute events, and vendor-defined data. A separate USB audio chip (vendor `0x0C76`, product `0x1710`) handles the Play/Pause button via its own HID Consumer Control interface.

The protocol is entirely custom — the 437-byte combined report descriptor does not match any standard HID profile. All physical controls are wired to the firmware of these two chips; the host operating system sees only the HID reports they produce.

## USB Topology

The HID devices sit on the USB 2.0 (Hi-Speed, 480 Mb/s) companion bus behind a Bridgesil BGS2510 hub controller:

```
USB-C upstream
 └── USB 2.1 Hub (Bridgesil BGS2510, 0x35D6:0x2510)
      ├── HID Device (0x05AC:0x029C) ← Physical controls (buttons + knob)
      │    Manufacturer: Bridgesil
      │    Version: 0x0108
      │    Link Speed: 480 Mb/s
      │    Power: 12 W sink (2400 mA), 2.5 W allocated (500 mA)
      │    3 HID interfaces
      │
      └── USB 2.0 Hub (Terminus, 0x1A40:0x0801, 1 port)
           └── Audio Device (0x0C76:0x1710) ← Play/Pause button
                Link Speed: 12 Mb/s (Full Speed)
                USB Audio Class + HID Consumer Control
```

The controller HID uses the Apple vendor ID (`0x05AC`) for automatic driver matching on macOS — the actual manufacturer is Bridgesil, not Apple. Any software that enumerates HID devices must filter by `manufacturer == "Bridgesil"` or `product_string == "Hid Device"` to avoid confusion with genuine Apple peripherals.

## HID Controller — Interface Layout

The HID controller (`0x05AC:0x029C`) exposes 3 interfaces. Interfaces 0 and 1 send reports without a Report ID byte. Interface 2 is a composite interface containing 3 HID collections distinguished by Report ID.

| Interface | Usage | Report ID | Report Size | Description |
|-----------|-------|-----------|-------------|-------------|
| 0 | Generic Desktop → Keyboard (0x01:0x06) | None | 8 bytes | Standard 6KRO keyboard |
| 1 | Consumer → Consumer Control (0x0C:0x01) | None | 4 bytes | Knob rotate + click |
| 2, coll 1 | Generic Desktop → Keyboard (0x01:0x06) | `0x01` | 10 bytes | Keyboard 6KRO + Extras |
| 2, coll 2 | Consumer → Consumer Control (0x0C:0x01) | `0x52` | 2 bytes | Media key flags |
| 2, coll 3 | Vendor-Defined (0xFF:0x06) | `0x3F` | 65 bytes | Vendor data (firmware/diag) |

### Interface 0 — Standard Keyboard 6KRO

Report descriptor: 67 bytes. No Report ID.

**Input report (8 bytes, host receives)**:

| Byte | Bits | Field | Description |
|------|------|-------|-------------|
| 0 | 0 | Left Control | Modifier bitmap |
| 0 | 1 | Left Shift | |
| 0 | 2 | Left Alt | |
| 0 | 3 | Left GUI (Command) | |
| 0 | 4 | Right Control | |
| 0 | 5 | Right Shift | |
| 0 | 6 | Right Alt | |
| 0 | 7 | Right GUI | |
| 1 | 0–7 | Reserved | Always 0x00 |
| 2–7 | 0–7 | Keycode array | 6KRO (0x00 = no key pressed) |

**Output report (1 byte, host → device)**: LED status — Num Lock (bit 0), Caps Lock (bit 1), Scroll Lock (bit 2), Compose (bit 3), Kana (bit 4), padding (bits 5–7).

**Raw descriptor**:

```
05 01  Usage Page (Generic Desktop)
09 06  Usage (Keyboard)
a1 01  Collection (Application)
05 07  Usage Page (Keyboard/Keypad)
19 e0  Usage Minimum (Left Control)
29 e7  Usage Maximum (Right GUI)
15 00  Logical Minimum (0)
25 01  Logical Maximum (1)
95 08  Report Count (8)
75 01  Report Size (1 bit)
81 02  INPUT (Data,Var,Abs) → Modifier bitmap
95 01  Report Count (1)
75 08  Report Size (8 bit)
81 03  INPUT (Const,Var,Abs) → Reserved byte
05 07  Usage Page (Keyboard/Keypad)
95 06  Report Count (6)
75 08  Report Size (8 bit)
15 00  Logical Minimum (0)
26 ff 00  Logical Maximum (255)
19 00  Usage Minimum (0)
2a ff 00  Usage Maximum (255)
81 00  INPUT (Data,Array,Abs) → Keycode array 6KRO
05 08  Usage Page (LEDs)
25 01  Logical Maximum (1)
95 05  Report Count (5)
75 01  Report Size (1 bit)
19 01  Usage Minimum (Num Lock)
29 05  Usage Maximum (Kana)
91 02  OUTPUT (Data,Var,Abs) → LED status
95 01  Report Count (1)
75 03  Report Size (3 bit)
91 03  OUTPUT (Const,Var,Abs)
c0  End Collection
```

### Interface 1 — Consumer Control (Knob)

Report descriptor: 60 bytes. No Report ID.

**Input report (4 bytes, host receives)**:

| Byte | Bit | Usage | Type | Description |
|------|-----|-------|------|-------------|
| 0 | 0 | Volume Increment (0x0C:0xE9) | **Absolute** | 1 while knob rotates CW |
| 0 | 1 | Volume Decrement (0x0C:0xEA) | **Absolute** | 1 while knob rotates CCW |
| 0 | 2 | Mute (0x0C:0xE2) | **Relative** | Edge trigger on click |
| 0 | 3 | Unassigned (0x00) | Relative | Not used |
| 0 | 4 | Stop (0x0C:0xB7) | Relative | Not wired on this interface |
| 0 | 5 | Scan Next Track (0x0C:0xB5) | Relative | Not wired on this interface |
| 0 | 6 | Scan Prev Track (0x0C:0xB6) | Relative | Not wired on this interface |
| 0 | 7 | Play/Pause (0x0C:0xCD) | Relative | Not wired on this interface |
| 1–3 | 0–7 | Reserved | Absolute | Always 0x00 |

**Type semantics**:

| Type | Behavior |
|------|----------|
| Absolute | Bit = 1 while the knob is actively rotating, 0 when stationary |
| Relative | Transitions 0→1 produce discrete events (cumulative) |

> **Firmware discrepancy**: Bits 6 and 7 are inverted on this specific firmware revision. The report descriptor declares bit 6 = Scan Prev Track, bit 7 = Play/Pause, but the device actually sends Play/Pause on bit 6 and Scan Prev Track on bit 7. Any implementation must verify the mapping with a live test.

**Raw descriptor**:

```
05 0c  Usage Page (Consumer)
09 01  Usage (Consumer Control)
a1 01  Collection (Application)
15 00  Logical Minimum (0)
25 01  Logical Maximum (1)
09 e9  Usage (Volume Increment)
09 ea  Usage (Volume Decrement)
75 01  Report Size (1 bit)
95 02  Report Count (2)
81 02  INPUT (Data,Var,Abs)
09 e2  Usage (Mute)
09 00  Usage (Unassigned)
95 02  Report Count (2)
81 06  INPUT (Data,Var,Rel)
05 0c  Usage Page (Consumer)
09 b7  Usage (Stop)
09 b5  Usage (Scan Next Track)
09 b6  Usage (Scan Previous Track)
09 cd  Usage (Play/Pause)
95 04  Report Count (4)
81 06  INPUT (Data,Var,Rel)
26 ff 00  Logical Maximum (255)
09 00  Usage (Unassigned)
75 08  Report Size (8 bit)
95 03  Report Count (3)
81 02  INPUT (Data,Var,Abs)
09 00  Usage (Unassigned)
95 04  Report Count (4)
91 02  OUTPUT (Data,Var,Abs)
c0  End Collection
```

### Interface 2 — Composite

Report descriptor: 207 bytes. Contains 3 HID collections, each with its own Report ID byte as the first byte of every input report.

#### Collection 1 — Keyboard 6KRO (Report ID 0x01)

**Input report (10 bytes)**:

| Byte | Bits | Field | Description |
|------|------|-------|-------------|
| 0 | 0–7 | Report ID | `0x01` |
| 1 | 0 | Left Control | Modifier bitmap |
| 1 | 1 | Left Shift | |
| 1 | 2 | Left Alt | |
| 1 | 3 | Left GUI (Command) | |
| 1 | 4 | Right Control | |
| 1 | 5 | Right Shift | |
| 1 | 6 | Right Alt | |
| 1 | 7 | Right GUI | |
| 2 | 0–7 | Reserved | Always 0x00 |
| 3–8 | 0–7 | Keycode array | 6KRO (0x00 = no key pressed) |
| 9 | 0 | Eject (Consumer 0x0C:0xB8) | Extra button |
| 9 | 1 | Vendor-defined (0xFF:0x03) | |
| 9 | 2 | Menu (Consumer 0x0C:0x40) | |
| 9 | 3 | AL Terminal Lock / Screensaver (0x0C:0x19E) | |
| 9 | 4–7 | Padding | |

**Output report**: 5-bit LED status + 3-bit padding (same layout as Interface 0).

#### Collection 2 — Media Keys (Report ID 0x52)

**Input report (2 bytes)**:

| Byte | Bits | Field | Description |
|------|------|-------|-------------|
| 0 | 0–7 | Report ID | `0x52` |
| 1 | 0 | Play/Pause (0x0C:0xCD) | |
| 1 | 1 | Fast Forward (0x0C:0xB3) | |
| 1 | 2 | Rewind (0x0C:0xB4) | |
| 1 | 3 | Scan Next Track (0x0C:0xB5) | |
| 1 | 4 | Scan Prev Track (0x0C:0xB6) | |
| 1 | 5–7 | Padding | |

#### Collection 3 — Vendor-Defined (Report ID 0x3F)

**Input report (65 bytes)**: Report ID `0x3F` followed by 64 bytes of proprietary data. Usage Page `0xFF00`, Usage `0x06`. Presumed to be used for firmware configuration and internal diagnostics. The override-hub implementation ignores this report entirely.

## Audio Chip — Consumer Control

The USB audio chip (`0x0C76:0x1710`) exposes a separate HID Consumer Control interface for the Play/Pause button. This is the only physical control on the audio chip — the volume knob and mute button are on the controller HID's Interface 1.

**Input report (4 bytes, no Report ID)**:

| Bit | Mask | Function | Type |
|-----|------|----------|------|
| 0 | `0x01` | Volume Increment | Not wired on this chip |
| 1 | `0x02` | Volume Decrement | Not wired |
| 2 | `0x04` | Mute | Not wired |
| 3 | `0x08` | — | Not wired |
| 4 | `0x10` | Stop | Not wired |
| 5 | `0x20` | Scan Next Track | Not wired |
| 6 | `0x40` | **Play/Pause** | Absolute — 1 while pressed, 0 when released |
| 7 | `0x80` | Scan Prev Track | Not wired |

The report descriptor declares Play/Pause on bit 0 (`0x01`), but live monitoring shows the actual report uses bit 6 (`0x40`). This discrepancy suggests the firmware sends reports in a format different from the declared descriptor, or the descriptor read via `ioreg` belongs to a different interface than the one actually used for the button.

**Raw descriptor (audio chip HID)**:

```
05 0c  Usage Page (Consumer)
09 01  Usage (Consumer Control)
a1 01  Collection (Application)
09 cd  Play/Pause          ← bit 1 in descriptor
09 ea  Volume Decrement    ← bit 2
09 e9  Volume Increment    ← bit 3
15 00  Logical Min 0
25 01  Logical Max 1
95 03  Report Count 3
75 01  Report Size 1
81 02  INPUT (Data,Var,Abs)  — 3 bit di dati
95 05  Report Count 5
81 03  INPUT (Cnst,Var,Abs)  — 5 bit padding
c0     End Collection
```

## Button Keycode Map

The HID controller firmware implements dual-function behavior for two buttons by measuring press duration (threshold ~500 ms).

### Physical Controls

| # | Position | Default Label | HID Source | Notes |
|---|----------|---------------|------------|-------|
| 1 | Top-Left | Lock Screen | Interface 0 Keyboard | Dual-function: short vs hold |
| 2 | Bottom-Left | LED knob cycle | **None** | Firmware-only; no USB report |
| 3 | Top-Right | Play/Pause | Audio chip Consumer | Bit 6 (`0x40`) |
| 4 | Bottom-Right | Screenshot | Interface 0 Keyboard | Dual-function: short vs hold |
| — | Knob rotate | Volume ± | Interface 1 Consumer | Absolute bits |
| — | Knob click | Mute | Interface 1 Consumer | Relative edge trigger |

### Keystroke Combinations

| Button | Trigger | Keycode | HID Modifier | Resulting Combination |
|--------|---------|---------|--------------|----------------------|
| Top-Left | Short press (<500 ms) | `0x0F` | `0x01` (Left Ctrl) | Ctrl+3 |
| Top-Left | Hold (>500 ms) | `0x46` | `0x08` (Left GUI) | Cmd+F13 |
| Bottom-Right | Short press (<500 ms) | `0x14` | `0x01` (Left Ctrl) | Ctrl+Q |
| Bottom-Right | Hold (>500 ms) | `0x0F` | `0x08` (Left GUI) | Cmd+R |

> The hub firmware sends raw keystrokes — the host OS interprets them according to the active application context. On macOS these combinations produce Lock Screen and Screenshot respectively, but that is a macOS behavior, not a hub feature.

### Keycode Constants

The [Rust implementation](../modules/hid.md) defines symbolic constants for the button keycodes in `keycode.rs`:

| Constant | Value | Button |
|----------|-------|--------|
| `KeyCode::BTN_TOP_LEFT` | `0x0F` | Top-left short press |
| `KeyCode::BTN_TOP_LEFT_HOLD` | `0x14` | Top-left hold |
| `KeyCode::BTN_BOTTOM_RIGHT` | `0x46` | Bottom-right short press |
| `KeyCode::BTN_BOTTOM_RIGHT_HOLD` | `0x20` | Bottom-right hold |

### Consumer Bits

The `ConsumerBits` struct (wrapping a `u8`) exposes per-bit query methods for Interface 1 and audio chip reports:

| Method | Mask | Physical Control |
|--------|------|------------------|
| `has_vol_up()` | `0x01` | Knob rotate CW |
| `has_vol_down()` | `0x02` | Knob rotate CCW |
| `has_mute()` | `0x04` | Knob click |
| `has_play_pause()` | `0x40` | Play/Pause button (audio chip) |

## Knob Behavior

The rotary encoder is connected to Interface 1 of the HID controller.

| Action | Bit | Mask | Type | Electrical Behavior |
|--------|-----|------|------|---------------------|
| Rotate clockwise | 0 | `0x01` | Absolute | `1` while detent passes, `0` at rest |
| Rotate counter-clockwise | 1 | `0x02` | Absolute | `1` while detent passes, `0` at rest |
| Press (click) | 2 | `0x04` | Relative | Each press produces one 0→1 transition |

**Absolute vs Relative**:

- **Volume Increment/Decrement** (bits 0–1): The bit is asserted for the duration of the rotation. The host must detect the leading edge and count detents to accumulate volume steps.
- **Mute** (bit 2): The bit toggles on each press. The host should respond to the 0→1 edge (or 1→0 edge, whichever is preferred) and debounce programmatically.

## LED Knob (Bottom-Left Button)

The bottom-left button is connected directly to the HID controller firmware and produces **no HID report** to the host. Its sole function is cycling the integrated RGB LED ring through a sequence of colors and patterns (off → color 1 → color 2 → ...). The LED state is managed entirely within the microcontroller and is not controllable or observable via USB.

## Report Parsing

The Rust implementation in `parser.rs` dispatches on report ID and buffer length to determine the report type:

| Condition | Report Type | Fields Extracted |
|-----------|-------------|------------------|
| Report ID `0x3F` | `Report::Vendor` | None (ignored) |
| Report ID `0x52`, len ≥ 2 | `Report::MediaKeys` | `play`, `fast_forward`, `rewind`, `next_track`, `prev_track` |
| Report ID `0x01`, len ≥ 9 | `Report::Keyboard` | `modifier` from `data[1]`, `keycodes` from `data[3..9]` |
| Report ID = 0, len ≥ 8 | `Report::Keyboard` | `modifier` from `data[0]`, `keycodes` from `data[2..8]` |
| Report ID = 0, len ≥ 4 | `Report::Consumer` | `bits` from `data[0]` |
| Anything else | `Report::Unknown` | None |

The buffer includes the Report ID byte when present (Interface 2). Interfaces 0 and 1 have no Report ID, so `data[0]` is the first payload byte.

## Report Summary

| Source | Report ID | Size | Content |
|--------|-----------|------|---------|
| Controller HID, Interface 0 | None | 8 bytes | Keyboard 6KRO (1 modifier + 1 reserved + 6 keycodes) |
| Controller HID, Interface 1 | None | 4 bytes | Knob (Vol±, Mute, media bits) |
| Controller HID, Interface 2, coll 1 | `0x01` | 10 bytes | Keyboard 6KRO with extras (Eject, Menu, Screensaver) |
| Controller HID, Interface 2, coll 2 | `0x52` | 2 bytes | Media keys (Play, FF, Rewind, Next, Prev) |
| Controller HID, Interface 2, coll 3 | `0x3F` | 65 bytes | Vendor data (1 ID + 64 bytes payload) |
| Audio chip Consumer Control | None | 4 bytes | Play/Pause on bit 6 |

## Known Issues

### Play/Pause Bit Position Mismatch

The audio chip's HID report descriptor declares Play/Pause on bit 0 (`0x01`), but live monitoring reveals the actual report uses bit 6 (`0x40`). There are two possible explanations:

1. The descriptor read by the OS belongs to a different interface than the one the button actually uses.
2. The firmware sends reports in a format inconsistent with the declared descriptor.

The override-hub implementation uses the empirically observed bit 6 mapping (`ConsumerBits::has_play_pause()` tests `0x40`).

### Interface 1 Bit Inversion

On Interface 1, the report descriptor declares bit 6 = Scan Prev Track and bit 7 = Play/Pause, but the device sends Play/Pause on bit 6 and Scan Prev Track on bit 7. This inversion may be specific to this firmware revision (version `0x0108`).

### Play/Pause Triggers Spotlight (macOS)

On macOS Tahoe, the Play/Pause consumer event generated by the audio chip (`0x40`) triggers Spotlight search instead of toggling media playback. This is a macOS consumer-page mapping issue — the operating system routes the HID consumer control event to the wrong system service. Users who want media play/pause functionality may need to remap or intercept the event.

### No Host-Controllable LED

The RGB LED ring is driven entirely by firmware-internal state machine via the bottom-left button. There is no HID output report or vendor command documented to control the LED color/pattern from the host. Reverse-engineering the vendor-defined Report ID `0x3F` payload may (or may not) expose LED control registers.

### [macOS](../modules/backend-macos.md) Exclusive Access

Opening the HID device with exclusive access (`O_RDWR` via hidapi) disconnects the system driver and stops normal keyboard/consumer operation. Non-exclusive access (`kIOHIDOptionsTypeNone` on IOKit) is required to observe reports without disrupting functionality. The Consumer Control events from Interface 1 are consumed by `AppleUserHIDEventService` before reaching the Quartz event system, so `CGEventTap` cannot intercept them.

## Data Structures (Rust)

The parsing layer defines three report types and an enum:

**`KeyboardReport`** — modifier byte + up to 6 keycodes.
**`ConsumerReport`** — wraps a `ConsumerBits` byte.
**`MediaKeysReport`** — five booleans (play, fast_forward, rewind, next_track, prev_track).
**`Report`** — enum of `Keyboard(KeyboardReport)`, `Consumer(ConsumerReport)`, `MediaKeys(MediaKeysReport)`, `Vendor`, `Unknown`.

See `src/hid/report.rs` and `src/hid/parser.rs` for the full definitions and parse logic.
