//! [GRAIN] Master-key chords — the deterministic mid-dictation command surface.
//!
//! While a recording session is live, two transient global chords are held:
//!
//! - **Alt+1** → Prompt Record: everything spoken from here is an AI
//!   instruction. Identical to clicking the pill — it arms the same audio-mark
//!   split on the recording manager, so it works in ALL capture modes (Batch,
//!   Rolling, Native ASR) and the existing stop-path handling needs no changes.
//! - **Alt+2** → the prompt switcher: reveals the switcher capsule and grabs the
//!   bare arrow keys to cycle the active post-processing prompt. The capsule and
//!   the arrows auto-release after a short idle window.
//!
//! A chord was chosen over a spoken wake phrase deliberately: it is
//! deterministic (no false accepts/rejects), zero-latency, needs no enrollment
//! or acoustics, costs nothing at idle, and behaves identically in every mode —
//! Batch included, which a transcript-based trigger could never serve.
//!
//! The chords are registered when a recording starts and released when it stops
//! or is cancelled (the same transient pattern as the cancel shortcut), so they
//! never shadow Alt+1/Alt+2 in other apps outside a dictation session.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tauri::AppHandle;

use crate::settings::{get_settings, ShortcutBinding};

/// How long the switcher stays open (and the arrow keys stay grabbed) after the
/// last interaction before it auto-closes.
const IDLE_MS: u64 = 3500;

// ── Master chords (active for the whole recording session) ──────────────────

/// Binding ids — must match the `ACTION_MAP` entries in `actions.rs`.
const RECORD_CHORD_ID: &str = "master_prompt_record";
const SWITCH_CHORD_ID: &str = "master_prompt_switch";
const RECORD_CHORD_KEY: &str = "alt+1";
const SWITCH_CHORD_KEY: &str = "alt+2";

/// Whether the session chords are currently registered.
static CHORDS: AtomicBool = AtomicBool::new(false);

/// Register the master chords (on recording start). Idempotent.
pub fn register_chords(app: &AppHandle) {
    if CHORDS.swap(true, Ordering::SeqCst) {
        return;
    }
    register_transient(app, RECORD_CHORD_ID, RECORD_CHORD_KEY, "Prompt Record");
    register_transient(app, SWITCH_CHORD_ID, SWITCH_CHORD_KEY, "Prompt switcher");
    log::info!("[GRAIN] master chords registered (alt+1 record, alt+2 switcher)");
}

/// Release the master chords (on recording stop/cancel). Also closes the
/// switcher if it was left open. Safe to call when not registered.
pub fn unregister_chords(app: &AppHandle) {
    close_switcher(app);
    if CHORDS.swap(false, Ordering::SeqCst) {
        unregister_transient(app, RECORD_CHORD_ID, RECORD_CHORD_KEY);
        unregister_transient(app, SWITCH_CHORD_ID, SWITCH_CHORD_KEY);
        log::info!("[GRAIN] master chords released");
    }
}

// ── Prompt switcher (opened by Alt+2, arrow keys cycle) ─────────────────────

/// Binding ids — must match the `ACTION_MAP` entries in `actions.rs`.
const NEXT_ID: &str = "switcher_prompt_next";
const PREV_ID: &str = "switcher_prompt_prev";
/// Accelerators for the bare arrow keys.
const NEXT_KEY: &str = "ArrowRight";
const PREV_KEY: &str = "ArrowLeft";

/// Whether the switcher is currently open (arrows grabbed). Guards against
/// double-registration and lets [`close_switcher`] no-op when already closed.
static OPEN: AtomicBool = AtomicBool::new(false);
/// Bumped on every open / interaction; the idle-close task only fires if its
/// captured generation still matches, so a later interaction cancels an earlier
/// pending close.
static GEN: AtomicU64 = AtomicU64::new(0);

/// Open the switcher: reveal the capsule with the current prompt name and grab
/// the arrow keys. Idempotent — a second call while open just re-arms the timer.
pub fn open_switcher(app: &AppHandle) {
    // Reveal the capsule immediately by (re)emitting the current prompt name.
    if let Some(name) = current_prompt_name(app) {
        crate::bridge::emit(app, grain_core::DaemonEvent::PromptChanged { name });
    }

    if !OPEN.swap(true, Ordering::SeqCst) {
        register_transient(app, NEXT_ID, NEXT_KEY, "Next prompt");
        register_transient(app, PREV_ID, PREV_KEY, "Previous prompt");
        log::info!("[GRAIN] prompt switcher opened (arrow keys grabbed)");
    }
    arm_idle_close(app);
}

/// Re-arm the idle-close timer (called on each arrow press so the switcher stays
/// open while the user keeps cycling).
pub fn bump_switcher(app: &AppHandle) {
    if OPEN.load(Ordering::SeqCst) {
        arm_idle_close(app);
    }
}

/// Close the switcher: release the arrow keys. Safe to call when already closed
/// (e.g. from the recording-stop teardown). The capsule auto-hides on its own.
pub fn close_switcher(app: &AppHandle) {
    if OPEN.swap(false, Ordering::SeqCst) {
        // Invalidate any pending idle-close task.
        GEN.fetch_add(1, Ordering::SeqCst);
        unregister_transient(app, NEXT_ID, NEXT_KEY);
        unregister_transient(app, PREV_ID, PREV_KEY);
        log::info!("[GRAIN] prompt switcher closed (arrow keys released)");
    }
}

fn arm_idle_close(app: &AppHandle) {
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(IDLE_MS));
        // Only close if no newer interaction superseded this timer.
        if OPEN.load(Ordering::SeqCst) && GEN.load(Ordering::SeqCst) == gen {
            close_switcher(&app);
        }
    });
}

// ── Shared transient-binding plumbing ───────────────────────────────────────

fn transient_binding(id: &str, key: &str, name: &str) -> ShortcutBinding {
    ShortcutBinding {
        id: id.to_string(),
        name: name.to_string(),
        description: "Transient in-dictation shortcut.".to_string(),
        default_binding: key.to_string(),
        current_binding: key.to_string(),
    }
}

fn register_transient(app: &AppHandle, id: &str, key: &str, name: &str) {
    let binding = transient_binding(id, key, name);
    // Clear a stale registration first (mirrors the agent transient pattern).
    let _ = crate::shortcut::unregister_shortcut(app, binding.clone());
    if let Err(e) = crate::shortcut::register_shortcut(app, binding) {
        // Non-fatal: dictation continues; only the affected chord is unavailable.
        log::warn!("[GRAIN] master key: couldn't grab {key}: {e}");
    }
}

fn unregister_transient(app: &AppHandle, id: &str, key: &str) {
    let _ = crate::shortcut::unregister_shortcut(app, transient_binding(id, key, ""));
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
