//! [GRAIN] Host-owned **workspace** surfaces (SPEC §1.2) — the sleeping-window
//! pattern, generalized.
//!
//! Extracted verbatim from `grain_space/window.rs`, which proved it: a
//! workspace window is built ONCE (hidden), shown when its frontend reports the
//! UI mounted, and put to **sleep** instead of destroyed on close:
//!   1. the host emits the surface's sleep event,
//!   2. the frontend flushes pending work and unmounts its ENTIRE tree (the DOM
//!      purge — the JS heap becomes collectable),
//!   3. the frontend acks, and the host hides the window and suspends the
//!      WebView2 renderer (`TrySuspend` + memory target LOW on Windows).
//! Waking reverses it, and the window stays hidden until the frontend acks so it
//! appears already painted.
//!
//! **The ack is load-bearing, and so is its fallback.** Hiding without waiting
//! for the unmount leaves the old DOM resident and throws away the entire reason
//! the pattern exists; never acking at all would make a window unreachable or
//! unhideable. Both directions therefore have a fallback timer that forces the
//! transition, and a wedged frontend degrades to "an ordinary window" rather
//! than to a stuck one.
//!
//! **Extensions never own a window** (SPEC §1.2). They declare a workspace; the
//! host builds, places, sleeps and destroys it. Everything here is keyed by a
//! surface id, so the same machinery serves Grain Space and a third-party pack
//! without either learning about the other.
//!
//! ALL window show/hide/build calls hop to the async runtime — a synchronous
//! window operation from a command or shortcut handler can deadlock the main
//! thread (tauri#3990).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

/// How long to wait for a frontend ack before forcing the transition anyway.
const ACK_FALLBACK: Duration = Duration::from_millis(700);

/// How many **capped** workspaces may be awake at once (SPEC §1.2). Grain's own
/// surfaces are uncapped; this bounds what extensions can keep resident, so a
/// user who opens five workspaces ends up with one hidden window and four
/// sleeping ones rather than five live webviews.
///
/// Reaching the cap sleeps the least-recently-used workspace — it never refuses
/// to open. A surface the user explicitly asked for must always appear.
const MAX_AWAKE_CAPPED: usize = 1;

/// Monotonic tick handed out on each wake, so "least recently used" is ordering
/// we own rather than wall-clock time (which can jump).
static USE_CLOCK: AtomicU64 = AtomicU64::new(0);

/// A hook the owning feature runs at a lifecycle moment — Grain Space uses it to
/// drop its embedding engine once the workspace is asleep.
pub type SurfaceHook = Arc<dyn Fn(&AppHandle) + Send + Sync>;

/// Everything the host needs to build and drive one workspace window.
///
/// Event names are per-surface rather than derived, because Grain Space's
/// (`grain-space://sleep`, …) are already load-bearing contract with its
/// frontend and must not change during the extraction.
#[derive(Clone)]
pub struct WorkspaceSpec {
    /// Stable surface id — the registry key. For an extension, its id.
    pub id: String,
    /// Tauri window label.
    pub label: String,
    pub url: tauri::WebviewUrl,
    pub title: String,
    pub size: (f64, f64),
    pub min_size: (f64, f64),
    /// Host → frontend: unmount and ack.
    pub sleep_event: String,
    /// Host → frontend: re-mount and ack.
    pub revive_event: String,
    /// Host → an *already awake* frontend: here is a new payload.
    pub payload_event: String,
    pub decorations: bool,
    pub transparent: bool,
    /// Whether this surface counts against [`MAX_AWAKE_CAPPED`]. Grain's own
    /// workspaces are uncapped — the cap exists to bound *extensions*, and a
    /// core feature must never be slept to make room for one.
    pub capped: bool,
    /// Run after the window is hidden and its renderer suspended.
    pub on_sleep: Option<SurfaceHook>,
    /// Run when the window is truly destroyed.
    pub on_destroy: Option<SurfaceHook>,
}

/// Live state for one registered workspace.
pub struct Surface {
    spec: WorkspaceSpec,
    /// Whether the window is (or is becoming) visible. Guards against a stale
    /// sleep ack hiding a window the user just re-summoned.
    awake: AtomicBool,
    /// What the frontend should open onto, consumed once on mount — the same
    /// stash-then-take pattern as the Agent's selection context, generalized
    /// from Grain Space's focus-note to arbitrary JSON.
    payload: Mutex<Option<serde_json::Value>>,
    /// Tick of the most recent wake, for LRU eviction.
    last_used: AtomicU64,
}

impl Surface {
    pub fn is_awake(&self) -> bool {
        self.awake.load(Ordering::SeqCst)
    }

    /// Take the pending payload, leaving none behind.
    pub fn take_payload(&self) -> Option<serde_json::Value> {
        self.payload.lock().unwrap().take()
    }

    pub fn stash_payload(&self, payload: Option<serde_json::Value>) {
        *self.payload.lock().unwrap() = payload;
    }
}

/// id → surface. Populated by [`ensure`] on first use, so a workspace that is
/// never opened costs nothing but the absent map entry.
static SURFACES: OnceLock<Mutex<HashMap<String, Arc<Surface>>>> = OnceLock::new();

fn surfaces() -> &'static Mutex<HashMap<String, Arc<Surface>>> {
    SURFACES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a workspace (idempotent) and hand back its live state. The spec of
/// an already-registered surface is kept — its window may be built against it.
pub fn ensure(spec: WorkspaceSpec) -> Arc<Surface> {
    let mut map = surfaces().lock().unwrap();
    Arc::clone(map.entry(spec.id.clone()).or_insert_with(|| {
        Arc::new(Surface {
            spec,
            awake: AtomicBool::new(false),
            payload: Mutex::new(None),
            last_used: AtomicU64::new(0),
        })
    }))
}

/// A registered surface, if any. `None` is always a legitimate answer — an ack
/// can arrive for a surface whose feature was disabled meanwhile.
pub fn get(id: &str) -> Option<Arc<Surface>> {
    surfaces().lock().unwrap().get(id).cloned()
}

/// Toggle the workspace. Safe from a shortcut handler — returns immediately.
/// Semantics that keep the window reachable:
/// - not built / asleep → wake it,
/// - awake but behind   → bring it forward (never sleep — don't lose the user's
///                        place just because they clicked away),
/// - awake + focused    → sleep.
pub fn toggle(app: &AppHandle, surface: &Arc<Surface>) {
    let app = app.clone();
    let surface = Arc::clone(surface);
    tauri::async_runtime::spawn(async move {
        match app.get_webview_window(&surface.spec.label) {
            Some(win) => {
                if !surface.is_awake() || !win.is_visible().unwrap_or(false) {
                    wake(&app, &surface);
                } else if win.is_focused().unwrap_or(false) {
                    close(&app, &surface.spec.id);
                } else {
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
            None => build(&app, &surface),
        }
    });
}

/// Open (or wake, or refocus) the workspace, optionally handing the frontend a
/// payload. Safe to call from any thread.
pub fn open(app: &AppHandle, surface: &Arc<Surface>, payload: Option<serde_json::Value>) {
    surface.stash_payload(payload);
    let app = app.clone();
    let surface = Arc::clone(surface);
    tauri::async_runtime::spawn(async move {
        match app.get_webview_window(&surface.spec.label) {
            Some(win) => {
                if !surface.is_awake() || !win.is_visible().unwrap_or(false) {
                    // Asleep: the reviving UI consumes the stashed payload on
                    // mount, exactly like a fresh build.
                    wake(&app, &surface);
                } else {
                    // Awake already: refocus and tell the live UI to jump.
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                    if let Some(p) = surface.take_payload() {
                        let _ = app.emit(&surface.spec.payload_event, p);
                    }
                }
            }
            None => build(&app, &surface),
        }
    });
}

/// Put the workspace to sleep — the public "close": every caller (Esc, the close
/// button, a backend switch) lands here. The frontend gets a chance to flush and
/// purge its DOM before the window hides; a fallback timer hides it regardless,
/// so sleep can never hang on a wedged webview.
pub fn close(app: &AppHandle, id: &str) {
    let Some(surface) = get(id) else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(win) = app.get_webview_window(&surface.spec.label) else {
            return;
        };
        if !win.is_visible().unwrap_or(false) && !surface.is_awake() {
            return; // already asleep
        }
        surface.awake.store(false, Ordering::SeqCst);
        let _ = app.emit(&surface.spec.sleep_event, ());
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(ACK_FALLBACK).await;
            if !surface.is_awake() {
                if let Some(win) = app2.get_webview_window(&surface.spec.label) {
                    if win.is_visible().unwrap_or(false) {
                        log::warn!(
                            "[GRAIN] surface '{}': sleep ack timed out — hiding anyway",
                            surface.spec.id
                        );
                        finish_sleep(&app2, &surface);
                    }
                }
            }
        });
    });
}

/// TRULY close the workspace: destroy the window and its webview so NOTHING of
/// it stays resident. For when the owning feature is disabled or its underlying
/// data changed — cases where instant re-summon is not worth a hidden window
/// (and, for disable, must not be). Distinct from [`close`], which only sleeps.
pub fn destroy(app: &AppHandle, id: &str) {
    let Some(surface) = get(id) else {
        return;
    };
    surface.awake.store(false, Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(win) = app.get_webview_window(&surface.spec.label) {
            // Resume first so a suspended renderer tears down cleanly; the
            // Destroyed hook does the rest.
            set_webview_suspended(&win, false);
            let _ = win.destroy();
        }
    });
}

/// Frontend ack: the tree is unmounted — hide and trim now.
pub fn sleep_ready(app: &AppHandle, id: &str) {
    let Some(surface) = get(id) else {
        return;
    };
    if surface.is_awake() {
        return; // stale ack: the user already re-summoned it
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        finish_sleep(&app, &surface);
    });
}

/// Frontend ack: the UI is mounted and painted — reveal the window.
pub fn ui_ready(app: &AppHandle, id: &str) {
    let Some(surface) = get(id) else {
        return;
    };
    surface.awake.store(true, Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(win) = app.get_webview_window(&surface.spec.label) {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
    });
}

/// Hide and release everything a sleeping window doesn't need: the WebView2
/// renderer is suspended (its working set collapses once the purged DOM is the
/// whole document), then the surface's own hook drops feature resources.
fn finish_sleep(app: &AppHandle, surface: &Arc<Surface>) {
    if let Some(win) = app.get_webview_window(&surface.spec.label) {
        let _ = win.hide();
        set_webview_suspended(&win, true);
    }
    if let Some(hook) = &surface.spec.on_sleep {
        hook(app);
    }
    // A stale payload must not leak into the next wake.
    surface.stash_payload(None);
}

/// Which capped workspaces must sleep so `incoming` can wake without exceeding
/// `cap`, least-recently-used first.
///
/// Pure, because this is the part with the rule. Note it returns *victims to
/// sleep*, never "refuse the open": SPEC §1.2 caps residency, not access — a
/// workspace the user just asked for always appears.
fn lru_victims(awake: &[(String, u64)], incoming: &str, cap: usize) -> Vec<String> {
    // Re-waking something already awake displaces nobody.
    let mut others: Vec<&(String, u64)> = awake.iter().filter(|(id, _)| id != incoming).collect();
    let over = (others.len() + 1).saturating_sub(cap.max(1));
    if over == 0 {
        return Vec::new();
    }
    others.sort_by_key(|(_, tick)| *tick);
    others
        .into_iter()
        .take(over)
        .map(|(id, _)| id.clone())
        .collect()
}

/// Sleep the least-recently-used capped workspaces so `incoming` fits under the
/// cap. Runs before a wake, and only for capped (i.e. extension) surfaces.
fn enforce_cap(app: &AppHandle, incoming: &Arc<Surface>) {
    if !incoming.spec.capped {
        return;
    }
    // Snapshot under the lock, then release it — `close` re-locks.
    let awake: Vec<(String, u64)> = {
        let map = surfaces().lock().unwrap();
        map.values()
            .filter(|s| s.spec.capped && s.is_awake())
            .map(|s| (s.spec.id.clone(), s.last_used.load(Ordering::SeqCst)))
            .collect()
    };
    for victim in lru_victims(&awake, &incoming.spec.id, MAX_AWAKE_CAPPED) {
        log::info!(
            "[GRAIN] surface '{}': sleeping (least recently used) to make room for '{}'",
            victim,
            incoming.spec.id
        );
        close(app, &victim);
    }
}

/// Resume the webview and ask the (purged) frontend to re-mount. The window
/// stays hidden until `ui_ready`; the fallback shows it even without an ack.
fn wake(app: &AppHandle, surface: &Arc<Surface>) {
    let Some(win) = app.get_webview_window(&surface.spec.label) else {
        return;
    };
    enforce_cap(app, surface);
    surface
        .last_used
        .store(USE_CLOCK.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
    set_webview_suspended(&win, false);
    let _ = app.emit(&surface.spec.revive_event, ());
    fallback_show(app, surface);
}

/// Show the window after `ACK_FALLBACK` even if the frontend never acked — a
/// broken UI must stay reachable (and closable).
fn fallback_show(app: &AppHandle, surface: &Arc<Surface>) {
    let app = app.clone();
    let surface = Arc::clone(surface);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(ACK_FALLBACK).await;
        if let Some(win) = app.get_webview_window(&surface.spec.label) {
            if !win.is_visible().unwrap_or(true) {
                log::warn!(
                    "[GRAIN] surface '{}': ui-ready ack timed out — showing anyway",
                    surface.spec.id
                );
                surface.awake.store(true, Ordering::SeqCst);
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
        log::warn!("[GRAIN] surface: webview suspend({suspend}) unavailable: {e}");
    }
}

#[cfg(not(windows))]
fn set_webview_suspended(_win: &tauri::WebviewWindow, _suspend: bool) {}

/// Build the window, hidden. Callers are already on the async runtime.
fn build(app: &AppHandle, surface: &Arc<Surface>) {
    // A first open is a wake too — it must respect the cap and take its place
    // in LRU order, or the newest workspace would look like the oldest.
    enforce_cap(app, surface);
    surface
        .last_used
        .store(USE_CLOCK.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
    let spec = &surface.spec;
    let mut builder = tauri::WebviewWindowBuilder::new(app, &spec.label, spec.url.clone())
        .title(&spec.title)
        .inner_size(spec.size.0, spec.size.1)
        .min_inner_size(spec.min_size.0, spec.min_size.1)
        .resizable(true)
        .decorations(spec.decorations)
        .transparent(spec.transparent)
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

    let window = match builder.build() {
        Ok(w) => w,
        Err(e) => {
            log::error!("[GRAIN] failed to build surface '{}': {e}", spec.id);
            return;
        }
    };
    fallback_show(app, surface);

    {
        let app = app.clone();
        let surface = Arc::clone(surface);
        window.on_window_event(move |event| match event {
            // OS-level close (Alt+F4 / taskbar) becomes sleep, so reopen stays
            // instant. The window is only truly destroyed on demand.
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                close(&app, &surface.spec.id);
            }
            tauri::WindowEvent::Destroyed => {
                surface.awake.store(false, Ordering::SeqCst);
                if let Some(hook) = &surface.spec.on_destroy {
                    hook(&app);
                }
                surface.stash_payload(None);
            }
            _ => {}
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str) -> WorkspaceSpec {
        WorkspaceSpec {
            id: id.into(),
            label: id.into(),
            url: tauri::WebviewUrl::App("/".into()),
            title: id.into(),
            size: (800.0, 600.0),
            min_size: (400.0, 300.0),
            sleep_event: format!("{id}://sleep"),
            revive_event: format!("{id}://revive"),
            payload_event: format!("{id}://payload"),
            decorations: false,
            transparent: true,
            capped: false,
            on_sleep: None,
            on_destroy: None,
        }
    }

    #[test]
    fn ensure_is_idempotent_and_keeps_the_live_state() {
        // Re-registering must not reset `awake` or drop a stashed payload:
        // Grain Space registers on every open, and the second one happens while
        // the window is up.
        let a = ensure(spec("test.idempotent"));
        a.awake.store(true, Ordering::SeqCst);
        a.stash_payload(Some(serde_json::json!("note-1")));

        let b = ensure(spec("test.idempotent"));
        assert!(Arc::ptr_eq(&a, &b));
        assert!(b.is_awake());
        assert_eq!(b.take_payload(), Some(serde_json::json!("note-1")));
    }

    #[test]
    fn payload_is_taken_exactly_once() {
        // The stash-then-take contract: a wake consumes the payload, and a
        // second consumer must not re-open onto a stale target.
        let s = ensure(spec("test.payload"));
        s.stash_payload(Some(serde_json::json!({ "note": "abc" })));
        assert!(s.take_payload().is_some());
        assert_eq!(s.take_payload(), None);
    }

    #[test]
    fn surfaces_do_not_share_state() {
        // The whole point of keying by id: Grain Space sleeping must not make a
        // third-party workspace believe it is asleep.
        let a = ensure(spec("test.one"));
        let b = ensure(spec("test.two"));
        a.awake.store(true, Ordering::SeqCst);
        assert!(a.is_awake());
        assert!(!b.is_awake());

        a.stash_payload(Some(serde_json::json!(1)));
        assert_eq!(b.take_payload(), None);
    }

    #[test]
    fn the_cap_sleeps_the_least_recently_used_never_the_newest() {
        // (id, last-used tick) — "b" was used most recently.
        let awake = [("a".to_string(), 1u64), ("b".to_string(), 9)];
        assert_eq!(lru_victims(&awake, "c", 1), vec!["a", "b"]);
        assert_eq!(lru_victims(&awake, "c", 2), vec!["a"]);
        assert!(lru_victims(&awake, "c", 3).is_empty());
    }

    #[test]
    fn re_waking_an_awake_workspace_displaces_nobody() {
        // Toggling back to a workspace you already have open must not sleep it
        // to make room for itself.
        let awake = [("a".to_string(), 1u64)];
        assert!(lru_victims(&awake, "a", 1).is_empty());
    }

    #[test]
    fn the_cap_never_refuses_an_open() {
        // SPEC §1.2 caps residency, not access: even at cap 0 (nonsense input)
        // the incoming workspace opens — the others just sleep.
        let awake = [("a".to_string(), 1u64)];
        let victims = lru_victims(&awake, "b", 0);
        assert_eq!(victims, vec!["a"], "the incoming surface is never a victim");
        assert!(!victims.contains(&"b".to_string()));
    }

    #[test]
    fn an_unregistered_surface_is_a_no_op_not_a_panic() {
        // Acks can arrive for a surface whose feature was disabled meanwhile.
        assert!(get("test.never-registered").is_none());
    }
}
