//! [GRAIN] Recall AI: a summary and an automatic title for one meeting,
//! written through the provider the user already configured for AI
//! cleanup (`post_process_*`).
//!
//! Nothing here runs on its own. The summary and title only run after a
//! transcript is published when their opt-in settings are on, or when the user
//! asks; with both off Recall adds no idle work, memory or network.
//!
//! Speakers stay anonymous ("Speaker 1", "Speaker 2"): the prompts forbid
//! guessing names from the conversation, so a later naming pass has a clean
//! slate to build on.

use std::fs;
use std::path::Path;
use std::time::Duration;

use log::{info, warn};
use tauri::{AppHandle, Manager};

use super::commands::{emit_status, meetings_root, state};
use super::storage;
use super::transcript::{self, TranscriptRevision};
use crate::settings::{get_settings, APPLE_INTELLIGENCE_PROVIDER_ID};
pub use grain_recall::summary::{read_summary, MeetSummaryDoc, SUMMARY_FILE};
use grain_recall::text::transcript_text;

/// Longest transcript sent to the model; a longer one keeps its start and end.
const MAX_TRANSCRIPT_CHARS: usize = 60_000;
const KEEP_HEAD_CHARS: usize = 40_000;
/// Enough of the opening to name a meeting.
const TITLE_CONTEXT_CHARS: usize = 12_000;
const MAX_TITLE_CHARS: usize = 80;
/// Titles Recall itself assigns; anything else was chosen by the user.
const DEFAULT_TITLES: [&str; 2] = ["Meeting", "Recording"];
const LLM_TIMEOUT: Duration = Duration::from_secs(180);
const NOT_READY: &str = "Set up an AI provider and model in Settings first";

const SUMMARY_PROMPT: &str = "You write meeting notes from a transcript. \
Each line is \"[time] Speaker N: text\". Speakers are anonymous: refer to them \
only as \"Speaker 1\", \"Speaker 2\" and so on, exactly as in the transcript. \
Never guess, infer or use a real name, even when someone is addressed by name. \
Write in the language of the transcript and stay faithful to it; never invent \
facts, numbers or commitments.\n\n\
Reply in Markdown with these sections, in this order:\n\
## Overview\nA short paragraph on what the meeting was about and how it ended.\n\
## Key points\nBullets of the main topics and what was said about each.\n\
## Decisions\nBullets of anything decided. Omit the section if nothing was.\n\
## Action items\nBullets as \"Speaker N: task\". Omit the section if there are none.\n\
## Who said what\nA \"### Speaker N\" heading for each speaker, with bullets of \
what they contributed.\n\n\
Length follows the meeting: a few lines for a short one, up to about two or \
three pages for a long one. No preamble, no closing remarks.";

const TITLE_PROMPT: &str = "Write a title for this meeting transcript: at most \
six words, specific to what was discussed, in the language of the transcript. \
Reply with the title only: no quotes, no trailing punctuation, no Markdown.";

// --- Transcript as plain text ----------------------------------------------

/// Keep a long transcript within the model's budget: its start and its end.
fn bounded(text: String) -> String {
    let count = text.chars().count();
    if count <= MAX_TRANSCRIPT_CHARS {
        return text;
    }
    let head: String = text.chars().take(KEEP_HEAD_CHARS).collect();
    let tail: String = text
        .chars()
        .skip(count - (MAX_TRANSCRIPT_CHARS - KEEP_HEAD_CHARS))
        .collect();
    format!("{head}\n[… the middle of the transcript is omitted for length …]\n{tail}")
}

fn load_transcript_text(dir: &Path) -> Result<(TranscriptRevision, String), String> {
    let revision = transcript::latest(dir)?.ok_or("Transcribe this recording first")?;
    let text = transcript_text(&revision);
    if text.trim().is_empty() {
        return Err("No speech was found in this recording".into());
    }
    Ok((revision, text))
}

// --- Provider ---------------------------------------------------------------

/// Whether an AI provider and model are configured and able to write text.
fn ai_ready(app: &AppHandle) -> bool {
    let settings = get_settings(app);
    settings
        .active_post_process_provider()
        .is_some_and(|provider| {
            provider.id != APPLE_INTELLIGENCE_PROVIDER_ID
                && settings
                    .post_process_models
                    .get(&provider.id)
                    .is_some_and(|model| !model.trim().is_empty())
        })
}

async fn complete(app: &AppHandle, messages: Vec<(String, String)>) -> Result<String, String> {
    let settings = get_settings(app);
    let provider = settings
        .active_post_process_provider()
        .cloned()
        .ok_or(NOT_READY)?;
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return Err(
            "Apple Intelligence cannot write meeting notes; choose another AI provider".into(),
        );
    }
    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    if model.trim().is_empty() {
        return Err(NOT_READY.into());
    }
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let client = app
        .try_state::<reqwest::Client>()
        .ok_or("The network client is unavailable")?
        .inner()
        .clone();
    // Reasoning rarely helps note-taking and slows it; same rule as cleanup.
    let (reasoning_effort, reasoning) = match provider.id.as_str() {
        "custom" => (Some("none".to_string()), None),
        "openrouter" => (
            None,
            Some(crate::llm_client::ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
        ),
        _ => (None, None),
    };
    let response = tokio::time::timeout(
        LLM_TIMEOUT,
        crate::llm_client::send_chat(
            &client,
            &provider,
            api_key,
            &model,
            messages,
            reasoning_effort,
            reasoning,
        ),
    )
    .await
    .map_err(|_| "The AI provider took too long to answer".to_string())?
    .map_err(|e| e.to_string())?;
    response
        .content
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| "The AI provider returned an empty answer".to_string())
}

// --- In-flight marker -------------------------------------------------------

/// Marks a meeting as "AI work in progress" for the status the UI watches, and
/// clears it however the work ends.
struct Busy {
    app: AppHandle,
}

impl Busy {
    fn begin(app: &AppHandle, session_id: &str) -> Result<Self, String> {
        {
            let mut guard = state();
            if guard.summarizing.is_some() {
                return Err("Meeting notes are already being written".into());
            }
            guard.summarizing = Some(session_id.to_string());
        }
        emit_status(app);
        Ok(Self { app: app.clone() })
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        state().summarizing = None;
        emit_status(&self.app);
    }
}

// --- Summary and title ------------------------------------------------------

fn write_summary(dir: &Path, doc: &MeetSummaryDoc) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{SUMMARY_FILE}.tmp"));
    fs::write(&tmp, json).map_err(|e| format!("Could not save the summary: {e}"))?;
    fs::rename(&tmp, dir.join(SUMMARY_FILE)).map_err(|e| format!("Could not save the summary: {e}"))
}

async fn write_summary_for(app: &AppHandle, dir: &Path) -> Result<MeetSummaryDoc, String> {
    let (revision, text) = load_transcript_text(dir)?;
    let text = complete(
        app,
        vec![
            ("system".into(), SUMMARY_PROMPT.into()),
            ("user".into(), bounded(text)),
        ],
    )
    .await?;
    let doc = MeetSummaryDoc {
        transcript_revision: revision.revision,
        created_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
        text,
    };
    write_summary(dir, &doc)?;
    Ok(doc)
}

/// A one-line title from the model's reply: first line, no quotes or Markdown.
fn clean_title(reply: &str) -> Option<String> {
    let line = reply.lines().map(str::trim).find(|line| !line.is_empty())?;
    let title: String = line
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '*' | '#' | '“' | '”'))
        .trim()
        .trim_end_matches(['.', '。'])
        .trim()
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect();
    (!title.is_empty()).then_some(title)
}

/// Replace a default title with one drawn from the conversation. A title the
/// user chose is never touched.
async fn title_for(app: &AppHandle, dir: &Path) -> Result<(), String> {
    let manifest = storage::load_manifest(dir)?;
    if !DEFAULT_TITLES.contains(&manifest.title.as_str()) {
        return Ok(());
    }
    let (_, text) = load_transcript_text(dir)?;
    let context: String = text.chars().take(TITLE_CONTEXT_CHARS).collect();
    let reply = complete(
        app,
        vec![
            ("system".into(), TITLE_PROMPT.into()),
            ("user".into(), context),
        ],
    )
    .await?;
    let title = clean_title(&reply).ok_or("The AI provider returned no title")?;
    // The user may have renamed it while the model was thinking.
    if DEFAULT_TITLES.contains(&storage::load_manifest(dir)?.title.as_str()) {
        storage::rename_session(dir, &title)?;
    }
    Ok(())
}

/// After a transcript is published: write the summary and/or title the user
/// opted into. Failures are logged; the meeting is already safely saved.
pub fn spawn_auto(app: AppHandle, session_id: String) {
    let settings = get_settings(&app);
    if !(settings.meet_auto_summary || settings.meet_auto_title) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if !ai_ready(&app) {
            info!("Recall auto summary skipped: no AI provider and model configured");
            return;
        }
        let Ok(root) = meetings_root(&app) else {
            return;
        };
        let Ok(dir) = storage::session_dir(&root, &session_id) else {
            return;
        };
        let Ok(_busy) = Busy::begin(&app, &session_id) else {
            return;
        };
        if settings.meet_auto_summary {
            if let Err(error) = write_summary_for(&app, &dir).await {
                warn!("Recall auto summary failed: {error}");
            }
        }
        if settings.meet_auto_title {
            if let Err(error) = title_for(&app, &dir).await {
                warn!("Recall auto title failed: {error}");
            }
        }
    });
}

// --- Commands ---------------------------------------------------------------

/// Whether an AI provider and model are configured for Recall to use.
#[tauri::command]
#[specta::specta]
pub fn meet_ai_available(app: AppHandle) -> bool {
    ai_ready(&app)
}

/// The saved summary of a meeting, if one has been written.
#[tauri::command]
#[specta::specta]
pub fn meet_summary(app: AppHandle, session_id: String) -> Result<Option<MeetSummaryDoc>, String> {
    let dir = storage::session_dir(&meetings_root(&app)?, &session_id)?;
    Ok(read_summary(&dir))
}

/// Write (or rewrite) a meeting's summary now.
#[tauri::command]
#[specta::specta]
pub async fn meet_generate_summary(
    app: AppHandle,
    session_id: String,
) -> Result<MeetSummaryDoc, String> {
    let dir = storage::session_dir(&meetings_root(&app)?, &session_id)?;
    let _busy = Busy::begin(&app, &session_id)?;
    write_summary_for(&app, &dir).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_transcripts_keep_their_start_and_end() {
        let long = format!(
            "{}{}{}",
            "a".repeat(50_000),
            "b".repeat(30_000),
            "c".repeat(20_000)
        );
        let cut = bounded(long);
        assert!(cut.starts_with('a'));
        assert!(cut.ends_with('c'));
        assert!(cut.contains("omitted"));
        assert!(cut.chars().count() < 61_000);
        assert_eq!(bounded("short".into()), "short");
    }

    #[test]
    fn titles_are_one_clean_line() {
        assert_eq!(
            clean_title("\"Quarterly Budget Review.\"\nextra").as_deref(),
            Some("Quarterly Budget Review")
        );
        assert_eq!(
            clean_title("**Roadmap sync**").as_deref(),
            Some("Roadmap sync")
        );
        assert_eq!(clean_title("  \n ").as_deref(), None);
        assert_eq!(
            clean_title(&"x".repeat(200)).unwrap().chars().count(),
            MAX_TITLE_CHARS
        );
    }
}
