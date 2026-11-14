//! [GRAIN] Canvas: the Agent notch's generative-UI view (experiment, see
//! `docs/Generative UI/PLAN.md`). The model answers in OpenUI Lang and the
//! notch renders it with its own component library.
//!
//! The host keeps the canvas conversation in `agent/canvas.json` — the user's
//! requests and the UI programs the model produced — so a follow-up can refine
//! the last UI and a reopened notch shows it again. Read on demand; nothing is
//! held in memory.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use std::sync::Mutex;
use tauri::AppHandle;

use crate::agent::{AgentMessage, AgentReply};
use crate::grain_agent_history::{self as history, AgentHistoryMessage};

const FILE: &str = "canvas.json";
/// Kept turns (requests + UIs), oldest dropped first. UI programs are long,
/// so the model sees only the recent ones.
const MAX_MESSAGES: usize = 12;
/// A stored request or UI program is cut to this many characters.
const MAX_CHARS: usize = 40_000;
/// The component reference the notch sends; anything larger is refused.
const MAX_SPEC_BYTES: usize = 96 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

/// The canvas conversation, oldest first. The last assistant message is the
/// UI on screen.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct AgentCanvas {
    pub messages: Vec<AgentHistoryMessage>,
}

fn append(dir: &Path, turn: [(&str, &str); 2], at_unix_ms: u64) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = dir.join(FILE);
    let mut canvas: AgentCanvas = history::read(&path);
    canvas
        .messages
        .extend(turn.into_iter().map(|(role, content)| AgentHistoryMessage {
            role: role.into(),
            content: content.trim().chars().take(MAX_CHARS).collect(),
            at_unix_ms,
        }));
    let excess = canvas.messages.len().saturating_sub(MAX_MESSAGES);
    canvas.messages.drain(..excess);
    history::write(&path, &canvas)
}

/// Split a reply into its voice line (`SAY: …`, first line) and the UI
/// program after it. A reply without one is all UI.
pub(crate) fn split_speech(reply: &str) -> (Option<&str>, &str) {
    let text = reply.trim_start();
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let first = first.trim();
    match first.get(..4) {
        Some(tag) if tag.eq_ignore_ascii_case("say:") => {
            let speech = first[4..].trim().trim_matches('"').trim();
            ((!speech.is_empty()).then_some(speech), rest.trim())
        }
        _ => (None, reply.trim()),
    }
}

/// The canvas conversation (the last UI is its last assistant message).
#[tauri::command]
#[specta::specta]
pub fn agent_canvas(app: AppHandle) -> AgentCanvas {
    let Ok(dir) = history::dir(&app) else {
        return AgentCanvas::default();
    };
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    history::read(&dir.join(FILE))
}

/// Start the canvas over.
#[tauri::command]
#[specta::specta]
pub fn agent_canvas_clear(app: AppHandle) -> Result<(), String> {
    let dir = history::dir(&app)?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match std::fs::remove_file(dir.join(FILE)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

/// Ask for a UI. `spec` is the notch's component reference (OpenUI Lang
/// prompt generated from its library). The request and the UI are recorded
/// once the reply lands; a reply still awaiting approval is not.
#[tauri::command]
#[specta::specta]
pub async fn agent_run_canvas(
    app: AppHandle,
    request: String,
    spec: String,
    context: Option<String>,
    generation: Option<u64>,
) -> Result<AgentReply, String> {
    let request = request.trim().to_owned();
    if request.is_empty() {
        return Err("Nothing to ask".into());
    }
    if spec.len() > MAX_SPEC_BYTES {
        return Err("The canvas component reference is too large".into());
    }
    let mut messages: Vec<AgentMessage> = agent_canvas(app.clone())
        .messages
        .into_iter()
        .map(|message| AgentMessage {
            role: message.role,
            content: message.content,
        })
        .collect();
    messages.push(AgentMessage {
        role: "user".into(),
        content: request.clone(),
    });
    let mut reply =
        crate::agent::run_canvas_turn(&app, messages, context, generation, &spec).await?;
    // The voice line is said (talk-back, when on), never shown; the UI is shown,
    // never said. The memory keeps both, so the model keeps the pattern.
    let full = reply.text.clone();
    let (speech, ui) = split_speech(&full);
    if let Some(speech) = speech {
        // Not a compact spoken reply: the UI is the point, the notch opens.
        crate::agent::speak_reply(&app, speech, None);
    }
    reply.text = ui.to_owned();
    if reply.confirm_action.is_none() && !full.trim().is_empty() {
        let saved = history::dir(&app).and_then(|dir| {
            append(
                &dir,
                [("user", &request), ("assistant", &full)],
                history::now_ms(),
            )
        });
        if let Err(error) = saved {
            log::warn!("[GRAIN] agent: could not record the canvas: {error}");
        }
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_voice_line_is_split_from_the_ui() {
        let reply = "SAY: It's 14 degrees and drizzly.\nroot = Canvas([])";
        assert_eq!(
            split_speech(reply),
            (Some("It's 14 degrees and drizzly."), "root = Canvas([])")
        );
        assert_eq!(
            split_speech("  say: \"Sunny all day.\"\r\nroot = Canvas([])"),
            (Some("Sunny all day."), "root = Canvas([])")
        );
        assert_eq!(
            split_speech("root = Canvas([])"),
            (None, "root = Canvas([])")
        );
        assert_eq!(
            split_speech("SAY:\nroot = Canvas([])"),
            (None, "root = Canvas([])")
        );
        // Multi-byte text right at the tag boundary must not panic.
        assert_eq!(split_speech("Sé"), (None, "Sé"));
    }

    #[test]
    fn append_keeps_the_newest_turns() {
        let dir = std::env::temp_dir().join(format!("grain-canvas-{}", uuid::Uuid::new_v4()));
        for i in 0..10 {
            let ask = format!("ask {i}");
            let ui = format!("root = Canvas([])  // {i}");
            append(&dir, [("user", &ask), ("assistant", &ui)], i).unwrap();
        }
        let canvas: AgentCanvas = history::read(&dir.join(FILE));
        assert_eq!(canvas.messages.len(), MAX_MESSAGES);
        assert_eq!(canvas.messages[0].content, "ask 4");
        assert_eq!(canvas.messages.last().unwrap().role, "assistant");
        assert!(canvas.messages.last().unwrap().content.ends_with("// 9"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
