//! [GRAIN] The Handy webview recording overlay is fully RETIRED.
//!
//! grain-pill (crates/grain-pill) is now the single overlay surface for both
//! batch and rolling transcription — shown/hidden and positioned entirely from
//! the core's `DaemonEvent` stream (lifecycle + `OverlayConfig`, which carries
//! the user's `overlay_position`). The old per-platform webview window
//! (WebviewWindowBuilder / macOS NSPanel / Linux GTK layer-shell) and its
//! show/hide/position helpers are gone. Only the audio-level fan-out lives here.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

static LAST_MIC_LEVEL_EMIT: AtomicU64 = AtomicU64::new(0);
const EMIT_THROTTLE_MS: u64 = 33; // ~30 FPS

/// Forward per-bucket mic levels to the headless event bus, where the pill
/// picks them up over the WS to drive its Aura animation. This is the ONLY
/// consumer: the `"mic-level"` webview event upstream emits for its overlay
/// went away with the webview overlay, and nothing in the frontend listens
/// for it.
///
/// Upstream additionally gates this on an `OVERLAY_ENABLED` cache (#1447) to
/// spare its overlay's WebKit process; that leak (wry#1489) is webview-only,
/// so the gate is deliberately not carried here.
pub fn emit_levels(app_handle: &AppHandle, levels: &Vec<f32>) {
    // Throttle to ~30 FPS (upstream #1444). The raw audio callback fires far
    // faster than any UI needs, and the pill redraws at frame rate anyway.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let last = LAST_MIC_LEVEL_EMIT.load(Ordering::Relaxed);
    if now.saturating_sub(last) < EMIT_THROTTLE_MS {
        return;
    }
    LAST_MIC_LEVEL_EMIT.store(now, Ordering::Relaxed);
    crate::bridge::emit(
        app_handle,
        grain_core::DaemonEvent::AudioLevel {
            levels: levels.clone(),
        },
    );
}
