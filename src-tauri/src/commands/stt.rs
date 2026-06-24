//! [GRAIN] S6: backend management of the STT routing pool. Lets a user (or a
//! future UI) add/remove cloud providers, set keys, and toggle smart rotation
//! WITHOUT any front end — everything operates on grain-core's owned
//! `AppContext` settings + `grain.secrets.json`. Keys never leave the backend.

use std::sync::Arc;

use grain_core::{AppContext, SttProvider, SttProviderKind, STT_LOCAL_PROVIDER_ID};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

fn ctx(app: &AppHandle) -> Result<Arc<AppContext>, String> {
    app.try_state::<Arc<AppContext>>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| "AppContext unavailable".to_string())
}

/// A read-only view of the STT pool. API keys are NEVER returned — only the set
/// of provider ids that currently have a key stored.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct SttPoolView {
    pub smart_rotation: bool,
    pub providers: Vec<SttProvider>,
    pub providers_with_keys: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub fn stt_get_pool(app: AppHandle) -> Result<SttPoolView, String> {
    Ok(ctx(&app)?.with_settings(|s| SttPoolView {
        smart_rotation: s.stt_smart_rotation,
        providers: s.stt_providers.clone(),
        // Only ids with a NON-EMPTY key count as "has key" (mirrors pp_get_pool),
        // so the UI's key indicator can't show a false positive for a blank entry.
        providers_with_keys: s
            .stt_api_keys
            .0
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.clone())
            .collect(),
    }))
}

#[tauri::command]
#[specta::specta]
pub fn stt_set_smart_rotation(app: AppHandle, enabled: bool) -> Result<(), String> {
    ctx(&app)?
        .update_settings(|s| s.stt_smart_rotation = enabled)
        .map_err(|e| e.to_string())?;
    // [GRAIN] When rotation is ON, batch transcription routes to cloud providers,
    // so the local on-device model becomes dead weight — free it now instead of
    // letting it sit resident until the idle-unload timeout ("if it's not in use,
    // destroy it"). The rolling/real-time path is unaffected (always local,
    // on-demand). Turning rotation OFF needs nothing: the local model reloads
    // lazily on the next transcription.
    if enabled {
        if let Some(tm) =
            app.try_state::<std::sync::Arc<crate::managers::transcription::TranscriptionManager>>()
        {
            let _ = tm.unload_model();
        }
    }
    Ok(())
}

/// Add or update a cloud provider (matched by `id`), optionally setting its API
/// key (written to grain.secrets.json). The local provider is managed
/// automatically and cannot be created/edited here.
#[tauri::command]
#[specta::specta]
pub fn stt_upsert_provider(
    app: AppHandle,
    provider: SttProvider,
    api_key: Option<String>,
) -> Result<(), String> {
    if provider.kind == SttProviderKind::Local {
        return Err("the local provider is managed automatically".to_string());
    }
    if provider.id.is_empty() || provider.id == STT_LOCAL_PROVIDER_ID {
        return Err("invalid provider id".to_string());
    }
    ctx(&app)?
        .update_settings(move |s| {
            match s.stt_providers.iter_mut().find(|p| p.id == provider.id) {
                Some(existing) => *existing = provider.clone(),
                None => s.stt_providers.push(provider.clone()),
            }
            if let Some(key) = api_key {
                if key.is_empty() {
                    s.stt_api_keys.0.remove(&provider.id);
                } else {
                    s.stt_api_keys.0.insert(provider.id.clone(), key);
                }
            }
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn stt_remove_provider(app: AppHandle, id: String) -> Result<(), String> {
    if id == STT_LOCAL_PROVIDER_ID {
        return Err("cannot remove the local provider".to_string());
    }
    ctx(&app)?
        .update_settings(|s| {
            s.stt_providers.retain(|p| p.id != id);
            s.stt_api_keys.0.remove(&id);
        })
        .map_err(|e| e.to_string())
}
