/// Parsed result of a key combo like `"Ctrl+Q"`.
#[derive(Debug, Clone)]
pub struct ParsedCombo {
    /// Platform Virtual-Key code for the main key.
    pub vk: u16,
    /// Modifier bitmask (USB HID convention: 1=Ctrl, 2=Shift, 4=Alt, 8=Super).
    pub modifiers: u8,
}

/// Parse a key combo string like `"Ctrl+Q"` or `"Cmd+Shift+3"`.
///
/// Format: optional modifier(s) separated by `+`, then the key.
/// The last token is the key; everything before it is modifiers.
///
/// Supported modifier names (case-insensitive):
///   `Ctrl`, `Control`, `Shift`, `Alt`, `Option`,
///   `Cmd`, `Command`, `Super`, `Meta`, `Win`
///
/// Supported key names (case-insensitive):
///   Single letters: `A`–`Z`
///   Digits: `0`–`9`
///   Function keys: `F1`–`F19`
///   Named keys: `Space`, `Enter`, `Escape`, `Backspace`, `Tab`, `Delete`,
///     `Home`, `End`, `PageUp`, `PageDown`, `ArrowLeft`, `ArrowRight`,
///     `ArrowUp`, `ArrowDown`,
///     `F16`, `F17`, `F18`, `F19`
pub fn parse_combo(binding: &str) -> Option<ParsedCombo> {
    let mut modifiers = 0u8;
    let parts: Vec<&str> = binding.split('+').map(|s| s.trim()).collect();

    if parts.is_empty() {
        return None;
    }

    let key_part = parts.last()?;

    // Everything except the last part is a modifier
    for &mod_str in &parts[..parts.len() - 1] {
        match mod_str.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= 1,
            "shift"               => modifiers |= 2,
            "alt" | "option"      => modifiers |= 4,
            "cmd" | "command" | "super" | "meta" | "win" => modifiers |= 8,
            _ => return None, // unknown modifier
        }
    }

    let vk = key_to_vk(key_part)?;

    Some(ParsedCombo { vk, modifiers })
}

fn key_to_vk(key: &str) -> Option<u16> {
    let lower = key.to_lowercase();
    let s = lower.as_str();

    #[cfg(target_os = "macos")]
    {
        match s {
            // Letters (macOS Carbon VK codes)
            "a" => Some(0x00), "b" => Some(0x0B), "c" => Some(0x08),
            "d" => Some(0x02), "e" => Some(0x0E), "f" => Some(0x03),
            "g" => Some(0x05), "h" => Some(0x04), "i" => Some(0x22),
            "j" => Some(0x26), "k" => Some(0x28), "l" => Some(0x25),
            "m" => Some(0x2E), "n" => Some(0x2D), "o" => Some(0x1F),
            "p" => Some(0x23), "q" => Some(0x0C), "r" => Some(0x0F),
            "s" => Some(0x01), "t" => Some(0x11), "u" => Some(0x20),
            "v" => Some(0x09), "w" => Some(0x0D), "x" => Some(0x07),
            "y" => Some(0x10), "z" => Some(0x06),
            // Digits
            "0" => Some(0x1D), "1" => Some(0x12), "2" => Some(0x13),
            "3" => Some(0x14), "4" => Some(0x15), "5" => Some(0x17),
            "6" => Some(0x16), "7" => Some(0x1A), "8" => Some(0x1C),
            "9" => Some(0x19),
            // Function keys
            "f1" => Some(0x7A), "f2" => Some(0x78), "f3" => Some(0x63),
            "f4" => Some(0x76), "f5" => Some(0x60), "f6" => Some(0x61),
            "f7" => Some(0x62), "f8" => Some(0x64), "f9" => Some(0x65),
            "f10" => Some(0x6D), "f11" => Some(0x67), "f12" => Some(0x6F),
            "f13" => Some(0x69), "f14" => Some(0x6B), "f15" => Some(0x71),
            "f16" => Some(0x6A), "f17" => Some(0x40), "f18" => Some(0x4F),
            "f19" => Some(0x50),
            // Special keys
            "space" => Some(0x31),
            "enter" | "return" => Some(0x24),
            "escape" | "esc" => Some(0x35),
            "backspace" => Some(0x33),
            "tab" => Some(0x30),
            "delete" | "del" => Some(0x75),
            "home" => Some(0x73),
            "end" => Some(0x77),
            "pageup" => Some(0x74),
            "pagedown" => Some(0x79),
            "arrowleft" | "left" => Some(0x7B),
            "arrowright" | "right" => Some(0x7C),
            "arrowup" | "up" => Some(0x7E),
            "arrowdown" | "down" => Some(0x7D),
            _ => None,
        }
    }

    #[cfg(target_os = "windows")]
    {
        match s {
            // Letters (Windows VK codes = ASCII uppercase)
            "a" => Some(0x41), "b" => Some(0x42), "c" => Some(0x43),
            "d" => Some(0x44), "e" => Some(0x45), "f" => Some(0x46),
            "g" => Some(0x47), "h" => Some(0x48), "i" => Some(0x49),
            "j" => Some(0x4A), "k" => Some(0x4B), "l" => Some(0x4C),
            "m" => Some(0x4D), "n" => Some(0x4E), "o" => Some(0x4F),
            "p" => Some(0x50), "q" => Some(0x51), "r" => Some(0x52),
            "s" => Some(0x53), "t" => Some(0x54), "u" => Some(0x55),
            "v" => Some(0x56), "w" => Some(0x57), "x" => Some(0x58),
            "y" => Some(0x59), "z" => Some(0x5A),
            // Digits
            "0" => Some(0x30), "1" => Some(0x31), "2" => Some(0x32),
            "3" => Some(0x33), "4" => Some(0x34), "5" => Some(0x35),
            "6" => Some(0x36), "7" => Some(0x37), "8" => Some(0x38),
            "9" => Some(0x39),
            // Function keys
            "f1" => Some(0x70), "f2" => Some(0x71), "f3" => Some(0x72),
            "f4" => Some(0x73), "f5" => Some(0x74), "f6" => Some(0x75),
            "f7" => Some(0x76), "f8" => Some(0x77), "f9" => Some(0x78),
            "f10" => Some(0x79), "f11" => Some(0x7A), "f12" => Some(0x7B),
            "f13" => Some(0x7C), "f14" => Some(0x7D), "f15" => Some(0x7E),
            "f16" => Some(0x7F), "f17" => Some(0x80), "f18" => Some(0x81),
            "f19" => Some(0x82),
            // Special keys
            "space" => Some(0x20),
            "enter" | "return" => Some(0x0D),
            "escape" | "esc" => Some(0x1B),
            "backspace" => Some(0x08),
            "tab" => Some(0x09),
            "delete" | "del" => Some(0x2E),
            "home" => Some(0x24),
            "end" => Some(0x23),
            "pageup" => Some(0x21),
            "pagedown" => Some(0x22),
            "arrowleft" | "left" => Some(0x25),
            "arrowright" | "right" => Some(0x27),
            "arrowup" | "up" => Some(0x26),
            "arrowdown" | "down" => Some(0x28),
            _ => None,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    { let _ = s; None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_key_lowercase() {
        let c = parse_combo("a").unwrap();
        assert!(c.modifiers == 0, "no modifiers");
        // VK depends on platform; 'a' is 0x00 on macOS, 0x41 on Windows
    }

    #[test]
    fn ctrl_key() {
        let c = parse_combo("Ctrl+q").unwrap();
        assert_eq!(c.modifiers, 1);
    }

    #[test]
    fn shift_cmd_key() {
        let c = parse_combo("Shift+Cmd+N").unwrap();
        assert_eq!(c.modifiers, 2 | 8);
    }

    #[test]
    fn all_modifiers() {
        let c = parse_combo("Ctrl+Shift+Alt+Super+Space").unwrap();
        assert_eq!(c.modifiers, 1 | 2 | 4 | 8);
    }

    #[test]
    fn modifier_aliases() {
        assert_eq!(parse_combo("Command+x").unwrap().modifiers, 8);
        assert_eq!(parse_combo("Win+x").unwrap().modifiers, 8);
        assert_eq!(parse_combo("Meta+x").unwrap().modifiers, 8);
        assert_eq!(parse_combo("Option+x").unwrap().modifiers, 4);
        assert_eq!(parse_combo("Control+x").unwrap().modifiers, 1);
    }

    #[test]
    fn named_keys() {
        assert!(parse_combo("Space").is_some());
        assert!(parse_combo("Enter").is_some());
        assert!(parse_combo("Escape").is_some());
        assert!(parse_combo("Backspace").is_some());
        assert!(parse_combo("Tab").is_some());
        assert!(parse_combo("Delete").is_some());
        assert!(parse_combo("Home").is_some());
        assert!(parse_combo("End").is_some());
        assert!(parse_combo("PageUp").is_some());
        assert!(parse_combo("PageDown").is_some());
        assert!(parse_combo("ArrowLeft").is_some());
        assert!(parse_combo("ArrowRight").is_some());
        assert!(parse_combo("ArrowUp").is_some());
        assert!(parse_combo("ArrowDown").is_some());
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            parse_combo("ctrl+q").unwrap().modifiers,
            parse_combo("Ctrl+Q").unwrap().modifiers,
        );
    }

    #[test]
    fn whitespace_around_plus() {
        let c = parse_combo("Ctrl + Shift + N").unwrap();
        assert_eq!(c.modifiers, 1 | 2);
    }

    #[test]
    fn function_keys() {
        for i in 1..=19 {
            let s = format!("F{}", i);
            assert!(parse_combo(&s).is_some(), "F{} failed", i);
        }
    }

    #[test]
    fn digits() {
        for d in 0..=9 {
            let s = format!("{}", d);
            assert!(parse_combo(&s).is_some(), "digit {} failed", d);
        }
    }

    #[test]
    fn unknown_modifier_returns_none() {
        assert!(parse_combo("Bogus+q").is_none());
    }

    #[test]
    fn unknown_key_returns_none() {
        assert!(parse_combo("BogusKey").is_none());
    }

    #[test]
    fn empty_string() {
        assert!(parse_combo("").is_none());
    }

    #[test]
    fn only_modifiers_no_key() {
        // "Ctrl" alone has no key part
        assert!(parse_combo("Ctrl").is_none());
    }
}
