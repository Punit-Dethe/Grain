//! [GRAIN] Grain Space Tauri commands. Thin async wrappers: gate on the master
//! toggle, resolve the base dir, run the blocking store work off the async
//! runtime, emit the changed event after mutations. No state is managed —
//! every call opens and drops its own resources (zero idle RAM).

use tauri::{AppHandle, Manager};

use super::backend::{self, Backend};
use super::note::{self, Note, ReminderState, ReminderStatus};
use super::{emit_notes_changed, is_enabled};

/// Gate on the master toggle, then resolve which vault backs the feature
/// (the Grain-managed native folder or a user-chosen Obsidian vault).
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

/// Sidebar browse: light cards (no bodies) for the whole active store, with
/// each note's derived collection. The overlay's listing surface; full notes
/// load one at a time via `grain_space_get_note` on select.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_list_cards(app: AppHandle) -> Result<Vec<note::NoteCard>, String> {
    let be = gate(&app)?;
    blocking(move || backend::list_cards(&be)).await
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
        let json = note::export_json(&notes)?;
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

/// The existing Grain folders (collections that hold notes) — the candidate
/// categories the frontend shows when suggesting where a note belongs.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_list_folders(app: AppHandle) -> Result<Vec<String>, String> {
    let be = gate(&app)?;
    blocking(move || backend::list_folders(&be)).await
}

/// File a note into a Grain subfolder (or back to the Grain root when `folder`
/// is null/empty) — the accept action for an auto-categorization suggestion, or
/// a manual re-file. Returns the moved note.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_move_note(
    app: AppHandle,
    id: String,
    folder: Option<String>,
) -> Result<Note, String> {
    let be = gate(&app)?;
    let result = blocking(move || backend::move_note_to_folder(&be, &id, folder.as_deref())).await;
    if result.is_ok() {
        emit_notes_changed(&app);
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

/// Recovery: re-derive the whole index from the note files.
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

/// Open a note's file in Obsidian via its `obsidian://open?path=…` deep link
/// (vault backend only). Returns `true` when a link was opened, `false` for the
/// grain store (its notes have no external file). Opened backend-side so no
/// custom-scheme frontend capability is needed.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_open_in_obsidian(app: AppHandle, id: String) -> Result<bool, String> {
    use tauri_plugin_opener::OpenerExt;
    let be = gate(&app)?;
    let path = blocking(move || {
        Ok(backend::note_abs_path(&be, &id)?.map(|p| p.to_string_lossy().to_string()))
    })
    .await?;
    let Some(path) = path else {
        return Ok(false); // grain store — nothing external to open
    };
    // Percent-encode the path so spaces/#/? in filenames survive the deep link.
    let encoded: String = path
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    let uri = format!("obsidian://open?path={encoded}");
    app.opener()
        .open_url(uri, None::<String>)
        .map_err(|e| format!("open in Obsidian failed: {e}"))?;
    Ok(true)
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

/// Close (sleep) the overlay: the window hides and its renderer is suspended,
/// but it survives for an instant re-summon. Deliberately NOT gated: the
/// window must be closable even if the feature was just disabled underneath it.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_close_window(app: AppHandle) -> Result<(), String> {
    super::window::close(&app);
    Ok(())
}

/// Frontend ack (not gated): the workspace UI is mounted and painted — the
/// backend may reveal the window now.
#[tauri::command]
#[specta::specta]
pub fn grain_space_ui_ready(app: AppHandle) {
    super::window::ui_ready(&app);
}

/// Frontend ack (not gated): the React tree is unmounted (DOM purged) — the
/// backend may hide the window and suspend the webview now.
#[tauri::command]
#[specta::specta]
pub fn grain_space_sleep_ready(app: AppHandle) {
    super::window::sleep_ready(&app);
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

/// Semantic-mode search — actually HYBRID: exact FTS ∪ meaning-based KNN,
/// fused with Reciprocal Rank Fusion, so typing a literal title in semantic
/// mode can never miss it (either leg alone loses queries the other wins).
/// Spawns the engine lazily on first use — allowed only while the overlay
/// window exists, so the weights can never outlive it. Re-embeds stale notes
/// before serving so results stay truthful.
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
    // Directive 7: the engine's lifetime is bound to the overlay window — and
    // since hide-don't-destroy, to a VISIBLE overlay (a sleeping window must
    // never spawn the model).
    let overlay_visible = app
        .get_webview_window(super::window::WINDOW_LABEL)
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);
    if !overlay_visible {
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
        // Asymmetric query embedding (BGE retrieval instruction on the query
        // side only; stored note vectors stay bare).
        let query_vec = super::embed::embed_query(trimmed.clone())?;
        let semantic = backend::semantic_search(&be, &query_vec, half_life_days)?;
        // Lexical leg: the precise as-you-type AND matcher first; if the query
        // reads like a sentence and AND finds nothing, the stopword-filtered
        // OR leg still contributes exact-keyword hits.
        let mut fts = backend::search_notes(&be, &trimmed)?;
        if fts.is_empty() {
            fts = backend::search_notes_natural(&be, &trimmed, None)?;
        }
        Ok(super::recall::fuse_scored(fts, semantic, HYBRID_UI_RESULTS)
            .into_iter()
            .map(|(note, _)| note)
            .collect())
    })
    .await
}

/// Result cap for the overlay's hybrid search — a sidebar list, not a corpus dump.
const HYBRID_UI_RESULTS: usize = 40;
