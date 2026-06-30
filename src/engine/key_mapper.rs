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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{ButtonMappingSet, Config, DeviceConfig, LoggingConfig, Profile, TargetEvent};

    fn test_config() -> Config {
        Config {
            device: DeviceConfig { seize: vec![] },
            default: ButtonMappingSet {
                button_top_left: Some(TargetEvent::Keyboard {
                    binding: "Shift+Cmd+N".into(),
                    label: "New".into(),
                }),
                button_top_left_hold: Some(TargetEvent::Keyboard {
                    binding: "Cmd+T".into(),
                    label: "".into(),
                }),
                button_bottom_right: None,
                button_bottom_right_hold: None,
                play_pause: Some(TargetEvent::MediaKey {
                    key_type: 16,
                    label: "".into(),
                }),
                knob_cw: Some(TargetEvent::MediaKey {
                    key_type: 0,
                    label: "Vol+".into(),
                }),
                knob_ccw: None,
                knob_click: None,
            },
            profiles: vec![],
            logging: LoggingConfig { loglevel: "info".into() },
            allow_destructive: false,
        }
    }

    #[test]
    fn resolve_default_when_no_focus() {
        let cfg = test_config();
        let m = ConfigKeyMapper::resolve(&cfg, None);
        assert_eq!(m.button_top_left.as_ref().unwrap().label(), "New");
    }

    #[test]
    fn resolve_default_when_app_not_matched() {
        let cfg = test_config();
        let m = ConfigKeyMapper::resolve(&cfg, Some("com.unknown.App"));
        assert_eq!(m.button_top_left.as_ref().unwrap().label(), "New");
    }

    #[test]
    fn resolve_profile_overrides_default() {
        let mut cfg = test_config();
        cfg.profiles.push(Profile {
            app_id: "com.app.Term".into(),
            app_name: "Term".into(),
            mappings: ButtonMappingSet {
                button_top_left: Some(TargetEvent::Keyboard {
                    binding: "Cmd+N".into(),
                    label: "TermWin".into(),
                }),
                // Leave other fields empty — they inherit defaults
                button_top_left_hold: None,
                button_bottom_right: None,
                button_bottom_right_hold: None,
                play_pause: None,
                knob_cw: None,
                knob_ccw: None,
                knob_click: None,
            },
        });
        let m = ConfigKeyMapper::resolve(&cfg, Some("com.app.Term"));
        // Overridden
        assert_eq!(m.button_top_left.as_ref().unwrap().label(), "TermWin");
        assert_eq!(m.button_top_left.as_ref().unwrap().label(), "TermWin");
        let binding = match m.button_top_left.as_ref().unwrap() {
            TargetEvent::Keyboard { binding, .. } => binding.as_str(),
            _ => "",
        };
        assert_eq!(binding, "Cmd+N");
        // Inherited from default
        assert_eq!(m.button_top_left_hold.as_ref().unwrap().label(), "Cmd+T");
        assert_eq!(m.knob_cw.as_ref().unwrap().label(), "Vol+");
    }

    #[test]
    fn map_keycode_known() {
        let mapping = test_config().default;
        assert!(ConfigKeyMapper::map_keycode(KeyCode::BTN_TOP_LEFT, &mapping).is_some());
        assert!(ConfigKeyMapper::map_keycode(KeyCode::BTN_TOP_LEFT_HOLD, &mapping).is_some());
        assert!(ConfigKeyMapper::map_keycode(KeyCode::BTN_BOTTOM_RIGHT, &mapping).is_none());
        assert!(ConfigKeyMapper::map_keycode(KeyCode(0x99), &mapping).is_none());
    }
}
