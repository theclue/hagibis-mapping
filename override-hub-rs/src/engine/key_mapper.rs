use crate::config::{ButtonMappingSet, Config, TargetEvent};
use crate::hid::KeyCode;

/// Resolves a hub button to its target event for the focused app.
pub struct ConfigKeyMapper;

impl ConfigKeyMapper {
    /// Return the `ButtonMappingSet` that applies given a focused app ID.
    /// If a profile matches, its mappings are merged onto the default
    /// (missing fields inherit the default value).
    pub fn resolve(config: &Config, focused_app_id: Option<&str>) -> ButtonMappingSet {
        if let Some(app_id) = focused_app_id {
            for p in &config.profiles {
                if p.app_id == app_id {
                    return p.mappings.merge(&config.default);
                }
            }
        }
        config.default.clone()
    }

    /// Map a hub keyboard [`KeyCode`] to its target event.
    pub fn map_keycode<'a>(kc: KeyCode, mapping: &'a ButtonMappingSet) -> Option<&'a TargetEvent> {
        match kc {
            KeyCode::BTN_BOTTOM_RIGHT      => mapping.button_bottom_right.as_ref(),
            KeyCode::BTN_BOTTOM_RIGHT_HOLD => mapping.button_bottom_right_hold.as_ref(),
            KeyCode::BTN_TOP_LEFT          => mapping.button_top_left.as_ref(),
            KeyCode::BTN_TOP_LEFT_HOLD     => mapping.button_top_left_hold.as_ref(),
            _ => None,
        }
    }
}
