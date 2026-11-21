//! [GRAIN] Open the main window on a page, from the backend (an Agent link to
//! a meeting).
//!
//! The main window is destroyed on close to free RAM, so a route sent as an
//! event alone could be lost while the window is being rebuilt. The route is
//! parked in a one-slot mailbox instead: the backend shows the window and
//! emits `MainWindowRoute`; the page takes the route on that event or when it
//! mounts, whichever comes first. Taking empties the slot.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event as _;

static PENDING: Mutex<Option<String>> = Mutex::new(None);

/// A route is waiting: call `take_main_window_route`.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct MainWindowRoute;

/// Show the main window (rebuilding it if closed) on `route`, a hash path
/// such as `/meet/<id>`.
pub(crate) fn open(app: &AppHandle, route: String) {
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(route);
    crate::show_main_window(app);
    if let Err(e) = MainWindowRoute.emit(app) {
        log::debug!("[GRAIN] route: event not delivered ({e}); the page takes it on load");
    }
}

/// The route waiting for the main window, if any. Empties the slot.
#[tauri::command]
#[specta::specta]
pub fn take_main_window_route() -> Option<String> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).take()
}
