//! [GRAIN] Grain Space store: flat JSON notes + derived SQLite index.
//!
//! JSON files are the source of truth (schema is LOCKED — see `Note`); the
//! SQLite index only exists to make search cheap and is rebuildable from the
//! JSON at any time. All public functions take the app-wide `STORE_LOCK`, open
//! a fresh connection, and drop everything on return — zero idle RAM.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use specta::Type;

/// One application-wide lock serializing every read/write against the note
/// files AND the index DB (the concurrency directive: no WAL, single writer).
static STORE_LOCK: Mutex<()> = Mutex::new(());

/// Register sqlite-vec on every future connection (process-wide, once). The
/// vec0 virtual-table module becomes available; no vec table is created until
/// the user opts into semantic search (Phase 4).
fn ensure_vec_extension() {
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

// -- paths --------------------------------------------------------------------

fn notes_dir(base: &Path) -> PathBuf {
    base.join("notes")
}

fn db_path(base: &Path) -> PathBuf {
    base.join("index.sqlite")
}

fn note_path(base: &Path, id: &str) -> PathBuf {
    notes_dir(base).join(format!("{id}.json"))
}

/// Reject ids that could escape the notes dir (defense in depth — ids are
/// always uuids we minted, but they round-trip through the frontend).
fn validate_id(id: &str) -> Result<()> {
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

// -- index --------------------------------------------------------------------

/// Open (creating if needed) the index DB. TRUNCATE journal per the
/// concurrency directive — never WAL. Called with `STORE_LOCK` held.
fn open_index(base: &Path) -> Result<Connection> {
    ensure_vec_extension();
    fs::create_dir_all(notes_dir(base)).context("create grain_space/notes dir")?;
    let conn = Connection::open(db_path(base)).context("open grain_space index")?;
    conn.pragma_update(None, "journal_mode", "TRUNCATE")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notes_meta (
            id          TEXT PRIMARY KEY,
            timestamp   INTEGER NOT NULL,
            is_pinned   INTEGER NOT NULL DEFAULT 0,
            embed_stale INTEGER NOT NULL DEFAULT 1
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
            id UNINDEXED, title, tldr, body
        );",
    )?;
    Ok(conn)
}

/// Upsert one note into the index. Every content change marks the embedding
/// stale; Phase 4 re-embeds stale rows when the model is next resident.
fn index_note(conn: &Connection, note: &Note) -> Result<()> {
    conn.execute(
        "INSERT INTO notes_meta (id, timestamp, is_pinned, embed_stale)
         VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(id) DO UPDATE SET
            timestamp = excluded.timestamp,
            is_pinned = excluded.is_pinned,
            embed_stale = 1",
        params![note.id, note.timestamp, note.is_pinned as i64],
    )?;
    conn.execute("DELETE FROM notes_fts WHERE id = ?1", params![note.id])?;
    conn.execute(
        "INSERT INTO notes_fts (id, title, tldr, body) VALUES (?1, ?2, ?3, ?4)",
        params![note.id, note.title, note.tldr, note.body],
    )?;
    Ok(())
}

fn unindex_note(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM notes_meta WHERE id = ?1", params![id])?;
    conn.execute("DELETE FROM notes_fts WHERE id = ?1", params![id])?;
    Ok(())
}

// -- JSON I/O -----------------------------------------------------------------

fn read_note_file(path: &Path) -> Result<Note> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

/// Atomic write: temp file in the same directory, then rename over the target
/// so a crash can never leave a half-written note.
fn write_note_file(base: &Path, note: &Note) -> Result<()> {
    fs::create_dir_all(notes_dir(base)).context("create grain_space/notes dir")?;
    let final_path = note_path(base, &note.id);
    let tmp_path = final_path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(note)?;
    fs::write(&tmp_path, json).with_context(|| format!("write {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &final_path)
        .with_context(|| format!("rename into {}", final_path.display()))?;
    Ok(())
}

// -- public API (each fn: lock → open → work → drop) ---------------------------

/// Create or update a note: JSON first (source of truth), then the index.
pub fn save_note(base: &Path, note: &Note) -> Result<()> {
    validate_id(&note.id)?;
    let _guard = STORE_LOCK.lock().unwrap();
    write_note_file(base, note)?;
    let conn = open_index(base)?;
    index_note(&conn, note)?;
    Ok(())
}

pub fn get_note(base: &Path, id: &str) -> Result<Note> {
    validate_id(id)?;
    let _guard = STORE_LOCK.lock().unwrap();
    read_note_file(&note_path(base, id))
}

/// Delete the JSON file and its index rows. (The "never delete data" rule is
/// about the feature toggle — explicit per-note delete is a user action.)
pub fn delete_note(base: &Path, id: &str) -> Result<()> {
    validate_id(id)?;
    let _guard = STORE_LOCK.lock().unwrap();
    let path = note_path(base, id);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("delete {}", path.display()))?;
    }
    let conn = open_index(base)?;
    unindex_note(&conn, id)?;
    Ok(())
}

pub fn set_pinned(base: &Path, id: &str, pinned: bool) -> Result<Note> {
    validate_id(id)?;
    let _guard = STORE_LOCK.lock().unwrap();
    let mut note = read_note_file(&note_path(base, id))?;
    note.is_pinned = pinned;
    write_note_file(base, &note)?;
    let conn = open_index(base)?;
    // Pin flips don't change content — keep the embedding fresh, only the flag.
    conn.execute(
        "UPDATE notes_meta SET is_pinned = ?2 WHERE id = ?1",
        params![id, pinned as i64],
    )?;
    Ok(note)
}

/// Update only the reminder state. Like pin flips, this is not a content
/// change — the FTS row and embedding stay fresh (reminders aren't indexed).
pub fn set_reminder(base: &Path, id: &str, state: ReminderState) -> Result<Note> {
    validate_id(id)?;
    let _guard = STORE_LOCK.lock().unwrap();
    let mut note = read_note_file(&note_path(base, id))?;
    note.reminder_state = state;
    write_note_file(base, &note)?;
    Ok(note)
}

/// Every note, newest first. Reads the JSON files directly (source of truth);
/// unreadable files are skipped with a log line, never a crash.
pub fn list_notes(base: &Path) -> Result<Vec<Note>> {
    let _guard = STORE_LOCK.lock().unwrap();
    list_notes_unlocked(base)
}

fn list_notes_unlocked(base: &Path) -> Result<Vec<Note>> {
    let dir = notes_dir(base);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut notes = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match read_note_file(&path) {
            Ok(note) => notes.push(note),
            Err(e) => log::warn!("[GRAIN] skipping unreadable note {}: {e:#}", path.display()),
        }
    }
    notes.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(notes)
}

/// FTS5 prefix search (each whitespace term quoted + `*`), newest-relevant
/// first via bm25. Falls back to a plain substring scan if the FTS query is
/// unusable. Semantic search is a separate Phase-4 path.
pub fn search_notes(base: &Path, query: &str) -> Result<Vec<Note>> {
    let _guard = STORE_LOCK.lock().unwrap();
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return list_notes_unlocked(base);
    }

    let conn = open_index(base)?;
    // Quote every term (so FTS5 operators/punctuation can't break the query),
    // add `*` for search-as-you-type prefixes, drop tokenless punctuation-only
    // terms. Terms are implicitly ANDed.
    let fts_query = trimmed
        .split_whitespace()
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");

    let ids: Result<Vec<String>> = (|| {
        let mut stmt = conn.prepare(
            "SELECT id FROM notes_fts WHERE notes_fts MATCH ?1 ORDER BY bm25(notes_fts)",
        )?;
        let rows = stmt.query_map(params![fts_query], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })();

    match ids {
        Ok(ids) => {
            let mut out = Vec::with_capacity(ids.len());
            for id in ids {
                match read_note_file(&note_path(base, &id)) {
                    Ok(note) => out.push(note),
                    // Index ahead of disk (shouldn't happen) — skip, self-heals
                    // on the next rebuild.
                    Err(e) => log::warn!("[GRAIN] search hit {id} unreadable: {e:#}"),
                }
            }
            Ok(out)
        }
        Err(e) => {
            log::warn!("[GRAIN] FTS query failed ({e:#}); falling back to substring scan");
            let needle = trimmed.to_lowercase();
            Ok(list_notes_unlocked(base)?
                .into_iter()
                .filter(|n| {
                    n.title.to_lowercase().contains(&needle)
                        || n.tldr.to_lowercase().contains(&needle)
                        || n.body.to_lowercase().contains(&needle)
                })
                .collect())
        }
    }
}

/// Drop and re-derive the whole index from the JSON files. The recovery path
/// for any index corruption; embeddings all come back stale.
pub fn rebuild_index(base: &Path) -> Result<u32> {
    let _guard = STORE_LOCK.lock().unwrap();
    let notes = list_notes_unlocked(base)?;
    let conn = open_index(base)?;
    conn.execute_batch("DELETE FROM notes_meta; DELETE FROM notes_fts;")?;
    let mut count = 0u32;
    for note in &notes {
        index_note(&conn, note)?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("grain_space_test_{tag}_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let base = temp_base("roundtrip");
        let mut note = Note::raw("remember the wifi password is hunter2".into());
        note.title = "Wifi Password Note".into();
        save_note(&base, &note).unwrap();

        let loaded = get_note(&base, &note.id).unwrap();
        assert_eq!(loaded, note);

        let listed = list_notes(&base).unwrap();
        assert_eq!(listed.len(), 1);
        fs::remove_dir_all(&base).unwrap();
    }

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
    fn fts_search_finds_body_terms_and_survives_odd_queries() {
        let base = temp_base("search");
        let mut a = Note::raw("the wifi password is hunter2".into());
        a.title = "Home Network".into();
        let b = Note::raw("buy milk and eggs".into());
        save_note(&base, &a).unwrap();
        save_note(&base, &b).unwrap();

        let hits = search_notes(&base, "wifi pass").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, a.id);

        // Quotes/operators must not error or leak FTS syntax: the stray quote
        // and bare `(` are neutralized, `wifi` still prefix-matches.
        let odd = search_notes(&base, "wifi\" (").unwrap();
        assert_eq!(odd.len(), 1);
        assert_eq!(odd[0].id, a.id);

        // Empty query = full list, newest first.
        assert_eq!(search_notes(&base, "  ").unwrap().len(), 2);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn delete_removes_file_and_index() {
        let base = temp_base("delete");
        let note = Note::raw("temporary".into());
        save_note(&base, &note).unwrap();
        delete_note(&base, &note.id).unwrap();
        assert!(get_note(&base, &note.id).is_err());
        assert!(search_notes(&base, "temporary").unwrap().is_empty());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn rebuild_index_recovers_from_missing_db() {
        let base = temp_base("rebuild");
        let note = Note::raw("rebuild me".into());
        save_note(&base, &note).unwrap();
        // Simulate index corruption/loss.
        {
            let _guard = STORE_LOCK.lock().unwrap();
            fs::remove_file(db_path(&base)).unwrap();
        }
        let count = rebuild_index(&base).unwrap();
        assert_eq!(count, 1);
        assert_eq!(search_notes(&base, "rebuild").unwrap().len(), 1);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn invalid_ids_are_rejected() {
        let base = temp_base("ids");
        assert!(get_note(&base, "../../etc/passwd").is_err());
        assert!(get_note(&base, "").is_err());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn pin_toggle_persists_without_marking_embedding_stale() {
        let base = temp_base("pin");
        let note = Note::raw("pin me".into());
        save_note(&base, &note).unwrap();
        {
            let _guard = STORE_LOCK.lock().unwrap();
            let conn = open_index(&base).unwrap();
            conn.execute("UPDATE notes_meta SET embed_stale = 0", [])
                .unwrap();
        }
        let updated = set_pinned(&base, &note.id, true).unwrap();
        assert!(updated.is_pinned);
        assert!(get_note(&base, &note.id).unwrap().is_pinned);
        {
            let _guard = STORE_LOCK.lock().unwrap();
            let conn = open_index(&base).unwrap();
            let (pinned, stale): (i64, i64) = conn
                .query_row(
                    "SELECT is_pinned, embed_stale FROM notes_meta WHERE id = ?1",
                    params![note.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(pinned, 1);
            assert_eq!(stale, 0);
        }
        fs::remove_dir_all(&base).unwrap();
    }
}
