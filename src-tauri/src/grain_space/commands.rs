//! [GRAIN] Grain Space Tauri commands. Thin async wrappers: gate on the master
//! toggle, resolve the base dir, run the blocking store work off the async
//! runtime, emit the changed event after mutations. No state is managed —
//! every call opens and drops its own resources (zero idle RAM).

use tauri::AppHandle;

use super::store::{self, Note};
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

/// Recovery: re-derive the whole index from the JSON files.
#[tauri::command]
#[specta::specta]
pub async fn grain_space_rebuild_index(app: AppHandle) -> Result<u32, String> {
    let base = gate(&app)?;
    blocking(move || store::rebuild_index(&base)).await
}
