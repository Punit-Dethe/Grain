//! [GRAIN] The shared note model (the LOCKED wire schema) + helpers used by
//! every store surface. Since the format unification (EXECUTION-PLAN.md P1)
//! there is exactly ONE store implementation — `vault.rs`, Markdown + YAML
//! frontmatter — and this module holds what both backends and the frontend
//! bindings share: the `Note` type, id validation, the sqlite-vec extension
//! hook, and JSON export (the portability bridge).

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Register sqlite-vec on every future connection (process-wide, once). The
/// vec0 virtual-table module becomes available; no vec table is created until
/// the user opts into semantic search.
pub(crate) fn ensure_vec_extension() {
    use std::sync::Once;
    static VEC_INIT: Once = Once::new();
    #[allow(clippy::missing_transmute_annotations)]
    VEC_INIT.call_once(|| unsafe {
        let rc = rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
        if rc != rusqlite::ffi::SQLITE_OK {
            log::error!("[GRAIN] failed to register sqlite-vec auto extension (rc={rc})");
        }
    });
}

// -- LOCKED note schema -------------------------------------------------------
// `id, title, tldr, body, timestamp, todo_tags, reminder_state, is_pinned` —
// exactly these fields, and never an embedding. Do not extend without updating
// FINAL-PLAN.md §3.2 first.

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct TodoTag {
    pub text: String,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReminderStatus {
    /// No reminder on this note.
    None,
    /// Extracted/suggested but not armed (auto-reminders off).
    Pending,
    /// Scheduled to fire at `fire_at`.
    Armed,
    /// Fired; kept for the settings-tab reminders list.
    Fired,
    /// User dismissed/completed it.
    Dismissed,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct ReminderState {
    pub status: ReminderStatus,
    /// Epoch ms; `None` unless `status` is `Armed`/`Fired`.
    pub fire_at: Option<i64>,
}

impl Default for ReminderState {
    fn default() -> Self {
        ReminderState {
            status: ReminderStatus::None,
            fire_at: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct Note {
    pub id: String,
    /// 3-word AI title, or "" for raw (no-LLM) captures.
    pub title: String,
    /// 1-sentence AI summary, or "" for raw captures.
    pub tldr: String,
    pub body: String,
    /// Epoch ms (UTC). Date grouping happens in the UI, in local time.
    pub timestamp: i64,
    #[serde(default)]
    pub todo_tags: Vec<TodoTag>,
    #[serde(default)]
    pub reminder_state: ReminderState,
    #[serde(default)]
    pub is_pinned: bool,
}

impl Note {
    /// A fresh raw note (Input B/C shape): blank title/tldr, stamped now.
    pub fn raw(body: String) -> Self {
        Note {
            id: uuid::Uuid::new_v4().to_string(),
            title: String::new(),
            tldr: String::new(),
            body,
            timestamp: chrono::Utc::now().timestamp_millis(),
            todo_tags: Vec::new(),
            reminder_state: ReminderState::default(),
            is_pinned: false,
        }
    }
}

/// Reject ids that could escape a notes directory (defense in depth — ids are
/// always uuids/hashes we minted, but they round-trip through the frontend).
pub(crate) fn validate_id(id: &str) -> Result<()> {
    if !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Ok(())
    } else {
        Err(anyhow!("invalid note id: {id:?}"))
    }
}

/// Serialize notes to a stable, human-readable JSON array for export
/// (RECALL-PLAN §8 — data portability). Since the unification the store format
/// is Markdown; this JSON dump of the locked schema stays as the
/// backend-agnostic backup/interchange format.
pub fn export_json(notes: &[Note]) -> Result<String> {
    Ok(serde_json::to_string_pretty(notes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_schema_is_locked() {
        // The locked field set, and nothing else — catches accidental schema drift.
        let note = Note::raw("body".into());
        let value = serde_json::to_value(&note).unwrap();
        let obj = value.as_object().unwrap();
        let mut keys: Vec<_> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "body",
                "id",
                "is_pinned",
                "reminder_state",
                "timestamp",
                "title",
                "tldr",
                "todo_tags"
            ]
        );
    }

    #[test]
    fn export_json_roundtrips_notes() {
        let mut a = Note::raw("first".into());
        a.title = "One".into();
        let b = Note::raw("second".into());
        let json = export_json(&[a.clone(), b.clone()]).unwrap();
        let back: Vec<Note> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0], a);
        assert_eq!(back[1], b);
        // Empty corpus is a valid (empty) export.
        assert_eq!(export_json(&[]).unwrap(), "[]");
    }

    #[test]
    fn invalid_ids_are_rejected() {
        assert!(validate_id("../../etc/passwd").is_err());
        assert!(validate_id("").is_err());
        assert!(validate_id("a/b").is_err());
        assert!(validate_id("8f2a1c9e-ok_id").is_ok());
    }
}
