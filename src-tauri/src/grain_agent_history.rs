//! [GRAIN] The Agent notch's memory: ONE running chat and the Chat/Recall mode
//! the notch last showed. Deliberately rudimentary (to be replaced by real
//! multi-topic history): only what was said is kept — the user's words and the
//! assistant's final replies. Tool calls, approvals and selections are not.
//!
//! One small JSON file, read on demand; nothing is held in memory.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;

use crate::agent::AgentMessage;

const FILE: &str = "notch.json";
/// The stored chat is trimmed to this many messages, oldest first.
const MAX_MESSAGES: usize = 200;
/// A single stored message is cut to this many characters.
const MAX_CHARS: usize = 20_000;

/// Serialises read-modify-write of the file.
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum AgentNotchMode {
    #[default]
    Chat,
    Recall,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentHistoryMessage {
    /// `"user"` or `"assistant"`.
    pub role: String,
    pub content: String,
    pub at_unix_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct AgentNotchMemory {
    #[serde(default)]
    pub mode: AgentNotchMode,
    #[serde(default)]
    pub messages: Vec<AgentHistoryMessage>,
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::portable::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("agent")
        .join(FILE))
}

fn load(app: &AppHandle) -> AgentNotchMemory {
    path(app)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, memory: &AgentNotchMemory) -> Result<(), String> {
    let path = path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec(memory).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json)
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("Could not save the Agent history: {e}"))
}

fn update(app: &AppHandle, change: impl FnOnce(&mut AgentNotchMemory)) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut memory = load(app);
    change(&mut memory);
    save(app, &memory)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn append_to(memory: &mut AgentNotchMemory, messages: Vec<AgentMessage>, at_unix_ms: u64) {
    memory.messages.extend(
        messages
            .into_iter()
            .filter(|message| !message.content.trim().is_empty())
            .map(|message| AgentHistoryMessage {
                role: if message.role == "assistant" {
                    "assistant".into()
                } else {
                    "user".into()
                },
                content: message.content.trim().chars().take(MAX_CHARS).collect(),
                at_unix_ms,
            }),
    );
    let excess = memory.messages.len().saturating_sub(MAX_MESSAGES);
    memory.messages.drain(..excess);
}

/// [GRAIN] Host-side recording of a chat turn (see `agent::agent_run`). A
/// failure is logged: losing a history line must never fail the request.
pub(crate) fn record(app: &AppHandle, messages: Vec<AgentMessage>) {
    if let Err(error) = update(app, |memory| append_to(memory, messages, now_ms())) {
        log::warn!("[GRAIN] agent: could not record the chat: {error}");
    }
}

/// The notch's chat and last mode.
#[tauri::command]
#[specta::specta]
pub fn agent_history(app: AppHandle) -> AgentNotchMemory {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    load(&app)
}

/// Append finished turns (what was said, never tool traffic) to the chat.
#[tauri::command]
#[specta::specta]
pub fn agent_history_append(app: AppHandle, messages: Vec<AgentMessage>) -> Result<(), String> {
    update(&app, |memory| append_to(memory, messages, now_ms()))
}

/// Start the chat over. The mode is kept.
#[tauri::command]
#[specta::specta]
pub fn agent_history_clear(app: AppHandle) -> Result<(), String> {
    update(&app, |memory| memory.messages.clear())
}

/// Remember the mode the notch shows next time it opens.
#[tauri::command]
#[specta::specta]
pub fn agent_notch_set_mode(app: AppHandle, mode: AgentNotchMode) -> Result<(), String> {
    update(&app, |memory| memory.mode = mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, content: &str) -> AgentMessage {
        AgentMessage {
            role: role.into(),
            content: content.into(),
        }
    }

    #[test]
    fn append_keeps_only_spoken_text_and_trims_the_oldest() {
        let mut memory = AgentNotchMemory::default();
        append_to(
            &mut memory,
            vec![message("tool", " hi "), message("assistant", "   ")],
            1,
        );
        assert_eq!(memory.messages.len(), 1);
        assert_eq!(memory.messages[0].role, "user");
        assert_eq!(memory.messages[0].content, "hi");

        let many = (0..MAX_MESSAGES + 5)
            .map(|i| message("assistant", &i.to_string()))
            .collect();
        append_to(&mut memory, many, 2);
        assert_eq!(memory.messages.len(), MAX_MESSAGES);
        assert_eq!(
            memory.messages.last().unwrap().content,
            (MAX_MESSAGES + 4).to_string()
        );
    }

    #[test]
    fn missing_fields_load_as_an_empty_chat() {
        let memory: AgentNotchMemory = serde_json::from_str("{}").unwrap();
        assert_eq!(memory.mode, AgentNotchMode::Chat);
        assert!(memory.messages.is_empty());
    }
}
