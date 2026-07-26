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

pub mod backend;
pub mod capture;
pub mod commands;
pub mod embed;
pub mod graph;
pub mod note;
pub mod recall;
pub mod reminders;
pub mod vault;
pub mod window;

use tauri::{AppHandle, Manager};

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
/// immediately, the reminder timer arms or tears down, and turning off DESTROYS
/// the workspace window (not sleeps it) and drops the embedding engine.
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
        window::destroy(app);
        embed::shutdown_engine();
    }
}

/// Notify open surfaces (settings tab / overlay) that notes changed.
pub fn emit_notes_changed(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit(NOTES_CHANGED_EVENT, ());
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

/// The full hybrid search — FTS, vectors and the entity graph, fused — the same
/// one the overlay's search box runs.
pub async fn search(app: &AppHandle, query: &str, limit: usize) -> Result<Vec<SpaceHit>, String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    let q = query.to_string();
    let notes = tauri::async_runtime::spawn_blocking(move || backend::search_notes(&be, &q))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(notes
        .into_iter()
        .take(limit)
        .map(|n| SpaceHit {
            snippet: n.tldr.clone(),
            entities: n.entities.clone(),
            saved_at: n.timestamp,
            id: n.id,
            title: n.title,
        })
        .collect())
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
    let body = serde_json::json!({ "token": token, "port": 7124 });
    if let Err(e) = std::fs::write(&path, body.to_string()) {
        log::warn!("[GRAIN] space mcp: could not write the token file: {e}");
        return;
    }
    restrict_to_owner(&path);
    log::info!("[GRAIN] space mcp: bridge on");
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


/// Metadata an MCP client supplied with a note.
///
/// Every field is optional and every one that is present WINS over anything
/// Grain would derive. The caller is a language model that has just read the
/// conversation the note came out of; Grain's own extraction call sees the body
/// and nothing else, costs a round trip to the user's provider, and — for the
/// many users who have configured no provider at all — does not happen, leaving
/// a plain-code title and no distillation on a note that a perfectly capable
/// model just wrote. Preferring what the caller knows is cheaper AND better.
#[derive(Default)]
pub struct SuppliedMeta {
    pub title: Option<String>,
    pub summary: Option<String>,
    /// Leads the text the note is indexed by, so this is the field that most
    /// affects whether the note is found again.
    pub question: Option<String>,
    pub entities: Vec<String>,
    pub collection: Option<String>,
}

impl SuppliedMeta {
    /// True when the caller told us enough that a distillation call would only
    /// be re-deriving what we already have.
    fn covers_distillation(&self) -> bool {
        self.title.is_some() && (self.question.is_some() || !self.entities.is_empty())
    }
}

/// Save a note sent over the bridge. Returns its id.
///
/// Preference order: what the caller supplied → Grain's own distillation when it
/// did not and a provider exists → a verbatim note with a plain-code title.
pub async fn save(app: &AppHandle, body: &str, supplied: SuppliedMeta) -> Result<String, String> {
    require_enabled(app)?;
    let backend = backend::resolve(app)?;

    let (mut note, relations) = if supplied.covers_distillation() {
        (note::Note::raw(body.trim().to_string()), Vec::new())
    } else {
        capture::compose_note(app, body, None, "mcp").await
    };
    note.source = "mcp".to_string();
    if let Some(t) = supplied.title {
        note.title = t.chars().take(80).collect();
    }
    if let Some(s) = supplied.summary {
        note.tldr = s;
    }
    if let Some(q) = supplied.question {
        note.question = q.chars().take(240).collect();
    }
    if !supplied.entities.is_empty() {
        // Normalised and capped exactly as the capture path does — a supplied
        // entity list is still untrusted input, and the graph's identity rules
        // are not negotiable by the caller.
        note.entities = capture::clean_entity_names(&supplied.entities);
    }
    if note.title.trim().is_empty() {
        note.title = capture::fallback_title(&note.body);
    }

    let id = note.id.clone();
    let be = backend.clone();
    tauri::async_runtime::spawn_blocking(move || backend::save_note(&be, &note))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    if !relations.is_empty() {
        let be = backend.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            backend::record_relations(&be, &relations)
        })
        .await;
    }
    if let Some(folder) = supplied.collection {
        let be = backend.clone();
        let moved = id.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            backend::move_note_to_folder(&be, &moved, Some(&folder))
        })
        .await;
    }
    emit_notes_changed(app);
    reminders::sync(app);
    Ok(id)
}

/// Append to an existing note, under a rule. The running-log case: a session's
/// decisions, a list that keeps growing. Never rewrites what is already there.
pub async fn append(app: &AppHandle, id: &str, text: &str) -> Result<(), String> {
    require_enabled(app)?;
    let be = backend::resolve(app)?;
    let id = id.to_string();
    let addition = text.trim().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let mut note = backend::get_note(&be, &id)?;
        note.body = format!("{}\n\n---\n\n{}", note.body.trim_end(), addition);
        backend::save_note(&be, &note)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    emit_notes_changed(app);
    Ok(())
}
