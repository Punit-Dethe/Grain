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

// -- The note schema ----------------------------------------------------------
// `id, title, tldr, body, timestamp, todo_tags, reminder_state, is_pinned,
// question, entities, source` — exactly these fields, and NEVER an embedding.
//
// The first eight were the original locked set (FINAL-PLAN.md §3.2). The last
// three are the DISTILLED ARTIFACT (KNOWLEDGE-ARCHITECTURE-PLAN.md B1): the
// searchable question, the entities named in the note, and where the capture
// came from. They are persisted in frontmatter rather than only in the derived
// index, so a full reindex never has to re-run the LLM to get them back.
//
// Extending this is a deliberate contract change: update
// `json_schema_is_locked` below in the same commit, or the schema drifts
// silently. No migration is ever needed — absent fields deserialize empty, so
// every pre-existing note stays valid on read and simply gains the new fields
// the next time it is written.

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
    /// The distilled **searchable question** — one sentence someone would
    /// actually type or say when looking for this note ("why did token refresh
    /// fail on large payloads?"). Cerebras's measured accuracy win is embedding
    /// this rather than the raw body. Empty for raw captures and foreign notes.
    #[serde(default)]
    pub question: String,
    /// Entities named in the note (files, people, apps, projects, topics), as
    /// written. The keys of the entity graph; display names live here, the
    /// deduplicated norms live in the derived index.
    #[serde(default)]
    pub entities: Vec<String>,
    /// Where the capture came from: `dictation` | `selection` | `manual` |
    /// `import`, or "" when unknown (every note written before this field).
    /// Cerebras's `source` column: cheap, and the thing you actually want to
    /// filter on when a query means "that thing I copied", not "that thing I
    /// said".
    #[serde(default)]
    pub source: String,
}

/// Host-sanitized replacement fields for an Agent-authored full-note rewrite.
///
/// This is deliberately not part of the persisted/wire schema. The storage
/// layer applies it to the current note under the vault lock, preserving the
/// note's identity and user-owned state (pin, reminder, source, todo state).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NoteRewrite {
    pub title: String,
    pub tldr: String,
    pub body: String,
    pub question: String,
    pub entities: Vec<String>,
}

/// Listing-only sidebar card (TAURI-OVERLAY-PLAN.md Phase A). NOT the locked
/// `Note` schema and never persisted: light metadata derived at list time so a
/// browse ships no bodies to the webview.
#[derive(Serialize, Debug, Clone, PartialEq, Type)]
pub struct NoteCard {
    pub id: String,
    pub title: String,
    pub tldr: String,
    pub timestamp: i64,
    pub is_pinned: bool,
    pub reminder_state: ReminderState,
    /// The note's subfolder path INSIDE the Grain folder (the Grain home prefix
    /// is stripped, so the folder itself is never a collection), with `/`
    /// separators so the sidebar can render a nested tree. `None` = the note
    /// sits loose directly in the Grain folder (shown under "Notes").
    pub folder: Option<String>,
    /// Optional manual position inside `folder`. This is presentation metadata
    /// from the local derived index, never part of the Markdown note or its
    /// locked frontmatter schema. `None` keeps the normal newest-first order.
    pub manual_order: Option<i64>,
    /// True = authored OUTSIDE Grain (an Obsidian file inside the Grain folder
    /// with no `grain_id` yet). Still fully editable — Grain adopts it on first
    /// edit; the flag only groups it below the divider in the loose "Notes"
    /// list. (Legacy field name kept for the wire schema.)
    pub readonly: bool,
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
            question: String::new(),
            entities: Vec::new(),
            source: String::new(),
        }
    }
}

fn normalized_title(value: &str) -> String {
    value
        .trim()
        .trim_matches(['*', '_', '`'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn line_end(value: &str) -> usize {
    value.find('\n').unwrap_or(value.len())
}

fn next_line(value: &str, end: usize) -> &str {
    value
        .get(end..)
        .unwrap_or_default()
        .strip_prefix('\n')
        .unwrap_or_default()
}

fn content_after_line(value: &str, end: usize) -> &str {
    next_line(value, end).trim_start_matches(['\r', '\n'])
}

/// Return a real opening Markdown heading, when present. Agent tools use this
/// as the title fallback so a model that supplied only `body: "# Title …"`
/// still produces a separate title field without rendering the heading twice.
pub(crate) fn opening_markdown_heading(body: &str) -> Option<String> {
    let candidate = body.trim_start_matches(['\r', '\n']);
    let first_end = line_end(candidate);
    let first = candidate[..first_end].trim_end_matches('\r');
    let markdown_line = first.trim_start_matches(' ');
    let hashes = markdown_line.chars().take_while(|&ch| ch == '#').count();
    if (1..=6).contains(&hashes) {
        let remainder = &markdown_line[hashes..];
        if remainder.chars().next().is_some_and(char::is_whitespace) {
            let heading = remainder.trim().trim_matches(['*', '_', '`']).trim();
            if !heading.is_empty() {
                return Some(heading.to_string());
            }
        }
    }

    let rest = next_line(candidate, first_end);
    let second_end = line_end(rest);
    let underline = rest[..second_end].trim();
    let is_setext = underline.len() >= 3
        && (underline.chars().all(|ch| ch == '=') || underline.chars().all(|ch| ch == '-'));
    if is_setext && !first.trim().is_empty() {
        return Some(first.trim().trim_matches(['*', '_', '`']).trim().to_string());
    }

    None
}

/// Remove an opening Markdown heading when it merely repeats the title stored
/// in the note's dedicated title field. Agent-written notes pass through this
/// guard because model instructions are guidance, not an integrity boundary.
///
/// Deliberately narrow: only an exact, case-insensitive ATX heading (`# Title`)
/// or Setext heading (`Title` + `===`) is removed. Similar wording and ordinary
/// first paragraphs are preserved.
pub(crate) fn remove_duplicate_title_heading(title: &str, body: &str) -> String {
    let Some(heading) = opening_markdown_heading(body) else {
        return body.to_string();
    };
    let expected = normalized_title(title);
    if expected.is_empty() || normalized_title(&heading) != expected {
        return body.to_string();
    }

    let candidate = body.trim_start_matches(['\r', '\n']);
    let first_end = line_end(candidate);
    let first = candidate[..first_end].trim_start_matches(' ');
    if first.starts_with('#') {
        return content_after_line(candidate, first_end).to_string();
    }
    let rest = next_line(candidate, first_end);
    let second_end = line_end(rest);
    content_after_line(rest, second_end).to_string()
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
        // The declared field set, and nothing else — catches accidental drift.
        // Changing this list is a contract change; see the note above the struct.
        let note = Note::raw("body".into());
        let value = serde_json::to_value(&note).unwrap();
        let obj = value.as_object().unwrap();
        let mut keys: Vec<_> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "body",
                "entities",
                "id",
                "is_pinned",
                "question",
                "reminder_state",
                "source",
                "timestamp",
                "title",
                "tldr",
                "todo_tags"
            ]
        );
        // An embedding must NEVER reach the note file — vectors are derived and
        // rebuildable, and a note is something the user can read.
        assert!(!obj.contains_key("embedding"));
    }

    /// A note written before the distilled fields existed must still read
    /// cleanly, with the new fields empty rather than the parse failing.
    #[test]
    fn notes_written_before_the_distilled_fields_still_deserialize() {
        let old = r#"{"id":"a","title":"T","tldr":"S","body":"B","timestamp":1,
            "todo_tags":[],"reminder_state":{"status":"none","fire_at":null},
            "is_pinned":false}"#;
        let note: Note = serde_json::from_str(old).unwrap();
        assert_eq!(note.question, "");
        assert!(note.entities.is_empty());
        assert_eq!(note.source, "");
    }

    #[test]
    fn duplicate_title_heading_is_removed_without_touching_real_content() {
        assert_eq!(
            remove_duplicate_title_heading(
                "Authentication Issues",
                "# Authentication Issues\n\nToken refresh fails on large payloads."
            ),
            "Token refresh fails on large payloads."
        );
        assert_eq!(
            remove_duplicate_title_heading("Authentication Issues", "AUTHENTICATION ISSUES\n===\n\nDetails"),
            "Details"
        );
        assert_eq!(
            remove_duplicate_title_heading(
                "Authentication Issues",
                "# Authentication Follow-up\n\nAuthentication Issues remain open."
            ),
            "# Authentication Follow-up\n\nAuthentication Issues remain open."
        );
        assert_eq!(
            remove_duplicate_title_heading(
                "Authentication Issues",
                "Authentication Issues remain open."
            ),
            "Authentication Issues remain open."
        );
        assert_eq!(
            remove_duplicate_title_heading(
                "Authentication Issues",
                "Authentication Issues\n\n---\n\nDetails"
            ),
            "Authentication Issues\n\n---\n\nDetails"
        );
        assert_eq!(
            opening_markdown_heading("## A Longer Project Title\n\nDetails"),
            Some("A Longer Project Title".to_string())
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
