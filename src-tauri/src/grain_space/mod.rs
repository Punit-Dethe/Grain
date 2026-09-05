//! [GRAIN] Grain Space — local, zero-idle-RAM notes.
//!
//! Design contract (docs/Grain Space 2.0/: OBSIDIAN-PLAN.md + EXECUTION-PLAN.md):
//! - ONE store format everywhere: Markdown + YAML frontmatter (`vault.rs`).
//!   The native backend is a Grain-managed vault under
//!   `{app_data_dir}/grain_space/notes/`; the obsidian backend is a
//!   user-chosen vault. The per-backend SQLite index (FTS5 + sqlite-vec) is
//!   derived and rebuildable; embeddings NEVER live in the note files.
//! - No WAL: `journal_mode=TRUNCATE` + one application-wide `Mutex` serializes
//!   every store operation. Connections open per operation and drop — the
//!   feature holds zero resident memory while its surfaces are closed.
//! - `grain_space_enabled == false` ⇒ nothing initializes: shortcuts are
//!   skipped at registration (see `shortcut::tauri_impl` / `handy_keys`) and
//!   every command below early-returns. Disabling never deletes data files.

pub mod agent_tools;
pub mod backend;
pub mod capture;
pub mod commands;
pub mod embed;
pub mod eval;
pub mod graph;
pub mod note;
pub mod recall;
pub mod reminders;
pub mod temporal;
pub mod vault;

use tauri::{AppHandle, Manager};

/// Event emitted after any note mutation so open UI surfaces refresh.
pub const NOTES_CHANGED_EVENT: &str = "grain-space://notes-changed";

/// Event emitted when the active corpus is replaced wholesale (backend switch,
/// different vault, different store folder). See [`emit_corpus_changed`].
pub const CORPUS_CHANGED_EVENT: &str = "grain-space://corpus-changed";

/// Backend → main window: bring the Notes tab forward (payload: the note id, or
/// null for "just show me my notes"). Emitted by [`reveal_note`].
pub const REVEAL_EVENT: &str = "grain-space://reveal";

/// Emitted at an already-mounted Notes tab to make it select a note.
pub const FOCUS_NOTE_EVENT: &str = "grain-space://focus-note";

/// The note the Notes tab should open on mount, consumed once.
///
/// The workspace used to be its own window, and this rode along as that window's
/// surface payload. It is a tab now, with no window to carry it — but the handoff
/// still needs two halves, because the tab may or may not already be mounted when
/// something asks to reveal a note: [`reveal_note`] stashes the id here AND emits
/// [`FOCUS_NOTE_EVENT`]. A mounting tab takes the stash; a mounted one hears the
/// event. Exactly one of the two lands.
static FOCUS_NOTE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Show the main window with the Notes tab forward, optionally on a given note.
/// The one entry point for "open this note in the UI" — used by the Agent's
/// source chips and by the reminder rows in settings.
pub fn reveal_note(app: &AppHandle, note_id: Option<String>) {
    use tauri::Emitter;
    if !is_enabled(app) {
        return;
    }
    if let Ok(mut guard) = FOCUS_NOTE.lock() {
        *guard = note_id.clone();
    }
    crate::show_main_window(app);
    let _ = app.emit(REVEAL_EVENT, note_id.clone());
    if let Some(id) = note_id {
        let _ = app.emit(FOCUS_NOTE_EVENT, id);
    }
}

/// Take the pending focus note, if any. Consuming: a stale id must never make a
/// later mount jump to a note the user did not ask for.
pub fn take_focus_note() -> Option<String> {
    FOCUS_NOTE.lock().ok().and_then(|mut g| g.take())
}

/// Whether the Notes tab is currently mounted.
///
/// [GRAIN] This replaces "is the workspace window visible", which is what used to
/// bound the embedding engine's lifetime. The window is gone, and "is the main
/// window visible" is not the same question — the user can sit on the History tab
/// all day with Grain in the foreground, and the model must not stay resident for
/// that. The tab is the only thing that knows, so the tab says so: `NotesTab`
/// sets this on mount and clears it on unmount, which is the same unmount that
/// flushes pending saves.
static WORKSPACE_MOUNTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Frontend → backend: the Notes tab mounted (`true`) or unmounted (`false`).
pub fn set_workspace_mounted(app: &AppHandle, mounted: bool) {
    WORKSPACE_MOUNTED.store(mounted, std::sync::atomic::Ordering::SeqCst);
    if !mounted {
        // Leaving the tab is the moment to reclaim the model — unless the Agent
        // panel is still using it, which `_if_idle` is what checks.
        embed::shutdown_engine_if_idle(app);
    }
}

/// True while the Notes tab is on screen. See [`WORKSPACE_MOUNTED`].
pub fn workspace_mounted() -> bool {
    WORKSPACE_MOUNTED.load(std::sync::atomic::Ordering::SeqCst)
}

/// The feature's base directory: `{app_data_dir}/grain_space`. Nothing is
/// created by calling this — the store creates directories lazily on first write.
pub fn base_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    crate::portable::app_data_dir(app)
        .map(|d| d.join("grain_space"))
        .map_err(|e| format!("failed to resolve app data dir: {e}"))
}

/// Where the Grain store keeps its NOTES: the user's chosen folder, or the
/// app's own data folder when they have not chosen one.
///
/// Deliberately separate from [`base_dir`], which stays the home of the derived
/// index. The index is rebuildable and machine-local; putting it inside a folder
/// the user syncs would ship a SQLite file between machines to no benefit and
/// some risk.
pub fn store_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let chosen = crate::settings::get_settings(app).grain_space_store_path;
    let chosen = chosen.trim();
    if chosen.is_empty() {
        return base_dir(app);
    }
    let path = std::path::PathBuf::from(chosen);
    if !path.is_dir() {
        return Err(format!("Notes folder not found: {chosen}"));
    }
    Ok(path)
}

/// Master gate. Every Grain Space entry point checks this first.
///
/// `grain_space_enabled` remains the ONE runtime flag (the shortcut-registration
/// hooks in the Handy tree read it directly, and every command early-returns on
/// it). Set by the master switch at the top of the Grain Space tab, which is the
/// single runtime gate every early-return and shortcut hook reads.
pub fn is_enabled(app: &AppHandle) -> bool {
    crate::settings::get_settings(app).grain_space_enabled
}

/// Bring the running process in line with the flag, so OFF is zero-overhead
/// without a restart: the feature's global shortcuts register or unregister
/// immediately, the reminder timer arms or tears down, and turning off drops the
/// embedding engine.
///
/// The workspace itself needs no teardown here any more — it is a tab, and
/// `NotesTab` renders the on-ramp instead of the workspace the moment the flag
/// goes false.
///
/// Never touches note data on disk — disabling and uninstalling both leave every
/// file exactly where it is.
pub fn apply_enabled(app: &AppHandle, enabled: bool) {
    let settings = crate::settings::get_settings(app);
    for (id, binding) in settings.bindings.iter() {
        if !id.starts_with("grain_space_") {
            continue;
        }
        if enabled {
            let _ = crate::shortcut::register_shortcut(app, binding.clone());
        } else {
            let _ = crate::shortcut::unregister_shortcut(app, binding.clone());
        }
    }
    reminders::sync(app);
    if !enabled {
        embed::shutdown_engine();
    }
}

/// Notify open surfaces (settings tab / overlay) that notes changed.
pub fn emit_notes_changed(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit(NOTES_CHANGED_EVENT, ());
}

/// Notify open surfaces that the whole CORPUS changed underneath them — a
/// different backend, a different vault, a different store folder.
///
/// This used to be handled by destroying the workspace window, so the next open
/// rebuilt against the new corpus. The workspace is a tab now: it is already
/// mounted, and without this it would keep showing the old vault's notes until
/// something else happened to refresh it. Distinct from
/// [`NOTES_CHANGED_EVENT`] on purpose — that means "re-list"; this means "throw
/// away everything you know, including the open note and the search".
pub fn emit_corpus_changed(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit(CORPUS_CHANGED_EVENT, ());
}

// ── The MCP bridge's read surface ───────────────────────────────────────────
//
// [GRAIN] Three calls the `grain-mcp` proxy makes over the existing local
// request frame (see `host_api::dispatch`). They read the SAME vault and index
// the app's own UI reads — there is no second copy of the notebook and no
// second embedding engine, which is the whole reason the proxy is a proxy.
//
// Each is gated on the feature being on, so switching Grain Space off closes the
// bridge with it rather than leaving a door open onto a disabled feature.

/// The shape one search hit crosses the wire in. Deliberately small: an agent
/// deciding WHICH note to open should not be made to read every note first.
///
/// No `collection`: a note's folder is a property of where its FILE sits, which
/// the card listing derives and a search result does not carry. Returning a
/// field that is always null would tell the caller the notebook has no
/// collections, which is worse than not answering.
#[derive(serde::Serialize)]
pub struct SpaceHit {
    pub id: String,
    pub title: String,
    pub snippet: String,
    /// What the note is about — the same entity list the graph is built from.
    /// Useful for deciding which of several hits to open in full.
    pub entities: Vec<String>,
    pub saved_at: i64,
}

fn require_enabled(app: &AppHandle) -> Result<(), String> {
    if is_enabled(app) {
        Ok(())
    } else {
        Err("Grain Space is switched off.".to_string())
    }
}

/// Collection names, as the sidebar knows them.
pub async fn collections(app: &AppHandle) -> Result<Vec<String>, String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    tauri::async_runtime::spawn_blocking(move || backend::list_folders(&be))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Lightweight bridge search over the derived lexical index. It deliberately
/// does not wake the local embedding model for a headless MCP request. The
/// built-in Agent uses [`search_for_agent`] below for the full retrieval stack.
pub async fn search(app: &AppHandle, query: &str, limit: usize) -> Result<Vec<SpaceHit>, String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    let q = query.to_string();
    let notes = tauri::async_runtime::spawn_blocking(move || backend::search_notes(&be, &q))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(notes.into_iter().take(limit).map(bridge_hit).collect())
}

/// Search for an active built-in Agent turn. Unlike the lightweight MCP bridge
/// search above, this uses Recall's full candidate stack: natural-language FTS,
/// the entity graph, and local BGE when the user enabled it and the model is on
/// disk. BGE remains an optional quality leg; lexical + graph retrieval still
/// works when it is disabled or unavailable.
pub(crate) async fn search_for_agent(
    app: &AppHandle,
    query: &str,
    limit: usize,
) -> Result<Vec<SpaceHit>, String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    let notes = recall::retrieve_for_agent(app, &be, query, limit)
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(notes.into_iter().map(bridge_hit).collect())
}

/// One note in full, by id.
pub async fn get(app: &AppHandle, id: &str) -> Result<note::Note, String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    let id = id.to_string();
    tauri::async_runtime::spawn_blocking(move || backend::get_note(&be, &id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Token file the `grain-mcp` proxy reads to authenticate. Present exactly while
/// the bridge is on.
const MCP_TOKEN_FILE: &str = "mcp-token.json";

/// Bring the MCP bridge in line with its flag. ON mints a fresh token and writes
/// it; OFF revokes whatever was minted and deletes the file, so the door closes
/// rather than merely being unadvertised.
///
/// A fresh token each time is deliberate: turning the bridge off and on again is
/// how a user revokes access from a client they no longer trust, and that only
/// means anything if the old secret stops working.
pub fn apply_mcp(app: &AppHandle, enabled: bool) {
    let Ok(dir) = crate::grain_space::data_dir(app) else {
        return;
    };
    let path = dir.join(MCP_TOKEN_FILE);
    // Whatever was there is dead either way.
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(old) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(token) = old.get("token").and_then(|t| t.as_str()) {
                crate::events_server::revoke_token(token);
            }
        }
    }
    let _ = std::fs::remove_file(&path);
    if !enabled {
        return;
    }
    let token = crate::events_server::mint_mcp_token();
    let body = serde_json::json!({ "token": &token, "port": 7124 });
    if let Err(e) = write_mcp_token_file(&path, body.to_string().as_bytes()) {
        crate::events_server::revoke_token(&token);
        log::warn!("[GRAIN] space mcp: could not write the token file: {e}");
        return;
    }
    restrict_to_owner(&path);
    log::info!("[GRAIN] space mcp: bridge on");
}

fn write_mcp_token_file(path: &std::path::Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "token path has no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".mcp-token-{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(body)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// The token is a bearer secret; on unix the file is 0600. Windows inherits the
/// user profile's ACL, which is already user-only.
fn restrict_to_owner(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Grain's own data directory (where the token file lives).
fn data_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.try_state::<std::sync::Arc<grain_core::AppContext>>()
        .map(|ctx| ctx.data_dir.clone())
        .ok_or_else(|| "app context unavailable".to_string())
}

/// Save a note verbatim without invoking secondary model extraction or reformatting passes.
/// Used by action execution following user confirmation to guarantee byte-for-byte fidelity.
pub async fn save_verbatim(
    app: &AppHandle,
    title: &str,
    body: &str,
    collection: Option<&str>,
) -> Result<String, String> {
    require_enabled(app)?;
    let backend = backend::resolve(app)?;

    let mut note = note::Note::raw(body.trim().to_string());
    note.source = "agent".to_string();
    let clean_title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean_title: String = clean_title.chars().take(80).collect();
    note.title = if clean_title.trim().is_empty() {
        capture::fallback_title(&note.body)
    } else {
        clean_title
    };
    let id = note.id.clone();
    let be = backend.clone();
    tauri::async_runtime::spawn_blocking(move || backend::save_note(&be, &note))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    if let Some(folder) = collection {
        let be = backend.clone();
        let moved = id.clone();
        let folder = folder.trim().to_string();
        if !folder.is_empty() {
            let _ = tauri::async_runtime::spawn_blocking(move || {
                backend::move_note_to_folder(&be, &moved, Some(&folder))
            })
            .await;
        }
    }

    emit_notes_changed(app);
    reminders::sync(app);
    Ok(id)
}

use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;

#[derive(Default)]
struct IdempotencyRegistry {
    completed: VecDeque<String>,
    in_flight: HashSet<String>,
}

static IDEMPOTENCY: Mutex<Option<IdempotencyRegistry>> = Mutex::new(None);
const MAX_IDEMPOTENCY_KEYS: usize = 256;
const MAX_IN_FLIGHT_WRITES: usize = 32;

enum IdempotencyStart {
    Duplicate,
    Started,
}

fn begin_idempotent_write(key: &str) -> Result<IdempotencyStart, String> {
    let mut guard = IDEMPOTENCY
        .lock()
        .map_err(|_| "Write replay registry is unavailable.".to_string())?;
    let registry = guard.get_or_insert_with(IdempotencyRegistry::default);
    if registry.completed.iter().any(|existing| existing == key) {
        return Ok(IdempotencyStart::Duplicate);
    }
    if registry.in_flight.contains(key) {
        return Err("That exact write is already being applied.".to_string());
    }
    if registry.in_flight.len() >= MAX_IN_FLIGHT_WRITES {
        return Err("Too many note writes are already in progress.".to_string());
    }
    registry.in_flight.insert(key.to_string());
    Ok(IdempotencyStart::Started)
}

fn finish_idempotent_write(key: &str, succeeded: bool) {
    let Ok(mut guard) = IDEMPOTENCY.lock() else {
        return;
    };
    let registry = guard.get_or_insert_with(IdempotencyRegistry::default);
    registry.in_flight.remove(key);
    if !succeeded || registry.completed.iter().any(|existing| existing == key) {
        return;
    }
    if registry.completed.len() >= MAX_IDEMPOTENCY_KEYS {
        registry.completed.pop_front();
    }
    registry.completed.push_back(key.to_string());
}

/// Cryptographic 256-bit content version hash (SHA-256) covering title and body
/// for optimistic concurrency and stale-write detection on notes.
#[cfg(test)]
pub fn note_version_hash(title: &str, body: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update(b"\n---\n");
    hasher.update(body.as_bytes());
    format!("{:064x}", hasher.finalize())
}

/// Single-string body helper for tests and backward compatibility.
#[cfg(test)]
pub fn content_version_hash(content: &str) -> String {
    note_version_hash("", content)
}

pub async fn get_append_snapshot(
    app: &AppHandle,
    id: &str,
) -> Result<(note::Note, String), String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    let id = id.to_string();
    tauri::async_runtime::spawn_blocking(move || backend::get_append_snapshot(&be, &id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

pub(crate) fn bounded_bridge_text(mut value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    value
}

fn bridge_hit(note: note::Note) -> SpaceHit {
    SpaceHit {
        id: note.id,
        title: bounded_bridge_text(note.title, 512),
        snippet: bounded_bridge_text(note.tldr, 1024),
        entities: note
            .entities
            .into_iter()
            .take(16)
            .map(|entity| bounded_bridge_text(entity, 128))
            .collect(),
        saved_at: note.timestamp,
    }
}

#[cfg(test)]
mod bridge_security_tests {
    use super::*;

    #[test]
    fn token_file_is_published_complete_without_temp_residue() {
        let dir = std::env::temp_dir().join(format!(
            "grain-mcp-token-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let path = dir.join(MCP_TOKEN_FILE);
        write_mcp_token_file(&path, br#"{"token":"secret","port":7124}"#).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"token":"secret","port":7124}"#
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bridge_metadata_bounds_preserve_utf8() {
        let bounded = bounded_bridge_text("界".repeat(1024), 128);
        assert!(bounded.len() <= 128);
        assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
    }
}

/// Append with explicit operation identity key for deduplication.
pub async fn append_with_idempotency(
    app: &AppHandle,
    id: &str,
    text: &str,
    expected_version: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<(), String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    let id = id.to_string();
    let addition = text.trim().to_string();
    let expected = expected_version.map(|s| s.to_string());

    let reserved_key = match idempotency_key {
        Some(key) => match begin_idempotent_write(key)? {
            IdempotencyStart::Duplicate => {
                log::info!("[GRAIN] space: duplicate append delivery suppressed");
                return Ok(());
            }
            IdempotencyStart::Started => Some(key.to_string()),
        },
        None => None,
    };

    let result = tauri::async_runtime::spawn_blocking(move || {
        backend::append_note_atomic(&be, &id, &addition, expected.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);

    if let Some(key) = &reserved_key {
        finish_idempotent_write(key, result.is_ok());
    }
    result?;

    emit_notes_changed(app);
    Ok(())
}

// ── The extension-facing read surface ───────────────────────────────────────
//
// Both the `notes` extension capability and first-party MCP bridge remain
// read-only until writes have a user-owned, out-of-band approval path.

/// Note cards for a listing: the small shape, without bodies.
pub async fn cards(app: &AppHandle) -> Result<Vec<note::NoteCard>, String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    tauri::async_runtime::spawn_blocking(move || backend::list_cards(&be))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Delete one note.
///
/// Internal deletion primitive. Callers must enforce the normal confirmation
/// policy before entering this host-owned storage path.
pub async fn delete(app: &AppHandle, id: &str) -> Result<(), String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    let id = id.to_string();
    tauri::async_runtime::spawn_blocking(move || backend::delete_note(&be, &id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    emit_notes_changed(app);
    Ok(())
}

/// Rewrite with explicit operation identity and exact-snapshot concurrency.
/// The Agent-facing caller has already canonicalized every replacement field;
/// this function owns replay suppression and the blocking storage boundary.
pub async fn rewrite_with_idempotency(
    app: &AppHandle,
    id: &str,
    replacement: note::NoteRewrite,
    expected_version: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<note::Note, String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    let id = id.to_string();
    let expected = expected_version.map(str::to_string);

    let reserved_key = match idempotency_key {
        Some(key) => match begin_idempotent_write(key)? {
            IdempotencyStart::Duplicate => {
                log::info!("[GRAIN] space: duplicate rewrite delivery suppressed");
                return get(app, &id).await;
            }
            IdempotencyStart::Started => Some(key.to_string()),
        },
        None => None,
    };

    let result = tauri::async_runtime::spawn_blocking(move || {
        backend::rewrite_note_atomic(&be, &id, &replacement, expected.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);

    if let Some(key) = &reserved_key {
        finish_idempotent_write(key, result.is_ok());
    }
    let note = result?;

    emit_notes_changed(app);
    Ok(note)
}
