//! Grain-owned pill styling updates on the public bus and WebView event bridge.
use grain_sdk::{DaemonEvent, PillSkin};
use tauri::AppHandle;

/// Late WebView subscribers obtain current settings from `overlay_snapshot`.
pub fn broadcast(app: &AppHandle, skin: PillSkin) {
    crate::bridge::emit(app, DaemonEvent::PillSkin { skin });
}
