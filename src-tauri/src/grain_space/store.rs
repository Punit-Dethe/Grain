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

/// Where genuinely-corrupt note files are moved so they stop polluting every
/// scan/rebuild while staying recoverable (RECALL-PLAN §8). A subdir of
/// `notes/`; it has no `.json` extension itself, so the (non-recursive) scan
/// never re-reads what's inside it.
fn corrupt_dir(base: &Path) -> PathBuf {
    notes_dir(base).join("corrupt")
}

/// Move a file that failed to PARSE (not merely a transient read) into
/// `notes/corrupt/`. Never clobbers a prior quarantine of the same id.
fn quarantine_corrupt(base: &Path, path: &Path) -> Result<()> {
    let dir = corrupt_dir(base);
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| anyhow!("corrupt note has no file name: {}", path.display()))?;
    let mut dest = dir.join(name);
    if dest.exists() {
        let ts = chrono::Utc::now().timestamp_millis();
        dest = dir.join(format!("{}.{ts}", name.to_string_lossy()));
    }
    fs::rename(path, &dest).with_context(|| format!("quarantine {}", path.display()))?;
    Ok(())
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
    if vec_table_exists(conn) {
        conn.execute("DELETE FROM notes_vec WHERE id = ?1", params![id])?;
    }
    Ok(())
}

// -- vector index (Phase 4, created only once semantic search is used) ---------

/// The vec0 table is NOT part of `open_index` on purpose: it only comes into
/// existence on the first semantic operation, so non-semantic users never
/// carry it.
fn vec_table_exists(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'notes_vec'",
        [],
        |_| Ok(()),
    )
    .is_ok()
}

fn ensure_vec_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS notes_vec USING vec0(
            id TEXT PRIMARY KEY,
            embedding float[384]
        );",
    )?;
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

/// Serialize notes to a stable, human-readable JSON array for export
/// (RECALL-PLAN §8 — data portability). Notes are already the source of truth,
/// so this is a straight pretty dump of the locked schema.
pub fn export_json(notes: &[Note]) -> Result<String> {
    Ok(serde_json::to_string_pretty(notes)?)
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
        // Classify read vs parse: a transient I/O error (lock, permissions) is
        // skipped and retried next scan; genuinely corrupt JSON is quarantined
        // so it stops breaking every scan/rebuild.
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Note>(&bytes) {
                Ok(note) => notes.push(note),
                Err(e) => {
                    log::warn!("[GRAIN] quarantining corrupt note {}: {e}", path.display());
                    if let Err(qe) = quarantine_corrupt(base, &path) {
                        log::error!("[GRAIN] quarantine failed for {}: {qe:#}", path.display());
                    }
                }
            },
            Err(e) => log::warn!("[GRAIN] skipping unreadable note {}: {e}", path.display()),
        }
    }
    notes.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(notes)
}

/// FTS5 prefix search (each whitespace term quoted + `*`), newest-relevant
/// first via bm25. Falls back to a plain substring scan if the FTS query is
/// unusable. Semantic search is a separate Phase-4 path.
pub fn search_notes(base: &Path, query: &str) -> Result<Vec<Note>> {
    search_notes_ranged(base, query, None)
}

/// [GRAIN] FTS search with an optional inclusive `(min_ms, max_ms)` timestamp
/// pre-filter (Grain Recall's `search_memory` date scoping). The filter is a SQL
/// `timestamp BETWEEN` on `notes_meta` — it works with FTS alone (no embed
/// model), so date-scoped recall degrades gracefully. An empty query with a
/// range returns every note in that window (e.g. "what did I save in June?").
pub fn search_notes_ranged(
    base: &Path,
    query: &str,
    range: Option<(i64, i64)>,
) -> Result<Vec<Note>> {
    let _guard = STORE_LOCK.lock().unwrap();
    let trimmed = query.trim();
    if trimmed.is_empty() {
        let mut notes = list_notes_unlocked(base)?;
        if let Some((lo, hi)) = range {
            notes.retain(|n| n.timestamp >= lo && n.timestamp <= hi);
        }
        return Ok(notes);
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
        // The date pre-filter joins notes_meta and constrains its timestamp;
        // without a range it's the plain FTS query (identical plan to before).
        // Each branch owns its statement + closure to keep the row-mapper types
        // monomorphic.
        let ids = match range {
            Some((lo, hi)) => {
                let mut stmt = conn.prepare(
                    "SELECT f.id FROM notes_fts f JOIN notes_meta m ON f.id = m.id \
                     WHERE notes_fts MATCH ?1 AND m.timestamp BETWEEN ?2 AND ?3 \
                     ORDER BY bm25(notes_fts)",
                )?;
                let rows =
                    stmt.query_map(params![fts_query, lo, hi], |row| row.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id FROM notes_fts WHERE notes_fts MATCH ?1 ORDER BY bm25(notes_fts)",
                )?;
                let rows = stmt.query_map(params![fts_query], |row| row.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        Ok(ids)
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
                .filter(|n| match range {
                    Some((lo, hi)) => n.timestamp >= lo && n.timestamp <= hi,
                    None => true,
                })
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
    // Vectors are derived too: wipe them so stale rows for vanished notes can't
    // survive the rebuild (everything re-embeds via embed_stale = 1).
    if vec_table_exists(&conn) {
        conn.execute("DELETE FROM notes_vec", [])?;
    }
    let mut count = 0u32;
    for note in &notes {
        index_note(&conn, note)?;
        count += 1;
    }
    Ok(count)
}

// -- semantic search (Phase 4) --------------------------------------------------

/// Notes whose embedding is stale (new/edited since the last embed), as
/// `(id, embed_text)` pairs ready for the engine. Creates the vec table on
/// first use.
pub fn stale_embed_texts(base: &Path) -> Result<Vec<(String, String)>> {
    let _guard = STORE_LOCK.lock().unwrap();
    let conn = open_index(base)?;
    ensure_vec_table(&conn)?;
    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM notes_meta WHERE embed_stale = 1")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        match read_note_file(&note_path(base, &id)) {
            Ok(note) => out.push((
                id,
                super::embed::note_embed_text(&note.title, &note.tldr, &note.body),
            )),
            // Index ahead of disk — drop the meta row so it stops re-surfacing.
            Err(e) => {
                log::warn!("[GRAIN] stale-embed note {id} unreadable: {e:#}");
                unindex_note(&conn, &id)?;
            }
        }
    }
    Ok(out)
}

/// Store freshly computed embeddings and clear their stale flags.
pub fn store_embeddings(base: &Path, items: &[(String, Vec<f32>)]) -> Result<()> {
    use zerocopy::IntoBytes;
    let _guard = STORE_LOCK.lock().unwrap();
    let conn = open_index(base)?;
    ensure_vec_table(&conn)?;
    for (id, embedding) in items {
        validate_id(id)?;
        // Never poison the vec index: a non-finite (NaN/Inf) or all-zero
        // embedding makes KNN return NULL distance and crash recall. Skip it
        // and leave embed_stale=1 so it retries instead of being stored.
        if !embedding.iter().all(|x| x.is_finite())
            || embedding.iter().all(|&x| x == 0.0)
        {
            log::error!(
                "[GRAIN] refusing to store non-finite/zero embedding for note {id}; leaving stale"
            );
            continue;
        }
        // vec0 has no upsert — replace by delete + insert.
        conn.execute("DELETE FROM notes_vec WHERE id = ?1", params![id])?;
        conn.execute(
            "INSERT INTO notes_vec (id, embedding) VALUES (?1, ?2)",
            params![id, embedding.as_slice().as_bytes()],
        )?;
        conn.execute(
            "UPDATE notes_meta SET embed_stale = 0 WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

/// KNN over the vec index, re-ranked with recency decay:
/// `S_final = S_semantic · exp(-λ·Δt)` where λ = ln2 / half-life and pinned
/// notes get Δt = 0. Query vector must be L2-normalized (as the engine
/// guarantees) so L2 distance is monotonic with cosine:
/// `cos = 1 − d²/2`.
pub fn semantic_search(
    base: &Path,
    query_embedding: &[f32],
    half_life_days: u32,
) -> Result<Vec<Note>> {
    semantic_search_ranged(base, query_embedding, half_life_days, None)
}

/// [GRAIN] KNN semantic search with an optional inclusive `(min_ms, max_ms)`
/// timestamp filter. The filter is applied to the KNN result set (the vec index
/// has no cheap metadata prefilter); for date-scoped recall the FTS leg
/// (`search_notes_ranged`) is the reliable workhorse, so a few semantic hits
/// falling outside the window is acceptable.
pub fn semantic_search_ranged(
    base: &Path,
    query_embedding: &[f32],
    half_life_days: u32,
    range: Option<(i64, i64)>,
) -> Result<Vec<Note>> {
    use zerocopy::IntoBytes;
    let _guard = STORE_LOCK.lock().unwrap();
    let conn = open_index(base)?;
    ensure_vec_table(&conn)?;

    let hits: Vec<(String, Option<f64>)> = {
        let mut stmt = conn.prepare(
            "SELECT id, distance FROM notes_vec
             WHERE embedding MATCH ?1
             ORDER BY distance
             LIMIT 24",
        )?;
        let rows = stmt.query_map(params![query_embedding.as_bytes()], |row| {
            // distance is NULL when a stored vector is corrupt (NaN/zero) —
            // sqlite-vec can't compute a distance against it. Skip those rows
            // instead of letting row.get::<_,f64> error out and crash recall.
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let now = chrono::Utc::now().timestamp_millis();
    let lambda_per_ms = if half_life_days == 0 {
        0.0
    } else {
        std::f64::consts::LN_2 / (half_life_days as f64 * 24.0 * 60.0 * 60.0 * 1000.0)
    };

    let mut scored: Vec<(f64, Note)> = Vec::with_capacity(hits.len());
    for (id, distance) in hits {
        let Some(distance) = distance else {
            // Self-heal: a NULL-distance row is poison — drop it and mark the
            // note stale so the next search re-embeds it cleanly.
            log::warn!("[GRAIN] NULL distance for {id}; marking stale");
            let _ = conn.execute(
                "UPDATE notes_meta SET embed_stale = 1 WHERE id = ?1",
                params![id],
            );
            let _ = conn.execute("DELETE FROM notes_vec WHERE id = ?1", params![id]);
            continue;
        };
        let note = match read_note_file(&note_path(base, &id)) {
            Ok(n) => n,
            Err(e) => {
                log::warn!("[GRAIN] semantic hit {id} unreadable: {e:#}");
                continue;
            }
        };
        // Date pre-filter (Recall's search_memory min/maxDate): drop hits
        // outside the requested window before scoring.
        if let Some((lo, hi)) = range {
            if note.timestamp < lo || note.timestamp > hi {
                continue;
            }
        }
        let similarity = 1.0 - (distance * distance) / 2.0;
        let age_ms = if note.is_pinned {
            0.0
        } else {
            (now - note.timestamp).max(0) as f64
        };
        let score = similarity * (-lambda_per_ms * age_ms).exp();
        scored.push((score, note));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scored.into_iter().map(|(_, note)| note).collect())
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
    fn corrupt_note_is_quarantined_on_scan() {
        let base = temp_base("corrupt");
        let good = Note::raw("i am fine".into());
        save_note(&base, &good).unwrap();
        // A garbage .json file lands in the notes dir (partial write, bad edit).
        let bad = notes_dir(&base).join("deadbeef-bad.json");
        fs::write(&bad, b"{ this is not valid json ").unwrap();

        let notes = list_notes(&base).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, good.id);
        // The bad file was moved out of the scan path into corrupt/.
        assert!(!bad.exists());
        assert!(corrupt_dir(&base).join("deadbeef-bad.json").exists());
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
    fn semantic_index_roundtrip_ranking_and_delete() {
        let base = temp_base("vec");
        let a = Note::raw("apples and oranges".into());
        let b = Note::raw("quarterly report".into());
        save_note(&base, &a).unwrap();
        save_note(&base, &b).unwrap();

        // Fresh notes are stale (need embedding).
        let stale = stale_embed_texts(&base).unwrap();
        assert_eq!(stale.len(), 2);
        assert!(stale
            .iter()
            .any(|(id, text)| id == &a.id && text.contains("apples")));

        // Fake orthogonal unit vectors: a → e0, b → e1.
        let mut va = vec![0.0f32; 384];
        va[0] = 1.0;
        let mut vb = vec![0.0f32; 384];
        vb[1] = 1.0;
        store_embeddings(&base, &[(a.id.clone(), va.clone()), (b.id.clone(), vb)]).unwrap();
        assert!(stale_embed_texts(&base).unwrap().is_empty());

        // Querying with a's vector ranks a first (cosine 1.0 vs 0.0).
        let hits = semantic_search(&base, &va, 30).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, a.id);

        // Editing a note marks it stale again.
        let mut edited = a.clone();
        edited.body = "apples oranges and pears".into();
        save_note(&base, &edited).unwrap();
        assert_eq!(stale_embed_texts(&base).unwrap().len(), 1);

        // Deleting removes the vector row too.
        delete_note(&base, &a.id).unwrap();
        let hits = semantic_search(&base, &va, 30).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, b.id);
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
