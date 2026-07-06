//! [GRAIN] Grain Space — local, zero-idle-RAM notes.
//!
//! Design contract (see docs/Grain space files/FINAL-PLAN.md):
//! - Flat JSON files under `{app_data_dir}/grain_space/notes/` are the source
//!   of truth; `index.sqlite` (FTS5 + sqlite-vec) is a derived, rebuildable
//!   index. Embeddings NEVER live in the JSON files.
//! - No WAL: `journal_mode=TRUNCATE` + one application-wide `Mutex` serializes
//!   every store operation. Connections open per operation and drop — the
//!   feature holds zero resident memory while its surfaces are closed.
//! - `grain_space_enabled == false` ⇒ nothing initializes: shortcuts are
//!   skipped at registration (see `shortcut::tauri_impl` / `handy_keys`) and
//!   every command below early-returns. Disabling never deletes data files.

pub mod capture;
pub mod commands;
pub mod store;

use tauri::AppHandle;

/// Event emitted after any note mutation so open UI surfaces refresh.
pub const NOTES_CHANGED_EVENT: &str = "grain-space://notes-changed";

/// The feature's base directory: `{app_data_dir}/grain_space`. Nothing is
/// created by calling this — the store creates directories lazily on first write.
pub fn base_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    crate::portable::app_data_dir(app)
        .map(|d| d.join("grain_space"))
        .map_err(|e| format!("failed to resolve app data dir: {e}"))
}

/// Master gate. Every Grain Space entry point checks this first.
pub fn is_enabled(app: &AppHandle) -> bool {
    crate::settings::get_settings(app).grain_space_enabled
}

/// Notify open surfaces (settings tab / overlay) that notes changed.
pub fn emit_notes_changed(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit(NOTES_CHANGED_EVENT, ());
}
