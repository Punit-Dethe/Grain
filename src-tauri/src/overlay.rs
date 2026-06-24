//! [GRAIN] The Handy webview recording overlay is fully RETIRED.
//!
//! grain-pill (crates/grain-pill) is now the single overlay surface for both
//! batch and rolling transcription — shown/hidden and positioned entirely from
//! the core's `DaemonEvent` stream (lifecycle + `OverlayConfig`, which carries
//! the user's `overlay_position`). The old per-platform webview window
//! (WebviewWindowBuilder / macOS NSPanel / Linux GTK layer-shell) and its
//! show/hide/position helpers are gone. Only the audio-level fan-out lives here.

use tauri::{AppHandle, Emitter};

/// Forward per-bucket mic levels to (1) the main settings window's visualizer
/// (the `"mic-level"` webview event) and (2) the headless event bus, where the
/// pill picks them up over the WS to drive its Aura animation.
pub fn emit_levels(app_handle: &AppHandle, levels: &Vec<f32>) {
    let _ = app_handle.emit("mic-level", levels);

    crate::bridge::emit(
        app_handle,
        grain_core::DaemonEvent::AudioLevel {
            levels: levels.clone(),
        },
    );
}
