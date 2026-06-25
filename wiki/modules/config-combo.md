---
title: "Combo String Parser"
description: "Parses human-readable key combo strings (e.g. Ctrl+Shift+Q) into typed virtual-key events for platform-specific input simulation"
category: "modules"
source_files:
  - "src/config/combo.rs"
created: "2026-06-25"
last_updated: "2026-06-25"
---

# Combo String Parser

## Purpose

Translates user-facing keyboard shortcut strings like `"Ctrl+Shift+Q"` or `"Cmd+Option+Space"` into a structured `ParsedCombo` that contains a platform-specific virtual-key code and a modifier bitmask. This is the entry point for [config](../modules/config.md)-driven [key bindings](../config/config-toml.md) — the rest of the system consumes `ParsedCombo` values to simulate keystrokes or match incoming input.

The module handles two concerns that are often tangled: (1) parsing the human-readable DSL (modifier names, key name aliases, whitespace tolerance), and (2) mapping key names to platform `u16` virtual-key codes via `#[cfg]` conditional compilation (macOS Carbon codes vs Windows VK codes).

## Key Files

| File | Role |
|------|------|
| `src/config/combo.rs` | Full implementation: parser, key-to-VK mapping, and unit tests |

## Public API

### `ParsedCombo`

```rust
#[derive(Debug, Clone)]
pub struct ParsedCombo {
    pub vk: u16,         // Platform virtual-key code
    pub modifiers: u8,   // USB HID modifier bitmask
}
```

- `vk` — platform-specific key code (see Platform-Specific VK Mappings below).
- `modifiers` — bitmask where bits map to modifier keys using the USB HID convention:
  - Bit 0 (0x01) = Ctrl / Control
  - Bit 1 (0x02) = Shift
  - Bit 2 (0x04) = Alt / Option
  - Bit 3 (0x08) = Super / Cmd / Win / Meta

### `parse_combo(binding: &str) -> Option<ParsedCombo>`

Main entry point. Returns `None` if the string contains an unknown modifier, an unknown key name, or is empty.

**Format**: optional modifier(s) separated by `+`, then the key. Whitespace around `+` is tolerated. The last token is always treated as the key; everything before it is a modifier.

```rust
parse_combo("Ctrl+Shift+N");      // Some(ParsedCombo { modifiers: 1|2, ... })
parse_combo("Space");             // Some(ParsedCombo { modifiers: 0, ... })
parse_combo("Bogus+Q");           // None (unknown modifier)
parse_combo("SomeUnknownKey");    // None (unknown key)
parse_combo("");                  // None
parse_combo("Ctrl");              // None (only modifiers, no key)
```

## Supported Modifiers

All modifier names are case-insensitive.

| Name(s) | Bit | Mask |
|---------|-----|------|
| `Ctrl`, `Control` | 0 | `0x01` |
| `Shift` | 1 | `0x02` |
| `Alt`, `Option` | 2 | `0x04` |
| `Cmd`, `Command`, `Super`, `Meta`, `Win` | 3 | `0x08` |

Exactly one bit is set per modifier name — no multi-bit aliases exist. All four modifiers can be combined (e.g. `Ctrl+Shift+Alt+Super+Space`).

## Supported Key Names

All key names are case-insensitive. Aliases are accepted where listed.

### Letters (A-Z)

| Input | macOS VK | Windows VK |
|-------|----------|------------|
| `a` | `0x00` | `0x41` |
| `b` | `0x0B` | `0x42` |
| `c` | `0x08` | `0x43` |
| `d` | `0x02` | `0x44` |
| `e` | `0x0E` | `0x45` |
| `f` | `0x03` | `0x46` |
| `g` | `0x05` | `0x47` |
| `h` | `0x04` | `0x48` |
| `i` | `0x22` | `0x49` |
| `j` | `0x26` | `0x4A` |
| `k` | `0x28` | `0x4B` |
| `l` | `0x25` | `0x4C` |
| `m` | `0x2E` | `0x4D` |
| `n` | `0x2D` | `0x4E` |
| `o` | `0x1F` | `0x4F` |
| `p` | `0x23` | `0x50` |
| `q` | `0x0C` | `0x51` |
| `r` | `0x0F` | `0x52` |
| `s` | `0x01` | `0x53` |
| `t` | `0x11` | `0x54` |
| `u` | `0x20` | `0x55` |
| `v` | `0x09` | `0x56` |
| `w` | `0x0D` | `0x57` |
| `x` | `0x07` | `0x58` |
| `y` | `0x10` | `0x59` |
| `z` | `0x06` | `0x5A` |

### Digits (0-9)

| Input | macOS VK | Windows VK |
|-------|----------|------------|
| `0` | `0x1D` | `0x30` |
| `1` | `0x12` | `0x31` |
| `2` | `0x13` | `0x32` |
| `3` | `0x14` | `0x33` |
| `4` | `0x15` | `0x34` |
| `5` | `0x17` | `0x35` |
| `6` | `0x16` | `0x36` |
| `7` | `0x1A` | `0x37` |
| `8` | `0x1C` | `0x38` |
| `9` | `0x19` | `0x39` |

### Function Keys (F1-F15)

| Input | macOS VK | Windows VK |
|-------|----------|------------|
| `f1` | `0x7A` | `0x70` |
| `f2` | `0x78` | `0x71` |
| `f3` | `0x63` | `0x72` |
| `f4` | `0x76` | `0x73` |
| `f5` | `0x60` | `0x74` |
| `f6` | `0x61` | `0x75` |
| `f7` | `0x62` | `0x76` |
| `f8` | `0x64` | `0x77` |
| `f9` | `0x65` | `0x78` |
| `f10` | `0x6D` | `0x79` |
| `f11` | `0x67` | `0x7A` |
| `f12` | `0x6F` | `0x7B` |
| `f13` | `0x69` | `0x7C` |
| `f14` | `0x6B` | `0x7D` |
| `f15` | `0x71` | `0x7E` |

### Named Special Keys

| Accepted Input(s) | macOS VK | Windows VK |
|-------------------|----------|------------|
| `space` | `0x31` | `0x20` |
| `enter`, `return` | `0x24` | `0x0D` |
| `escape`, `esc` | `0x35` | `0x1B` |
| `backspace` | `0x33` | `0x08` |
| `tab` | `0x30` | `0x09` |
| `delete`, `del` | `0x75` | `0x2E` |
| `home` | `0x73` | `0x24` |
| `end` | `0x77` | `0x23` |
| `pageup` | `0x74` | `0x21` |
| `pagedown` | `0x79` | `0x22` |
| `arrowleft`, `left` | `0x7B` | `0x25` |
| `arrowright`, `right` | `0x7C` | `0x27` |
| `arrowup`, `up` | `0x7E` | `0x26` |
| `arrowdown`, `down` | `0x7D` | `0x28` |

### Unsupported Platforms

On platforms other than macOS and Windows (e.g. Linux), `key_to_vk` returns `None` for every key, so `parse_combo` will always return `None` regardless of input.

## Platform-Specific VK Mappings

The VK values differ significantly between macOS and Windows:

- **[macOS](../modules/backend-macos.md)**: uses Carbon virtual-key codes (a legacy HID layout from the original Mac USB keyboard firmware). These are not contiguous — letters map to seemingly arbitrary byte values (e.g. `a` = `0x00`, `b` = `0x0B`).
- **[Windows](../modules/backend-windows.md)**: uses the standard Windows Virtual-Key codes, where letters are simply the ASCII uppercase value (e.g. `a` = `0x41` = `'A'`), digits are their ASCII values (`0` = `0x30`), and special keys are defined in the `WinUser.h` header.

The correct mapping is selected at compile time via `#[cfg(target_os = "macos")]` and `#[cfg(target_os = "windows")]` attributes. A single binary never contains both mappings — the other arm is dead code eliminated.

## Internal Details

### `key_to_vk(key: &str) -> Option<u16>`

Private helper that lowercases the input and matches against the platform-conditional key-name tables. Any unrecognized key name returns `None`, which causes `parse_combo` to return `None` as well.

### Modifier Bitmask Construction

Modifiers accumulate bits using OR assignment:

```rust
match mod_str.to_lowercase().as_str() {
    "ctrl" | "control" => modifiers |= 1,
    "shift"             => modifiers |= 2,
    "alt" | "option"    => modifiers |= 4,
    "cmd" | "command" | "super" | "meta" | "win" => modifiers |= 8,
    _ => return None,
}
```

## Error Handling

`parse_combo` does not distinguish between error types — all failures (unknown modifier, unknown key, empty string, modifier without key) produce `None`. This is intentional: the caller typically falls back to a default binding or logs a warning when `None` is returned.

## Usage Examples

### Simple key binding (no modifiers)

```rust
let combo = parse_combo("Space").unwrap();
assert_eq!(combo.modifiers, 0);
// vk is 0x31 on macOS, 0x20 on Windows
```

### Single modifier

```rust
let combo = parse_combo("Ctrl+q").unwrap();
assert_eq!(combo.modifiers, 1); // 0x01
```

### Multiple modifiers

```rust
let combo = parse_combo("Shift+Cmd+N").unwrap();
assert_eq!(combo.modifiers, 2 | 8); // 0x0A
```

### All four modifiers

```rust
let combo = parse_combo("Ctrl+Shift+Alt+Super+Space").unwrap();
assert_eq!(combo.modifiers, 1 | 2 | 4 | 8); // 0x0F
```

### Alias equivalence

```rust
assert_eq!(parse_combo("Command+x").unwrap().modifiers, 8);
assert_eq!(parse_combo("Win+x").unwrap().modifiers,      8);
assert_eq!(parse_combo("Meta+x").unwrap().modifiers,     8);
assert_eq!(parse_combo("Option+x").unwrap().modifiers,   4);
assert_eq!(parse_combo("Control+x").unwrap().modifiers,  1);
```

### Whitespace tolerance

```rust
let combo = parse_combo("Ctrl + Shift + N").unwrap();
assert_eq!(combo.modifiers, 1 | 2);
```

### Error cases

```rust
assert!(parse_combo("Bogus+q").is_none());      // unknown modifier
assert!(parse_combo("BogusKey").is_none());      // unknown key
assert!(parse_combo("").is_none());              // empty string
assert!(parse_combo("Ctrl").is_none());           // modifier without key
```
