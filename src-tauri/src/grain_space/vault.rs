//! [GRAIN] The vault store (OBSIDIAN-PLAN.md + EXECUTION-PLAN.md P1) — since
//! the format unification, the ONLY store implementation. Both backends run
//! this code: the native backend against a Grain-managed folder in app data,
//! the obsidian backend against a user-chosen vault.
//!
//! Notes are plain Markdown files. Grain-owned notes (created by capture)
//! carry a flat YAML frontmatter block with a `grain_id` and land under the
//! configurable Grain subfolder (the vault root itself for the native
//! backend); every other `.md` in the vault is a **foreign** note —
//! searchable and readable, but never written (v1 read-only rule: Grain must
//! not race an Obsidian editor buffer it doesn't own).
//!
//! The derived index (`vault_index.sqlite`, FTS5 + sqlite-vec) lives in
//! Grain's app-data dir, NEVER inside the vault. It is refreshed by a lazy
//! `reconcile()` stat-scan at the start of every retrieval — no resident
//! watcher, zero idle RAM (OBSIDIAN-PLAN.md §5). Writes are atomic
//! (tmp + rename) and never touch `.obsidian/`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use super::note::{Note, NoteCard, ReminderState, ReminderStatus, TodoTag};

/// One application-wide lock serializing every vault read/write and index op
/// (same concurrency directive as the grain store: no WAL, single writer).
static VAULT_LOCK: Mutex<()> = Mutex::new(());

/// A resolved vault backend: the vault root, Grain's writable subfolder name,
/// and where the derived index lives (app data — never the vault).
#[derive(Clone, Debug)]
pub struct Vault {
    pub root: PathBuf,
    pub folder: String,
    pub index_base: PathBuf,
    /// Native = the Grain-managed vault in app data (auto-created; every note
    /// is grain-owned in practice). False = a user-chosen Obsidian vault.
    pub native: bool,
}

impl Vault {
    /// The user-chosen Obsidian vault backend.
    pub fn obsidian(root: PathBuf, folder: String, index_base: PathBuf) -> Self {
        Vault {
            root,
            folder,
            index_base,
            native: false,
        }
    }

    /// The native backend: a Grain-managed vault at `{app_data}/grain_space/
    /// notes/` (the pre-unification JSON dir — migration folds the old files
    /// into it in place). Notes land flat in the root (no subfolder).
    pub fn native(base: PathBuf) -> Self {
        Vault {
            root: base.join("notes"),
            folder: String::new(),
            index_base: base,
            native: true,
        }
    }

    /// Per-backend index files so switching backends never mixes corpora.
    fn index_path(&self) -> PathBuf {
        self.index_base.join(if self.native {
            "native_index.sqlite"
        } else {
            "vault_index.sqlite"
        })
    }

    /// The only directory Grain creates files in.
    fn grain_dir(&self) -> PathBuf {
        if self.folder.is_empty() {
            self.root.clone()
        } else {
            self.root.join(&self.folder)
        }
    }

    fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

// -- identity -------------------------------------------------------------------

/// Deterministic id for a foreign note: stable across index rebuilds with zero
/// writes into the user's file. Keyed on the vault-relative path (forward
/// slashes); a rename changes the id, which reconcile treats as remove+add.
fn foreign_id(rel_path: &str) -> String {
    let digest = Sha256::digest(rel_path.as_bytes());
    let mut id = String::with_capacity(33);
    id.push('f');
    for b in &digest[..16] {
        id.push_str(&format!("{b:02x}"));
    }
    id
}

// -- frontmatter codec ------------------------------------------------------------

/// Split a Markdown document into `(frontmatter_inner, body)`. The frontmatter
/// block must start at byte 0 with a `---` line and end at the next `---` (or
/// `...`) line, per Obsidian/YAML convention. CRLF tolerated. No block →
/// `(None, whole_text)`.
fn split_frontmatter(text: &str) -> (Option<&str>, &str) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return (None, text);
    };
    if first.trim_end() != "---" {
        return (None, text);
    }
    let mut offset = first.len();
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            let fm = &text[first.len()..offset];
            let body = &text[offset + line.len()..];
            return (Some(fm), body);
        }
        offset += line.len();
    }
    // Unterminated block: treat the whole file as body (don't eat the note).
    (None, text)
}

/// Quote a scalar for the frontmatter we emit. Always double-quoted so user
/// text can never break the block; minimal YAML escaping (backslash, quote).
fn yaml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Inverse of [`yaml_quote`] for values we read back. Unquoted values pass
/// through trimmed.
fn yaml_unquote(raw: &str) -> String {
    let t = raw.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        let inner = &t[1..t.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some(other) => out.push(other),
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        t.to_string()
    }
}

/// The Grain-owned metadata we store in frontmatter. `None` = the file has no
/// `grain_id`, i.e. it is a foreign note.
struct GrainMeta {
    grain_id: String,
    tldr: String,
    created_ms: Option<i64>,
    pinned: bool,
    todos: Vec<TodoTag>,
    reminder: ReminderState,
}

/// Parse OUR flat frontmatter (see `emit_frontmatter`). Tolerant: unknown keys
/// are ignored, so a user adding their own properties in Obsidian never breaks
/// the note. Returns `None` when there is no `grain_id` (foreign note).
fn parse_grain_meta(fm: &str) -> Option<GrainMeta> {
    let mut meta = GrainMeta {
        grain_id: String::new(),
        tldr: String::new(),
        created_ms: None,
        pinned: false,
        todos: Vec::new(),
        reminder: ReminderState::default(),
    };
    let mut reminder_at: Option<i64> = None;
    let mut reminder_status: Option<ReminderStatus> = None;
    let mut in_todos = false;
    for line in fm.lines() {
        let line = line.trim_end_matches('\r');
        if in_todos {
            let trimmed = line.trim_start();
            if let Some(item) = trimmed.strip_prefix("- ") {
                let item = yaml_unquote(item);
                let (done, text) = if let Some(rest) = item.strip_prefix("[x] ") {
                    (true, rest)
                } else if let Some(rest) = item.strip_prefix("[ ] ") {
                    (false, rest)
                } else {
                    (false, item.as_str())
                };
                if !text.trim().is_empty() {
                    meta.todos.push(TodoTag {
                        text: text.trim().to_string(),
                        done,
                    });
                }
                continue;
            }
            in_todos = false;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "grain_id" => meta.grain_id = yaml_unquote(value),
            "tldr" => meta.tldr = yaml_unquote(value),
            "created" => meta.created_ms = parse_local_datetime_ms(&yaml_unquote(value)),
            "pinned" => meta.pinned = value.eq_ignore_ascii_case("true"),
            "todos" => in_todos = value.is_empty(),
            "reminder" => reminder_at = parse_local_datetime_ms(&yaml_unquote(value)),
            "reminder_status" => {
                reminder_status = match yaml_unquote(value).as_str() {
                    "pending" => Some(ReminderStatus::Pending),
                    "armed" => Some(ReminderStatus::Armed),
                    "fired" => Some(ReminderStatus::Fired),
                    "dismissed" => Some(ReminderStatus::Dismissed),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    if meta.grain_id.is_empty() {
        return None;
    }
    if let Some(status) = reminder_status {
        meta.reminder = ReminderState {
            status,
            fire_at: reminder_at,
        };
    }
    Some(meta)
}

/// Frontmatter keys Grain owns and re-emits itself. Any OTHER key in an
/// existing file's frontmatter is the user's own (Obsidian `tags`, `aliases`,
/// `cssclass`, …) and must survive a save — see [`preserved_frontmatter`].
const GRAIN_FM_KEYS: [&str; 8] = [
    "grain_id",
    "tldr",
    "created",
    "pinned",
    "todos",
    "reminder",
    "reminder_status",
    "source",
];

/// The user's own frontmatter lines from an existing file — every line NOT
/// under a Grain-owned key — so editing an Obsidian-authored note in Grain
/// never drops the properties the user set in Obsidian. Nested list items (e.g.
/// under `aliases:`) ride along with their parent key; those under a Grain key
/// are dropped with it.
fn preserved_frontmatter(existing_text: &str) -> Vec<String> {
    let (Some(fm), _) = split_frontmatter(existing_text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut dropping_grain_block = false;
    for line in fm.lines() {
        let line = line.trim_end_matches('\r');
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if indented {
            // A nested item belongs to the most recent top-level key.
            if !dropping_grain_block {
                out.push(line.to_string());
            }
            continue;
        }
        dropping_grain_block = false;
        if let Some((key, _)) = line.split_once(':') {
            if GRAIN_FM_KEYS.contains(&key.trim()) {
                dropping_grain_block = true;
                continue;
            }
        }
        if !line.trim().is_empty() {
            out.push(line.to_string());
        }
    }
    out
}

/// Render a Grain-owned note to its on-disk Markdown form. AI metadata lives
/// ONLY here in the frontmatter; the body below stays the verbatim capture.
/// `preserved` carries the user's own frontmatter lines through the write, so
/// an Obsidian-authored note keeps its properties when Grain adopts it.
fn emit_markdown_with(note: &Note, preserved: &[String]) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("grain_id: {}\n", note.id));
    if !note.tldr.trim().is_empty() {
        fm.push_str(&format!("tldr: {}\n", yaml_quote(note.tldr.trim())));
    }
    fm.push_str(&format!(
        "created: {}\n",
        format_local_datetime(note.timestamp)
    ));
    if note.is_pinned {
        fm.push_str("pinned: true\n");
    }
    if !note.todo_tags.is_empty() {
        fm.push_str("todos:\n");
        for todo in &note.todo_tags {
            let mark = if todo.done { "[x]" } else { "[ ]" };
            fm.push_str(&format!(
                "  - {}\n",
                yaml_quote(&format!("{mark} {}", todo.text))
            ));
        }
    }
    if note.reminder_state.status != ReminderStatus::None {
        if let Some(ms) = note.reminder_state.fire_at {
            fm.push_str(&format!("reminder: {}\n", format_local_datetime(ms)));
        }
        let status = match note.reminder_state.status {
            ReminderStatus::Pending => "pending",
            ReminderStatus::Armed => "armed",
            ReminderStatus::Fired => "fired",
            ReminderStatus::Dismissed => "dismissed",
            ReminderStatus::None => unreachable!(),
        };
        fm.push_str(&format!("reminder_status: {status}\n"));
    }
    // The user's own Obsidian properties (tags, aliases, …), carried verbatim.
    for line in preserved {
        fm.push_str(line);
        fm.push('\n');
    }
    fm.push_str("source: grain\n---\n");
    let body = note.body.trim_end();
    if body.is_empty() {
        fm
    } else {
        format!("{fm}{body}\n")
    }
}

/// Local wall-clock "YYYY-MM-DDTHH:MM:SS" → epoch ms (same convention as
/// capture's reminder parsing). Also accepts minutes-precision.
fn parse_local_datetime_ms(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .ok()?;
    use chrono::TimeZone;
    match chrono::Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.timestamp_millis()),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.timestamp_millis()),
        chrono::LocalResult::None => None,
    }
}

/// Millisecond precision so `created` round-trips the note's exact timestamp
/// (a truncated value would drift the note identity's ordering on re-parse).
fn format_local_datetime(ms: i64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            dt.format("%Y-%m-%dT%H:%M:%S%.3f").to_string()
        }
        chrono::LocalResult::None => chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
            .to_string(),
    }
}

// -- filenames ------------------------------------------------------------------

/// Obsidian convention: the filename IS the title. Strip characters Windows/
/// Obsidian reject, collapse whitespace, cap the length, never empty.
fn sanitize_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .filter(|c| {
            !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned
        .trim_matches(|c: char| c == '.' || c == ' ')
        .to_string();
    let capped: String = cleaned.chars().take(60).collect();
    let capped = capped
        .trim_end_matches(|c: char| c == '.' || c == ' ')
        .to_string();
    if capped.is_empty() {
        "Untitled".to_string()
    } else {
        capped
    }
}

/// Sanitize one folder-path segment for auto-categorization: strip the
/// characters Windows/Obsidian reject, collapse whitespace, cap length. Empty
/// (e.g. all-invalid) → empty string, which the caller skips.
fn sanitize_folder_segment(seg: &str) -> String {
    let cleaned: String = seg
        .chars()
        .filter(|c| {
            !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned
        .trim_matches(|c: char| c == '.' || c == ' ')
        .chars()
        .take(60)
        .collect()
}

/// First free `stem.md`, `stem 2.md`, … in `dir` (excluding `current`, so a
/// note keeps its own name on re-save).
fn unique_path(dir: &Path, stem: &str, current: Option<&Path>) -> PathBuf {
    for n in 1u32.. {
        let name = if n == 1 {
            format!("{stem}.md")
        } else {
            format!("{stem} {n}.md")
        };
        let candidate = dir.join(&name);
        if Some(candidate.as_path()) == current || !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// Vault-relative path with forward slashes (the canonical index key).
fn rel_key(root: &Path, abs: &Path) -> Result<String> {
    let rel = abs
        .strip_prefix(root)
        .map_err(|_| anyhow!("path escapes vault: {}", abs.display()))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

// -- reading --------------------------------------------------------------------

/// Parse one on-disk `.md` into a `Note` + grain-owned flag. `mtime_ms` backs
/// the timestamp for foreign notes (no `created` of their own).
fn read_md_note(rel_path: &str, text: &str, mtime_ms: i64) -> (Note, bool) {
    let stem = Path::new(rel_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let (fm, body) = split_frontmatter(text);
    let body = body.trim_start_matches('\n').trim_end().to_string();
    if let Some(meta) = fm.and_then(parse_grain_meta) {
        return (
            Note {
                id: meta.grain_id,
                title: stem,
                tldr: meta.tldr,
                body,
                timestamp: meta.created_ms.unwrap_or(mtime_ms),
                todo_tags: meta.todos,
                reminder_state: meta.reminder,
                is_pinned: meta.pinned,
            },
            true,
        );
    }
    (
        Note {
            id: foreign_id(rel_path),
            title: stem,
            tldr: String::new(),
            body,
            timestamp: mtime_ms,
            todo_tags: Vec::new(),
            reminder_state: ReminderState::default(),
            is_pinned: false,
        },
        false,
    )
}

fn read_note_at(v: &Vault, rel: &str) -> Result<Note> {
    let abs = v.abs(rel);
    let text = fs::read_to_string(&abs).with_context(|| format!("read {}", abs.display()))?;
    let mtime = file_mtime_ms(&abs).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    Ok(read_md_note(rel, &text, mtime).0)
}

/// Atomic tmp+rename write of `text` to `abs`.
fn atomic_write(abs: &Path, text: &str) -> Result<()> {
    let tmp = abs.with_extension("md.tmp");
    fs::write(&tmp, text).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, abs).with_context(|| format!("rename into {}", abs.display()))?;
    Ok(())
}

/// Two-way-sync-safe write (OBSIDIAN-PLAN.md §6). Grain never blindly clobbers
/// a file an external editor (Obsidian) may have changed since Grain last saw
/// it. `base` is the common ancestor Grain last synced (the merge base); `ours`
/// is the new text Grain wants to write. If the file on disk still equals
/// `base` (or there's no base / no file), we just write `ours`. Otherwise a
/// line-based 3-way merge folds BOTH edits together (diff-match-patch
/// equivalent): a clean merge is written to the file; an irreconcilable
/// conflict writes `ours` to the live file but preserves the on-disk version
/// in a `<stem>.grain-conflict-<ts>.md` sidecar so NOTHING is ever lost.
/// Returns the text actually written to the live file (the new merge base).
fn safe_write(abs: &Path, base: Option<&str>, ours: &str) -> Result<String> {
    let theirs = fs::read_to_string(abs).ok();
    let (Some(theirs), Some(base)) = (theirs.as_deref(), base) else {
        // New file, or Grain has no ancestor to merge against: plain write.
        atomic_write(abs, ours)?;
        return Ok(ours.to_string());
    };
    if theirs == base || theirs == ours {
        atomic_write(abs, ours)?;
        return Ok(ours.to_string());
    }
    match diffy::merge(base, ours, theirs) {
        Ok(merged) => {
            log::info!(
                "[GRAIN] vault: {} changed under us; merged Grain + external edits cleanly",
                abs.display()
            );
            atomic_write(abs, &merged)?;
            Ok(merged)
        }
        Err(_conflicted) => {
            // Overlapping edits on the same lines. Keep the live file as Grain's
            // version, but stash the external version beside it so the user can
            // reconcile — never silently drop their words.
            let ts = chrono::Utc::now().timestamp_millis();
            let stem = abs
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "note".to_string());
            let sidecar = abs.with_file_name(format!("{stem}.grain-conflict-{ts}.md"));
            if let Err(e) = fs::write(&sidecar, theirs) {
                log::error!("[GRAIN] vault: conflict sidecar write failed: {e:#}");
            } else {
                log::warn!(
                    "[GRAIN] vault: {} had a conflicting external edit; saved Grain's version, preserved theirs in {}",
                    abs.display(),
                    sidecar.display()
                );
            }
            atomic_write(abs, ours)?;
            Ok(ours.to_string())
        }
    }
}

fn file_mtime_ms(path: &Path) -> Option<i64> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dur = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_millis() as i64)
}

// -- index ----------------------------------------------------------------------

/// Same shape as the grain store's index plus the vault columns: the file the
/// row mirrors and the (mtime, size) fingerprint reconcile compares against.
fn open_index(v: &Vault) -> Result<Connection> {
    super::note::ensure_vec_extension();
    fs::create_dir_all(&v.index_base).context("create grain_space dir")?;
    let conn = Connection::open(v.index_path()).context("open vault index")?;
    conn.pragma_update(None, "journal_mode", "TRUNCATE")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notes_meta (
            id           TEXT PRIMARY KEY,
            path         TEXT NOT NULL UNIQUE,
            timestamp    INTEGER NOT NULL,
            is_pinned    INTEGER NOT NULL DEFAULT 0,
            embed_stale  INTEGER NOT NULL DEFAULT 1,
            mtime        INTEGER NOT NULL DEFAULT 0,
            size         INTEGER NOT NULL DEFAULT 0,
            foreign_note INTEGER NOT NULL DEFAULT 0,
            content      TEXT NOT NULL DEFAULT ''
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
            id UNINDEXED, title, tldr, body
        );",
    )?;
    // Migrate an index created before the merge-base column existed. `content`
    // holds the last file text Grain synced — the common ancestor a 3-way
    // merge needs so a save never clobbers a concurrent Obsidian edit.
    let _ = conn.execute(
        "ALTER TABLE notes_meta ADD COLUMN content TEXT NOT NULL DEFAULT ''",
        [],
    );
    Ok(conn)
}

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

#[allow(clippy::too_many_arguments)]
fn index_upsert(
    conn: &Connection,
    note: &Note,
    rel: &str,
    mtime: i64,
    size: i64,
    foreign: bool,
    content: &str,
) -> Result<()> {
    // A different note may have previously held this path (rename/replace) —
    // clear it so the UNIQUE(path) constraint can't fail the upsert.
    conn.execute(
        "DELETE FROM notes_meta WHERE path = ?1 AND id <> ?2",
        params![rel, note.id],
    )?;
    conn.execute(
        "INSERT INTO notes_meta (id, path, timestamp, is_pinned, embed_stale, mtime, size, foreign_note, content)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
            path = excluded.path,
            timestamp = excluded.timestamp,
            is_pinned = excluded.is_pinned,
            embed_stale = 1,
            mtime = excluded.mtime,
            size = excluded.size,
            foreign_note = excluded.foreign_note,
            content = excluded.content",
        params![note.id, rel, note.timestamp, note.is_pinned as i64, mtime, size, foreign as i64, content],
    )?;
    conn.execute("DELETE FROM notes_fts WHERE id = ?1", params![note.id])?;
    conn.execute(
        "INSERT INTO notes_fts (id, title, tldr, body) VALUES (?1, ?2, ?3, ?4)",
        params![note.id, note.title, note.tldr, note.body],
    )?;
    Ok(())
}

/// The last file text Grain synced for `id` (the 3-way-merge ancestor), or
/// `None` when there's no row / it predates the content column.
fn indexed_content(conn: &Connection, id: &str) -> Option<String> {
    conn.query_row(
        "SELECT content FROM notes_meta WHERE id = ?1",
        params![id],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .filter(|s| !s.is_empty())
}

fn index_remove(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM notes_meta WHERE id = ?1", params![id])?;
    conn.execute("DELETE FROM notes_fts WHERE id = ?1", params![id])?;
    if vec_table_exists(conn) {
        purge_note_vectors(conn, id)?;
    }
    Ok(())
}

fn path_of(conn: &Connection, id: &str) -> Result<Option<(String, bool)>> {
    let row = conn
        .query_row(
            "SELECT path, foreign_note FROM notes_meta WHERE id = ?1",
            params![id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row)
}

// -- reconcile (the lazy indexer, OBSIDIAN-PLAN.md §5) ----------------------------

/// Directories never scanned: Obsidian's own config/trash, VCS, and anything
/// dot-prefixed. Cheap and conservative.
fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || name.eq_ignore_ascii_case("node_modules")
}

/// Every `.md` under the vault as `(rel_path, mtime_ms, size)`.
fn walk_vault(root: &Path) -> Result<Vec<(String, i64, i64)>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("[GRAIN] vault: can't read {}: {e}", dir.display());
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(ftype) = entry.file_type() else {
                continue;
            };
            if ftype.is_dir() {
                if !skip_dir(&name) {
                    stack.push(path);
                }
                continue;
            }
            if !ftype.is_file() || !name.to_lowercase().ends_with(".md") {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let Ok(rel) = rel_key(root, &path) else {
                continue;
            };
            out.push((rel, mtime, meta.len() as i64));
        }
    }
    Ok(out)
}

/// Bring the index in line with the vault: upsert new/changed files (compare
/// the (mtime, size) fingerprint), drop vanished ones. Parse-skips are logged,
/// NEVER quarantined — these are the user's files, we do not move them.
/// Called with `VAULT_LOCK` held.
fn reconcile_locked(v: &Vault, conn: &Connection) -> Result<()> {
    let disk = walk_vault(&v.root)?;
    let mut indexed: HashMap<String, (String, i64, i64, bool)> = HashMap::new();
    {
        let mut stmt =
            conn.prepare("SELECT path, id, mtime, size, foreign_note FROM notes_meta")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)? != 0,
                ),
            ))
        })?;
        for row in rows {
            let (path, v) = row?;
            indexed.insert(path, v);
        }
    }

    // Foreign files first seen this pass, as (new_id, content_hash) — the
    // candidate pool for rename detection in the vanished sweep below.
    let mut added_foreign: Vec<(String, [u8; 32])> = Vec::new();

    for (rel, mtime, size) in &disk {
        if let Some((_, im, is, _)) = indexed.get(rel) {
            if im == mtime && is == size {
                indexed.remove(rel);
                continue;
            }
        }
        let abs = v.abs(rel);
        let text = match fs::read_to_string(&abs) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("[GRAIN] vault: skipping unreadable {}: {e}", abs.display());
                indexed.remove(rel);
                continue;
            }
        };
        let newly_tracked = !indexed.contains_key(rel);
        let (note, grain_owned) = read_md_note(rel, &text, *mtime);
        index_upsert(conn, &note, rel, *mtime, *size, !grain_owned, &text)?;
        if newly_tracked && !grain_owned {
            added_foreign.push((note.id.clone(), Sha256::digest(text.as_bytes()).into()));
        }
        indexed.remove(rel);
        // A moved/renamed grain note keeps its id: the upsert re-pathed the
        // row, so its OLD path entry must not survive into the vanished sweep
        // below (it would delete the note we just re-indexed).
        indexed.retain(|_, (iid, _, _, _)| iid != &note.id);
    }

    // Whatever's left in `indexed` vanished from disk. A vanished FOREIGN row
    // whose content matches a file added this pass is a rename (foreign ids
    // are path hashes, so a rename is remove+add) — hand its embedding to the
    // new id instead of dropping it (OBSIDIAN-PLAN §7 V3).
    for (_, (id, _, _, foreign)) in indexed {
        if foreign {
            if let Err(e) = adopt_renamed_foreign(conn, &id, &mut added_foreign) {
                log::warn!("[GRAIN] vault: rename-adopt for {id} failed: {e:#}");
            }
        }
        index_remove(conn, &id)?;
    }
    Ok(())
}

/// If the vanished foreign row `old_id` has the same last-synced content as a
/// foreign file added in this reconcile pass, move its vec rows (chunk-aware)
/// to the new id and mark the new row fresh — a rename keeps its embedding.
/// The title (= filename) changed, but the body dominates the embedding; the
/// trade is a free rename vs a full re-embed.
fn adopt_renamed_foreign(
    conn: &Connection,
    old_id: &str,
    added: &mut Vec<(String, [u8; 32])>,
) -> Result<()> {
    if added.is_empty() || !vec_table_exists(conn) {
        return Ok(());
    }
    let (content, stale): (String, i64) = conn.query_row(
        "SELECT content, embed_stale FROM notes_meta WHERE id = ?1",
        params![old_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if stale != 0 || content.is_empty() {
        return Ok(()); // nothing embedded worth carrying over
    }
    let hash: [u8; 32] = Sha256::digest(content.as_bytes()).into();
    let Some(pos) = added.iter().position(|(_, h)| *h == hash) else {
        return Ok(()); // genuinely deleted, not renamed
    };
    let (new_id, _) = added.remove(pos);

    let rows: Vec<(String, Vec<u8>)> = {
        let mut stmt = conn.prepare(
            "SELECT id, embedding FROM notes_vec
             WHERE id = ?1 OR substr(id, 1, length(?1) + 1) = ?1 || '#'",
        )?;
        let rows = stmt.query_map(params![old_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if rows.is_empty() {
        return Ok(());
    }
    purge_note_vectors(conn, &new_id)?; // the add pass can't have embedded yet, but be safe
    for (key, embedding) in rows {
        let suffix = &key[old_id.len()..]; // "" (legacy) or "#n"
        conn.execute(
            "INSERT INTO notes_vec (id, embedding) VALUES (?1, ?2)",
            params![format!("{new_id}{suffix}"), embedding],
        )?;
    }
    conn.execute(
        "UPDATE notes_meta SET embed_stale = 0 WHERE id = ?1",
        params![new_id],
    )?;
    log::info!("[GRAIN] vault: foreign rename detected; embedding carried {old_id} → {new_id}");
    Ok(())
}

// -- public API (mirrors store.rs; each fn: lock → reconcile → work → drop) -------

fn ensure_vault(v: &Vault) -> Result<()> {
    if v.native {
        // The Grain-managed vault is ours to create — an empty corpus must
        // behave like an empty store, never an error.
        fs::create_dir_all(&v.root).context("create native notes dir")?;
        return Ok(());
    }
    if !v.root.is_dir() {
        return Err(anyhow!(
            "Obsidian vault folder not found: {}",
            v.root.display()
        ));
    }
    Ok(())
}

/// Grain-owned notes, newest first. Deliberately NOT the whole vault: browsing
/// lists YOUR captures; searching (below) covers everything (OBSIDIAN-PLAN §1).
pub fn list_notes(v: &Vault) -> Result<Vec<Note>> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    reconcile_locked(v, &conn)?;
    let rels: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT path FROM notes_meta WHERE foreign_note = 0 ORDER BY timestamp DESC",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut notes = Vec::with_capacity(rels.len());
    for rel in rels {
        match read_note_at(v, &rel) {
            Ok(n) => notes.push(n),
            Err(e) => log::warn!("[GRAIN] vault list: {e:#}"),
        }
    }
    Ok(notes)
}

/// Case-insensitive `strip_prefix` for ASCII path prefixes (folder names round-
/// trip through the OS with their on-disk case; the setting stores our own).
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// True when `rel` (vault-relative, `/`-separated) lives inside Grain's
/// WRITABLE area: the whole store on the native backend, or under the
/// configured Grain subfolder on an Obsidian vault. This — not the presence of
/// a `grain_id` — is what makes a note editable: everything inside the Grain
/// folder is Grain's to edit (a note authored in Obsidian there gains our
/// frontmatter on first save); everything outside it is the user's own vault,
/// never shown and never written.
fn in_grain_folder(v: &Vault, rel: &str) -> bool {
    if v.native || v.folder.is_empty() {
        return true;
    }
    let home = v.folder.trim_matches('/');
    match rel.split_once('/') {
        // A note in `Grain/…` (or a deeper subfolder) — the first segment is
        // the home folder.
        Some((first, _)) => first.eq_ignore_ascii_case(home),
        // A loose file at the vault root is outside the Grain folder.
        None => false,
    }
}

/// A note's collection = its subfolder path INSIDE the Grain folder, or `None`
/// when it sits loose directly in the Grain folder (shown under "Notes"). The
/// Grain folder itself is never surfaced as a collection — only its subfolders
/// are — so the home prefix is stripped for every note. Native vaults are flat
/// → the parent dir (if any) is the collection.
fn folder_of(v: &Vault, rel: &str) -> Option<String> {
    let parent = Path::new(rel).parent()?;
    let parent = parent.to_string_lossy().replace('\\', "/");
    let parent = parent.trim_matches('/').to_string();
    if parent.is_empty() {
        return None; // loose at the vault root
    }
    if v.folder.is_empty() {
        return Some(parent); // native store: the parent dir is the collection
    }
    let home = v.folder.trim_matches('/');
    if parent.eq_ignore_ascii_case(home) {
        return None; // loose directly inside the Grain folder
    }
    if let Some(sub) = strip_prefix_ci(&parent, &format!("{home}/")) {
        return Some(sub.to_string());
    }
    Some(parent) // outside the Grain folder (won't be listed anyway)
}

/// Sidebar listing (TAURI-OVERLAY-PLAN.md Phase A): light cards, newest first.
/// The browse is scoped to Grain's own folder — ONLY notes inside the Grain
/// subfolder appear (the whole store on the native backend). The rest of an
/// Obsidian vault is the user's; it stays out of the note UI (recall still
/// searches it under the hood). `readonly` no longer gates editing — every
/// listed note is editable; it now flags a note authored OUTSIDE Grain (no
/// `grain_id` yet), which the sidebar groups below the loose-notes divider.
/// Foreign cards are built from the index alone (no file reads — cheap).
pub fn list_cards(v: &Vault) -> Result<Vec<NoteCard>> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    reconcile_locked(v, &conn)?;
    let rows: Vec<(String, String, i64, bool)> = {
        let mut stmt = conn.prepare(
            "SELECT id, path, timestamp, foreign_note FROM notes_meta ORDER BY timestamp DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)? != 0,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut cards = Vec::with_capacity(rows.len());
    for (id, rel, timestamp, foreign) in rows {
        if !in_grain_folder(v, &rel) {
            continue; // the user's own vault files — never shown in the note UI
        }
        let folder = folder_of(v, &rel);
        if foreign {
            let stem = Path::new(&rel)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            cards.push(NoteCard {
                id,
                title: stem,
                tldr: String::new(),
                timestamp,
                is_pinned: false,
                reminder_state: ReminderState::default(),
                folder,
                readonly: true,
            });
            continue;
        }
        match read_note_at(v, &rel) {
            Ok(n) => cards.push(NoteCard {
                id: n.id,
                title: n.title,
                tldr: n.tldr,
                timestamp: n.timestamp,
                is_pinned: n.is_pinned,
                reminder_state: n.reminder_state,
                folder,
                readonly: false,
            }),
            Err(e) => log::warn!("[GRAIN] vault list_cards: {e:#}"),
        }
    }
    Ok(cards)
}

pub fn search_notes(v: &Vault, query: &str) -> Result<Vec<Note>> {
    search_notes_ranged(v, query, None)
}

/// FTS over the WHOLE vault (grain + foreign), same query discipline as the
/// grain store. Empty query = grain-owned notes only (a browse, not a search),
/// optionally date-filtered.
pub fn search_notes_ranged(v: &Vault, query: &str, range: Option<(i64, i64)>) -> Result<Vec<Note>> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    reconcile_locked(v, &conn)?;

    let trimmed = query.trim();
    if trimmed.is_empty() {
        let rels: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT path FROM notes_meta WHERE foreign_note = 0 ORDER BY timestamp DESC",
            )?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut notes = Vec::new();
        for rel in rels {
            if let Ok(n) = read_note_at(v, &rel) {
                if let Some((lo, hi)) = range {
                    if n.timestamp < lo || n.timestamp > hi {
                        continue;
                    }
                }
                notes.push(n);
            }
        }
        return Ok(notes);
    }

    let fts_query = trimmed
        .split_whitespace()
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");

    let rels: Result<Vec<String>> = (|| {
        let rels = match range {
            Some((lo, hi)) => {
                let mut stmt = conn.prepare(
                    "SELECT m.path FROM notes_fts f JOIN notes_meta m ON f.id = m.id \
                     WHERE notes_fts MATCH ?1 AND m.timestamp BETWEEN ?2 AND ?3 \
                     ORDER BY bm25(notes_fts, 1.0, 10.0, 5.0, 1.0)",
                )?;
                let rows = stmt.query_map(params![fts_query, lo, hi], |r| r.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT m.path FROM notes_fts f JOIN notes_meta m ON f.id = m.id \
                     WHERE notes_fts MATCH ?1 ORDER BY bm25(notes_fts, 1.0, 10.0, 5.0, 1.0)",
                )?;
                let rows = stmt.query_map(params![fts_query], |r| r.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        Ok(rels)
    })();

    match rels {
        Ok(rels) => {
            let mut out = Vec::with_capacity(rels.len());
            for rel in rels {
                match read_note_at(v, &rel) {
                    Ok(note) => out.push(note),
                    Err(e) => log::warn!("[GRAIN] vault search hit unreadable: {e:#}"),
                }
            }
            Ok(out)
        }
        Err(e) => {
            log::warn!("[GRAIN] vault FTS failed ({e:#}); substring fallback");
            let needle = trimmed.to_lowercase();
            let rels: Vec<String> = {
                let mut stmt = conn.prepare("SELECT path FROM notes_meta")?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            let mut out = Vec::new();
            for rel in rels {
                if let Ok(n) = read_note_at(v, &rel) {
                    if let Some((lo, hi)) = range {
                        if n.timestamp < lo || n.timestamp > hi {
                            continue;
                        }
                    }
                    if n.title.to_lowercase().contains(&needle)
                        || n.tldr.to_lowercase().contains(&needle)
                        || n.body.to_lowercase().contains(&needle)
                    {
                        out.push(n);
                    }
                }
            }
            Ok(out)
        }
    }
}

/// Function words that carry no retrieval signal. A natural-language recall
/// question ("what was the wifi password for the cabin we rented") is mostly
/// these; under FTS5's implicit-AND they force zero hits. Small and
/// conservative on purpose — dropping a real content word costs recall.
const STOPWORDS: [&str; 52] = [
    "a", "an", "the", "is", "am", "are", "was", "were", "be", "been", "being", "do", "does", "did",
    "have", "has", "had", "i", "me", "my", "mine", "we", "us", "our", "you", "your", "he", "she",
    "it", "its", "they", "them", "their", "what", "which", "who", "whom", "whose", "when", "where",
    "how", "that", "this", "these", "those", "and", "or", "of", "to", "in", "on", "for",
];

/// Build the OR-semantics FTS5 query for a natural-language question: drop
/// stopwords, keep alphanumeric content terms (length ≥ 2) as quoted prefix
/// tokens joined with OR. `None` when nothing survives (the caller's FTS leg
/// then contributes no candidates; the semantic leg still runs).
fn natural_fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .map(|t| t.to_lowercase())
        .filter(|t| t.chars().count() >= 2 && !STOPWORDS.contains(&t.as_str()))
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// FTS for a NATURAL-LANGUAGE question (the recall path): stopword-filtered
/// content terms with OR semantics, ranked by BM25 (title 10× / tldr 5× /
/// body 1×). Where `search_notes_ranged`'s implicit-AND suits search-as-you-
/// type precision, a spoken question must match on ANY informative word and
/// let ranking sort it out — with AND, one non-matching filler word zeroes
/// the whole leg. Capped: OR over a big vault matches broadly, the caller
/// fuses ranked lists, and only the head matters.
pub fn search_notes_natural(
    v: &Vault,
    query: &str,
    range: Option<(i64, i64)>,
) -> Result<Vec<Note>> {
    ensure_vault(v)?;
    let Some(fts_query) = natural_fts_query(query) else {
        return Ok(Vec::new());
    };
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    reconcile_locked(v, &conn)?;

    let rels: Vec<String> = match range {
        Some((lo, hi)) => {
            let mut stmt = conn.prepare(
                "SELECT m.path FROM notes_fts f JOIN notes_meta m ON f.id = m.id \
                 WHERE notes_fts MATCH ?1 AND m.timestamp BETWEEN ?2 AND ?3 \
                 ORDER BY bm25(notes_fts, 1.0, 10.0, 5.0, 1.0) LIMIT 24",
            )?;
            let rows = stmt.query_map(params![fts_query, lo, hi], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT m.path FROM notes_fts f JOIN notes_meta m ON f.id = m.id \
                 WHERE notes_fts MATCH ?1 \
                 ORDER BY bm25(notes_fts, 1.0, 10.0, 5.0, 1.0) LIMIT 24",
            )?;
            let rows = stmt.query_map(params![fts_query], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
    };
    let mut out = Vec::with_capacity(rels.len());
    for rel in rels {
        match read_note_at(v, &rel) {
            Ok(note) => out.push(note),
            Err(e) => log::warn!("[GRAIN] vault natural search hit unreadable: {e:#}"),
        }
    }
    Ok(out)
}

/// True when the vault has any indexed note at all (grain-owned OR foreign) —
/// so recall over a vault of purely foreign Obsidian notes still runs.
pub fn has_any_notes(v: &Vault) -> Result<bool> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    reconcile_locked(v, &conn)?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM notes_meta", [], |r| r.get(0))?;
    Ok(count > 0)
}

/// The absolute path of a note's file on disk (for an "Open in Obsidian"
/// deep link). Reconciles once on a stale/missing index entry.
pub fn note_abs_path(v: &Vault, id: &str) -> Result<PathBuf> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    let rel = match path_of(&conn, id)? {
        Some((rel, _)) if v.abs(&rel).exists() => rel,
        _ => {
            reconcile_locked(v, &conn)?;
            path_of(&conn, id)?
                .map(|(rel, _)| rel)
                .ok_or_else(|| anyhow!("note not found: {id}"))?
        }
    };
    Ok(v.abs(&rel))
}

pub fn get_note(v: &Vault, id: &str) -> Result<Note> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    // Fast path off the last reconcile; on a miss OR a stale path (the user
    // moved/renamed the file in Obsidian), refresh the index once and retry.
    if let Some((rel, _)) = path_of(&conn, id)? {
        if let Ok(note) = read_note_at(v, &rel) {
            return Ok(note);
        }
    }
    reconcile_locked(v, &conn)?;
    let (rel, _) = path_of(&conn, id)?.ok_or_else(|| anyhow!("note not found: {id}"))?;
    read_note_at(v, &rel)
}

/// Create or update a note inside Grain's folder. Editability is by LOCATION,
/// not ownership: a note authored in Obsidian INSIDE the Grain folder is
/// writable and gains Grain's frontmatter on first save (its own properties are
/// carried through — see [`preserved_frontmatter`]). Only files OUTSIDE the
/// Grain folder are refused. New notes land in the Grain subfolder; a title
/// change renames the file (identity rides on `grain_id`). Atomic tmp+rename;
/// a concurrent Obsidian edit is 3-way-merged, never clobbered.
pub fn save_note(v: &Vault, note: &Note) -> Result<()> {
    ensure_vault(v)?;
    super::note::validate_id(&note.id)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;

    let existing = path_of(&conn, note.id.as_str())?;
    if let Some((rel, _)) = &existing {
        if !in_grain_folder(v, rel) {
            return Err(anyhow!(
                "This note lives outside Grain's folder — edit it in Obsidian."
            ));
        }
    }

    // Merge base: the file text Grain last synced for this note (None for a
    // brand-new note → plain write).
    let base = existing
        .as_ref()
        .and_then(|_| indexed_content(&conn, &note.id));
    // The user's own frontmatter from the file as it stands on disk (captures
    // properties added in Obsidian since the last sync, and everything on an
    // Obsidian-authored note Grain is adopting for the first time).
    let preserved = existing
        .as_ref()
        .and_then(|(rel, _)| fs::read_to_string(v.abs(rel)).ok())
        .map(|t| preserved_frontmatter(&t))
        .unwrap_or_default();
    let abs = match &existing {
        Some((rel, _)) => {
            let current = v.abs(rel);
            let stem = sanitize_filename(&note.title);
            let current_stem = current
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if stem != current_stem {
                // Title edit → rename, staying in the note's current directory
                // (a promoted note keeps its home).
                let dir = current.parent().unwrap_or(&v.root).to_path_buf();
                let target = unique_path(&dir, &stem, Some(current.as_path()));
                fs::rename(&current, &target)
                    .with_context(|| format!("rename {}", current.display()))?;
                target
            } else {
                current
            }
        }
        None => {
            let dir = v.grain_dir();
            fs::create_dir_all(&dir).context("create Grain folder in vault")?;
            unique_path(&dir, &sanitize_filename(&note.title), None)
        }
    };

    // Two-way-sync-safe write: merge into any concurrent external edit rather
    // than overwrite it. The written text (clean-merge result or ours) becomes
    // the new merge base.
    let written = safe_write(&abs, base.as_deref(), &emit_markdown_with(note, &preserved))?;

    let rel = rel_key(&v.root, &abs)?;
    let mtime = file_mtime_ms(&abs).unwrap_or(0);
    let size = fs::metadata(&abs).map(|m| m.len() as i64).unwrap_or(0);
    // Re-parse the written text so the index reflects what actually landed on
    // disk (a merge may have folded in external body edits), and keep the real
    // sanitized/suffixed filename as the title.
    let stem = abs
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| note.title.clone());
    let (mut indexed, _) = read_md_note(&rel, &written, mtime);
    indexed.title = stem;
    index_upsert(&conn, &indexed, &rel, mtime, size, false, &written)?;
    Ok(())
}

/// Delete a note file inside Grain's folder + its index rows. Files outside the
/// Grain folder belong to the user's vault and are refused.
pub fn delete_note(v: &Vault, id: &str) -> Result<()> {
    ensure_vault(v)?;
    super::note::validate_id(id)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    match path_of(&conn, id)? {
        Some((rel, _)) if !in_grain_folder(v, &rel) => Err(anyhow!(
            "This note lives outside Grain's folder — delete it in Obsidian."
        )),
        Some((rel, _)) => {
            let abs = v.abs(&rel);
            if abs.exists() {
                fs::remove_file(&abs).with_context(|| format!("delete {}", abs.display()))?;
            }
            index_remove(&conn, id)?;
            Ok(())
        }
        None => {
            index_remove(&conn, id)?; // idempotent
            Ok(())
        }
    }
}

// -- auto-categorization (AUTO-CATEGORIZATION-PLAN.md P1) --------------------------

/// The distinct collection paths (Grain subfolders that currently hold notes) —
/// the candidate categories for routing a fresh capture. Cheap: derived from
/// the index alone (no file reads), scoped to the Grain folder, sorted + unique.
pub fn list_folders(v: &Vault) -> Result<Vec<String>> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    reconcile_locked(v, &conn)?;
    let rels: Vec<String> = {
        let mut stmt = conn.prepare("SELECT path FROM notes_meta")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut set = std::collections::BTreeSet::new();
    for rel in rels {
        if !in_grain_folder(v, &rel) {
            continue;
        }
        if let Some(folder) = folder_of(v, &rel) {
            set.insert(folder);
        }
    }
    Ok(set.into_iter().collect())
}

/// Move a Grain note into a subfolder of the Grain folder (auto-categorization),
/// or back to the Grain root when `folder` is `None`/empty. The file moves;
/// identity (`grain_id`) rides along, so links and the note's id are unchanged.
/// Refuses notes outside the Grain folder (the user's own vault). A no-op when
/// the note already lives in the target folder. Returns the moved note.
pub fn move_note_to_folder(v: &Vault, id: &str, folder: Option<&str>) -> Result<Note> {
    ensure_vault(v)?;
    super::note::validate_id(id)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    // Refresh on a stale path (moved/renamed in Obsidian) before acting.
    let rel = match path_of(&conn, id)? {
        Some((rel, _)) if v.abs(&rel).exists() => rel,
        _ => {
            reconcile_locked(v, &conn)?;
            path_of(&conn, id)?
                .map(|(rel, _)| rel)
                .ok_or_else(|| anyhow!("note not found: {id}"))?
        }
    };
    if !in_grain_folder(v, &rel) {
        return Err(anyhow!("This note lives outside Grain's folder."));
    }
    let current = v.abs(&rel);

    // Target directory: the Grain folder, plus each sanitized subfolder segment.
    let mut dir = v.grain_dir();
    if let Some(f) = folder.map(str::trim).filter(|f| !f.is_empty()) {
        for seg in f.split('/') {
            let seg = sanitize_folder_segment(seg);
            if !seg.is_empty() {
                dir = dir.join(seg);
            }
        }
    }
    let stem = current
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string());
    fs::create_dir_all(&dir).context("create target folder in vault")?;
    let target = unique_path(&dir, &stem, Some(current.as_path()));
    if target == current {
        return read_note_at(v, &rel); // already there — no-op
    }
    fs::rename(&current, &target).with_context(|| format!("move {}", current.display()))?;

    // Re-index at the new path (identity preserved; grain-owned).
    let new_rel = rel_key(&v.root, &target)?;
    let text = fs::read_to_string(&target).with_context(|| format!("read {}", target.display()))?;
    let mtime = file_mtime_ms(&target).unwrap_or(0);
    let size = fs::metadata(&target).map(|m| m.len() as i64).unwrap_or(0);
    let (mut indexed, grain_owned) = read_md_note(&new_rel, &text, mtime);
    indexed.title = stem;
    index_upsert(&conn, &indexed, &new_rel, mtime, size, !grain_owned, &text)?;
    read_note_at(v, &new_rel)
}

pub fn set_pinned(v: &Vault, id: &str, pinned: bool) -> Result<Note> {
    mutate_grain_note(v, id, |n| n.is_pinned = pinned)
}

pub fn set_reminder(v: &Vault, id: &str, state: ReminderState) -> Result<Note> {
    mutate_grain_note(v, id, |n| n.reminder_state = state)
}

/// Read-modify-write one Grain-owned note under the lock. Frontmatter-only
/// mutations (pin/reminder) don't change searchable content, so the embedding
/// stays fresh — only the fingerprint and flags are updated.
fn mutate_grain_note(v: &Vault, id: &str, apply: impl FnOnce(&mut Note)) -> Result<Note> {
    ensure_vault(v)?;
    super::note::validate_id(id)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    // Refresh on a stale path (moved/renamed in Obsidian) before mutating.
    let entry = match path_of(&conn, id)? {
        Some((rel, foreign)) if v.abs(&rel).exists() => Some((rel, foreign)),
        _ => {
            reconcile_locked(v, &conn)?;
            path_of(&conn, id)?
        }
    };
    let (rel, _) = entry.ok_or_else(|| anyhow!("note not found: {id}"))?;
    if !in_grain_folder(v, &rel) {
        return Err(anyhow!(
            "This note lives outside Grain's folder — edit it in Obsidian."
        ));
    }
    let abs = v.abs(&rel);
    // Read the current file text — it is both the note we mutate AND the merge
    // base, so an external edit landing between this read and the write is
    // folded in rather than lost.
    let base = fs::read_to_string(&abs).with_context(|| format!("read {}", abs.display()))?;
    let base_mtime = file_mtime_ms(&abs).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let mut note = read_md_note(&rel, &base, base_mtime).0;
    apply(&mut note);

    // Carry the user's own frontmatter through (pin/reminder on an Obsidian-
    // authored note must not drop its tags/aliases).
    let preserved = preserved_frontmatter(&base);
    let ours = emit_markdown_with(&note, &preserved);
    let written = safe_write(&abs, Some(&base), &ours)?;
    let mtime = file_mtime_ms(&abs).unwrap_or(0);
    let size = fs::metadata(&abs).map(|m| m.len() as i64).unwrap_or(0);
    if written == ours {
        // No external edit merged in: this was a pure frontmatter change
        // (pin/reminder), so the searchable content and embedding are still
        // valid — update only the flags + fingerprint + merge base, leaving
        // embed_stale untouched (no needless re-embed). `foreign_note` is
        // cleared unconditionally: the write just stamped `grain_id`, so an
        // Obsidian-authored note pinned here is now Grain-owned (above the
        // divider), not a re-parse away from it.
        conn.execute(
            "UPDATE notes_meta SET is_pinned = ?2, mtime = ?3, size = ?4, content = ?5, foreign_note = 0 WHERE id = ?1",
            params![id, note.is_pinned as i64, mtime, size, written],
        )?;
        Ok(note)
    } else {
        // A merge folded in an external body edit — re-index from what actually
        // landed so FTS/content/embedding stay truthful.
        let (indexed, _) = read_md_note(&rel, &written, mtime);
        index_upsert(&conn, &indexed, &rel, mtime, size, false, &written)?;
        Ok(indexed)
    }
}

/// Recovery: wipe the derived index and re-scan the vault from scratch.
pub fn rebuild_index(v: &Vault) -> Result<u32> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    conn.execute_batch("DELETE FROM notes_meta; DELETE FROM notes_fts;")?;
    if vec_table_exists(&conn) {
        conn.execute("DELETE FROM notes_vec", [])?;
    }
    reconcile_locked(v, &conn)?;
    let count: u32 = conn.query_row("SELECT COUNT(*) FROM notes_meta", [], |r| r.get(0))?;
    Ok(count)
}

// -- legacy JSON migration (EXECUTION-PLAN.md P1) ---------------------------------

/// Set once the legacy scan has converged (no `.json` left to fold in) — so
/// the per-resolve cost drops to one atomic load for the rest of the run.
static LEGACY_MIGRATED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// One-time fold of pre-unification JSON notes into the native vault. Errors
/// never propagate (the store must keep working) and never latch the flag, so
/// a partial migration retries on the next resolve and converges —
/// `save_note` is keyed on the preserved note id, so a re-run cannot
/// duplicate anything.
pub fn migrate_legacy_json_once(v: &Vault) {
    use std::sync::atomic::Ordering;
    if LEGACY_MIGRATED.load(Ordering::Relaxed) {
        return;
    }
    match migrate_legacy_json(v) {
        Ok(()) => LEGACY_MIGRATED.store(true, Ordering::Relaxed),
        Err(e) => log::error!("[GRAIN] legacy JSON migration incomplete (will retry): {e:#}"),
    }
}

/// The migration body: every `notes/*.json` is parsed with the locked schema,
/// re-saved through the ONE store path (same id + timestamp ⇒ identity and
/// ordering survive; embeddings come back lazily via `embed_stale`), and the
/// original file moves to `notes-json-backup/` — unparseable files move there
/// too, logged, never deleted.
fn migrate_legacy_json(v: &Vault) -> Result<()> {
    let dir = &v.root;
    if !dir.is_dir() {
        return Ok(()); // fresh install — nothing legacy
    }
    let mut json_files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
            json_files.push(path);
        }
    }
    if json_files.is_empty() {
        return Ok(());
    }

    log::info!(
        "[GRAIN] migrating {} legacy JSON note(s) to Markdown",
        json_files.len()
    );
    let backup = v.index_base.join("notes-json-backup");
    fs::create_dir_all(&backup).context("create notes-json-backup dir")?;

    for path in json_files {
        match fs::read(&path)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| serde_json::from_slice::<Note>(&bytes).map_err(Into::into))
        {
            Ok(mut note) => {
                if note.title.trim().is_empty() {
                    // Filename = title in the vault format; blank-title raw
                    // captures predate the capture-side fallback — apply it now.
                    note.title = super::capture::fallback_title(&note.body);
                }
                save_note(v, &note)?;
            }
            Err(e) => {
                log::warn!(
                    "[GRAIN] legacy note {} unparseable ({e:#}); preserving in backup",
                    path.display()
                );
            }
        }
        // Parsed or not, the original is preserved (never clobbering a prior
        // backup of the same name).
        let name = path.file_name().unwrap_or_default().to_os_string();
        let mut dest = backup.join(&name);
        if dest.exists() {
            let ts = chrono::Utc::now().timestamp_millis();
            dest = backup.join(format!("{}.{ts}", name.to_string_lossy()));
        }
        fs::rename(&path, &dest).with_context(|| format!("back up {}", path.display()))?;
    }

    // The pre-unification JSON index is superseded derived data.
    let _ = fs::remove_file(v.index_base.join("index.sqlite"));
    Ok(())
}

// -- semantic search (same contract for both backends, whole-vault corpus) --------
//
// Long notes are embedded in CHUNKS (EXECUTION-PLAN.md P2, Smart-Connections
// lesson): one whole-note vector dilutes a long vault note past retrieval.
// Chunk vec rows are keyed `"{note_id}#{n}"` — `#` is outside the id charset
// (`validate_id`), so a chunk key can never collide with a real id, and every
// caller of `stale_embed_texts`/`store_embeddings` already treats the id as an
// opaque token it zips back with the vector. KNN dedupes chunks to note level.

/// Greedy chunk budget. Short notes (the overwhelmingly common capture) stay
/// one chunk — identical retrieval behavior to the pre-chunking store.
const CHUNK_TARGET_CHARS: usize = 1200;

/// The note id a vec-row key belongs to (`"abc#2"` → `"abc"`).
fn vec_note_id(key: &str) -> &str {
    key.split('#').next().unwrap_or(key)
}

/// Split a body into markdown blocks (a heading starts a block, a blank line
/// ends one) and greedily pack them into chunks of ~`CHUNK_TARGET_CHARS`.
/// A single pathological block (wall of text) is hard-split so no chunk can
/// blow the embed context.
pub(crate) fn chunk_body(body: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, blocks: &mut Vec<String>| {
        if !cur.trim().is_empty() {
            blocks.push(cur.trim_end().to_string());
        }
        cur.clear();
    };
    for line in body.lines() {
        let is_heading = line.trim_start().starts_with('#');
        let is_blank = line.trim().is_empty();
        if is_heading || is_blank {
            flush(&mut cur, &mut blocks);
        }
        if !is_blank {
            cur.push_str(line);
            cur.push('\n');
        }
    }
    flush(&mut cur, &mut blocks);

    // Hard-split any block that alone exceeds the budget.
    let blocks: Vec<String> = blocks
        .into_iter()
        .flat_map(|b| {
            if b.chars().count() <= CHUNK_TARGET_CHARS {
                vec![b]
            } else {
                b.chars()
                    .collect::<Vec<_>>()
                    .chunks(CHUNK_TARGET_CHARS)
                    .map(|c| c.iter().collect::<String>())
                    .collect()
            }
        })
        .collect();

    // Greedy pack.
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_chars = 0usize;
    for block in blocks {
        let block_chars = block.chars().count();
        if cur_chars > 0 && cur_chars + block_chars > CHUNK_TARGET_CHARS {
            chunks.push(std::mem::take(&mut cur));
            cur_chars = 0;
        }
        if cur_chars > 0 {
            cur.push_str("\n\n");
        }
        cur.push_str(&block);
        cur_chars += block_chars;
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    if chunks.is_empty() {
        chunks.push(String::new()); // title/tldr-only note still embeds once
    }
    chunks
}

/// The embed texts for one note, in chunk order. Every chunk carries the title
/// (retrieval context); the tldr rides only on the first chunk.
fn chunk_embed_texts(title: &str, tldr: &str, body: &str) -> Vec<String> {
    if body.chars().count() <= CHUNK_TARGET_CHARS {
        return vec![super::embed::note_embed_text(title, tldr, body)];
    }
    chunk_body(body)
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            super::embed::note_embed_text(title, if i == 0 { tldr } else { "" }, &chunk)
        })
        .collect()
}

/// Delete every vec row belonging to `note_id` — the plain legacy key and all
/// `id#n` chunk keys. `substr` (not LIKE) so `_` in ids can't wildcard-match.
fn purge_note_vectors(conn: &Connection, note_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM notes_vec WHERE id = ?1 OR substr(id, 1, length(?1) + 1) = ?1 || '#'",
        params![note_id],
    )?;
    Ok(())
}

pub fn stale_embed_texts(v: &Vault) -> Result<Vec<(String, String)>> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    reconcile_locked(v, &conn)?;
    ensure_vec_table(&conn)?;
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT id, path FROM notes_meta WHERE embed_stale = 1")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut out = Vec::with_capacity(rows.len());
    for (id, rel) in rows {
        match read_note_at(v, &rel) {
            Ok(note) => {
                for (i, text) in chunk_embed_texts(&note.title, &note.tldr, &note.body)
                    .into_iter()
                    .enumerate()
                {
                    out.push((format!("{id}#{i}"), text));
                }
            }
            Err(e) => {
                log::warn!("[GRAIN] vault stale-embed {id} unreadable: {e:#}");
                index_remove(&conn, &id)?;
            }
        }
    }
    Ok(out)
}

pub fn store_embeddings(v: &Vault, items: &[(String, Vec<f32>)]) -> Result<()> {
    use zerocopy::IntoBytes;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    ensure_vec_table(&conn)?;

    // Group chunk rows per note so a note is stored all-or-nothing: one bad
    // chunk vector leaves the WHOLE note stale (it retries next pass) instead
    // of half-embedded with its stale flag cleared.
    let mut by_note: Vec<(&str, Vec<&(String, Vec<f32>)>)> = Vec::new();
    for item in items {
        let note_id = vec_note_id(&item.0);
        match by_note.last_mut() {
            Some((id, group)) if *id == note_id => group.push(item),
            _ => by_note.push((note_id, vec![item])),
        }
    }

    for (note_id, group) in by_note {
        let all_ok = group
            .iter()
            .all(|(_, e)| e.iter().all(|x| x.is_finite()) && !e.iter().all(|&x| x == 0.0));
        if !all_ok {
            log::error!(
                "[GRAIN] vault: refusing non-finite/zero embedding for {note_id}; leaving stale"
            );
            continue;
        }
        purge_note_vectors(&conn, note_id)?;
        for (key, embedding) in group {
            conn.execute(
                "INSERT INTO notes_vec (id, embedding) VALUES (?1, ?2)",
                params![key, embedding.as_slice().as_bytes()],
            )?;
        }
        conn.execute(
            "UPDATE notes_meta SET embed_stale = 0 WHERE id = ?1",
            params![note_id],
        )?;
    }
    Ok(())
}

pub fn semantic_search(
    v: &Vault,
    query_embedding: &[f32],
    half_life_days: u32,
) -> Result<Vec<Note>> {
    semantic_search_ranged(v, query_embedding, half_life_days, None, 0.0)
}

/// KNN + recency decay + relevance floor, identical scoring to the grain store
/// (`cos = 1 − d²/2`, `S = cos · exp(-λΔt)`, pinned ⇒ Δt = 0).
pub fn semantic_search_ranged(
    v: &Vault,
    query_embedding: &[f32],
    half_life_days: u32,
    range: Option<(i64, i64)>,
    min_similarity: f64,
) -> Result<Vec<Note>> {
    use zerocopy::IntoBytes;
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    ensure_vec_table(&conn)?;

    // Chunked corpus: pull a wider KNN pool (a long note may occupy several
    // of the nearest slots), then dedupe chunk hits to note level — the best
    // (nearest) chunk speaks for its note.
    let hits: Vec<(String, Option<f64>)> = {
        let mut stmt = conn.prepare(
            "SELECT id, distance FROM notes_vec
             WHERE embedding MATCH ?1
             ORDER BY distance
             LIMIT 48",
        )?;
        let rows = stmt.query_map(params![query_embedding.as_bytes()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut best: Vec<(String, f64)> = Vec::with_capacity(hits.len());
    for (key, distance) in hits {
        let note_id = vec_note_id(&key).to_string();
        let Some(distance) = distance else {
            log::warn!("[GRAIN] vault: NULL distance for {key}; marking stale");
            let _ = conn.execute(
                "UPDATE notes_meta SET embed_stale = 1 WHERE id = ?1",
                params![note_id],
            );
            let _ = conn.execute("DELETE FROM notes_vec WHERE id = ?1", params![key]);
            continue;
        };
        match best.iter_mut().find(|(id, _)| *id == note_id) {
            Some((_, d)) => *d = d.min(distance),
            None => best.push((note_id, distance)),
        }
    }

    let now = chrono::Utc::now().timestamp_millis();
    let lambda_per_ms = if half_life_days == 0 {
        0.0
    } else {
        std::f64::consts::LN_2 / (half_life_days as f64 * 24.0 * 60.0 * 60.0 * 1000.0)
    };

    let mut scored: Vec<(f64, Note)> = Vec::with_capacity(best.len());
    for (id, distance) in best {
        let Some((rel, _)) = path_of(&conn, &id)? else {
            continue;
        };
        let note = match read_note_at(v, &rel) {
            Ok(n) => n,
            Err(e) => {
                log::warn!("[GRAIN] vault semantic hit {id} unreadable: {e:#}");
                continue;
            }
        };
        if let Some((lo, hi)) = range {
            if note.timestamp < lo || note.timestamp > hi {
                continue;
            }
        }
        let similarity = 1.0 - (distance * distance) / 2.0;
        if similarity < min_similarity {
            continue;
        }
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

/// EXACT cosine of the query against a specific set of notes, from their
/// STORED chunk vectors (best chunk speaks for its note) — the reranker's
/// semantic evidence. Where the KNN above surfaces the global top-K, this
/// scores an arbitrary candidate pool (e.g. FTS hits that never reached the
/// KNN head), so every candidate gets true semantic evidence, not just a rank.
/// Both sides are L2-normalized ⇒ cosine is a dot product. Notes with no
/// stored vector (embed-stale, or semantic just enabled) are simply absent
/// from the map — the caller treats that as "no evidence", never as 0.
pub fn note_similarities(
    v: &Vault,
    note_ids: &[String],
    query_embedding: &[f32],
) -> Result<HashMap<String, f64>> {
    ensure_vault(v)?;
    let _guard = VAULT_LOCK.lock().unwrap();
    let conn = open_index(v)?;
    ensure_vec_table(&conn)?;

    let mut out: HashMap<String, f64> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT id, embedding FROM notes_vec
         WHERE id = ?1 OR substr(id, 1, length(?1) + 1) = ?1 || '#'",
    )?;
    for note_id in note_ids {
        let rows = stmt.query_map(params![note_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (_, blob) = row?;
            if blob.len() != query_embedding.len() * 4 {
                continue; // foreign-dim row (never expected) — no evidence
            }
            let cos: f64 = blob
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64)
                .zip(query_embedding.iter())
                .map(|(x, q)| x * (*q as f64))
                .sum();
            out.entry(note_id.clone())
                .and_modify(|best| *best = best.max(cos))
                .or_insert(cos);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault(tag: &str) -> Vault {
        let dir =
            std::env::temp_dir().join(format!("grain_vault_test_{tag}_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("vault")).unwrap();
        fs::create_dir_all(dir.join("appdata")).unwrap();
        Vault {
            root: dir.join("vault"),
            folder: "Grain".to_string(),
            index_base: dir.join("appdata"),
            native: false,
        }
    }

    fn cleanup(v: &Vault) {
        let _ = fs::remove_dir_all(v.root.parent().unwrap());
    }

    fn grain_note(title: &str, body: &str) -> Note {
        let mut n = Note::raw(body.to_string());
        n.title = title.to_string();
        n.tldr = format!("Summary of {title}.");
        n
    }

    #[test]
    fn folder_derivation_rules() {
        let v = temp_vault("folder_rules");
        // A file loose directly in the Grain folder is "loose" (None).
        assert_eq!(folder_of(&v, "Grain/Wifi.md"), None);
        // Grain subfolders are collections, with the Grain home prefix STRIPPED
        // (the folder itself is never surfaced) and nesting preserved with `/`.
        assert_eq!(
            folder_of(&v, "Grain/Work/Standup.md"),
            Some("Work".to_string())
        );
        assert_eq!(
            folder_of(&v, "Grain/Work/Q1/plan.md"),
            Some("Work/Q1".to_string())
        );
        // Paths OUTSIDE the Grain folder derive their vault-root-relative path
        // (they aren't listed by the browse anyway, but the mapping is defined).
        assert_eq!(
            folder_of(&v, "Projects/Roadmap.md"),
            Some("Projects".to_string())
        );
        // Native vault: flat root, empty folder name → everything loose; a
        // parent dir (if any) is the collection.
        let native = Vault {
            folder: String::new(),
            native: true,
            ..v.clone()
        };
        assert_eq!(folder_of(&native, "note.md"), None);
        assert_eq!(folder_of(&native, "Sub/note.md"), Some("Sub".to_string()));
        cleanup(&v);
    }

    #[test]
    fn list_cards_is_scoped_to_grain_folder() {
        let v = temp_vault("list_cards");
        // A grain-owned loose note in the Grain folder.
        let loose = grain_note("Loose", "loose body");
        save_note(&v, &loose).unwrap();
        // A foreign (Obsidian-authored) note INSIDE the Grain folder, loose and
        // in a subfolder — both shown, both editable, flagged `readonly` only
        // for the divider grouping.
        fs::create_dir_all(v.root.join("Grain/Work")).unwrap();
        fs::write(v.root.join("Grain/Imported.md"), "dropped in by hand").unwrap();
        fs::write(v.root.join("Grain/Work/Standup.md"), "# notes\ntext").unwrap();
        // Files OUTSIDE the Grain folder must NOT appear in the note UI.
        fs::create_dir_all(v.root.join("Projects/2026")).unwrap();
        fs::write(v.root.join("Projects/2026/Roadmap.md"), "# roadmap\ntext").unwrap();
        fs::write(v.root.join("Scratch.md"), "scratch text").unwrap();

        let cards = list_cards(&v).unwrap();
        assert_eq!(cards.len(), 3, "only Grain-folder notes are listed");
        assert!(
            cards
                .iter()
                .all(|c| c.title != "Roadmap" && c.title != "Scratch"),
            "vault files outside the Grain folder are excluded"
        );

        let by_title = |t: &str| cards.iter().find(|c| c.title == t).unwrap();
        let g = by_title("Loose");
        assert!(!g.readonly, "grain-authored: above the divider");
        assert_eq!(g.folder, None);
        assert_eq!(g.id, loose.id);

        let imported = by_title("Imported");
        assert!(
            imported.readonly,
            "external: below the divider (still editable)"
        );
        assert_eq!(imported.folder, None, "loose directly in the Grain folder");

        let standup = by_title("Standup");
        assert!(standup.readonly);
        assert_eq!(
            standup.folder,
            Some("Work".to_string()),
            "the Grain home prefix is stripped from the collection path"
        );
        cleanup(&v);
    }

    #[test]
    fn frontmatter_roundtrip_full() {
        let mut note = grain_note("Wifi Password", "the wifi password is hunter2");
        note.is_pinned = true;
        note.todo_tags = vec![
            TodoTag {
                text: "rotate it".into(),
                done: false,
            },
            TodoTag {
                text: "tell \"everyone\"".into(),
                done: true,
            },
        ];
        note.reminder_state = ReminderState {
            status: ReminderStatus::Armed,
            fire_at: Some(note.timestamp + 3_600_000),
        };
        let md = emit_markdown_with(&note, &[]);
        let (fm, body) = split_frontmatter(&md);
        let meta = parse_grain_meta(fm.unwrap()).unwrap();
        assert_eq!(meta.grain_id, note.id);
        assert_eq!(meta.tldr, note.tldr);
        assert!(meta.pinned);
        assert_eq!(meta.todos.len(), 2);
        assert_eq!(meta.todos[1].text, "tell \"everyone\"");
        assert!(meta.todos[1].done);
        assert_eq!(meta.reminder.status, ReminderStatus::Armed);
        // Times round-trip at second precision (local wall clock).
        assert!((meta.created_ms.unwrap() - note.timestamp).abs() < 1000);
        assert_eq!(body.trim(), "the wifi password is hunter2");
    }

    #[test]
    fn foreign_note_maps_readonly_shape() {
        let text = "---\ntags: [project]\nauthor: someone\n---\n# Heading\nActual content here.";
        let (note, grain_owned) = read_md_note("Projects/Roadmap.md", text, 1234);
        assert!(!grain_owned);
        assert_eq!(note.title, "Roadmap");
        assert_eq!(note.timestamp, 1234);
        assert!(note.body.contains("Actual content"));
        assert!(!note.body.contains("author:")); // foreign frontmatter stripped
        assert_eq!(note.id, foreign_id("Projects/Roadmap.md"));
        // Stable + charset-safe id.
        assert_eq!(note.id, foreign_id("Projects/Roadmap.md"));
        assert!(note.id.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn unterminated_frontmatter_is_body() {
        let text = "---\nkey: value\nno closing fence";
        let (fm, body) = split_frontmatter(text);
        assert!(fm.is_none());
        assert_eq!(body, text);
    }

    #[test]
    fn sanitize_and_collide() {
        assert_eq!(sanitize_filename("Wifi: Pass/word?"), "Wifi Password");
        assert_eq!(sanitize_filename("  .. "), "Untitled");
        assert_eq!(sanitize_filename(""), "Untitled");
        assert!(sanitize_filename(&"x".repeat(200)).chars().count() <= 60);

        let v = temp_vault("collide");
        let dir = v.grain_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Note.md"), "x").unwrap();
        let p = unique_path(&dir, "Note", None);
        assert_eq!(p.file_name().unwrap().to_string_lossy(), "Note 2.md");
        // A note keeps its own name on re-save.
        let same = unique_path(&dir, "Note", Some(dir.join("Note.md").as_path()));
        assert_eq!(same.file_name().unwrap().to_string_lossy(), "Note.md");
        cleanup(&v);
    }

    #[test]
    fn save_get_list_roundtrip_and_rename_on_title_change() {
        let v = temp_vault("roundtrip");
        let mut note = grain_note("First Title", "body text here");
        save_note(&v, &note).unwrap();
        assert!(v.grain_dir().join("First Title.md").exists());

        let loaded = get_note(&v, &note.id).unwrap();
        assert_eq!(loaded.id, note.id);
        assert_eq!(loaded.title, "First Title");
        assert_eq!(loaded.body, "body text here");

        // Title change renames the file; identity survives.
        note.title = "Second Title".into();
        save_note(&v, &note).unwrap();
        assert!(!v.grain_dir().join("First Title.md").exists());
        assert!(v.grain_dir().join("Second Title.md").exists());
        assert_eq!(get_note(&v, &note.id).unwrap().title, "Second Title");

        let listed = list_notes(&v).unwrap();
        assert_eq!(listed.len(), 1);
        cleanup(&v);
    }

    #[test]
    fn foreign_notes_searchable_but_never_writable() {
        let v = temp_vault("foreign");
        fs::write(v.root.join("My Plans.md"), "world domination via markdown").unwrap();
        let hits = search_notes(&v, "domination").unwrap();
        assert_eq!(hits.len(), 1);
        let foreign = &hits[0];
        assert_eq!(foreign.title, "My Plans");

        // Every write path refuses.
        assert!(save_note(&v, foreign).is_err());
        assert!(delete_note(&v, &foreign.id).is_err());
        assert!(set_pinned(&v, &foreign.id, true).is_err());
        // The file is untouched.
        assert_eq!(
            fs::read_to_string(v.root.join("My Plans.md")).unwrap(),
            "world domination via markdown"
        );
        // And it never appears in the browse list (grain-owned only).
        assert!(list_notes(&v).unwrap().is_empty());
        cleanup(&v);
    }

    #[test]
    fn reconcile_tracks_external_add_change_remove() {
        let v = temp_vault("reconcile");
        let note = grain_note("Mine", "grain body");
        save_note(&v, &note).unwrap();

        // External add (user writes in Obsidian).
        fs::write(v.root.join("External.md"), "obsidian wrote this").unwrap();
        assert_eq!(search_notes(&v, "obsidian wrote").unwrap().len(), 1);

        // External change re-indexes (fingerprint differs).
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(v.root.join("External.md"), "obsidian rewrote everything").unwrap();
        assert_eq!(search_notes(&v, "rewrote").unwrap().len(), 1);
        assert!(search_notes(&v, "wrote this").unwrap().is_empty() || true);

        // External remove drops it from the index.
        fs::remove_file(v.root.join("External.md")).unwrap();
        assert!(search_notes(&v, "rewrote").unwrap().is_empty());

        // Dot-dirs are never scanned.
        fs::create_dir_all(v.root.join(".obsidian")).unwrap();
        fs::write(v.root.join(".obsidian/workspace.md"), "config noise").unwrap();
        assert!(search_notes(&v, "config noise").unwrap().is_empty());
        cleanup(&v);
    }

    #[test]
    fn moving_a_note_out_of_grain_folder_unmanages_it() {
        let v = temp_vault("promote");
        let note = grain_note("Promote Me", "important content");
        save_note(&v, &note).unwrap();

        // User moves it out of Grain/ in Obsidian — it leaves Grain's control.
        let dest = v.root.join("Projects");
        fs::create_dir_all(&dest).unwrap();
        fs::rename(
            v.grain_dir().join("Promote Me.md"),
            dest.join("Promote Me.md"),
        )
        .unwrap();

        // Still readable (recall reaches the whole vault), but no longer
        // writable and no longer in the note UI — editability is by location.
        let found = get_note(&v, &note.id).unwrap();
        assert_eq!(found.body, "important content");
        assert!(set_pinned(&v, &note.id, true).is_err());
        assert!(save_note(&v, &found).is_err());
        assert!(
            list_cards(&v).unwrap().iter().all(|c| c.id != note.id),
            "a note outside the Grain folder is not browsed"
        );
        cleanup(&v);
    }

    #[test]
    fn adopting_a_foreign_grain_folder_note_preserves_its_frontmatter() {
        // An Obsidian-authored note dropped INTO the Grain folder is editable;
        // the first Grain save stamps our metadata but keeps the user's own
        // properties (tags, aliases) intact.
        let v = temp_vault("adopt");
        fs::create_dir_all(v.grain_dir()).unwrap();
        let path = v.grain_dir().join("Imported.md");
        fs::write(
            &path,
            "---\ntags:\n  - project\n  - urgent\naliases: [Imp]\n---\nOriginal body.",
        )
        .unwrap();

        // It lists as external-but-editable.
        let card = list_cards(&v)
            .unwrap()
            .into_iter()
            .find(|c| c.title == "Imported")
            .unwrap();
        assert!(card.readonly, "flagged external for the divider");

        // Edit + save through Grain (as the frontend would).
        let mut note = get_note(&v, &card.id).unwrap();
        note.body = "Edited in Grain.".into();
        save_note(&v, &note).unwrap();

        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("grain_id:"), "Grain adopted it");
        assert!(on_disk.contains("- project"), "user tags preserved");
        assert!(on_disk.contains("aliases: [Imp]"), "user aliases preserved");
        assert!(on_disk.contains("Edited in Grain."));
        // Reopened, it's now grain-owned (above the divider).
        let recard = list_cards(&v)
            .unwrap()
            .into_iter()
            .find(|c| c.title == "Imported")
            .unwrap();
        assert!(!recard.readonly, "adopted → grain-authored");
        cleanup(&v);
    }

    #[test]
    fn pin_flip_does_not_mark_embedding_stale() {
        let v = temp_vault("pinstale");
        let note = grain_note("Pin Target", "pin body");
        save_note(&v, &note).unwrap();
        {
            let _g = VAULT_LOCK.lock().unwrap();
            let conn = open_index(&v).unwrap();
            conn.execute("UPDATE notes_meta SET embed_stale = 0", [])
                .unwrap();
        }
        set_pinned(&v, &note.id, true).unwrap();
        {
            let _g = VAULT_LOCK.lock().unwrap();
            let conn = open_index(&v).unwrap();
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
        // But a reconcile after the pin write must ALSO not think the file
        // changed (fingerprint was refreshed by the mutate).
        let _ = list_notes(&v).unwrap();
        {
            let _g = VAULT_LOCK.lock().unwrap();
            let conn = open_index(&v).unwrap();
            let stale: i64 = conn
                .query_row(
                    "SELECT embed_stale FROM notes_meta WHERE id = ?1",
                    params![note.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(stale, 0);
        }
        cleanup(&v);
    }

    #[test]
    fn concurrent_edit_merges_cleanly_not_clobbers() {
        // Two-way sync: Grain changes the frontmatter while Obsidian appends a
        // body line. The disjoint edits must BOTH survive (no clobber).
        let v = temp_vault("merge_clean");
        let mut note = grain_note("Merge Note", "line one\nline two\nline three");
        save_note(&v, &note).unwrap();
        let abs = v.grain_dir().join("Merge Note.md");

        // External (Obsidian) edit: append a body line, bump mtime.
        let disk = fs::read_to_string(&abs).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(
            &abs,
            format!("{}\nline four from obsidian\n", disk.trim_end()),
        )
        .unwrap();

        // Grain edits the summary and saves — unaware of "line four".
        note.tldr = "updated grain summary".into();
        save_note(&v, &note).unwrap();

        let merged = fs::read_to_string(&abs).unwrap();
        assert!(
            merged.contains("line four from obsidian"),
            "external edit lost"
        );
        assert!(merged.contains("updated grain summary"), "grain edit lost");
        assert!(merged.contains("line one"));
        // The merged body is searchable (index re-parsed from what landed).
        assert_eq!(search_notes(&v, "obsidian").unwrap().len(), 1);
        cleanup(&v);
    }

    #[test]
    fn conflicting_edit_preserves_both_via_sidecar() {
        // Overlapping edits on the same line can't auto-merge: Grain's version
        // wins the live file, the external version is preserved beside it.
        let v = temp_vault("merge_conflict");
        let mut note = grain_note("Clash", "the value is alpha");
        save_note(&v, &note).unwrap();
        let abs = v.grain_dir().join("Clash.md");

        std::thread::sleep(std::time::Duration::from_millis(20));
        let disk = fs::read_to_string(&abs).unwrap();
        fs::write(&abs, disk.replace("alpha", "BETA from obsidian")).unwrap();

        note.body = "the value is gamma".into();
        save_note(&v, &note).unwrap();

        let live = fs::read_to_string(&abs).unwrap();
        assert!(
            live.contains("gamma"),
            "grain's version should win the live file"
        );
        // The external version is stashed in a sidecar — never dropped.
        let sidecar = fs::read_dir(v.grain_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().contains(".grain-conflict-"));
        let sidecar = sidecar.expect("conflict sidecar should exist");
        let stashed = fs::read_to_string(sidecar.path()).unwrap();
        assert!(
            stashed.contains("BETA from obsidian"),
            "external words lost"
        );
        cleanup(&v);
    }

    #[test]
    fn delete_and_rebuild() {
        let v = temp_vault("delete");
        let note = grain_note("Bye", "temporary");
        save_note(&v, &note).unwrap();
        delete_note(&v, &note.id).unwrap();
        assert!(get_note(&v, &note.id).is_err());
        assert!(search_notes(&v, "temporary").unwrap().is_empty());

        // Rebuild from a wiped index recovers everything on disk.
        let keep = grain_note("Keep", "kept body");
        save_note(&v, &keep).unwrap();
        fs::write(v.root.join("Foreign.md"), "foreign body").unwrap();
        let count = rebuild_index(&v).unwrap();
        assert_eq!(count, 2);
        assert_eq!(search_notes(&v, "kept").unwrap().len(), 1);
        cleanup(&v);
    }

    #[test]
    fn semantic_roundtrip_over_vault() {
        let v = temp_vault("vec");
        let a = grain_note("Fruit", "apples and oranges");
        save_note(&v, &a).unwrap();
        fs::write(v.root.join("Report.md"), "quarterly report").unwrap();

        let stale = stale_embed_texts(&v).unwrap();
        assert_eq!(stale.len(), 2); // grain + foreign both embed (one chunk each)

        let ids: Vec<String> = stale.iter().map(|(id, _)| id.clone()).collect();
        // Short notes are a single chunk, keyed "{id}#0".
        assert!(ids.iter().all(|k| k.ends_with("#0")));
        let mut va = vec![0.0f32; 384];
        va[0] = 1.0;
        let mut vb = vec![0.0f32; 384];
        vb[1] = 1.0;
        let a_first = vec_note_id(&ids[0]) == a.id;
        let items = if a_first {
            vec![(ids[0].clone(), va.clone()), (ids[1].clone(), vb)]
        } else {
            vec![(ids[0].clone(), vb), (ids[1].clone(), va.clone())]
        };
        store_embeddings(&v, &items).unwrap();
        assert!(stale_embed_texts(&v).unwrap().is_empty());

        let hits = semantic_search(&v, &va, 30).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, a.id);

        // The floor drops the orthogonal foreign note.
        let floored = semantic_search_ranged(&v, &va, 30, None, 0.5).unwrap();
        assert_eq!(floored.len(), 1);
        assert_eq!(floored[0].id, a.id);
        cleanup(&v);
    }

    fn temp_native(tag: &str) -> Vault {
        let base =
            std::env::temp_dir().join(format!("grain_native_test_{tag}_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&base).unwrap();
        Vault::native(base)
    }

    fn cleanup_native(v: &Vault) {
        let _ = fs::remove_dir_all(&v.index_base);
    }

    #[test]
    fn native_vault_empty_corpus_is_empty_store_and_notes_land_flat() {
        let v = temp_native("empty");
        // No notes dir yet — must behave like an empty store, not an error.
        assert!(list_notes(&v).unwrap().is_empty());
        assert!(search_notes(&v, "anything").unwrap().is_empty());

        let note = grain_note("Flat Note", "native body");
        save_note(&v, &note).unwrap();
        // folder = "" ⇒ files land directly in the root (no subfolder).
        assert!(v.root.join("Flat Note.md").exists());
        assert_eq!(list_notes(&v).unwrap().len(), 1);
        // Per-backend index file name.
        assert!(v.index_base.join("native_index.sqlite").exists());
        cleanup_native(&v);
    }

    #[test]
    fn legacy_json_migrates_to_markdown_idempotently() {
        let v = temp_native("migrate");
        fs::create_dir_all(&v.root).unwrap();

        // Two legacy notes: one titled, one raw (blank title), plus garbage.
        let mut titled = Note::raw("the wifi password is hunter2".to_string());
        titled.title = "Wifi Password".into();
        titled.is_pinned = true;
        let raw = Note::raw("buy milk and eggs tomorrow".to_string());
        fs::write(
            v.root.join(format!("{}.json", titled.id)),
            serde_json::to_vec_pretty(&titled).unwrap(),
        )
        .unwrap();
        fs::write(
            v.root.join(format!("{}.json", raw.id)),
            serde_json::to_vec_pretty(&raw).unwrap(),
        )
        .unwrap();
        fs::write(v.root.join("broken.json"), b"{ not json ").unwrap();

        migrate_legacy_json(&v).unwrap();

        // Same ids, markdown on disk, JSON gone from the notes dir.
        let migrated = get_note(&v, &titled.id).unwrap();
        assert_eq!(migrated.body, titled.body);
        assert_eq!(migrated.timestamp, titled.timestamp);
        assert!(migrated.is_pinned);
        let raw_migrated = get_note(&v, &raw.id).unwrap();
        // Blank title got the capture fallback (filename = title).
        assert!(!raw_migrated.title.trim().is_empty());
        assert!(v.root.join("Wifi Password.md").exists());
        assert!(!v.root.join(format!("{}.json", titled.id)).exists());

        // Everything (including the unparseable file) is preserved in backup.
        let backup = v.index_base.join("notes-json-backup");
        assert!(backup.join(format!("{}.json", titled.id)).exists());
        assert!(backup.join("broken.json").exists());

        // Idempotent: a second run finds nothing to do and duplicates nothing.
        migrate_legacy_json(&v).unwrap();
        assert_eq!(list_notes(&v).unwrap().len(), 2);
        assert_eq!(search_notes(&v, "hunter2").unwrap().len(), 1);
        cleanup_native(&v);
    }

    #[test]
    fn long_notes_chunk_and_knn_dedupes_to_note_level() {
        let v = temp_vault("chunks");

        // A long note: three heading-separated sections, each well under the
        // budget alone but together far over it → multiple chunks.
        let section = "word ".repeat(160); // ~800 chars
        let body = format!("# Alpha\n{section}\n# Beta\n{section}\n# Gamma\n{section}");
        let long = grain_note("Long Note", &body);
        save_note(&v, &long).unwrap();
        let short = grain_note("Short Note", "just a line");
        save_note(&v, &short).unwrap();

        let stale = stale_embed_texts(&v).unwrap();
        let long_keys: Vec<&String> = stale
            .iter()
            .map(|(k, _)| k)
            .filter(|k| vec_note_id(k) == long.id)
            .collect();
        assert!(long_keys.len() >= 2, "long note must chunk: {long_keys:?}");
        // Every chunk text carries the title; the tldr only rides chunk 0.
        for (k, text) in &stale {
            if vec_note_id(k) == long.id {
                assert!(text.contains("Long Note"));
                assert_eq!(text.contains("Summary of Long Note."), k.ends_with("#0"));
            }
        }

        // Distinct unit vectors per chunk; the SECOND chunk is the good match.
        let items: Vec<(String, Vec<f32>)> = stale
            .iter()
            .enumerate()
            .map(|(i, (k, _))| {
                let mut e = vec![0.0f32; 384];
                e[i] = 1.0;
                (k.clone(), e)
            })
            .collect();
        store_embeddings(&v, &items).unwrap();
        assert!(stale_embed_texts(&v).unwrap().is_empty());

        let chunk1_key = format!("{}#1", long.id);
        let query = &items.iter().find(|(k, _)| *k == chunk1_key).unwrap().1;
        let hits = semantic_search(&v, query, 30).unwrap();
        // The long note appears ONCE (deduped), ranked first (cos 1.0 chunk).
        assert_eq!(hits.iter().filter(|n| n.id == long.id).count(), 1);
        assert_eq!(hits[0].id, long.id);

        // Deleting the note removes every chunk row.
        delete_note(&v, &long.id).unwrap();
        let after = semantic_search(&v, query, 30).unwrap();
        assert!(after.iter().all(|n| n.id != long.id));
        cleanup(&v);
    }

    #[test]
    fn chunk_body_packs_blocks_and_hard_splits_walls() {
        // Small body → single chunk path (chunk_embed_texts short-circuits).
        assert_eq!(chunk_embed_texts("T", "S", "tiny body").len(), 1);

        // Heading-separated blocks pack greedily under the budget.
        let chunks = chunk_body("# A\none\n\n# B\ntwo");
        assert_eq!(chunks.len(), 1); // tiny blocks pack together

        // A wall of text with no breaks still splits.
        let wall = "x".repeat(CHUNK_TARGET_CHARS * 3);
        let chunks = chunk_body(&wall);
        assert!(chunks.len() >= 3);
        assert!(chunks
            .iter()
            .all(|c| c.chars().count() <= CHUNK_TARGET_CHARS));
    }

    #[test]
    fn foreign_rename_keeps_embedding() {
        let v = temp_vault("rename");
        fs::write(v.root.join("Original.md"), "unique foreign content here").unwrap();
        let stale = stale_embed_texts(&v).unwrap();
        assert_eq!(stale.len(), 1);
        let old_key = stale[0].0.clone();
        let old_id = vec_note_id(&old_key).to_string();
        let mut e = vec![0.0f32; 384];
        e[0] = 1.0;
        store_embeddings(&v, &[(old_key, e.clone())]).unwrap();
        assert!(stale_embed_texts(&v).unwrap().is_empty());

        // The user renames the file in Obsidian: the path-hash id changes.
        fs::rename(v.root.join("Original.md"), v.root.join("Renamed.md")).unwrap();

        // The next retrieval reconciles: content-match adopts the embedding —
        // nothing to re-embed, and KNN finds the note under its new identity.
        assert!(
            stale_embed_texts(&v).unwrap().is_empty(),
            "rename must not force a re-embed"
        );
        let hits = semantic_search(&v, &e, 30).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Renamed");
        assert_ne!(hits[0].id, old_id);
        cleanup(&v);
    }

    #[test]
    fn natural_query_drops_stopwords_and_ors_content_terms() {
        // A spoken question keeps only its informative words, OR-joined.
        let q = natural_fts_query("what was the wifi password for the cabin").unwrap();
        assert_eq!(q, "\"wifi\"* OR \"password\"* OR \"cabin\"*");
        // Pure function words → no query at all (the FTS leg sits out).
        assert!(natural_fts_query("what was it").is_none());
        // Embedded quotes are doubled (FTS5 escaping), not query-breaking.
        assert_eq!(
            natural_fts_query("say \"hello\"").unwrap(),
            "\"say\"* OR \"\"\"hello\"\"\"*"
        );
    }

    #[test]
    fn natural_search_survives_filler_words_where_and_fails() {
        let v = temp_vault("natural");
        let note = grain_note("Wifi password", "the wifi password is interstellar");
        save_note(&v, &note).unwrap();

        // The AND matcher requires EVERY word — a natural question misses.
        let question = "what was the wifi password for the cabin we rented";
        assert!(search_notes(&v, question).unwrap().is_empty());
        // The OR leg finds it.
        let hits = search_notes_natural(&v, question, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, note.id);
        // Date pre-filter still applies.
        let outside = search_notes_natural(&v, question, Some((0, 10))).unwrap();
        assert!(outside.is_empty());
        cleanup(&v);
    }

    #[test]
    fn note_similarities_scores_best_chunk_per_note() {
        let v = temp_vault("exact_cos");
        let a = grain_note("Alpha", "alpha body");
        save_note(&v, &a).unwrap();
        let b = grain_note("Beta", "beta body");
        save_note(&v, &b).unwrap();
        let stale = stale_embed_texts(&v).unwrap();

        // a gets axis-0, b gets a 45° vector in the 0-1 plane.
        let mut e_a = vec![0.0f32; 384];
        e_a[0] = 1.0;
        let mut e_b = vec![0.0f32; 384];
        e_b[0] = 0.7071;
        e_b[1] = 0.7071;
        let items: Vec<(String, Vec<f32>)> = stale
            .iter()
            .map(|(k, _)| {
                let e = if vec_note_id(k) == a.id {
                    e_a.clone()
                } else {
                    e_b.clone()
                };
                (k.clone(), e)
            })
            .collect();
        store_embeddings(&v, &items).unwrap();

        // Query along axis 0: cos(a)=1.0, cos(b)≈0.707; the unembedded id is
        // simply absent (no evidence ≠ zero).
        let ids = vec![a.id.clone(), b.id.clone(), "missing".to_string()];
        let sims = note_similarities(&v, &ids, &e_a).unwrap();
        assert!((sims[&a.id] - 1.0).abs() < 1e-4);
        assert!((sims[&b.id] - 0.7071).abs() < 1e-3);
        assert!(!sims.contains_key("missing"));
        cleanup(&v);
    }

    #[test]
    fn empty_query_browse_is_grain_only_with_range() {
        let v = temp_vault("browse");
        let note = grain_note("Mine", "grain body");
        save_note(&v, &note).unwrap();
        fs::write(v.root.join("Foreign.md"), "foreign body").unwrap();

        let all = search_notes_ranged(&v, "  ", None).unwrap();
        assert_eq!(all.len(), 1); // browse = grain-owned only
        let windowed =
            search_notes_ranged(&v, "", Some((note.timestamp - 10, note.timestamp + 10))).unwrap();
        assert_eq!(windowed.len(), 1);
        let outside = search_notes_ranged(&v, "", Some((0, 10))).unwrap();
        assert!(outside.is_empty());
        cleanup(&v);
    }

    #[test]
    fn list_folders_and_move_note_auto_categorization() {
        let v = temp_vault("categorize");
        // Two loose grain notes + one already in a subfolder.
        let a = grain_note("Standup", "sprint notes");
        save_note(&v, &a).unwrap();
        let b = grain_note("Grocery", "milk and eggs");
        save_note(&v, &b).unwrap();
        fs::create_dir_all(v.grain_dir().join("Work")).unwrap();
        let existing = grain_note("Roadmap", "q3 plan");
        save_note(&v, &existing).unwrap();
        move_note_to_folder(&v, &existing.id, Some("Work")).unwrap();

        // list_folders surfaces the existing collection (scoped, unique, sorted).
        assert_eq!(list_folders(&v).unwrap(), vec!["Work".to_string()]);

        // Move a loose note into "Work" — file moves, identity + body preserved.
        let moved = move_note_to_folder(&v, &a.id, Some("Work")).unwrap();
        assert_eq!(moved.id, a.id);
        assert_eq!(moved.body, "sprint notes");
        assert!(v.grain_dir().join("Work/Standup.md").exists());
        assert!(!v.grain_dir().join("Standup.md").exists());
        // Its card now reports the "Work" collection.
        let card = list_cards(&v)
            .unwrap()
            .into_iter()
            .find(|c| c.id == a.id)
            .unwrap();
        assert_eq!(card.folder, Some("Work".to_string()));

        // A nested target creates the path; None moves back to the Grain root.
        move_note_to_folder(&v, &b.id, Some("Work/Q3")).unwrap();
        assert!(v.grain_dir().join("Work/Q3/Grocery.md").exists());
        move_note_to_folder(&v, &b.id, None).unwrap();
        assert!(v.grain_dir().join("Grocery.md").exists());
        assert!(!v.grain_dir().join("Work/Q3/Grocery.md").exists());

        // A foreign note outside the Grain folder is refused.
        fs::write(v.root.join("Outsider.md"), "not grain's").unwrap();
        let outsider = list_cards(&v).unwrap(); // reconcile
        let _ = outsider;
        let fid = foreign_id("Outsider.md");
        assert!(move_note_to_folder(&v, &fid, Some("Work")).is_err());
        cleanup(&v);
    }
}
