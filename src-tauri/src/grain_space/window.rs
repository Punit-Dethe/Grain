//! [GRAIN] Grain Space workspace window (TAURI-OVERLAY-PLAN.md): the
//! Mem/Obsidian-style three-pane notes workspace.
//!
//! The sleeping-window lifecycle this feature pioneered now lives in
//! [`crate::surfaces::workspace`], where extensions use the same machinery
//! (SPEC §1.2). What stays here is only what is *about Grain Space*: its window
//! geometry, its event names, the enabled-gate, and the fact that its payload is
//! a note id.
//!
//! Behaviour is unchanged by the extraction, deliberately — the ack handshake
//! (unmount, then hide + suspend the renderer) is what makes the window cost
//! almost nothing while asleep, and both directions keep their fallback timers
//! so a wedged frontend can never make the window unreachable or unhideable.

use std::sync::Arc;

use tauri::AppHandle;

use crate::surfaces::workspace::{self, Surface, WorkspaceSpec};

pub const WINDOW_LABEL: &str = "grain-space";

/// Emitted at an already-awake overlay to make it select a note.
pub const FOCUS_NOTE_EVENT: &str = "grain-space://focus-note";
/// Backend → frontend: unmount the UI and ack with `grain_space_sleep_ready`.
pub const SLEEP_EVENT: &str = "grain-space://sleep";
/// Backend → frontend: re-mount the UI and ack with `grain_space_ui_ready`.
pub const REVIVE_EVENT: &str = "grain-space://revive";

const WINDOW_W: f64 = 1180.0;
const WINDOW_H: f64 = 760.0;

/// Grain Space's registration with the surface host. Idempotent and cheap — the
/// surface is created on first use, so a user who never opens the workspace
/// never pays for it.
fn surface() -> Arc<Surface> {
    workspace::ensure(WorkspaceSpec {
        id: WINDOW_LABEL.to_string(),
        label: WINDOW_LABEL.to_string(),
        url: tauri::WebviewUrl::App("/".into()),
        title: "Grain Space".to_string(),
        size: (WINDOW_W, WINDOW_H),
        min_size: (880.0, 560.0),
        sleep_event: SLEEP_EVENT.to_string(),
        revive_event: REVIVE_EVENT.to_string(),
        payload_event: FOCUS_NOTE_EVENT.to_string(),
        decorations: false,
        transparent: true,
        // Asleep, the embedding engine is dead weight — but the Agent panel may
        // still be using it, so ask rather than drop.
        on_sleep: Some(Arc::new(|app| super::embed::shutdown_engine_if_idle(app))),
        on_destroy: Some(Arc::new(|app| super::embed::shutdown_engine_if_idle(app))),
    })
}

/// The note the overlay should select on mount, consumed once — set when the
/// settings tab or a reminder opens the overlay onto a specific note.
pub fn take_focus_note() -> Option<String> {
    surface()
        .take_payload()
        .and_then(|v| v.as_str().map(str::to_string))
}

/// Toggle the overlay. Tap-shortcut entry point — returns immediately.
pub fn toggle(app: &AppHandle) {
    if !super::is_enabled(app) {
        return;
    }
    workspace::toggle(app, &surface());
}

/// Open (or wake, or refocus) the overlay, optionally landing on a note.
pub fn open(app: &AppHandle, focus_note: Option<String>) {
    if !super::is_enabled(app) {
        return;
    }
    workspace::open(app, &surface(), focus_note.map(serde_json::Value::String));
}

/// Put the overlay to sleep — the public "close" every caller lands on.
pub fn close(app: &AppHandle) {
    workspace::close(app, WINDOW_LABEL);
}

/// TRULY close the overlay: destroy the window and its webview so nothing of the
/// workspace stays resident. Used when the feature is disabled or the active
/// corpus changes underneath it.
pub fn destroy(app: &AppHandle) {
    // Registered on demand, so a destroy before the first open would otherwise
    // find no surface and skip the (harmless) teardown.
    let _ = surface();
    workspace::destroy(app, WINDOW_LABEL);
}

/// Frontend ack: the React tree is unmounted — hide + trim now.
pub fn sleep_ready(app: &AppHandle) {
    workspace::sleep_ready(app, WINDOW_LABEL);
}

/// Frontend ack: the UI is mounted and painted — reveal the window.
pub fn ui_ready(app: &AppHandle) {
    workspace::ui_ready(app, WINDOW_LABEL);
}
