//! Grain settings ownership around Handy's shortcut implementation.
use std::{sync::Arc, time::Duration};
use tauri::{AppHandle, Manager};

pub(crate) const HOLD_THRESHOLD: Duration = Duration::from_millis(300);

#[tauri::command]
#[specta::specta]
pub fn change_shortcut_activation_setting(
    app: AppHandle,
    activation: grain_core::settings::ShortcutActivation,
) -> Result<(), String> {
    app.state::<Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.shortcut_activation = activation)
        .map_err(|error| error.to_string())
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn same_shadow(
    a: &grain_core::settings::ShortcutBinding,
    b: &grain_core::settings::ShortcutBinding,
) -> bool {
    a.id == b.id && a.current_binding == b.current_binding
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carbon_registration_identity_preserves_held_keys_and_replaces_changed_callbacks_or_chords() {
        let settings = grain_core::settings::get_default_settings();
        let original = settings.bindings["transcribe"].clone();
        let mut changed = original.clone();
        changed.name = "Renamed display label".into();
        assert!(same_shadow(&original, &changed));
        changed.id = "summon_agent".into();
        assert!(!same_shadow(&original, &changed));
        changed = original.clone();
        changed.current_binding = "ctrl+shift+f12".into();
        assert!(!same_shadow(&original, &changed));
    }

    #[test]
    fn activation_round_trips_in_the_owned_settings_schema() {
        use grain_core::settings::ShortcutActivation::*;
        for mode in [Toggle, PushToTalk, HoldOrToggle] {
            let mut settings = grain_core::settings::get_default_settings();
            settings.shortcut_activation = mode;
            let json = serde_json::to_value(&settings).unwrap();
            assert!(json.get("push_to_talk").is_none());
            let decoded: grain_core::settings::AppSettings = serde_json::from_value(json).unwrap();
            assert_eq!(decoded.shortcut_activation, mode);
        }
    }
}
