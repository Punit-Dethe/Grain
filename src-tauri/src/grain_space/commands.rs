//! [GRAIN] Grain Space Tauri commands. Thin async wrappers: gate on the master
//! toggle, resolve the base dir, run the blocking store work off the async
//! runtime, emit the changed event after mutations. No state is managed —
//! every call opens and drops its own resources (zero idle RAM).

use tauri::{AppHandle, Manager};

use super::backend::{self, Backend};
use super::store::{self, Note, ReminderState, ReminderStatus};
use super::{emit_notes_changed, is_enabled};

/// Gate on the master toggle, then resolve which store backs the feature
/// (grain JSON files or an Obsidian vault — OBSIDIAN-PLAN.md §1).
fn gate(app: &AppHandle) -> Result<Backend, String> {
    if !is_enabled(app) {
        return Err("Grain Space is disabled".to_string());
    }
    backend::resolve(app)
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
    let be = gate(&app)?;
    blocking(move || backend::list_notes(&be)).await
}

#[tauri::command]
#[specta::specta]
pub async fn grain_space_search_notes(app: AppHandle, query: String) -> Result<Vec<Note>, String> {
    let be = gate(&app)?;
    blocking(move || backend::search_notes(&be, &query)).await
}

/// Export every note as one pretty JSON array to a user-chosen file
/// (RECALL-PLAN §8 — data portability/backup). Returns the written path, or
/// `None` if the user cancelled the save dialog. Serializes BEFORE prompting so
/// an empty corpus (or a read failure) never opens a pointless dialog.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_export_notes(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let be = gate(&app)?;
    let (count, json) = blocking(move || {
        let notes = backend::list_notes(&be)?;
        let json = store::export_json(&notes)?;
        Ok((notes.len(), json))
    })
    .await?;
    if count == 0 {
        return Err("No notes to export yet.".to_string());
    }

    let default_name = format!(
        "grain-space-notes-{}.json",
        chrono::Local::now().format("%Y%m%d")
    );
    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app2.dialog()
            .file()
            .set_file_name(&default_name)
            .add_filter("JSON", &["json"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;

    let Some(path) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(None); // user cancelled
    };
    std::fs::write(&path, json).map_err(|e| format!("write export: {e}"))?;
    log::info!(
        "[GRAIN] space: exported {count} note(s) to {}",
        path.display()
    );
    Ok(Some(path.to_string_lossy().to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn grain_space_get_note(app: AppHandle, id: String) -> Result<Note, String> {
    let be = gate(&app)?;
    blocking(move || backend::get_note(&be, &id)).await
}

/// Create or update. The frontend sends the full locked-schema note; for new
/// notes it uses `grain_space_create_note` instead so ids stay backend-minted.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_save_note(app: AppHandle, note: Note) -> Result<(), String> {
    let be = gate(&app)?;
    let result = blocking(move || backend::save_note(&be, &note)).await;
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
    let be = gate(&app)?;
    let result = blocking(move || {
        let note = Note::raw(body);
        backend::save_note(&be, &note)?;
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
    let be = gate(&app)?;
    let result = blocking(move || backend::delete_note(&be, &id)).await;
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
    let be = gate(&app)?;
    let result = blocking(move || backend::set_pinned(&be, &id, pinned)).await;
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
    let be = gate(&app)?;
    let result = blocking(move || {
        backend::set_reminder(
            &be,
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
    let be = gate(&app)?;
    let result = blocking(move || {
        backend::set_reminder(
            &be,
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
    let be = gate(&app)?;
    blocking(move || backend::rebuild_index(&be)).await
}

/// Native folder picker for the Obsidian vault (OBSIDIAN-PLAN.md). Runs the
/// dialog backend-side (same pattern as export) so no new webview capability is
/// needed. On pick, persists the path via the validated setting command and
/// returns it; `None` = user cancelled.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_pick_vault(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    if !is_enabled(&app) {
        return Err("Grain Space is disabled".to_string());
    }
    let app2 = app.clone();
    let picked =
        tauri::async_runtime::spawn_blocking(move || app2.dialog().file().blocking_pick_folder())
            .await
            .map_err(|e| e.to_string())?;
    let Some(folder) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(None); // user cancelled
    };
    let path = folder.to_string_lossy().to_string();
    crate::shortcut::change_grain_space_vault_path_setting(app, path.clone())?;
    Ok(Some(path))
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

/// Uninstall the semantic model from the HF cache (R4 — reclaim ~130 MB). Drops
/// the engine and deletes the files. Refuses mid-download. The frontend turns
/// the semantic setting off afterward so nothing tries to load a missing model.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_uninstall_embed_model(app: AppHandle) -> Result<(), String> {
    if !is_enabled(&app) {
        return Err("Grain Space is disabled".to_string());
    }
    if super::embed::is_downloading() {
        return Err("A model download is in progress.".to_string());
    }
    tauri::async_runtime::spawn_blocking(super::embed::uninstall_model)
        .await
        .map_err(|e| format!("task join error: {e}"))?
        .map_err(|e| format!("{e:#}"))
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
    let be = gate(&app)?;
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
            return backend::list_notes(&be);
        }
        // Batch re-embed everything stale (covers engine spawn + edits made
        // while the engine was resident — save marks stale, search refreshes).
        let stale = backend::stale_embed_texts(&be)?;
        if !stale.is_empty() {
            let texts: Vec<String> = stale.iter().map(|(_, t)| t.clone()).collect();
            let vectors = super::embed::embed(texts)?;
            let items: Vec<(String, Vec<f32>)> =
                stale.into_iter().map(|(id, _)| id).zip(vectors).collect();
            backend::store_embeddings(&be, &items)?;
        }
        let query_vec = super::embed::embed(vec![trimmed])?
            .pop()
            .ok_or_else(|| anyhow::anyhow!("empty query embedding"))?;
        backend::semantic_search(&be, &query_vec, half_life_days)
    })
    .await
}
