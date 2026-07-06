//! [GRAIN] Grain Space capture inputs.
//!
//! - **Input C (quick add):** tap shortcut → grab the current selection via the
//!   Agent's invisible-copy mechanism → save silently as a raw note.
//! - **Inputs A/B (voice):** the `grain_space_capture` binding records through
//!   the ordinary transcription pipeline; `actions.rs` hands the finished
//!   transcript to [`intake_transcript`] instead of pasting it. With a usable
//!   BYOM provider (Input A) one structured LLM call extracts title/TLDR/
//!   todos/reminder; on ANY failure it degrades to Input B (raw save) — a
//!   capture must never lose the user's words.

use std::sync::atomic::{AtomicI64, Ordering};

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use super::store::{self, Note, ReminderState, ReminderStatus, TodoTag};
use crate::settings::{get_settings, AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};

/// Input C debounce: OS key-repeat / double taps within this window are one add.
const QUICK_ADD_DEBOUNCE_MS: i64 = 500;
static LAST_QUICK_ADD_MS: AtomicI64 = AtomicI64::new(0);

/// Input C: capture the highlighted text and save it silently as a raw note.
/// Runs the whole capture off the input thread (the selection grab sleeps/polls
/// the clipboard). Empty selection ⇒ silent no-op — never an empty note.
pub fn quick_add(app: &AppHandle) {
    if !super::is_enabled(app) {
        return;
    }
    let now = chrono::Utc::now().timestamp_millis();
    let last = LAST_QUICK_ADD_MS.swap(now, Ordering::Relaxed);
    if now - last < QUICK_ADD_DEBOUNCE_MS {
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let Some(selection) = crate::agent::capture_selection(&app) else {
            log::debug!("[GRAIN] space quick-add: no selection captured; ignoring");
            return;
        };
        let base = match super::base_dir(&app) {
            Ok(b) => b,
            Err(e) => {
                log::error!("[GRAIN] space quick-add: {e}");
                return;
            }
        };
        let note = Note::raw(selection);
        match store::save_note(&base, &note) {
            Ok(()) => {
                log::info!("[GRAIN] space quick-add: saved note {}", note.id);
                super::emit_notes_changed(&app);
            }
            Err(e) => log::error!("[GRAIN] space quick-add: save failed: {e:#}"),
        }
    });
}

/// Inputs A/B: store a finished dictation transcript as a note. Called from the
/// `grain_space_capture` interception point in `actions.rs` (already off the
/// input thread, inside the transcription task).
pub async fn intake_transcript(app: &AppHandle, transcript: String) {
    let text = transcript.trim();
    if text.is_empty() {
        return;
    }
    let base = match super::base_dir(app) {
        Ok(b) => b,
        Err(e) => {
            log::error!("[GRAIN] space capture: {e}");
            return;
        }
    };

    let mut note = Note::raw(text.to_string());

    // Input A: one structured extraction call when a usable provider exists.
    // The body always stays the verbatim transcript — the LLM only supplies
    // metadata, so a bad completion can't corrupt what the user said.
    let settings = get_settings(app);
    if llm_usable(&settings) {
        match extract_metadata(app, &settings, text).await {
            Ok(meta) => meta.apply(&mut note, settings.grain_space_auto_reminders),
            Err(e) => {
                log::warn!("[GRAIN] space capture: extraction failed ({e}); saving raw (Input B)")
            }
        }
    }

    let app2 = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || store::save_note(&base, &note)).await;
    match result {
        Ok(Ok(())) => {
            super::emit_notes_changed(&app2);
            // Input A may have armed a reminder.
            super::reminders::sync(&app2);
        }
        Ok(Err(e)) => log::error!("[GRAIN] space capture: save failed: {e:#}"),
        Err(e) => log::error!("[GRAIN] space capture: save task panicked: {e}"),
    }
}

/// Input A is available iff post-processing has an HTTP provider with a model.
/// Apple Intelligence (no OpenAI-style structured output path here) degrades to
/// Input B for now.
fn llm_usable(settings: &AppSettings) -> bool {
    if !settings.post_process_enabled {
        return false;
    }
    let Some(provider) = settings.active_post_process_provider() else {
        return false;
    };
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return false;
    }
    settings
        .post_process_models
        .get(&provider.id)
        .map(|m| !m.trim().is_empty())
        .unwrap_or(false)
}

/// The structured-output shape for the extraction call.
#[derive(Deserialize, Debug)]
struct ExtractedMeta {
    title: String,
    tldr: String,
    #[serde(default)]
    todos: Vec<String>,
    /// Local time "YYYY-MM-DDTHH:MM"; empty string when the note has no
    /// reminder/timer.
    #[serde(default)]
    reminder_at: String,
}

impl ExtractedMeta {
    fn apply(self, note: &mut Note, auto_arm: bool) {
        note.title = self.title.trim().to_string();
        note.tldr = self.tldr.trim().to_string();
        note.todo_tags = self
            .todos
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .map(|text| TodoTag { text, done: false })
            .collect();

        let fire_at = parse_local_datetime_ms(self.reminder_at.trim());
        note.reminder_state = match fire_at {
            Some(ms) => ReminderState {
                // Auto-reminders off ⇒ extracted but not armed; the note pane
                // (Phase 3) offers a manual "arm" button.
                status: if auto_arm {
                    ReminderStatus::Armed
                } else {
                    ReminderStatus::Pending
                },
                fire_at: Some(ms),
            },
            None => ReminderState::default(),
        };
    }
}

/// "YYYY-MM-DDTHH:MM" (local wall clock, as instructed in the prompt) → epoch ms.
fn parse_local_datetime_ms(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()?;
    use chrono::TimeZone;
    match chrono::Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.timestamp_millis()),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.timestamp_millis()),
        chrono::LocalResult::None => None,
    }
}

/// One structured chat-completion against the active post-process provider.
async fn extract_metadata(
    app: &AppHandle,
    settings: &AppSettings,
    transcript: &str,
) -> Result<ExtractedMeta, String> {
    let provider = settings
        .active_post_process_provider()
        .cloned()
        .ok_or("no active provider")?;
    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let client = app
        .try_state::<reqwest::Client>()
        .map(|s| s.inner().clone())
        .ok_or("shared HTTP client unavailable")?;

    let now_local = chrono::Local::now().format("%A %Y-%m-%dT%H:%M").to_string();
    let system_prompt = format!(
        "You extract metadata from a spoken personal note. Generate a 3-word title, \
         a 1-sentence TLDR, and extract reminders/timers. Reply with JSON only.\n\
         Rules:\n\
         - title: at most 3 words, plain text.\n\
         - tldr: exactly one short sentence.\n\
         - todos: action items explicitly present in the note (empty array if none).\n\
         - reminder_at: if the note asks for a reminder/timer, the local datetime \
           it should fire, formatted YYYY-MM-DDTHH:MM; otherwise an empty string. \
           The current local datetime is {now_local}.\n\
         Never rewrite or summarize away the note itself — you only produce metadata."
    );

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "tldr": { "type": "string" },
            "todos": { "type": "array", "items": { "type": "string" } },
            "reminder_at": { "type": "string" }
        },
        "required": ["title", "tldr", "todos", "reminder_at"],
        "additionalProperties": false
    });

    let success = crate::llm_client::send_chat_completion_with_schema(
        &client,
        &provider,
        api_key,
        &model,
        transcript.to_string(),
        Some(system_prompt),
        Some(schema),
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    let content = success.content.ok_or("empty completion")?;
    let meta: ExtractedMeta =
        serde_json::from_str(strip_code_fences(&content)).map_err(|e| e.to_string())?;
    crate::post_process_router::record_usage(app, &provider.id);
    Ok(meta)
}

/// Some models fence JSON in ```json blocks even under structured output.
fn strip_code_fences(s: &str) -> &str {
    let t = s.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    t.strip_suffix("```").unwrap_or(t).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_apply_arms_or_parks_reminder() {
        let meta = |auto: bool| {
            let m = ExtractedMeta {
                title: " Wifi Note ".into(),
                tldr: "The wifi password.".into(),
                todos: vec!["buy milk".into(), "  ".into()],
                reminder_at: "2026-07-06T18:30".into(),
            };
            let mut note = Note::raw("body".into());
            m.apply(&mut note, auto);
            note
        };

        let armed = meta(true);
        assert_eq!(armed.title, "Wifi Note");
        assert_eq!(armed.todo_tags.len(), 1);
        assert_eq!(armed.reminder_state.status, ReminderStatus::Armed);
        assert!(armed.reminder_state.fire_at.is_some());
        // Body is never touched by extraction.
        assert_eq!(armed.body, "body");

        let parked = meta(false);
        assert_eq!(parked.reminder_state.status, ReminderStatus::Pending);
    }

    #[test]
    fn bad_reminder_string_means_no_reminder() {
        let m = ExtractedMeta {
            title: "T".into(),
            tldr: "S".into(),
            todos: vec![],
            reminder_at: "tomorrow evening".into(),
        };
        let mut note = Note::raw("b".into());
        m.apply(&mut note, true);
        assert_eq!(note.reminder_state.status, ReminderStatus::None);
        assert!(note.reminder_state.fire_at.is_none());
    }

    #[test]
    fn code_fences_are_stripped() {
        assert_eq!(strip_code_fences("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fences("{\"a\":1}"), "{\"a\":1}");
    }
}
