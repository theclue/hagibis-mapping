use crate::backend::Injector;
use crate::config::{combo, TargetEvent};
use crate::error::Error;
use crate::log_debug;

/// Execute a [`TargetEvent`] via the given injector.
pub fn fire(event: &TargetEvent, injector: &dyn Injector) -> Result<(), Error> {
    log_debug!("inject", "{:?}", event);
    match event {
        TargetEvent::Keyboard { binding } => {
            let parsed = combo::parse_combo(binding)
                .ok_or_else(|| Error::Config(format!("invalid binding: {}", binding)))?;
            if parsed.modifiers != 0 {
                injector.inject_key_combo(parsed.vk, parsed.modifiers)?;
            } else {
                injector.inject_key(parsed.vk, true)?;
                injector.inject_key(parsed.vk, false)?;
            }
        }
        TargetEvent::MediaKey { key_type, .. } => {
            injector.inject_media_key(*key_type)?;
        }
        TargetEvent::SystemEvent { subtype, data, .. } => {
            injector.inject_system_event(*subtype, *data)?;
        }
        TargetEvent::MouseMove { dx, dy, .. } => {
            injector.inject_mouse_move(*dx, *dy)?;
        }
        TargetEvent::MouseClick { button, x, y, .. } => {
            injector.inject_mouse_click(*button, *x, *y)?;
        }
        TargetEvent::MouseScroll { dx, dy, .. } => {
            injector.inject_mouse_scroll(*dx, *dy)?;
        }
    }
    Ok(())
}
