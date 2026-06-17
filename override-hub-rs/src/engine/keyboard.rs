use std::collections::HashSet;

use crate::backend::Injector;
use crate::config::ButtonMappingSet;
use crate::engine::injector_util;
use crate::engine::key_mapper::ConfigKeyMapper;
use crate::error::Error;
use crate::hid::{KeyCode, KeyboardReport};
use crate::log_debug;
use crate::log_warn;

/// Tracks keyboard state and dispatches key-down / key-up events.
pub struct KeyboardHandler {
    pressed: HashSet<KeyCode>,
}

impl KeyboardHandler {
    pub fn new() -> Self { Self { pressed: HashSet::new() } }

    pub fn handle(
        &mut self,
        report: &KeyboardReport,
        mapping: &ButtonMappingSet,
        injector: &dyn Injector,
    ) -> Result<(), Error> {
        let new_keys: HashSet<KeyCode> = report.keycodes.iter().copied().collect();

        log_debug!(
            "keyboard",
            "mod=0x{:02X} prev={:?} now={:?}",
            report.modifier, self.pressed, new_keys
        );

        for kc in new_keys.difference(&self.pressed) {
            if let Some(event) = ConfigKeyMapper::map_keycode(*kc, mapping) {
                if let Err(e) = injector_util::fire(event, injector) {
                    log_warn!("keyboard", "fire error for {:?}: {}", kc, e);
                }
            }
        }

        self.pressed = new_keys;
        Ok(())
    }
}
