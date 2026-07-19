//! [GRAIN] Master-key chords — the deterministic mid-dictation command surface.
//!
//! While a recording session is live, two transient global chords are held:
//!
//! - **Alt+1** → Prompt Record: everything spoken from here is an AI
//!   instruction. Identical to clicking the pill — it arms the same audio-mark
//!   split on the recording manager, so it works in ALL capture modes (Batch,
//!   Rolling, Native ASR) and the existing stop-path handling needs no changes.
//! - **Alt+2** → the prompt switcher: reveals the switcher capsule and grabs
//!   **A** (previous) / **D** (next) to cycle the active post-processing prompt.
//!   A/D were chosen over arrow keys deliberately — arrows are awkward or
//!   absent on many compact laptop keyboards. The capsule and keys auto-release
//!   after a short idle window.
//!
//! The chords are registered when a recording starts and released when it stops
//! or is cancelled (the same transient pattern as the cancel shortcut), so they
//! never shadow these keys in other apps outside a dictation session.
//!
//! ## Threading rule (the hard-won part)
//!
//! Shortcut actions are dispatched FROM the keyboard backend's own thread. On
//! the handy-keys backend that is the manager thread which ALSO serves
//! register/unregister commands — calling `register_shortcut` synchronously
//! from inside an action therefore blocks the manager thread waiting for
//! itself, deadlocking EVERY global shortcut in the app (observed live:
//! Escape, stop, everything dead). The Tauri backend has an equivalent
//! re-entrancy hazard. So this module NEVER touches the shortcut plugin on the
//! calling thread: every register/unregister is deferred to the async runtime,
//! exactly like `register_cancel_shortcut` (see its "avoid deadlock" comment).
//! The `CHORDS`/`OPEN` statics flip synchronously as the source of truth; each
//! deferred registration re-checks its flag afterwards and rolls itself back if
//! the state flipped while it was in flight, so out-of-order task execution
//! can never leak a grabbed key.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tauri::AppHandle;

use crate::settings::{get_settings, ShortcutBinding};

/// How long the switcher stays open (and A/D stay grabbed) after the last
/// interaction before it auto-closes.
const IDLE_MS: u64 = 3500;

// ── Master chords (active for the whole recording session) ──────────────────

/// Binding ids — must match the `ACTION_MAP` entries in `actions.rs`.
const RECORD_CHORD_ID: &str = "master_prompt_record";
const SWITCH_CHORD_ID: &str = "master_prompt_switch";
const RECORD_CHORD_KEY: &str = "alt+1";
const SWITCH_CHORD_KEY: &str = "alt+2";

/// Whether the session chords should currently be registered (source of truth;
/// the actual plugin calls are deferred).
static CHORDS: AtomicBool = AtomicBool::new(false);

/// Register the master chords (on recording start). Idempotent, and safe to
/// call from a shortcut action: the plugin work happens on the async runtime.
pub fn register_chords(app: &AppHandle) {
    if CHORDS.swap(true, Ordering::SeqCst) {
        return;
    }
    deferred_register(
        app,
        RECORD_CHORD_ID,
        RECORD_CHORD_KEY,
        "Prompt Record",
        &CHORDS,
    );
    deferred_register(
        app,
        SWITCH_CHORD_ID,
        SWITCH_CHORD_KEY,
        "Prompt switcher",
        &CHORDS,
    );
}

/// Release the master chords (on recording stop/cancel). Also closes the
/// switcher if it was left open. Idempotent; plugin work is deferred.
pub fn unregister_chords(app: &AppHandle) {
    close_switcher(app);
    if CHORDS.swap(false, Ordering::SeqCst) {
        deferred_unregister(app, RECORD_CHORD_ID, RECORD_CHORD_KEY);
        deferred_unregister(app, SWITCH_CHORD_ID, SWITCH_CHORD_KEY);
    }
}

// ── Prompt switcher (opened by Alt+2; A/D cycle) ────────────────────────────

/// Binding ids — must match the `ACTION_MAP` entries in `actions.rs`.
const NEXT_ID: &str = "switcher_prompt_next";
const PREV_ID: &str = "switcher_prompt_prev";
/// Cycle keys. Letters, not arrows: reachable on every keyboard layout, and
/// the grab only exists while the switcher is open mid-recording (the user is
/// speaking, not typing).
const NEXT_KEY: &str = "d";
const PREV_KEY: &str = "a";

/// Whether the switcher should currently be open (A/D grabbed).
static OPEN: AtomicBool = AtomicBool::new(false);
/// Bumped on every open / interaction; the idle-close task only fires if its
/// captured generation still matches, so a later interaction cancels an earlier
/// pending close.
static GEN: AtomicU64 = AtomicU64::new(0);

/// Open the switcher: reveal the capsule with the current prompt name and grab
/// A/D. Idempotent — a second call while open just re-arms the idle timer.
/// Safe to call from a shortcut action (plugin work deferred).
pub fn open_switcher(app: &AppHandle) {
    // Reveal the capsule immediately by (re)emitting the current prompt name —
    // instant visual feedback even while the key grabs are still in flight.
    if let Some(name) = current_prompt_name(app) {
        crate::bridge::emit(app, grain_core::DaemonEvent::PromptChanged { name });
    }

    if !OPEN.swap(true, Ordering::SeqCst) {
        deferred_register(app, NEXT_ID, NEXT_KEY, "Next prompt", &OPEN);
        deferred_register(app, PREV_ID, PREV_KEY, "Previous prompt", &OPEN);
        log::info!("[GRAIN] prompt switcher opening (A/D grabs queued)");
    }
    arm_idle_close(app);
}

/// Re-arm the idle-close timer (called on each A/D press so the switcher stays
/// open while the user keeps cycling).
pub fn bump_switcher(app: &AppHandle) {
    if OPEN.load(Ordering::SeqCst) {
        arm_idle_close(app);
    }
}

/// Close the switcher: release A/D. Safe to call when already closed and from
/// any thread (plugin work deferred). The capsule auto-hides on its own.
pub fn close_switcher(app: &AppHandle) {
    if OPEN.swap(false, Ordering::SeqCst) {
        // Invalidate any pending idle-close task.
        GEN.fetch_add(1, Ordering::SeqCst);
        deferred_unregister(app, NEXT_ID, NEXT_KEY);
        deferred_unregister(app, PREV_ID, PREV_KEY);
        log::info!("[GRAIN] prompt switcher closed (A/D releases queued)");
    }
}

fn arm_idle_close(app: &AppHandle) {
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(IDLE_MS)).await;
        // Only close if no newer interaction superseded this timer.
        if OPEN.load(Ordering::SeqCst) && GEN.load(Ordering::SeqCst) == gen {
            close_switcher(&app);
        }
    });
}

// ── Deferred plugin plumbing ────────────────────────────────────────────────

fn transient_binding(id: &str, key: &str, name: &str) -> ShortcutBinding {
    ShortcutBinding {
        id: id.to_string(),
        name: name.to_string(),
        description: "Transient in-dictation shortcut.".to_string(),
        default_binding: key.to_string(),
        current_binding: key.to_string(),
    }
}

/// Register `binding` off the dispatch thread. After registering, re-check
/// `guard`: if the owning state flipped off while this task was queued (a
/// stop/cancel raced us), immediately release the key so nothing stays grabbed.
fn deferred_register(app: &AppHandle, id: &str, key: &str, name: &str, guard: &'static AtomicBool) {
    let app = app.clone();
    let binding = transient_binding(id, key, name);
    tauri::async_runtime::spawn(async move {
        match crate::shortcut::register_shortcut(&app, binding.clone()) {
            Ok(()) => {
                if !guard.load(Ordering::SeqCst) {
                    // State flipped while we were in flight — roll back.
                    let _ = crate::shortcut::unregister_shortcut(&app, binding);
                }
            }
            // Non-fatal: dictation continues; only this chord is unavailable.
            Err(e) => log::warn!(
                "[GRAIN] master key: couldn't grab '{}': {e}",
                binding.current_binding
            ),
        }
    });
}

/// Unregister `binding` off the dispatch thread. Failures are expected when the
/// key was never grabbed (e.g. its registration failed) and are ignored.
fn deferred_unregister(app: &AppHandle, id: &str, key: &str) {
    let app = app.clone();
    let binding = transient_binding(id, key, "");
    tauri::async_runtime::spawn(async move {
        let _ = crate::shortcut::unregister_shortcut(&app, binding);
    });
}

/// The display name of the currently-selected post-processing prompt, if any.
fn current_prompt_name(app: &AppHandle) -> Option<String> {
    let settings = get_settings(app);
    let id = settings.post_process_selected_prompt_id.as_deref()?;
    settings
        .post_process_prompts
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
}
