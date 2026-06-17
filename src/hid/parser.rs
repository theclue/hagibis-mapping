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
