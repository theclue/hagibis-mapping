//! Minimal terminal UI — renders HID report state in real time.
//!
//! Redraws on every poll cycle using ANSI escape codes.
//! Similar in spirit to the Python `monitor.py` PoC.

use crate::hid::{KeyboardReport, Report};

const CLEAR: &str = "\x1b[2J";
const HOME:  &str = "\x1b[H";
pub const HIDE:  &str = "\x1b[?25l";
pub const SHOW:  &str = "\x1b[?25h";
pub const GREEN: &str = "\x1b[1;32m";
const DIM:   &str = "\x1b[2;37m";
const CYAN:  &str = "\x1b[1;36m";
pub const RESET: &str = "\x1b[0m";
const ON:    &str = "●";
const OFF:   &str = "○";

const TOP_LEFT: &str = "┌";
const TOP_RIGHT: &str = "┐";
const BOT_LEFT: &str = "└";
const BOT_RIGHT: &str = "┘";
const VERT: &str = "│";
const T_LEFT: &str = "├";
const T_RIGHT: &str = "┤";

/// State the TUI needs to render.
pub struct TuiState {
    pub consumer: u8,
    pub keyboard: Option<KeyboardReport>,
    pub seized: bool,
    /// Focused app name (empty string = none / unknown).
    pub focused: String,
    /// Binding labels (updated each cycle from the active profile).
    pub btn_tl: String,
    pub btn_tl_hold: String,
    pub btn_br: String,
    pub btn_br_hold: String,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            consumer: 0, keyboard: None, seized: false,
            focused: String::new(),
            btn_tl: "?".into(), btn_tl_hold: "?".into(),
            btn_br: "?".into(), btn_br_hold: "?".into(),
        }
    }

    /// Update state from a parsed report.
    pub fn update(&mut self, report: &Report) {
        match report {
            Report::Consumer(c) => self.consumer = c.bits.0,
            Report::Keyboard(kb) => self.keyboard = Some(kb.clone()),
            _ => {}
        }
    }

    pub fn render(&self) -> String {
        let inner = 66; // total width minus 2 border chars

        let mut out = String::with_capacity(2048);
        out.push_str(CLEAR);
        out.push_str(HOME);

        out.push_str(&self.border(TOP_LEFT, TOP_RIGHT, &format!("{:^inner$}", " HAGIBIS HUB MAPPER ")));
        out.push('\n');
        let focus_display = if self.focused.is_empty() {
            "none".to_string()
        } else {
            self.focused.clone()
        };
        out.push_str(&self.border(VERT, VERT, &format!(" App: {}   Ctrl+C to exit", focus_display)));
        out.push('\n');

        let rule = "─".repeat(inner);
        out.push_str(&self.rule(T_LEFT, T_RIGHT, &rule));
        out.push('\n');

        // Knob & Media
        out.push_str(&self.border(VERT, VERT, "[Knob & Media]  Consumer reports"));
        out.push('\n');

        let vol_up   = self.bullet(self.consumer & 0x01 != 0);
        let vol_down = self.bullet(self.consumer & 0x02 != 0);
        let mute     = self.bullet(self.consumer & 0x04 != 0);
        out.push_str(&self.border(VERT, VERT, &format!("  Knob:  {} Vol+  {} Vol-  {} Mute", vol_up, vol_down, mute)));
        out.push('\n');

        let play = self.bullet(self.consumer & 0x40 != 0);
        out.push_str(&self.border(VERT, VERT, &format!("  Media: {} Play/Pause", play)));
        out.push('\n');

        out.push_str(&self.border(VERT, VERT, ""));
        out.push('\n');

        // Keyboard Buttons
        out.push_str(&self.border(VERT, VERT, "[Keyboard Buttons]  Physical state"));
        out.push('\n');

        let keys = self
            .keyboard
            .as_ref()
            .map(|kb| kb.keycodes.iter().map(|k| k.0).collect::<Vec<_>>())
            .unwrap_or_default();

        let has = |code: u8| keys.contains(&code);

        let row1 = format!(
            "  {} {:<15}  {} {}",
            self.bullet(has(0x14)),
            self.btn_tl,
            self.bullet(has(0x0F)),
            self.btn_tl_hold,
        );
        let row2 = format!(
            "  {} {:<15}  {} {}",
            self.bullet(has(0x20)),
            self.btn_br,
            self.bullet(has(0x46)),
            self.btn_br_hold,
        );
        out.push_str(&self.border(VERT, VERT, &row1));
        out.push('\n');
        out.push_str(&self.border(VERT, VERT, &row2));
        out.push('\n');

        // Modifier flags
        if let Some(ref kb) = self.keyboard {
            let mods = modifier_string(kb.modifier);
            out.push_str(&self.border(VERT, VERT, &format!("  Modifiers: {}", mods)));
            out.push('\n');
        }

        // Fill
        for _ in 0..5 {
            out.push_str(&self.border(VERT, VERT, ""));
            out.push('\n');
        }

        // Footer
        let status = if self.seized {
            format!("{GREEN}SEIZED{RESET} — remapping active")
        } else {
            "monitoring (passive)".to_string()
        };
        out.push_str(&self.rule(BOT_LEFT, BOT_RIGHT, &"─".repeat(inner)));
        out.push('\n');
        out.push_str(&format!("  {GREEN}●{RESET}=active  {DIM}○{RESET}=inactive  Status: {}\n", status));

        out
    }

    fn border(&self, left: &str, right: &str, content: &str) -> String {
        let pad_width = 64; // inner width minus left/right padding spaces
        let padded = inner_pad(content, pad_width);
        format!("{CYAN}{left}{RESET} {padded} {CYAN}{right}{RESET}")
    }

    fn rule(&self, left: &str, right: &str, fill: &str) -> String {
        format!("{CYAN}{left}{RESET}{fill}{CYAN}{right}{RESET}")
    }

    fn bullet(&self, active: bool) -> String {
        if active {
            format!("{GREEN}{ON}{RESET}")
        } else {
            format!("{DIM}{OFF}{RESET}")
        }
    }
}

fn inner_pad(s: &str, target_width: usize) -> String {
    let vis = visual_width(s);
    if vis >= target_width {
        // Truncate visually — take only as many chars as fit
        let mut result = String::new();
        let mut count = 0;
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                result.push(c);
                while let Some(&next) = chars.peek() {
                    let ch = chars.next().unwrap();
                    result.push(ch);
                    if next == 'm' { break; }
                }
            } else {
                if count >= target_width { break; }
                result.push(c);
                count += 1;
            }
        }
        result
    } else {
        format!("{}{:width$}", s, "", width = target_width - vis)
    }
}

/// Count printable characters, ignoring ANSI escape sequences (\x1b[...m).
fn visual_width(s: &str) -> usize {
    let mut count = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until the ANSI terminator (m)
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == 'm' {
                    break;
                }
            }
        } else {
            count += 1;
        }
    }
    count
}

fn modifier_string(mod_byte: u8) -> String {
    let mut parts = Vec::new();
    if mod_byte & 0x01 != 0 { parts.push("⌃"); } // Ctrl
    if mod_byte & 0x02 != 0 { parts.push("⇧"); } // Shift
    if mod_byte & 0x04 != 0 { parts.push("⌥"); } // Option/Alt
    if mod_byte & 0x08 != 0 { parts.push("⌘"); } // Cmd
    if mod_byte & 0x10 != 0 { parts.push("⌃R"); }
    if mod_byte & 0x20 != 0 { parts.push("⇧R"); }
    if mod_byte & 0x40 != 0 { parts.push("⌥R"); }
    if mod_byte & 0x80 != 0 { parts.push("⌘R"); }
    if parts.is_empty() { "—".into() } else { parts.join(" ") }
}
