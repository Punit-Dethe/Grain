//! [GRAIN] Backend resolution (OBSIDIAN-PLAN.md §1 + EXECUTION-PLAN.md P1):
//! the ONE place that decides which folder of Markdown notes Grain Space
//! operates on. Since the format unification both backends run the SAME
//! store implementation (`vault.rs`):
//!
//! - `grain` (native, default): a Grain-managed vault under
//!   `{app_data}/grain_space/notes/` — legacy JSON notes are migrated into it
//!   on first resolve.
//! - `obsidian`: a user-chosen Obsidian vault.
//!
//! The dispatch functions below survive as the stable surface every caller
//! (commands, capture, recall, reminders) already uses.

use anyhow::Result;
use std::path::PathBuf;
use tauri::AppHandle;

use super::note::{Note, ReminderState};
use super::vault::{self, Vault};

/// The resolved backend — since the unification, always a vault.
pub type Backend = Vault;

/// Resolve the active backend from settings. Errors are user-facing strings
/// (the obsidian backend refuses to run against a missing/unset folder).
pub fn resolve(app: &AppHandle) -> std::result::Result<Backend, String> {
    let settings = crate::settings::get_settings(app);
    match settings.grain_space_backend {
        crate::settings::GrainSpaceBackend::Grain => {
            let v = Vault::native(super::base_dir(app)?);
            // One-time (per run, cheap after the first call): fold any legacy
            // pre-unification JSON notes into the native vault.
            vault::migrate_legacy_json_once(&v);
            Ok(v)
        }
        crate::settings::GrainSpaceBackend::Obsidian => {
            let path = settings.grain_space_vault_path.trim();
            if path.is_empty() {
                return Err(
                    "No Obsidian vault selected — choose one in Grain Space settings.".to_string(),
                );
            }
            let root = PathBuf::from(path);
            if !root.is_dir() {
                return Err(format!("Obsidian vault folder not found: {path}"));
            }
            Ok(Vault::obsidian(
                root,
                settings.grain_space_vault_folder.clone(),
                super::base_dir(app)?,
            ))
        }
    }
}

pub fn list_notes(b: &Backend) -> Result<Vec<Note>> {
    vault::list_notes(b)
}

pub fn search_notes(b: &Backend, query: &str) -> Result<Vec<Note>> {
    vault::search_notes(b, query)
}

pub fn search_notes_ranged(
    b: &Backend,
    query: &str,
    range: Option<(i64, i64)>,
) -> Result<Vec<Note>> {
    vault::search_notes_ranged(b, query, range)
}

pub fn get_note(b: &Backend, id: &str) -> Result<Note> {
    vault::get_note(b, id)
}

/// The on-disk file path of a note, for the "Open in Obsidian" deep link.
/// `None` for the native backend — its notes live in Grain's app data, which
/// is not a location to advertise externally.
pub fn note_abs_path(b: &Backend, id: &str) -> Result<Option<std::path::PathBuf>> {
    if b.native {
        return Ok(None);
    }
    vault::note_abs_path(b, id).map(Some)
}

/// Whether the corpus has ANY notes (grain-owned or foreign) — the recall
/// empty-corpus fast path. For an Obsidian vault this includes foreign notes,
/// so a vault with only the user's own notes is still recall-able.
pub fn has_any_notes(b: &Backend) -> Result<bool> {
    vault::has_any_notes(b)
}

pub fn save_note(b: &Backend, note: &Note) -> Result<()> {
    vault::save_note(b, note)
}

pub fn delete_note(b: &Backend, id: &str) -> Result<()> {
    vault::delete_note(b, id)
}

pub fn set_pinned(b: &Backend, id: &str, pinned: bool) -> Result<Note> {
    vault::set_pinned(b, id, pinned)
}

pub fn set_reminder(b: &Backend, id: &str, state: ReminderState) -> Result<Note> {
    vault::set_reminder(b, id, state)
}

pub fn rebuild_index(b: &Backend) -> Result<u32> {
    vault::rebuild_index(b)
}

pub fn stale_embed_texts(b: &Backend) -> Result<Vec<(String, String)>> {
    vault::stale_embed_texts(b)
}

pub fn store_embeddings(b: &Backend, items: &[(String, Vec<f32>)]) -> Result<()> {
    vault::store_embeddings(b, items)
}

pub fn semantic_search(
    b: &Backend,
    query_embedding: &[f32],
    half_life_days: u32,
) -> Result<Vec<Note>> {
    vault::semantic_search(b, query_embedding, half_life_days)
}

pub fn semantic_search_ranged(
    b: &Backend,
    query_embedding: &[f32],
    half_life_days: u32,
    range: Option<(i64, i64)>,
    min_similarity: f64,
) -> Result<Vec<Note>> {
    vault::semantic_search_ranged(b, query_embedding, half_life_days, range, min_similarity)
}
