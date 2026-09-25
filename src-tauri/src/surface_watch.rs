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
//! nothing at all in between. No polling timer or dedicated thread. They live
//! only for the length of a session.
//!
//! # Why foreground and focus hooks
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
//! Windows also sends `EVENT_SYSTEM_SWITCHSTART` and `SWITCHEND` around Alt+Tab.
//! The switcher is a temporary shell surface, not the paste target. Hold the
//! last icon while it is open and resolve once when the switch completes (or is
//! cancelled), without delaying ordinary window or tab changes.
//!
//! # Immediate updates without a worker per event
//!
//! The hook only bumps a sequence number and starts a worker if none is running.
//! That worker resolves the foreground immediately, then checks the sequence
//! again so a switch made during resolution is not lost. No timer delays a
//! change, and bursts of focus events do not create a thread for each event.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

use tauri::AppHandle;

/// Incremented by each surface event, including events arriving during a read.
static CHANGE_SEQ: AtomicU64 = AtomicU64::new(0);
/// At most one foreground resolver runs at a time.
static RESOLVING: AtomicBool = AtomicBool::new(false);
/// False between sessions; makes a resolver in flight give up rather than repaint
/// a pill that is no longer showing.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// The Windows task switcher is in front. Its shell icon is never a paste target.
static SWITCHING: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_switching() -> bool {
    SWITCHING.load(Ordering::Acquire)
}

/// Set once, on the first session. The hook callback is a bare `extern "system"`
/// function with nowhere to carry state, so the handle has to be reachable
/// globally — it is cloned, never mutated.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Begin following the foreground for this session.
pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    SWITCHING.store(false, Ordering::Release);
    ACTIVE.store(true, Ordering::Release);
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

/// Stop following. Also stands down any resolver still in flight, so a change made
/// during the session's tail cannot repaint the pill after the session ended.
pub fn stop(app: &AppHandle) {
    ACTIVE.store(false, Ordering::Release);
    SWITCHING.store(false, Ordering::Release);
    // The worker checks ACTIVE, but resolves already in flight do not —
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

/// The surface may have changed. Resolve it on a worker right away.
///
/// Deliberately does almost nothing: this runs on the main thread's message
/// pump, and a UI Automation read here would stall the UI of whatever app the
/// user just switched to. Two atomics are the whole cost of an event.
fn note_change() {
    if !ACTIVE.load(Ordering::Acquire) || is_switching() {
        return;
    }
    CHANGE_SEQ.fetch_add(1, Ordering::AcqRel);
    // An in-flight read will pick up the new sequence when it finishes.
    if RESOLVING.swap(true, Ordering::AcqRel) {
        return;
    }
    let Some(app) = APP.get().cloned() else {
        RESOLVING.store(false, Ordering::Release);
        return;
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut observed = CHANGE_SEQ.load(Ordering::Acquire);
        loop {
            if ACTIVE.load(Ordering::Acquire) && !is_switching() {
                observed = CHANGE_SEQ.load(Ordering::Acquire);
                // Keep the current icon if this one read cannot name the surface.
                if !is_switching() {
                    crate::pill_icon::refresh(&app);
                }
                let changed = CHANGE_SEQ.load(Ordering::Acquire) != observed;
                if ACTIVE.load(Ordering::Acquire) && !is_switching() && changed {
                    continue;
                }
            }

            // Release before checking again: an event racing with this exit
            // either starts its own worker or is picked up by this one.
            RESOLVING.store(false, Ordering::Release);
            if !ACTIVE.load(Ordering::Acquire)
                || is_switching()
                || CHANGE_SEQ.load(Ordering::Acquire) == observed
                || RESOLVING.swap(true, Ordering::AcqRel)
            {
                break;
            }
        }
    });
}

/// Suspend only icon resolution; the current pixels remain on the pill.
#[cfg(windows)]
fn switch_started() {
    if !ACTIVE.load(Ordering::Acquire) {
        return;
    }
    SWITCHING.store(true, Ordering::Release);
    CHANGE_SEQ.fetch_add(1, Ordering::AcqRel);
    crate::pill_icon::cancel_pending();
}

/// Also refreshes when Windows sends SWITCHEND without SWITCHSTART.
#[cfg(windows)]
fn switch_ended() {
    // Retire any read that slipped in while the switcher was opening. Only the
    // fresh read below may publish pixels after the switcher closes.
    crate::pill_icon::cancel_pending();
    SWITCHING.store(false, Ordering::Release);
    note_change();
}

#[cfg(windows)]
mod windows_impl {
    use std::sync::atomic::{AtomicIsize, Ordering};

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_SWITCHEND,
        EVENT_SYSTEM_SWITCHSTART, OBJID_CLIENT, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    };

    /// The live hooks, or 0. Raw handles so install/remove are a few atomics
    /// rather than another lock on the main thread's path.
    ///
    /// Foreground and focus need separate hooks. The two switch events are
    /// adjacent, so one range covers both without subscribing to other events.
    static FOREGROUND_HOOK: AtomicIsize = AtomicIsize::new(0);
    static FOCUS_HOOK: AtomicIsize = AtomicIsize::new(0);
    static SWITCH_HOOK: AtomicIsize = AtomicIsize::new(0);

    // SKIPOWNPROCESS: Grain's own windows coming forward is not the user
    // choosing a paste target.
    const FLAGS: u32 = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;

    pub fn install() {
        hook(
            &SWITCH_HOOK,
            EVENT_SYSTEM_SWITCHSTART,
            EVENT_SYSTEM_SWITCHEND,
            Some(on_switch),
        );
        hook(
            &FOREGROUND_HOOK,
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            Some(on_event),
        );
        hook(
            &FOCUS_HOOK,
            EVENT_OBJECT_FOCUS,
            EVENT_OBJECT_FOCUS,
            Some(on_focus),
        );
    }

    fn hook(
        slot: &AtomicIsize,
        event_min: u32,
        event_max: u32,
        proc: windows::Win32::UI::Accessibility::WINEVENTPROC,
    ) {
        if slot.load(Ordering::Relaxed) != 0 {
            return; // already following
        }
        // SAFETY: these are one event or an adjacent pair, with no module
        // (out-of-context hooks take a function pointer in this process) and no
        // process/thread filter.
        let hook = unsafe { SetWinEventHook(event_min, event_max, None, proc, 0, 0, FLAGS) };
        slot.store(hook.0 as isize, Ordering::Relaxed);
    }

    pub fn remove() {
        for slot in [&FOREGROUND_HOOK, &FOCUS_HOOK, &SWITCH_HOOK] {
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

    /// The switcher is transient. Leave the last icon in place until END gives
    /// us the selected window; cancelling a switch is handled the same way.
    unsafe extern "system" fn on_switch(
        _hook: HWINEVENTHOOK,
        event: u32,
        _hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _thread: u32,
        _time: u32,
    ) {
        match event {
            EVENT_SYSTEM_SWITCHSTART => super::switch_started(),
            EVENT_SYSTEM_SWITCHEND => super::switch_ended(),
            _ => {}
        }
    }

    /// Focus, filtered to the window's own client area.
    ///
    /// Focus events also fire for carets, menu items and scrollbars; `OBJID_CLIENT`
    /// keeps this to "something in the content took focus", which is what a tab
    /// switch looks like, and drops a good deal of noise before resolution.
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
