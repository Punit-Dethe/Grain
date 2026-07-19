//! [GRAIN] Management of the post-process (LLM) routing pool — the counterpart to
//! `commands/stt.rs`. Lets a user (or a future UI) add/remove post-process
//! providers, set keys + models, and toggle smart rotation WITHOUT any front end:
//! everything operates on grain-core's owned `AppContext` settings +
//! `grain.secrets.json`. Keys never leave the backend.

use std::sync::Arc;

use grain_core::{AppContext, PostProcessProvider};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

fn ctx(app: &AppHandle) -> Result<Arc<AppContext>, String> {
    app.try_state::<Arc<AppContext>>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| "AppContext unavailable".to_string())
}

/// A read-only view of the post-process pool. API keys are NEVER returned — only
/// the set of provider ids that currently have a key stored, plus the per-provider
/// model map (model names are not secret).
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PpPoolView {
    pub smart_rotation: bool,
    pub providers: Vec<PostProcessProvider>,
    pub selected_provider_id: String,
    pub providers_with_keys: Vec<String>,
    pub models: std::collections::HashMap<String, String>,
}

#[tauri::command]
#[specta::specta]
pub fn pp_get_pool(app: AppHandle) -> Result<PpPoolView, String> {
    Ok(ctx(&app)?.with_settings(|s| PpPoolView {
        smart_rotation: s.post_process_smart_rotation,
        providers: s.post_process_providers.clone(),
        selected_provider_id: s.post_process_provider_id.clone(),
        providers_with_keys: s
            .post_process_api_keys
            .0
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.clone())
            .collect(),
        models: s.post_process_models.clone(),
    }))
}

#[tauri::command]
#[specta::specta]
pub fn pp_set_smart_rotation(app: AppHandle, enabled: bool) -> Result<(), String> {
    ctx(&app)?
        .update_settings(|s| s.post_process_smart_rotation = enabled)
        .map_err(|e| e.to_string())
}

/// Add or update a post-process provider (matched by `id`), optionally setting its
/// API key (→ grain.secrets.json) and model (→ `post_process_models`). Two entries
/// with the same `base_url` but different `id`s = two keys for one endpoint, which
/// is how multi-key rotation is expressed.
#[tauri::command]
#[specta::specta]
pub fn pp_upsert_provider(
    app: AppHandle,
    provider: PostProcessProvider,
    api_key: Option<String>,
    model: Option<String>,
) -> Result<(), String> {
    if provider.id.is_empty() {
        return Err("invalid provider id".to_string());
    }
    ctx(&app)?
        .update_settings(move |s| {
            match s
                .post_process_providers
                .iter_mut()
                .find(|p| p.id == provider.id)
            {
                Some(existing) => *existing = provider.clone(),
                None => s.post_process_providers.push(provider.clone()),
            }
            if let Some(key) = api_key {
                if key.is_empty() {
                    s.post_process_api_keys.0.remove(&provider.id);
                } else {
                    s.post_process_api_keys.0.insert(provider.id.clone(), key);
                }
            }
            if let Some(model) = model {
                s.post_process_models.insert(provider.id.clone(), model);
            }
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn pp_remove_provider(app: AppHandle, id: String) -> Result<(), String> {
    if id.is_empty() {
        return Err("invalid provider id".to_string());
    }
    ctx(&app)?
        .update_settings(|s| {
            s.post_process_providers.retain(|p| p.id != id);
            s.post_process_api_keys.0.remove(&id);
            s.post_process_models.remove(&id);
            // If the removed provider was the selected one, fall back to the first
            // remaining provider so the single-provider path stays valid.
            if s.post_process_provider_id == id {
                s.post_process_provider_id = s
                    .post_process_providers
                    .first()
                    .map(|p| p.id.clone())
                    .unwrap_or_default();
            }
        })
        .map_err(|e| e.to_string())
}
