//! [GRAIN] Voice-command prompt switcher — the transient UI opened when a wake
//! gesture resolves to "switch/change prompt" (see `voice_command`). It reveals
//! the pill's existing switcher capsule and, so the user needn't reach for a
//! modifier shortcut, registers the bare arrow keys as TRANSIENT global shortcuts
//! that cycle the active prompt. The keys are released after a short idle window
//! (each arrow press re-arms it via [`bump`]).
//!
//! This owns no engine and no persistent state beyond a generation counter: the
//! capsule is drawn by the pill from `PromptChanged` events, and cycling reuses
//! [`crate::actions::cycle_prompt`]. The user's own prompt-switch shortcut keeps
//! working unchanged, so switching still functions if a keyboard backend rejects
//! a bare arrow accelerator.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tauri::AppHandle;

use crate::settings::{get_settings, ShortcutBinding};
use crate::voice_command::WakeEvent;

/// How long the switcher stays open (and the arrow keys stay grabbed) after the
/// last interaction before it auto-closes.
const IDLE_MS: u64 = 3500;

/// Binding ids — must match the `ACTION_MAP` entries in `actions.rs`.
const NEXT_ID: &str = "voice_prompt_next";
const PREV_ID: &str = "voice_prompt_prev";
/// Accelerators for the bare arrow keys. `ArrowRight`/`ArrowLeft` are the
/// `Code`-style names the global-shortcut parser expects.
const NEXT_KEY: &str = "ArrowRight";
const PREV_KEY: &str = "ArrowLeft";

/// Whether the switcher is currently open (arrows grabbed). Guards against
/// double-registration and lets [`close`] no-op when already closed.
static OPEN: AtomicBool = AtomicBool::new(false);
/// Bumped on every open / interaction; the idle-close task only fires if its
/// captured generation still matches, so a later interaction cancels an earlier
/// pending close.
static GEN: AtomicU64 = AtomicU64::new(0);

/// Drive the pill / switcher from a wake-detector transition. Shared by the
/// Rolling worker and the Native ASR stream worker so both streaming modes get
/// identical behavior: Armed → yellow; Switch → open the switcher; Record →
/// yellow off + blue (Prompt Record) on.
pub fn drive(app: &AppHandle, session_id: u64, event: WakeEvent) {
    match event {
        WakeEvent::None => {}
        WakeEvent::Armed { .. } => emit_wake(app, session_id, true),
        WakeEvent::Switch => {
            emit_wake(app, session_id, false);
            open(app);
        }
        WakeEvent::Record => {
            emit_wake(app, session_id, false);
            crate::bridge::emit(
                app,
                grain_core::DaemonEvent::PromptRecordingChanged {
                    session_id,
                    active: true,
                },
            );
        }
    }
}

fn emit_wake(app: &AppHandle, session_id: u64, active: bool) {
    crate::bridge::emit(
        app,
        grain_core::DaemonEvent::WakeListening { session_id, active },
    );
}

fn arrow_binding(id: &str, key: &str) -> ShortcutBinding {
    ShortcutBinding {
        id: id.to_string(),
        name: "Voice prompt switch".to_string(),
        description: "Cycle the active prompt while the voice switcher is open.".to_string(),
        default_binding: key.to_string(),
        current_binding: key.to_string(),
    }
}

/// Open the switcher: reveal the capsule with the current prompt name and grab
/// the arrow keys. Idempotent — a second call while open just re-arms the timer.
pub fn open(app: &AppHandle) {
    // Reveal the capsule immediately by (re)emitting the current prompt name.
    if let Some(name) = current_prompt_name(app) {
        crate::bridge::emit(app, grain_core::DaemonEvent::PromptChanged { name });
    }

    if !OPEN.swap(true, Ordering::SeqCst) {
        register_arrow(app, NEXT_ID, NEXT_KEY);
        register_arrow(app, PREV_ID, PREV_KEY);
        log::info!("[GRAIN] voice switcher opened (arrow keys grabbed)");
    }
    arm_idle_close(app);
}

/// Re-arm the idle-close timer (called on each arrow press so the switcher stays
/// open while the user keeps cycling).
pub fn bump(app: &AppHandle) {
    if OPEN.load(Ordering::SeqCst) {
        arm_idle_close(app);
    }
}

/// Close the switcher: release the arrow keys. Safe to call when already closed
/// (e.g. from the recording-stop teardown). The capsule auto-hides on its own.
pub fn close(app: &AppHandle) {
    if OPEN.swap(false, Ordering::SeqCst) {
        // Invalidate any pending idle-close task.
        GEN.fetch_add(1, Ordering::SeqCst);
        unregister_arrow(app, NEXT_ID, NEXT_KEY);
        unregister_arrow(app, PREV_ID, PREV_KEY);
        log::info!("[GRAIN] voice switcher closed (arrow keys released)");
    }
}

fn arm_idle_close(app: &AppHandle) {
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(IDLE_MS));
        // Only close if no newer interaction superseded this timer.
        if OPEN.load(Ordering::SeqCst) && GEN.load(Ordering::SeqCst) == gen {
            close(&app);
        }
    });
}

fn register_arrow(app: &AppHandle, id: &str, key: &str) {
    let binding = arrow_binding(id, key);
    // Clear a stale registration first (mirrors the agent transient pattern).
    let _ = crate::shortcut::unregister_shortcut(app, binding.clone());
    if let Err(e) = crate::shortcut::register_shortcut(app, binding) {
        // Non-fatal: the capsule still shows and the user's own prompt-switch
        // shortcut keeps working. Some keyboard backends may reject a bare arrow.
        log::warn!("[GRAIN] voice switcher: couldn't grab {key}: {e}");
    }
}

fn unregister_arrow(app: &AppHandle, id: &str, key: &str) {
    let _ = crate::shortcut::unregister_shortcut(app, arrow_binding(id, key));
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
