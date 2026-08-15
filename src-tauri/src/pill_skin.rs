//! [GRAIN] Delivering the pill SKIN (the built-in look) from settings to the pill.
//!
//! The sibling of `pill_theme`, and it works the same way for the same reason:
//! the skin is *data*, resolved here and sent to the pill as a
//! [`DaemonEvent::PillSkin`]. Two delivery moments, and both matter:
//! - **on connect** the pill is handed the current skin directly (a broadcast
//!   only reaches an already-connected client, and the pill connects late);
//! - **on change** it is broadcast, from the settings command that wrote it.
//!
//! Where a theme comes from an extension and may be absent, a skin always has a
//! value — so this resolves to a concrete [`PillSkin`], never an `Option`.

use std::sync::Arc;

use grain_core::AppContext;
use grain_sdk::{DaemonEvent, PillSkin};
use tauri::{AppHandle, Manager};

/// The skin the pill should currently wear — straight from settings.
pub fn current(app: &AppHandle) -> PillSkin {
    crate::settings::get_settings(app).pill_skin
}

/// A serialized `PillSkin` event for the current skin, ready to queue onto a
/// connection. Used by the events server to greet the pill.
pub fn welcome_frame(app: &AppHandle) -> Option<String> {
    serde_json::to_string(&DaemonEvent::PillSkin { skin: current(app) }).ok()
}

/// Broadcast the current skin to every subscriber (the pill). Harmless if
/// nothing is listening yet — an idle pill picks it up from the welcome frame
/// when it next connects.
pub fn broadcast(app: &AppHandle, skin: PillSkin) {
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        ctx.emit(DaemonEvent::PillSkin { skin });
    }
}
