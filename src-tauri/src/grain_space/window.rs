//! [GRAIN] Grain Space workspace window (TAURI-OVERLAY-PLAN.md): the
//! Mem/Obsidian-style three-pane notes workspace.
//!
//! Lifecycle: **hide-don't-destroy**. The window is built ONCE (hidden), shown
//! when the frontend reports its UI mounted, and put to SLEEP instead of being
//! destroyed on close:
//!   1. backend emits `grain-space://sleep`,
//!   2. the frontend flushes pending saves and unmounts the ENTIRE React tree
//!      (the DOM purge — the JS heap becomes collectable),
//!   3. the frontend acks (`grain_space_sleep_ready`) and the backend hides
//!      the window, suspends the WebView2 renderer (`TrySuspend` + memory
//!      target LOW on Windows) and drops the embedding engine.
//! Waking reverses it: resume the webview, emit `grain-space://revive`, show +
//! focus only after the frontend re-mounts and acks (`grain_space_ui_ready`),
//! so the window appears already painted. Both acks have fallback timers — a
//! wedged frontend can never make the window unreachable or unhideable.
//!
//! ALL window show/hide/build calls run off the shortcut/input thread via the
//! async runtime (tauri#3990: sync window ops from a command/handler can
//! deadlock the main thread).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

pub const WINDOW_LABEL: &str = "grain-space";

/// Emitted at an already-awake overlay to make it select a note.
pub const FOCUS_NOTE_EVENT: &str = "grain-space://focus-note";
/// Backend → frontend: unmount the UI and ack with `grain_space_sleep_ready`.
pub const SLEEP_EVENT: &str = "grain-space://sleep";
/// Backend → frontend: re-mount the UI and ack with `grain_space_ui_ready`.
pub const REVIVE_EVENT: &str = "grain-space://revive";

const WINDOW_W: f64 = 1180.0;
const WINDOW_H: f64 = 760.0;

/// How long we wait for a frontend ack before forcing the transition anyway.
const ACK_FALLBACK: Duration = Duration::from_millis(700);

/// Whether the overlay is (or is becoming) visible. Guards against a stale
/// sleep ack hiding a window the user just re-summoned.
static AWAKE: AtomicBool = AtomicBool::new(false);

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
/// - not built / asleep → wake it,
/// - awake but behind   → bring it forward (never sleep — don't lose the
///                        user's place just because they clicked away),
/// - awake + focused    → sleep.
pub fn toggle(app: &AppHandle) {
    if !super::is_enabled(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match app.get_webview_window(WINDOW_LABEL) {
            Some(win) => {
                if !AWAKE.load(Ordering::SeqCst) || !win.is_visible().unwrap_or(false) {
                    wake(&app);
                } else if win.is_focused().unwrap_or(false) {
                    close(&app);
                } else {
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
            None => {
                if let Err(e) = build(&app) {
                    log::error!("[GRAIN] failed to build grain-space window: {e}");
                }
            }
        }
    });
}

/// Open (or wake, or refocus) the overlay, optionally landing on a note. Safe
/// to call from any thread; the work happens on the async runtime.
pub fn open(app: &AppHandle, focus_note: Option<String>) {
    if !super::is_enabled(app) {
        return;
    }
    stash_focus_note(focus_note);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match app.get_webview_window(WINDOW_LABEL) {
            Some(win) => {
                if !AWAKE.load(Ordering::SeqCst) || !win.is_visible().unwrap_or(false) {
                    // Asleep: the reviving UI consumes the stashed focus note
                    // on mount, exactly like a fresh build.
                    wake(&app);
                } else {
                    // Awake (e.g. settings tab click while the overlay is up):
                    // refocus and tell the live overlay to jump to the note.
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                    if let Some(id) = take_focus_note() {
                        let _ = app.emit(FOCUS_NOTE_EVENT, id);
                    }
                }
            }
            None => {
                if let Err(e) = build(&app) {
                    log::error!("[GRAIN] failed to build grain-space window: {e}");
                }
            }
        }
    });
}

/// Put the overlay to sleep (the public "close": every caller — Esc, the close
/// button, backend switches — lands here). The frontend gets a chance to flush
/// saves and purge its DOM before the window hides; a fallback timer hides it
/// regardless so sleep can never hang on a wedged webview.
pub fn close(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(win) = app.get_webview_window(WINDOW_LABEL) else {
            return;
        };
        if !win.is_visible().unwrap_or(false) && !AWAKE.load(Ordering::SeqCst) {
            return; // already asleep
        }
        AWAKE.store(false, Ordering::SeqCst);
        let _ = app.emit(SLEEP_EVENT, ());
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(ACK_FALLBACK).await;
            if !AWAKE.load(Ordering::SeqCst) {
                if let Some(win) = app2.get_webview_window(WINDOW_LABEL) {
                    if win.is_visible().unwrap_or(false) {
                        log::warn!("[GRAIN] space: sleep ack timed out — hiding anyway");
                        finish_sleep(&app2);
                    }
                }
            }
        });
    });
}

/// Frontend ack: the React tree is unmounted — hide + trim now.
pub fn sleep_ready(app: &AppHandle) {
    if AWAKE.load(Ordering::SeqCst) {
        return; // stale ack: the user already re-summoned the overlay
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        finish_sleep(&app);
    });
}

/// Frontend ack: the UI is mounted and painted — reveal the window.
pub fn ui_ready(app: &AppHandle) {
    AWAKE.store(true, Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
    });
}

/// Hide + release everything the sleeping window doesn't need: the WebView2
/// renderer gets suspended (its working set collapses once the purged DOM is
/// the whole document) and the embedding engine drops unless the Agent panel
/// still needs it. Runs on the async runtime.
fn finish_sleep(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
        let _ = win.hide();
        set_webview_suspended(&win, true);
    }
    super::embed::shutdown_engine_if_idle(app);
    // A stale focus target must not leak into the next wake.
    stash_focus_note(None);
}

/// Resume the webview and ask the (purged) frontend to re-mount. The window
/// stays hidden until `ui_ready`; the fallback shows it even without an ack.
fn wake(app: &AppHandle) {
    let Some(win) = app.get_webview_window(WINDOW_LABEL) else {
        return;
    };
    set_webview_suspended(&win, false);
    let _ = app.emit(REVIVE_EVENT, ());
    fallback_show(app);
}

/// Show the window after `ACK_FALLBACK` even if the frontend never acked —
/// a broken UI must stay reachable (and closable).
fn fallback_show(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(ACK_FALLBACK).await;
        if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
            if !win.is_visible().unwrap_or(true) {
                log::warn!("[GRAIN] space: ui-ready ack timed out — showing anyway");
                AWAKE.store(true, Ordering::SeqCst);
                let _ = win.show();
                let _ = win.set_focus();
            }
        }
    });
}

/// Suspend/resume the WebView2 renderer (Windows). Suspension is the Edge
/// "sleeping tabs" mechanism: with the DOM already purged it collapses the
/// webview processes' working set to a few MB. `MemoryUsageTargetLevel` LOW
/// additionally tells the browser process to shrink caches. Both are
/// best-effort — failures are ignored (worst case: idle RAM stays as before).
#[cfg(windows)]
fn set_webview_suspended(win: &tauri::WebviewWindow, suspend: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, ICoreWebView2_3, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows::core::Interface;
    let result = win.with_webview(move |wv| unsafe {
        let controller = wv.controller();
        let Ok(core) = controller.CoreWebView2() else {
            return;
        };
        if suspend {
            // TrySuspend requires the controller to be invisible first.
            let _ = controller.SetIsVisible(false);
            if let Ok(w19) = core.cast::<ICoreWebView2_19>() {
                let _ = w19.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW);
            }
            if let Ok(w3) = core.cast::<ICoreWebView2_3>() {
                let handler = webview2_com::TrySuspendCompletedHandler::create(Box::new(
                    |_hresult, _suspended| Ok(()),
                ));
                let _ = w3.TrySuspend(&handler);
            }
        } else {
            if let Ok(w3) = core.cast::<ICoreWebView2_3>() {
                let _ = w3.Resume();
            }
            if let Ok(w19) = core.cast::<ICoreWebView2_19>() {
                let _ =
                    w19.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL);
            }
            let _ = controller.SetIsVisible(true);
        }
    });
    if let Err(e) = result {
        log::warn!("[GRAIN] space: webview suspend({suspend}) unavailable: {e}");
    }
}

#[cfg(not(windows))]
fn set_webview_suspended(_win: &tauri::WebviewWindow, _suspend: bool) {}

fn build(app: &AppHandle) -> tauri::Result<()> {
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, WINDOW_LABEL, tauri::WebviewUrl::App("/".into()))
            .title("Grain Space")
            .inner_size(WINDOW_W, WINDOW_H)
            .min_inner_size(880.0, 560.0)
            .resizable(true)
            .decorations(false)
            .transparent(true)
            // Reachable like a normal window: shows in the taskbar / Alt-Tab so
            // it can be returned to after losing focus. NOT always-on-top.
            .skip_taskbar(false)
            .focused(false)
            .shadow(false)
            .center()
            // Born hidden: `ui_ready` reveals it once the UI is painted.
            .visible(false);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    let window = builder.build()?;
    fallback_show(app);

    {
        let app = app.clone();
        window.on_window_event(move |event| match event {
            // OS-level close (Alt+F4 / taskbar) becomes sleep, so reopen stays
            // instant. The window is only truly destroyed with the app.
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                close(&app);
            }
            tauri::WindowEvent::Destroyed => {
                AWAKE.store(false, Ordering::SeqCst);
                super::embed::shutdown_engine_if_idle(&app);
                stash_focus_note(None);
            }
            _ => {}
        });
    }

    Ok(())
}
