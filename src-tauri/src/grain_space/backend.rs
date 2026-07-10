//! [GRAIN] Backend dispatch (OBSIDIAN-PLAN.md §1): the ONE place that decides
//! whether Grain Space operations hit the flat-JSON grain store or an Obsidian
//! vault. Every caller (commands, capture, recall, reminders) resolves a
//! [`Backend`] from settings and calls the same function surface as before —
//! zero behavior change with the default `grain` backend.

use std::path::PathBuf;

use anyhow::Result;
use tauri::AppHandle;

use super::store::{self, Note, ReminderState};
use super::vault::{self, Vault};

#[derive(Clone)]
pub enum Backend {
    Grain(PathBuf),
    Vault(Vault),
}

/// Resolve the active backend from settings. Errors are user-facing strings
/// (the vault backend refuses to run against a missing/unset folder).
pub fn resolve(app: &AppHandle) -> std::result::Result<Backend, String> {
    let settings = crate::settings::get_settings(app);
    match settings.grain_space_backend {
        crate::settings::GrainSpaceBackend::Grain => Ok(Backend::Grain(super::base_dir(app)?)),
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
            Ok(Backend::Vault(Vault {
                root,
                folder: settings.grain_space_vault_folder.clone(),
                index_base: super::base_dir(app)?,
            }))
        }
    }
}

pub fn list_notes(b: &Backend) -> Result<Vec<Note>> {
    match b {
        Backend::Grain(base) => store::list_notes(base),
        Backend::Vault(v) => vault::list_notes(v),
    }
}

pub fn search_notes(b: &Backend, query: &str) -> Result<Vec<Note>> {
    match b {
        Backend::Grain(base) => store::search_notes(base, query),
        Backend::Vault(v) => vault::search_notes(v, query),
    }
}

pub fn search_notes_ranged(
    b: &Backend,
    query: &str,
    range: Option<(i64, i64)>,
) -> Result<Vec<Note>> {
    match b {
        Backend::Grain(base) => store::search_notes_ranged(base, query, range),
        Backend::Vault(v) => vault::search_notes_ranged(v, query, range),
    }
}

pub fn get_note(b: &Backend, id: &str) -> Result<Note> {
    match b {
        Backend::Grain(base) => store::get_note(base, id),
        Backend::Vault(v) => vault::get_note(v, id),
    }
}

pub fn save_note(b: &Backend, note: &Note) -> Result<()> {
    match b {
        Backend::Grain(base) => store::save_note(base, note),
        Backend::Vault(v) => vault::save_note(v, note),
    }
}

pub fn delete_note(b: &Backend, id: &str) -> Result<()> {
    match b {
        Backend::Grain(base) => store::delete_note(base, id),
        Backend::Vault(v) => vault::delete_note(v, id),
    }
}

pub fn set_pinned(b: &Backend, id: &str, pinned: bool) -> Result<Note> {
    match b {
        Backend::Grain(base) => store::set_pinned(base, id, pinned),
        Backend::Vault(v) => vault::set_pinned(v, id, pinned),
    }
}

pub fn set_reminder(b: &Backend, id: &str, state: ReminderState) -> Result<Note> {
    match b {
        Backend::Grain(base) => store::set_reminder(base, id, state),
        Backend::Vault(v) => vault::set_reminder(v, id, state),
    }
}

pub fn rebuild_index(b: &Backend) -> Result<u32> {
    match b {
        Backend::Grain(base) => store::rebuild_index(base),
        Backend::Vault(v) => vault::rebuild_index(v),
    }
}

pub fn stale_embed_texts(b: &Backend) -> Result<Vec<(String, String)>> {
    match b {
        Backend::Grain(base) => store::stale_embed_texts(base),
        Backend::Vault(v) => vault::stale_embed_texts(v),
    }
}

pub fn store_embeddings(b: &Backend, items: &[(String, Vec<f32>)]) -> Result<()> {
    match b {
        Backend::Grain(base) => store::store_embeddings(base, items),
        Backend::Vault(v) => vault::store_embeddings(v, items),
    }
}

pub fn semantic_search(
    b: &Backend,
    query_embedding: &[f32],
    half_life_days: u32,
) -> Result<Vec<Note>> {
    match b {
        Backend::Grain(base) => store::semantic_search(base, query_embedding, half_life_days),
        Backend::Vault(v) => vault::semantic_search(v, query_embedding, half_life_days),
    }
}

pub fn semantic_search_ranged(
    b: &Backend,
    query_embedding: &[f32],
    half_life_days: u32,
    range: Option<(i64, i64)>,
    min_similarity: f64,
) -> Result<Vec<Note>> {
    match b {
        Backend::Grain(base) => store::semantic_search_ranged(
            base,
            query_embedding,
            half_life_days,
            range,
            min_similarity,
        ),
        Backend::Vault(v) => {
            vault::semantic_search_ranged(v, query_embedding, half_life_days, range, min_similarity)
        }
    }
}
