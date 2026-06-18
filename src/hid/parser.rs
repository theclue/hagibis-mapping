use super::keycode::{ConsumerBits, KeyCode};
use super::report::{ConsumerReport, KeyboardReport, MediaKeysReport, Report};

/// Parse a raw HID input report buffer into a typed [`Report`].
///
/// The buffer includes the report ID byte when present (Interface 2).
/// Interfaces 0 and 1 have no report ID.
pub fn parse(data: &[u8], report_id: u32, report_len: usize) -> Report {
    if data.is_empty() {
        return Report::Unknown;
    }

    match report_id {
        // ── Vendor data (0x3F) ───────────────────────────────────────────
        0x3F => Report::Vendor,

        // ── Interface 2 media keys (0x52) ─────────────────────────────────
        0x52 => {
            if report_len >= 2 {
                let b = data[1];
                Report::MediaKeys(MediaKeysReport {
                    play:         b & 0x01 != 0,
                    fast_forward: b & 0x02 != 0,
                    rewind:       b & 0x04 != 0,
                    next_track:   b & 0x08 != 0,
                    prev_track:   b & 0x10 != 0,
                })
            } else {
                Report::Unknown
            }
        }

        // ── Interface 2 composite keyboard (0x01) ─────────────────────────
        0x01 => {
            if data.len() >= 9 {
                let modifier = data[1];
                let keycodes: Vec<KeyCode> = data[3..9]
                    .iter()
                    .filter(|&&b| b != 0)
                    .map(|&b| KeyCode(b))
                    .collect();
                Report::Keyboard(KeyboardReport { modifier, keycodes })
            } else {
                Report::Unknown
            }
        }

        // ── No report ID — distinguish by byte length ─────────────────────
        0 => {
            if report_len >= 8 {
                // Interface 0 keyboard (8 bytes)
                let modifier = data[0];
                let keycodes: Vec<KeyCode> = data[2..8]
                    .iter()
                    .filter(|&&b| b != 0)
                    .map(|&b| KeyCode(b))
                    .collect();
                Report::Keyboard(KeyboardReport { modifier, keycodes })
            } else if report_len >= 4 {
                // Interface 1 / audio chip consumer (4 bytes)
                Report::Consumer(ConsumerReport {
                    bits: ConsumerBits(data[0]),
                })
            } else {
                Report::Unknown
            }
        }

        _ => Report::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer() {
        assert_eq!(parse(&[], 0, 0), Report::Unknown);
    }

    #[test]
    fn report_id_vendor() {
        assert_eq!(parse(&[0x3F], 0x3F, 1), Report::Vendor);
    }

    #[test]
    fn report_id_unknown() {
        assert_eq!(parse(&[0x99], 0x99, 1), Report::Unknown);
    }

    #[test]
    fn if0_keyboard_no_modifiers() {
        // Interface 0: 8 bytes, no report ID
        // Byte 0 = modifier, bytes 2-7 = keycodes
        let data = [0x00, 0x00, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00];
        if let Report::Keyboard(kb) = parse(&data, 0, 8) {
            assert_eq!(kb.modifier, 0);
            assert_eq!(kb.keycodes.len(), 1);
            assert_eq!(kb.keycodes[0].0, 0x0F);
        } else {
            panic!("expected Keyboard");
        }
    }

    #[test]
    fn if0_keyboard_with_modifiers() {
        let data = [0x09, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00];
        if let Report::Keyboard(kb) = parse(&data, 0, 8) {
            assert_eq!(kb.modifier, 0x09);
            assert_eq!(kb.keycodes[0].0, 0x14);
        } else {
            panic!("expected Keyboard");
        }
    }

    #[test]
    fn if0_keyboard_no_keys_pressed() {
        let data = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        if let Report::Keyboard(kb) = parse(&data, 0, 8) {
            assert_eq!(kb.modifier, 0);
            assert!(kb.keycodes.is_empty());
        } else {
            panic!("expected Keyboard");
        }
    }

    #[test]
    fn if1_consumer_vol_up() {
        let data = [0x01, 0x00, 0x00, 0x00];
        if let Report::Consumer(c) = parse(&data, 0, 4) {
            assert!(c.bits.has_vol_up());
            assert!(!c.bits.has_vol_down());
        } else {
            panic!("expected Consumer");
        }
    }

    #[test]
    fn if1_consumer_mute() {
        let data = [0x04, 0x00, 0x00, 0x00];
        if let Report::Consumer(c) = parse(&data, 0, 4) {
            assert!(c.bits.has_mute());
        } else {
            panic!("expected Consumer");
        }
    }

    #[test]
    fn if1_consumer_play_pause() {
        let data = [0x40, 0x00, 0x00, 0x00];
        if let Report::Consumer(c) = parse(&data, 0, 4) {
            assert!(c.bits.has_play_pause());
        } else {
            panic!("expected Consumer");
        }
    }

    #[test]
    fn if2_media_keys_play() {
        let data = [0x52, 0x01];
        if let Report::MediaKeys(mk) = parse(&data, 0x52, 2) {
            assert!(mk.play);
            assert!(!mk.fast_forward);
        } else {
            panic!("expected MediaKeys");
        }
    }

    #[test]
    fn if2_media_keys_next_prev() {
        let data = [0x52, 0x18];
        if let Report::MediaKeys(mk) = parse(&data, 0x52, 2) {
            assert!(mk.next_track);
            assert!(mk.prev_track);
            assert!(!mk.play);
        } else {
            panic!("expected MediaKeys");
        }
    }

    #[test]
    fn if2_keyboard_composite() {
        // Report ID 0x01, 10 bytes
        // Byte 1 = modifier, bytes 3-9 = keycodes
        let data = [0x01, 0x08, 0x00, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        if let Report::Keyboard(kb) = parse(&data, 0x01, 10) {
            assert_eq!(kb.modifier, 0x08);
            assert_eq!(kb.keycodes.len(), 1);
            assert_eq!(kb.keycodes[0].0, 0x46);
        } else {
            panic!("expected Keyboard in interface 2");
        }
    }
}
