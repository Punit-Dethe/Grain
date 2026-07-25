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
/// it). Since the feature became a `builtin`-tier extension it is no longer set
/// by a settings tab: [`sync_install_state`] mirrors the registry bit into it, so
/// "not installed" and "installed but disabled" both land on `false` — exactly
/// the state the feature already knew how to be.
pub fn is_enabled(app: &AppHandle) -> bool {
    crate::settings::get_settings(app).grain_space_enabled
}

/// Mirror the extension record into the runtime flag. Called when the record is
/// toggled, when it is uninstalled, and once at startup — the last one matters,
/// because a `true` left behind by an install that is no longer present would
/// otherwise leave the feature half-alive with no way to see or turn it off.
///
/// Returns true when the flag actually changed (so callers can (un)register
/// shortcuts only when there is something to do).
pub fn sync_install_state(app: &AppHandle) -> bool {
    use grain_core::extensions as ext;
    let installed_and_on = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .is_some_and(|reg| {
            reg.record(ext::GRAIN_SPACE_ID)
                .is_some_and(|record| record.enabled)
        });
    let mut settings = crate::settings::get_settings(app);
    if settings.grain_space_enabled == installed_and_on {
        return false;
    }
    settings.grain_space_enabled = installed_and_on;
    crate::settings::write_settings(app, settings);
    apply_enabled(app, installed_and_on);
    true
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
