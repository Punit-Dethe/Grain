//! [GRAIN] Delivering the pill theme (SPEC §9) from the `pill.theme` slot
//! occupant to the pill.
//!
//! The theme is *data*, resolved here and sent to the pill as a
//! [`DaemonEvent::PillTheme`]. The pill renders it; nothing in this path runs
//! extension code. Two delivery moments, and both matter:
//! - **on connect** the pill is handed the current theme directly (a broadcast
//!   only reaches an already-connected client, and the pill connects late);
//! - **on change** — any `pill.theme` slot mutation — it is broadcast, which is
//!   why [`broadcast`] hangs off `refresh_index` alongside the shortcut sync.
//!
//! A theme that fails to parse resolves to `None` (Grain's default look), never
//! an error — SPEC §9's "the pill must always render," enforced at the source.

use std::sync::Arc;

use grain_core::AppContext;
use grain_sdk::{DaemonEvent, PillTheme};
use tauri::{AppHandle, Manager};

/// The theme the pill should currently wear: the `pill.theme` slot occupant's
/// declared theme, or `None` when core owns the slot. A garbage or absent theme
/// payload is `None`, so a broken theme pack shows Grain's look rather than
/// nothing.
pub fn current(app: &AppHandle) -> Option<PillTheme> {
    use grain_core::extensions as ext;
    let reg = app.try_state::<Arc<ext::ExtensionsRegistry>>()?;
    let occupant = reg.slot_occupant(ext::PILL_THEME_SLOT)?;
    if occupant == ext::CORE_DEFAULT {
        return None;
    }
    let pack = crate::extension_host::load_manifest(app, &occupant)?;
    let value = pack.payloads.pill_theme?;
    serde_json::from_value::<PillTheme>(value).ok()
}

/// A serialized `PillTheme` event for the current theme, ready to queue onto a
/// connection. Used by the events server to greet the pill.
pub fn welcome_frame(app: &AppHandle) -> Option<String> {
    serde_json::to_string(&DaemonEvent::PillTheme {
        theme: current(app),
    })
    .ok()
}

/// Broadcast the current theme to every subscriber (the pill). Call on any
/// `pill.theme` slot change; harmless if nothing is listening yet.
pub fn broadcast(app: &AppHandle) {
    let theme = current(app);
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        ctx.emit(DaemonEvent::PillTheme { theme });
    }
}
