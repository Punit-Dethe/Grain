//! [GRAIN] Surface watch — follow the foreground app *during* a session.
//!
//! People switch windows mid-dictation. The app you settle on is the one the
//! text lands in, so the pill should end up showing that app's icon rather than
//! whichever window happened to be in front when you pressed the key.
//!
//! **The post-processing context needs no help from this.** It is resolved in
//! `grain_post_process` at paste time, which is already after every switch — so
//! the whole transcript is treated as belonging to the final surface without
//! anything here being involved. This module exists purely so the PILL agrees
//! with what post-processing is going to do, in time for you to notice if it
//! picked the wrong window.
//!
//! # Why there is no polling
//!
//! `EVENT_SYSTEM_FOREGROUND` is an OS hook: it fires when the foreground window
//! actually changes and costs nothing at all in between. No timer, no thread, no
//! "check every N ms". The hook lives only for the length of a session.
//!
//! # Why there is no timer to cancel
//!
//! A switch must be *held* before it counts, or alt-tabbing past a window would
//! rewrite the pill. Rather than track and cancel pending timers, each switch
//! bumps a generation counter and spawns a task that sleeps and then checks
//! whether it is still the newest. Switch five times quickly and four tasks die
//! on a single atomic load. Nothing needs cleaning up, and there is no state
//! machine to get wrong.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use tauri::AppHandle;

/// How long a window must stay in front before Grain believes you meant it.
///
/// Long enough to ignore passing through a window on the way to another; short
/// enough that the icon confirms the target before a sentence ends. It also
/// gives Gecko time to build its accessibility tree, which it does lazily — a
/// browser that answers nothing on the first read often answers on this one.
const SETTLE: Duration = Duration::from_secs(5);

/// Bumped by every foreground change. A settle task adopts its switch only if it
/// is still the newest one, so superseded switches need no cancellation.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Set once, on the first session. The hook callback is a bare `extern "system"`
/// function with nowhere to carry state, so the handle has to be reachable
/// globally — it is cloned, never mutated.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Begin following the foreground for this session.
pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    #[cfg(windows)]
    {
        // The hook must be installed from a thread with a message pump, which is
        // the main thread; `WINEVENT_OUTOFCONTEXT` delivers through its queue.
        let _ = app.run_on_main_thread(windows_impl::install);
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

/// Stop following. Also invalidates any settle still in flight, so a switch made
/// during the session's tail cannot repaint the pill after the session ended.
pub fn stop(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::Relaxed);
    #[cfg(windows)]
    {
        let _ = app.run_on_main_thread(windows_impl::remove);
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

/// A foreground change arrived. Record it and check back once, later.
///
/// Deliberately does no work now: this runs on the main thread's message pump,
/// and a UI Automation read there would stall the UI of whatever app the user
/// just switched to.
fn schedule_settle() {
    let Some(app) = APP.get().cloned() else {
        return;
    };
    let mine = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SETTLE).await;
        // Superseded by a later switch (or by the session ending) → this window
        // was passed through, not settled on.
        if GENERATION.load(Ordering::Relaxed) != mine {
            return;
        }
        // Re-runs the same resolution session start does, so a site icon, a
        // cache miss and the async fetch all behave identically here.
        crate::pill_icon::emit_for_session(&app);
    });
}

#[cfg(windows)]
mod windows_impl {
    use std::sync::atomic::{AtomicIsize, Ordering};

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    };

    /// The live hook, or 0. Kept as a raw handle so install/remove are a pair of
    /// atomics rather than another lock on the main thread's path.
    static HOOK: AtomicIsize = AtomicIsize::new(0);

    pub fn install() {
        if HOOK.load(Ordering::Relaxed) != 0 {
            return; // already following
        }
        // SAFETY: a foreground-only range, no module (out-of-context hooks take
        // a function pointer in this process), and no process/thread filter.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(on_foreground),
                0,
                0,
                // SKIPOWNPROCESS: Grain's own windows coming forward is not the
                // user choosing a paste target.
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        HOOK.store(hook.0 as isize, Ordering::Relaxed);
    }

    pub fn remove() {
        let raw = HOOK.swap(0, Ordering::Relaxed);
        if raw == 0 {
            return;
        }
        // SAFETY: `raw` is a handle this module installed and has not yet freed —
        // the swap above guarantees exactly one caller sees a non-zero value.
        unsafe {
            let _ = UnhookWinEvent(HWINEVENTHOOK(raw as *mut std::ffi::c_void));
        }
    }

    /// Must return promptly: this runs on the main thread's message pump.
    unsafe extern "system" fn on_foreground(
        _hook: HWINEVENTHOOK,
        _event: u32,
        _hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _thread: u32,
        _time: u32,
    ) {
        super::schedule_settle();
    }
}
