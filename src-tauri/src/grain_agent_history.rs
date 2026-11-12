//! [GRAIN] The Agent notch's memory: its chats and the Chat/Recall mode it
//! last showed. Deliberately rudimentary: only what was said is kept — the
//! user's words and the assistant's final replies. Tool calls, approvals and
//! selections are not.
//!
//! `agent/notch.json` is a small index (mode, current chat, one summary per
//! chat); each chat's messages live in `agent/chats/<id>.json`. Everything is
//! read on demand; nothing is held in memory.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::AppHandle;

use crate::agent::AgentMessage;

const INDEX: &str = "notch.json";
const CHATS: &str = "chats";
/// The oldest chats beyond this many are deleted.
const MAX_CHATS: usize = 100;
/// A stored chat is trimmed to this many messages, oldest first.
const MAX_MESSAGES: usize = 200;
/// A single stored message is cut to this many characters.
const MAX_CHARS: usize = 20_000;
/// A chat's title: its first request, cut to this many characters.
const TITLE_CHARS: usize = 80;

/// Serialises read-modify-write of the files.
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

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentChatSummary {
    pub id: String,
    pub title: String,
    pub updated_unix_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    mode: AgentNotchMode,
    /// The chat the notch continues; `None` starts a new one on the next turn.
    #[serde(default)]
    current: Option<String>,
    #[serde(default)]
    chats: Vec<AgentChatSummary>,
}

impl Index {
    fn has(&self, id: &str) -> bool {
        self.chats.iter().any(|chat| chat.id == id)
    }

    fn current(&self) -> Option<&str> {
        self.current.as_deref().filter(|id| self.has(id))
    }
}

/// Every chat (newest first) and the one the notch continues.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct AgentChatList {
    pub current: Option<String>,
    pub chats: Vec<AgentChatSummary>,
}

/// What the notch opens with: its mode and the chat it continues.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct AgentNotchMemory {
    pub mode: AgentNotchMode,
    pub chat: Option<String>,
    pub messages: Vec<AgentHistoryMessage>,
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::portable::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("agent"))
}

/// Ids are host-minted hex; anything else never reaches the file system.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn chat_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(CHATS).join(format!("{id}.json"))
}

fn read<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| format!("Could not save the Agent history: {e}"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// What was said, cleaned: blank messages dropped, roles normalised.
fn spoken(messages: Vec<AgentMessage>, at_unix_ms: u64) -> Vec<AgentHistoryMessage> {
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
        })
        .collect()
}

fn title_of(messages: &[AgentHistoryMessage]) -> Option<String> {
    let first = messages.iter().find(|message| message.role == "user")?;
    let words = first
        .content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Some(words.chars().take(TITLE_CHARS).collect())
}

/// Append to `chat` — or to the current chat, starting one if there is none.
/// A chat deleted meanwhile takes nothing. Returns the chat appended to.
fn append(
    dir: &Path,
    chat: Option<&str>,
    messages: Vec<AgentMessage>,
    at_unix_ms: u64,
) -> Result<Option<String>, String> {
    let messages = spoken(messages, at_unix_ms);
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut index: Index = read(&dir.join(INDEX));
    let id = match chat {
        Some(id) if index.has(id) => id.to_owned(),
        Some(_) => return Ok(None),
        None => match index.current() {
            Some(id) => id.to_owned(),
            None if messages.is_empty() => return Ok(None),
            None => {
                let id = uuid::Uuid::new_v4().simple().to_string();
                index.chats.push(AgentChatSummary {
                    id: id.clone(),
                    title: String::new(),
                    updated_unix_ms: at_unix_ms,
                });
                index.current = Some(id.clone());
                id
            }
        },
    };
    if messages.is_empty() {
        return Ok(Some(id));
    }
    let path = chat_path(dir, &id);
    let mut log: Vec<AgentHistoryMessage> = read(&path);
    log.extend(messages);
    let excess = log.len().saturating_sub(MAX_MESSAGES);
    log.drain(..excess);
    write(&path, &log)?;

    if let Some(summary) = index.chats.iter_mut().find(|chat| chat.id == id) {
        summary.updated_unix_ms = at_unix_ms;
        if summary.title.is_empty() {
            summary.title = title_of(&log).unwrap_or_default();
        }
    }
    // Newest first; the oldest beyond the cap go (never the current chat).
    index
        .chats
        .sort_by(|a, b| b.updated_unix_ms.cmp(&a.updated_unix_ms));
    while index.chats.len() > MAX_CHATS {
        let Some(at) = index
            .chats
            .iter()
            .rposition(|chat| index.current.as_deref() != Some(chat.id.as_str()))
        else {
            break;
        };
        let gone = index.chats.remove(at);
        let _ = std::fs::remove_file(chat_path(dir, &gone.id));
    }
    write(&dir.join(INDEX), &index)?;
    Ok(Some(id))
}

fn memory(dir: &Path, index: &Index) -> AgentNotchMemory {
    let chat = index.current().map(str::to_owned);
    AgentNotchMemory {
        mode: index.mode,
        messages: chat
            .as_deref()
            .map(|id| read(&chat_path(dir, id)))
            .unwrap_or_default(),
        chat,
    }
}

fn change_index(app: &AppHandle, change: impl FnOnce(&Path, &mut Index)) -> Result<(), String> {
    let dir = dir(app)?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut index: Index = read(&dir.join(INDEX));
    change(&dir, &mut index);
    write(&dir.join(INDEX), &index)
}

/// [GRAIN] Host-side recording of a chat turn (see `agent::agent_run`): into
/// `chat`, or the current chat when `None`. Returns the chat recorded into, so
/// the reply lands beside its request even if the notch switched chats. A
/// failure is logged: losing a history line must never fail the request.
pub(crate) fn record(
    app: &AppHandle,
    chat: Option<&str>,
    messages: Vec<AgentMessage>,
) -> Option<String> {
    let result = dir(app).and_then(|dir| append(&dir, chat, messages, now_ms()));
    result.unwrap_or_else(|error| {
        log::warn!("[GRAIN] agent: could not record the chat: {error}");
        None
    })
}

/// The notch's mode and the chat it continues.
#[tauri::command]
#[specta::specta]
pub fn agent_history(app: AppHandle) -> AgentNotchMemory {
    let Ok(dir) = dir(&app) else {
        return AgentNotchMemory::default();
    };
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    memory(&dir, &read(&dir.join(INDEX)))
}

/// Every chat, newest first.
#[tauri::command]
#[specta::specta]
pub fn agent_history_list(app: AppHandle) -> AgentChatList {
    let Ok(dir) = dir(&app) else {
        return AgentChatList::default();
    };
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let index: Index = read(&dir.join(INDEX));
    AgentChatList {
        current: index.current().map(str::to_owned),
        chats: index.chats,
    }
}

/// Continue chat `id` from now on; returns its messages.
#[tauri::command]
#[specta::specta]
pub fn agent_history_open(app: AppHandle, id: String) -> Result<AgentNotchMemory, String> {
    let dir = dir(&app)?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut index: Index = read(&dir.join(INDEX));
    if !valid_id(&id) || !index.has(&id) {
        return Err("That chat no longer exists".into());
    }
    index.current = Some(id);
    write(&dir.join(INDEX), &index)?;
    Ok(memory(&dir, &index))
}

/// Append finished turns (what was said, never tool traffic) to the current chat.
#[tauri::command]
#[specta::specta]
pub fn agent_history_append(app: AppHandle, messages: Vec<AgentMessage>) -> Result<(), String> {
    append(&dir(&app)?, None, messages, now_ms()).map(|_| ())
}

/// Start a new chat with the next turn. Earlier chats are kept.
#[tauri::command]
#[specta::specta]
pub fn agent_history_new(app: AppHandle) -> Result<(), String> {
    change_index(&app, |_, index| index.current = None)
}

/// Delete one chat.
#[tauri::command]
#[specta::specta]
pub fn agent_history_delete(app: AppHandle, id: String) -> Result<(), String> {
    if !valid_id(&id) {
        return Err("That chat no longer exists".into());
    }
    change_index(&app, |dir, index| {
        index.chats.retain(|chat| chat.id != id);
        if index.current.as_deref() == Some(id.as_str()) {
            index.current = None;
        }
        let _ = std::fs::remove_file(chat_path(dir, &id));
    })
}

/// Remember the mode the notch shows next time it opens.
#[tauri::command]
#[specta::specta]
pub fn agent_notch_set_mode(app: AppHandle, mode: AgentNotchMode) -> Result<(), String> {
    change_index(&app, |_, index| index.mode = mode)
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

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "grain-agent-history-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn spoken_keeps_only_text_and_normalises_roles() {
        let kept = spoken(
            vec![message("tool", " hi "), message("assistant", "   ")],
            1,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].role, "user");
        assert_eq!(kept[0].content, "hi");
    }

    #[test]
    fn turns_start_a_chat_then_continue_it_and_a_reply_follows_its_request() {
        let dir = temp();
        let first = append(&dir, None, vec![message("user", "check   my notes")], 1)
            .unwrap()
            .unwrap();
        // The notch moves on to a new chat while the first reply is pending.
        let mut index: Index = read(&dir.join(INDEX));
        index.current = None;
        write(&dir.join(INDEX), &index).unwrap();
        let second = append(&dir, None, vec![message("user", "other")], 2)
            .unwrap()
            .unwrap();
        assert_ne!(first, second);
        // The pending reply still lands in the chat that asked.
        append(&dir, Some(&first), vec![message("assistant", "done")], 3).unwrap();

        let index: Index = read(&dir.join(INDEX));
        assert_eq!(index.chats[0].id, first, "newest first");
        assert_eq!(index.chats[0].title, "check my notes");
        let log: Vec<AgentHistoryMessage> = read(&chat_path(&dir, &first));
        assert_eq!(log.len(), 2);
        assert_eq!(index.current.as_deref(), Some(second.as_str()));
        // A deleted chat takes nothing.
        assert_eq!(
            append(&dir, Some("abc"), vec![message("user", "x")], 4).unwrap(),
            None
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_fields_load_as_no_chats() {
        let index: Index = serde_json::from_str("{}").unwrap();
        assert_eq!(index.mode, AgentNotchMode::Chat);
        assert!(index.current.is_none() && index.chats.is_empty());
        assert!(!valid_id("../x") && valid_id("0af3"));
    }
}
