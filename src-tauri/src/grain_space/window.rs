//! [GRAIN] Grain Space overlay window (Phase 3): a Raycast-style two-pane
//! notes browser. Create-on-summon, destroy-on-close — the webview holds zero
//! RAM while the window doesn't exist. ALL window create/resize/destroy calls
//! run off the shortcut/input thread via the async runtime (tauri#3990: sync
//! window ops from a command/handler can deadlock the main thread).

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

pub const WINDOW_LABEL: &str = "grain-space";

/// Emitted at an already-open overlay to make it select a note.
pub const FOCUS_NOTE_EVENT: &str = "grain-space://focus-note";

const WINDOW_W: f64 = 840.0;
const WINDOW_H: f64 = 560.0;

/// Note id the overlay should select on mount (set when the settings tab or a
/// reminder opens the overlay onto a specific note). Consumed once by
/// `grain_space_take_focus_note` — same stash-then-take pattern as the Agent's
/// selection context.
static FOCUS_NOTE: Mutex<Option<String>> = Mutex::new(None);

pub fn stash_focus_note(id: Option<String>) {
    *FOCUS_NOTE.lock().unwrap() = id;
}

pub fn take_focus_note() -> Option<String> {
    FOCUS_NOTE.lock().unwrap().take()
}

/// Toggle the overlay. Tap-shortcut entry point — returns immediately (work
/// hops to the async runtime). Semantics that keep the window reachable:
/// - not open        → create it,
/// - open but behind → bring it forward (never destroy — don't lose the user's
///                     place just because they clicked away),
/// - open + focused  → close (destroy).
pub fn toggle(app: &AppHandle) {
    if !super::is_enabled(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match app.get_webview_window(WINDOW_LABEL) {
            Some(win) => {
                if win.is_focused().unwrap_or(false) {
                    close(&app);
                } else {
                    let _ = win.unminimize();
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            None => open(&app, None),
        }
    });
}

/// Create (or focus) the overlay window, optionally landing on a note. Safe to
/// call from any thread; the build happens on the async runtime.
pub fn open(app: &AppHandle, focus_note: Option<String>) {
    if !super::is_enabled(app) {
        return;
    }
    stash_focus_note(focus_note);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
            // Already open (e.g. settings tab click while overlay is up):
            // refocus and tell the live overlay to jump to the target note.
            let _ = win.show();
            let _ = win.set_focus();
            if let Some(id) = take_focus_note() {
                use tauri::Emitter;
                let _ = app.emit(FOCUS_NOTE_EVENT, id);
            }
            return;
        }
        if let Err(e) = build(&app) {
            log::error!("[GRAIN] failed to build grain-space window: {e}");
        }
    });
}

/// Destroy the overlay window (close = destroy: `main.tsx` never hides it).
/// Also drops the embedding engine — model lifetime is bound to the window.
pub fn close(app: &AppHandle) {
    let app = app.clone();
    let _ = tauri::async_runtime::spawn(async move {
        if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
            let _ = win.close();
        }
    });
}

fn build(app: &AppHandle) -> tauri::Result<()> {
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, WINDOW_LABEL, tauri::WebviewUrl::App("/".into()))
            .title("Grain Space")
            .inner_size(WINDOW_W, WINDOW_H)
            .min_inner_size(640.0, 420.0)
            .resizable(true)
            .decorations(false)
            .transparent(true)
            // Reachable like a normal window: shows in the taskbar / Alt-Tab so
            // it can be returned to after losing focus (and closed to free the
            // embedding engine). It is NOT always-on-top by design.
            .skip_taskbar(false)
            .focused(true)
            .shadow(false)
            .center()
            .visible(true);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    let window = builder.build()?;

    // The embedding engine may be resident while EITHER this overlay or the
    // Recall agent panel is alive (RECALL-PLAN §3.4). Drop it only if neither
    // remains once this window is gone.
    {
        let app = app.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                super::embed::shutdown_engine_if_idle(&app);
                // A stale focus target must not leak into the next open.
                stash_focus_note(None);
            }
        });
    }

    Ok(())
}
