//! [GRAIN] Bridge from the Tauri shell to the headless core's event bus.
//!
//! Handy's managers emit to the webview via `app.emit("...")`. The winit pill is
//! a separate native surface and can't receive Tauri webview events, so it
//! subscribes to [`grain_core::DaemonEvent`]s on the [`AppContext`] broadcast bus
//! instead. This helper re-broadcasts a typed event there. Calls are additive:
//! existing `app.emit` stays, so nothing in Handy's behavior changes.

use std::sync::Arc;

use grain_core::{AppContext, DaemonEvent};
use tauri::{AppHandle, Manager};

/// Broadcast a `DaemonEvent` on the core bus. No-op if the context isn't staged
/// yet (e.g. very early startup), so this is always safe to call.
pub fn emit(app: &AppHandle, event: DaemonEvent) {
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        ctx.emit(event);
    }
}
