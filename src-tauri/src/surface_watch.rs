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
//! These are OS hooks: they fire when something actually changes and cost
//! nothing at all in between. No timer, no thread, no "check every N ms". They
//! live only for the length of a session.
//!
//! # Why TWO hooks
//!
//! `EVENT_SYSTEM_FOREGROUND` fires when the foreground *window* changes — which
//! a tab switch is not. Switching from GitHub to Gmail inside one browser window
//! changes nothing about which window is in front, so that hook alone made app
//! switching reliable and website switching almost never work. The cases that
//! did work were the ones that happened to cross a window boundary.
//!
//! `EVENT_OBJECT_FOCUS` covers the rest: changing tab moves focus to the new
//! tab's document. Together they answer the only question that matters — has
//! the place my text is going to land changed?
//!
//! # Why there is no timer to cancel
//!
//! A change must be *held* before it counts, or alt-tabbing past a window would
//! rewrite the pill. Rather than track and cancel timers, every event just
//! stamps the time; ONE settle task sleeps until the stamp is old enough. Focus
//! events are far too frequent to spawn a task each — a busy second would leave
//! dozens of them asleep — so the task is spawned only when none is already
//! running, and re-sleeps instead of exiting if more events arrived while it
//! waited.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tauri::AppHandle;

/// How long a surface must hold before Grain believes you meant it.
///
/// # Why windows and tabs wait the same
///
/// A window switch used to wait five seconds while a tab switch waited this, on
/// the reasoning that alt-tabbing *past* a window should not rewrite the pill.
/// But that case was already covered, and by this module's better half: every
/// event restamps [`LAST_CHANGE_MS`], so passing through a window pushes the
/// finish line out rather than settling on it. The long wait only ever fired for
/// someone who stopped on a window for over a second — which is not passing
/// through, it is arriving.
///
/// What the five seconds was quietly also buying was time for a browser to build
/// its accessibility tree. That is a retry, not a delay: it is now handled where
/// it belongs, by the one re-resolve `pill_icon` schedules when a browser has
/// not named its address yet. Paying it here charged every app switch for a
/// problem only browsers have, and still did nothing for tab switches.
const SETTLE: Duration = Duration::from_millis(1200);

/// Milliseconds (since `EPOCH`) of the most recent surface change. The settle
/// task compares against this rather than owning a deadline, so a change that
/// arrives while it is asleep simply moves the finish line.
static LAST_CHANGE_MS: AtomicU64 = AtomicU64::new(0);
/// Whether a settle task is alive. Keeps event volume and task count unrelated.
static SETTLING: AtomicBool = AtomicBool::new(false);
/// False between sessions; makes a settle in flight give up rather than repaint
/// a pill that is no longer showing.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Start of the monotonic clock these timestamps are measured against.
static EPOCH: OnceLock<Instant> = OnceLock::new();

fn now_ms() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Set once, on the first session. The hook callback is a bare `extern "system"`
/// function with nowhere to carry state, so the handle has to be reachable
/// globally — it is cloned, never mutated.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Begin following the foreground for this session.
pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    ACTIVE.store(true, Ordering::Relaxed);
    #[cfg(windows)]
    {
        // The hooks must be installed from a thread with a message pump, which
        // is the main thread; `WINEVENT_OUTOFCONTEXT` delivers through its queue.
        let _ = app.run_on_main_thread(windows_impl::install);
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

/// Stop following. Also stands down any settle still in flight, so a change made
/// during the session's tail cannot repaint the pill after the session ended.
pub fn stop(app: &AppHandle) {
    ACTIVE.store(false, Ordering::Relaxed);
    // The settle task checks ACTIVE, but resolves already in flight do not —
    // they answer to the icon path's own generation counter, so retire that too.
    crate::pill_icon::cancel_pending();
    #[cfg(windows)]
    {
        let _ = app.run_on_main_thread(windows_impl::remove);
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

/// The surface may have changed. Stamp the time; settle later.
///
/// Deliberately does almost nothing: this runs on the main thread's message
/// pump, and a UI Automation read here would stall the UI of whatever app the
/// user just switched to. Two atomics is the whole cost of an event.
fn note_change() {
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    LAST_CHANGE_MS.store(now_ms(), Ordering::Relaxed);
    // Already counting down — that task will pick up the new stamp.
    if SETTLING.swap(true, Ordering::AcqRel) {
        return;
    }
    let Some(app) = APP.get().cloned() else {
        SETTLING.store(false, Ordering::Release);
        return;
    };
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(SETTLE).await;
            if !ACTIVE.load(Ordering::Relaxed) {
                break;
            }
            let quiet_for = now_ms().saturating_sub(LAST_CHANGE_MS.load(Ordering::Relaxed));
            if quiet_for + 1 >= SETTLE.as_millis() as u64 {
                // Held long enough to mean it. Re-resolve, keeping the current
                // icon if the surface cannot be named this time.
                crate::pill_icon::refresh(&app);
                break;
            }
            // Something moved while we slept; wait out the remainder instead of
            // spawning a second task for it.
        }
        SETTLING.store(false, Ordering::Release);
    });
}

#[cfg(windows)]
mod windows_impl {
    use std::sync::atomic::{AtomicIsize, Ordering};

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND, OBJID_CLIENT, WINEVENT_OUTOFCONTEXT,
        WINEVENT_SKIPOWNPROCESS,
    };

    /// The live hooks, or 0. Raw handles so install/remove are a few atomics
    /// rather than another lock on the main thread's path.
    ///
    /// Two, because the events are not adjacent: `EVENT_SYSTEM_FOREGROUND`
    /// (0x0003) and `EVENT_OBJECT_FOCUS` (0x8005) are far apart, and one hook
    /// spanning both would also deliver everything in between.
    static FOREGROUND_HOOK: AtomicIsize = AtomicIsize::new(0);
    static FOCUS_HOOK: AtomicIsize = AtomicIsize::new(0);

    // SKIPOWNPROCESS: Grain's own windows coming forward is not the user
    // choosing a paste target.
    const FLAGS: u32 = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;

    pub fn install() {
        hook_one(&FOREGROUND_HOOK, EVENT_SYSTEM_FOREGROUND, Some(on_event));
        hook_one(&FOCUS_HOOK, EVENT_OBJECT_FOCUS, Some(on_focus));
    }

    fn hook_one(
        slot: &AtomicIsize,
        event: u32,
        proc: windows::Win32::UI::Accessibility::WINEVENTPROC,
    ) {
        if slot.load(Ordering::Relaxed) != 0 {
            return; // already following
        }
        // SAFETY: a single-event range, no module (out-of-context hooks take a
        // function pointer in this process), and no process/thread filter.
        let hook = unsafe { SetWinEventHook(event, event, None, proc, 0, 0, FLAGS) };
        slot.store(hook.0 as isize, Ordering::Relaxed);
    }

    pub fn remove() {
        for slot in [&FOREGROUND_HOOK, &FOCUS_HOOK] {
            let raw = slot.swap(0, Ordering::Relaxed);
            if raw == 0 {
                continue;
            }
            // SAFETY: a handle this module installed and has not yet freed — the
            // swap guarantees exactly one caller sees a non-zero value.
            unsafe {
                let _ = UnhookWinEvent(HWINEVENTHOOK(raw as *mut std::ffi::c_void));
            }
        }
    }

    /// Must return promptly: this runs on the main thread's message pump.
    unsafe extern "system" fn on_event(
        _hook: HWINEVENTHOOK,
        _event: u32,
        _hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _thread: u32,
        _time: u32,
    ) {
        super::note_change();
    }

    /// Focus, filtered to the window's own client area.
    ///
    /// Focus events also fire for carets, menu items and scrollbars; `OBJID_CLIENT`
    /// keeps this to "something in the content took focus", which is what a tab
    /// switch looks like, and drops a good deal of noise before it reaches the
    /// settle logic.
    unsafe extern "system" fn on_focus(
        _hook: HWINEVENTHOOK,
        _event: u32,
        _hwnd: HWND,
        id_object: i32,
        _id_child: i32,
        _thread: u32,
        _time: u32,
    ) {
        if id_object != OBJID_CLIENT.0 {
            return;
        }
        super::note_change();
    }
}
