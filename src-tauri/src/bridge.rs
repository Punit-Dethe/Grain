//! [GRAIN] Bridge from the Tauri shell to the headless core's event bus.
//!
//! Deliver Grain capture context to the recording WebView and retain public
//! SDK events on the core bus for extensions and the development controller.

use std::sync::Arc;

use grain_core::{AppContext, DaemonEvent};
use tauri::{AppHandle, Manager};

/// Broadcast a `DaemonEvent` on the core bus. No-op if the context isn't staged
/// yet (e.g. very early startup), so this is always safe to call.
pub fn emit(app: &AppHandle, event: DaemonEvent) {
    crate::prompt_record::on_event(app, &event);
    crate::grain_overlay::on_event(app, &event);
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        ctx.emit(event);
    }
}
