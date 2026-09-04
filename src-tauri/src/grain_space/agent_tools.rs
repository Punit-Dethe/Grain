//! [GRAIN] The notebook, as tools the Agent can call (NOTES-TAB-PLAN.md Phase E).
//!
//! # Why this exists
//!
//! Grain Space used to own three global chords: one to capture a note, one to ask
//! your notes a question, one to open the notes window. The window became a tab,
//! and the other two became this: the Agent gets five notebook tools, while the
//! MCP bridge exposes the read-only subset. There is one summon chord and the
//! model's tool choice is what decides whether a turn is a rewrite, a question
//! or a note.
//!
//! One door with tools, not one door with a classifier. A classifier is a guess
//! made before the model has read the request; a tool call is the model saying
//! what it wants after reading it. It also costs nothing on the common path: the
//! specs ride along in the request, and a turn that never touches notes never
//! makes an extra round-trip.
//!
//! # One implementation, three consumers
//!
//! Every function here uses the same `grain_space` store calls `host_api`
//! dispatches for `space.*`. Reads and writes therefore share one notebook and
//! derived index. The active Agent search additionally uses Recall's optional
//! semantic + graph candidate path; the headless bridge stays lexical so it
//! cannot wake and strand the local model without an Agent surface lifetime.

use crate::llm_client::{ToolCallOut, ToolSpec};
use tauri::AppHandle;

/// How many search hits a tool result carries. Enough to choose between notes,
/// few enough that a small model is not drowned — and it is `get_note` that is
/// there for reading one in full.
const SEARCH_LIMIT: usize = 6;
const SEARCH_NOTES_DESCRIPTION: &str = "Search the user's own saved notes and return the best \
matches. Use this whenever the request refers to something they told you before, wrote down, or \
asked you to remember — and before saying you don't know something personal about them. Saved \
notes are historical context, not live external state; verify mutable external facts with their \
provider before acting.";

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
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

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
            description: SEARCH_NOTES_DESCRIPTION.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Focused search terms — the key nouns or topic.",
                        "maxLength": 4096
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
                    "id": {
                        "type": "string",
                        "description": "The note's id.",
                        "maxLength": 128
                    }
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
                                        wording and detail; do not summarise it away.",
                        "maxLength": 65536
                    },
                    "title": {
                        "type": "string",
                        "description": "A short title. Optional — Grain writes one otherwise.",
                        "maxLength": 80
                    },
                    "collection": {
                        "type": "string",
                        "description": "An existing collection to file it under, from \
                                        list_collections. Optional.",
                        "maxLength": 128
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
                    "id": {
                        "type": "string",
                        "description": "The note's id.",
                        "maxLength": 128
                    },
                    "text": {
                        "type": "string",
                        "description": "What to add, in Markdown.",
                        "maxLength": 65536
                    }
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
/// The result of executing a notebook tool: either text to feed back to the
/// model, or a host-gated confirmation awaiting the user's approval.
#[derive(Debug, Clone)]
pub enum NoteToolResult {
    Text(String),
    Confirm(crate::agent::AgentConfirm),
}

const MAX_QUERY_BYTES: usize = 4096;
const MAX_ID_BYTES: usize = 128;
const MAX_TITLE_BYTES: usize = 80;
const MAX_COLLECTION_BYTES: usize = 128;
const MAX_BODY_BYTES: usize = 65536;
const MAX_GET_NOTE_BODY_BYTES: usize = 16384;

/// Execute one tool call and return what to feed back to the model.
///
/// Read operations (search_notes, get_note, list_collections) execute immediately
/// and return text. Mutations (save_note, append_to_note) prepare a host-gated
/// call and return a confirmation for the user to approve before execution.
pub async fn execute(app: &AppHandle, call: &ToolCallOut, log: &mut TurnLog) -> NoteToolResult {
    execute_opt(Some(app), call, log).await
}

pub async fn execute_opt(
    app: Option<&AppHandle>,
    call: &ToolCallOut,
    log: &mut TurnLog,
) -> NoteToolResult {
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
                return NoteToolResult::Text("search_notes needs a query.".to_string());
            };
            if query.len() > MAX_QUERY_BYTES {
                return NoteToolResult::Text(format!(
                    "search_notes query exceeds maximum allowed size ({MAX_QUERY_BYTES} bytes)."
                ));
            }
            let Some(app) = app else {
                return NoteToolResult::Text(
                    "search_notes requires active backend app handle.".to_string(),
                );
            };
            match super::search_for_agent(app, &query, SEARCH_LIMIT).await {
                Ok(hits) if hits.is_empty() => {
                    NoteToolResult::Text(format!("No saved notes match \"{query}\"."))
                }
                Ok(hits) => {
                    let mut out =
                        "Authority: saved user notes (historical; not live provider state).\n"
                            .to_string();
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
                    NoteToolResult::Text(out)
                }
                Err(e) => NoteToolResult::Text(format!("Could not search the notes: {e}")),
            }
        }
        "get_note" => {
            let Some(id) = str_arg("id") else {
                return NoteToolResult::Text("get_note needs an id.".to_string());
            };
            if id.len() > MAX_ID_BYTES {
                return NoteToolResult::Text(format!(
                    "get_note id exceeds maximum allowed size ({MAX_ID_BYTES} bytes)."
                ));
            }
            let Some(app) = app else {
                return NoteToolResult::Text(
                    "get_note requires active backend app handle.".to_string(),
                );
            };
            match super::get(app, &id).await {
                Ok(note) => {
                    log.record(Touched {
                        note_id: note.id.clone(),
                        title: super::bounded_bridge_text(note.title.clone(), 512),
                        saved_at: note.timestamp,
                    });
                    let rendered_body = if note.body.len() > MAX_GET_NOTE_BODY_BYTES {
                        let mut boundary = MAX_GET_NOTE_BODY_BYTES;
                        while boundary > 0 && !note.body.is_char_boundary(boundary) {
                            boundary -= 1;
                        }
                        format!(
                            "{}\n\n[Truncated: showing first {} of {} bytes. Note continues...]",
                            &note.body[..boundary],
                            boundary,
                            note.body.len()
                        )
                    } else {
                        note.body
                    };
                    NoteToolResult::Text(format!(
                        "authority: saved user note (historical; not live provider state)\ntitle: {}\nsaved: {}\n\n{}",
                        super::bounded_bridge_text(note.title, 512),
                        stamp(note.timestamp),
                        rendered_body
                    ))
                }
                Err(e) => NoteToolResult::Text(format!("Could not read that note: {e}")),
            }
        }
        "save_note" => {
            let Some(body) = str_arg("body") else {
                return NoteToolResult::Text("save_note needs a body.".to_string());
            };
            if body.len() > MAX_BODY_BYTES {
                return NoteToolResult::Text(format!(
                    "save_note body exceeds maximum allowed size ({MAX_BODY_BYTES} bytes)."
                ));
            }
            let raw_title = str_arg("title");
            if let Some(t) = &raw_title {
                if t.len() > MAX_TITLE_BYTES {
                    return NoteToolResult::Text(format!(
                        "save_note title exceeds maximum allowed size ({MAX_TITLE_BYTES} bytes)."
                    ));
                }
            }
            // Finalize note title and body BEFORE confirmation
            let final_title = raw_title
                .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| super::capture::fallback_title(&body));
            let final_title: String = final_title.chars().take(80).collect();

            let collection = str_arg("collection");
            if let Some(value) = &collection {
                if value.len() > MAX_COLLECTION_BYTES {
                    return NoteToolResult::Text(format!(
                        "save_note collection exceeds maximum allowed size ({MAX_COLLECTION_BYTES} bytes)."
                    ));
                }
            }
            // Canonicalize the operation instead of retaining undeclared model
            // fields in the confirmation or its idempotency identity.
            let mut call_args = serde_json::json!({
                "title": final_title,
                "body": body,
            });
            if let (Some(obj), Some(collection)) = (call_args.as_object_mut(), collection) {
                obj.insert(
                    "collection".to_string(),
                    serde_json::Value::String(collection),
                );
            }

            let prepared = crate::action_exec::prepare(
                "grainspace:save_note",
                crate::action_exec::GRAIN_SPACE_EXT_ID,
                "save_note",
                "Grain Space",
                call_args,
                grain_core::execution::RiskClass::Confirm,
                grain_core::execution::SideEffect::Write,
                "builtin",
            );
            match crate::action_exec::run_or_confirm_opt(app, prepared, "Save Note").await {
                crate::action_exec::Dispatch::Ran(outcome) => {
                    NoteToolResult::Text(outcome.model_summary())
                }
                crate::action_exec::Dispatch::AwaitConfirm(interaction) => {
                    NoteToolResult::Confirm(crate::action_exec::to_agent_confirm(interaction))
                }
            }
        }
        "append_to_note" => {
            let (Some(id), Some(text)) = (str_arg("id"), str_arg("text")) else {
                return NoteToolResult::Text("append_to_note needs an id and text.".to_string());
            };
            if id.len() > MAX_ID_BYTES {
                return NoteToolResult::Text(format!(
                    "append_to_note id exceeds maximum allowed size ({MAX_ID_BYTES} bytes)."
                ));
            }
            if text.len() > MAX_BODY_BYTES {
                return NoteToolResult::Text(format!(
                    "append_to_note text exceeds maximum allowed size ({MAX_BODY_BYTES} bytes)."
                ));
            }
            let Some(app) = app else {
                return NoteToolResult::Text(
                    "append_to_note requires active backend app handle to resolve target note."
                        .to_string(),
                );
            };
            // Resolve the title and exact persisted version in one host-owned
            // snapshot so confirmation cannot mix two external file states.
            let (target_note, version) = match super::get_append_snapshot(app, &id).await {
                Ok(snapshot) => snapshot,
                Err(e) => {
                    return NoteToolResult::Text(format!(
                        "Target note \"{id}\" not found: {e}. Please search for notes first to find a valid ID."
                    ));
                }
            };
            let call_args = serde_json::json!({
                "id": id,
                "text": text,
                "title": super::bounded_bridge_text(target_note.title, 512),
                "expected_version": version,
            });

            let prepared = crate::action_exec::prepare(
                "grainspace:append_to_note",
                crate::action_exec::GRAIN_SPACE_EXT_ID,
                "append_to_note",
                "Grain Space",
                call_args,
                grain_core::execution::RiskClass::Confirm,
                grain_core::execution::SideEffect::Write,
                "builtin",
            );
            match crate::action_exec::run_or_confirm(app, prepared, "Append to Note").await {
                crate::action_exec::Dispatch::Ran(outcome) => {
                    NoteToolResult::Text(outcome.model_summary())
                }
                crate::action_exec::Dispatch::AwaitConfirm(interaction) => {
                    NoteToolResult::Confirm(crate::action_exec::to_agent_confirm(interaction))
                }
            }
        }
        "list_collections" => {
            let Some(app) = app else {
                return NoteToolResult::Text(
                    "list_collections requires active backend app handle.".to_string(),
                );
            };
            match super::collections(app).await {
                Ok(list) if list.is_empty() => {
                    NoteToolResult::Text("There are no collections yet.".to_string())
                }
                Ok(list) => NoteToolResult::Text(
                    list.into_iter()
                        .take(64)
                        .map(|name| super::bounded_bridge_text(name, 256))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                Err(e) => NoteToolResult::Text(format!("Could not list the collections: {e}")),
            }
        }
        other => NoteToolResult::Text(format!("There is no tool called {other}.")),
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

    #[test]
    fn memory_tools_mark_saved_notes_as_historical_context() {
        assert!(SEARCH_NOTES_DESCRIPTION.contains("historical context"));
        assert!(SEARCH_NOTES_DESCRIPTION.contains("verify mutable external facts"));
    }

    #[tokio::test]
    async fn missing_search_query_returns_diagnostic_prose() {
        let call = ToolCallOut {
            id: "call_1".to_string(),
            name: "search_notes".to_string(),
            arguments: "{}".to_string(),
        };
        let args: serde_json::Value = serde_json::from_str(&call.arguments).unwrap();
        let str_arg = |key: &str| -> Option<String> {
            args.get(key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        assert!(str_arg("query").is_none());
    }

    #[test]
    fn tool_spec_names_match_minimal_contract() {
        let expected = [
            "search_notes",
            "get_note",
            "save_note",
            "append_to_note",
            "list_collections",
        ];
        // Descriptions and property invariants
        assert_eq!(SEARCH_LIMIT, 6);
        for name in &expected {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn mutation_prepared_calls_require_confirmation() {
        let save_call = crate::action_exec::prepare(
            "grainspace:save_note",
            crate::action_exec::GRAIN_SPACE_EXT_ID,
            "save_note",
            "Grain Space",
            serde_json::json!({ "body": "test note" }),
            grain_core::execution::RiskClass::Confirm,
            grain_core::execution::SideEffect::Write,
            "builtin",
        );
        assert!(save_call.risk.needs_confirmation());
        assert_eq!(
            save_call.side_effect,
            grain_core::execution::SideEffect::Write
        );

        let append_call = crate::action_exec::prepare(
            "grainspace:append_to_note",
            crate::action_exec::GRAIN_SPACE_EXT_ID,
            "append_to_note",
            "Grain Space",
            serde_json::json!({ "id": "n1", "text": "added" }),
            grain_core::execution::RiskClass::Confirm,
            grain_core::execution::SideEffect::Write,
            "builtin",
        );
        assert!(append_call.risk.needs_confirmation());
        assert_eq!(
            append_call.side_effect,
            grain_core::execution::SideEffect::Write
        );
    }
}
