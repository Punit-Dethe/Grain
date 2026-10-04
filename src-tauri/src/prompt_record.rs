//! [GRAIN] Prompt Record — split one recording into spoken CONTENT + a spoken AI
//! INSTRUCTION at the explicit shortcut mark, transcribing each independently.
//!
//! The shortcut is held only during dictation. No persistent worker is needed.
//! The per-session split mark lives on the
//! `AudioRecordingManager` (set by F8 by default); this consumes it
//! once at stop.
//!
//! Why slice the audio rather than the transcript? The mark is a sample index, so
//! the two halves transcribe as fully independent utterances — no dependence on
//! word-level timestamps and no ambiguity about which side a boundary word
//! belongs to. It costs a
//! second STT pass, but only when Prompt Record was actually used (a deliberate,
//! occasional action) — the no-mark path is byte-for-byte today's single pass.

use crate::settings::{KeyboardImplementation, ShortcutBinding};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

// Remember the actual registered chord/backend so edits cannot leak an old key.
static ACTIVE_SESSION: AtomicU64 = AtomicU64::new(0);
static EDITOR_SUSPENDED: AtomicBool = AtomicBool::new(false);
static LIVE_SHORTCUT: Mutex<Option<(ShortcutBinding, KeyboardImplementation)>> = Mutex::new(None);

pub fn shortcut_is_active() -> bool {
    ACTIVE_SESSION.load(Ordering::Acquire) != 0 && !EDITOR_SUSPENDED.load(Ordering::Acquire)
}

pub fn set_shortcut_suspended(app: &AppHandle, suspended: bool) {
    EDITOR_SUSPENDED.store(suspended, Ordering::Release);
    reconcile_shortcut(app);
}

pub fn capture_started(app: &AppHandle, session_id: u64) {
    ACTIVE_SESSION.store(session_id, Ordering::Release);
    reconcile_shortcut(app);
}

fn retire_session(active: &AtomicU64, session_id: u64) -> bool {
    session_id != 0
        && active
            .compare_exchange(session_id, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
}

/// Ignore late terminal events from an older capture.
pub fn on_event(app: &AppHandle, event: &grain_core::DaemonEvent) {
    let session_id = match event {
        grain_core::DaemonEvent::RecordingStopped { session_id }
        | grain_core::DaemonEvent::SessionCancelled { session_id }
        | grain_core::DaemonEvent::ProcessingComplete { session_id, .. } => *session_id,
        _ => return,
    };
    if retire_session(&ACTIVE_SESSION, session_id) {
        reconcile_shortcut(app);
    }
}

/// Registry calls must be deferred out of shortcut callbacks. Each job reads
/// the latest desired session, so Stop/Cancel wins over a queued startup.
pub fn reconcile_shortcut(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        {
            let mut live = LIVE_SHORTCUT.lock().unwrap();
            let cleaned = if let Some((binding, implementation)) = live.take() {
                let result = match implementation {
                    KeyboardImplementation::Tauri => {
                        // The editor/backend switch may already have removed it.
                        if binding
                            .current_binding
                            .parse::<Shortcut>()
                            .is_ok_and(|key| !app.global_shortcut().is_registered(key))
                        {
                            Ok(())
                        } else {
                            crate::shortcut::tauri_impl::unregister_shortcut(&app, binding.clone())
                        }
                    }
                    KeyboardImplementation::HandyKeys => {
                        crate::shortcut::handy_keys::unregister_shortcut(&app, binding.clone())
                    }
                };
                if let Err(error) = result {
                    log::warn!("Prompt Record shortcut cleanup: {error}");
                    *live = Some((binding, implementation));
                    false
                } else {
                    true
                }
            } else {
                true
            };
            if cleaned
                && shortcut_is_active()
                && app
                    .try_state::<Arc<crate::managers::audio::AudioRecordingManager>>()
                    .is_some_and(|manager| manager.is_recording())
            {
                let settings = crate::settings::get_settings(&app);
                if let Some(binding) = settings.bindings.get("prompt_record").cloned() {
                    let result = match settings.keyboard_implementation {
                        KeyboardImplementation::Tauri => {
                            crate::shortcut::tauri_impl::register_shortcut(&app, binding.clone())
                        }
                        KeyboardImplementation::HandyKeys => {
                            crate::shortcut::handy_keys::register_shortcut(&app, binding.clone())
                        }
                    };
                    match result {
                        Ok(()) => *live = Some((binding, settings.keyboard_implementation)),
                        Err(error) => log::warn!("Prompt Record shortcut registration: {error}"),
                    }
                }
            }
        }
        crate::secure_input::reconcile_fallback(&app);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_events_retire_only_their_own_capture_shortcut() {
        let active = AtomicU64::new(1);
        assert!(retire_session(&active, 1));
        assert_eq!(active.load(Ordering::Acquire), 0);
        assert!(!retire_session(&active, 1));
        active.store(2, Ordering::Release);
        assert!(!retire_session(&active, 1));
        assert_eq!(active.load(Ordering::Acquire), 2);
        assert!(retire_session(&active, 2));
        assert!(!retire_session(&active, 0));
    }
}

/// Transcribe `samples`, optionally splitting it at `mark` into content (before)
/// and a spoken AI instruction (after).
///
/// - No usable mark → a single pass over the whole buffer (today's behavior);
///   the instruction is `None`.
/// - Usable mark (`0 < mark < len`) → the content half is transcribed as the
///   primary result; the instruction half is transcribed best-effort (a failure
///   or empty result simply yields `None`, so the session degrades to a normal
///   dictation instead of erroring).
///
/// Both passes route through [`crate::grain_transcription::transcribe`], so they honor the
/// same local model and final-text cleanup as any other transcription.
pub async fn transcribe_split(
    app: &AppHandle,
    samples: Vec<f32>,
    mark: Option<usize>,
) -> (Result<String, String>, Option<String>) {
    match mark {
        Some(m) if m > 0 && m < samples.len() => {
            let content = samples[..m].to_vec();
            let instruction = samples[m..].to_vec();

            // Content first (this is what gets pasted / post-processed).
            let content_res = crate::grain_transcription::transcribe(app, content).await;

            // Instruction is best-effort: an error or blank result just means "no
            // spoken prompt", and the caller falls back to a normal paste.
            let spoken = crate::grain_transcription::transcribe(app, instruction)
                .await
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            (content_res, spoken)
        }
        _ => (
            crate::grain_transcription::transcribe(app, samples).await,
            None,
        ),
    }
}
