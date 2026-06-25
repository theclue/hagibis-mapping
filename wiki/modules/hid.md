---
title: HID Report Layer
category: modules
source_files:
  - src/hid/mod.rs
  - src/hid/report.rs
  - src/hid/parser.rs
  - src/hid/keycode.rs
  - DEVICE.md
created: 2026-06-25
last_updated: 2026-06-25
---

# HID Report Layer

## Purpose

Parses raw HID input report bytes received from the Hagibis UC-1102AG hub's three USB interfaces (and the separate audio chip) into typed Rust enums and structs. The layer abstracts away the different report formats — Interface 0 (8-byte 6KRO keyboard), Interface 1 (4-byte consumer / knob), Interface 2 composite (keyboard + media + vendor), and the audio chip's consumer control — behind a single `Report` enum and a central `parse()` function.

## Key Files

| File | Purpose |
|------|---------|
| `src/hid/mod.rs` | Module structure; re-exports `KeyCode`, `KeyboardReport`, `ConsumerReport`, `Report` |
| `src/hid/report.rs` | Defines `Report` enum, `KeyboardReport`, `ConsumerReport`, `MediaKeysReport` |
| `src/hid/parser.rs` | Implements `parse()` — dispatches by `report_id` and buffer length |
| `src/hid/keycode.rs` | Defines `KeyCode` newtype with hub-specific button constants, `ConsumerBits` wrapper with bit-test helpers |
| [`DEVICE.md`](../concepts/hid-protocol.md) | Hardware reference: USB topology, interface enumeration, report descriptor dumps, button behaviour |

## Public API

### `Report` enum

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    Keyboard(KeyboardReport),
    Consumer(ConsumerReport),
    MediaKeys(MediaKeysReport),
    Vendor,       // report ID 0x3F — ignored
    Unknown,      // unsupported format
}
```

Every parsed HID event is returned as one of these five variants.

### `KeyboardReport`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardReport {
    pub modifier: u8,        // HID modifier bitmask (Ctrl, Shift, Alt, GUI)
    pub keycodes: Vec<KeyCode>,  // currently pressed keys (0–6 entries)
}
```

The `modifier` byte maps each bit to a modifier key:

| Bit | Modifier |
|-----|----------|
| 0 | Left Control |
| 1 | Left Shift |
| 2 | Left Alt |
| 3 | Left GUI (Command) |
| 4 | Right Control |
| 5 | Right Shift |
| 6 | Right Alt |
| 7 | Right GUI |

The `keycodes` vector contains only non-zero keycode bytes from the 6KRO array. A released key is absent from the vector.

### `ConsumerReport`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumerReport {
    pub bits: ConsumerBits,
}
```

Wraps the first byte of a consumer report. See `ConsumerBits` for individual bit queries.

### `MediaKeysReport`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaKeysReport {
    pub play: bool,
    pub fast_forward: bool,
    pub rewind: bool,
    pub next_track: bool,
    pub prev_track: bool,
}
```

Decoded from the second byte of Interface 2 report ID `0x52`.

### `KeyCode`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode(pub u8);
```

A newtype around a USB HID keyboard usage code (page 0x07). Standard keyboard codes (e.g. `0x04` = keyboard A, `0x07` = keyboard D) pass through as raw values. The named constants below cover only the hub-specific codes.

### `ConsumerBits`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumerBits(pub u8);
```

A newtype around the first byte of a consumer report. Provides named bit-test helpers:

| Method | Bit | Function |
|--------|-----|----------|
| `has_vol_up()` | 0 (`0x01`) | Volume Increment (knob rotate CW) |
| `has_vol_down()` | 1 (`0x02`) | Volume Decrement (knob rotate CCW) |
| `has_mute()` | 2 (`0x04`) | Mute / Unmute (knob click) |
| `has_play_pause()` | 6 (`0x40`) | Play / Pause (audio chip) |

### `parse()`

```rust
pub fn parse(data: &[u8], report_id: u32, report_len: usize) -> Report
```

The single entry point for all HID report parsing. Takes the raw byte buffer, the report ID (0 when no report ID is present), and the expected report length. Returns a typed `Report` variant.

## KeyCode Constants

The hub firmware emits non-standard HID keycodes for the two dual-function physical buttons:

| Constant | Value | Physical Button | Trigger |
|----------|-------|-----------------|---------|
| `BTN_TOP_LEFT` | `0x0F` | Bottom-Right | Hold (>500ms) |
| `BTN_TOP_LEFT_HOLD` | `0x14` | Bottom-Right | Short press |
| `BTN_BOTTOM_RIGHT` | `0x46` | Top-Left | Hold (>500ms) |
| `BTN_BOTTOM_RIGHT_HOLD` | `0x20` | Top-Left | Short press |

These codes are not part of the USB HID keyboard usage table — they are vendor-proprietary values assigned by the hub firmware. The host-side application maps them to actions such as screenshot or application launch.

## Report Structures

### Interface 0 — Standard Keyboard 6KRO

**Source:** Controller HID, Interface 0.
**Report ID:** None (absent from buffer).
**Length:** 8 bytes.

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Modifier bitmask |
| 1 | 1 | Reserved (`0x00`) |
| 2–7 | 6 | Keycode array (6KRO, `0x00` = empty slot) |

### Interface 1 — Consumer Control / Knob

**Source:** Controller HID, Interface 1.
**Report ID:** None.
**Length:** 4 bytes.

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Consumer bits (Vol+, Vol-, Mute, Stop, Next, Prev, Play/Pause) |
| 1–3 | 3 | Reserved |

Consumer bit layout:

| Bit | Mask | Function | Type |
|-----|------|----------|------|
| 0 | `0x01` | Volume Increment (knob rotate CW) | Absolute |
| 1 | `0x02` | Volume Decrement (knob rotate CCW) | Absolute |
| 2 | `0x04` | Mute / Unmute (knob click) | Relative |
| 3 | `0x08` | Unassigned | Relative |
| 4 | `0x10` | Stop | Relative |
| 5 | `0x20` | Scan Next Track | Relative |
| 6 | `0x40` | Scan Prev Track | Relative |
| 7 | `0x80` | Play / Pause | Relative |

The knob produces only bits 0–2. Bits 4–7 are not physically wired on this hub's controller — the Play/Pause button is handled by the separate audio chip.

### Interface 2 — Composite Keyboard (Report ID `0x01`)

**Source:** Controller HID, Interface 2, Collection 1.
**Report ID:** `0x01` (first byte of buffer).
**Length:** 10 bytes.

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Report ID = `0x01` |
| 1 | 1 | Modifier bitmask |
| 2 | 1 | Reserved (`0x00`) |
| 3–8 | 6 | Keycode array (6KRO) |
| 9 | 1 | Extra buttons (Eject, Vendor, Menu, Screensaver) |

### Interface 2 — Media Keys (Report ID `0x52`)

**Source:** Controller HID, Interface 2, Collection 2.
**Report ID:** `0x52`.
**Length:** 2 bytes.

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Report ID = `0x52` |
| 1 | 1 | Media key bitmask |

Media key bitmask (byte 1):

| Bit | Mask | Field |
|-----|------|-------|
| 0 | `0x01` | Play / Pause |
| 1 | `0x02` | Fast Forward |
| 2 | `0x04` | Rewind |
| 3 | `0x08` | Next Track |
| 4 | `0x10` | Previous Track |
| 5–7 | `0xE0` | Padding |

### Interface 2 — Vendor (Report ID `0x3F`)

**Source:** Controller HID, Interface 2, Collection 3.
**Report ID:** `0x3F`.
**Length:** 65 bytes (1 ID + 64 bytes vendor data).

Parsed as `Report::Vendor` and discarded. Used internally by the hub for firmware configuration and diagnostics.

### Audio Chip — Consumer Control

**Source:** USB Audio chip (`0x0C76:0x1710`), HID Consumer Control interface.
**Report ID:** None.
**Length:** 4 bytes.

Same 4-byte format as Interface 1. Only bit 6 (`0x40`, Play/Pause) is wired on this device.

| Bit | Mask | Function |
|-----|------|----------|
| 6 | `0x40` | Play / Pause (Absolute: 1 = pressed, 0 = released) |

## Parser Dispatch Logic

The `parse()` function selects the parsing strategy based on two inputs: the `report_id` (0 when absent) and the `report_len` from the USB descriptor.

```
parse(data, report_id, report_len)
  │
  ├─ data is empty ──────────────────────────────→ Report::Unknown
  │
  ├─ report_id = 0x3F ───────────────────────────→ Report::Vendor
  │
  ├─ report_id = 0x52 ───────────────────────────→ Report::MediaKeys
  │   └─ data[1]:  play, ff, rewind, next, prev bits
  │
  ├─ report_id = 0x01 ───────────────────────────→ Report::Keyboard
  │   └─ data[1]:  modifier
  │   └─ data[3..9]:  keycodes (filtered non-zero)
  │
  ├─ report_id = 0 ──┬─ report_len >= 8 ────────→ Report::Keyboard  (Interface 0)
  │   (no report ID)  │   └─ data[0]:  modifier
  │                   │   └─ data[2..8]:  keycodes (filtered non-zero)
  │                   │
  │                   ├─ report_len >= 4 ────────→ Report::Consumer  (Interface 1 / audio)
  │                   │   └─ data[0]:  ConsumerBits
  │                   │
  │                   └─ otherwise ──────────────→ Report::Unknown
  │
  └─ unknown report_id ──────────────────────────→ Report::Unknown
```

The key distinction between Interface 0 and Interface 2 keyboard reports:
- **Interface 0** has no report ID byte, so `report_id = 0`, modifier is at `data[0]`, and `report_len >= 8`.
- **Interface 2** (report ID `0x01`) has a leading report ID byte (included in the buffer), so `report_id = 0x01`, modifier is at `data[1]`, and `data.len() >= 9`.

Both produce a `Report::Keyboard(KeyboardReport { .. })` with the same struct fields.

## Dispatch Flow

```mermaid
graph LR
    Raw[Raw HID Data] --> Parse{parse()}
    Parse -->|report_id=0| Len{report_len}
    Len -->|>=8| IF0_KB[Interface 0 Keyboard]
    Len -->|>=4| Cons[Interface 1 / Audio Consumer]
    Len -->|else| Unk1[Unknown]

    Parse -->|report_id=0x01| IF2_KB[Interface 2 Keyboard]
    Parse -->|report_id=0x52| MK[Interface 2 Media Keys]
    Parse -->|report_id=0x3F| Vend[Vendor - Ignored]
    Parse -->|other| Unk2[Unknown]

    IF0_KB --> KB[KeyboardReport]
    IF2_KB --> KB
    Cons --> CR[ConsumerReport]
    MK --> MKR[MediaKeysReport]
    Vend --> R[Report enum]
    KB --> R
    CR --> R
    MKR --> R
    Unk1 --> R
    Unk2 --> R
```

## Tests

The parser module includes unit tests covering all dispatch paths:

| Test | Scenario |
|------|----------|
| `empty_buffer` | Empty slice returns `Report::Unknown` |
| `report_id_vendor` | Report ID `0x3F` returns `Report::Vendor` |
| `report_id_unknown` | Unknown report ID returns `Report::Unknown` |
| `if0_keyboard_no_modifiers` | Interface 0 with one keycode, no modifiers pressed |
| `if0_keyboard_with_modifiers` | Interface 0 with modifier bits set (`0x09`) |
| `if0_keyboard_no_keys_pressed` | Interface 0 with all-zero keycode array (empty report) |
| `if1_consumer_vol_up` | Interface 1 with Volume Increment bit set (`0x01`) |
| `if1_consumer_mute` | Interface 1 with Mute bit set (`0x04`) |
| `if1_consumer_play_pause` | Interface 1 with Play/Pause bit set (`0x40`) |
| `if2_media_keys_play` | Report ID `0x52` with Play bit set |
| `if2_media_keys_next_prev` | Report ID `0x52` with Next + Prev bits set (`0x18`) |
| `if2_keyboard_composite` | Report ID `0x01` with modifier `0x08` and keycode `0x46` |
