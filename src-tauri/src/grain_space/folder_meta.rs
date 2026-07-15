//! [GRAIN] Grain-managed metadata for auto-categorization (AUTO-CATEGORIZATION-
//! PLAN.md). Two things must survive an index rebuild yet never clutter the
//! user's vault:
//!
//! 1. **Folder descriptions** — a one/two-sentence "what belongs here" per
//!    collection. This is the label schema the router classifies against;
//!    research (Label-Description Training, arXiv:2305.02239) shows a short
//!    description beats a bare folder name by a wide margin, and it's exactly
//!    the evidence a misfile ("news → Work") was missing.
//! 2. **Pending suggestions** — a medium-confidence route Grain proposes but
//!    will NOT auto-apply: `note id -> folder`, awaiting a one-click accept/
//!    dismiss in the overlay. High-confidence routes file immediately and never
//!    land here; low-confidence ones are dropped.
//!
//! Both live in ONE small JSON beside the index in app-data (NOT the vault, NOT
//! the rebuildable index db). Loaded and dropped per call — no resident state,
//! zero idle RAM. Guarded by its own mutex so concurrent captures don't race.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::vault::Vault;

/// One writer at a time — the file is tiny and rewritten whole.
static META_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default, Serialize, Deserialize)]
struct Meta {
    /// Folder path (relative to the Grain folder, `/`-joined — the same key
    /// `list_folders` yields) → its description. Absent/empty = no description.
    #[serde(default)]
    descriptions: BTreeMap<String, String>,
    /// Note id → the folder a medium-confidence route wants it in, pending the
    /// user's accept. Cleared on accept, dismiss, or delete.
    #[serde(default)]
    suggestions: BTreeMap<String, String>,
}

fn meta_path(v: &Vault) -> PathBuf {
    v.index_base.join("grain_space_meta.json")
}

fn load(v: &Vault) -> Meta {
    // A missing or corrupt file is not an error: the feature simply starts with
    // no descriptions/suggestions. Never fail a capture over this cache.
    match std::fs::read_to_string(meta_path(v)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Meta::default(),
    }
}

fn store(v: &Vault, meta: &Meta) -> Result<()> {
    let path = meta_path(v);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let text = serde_json::to_string_pretty(meta).context("serialize grain space meta")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("rename into {}", path.display()))?;
    Ok(())
}

// -- descriptions ---------------------------------------------------------------

/// The description for one folder, or `None` when unset/blank.
pub fn description_of(v: &Vault, folder: &str) -> Option<String> {
    load(v)
        .descriptions
        .get(folder)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Set (or clear, when `desc` is blank) a folder's description.
pub fn set_description(v: &Vault, folder: &str, desc: &str) -> Result<()> {
    let folder = folder.trim();
    if folder.is_empty() {
        return Ok(());
    }
    let _guard = META_LOCK.lock().unwrap();
    let mut meta = load(v);
    let desc = desc.trim();
    if desc.is_empty() {
        meta.descriptions.remove(folder);
    } else {
        // Cap so a runaway description can never bloat the router prompt.
        meta.descriptions
            .insert(folder.to_string(), desc.chars().take(400).collect());
    }
    store(v, &meta)
}

/// Every folder in `existing` paired with its description (empty when unset),
/// pruning descriptions for folders that no longer exist so the store stays
/// tidy. `existing` is the current `list_folders` result.
pub fn descriptions_for(v: &Vault, existing: &[String]) -> Result<Vec<(String, String)>> {
    let _guard = META_LOCK.lock().unwrap();
    let mut meta = load(v);
    let live: std::collections::BTreeSet<&str> = existing.iter().map(String::as_str).collect();
    let before = meta.descriptions.len();
    meta.descriptions.retain(|k, _| live.contains(k.as_str()));
    if meta.descriptions.len() != before {
        store(v, &meta)?;
    }
    Ok(existing
        .iter()
        .map(|f| {
            let d = meta.descriptions.get(f).cloned().unwrap_or_default();
            (f.clone(), d)
        })
        .collect())
}

// -- pending suggestions --------------------------------------------------------

/// Record a medium-confidence route awaiting the user's accept.
pub fn set_suggestion(v: &Vault, id: &str, folder: &str) -> Result<()> {
    let folder = folder.trim();
    if id.is_empty() || folder.is_empty() {
        return Ok(());
    }
    let _guard = META_LOCK.lock().unwrap();
    let mut meta = load(v);
    meta.suggestions
        .insert(id.to_string(), folder.to_string());
    store(v, &meta)
}

/// Drop a note's pending suggestion (on accept, dismiss, or delete). Returns the
/// folder that was suggested, if any.
pub fn clear_suggestion(v: &Vault, id: &str) -> Result<Option<String>> {
    let _guard = META_LOCK.lock().unwrap();
    let mut meta = load(v);
    let removed = meta.suggestions.remove(id);
    if removed.is_some() {
        store(v, &meta)?;
    }
    Ok(removed)
}

/// All pending `(note id, suggested folder)` routes.
pub fn suggestions(v: &Vault) -> Vec<(String, String)> {
    load(v)
        .suggestions
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_vault(tag: &str) -> Vault {
        let base = std::env::temp_dir().join(format!("grain-meta-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        Vault {
            root: base.join("notes"),
            folder: String::new(),
            index_base: base,
            native: true,
        }
    }

    fn cleanup(v: &Vault) {
        let _ = std::fs::remove_dir_all(&v.index_base);
    }

    #[test]
    fn descriptions_round_trip_and_prune() {
        let v = temp_vault("desc");
        set_description(&v, "Work", "My job at Acme: projects and meetings.").unwrap();
        set_description(&v, "Travel", "Trips and itineraries.").unwrap();
        assert_eq!(
            description_of(&v, "Work").as_deref(),
            Some("My job at Acme: projects and meetings.")
        );
        // Blank clears.
        set_description(&v, "Travel", "   ").unwrap();
        assert!(description_of(&v, "Travel").is_none());

        // descriptions_for pairs each live folder and prunes vanished ones.
        let pairs = descriptions_for(&v, &["Work".into(), "Groceries".into()]).unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].0, "Work"); // order follows `existing`
        let work = pairs.iter().find(|(f, _)| f == "Work").unwrap();
        assert!(work.1.contains("Acme"));
        cleanup(&v);
    }

    #[test]
    fn suggestions_set_clear_list() {
        let v = temp_vault("sugg");
        set_suggestion(&v, "id-1", "Work").unwrap();
        set_suggestion(&v, "id-2", "Travel").unwrap();
        let mut all = suggestions(&v);
        all.sort();
        assert_eq!(all, vec![
            ("id-1".to_string(), "Work".to_string()),
            ("id-2".to_string(), "Travel".to_string()),
        ]);
        assert_eq!(clear_suggestion(&v, "id-1").unwrap().as_deref(), Some("Work"));
        assert!(clear_suggestion(&v, "id-1").unwrap().is_none());
        assert_eq!(suggestions(&v).len(), 1);
        cleanup(&v);
    }

    #[test]
    fn missing_file_is_empty_not_error() {
        let v = temp_vault("empty");
        assert!(description_of(&v, "Work").is_none());
        assert!(suggestions(&v).is_empty());
        assert!(!Path::new(&meta_path(&v)).exists());
        cleanup(&v);
    }
}
