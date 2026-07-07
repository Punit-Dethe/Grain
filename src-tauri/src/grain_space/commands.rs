//! [GRAIN] Grain Space Tauri commands. Thin async wrappers: gate on the master
//! toggle, resolve the base dir, run the blocking store work off the async
//! runtime, emit the changed event after mutations. No state is managed —
//! every call opens and drops its own resources (zero idle RAM).

use tauri::{AppHandle, Manager};

use super::store::{self, Note, ReminderState, ReminderStatus};
use super::{base_dir, emit_notes_changed, is_enabled};

fn gate(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    if !is_enabled(app) {
        return Err("Grain Space is disabled".to_string());
    }
    base_dir(app)
}

/// Run blocking store work off the async runtime (file + SQLite I/O).
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| format!("task join error: {e}"))?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
pub async fn grain_space_list_notes(app: AppHandle) -> Result<Vec<Note>, String> {
    let base = gate(&app)?;
    blocking(move || store::list_notes(&base)).await
}

#[tauri::command]
#[specta::specta]
pub async fn grain_space_search_notes(app: AppHandle, query: String) -> Result<Vec<Note>, String> {
    let base = gate(&app)?;
    blocking(move || store::search_notes(&base, &query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn grain_space_get_note(app: AppHandle, id: String) -> Result<Note, String> {
    let base = gate(&app)?;
    blocking(move || store::get_note(&base, &id)).await
}

/// Create or update. The frontend sends the full locked-schema note; for new
/// notes it uses `grain_space_create_note` instead so ids stay backend-minted.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_save_note(app: AppHandle, note: Note) -> Result<(), String> {
    let base = gate(&app)?;
    let result = blocking(move || store::save_note(&base, &note)).await;
    if result.is_ok() {
        emit_notes_changed(&app);
        // An edit may have added/changed/removed the reminder.
        super::reminders::sync(&app);
    }
    result
}

/// Mint a new raw note (blank title/tldr) and return it for editing.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_create_note(app: AppHandle, body: String) -> Result<Note, String> {
    let base = gate(&app)?;
    let result = blocking(move || {
        let note = Note::raw(body);
        store::save_note(&base, &note)?;
        Ok(note)
    })
    .await;
    if result.is_ok() {
        emit_notes_changed(&app);
    }
    result
}

#[tauri::command]
#[specta::specta]
pub async fn grain_space_delete_note(app: AppHandle, id: String) -> Result<(), String> {
    let base = gate(&app)?;
    let result = blocking(move || store::delete_note(&base, &id)).await;
    if result.is_ok() {
        emit_notes_changed(&app);
        // The deleted note may have held the next armed reminder.
        super::reminders::sync(&app);
    }
    result
}

#[tauri::command]
#[specta::specta]
pub async fn grain_space_set_pinned(
    app: AppHandle,
    id: String,
    pinned: bool,
) -> Result<Note, String> {
    let base = gate(&app)?;
    let result = blocking(move || store::set_pinned(&base, &id, pinned)).await;
    if result.is_ok() {
        emit_notes_changed(&app);
    }
    result
}

/// Arm (or re-arm) a note's reminder at `fire_at` (epoch ms).
#[tauri::command]
#[specta::specta]
pub async fn grain_space_arm_reminder(
    app: AppHandle,
    id: String,
    fire_at: i64,
) -> Result<Note, String> {
    let base = gate(&app)?;
    let result = blocking(move || {
        store::set_reminder(
            &base,
            &id,
            ReminderState {
                status: ReminderStatus::Armed,
                fire_at: Some(fire_at),
            },
        )
    })
    .await;
    if result.is_ok() {
        emit_notes_changed(&app);
        super::reminders::sync(&app);
    }
    result
}

/// Dismiss/complete a reminder (fired, armed, or pending).
#[tauri::command]
#[specta::specta]
pub async fn grain_space_dismiss_reminder(app: AppHandle, id: String) -> Result<Note, String> {
    let base = gate(&app)?;
    let result = blocking(move || {
        store::set_reminder(
            &base,
            &id,
            ReminderState {
                status: ReminderStatus::Dismissed,
                fire_at: None,
            },
        )
    })
    .await;
    if result.is_ok() {
        emit_notes_changed(&app);
        super::reminders::sync(&app);
    }
    result
}

/// Recovery: re-derive the whole index from the JSON files.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_rebuild_index(app: AppHandle) -> Result<u32, String> {
    let base = gate(&app)?;
    blocking(move || store::rebuild_index(&base)).await
}

// -- overlay window (Phase 3) ----------------------------------------------------

/// Open the overlay (or refocus it), optionally landing on a note. Used by the
/// settings tab's note rows; the global shortcut uses the toggle action.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_open_window(
    app: AppHandle,
    note_id: Option<String>,
) -> Result<(), String> {
    gate(&app)?;
    super::window::open(&app, note_id);
    Ok(())
}

/// Close (destroy) the overlay. Deliberately NOT gated: the window must be
/// closable even if the feature was just disabled underneath it.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_close_window(app: AppHandle) -> Result<(), String> {
    super::window::close(&app);
    Ok(())
}

/// One-shot: the note id the overlay should select on mount, if any.
#[tauri::command]
#[specta::specta]
pub fn grain_space_take_focus_note() -> Option<String> {
    super::window::take_focus_note()
}

// -- semantic search (Phase 4) ---------------------------------------------------

#[derive(serde::Serialize, specta::Type, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmbedModelStatus {
    Ready,
    Downloading,
    Absent,
}

#[tauri::command]
#[specta::specta]
pub fn grain_space_embed_model_status() -> EmbedModelStatus {
    if super::embed::is_downloading() {
        EmbedModelStatus::Downloading
    } else if super::embed::model_on_disk() {
        EmbedModelStatus::Ready
    } else {
        EmbedModelStatus::Absent
    }
}

/// Consent-gated model download (the frontend shows the consent dialog BEFORE
/// calling this). Progress/completion/error arrive as events; resolves when
/// the transfer ends either way.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_download_embed_model(app: AppHandle) -> Result<(), String> {
    gate(&app)?;
    super::embed::download_model(app).await
}

#[tauri::command]
#[specta::specta]
pub fn grain_space_cancel_embed_model_download() -> Result<(), String> {
    super::embed::cancel_download();
    Ok(())
}

/// Semantic (meaning-based) search. Spawns the engine lazily on first use —
/// allowed only while the overlay window exists, so the weights can never
/// outlive it. Re-embeds stale notes before serving so results stay truthful.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_semantic_search(
    app: AppHandle,
    query: String,
) -> Result<Vec<Note>, String> {
    let base = gate(&app)?;
    let settings = crate::settings::get_settings(&app);
    if !settings.grain_space_semantic {
        return Err("semantic search is disabled".to_string());
    }
    // Directive 7: the engine's lifetime is bound to the overlay window.
    if app
        .get_webview_window(super::window::WINDOW_LABEL)
        .is_none()
    {
        return Err("Grain Space window is not open".to_string());
    }
    if !super::embed::model_on_disk() {
        return Err("model-not-downloaded".to_string());
    }
    let half_life_days = settings.grain_space_decay_half_life_days;
    blocking(move || {
        let trimmed = query.trim().to_string();
        if trimmed.is_empty() {
            return store::list_notes(&base);
        }
        // Batch re-embed everything stale (covers engine spawn + edits made
        // while the engine was resident — save marks stale, search refreshes).
        let stale = store::stale_embed_texts(&base)?;
        if !stale.is_empty() {
            let texts: Vec<String> = stale.iter().map(|(_, t)| t.clone()).collect();
            let vectors = super::embed::embed(texts)?;
            let items: Vec<(String, Vec<f32>)> =
                stale.into_iter().map(|(id, _)| id).zip(vectors).collect();
            store::store_embeddings(&base, &items)?;
        }
        let query_vec = super::embed::embed(vec![trimmed])?
            .pop()
            .ok_or_else(|| anyhow::anyhow!("empty query embedding"))?;
        store::semantic_search(&base, &query_vec, half_life_days)
    })
    .await
}
