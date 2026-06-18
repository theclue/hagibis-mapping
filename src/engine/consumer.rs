use crate::backend::Injector;
use crate::config::ButtonMappingSet;
use crate::engine::injector_util;
use crate::error::Error;
use crate::hid::ConsumerReport;
use crate::log_debug;
use crate::log_warn;

/// Tracks consumer report state and dispatches button events.
pub struct ConsumerHandler {
    last: u8,
}

impl ConsumerHandler {
    pub fn new() -> Self { Self { last: 0 } }

    pub fn handle(
        &mut self,
        report: &ConsumerReport,
        mapping: &ButtonMappingSet,
        injector: &dyn Injector,
    ) -> Result<(), Error> {
        let curr = report.bits.0;
        let prev = self.last;

        log_debug!(
            "consumer",
            "bits prev=0x{:02X} curr=0x{:02X}",
            prev, curr
        );

        // Vol+ (0x01) — absolute, held while turning
        if Self::edge(prev, curr, 0x01) {
            if let Some(event) = &mapping.knob_cw {
                if let Err(e) = injector_util::fire(event, injector) {
                    log_warn!("consumer", "knob_cw fire error: {}", e);
                }
            }
        }
        // Vol- (0x02) — absolute
        if Self::edge(prev, curr, 0x02) {
            if let Some(event) = &mapping.knob_ccw {
                if let Err(e) = injector_util::fire(event, injector) {
                    log_warn!("consumer", "knob_ccw fire error: {}", e);
                }
            }
        }
        // Mute (0x04) — tap on rising edge
        if Self::edge(prev, curr, 0x04) {
            if let Some(event) = &mapping.knob_click {
                if let Err(e) = injector_util::fire(event, injector) {
                    log_warn!("consumer", "knob_click fire error: {}", e);
                }
            }
        }
        // Play/Pause (0x40) — tap on press
        if Self::edge(prev, curr, 0x40) {
            if let Some(event) = &mapping.play_pause {
                if let Err(e) = injector_util::fire(event, injector) {
                    log_warn!("consumer", "play_pause fire error: {}", e);
                }
            }
        }

        self.last = curr;
        Ok(())
    }

    fn edge(prev: u8, curr: u8, mask: u8) -> bool {
        curr & mask != 0 && prev & mask == 0
    }
}
