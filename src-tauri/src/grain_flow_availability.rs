//! [GRAIN] Daily-use availability policy for the model-specific Flow path.
//!
//! The pure artifact predicate lives in `grain-core`; this host seam adds the
//! only Tauri fact it cannot know — whether the selected file is installed —
//! and reconciles the one global shortcut when that answer changes.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::commands::ShortcutsInitialized;
use crate::managers::model::ModelManager;
use crate::settings::{get_settings, AppSettings};

fn selected_model_is_installed(app: &AppHandle, settings: &AppSettings) -> bool {
    app.try_state::<Arc<ModelManager>>()
        .and_then(|models| models.get_model_info(&settings.selected_model))
        .is_some_and(|model| model.is_downloaded)
}

pub(crate) fn is_available(app: &AppHandle) -> bool {
    let settings = get_settings(app);
    grain_core::capture::flow_is_eligible(&settings) && selected_model_is_installed(app, &settings)
}

/// Host-complete registration policy: pure settings gates plus the installed
/// model fact needed only by Flow.
pub(crate) fn shortcut_should_hold(
    app: &AppHandle,
    settings: &AppSettings,
    binding_id: &str,
) -> bool {
    grain_core::capture::shortcut_holds_hotkey(settings, binding_id)
        && (binding_id != "transcribe_realtime" || selected_model_is_installed(app, settings))
}

/// A stale registered key or queued event must be inert, including the AI key
/// when its persisted idle mode is Flow. This is deliberately silent: the UI
/// already explains how to make Flow available and an unavailable shortcut
/// should not create a recording-error toast.
pub(crate) fn binding_can_start(app: &AppHandle, binding_id: &str) -> bool {
    binding_id != "transcribe_realtime" || is_available(app)
}

/// Resolve the AI key against host availability as well as settings. A stale
/// selected artifact can disappear outside Grain between rescans; in that
/// narrow case AI dictation safely starts Standard instead of becoming dead.
pub(crate) fn action_id_for(app: &AppHandle, binding_id: &str) -> String {
    let settings = get_settings(app);
    let action_id = grain_core::capture::action_id_for(&settings, binding_id);
    if binding_id == "transcribe_send_to_ai"
        && action_id == "transcribe_realtime"
        && !is_available(app)
    {
        "transcribe".into()
    } else {
        action_id.to_string()
    }
}

/// Reconcile only when a settings/model operation changed availability. This
/// avoids duplicate registrations (which both backends correctly reject) and
/// does nothing before the normal shortcut initialization point.
pub(crate) fn reconcile_after_change(app: &AppHandle, was_available: bool) {
    if app.try_state::<ShortcutsInitialized>().is_none() {
        return;
    }
    let available = is_available(app);
    if available == was_available {
        return;
    }

    let settings = get_settings(app);
    let Some(binding) = settings.bindings.get("transcribe_realtime").cloned() else {
        return;
    };
    let result = if available {
        crate::shortcut::register_shortcut(app, binding)
    } else {
        crate::shortcut::unregister_shortcut(app, binding)
    };
    if let Err(error) = result {
        log::warn!(
            "[GRAIN] failed to {} Flow shortcut after availability changed: {error}",
            if available { "register" } else { "unregister" }
        );
    }
}
