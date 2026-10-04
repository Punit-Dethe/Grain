//! [GRAIN] Resolve a capture binding once, when the coordinator starts it.
//! Model/settings policy lives in grain-core; only artifact installation is
//! host state. The coordinator retains this action through stop/PTT release.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::managers::model::ModelManager;
use crate::settings::get_settings;

pub(crate) fn action_id_for(app: &AppHandle, binding_id: &str) -> String {
    let settings = get_settings(app);
    let action_id = grain_core::capture::action_id_for(&settings, binding_id);
    if action_id == "transcribe_realtime"
        && !app
            .try_state::<Arc<ModelManager>>()
            .and_then(|models| models.get_model_info(&settings.selected_model))
            .is_some_and(|model| model.is_downloaded)
    {
        // Let Standard report its normal missing-model error, without opening
        // a rolling worker against a stale or deleted artifact.
        "transcribe".into()
    } else {
        action_id.to_string()
    }
}
