//! [GRAIN] The notebook, as tools the Agent can call (NOTES-TAB-PLAN.md Phase E).
//!
//! # Why this exists
//!
//! Grain Space used to own three global chords: one to capture a note, one to ask
//! your notes a question, one to open the notes window. The window became a tab,
//! and the other two became this: the Agent gets the *same five tools the MCP
//! bridge already exposes*, so there is one summon chord and the model's tool
//! choice is what decides whether a turn is a rewrite, a question or a note.
//!
//! One door with tools, not one door with a classifier. A classifier is a guess
//! made before the model has read the request; a tool call is the model saying
//! what it wants after reading it. It also costs nothing on the common path: the
//! specs ride along in the request, and a turn that never touches notes never
//! makes an extra round-trip.
//!
//! # One implementation, three consumers
//!
//! Every function here delegates to the `grain_space` calls `host_api` already
//! dispatches for `space.*` — `collections`, `search`, `get`, `save`, `append`.
//! The MCP proxy, the Agent and the app's own UI therefore read and write one
//! notebook through one path. Adding a second would guarantee they drift.

use crate::llm_client::{ToolCallOut, ToolSpec};
use tauri::AppHandle;

/// How many search hits a tool result carries. Enough to choose between notes,
/// few enough that a small model is not drowned — and it is `get_note` that is
/// there for reading one in full.
const SEARCH_LIMIT: usize = 6;

/// A note the model looked at (or wrote) during a turn. Collected so the reply can
/// show provenance chips: "here is what I read", clickable straight into the Notes
/// tab.
///
/// This is deliberately "notes consulted", not "notes cited". Grain Recall asks the
/// model to tag memories `[Mn]` and echo the ones it used in a SOURCES line, which
/// is more precise when the model follows the convention and silently wrong when it
/// does not. What the tool calls actually touched is not a convention — it is a
/// fact we observed.
#[derive(Debug, Clone)]
pub struct Touched {
    pub note_id: String,
    pub title: String,
    pub saved_at: i64,
}

/// Per-turn accumulator: the notes touched, in the order they were touched.
#[derive(Debug, Default)]
pub struct TurnLog {
    touched: Vec<Touched>,
}

impl TurnLog {
    fn record(&mut self, note: Touched) {
        if self.touched.iter().any(|t| t.note_id == note.note_id) {
            return; // a note read twice is still one source
        }
        self.touched.push(note);
    }

    pub fn touched(&self) -> &[Touched] {
        &self.touched
    }
}

/// The tool specs to advertise, or empty when the notebook is switched off.
///
/// Empty matters: with the feature off there is no notebook to reach, and
/// advertising tools that can only fail would spend tokens teaching the model
/// about a door that is bricked up.
pub fn specs(app: &AppHandle) -> Vec<ToolSpec> {
    if !super::is_enabled(app) {
        return Vec::new();
    }
    vec![
        ToolSpec {
            name: "search_notes".to_string(),
            description: "Search the user's own saved notes and return the best matches. Use \
                          this whenever the request refers to something they told you before, \
                          wrote down, or asked you to remember — and before saying you don't \
                          know something personal about them."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Focused search terms — the key nouns or topic."
                    }
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "get_note".to_string(),
            description: "Read one note in full, by the id returned from search_notes. Use it \
                          when a search snippet is not enough to answer."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The note's id." }
                },
                "required": ["id"]
            }),
        },
        ToolSpec {
            name: "save_note".to_string(),
            description: "Save a NEW note. Only when the user asks you to write something down, \
                          remember it, or make a note of it — never as a side effect of \
                          answering, rewriting or explaining something."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "body": {
                        "type": "string",
                        "description": "The note itself, in Markdown. Keep the user's own \
                                        wording and detail; do not summarise it away."
                    },
                    "title": {
                        "type": "string",
                        "description": "A short title. Optional — Grain writes one otherwise."
                    },
                    "collection": {
                        "type": "string",
                        "description": "An existing collection to file it under, from \
                                        list_collections. Optional."
                    }
                },
                "required": ["body"]
            }),
        },
        ToolSpec {
            name: "append_to_note".to_string(),
            description: "Add text to the end of a note that already exists, by id. Use this \
                          rather than save_note when the user is adding to something."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The note's id." },
                    "text": { "type": "string", "description": "What to add, in Markdown." }
                },
                "required": ["id", "text"]
            }),
        },
        ToolSpec {
            name: "list_collections".to_string(),
            description: "List the collections the user files notes under. Use before \
                          save_note when they say where a note belongs."
                .to_string(),
            parameters: serde_json::json!({ "type": "object", "properties": {} }),
        },
    ]
}

/// Execute one tool call and return what to feed back to the model.
///
/// Errors come back as TEXT, not as `Err`: a tool that cannot answer is
/// information the model should have and reason about ("that note is gone, tell
/// the user"), whereas failing the turn throws away a conversation over one bad
/// argument.
pub async fn execute(app: &AppHandle, call: &ToolCallOut, log: &mut TurnLog) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&call.arguments).unwrap_or(serde_json::Value::Null);
    let str_arg = |key: &str| -> Option<String> {
        args.get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    match call.name.as_str() {
        "search_notes" => {
            let Some(query) = str_arg("query") else {
                return "search_notes needs a query.".to_string();
            };
            match super::search(app, &query, SEARCH_LIMIT).await {
                Ok(hits) if hits.is_empty() => {
                    format!("No saved notes match \"{query}\".")
                }
                Ok(hits) => {
                    let mut out = String::new();
                    for hit in &hits {
                        log.record(Touched {
                            note_id: hit.id.clone(),
                            title: hit.title.clone(),
                            saved_at: hit.saved_at,
                        });
                        out.push_str(&format!(
                            "- id: {}\n  title: {}\n  saved: {}\n  summary: {}\n",
                            hit.id,
                            hit.title,
                            stamp(hit.saved_at),
                            hit.snippet
                        ));
                        if !hit.entities.is_empty() {
                            out.push_str(&format!("  about: {}\n", hit.entities.join(", ")));
                        }
                    }
                    out
                }
                Err(e) => format!("Could not search the notes: {e}"),
            }
        }
        "get_note" => {
            let Some(id) = str_arg("id") else {
                return "get_note needs an id.".to_string();
            };
            match super::get(app, &id).await {
                Ok(note) => {
                    log.record(Touched {
                        note_id: note.id.clone(),
                        title: note.title.clone(),
                        saved_at: note.timestamp,
                    });
                    format!(
                        "title: {}\nsaved: {}\n\n{}",
                        note.title,
                        stamp(note.timestamp),
                        note.body
                    )
                }
                Err(e) => format!("Could not read that note: {e}"),
            }
        }
        "save_note" => {
            let Some(body) = str_arg("body") else {
                return "save_note needs a body.".to_string();
            };
            let supplied = super::SuppliedMeta {
                title: str_arg("title"),
                summary: None,
                question: None,
                entities: Vec::new(),
                collection: str_arg("collection"),
            };
            match super::save(app, &body, supplied).await {
                Ok(id) => {
                    // Read it back for the chip's real title: `save` may have
                    // distilled one, and the chip should say what is on disk
                    // rather than what the model proposed.
                    if let Ok(note) = super::get(app, &id).await {
                        log.record(Touched {
                            note_id: note.id.clone(),
                            title: note.title.clone(),
                            saved_at: note.timestamp,
                        });
                        format!("Saved as \"{}\" (id {}).", note.title, note.id)
                    } else {
                        format!("Saved (id {id}).")
                    }
                }
                Err(e) => format!("Could not save the note: {e}"),
            }
        }
        "append_to_note" => {
            let (Some(id), Some(text)) = (str_arg("id"), str_arg("text")) else {
                return "append_to_note needs an id and text.".to_string();
            };
            match super::append(app, &id, &text).await {
                Ok(()) => {
                    if let Ok(note) = super::get(app, &id).await {
                        log.record(Touched {
                            note_id: note.id.clone(),
                            title: note.title.clone(),
                            saved_at: note.timestamp,
                        });
                        format!("Added to \"{}\".", note.title)
                    } else {
                        "Added.".to_string()
                    }
                }
                Err(e) => format!("Could not add to that note: {e}"),
            }
        }
        "list_collections" => match super::collections(app).await {
            Ok(list) if list.is_empty() => "There are no collections yet.".to_string(),
            Ok(list) => list.join("\n"),
            Err(e) => format!("Could not list the collections: {e}"),
        },
        other => format!("There is no tool called {other}."),
    }
}

/// Human date for a tool result. The model reasons about "last Tuesday" far better
/// from a date than from an epoch.
fn stamp(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_read_twice_is_one_source() {
        let mut log = TurnLog::default();
        let note = Touched {
            note_id: "n1".to_string(),
            title: "One".to_string(),
            saved_at: 0,
        };
        log.record(note.clone());
        log.record(note);
        assert_eq!(log.touched().len(), 1);
    }
}
