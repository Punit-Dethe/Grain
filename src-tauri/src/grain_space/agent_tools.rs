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
//! # Single Source of Truth & Safe Execution Gate
//!
//! All tool specifications are unified and published by `crate::action_exec`.
//! All write operations (`save_note`, `append_to_note`) strictly traverse
//! `action_exec::prepare` and `action_exec::run_or_confirm`, which enforces host
//! confirmation policy. No note write can bypass user approval.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::llm_client::{ToolCallOut, ToolSpec};

/// How many search hits a tool result carries. Enough to choose between notes,
/// few enough that a small model is not drowned — and it is `get_note` that is
/// there for reading one in full.
const SEARCH_LIMIT: usize = 6;

pub const ERR_INVALID_ARGUMENTS: &str = "invalid_arguments";
pub const ERR_NOTE_NOT_FOUND: &str = "note_not_found";
pub const ERR_SCHEMA_VIOLATION: &str = "schema_violation";
pub const ERR_CONFIRMATION_REQUIRED: &str = "confirmation_required";
pub const ERR_INTERNAL: &str = "internal_error";

/// Stable machine-readable memory tool error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryErrorCode {
    InvalidArguments,
    NoteNotFound,
    SchemaViolation,
    ConfirmationRequired,
    Internal,
}

impl MemoryErrorCode {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidArguments => ERR_INVALID_ARGUMENTS,
            Self::NoteNotFound => ERR_NOTE_NOT_FOUND,
            Self::SchemaViolation => ERR_SCHEMA_VIOLATION,
            Self::ConfirmationRequired => ERR_CONFIRMATION_REQUIRED,
            Self::Internal => ERR_INTERNAL,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolErrorPayload {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolErrorResponse {
    pub ok: bool,
    pub error: ToolErrorPayload,
}

impl ToolErrorResponse {
    pub fn new(code: MemoryErrorCode, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: ToolErrorPayload {
                code: code.as_str().to_string(),
                message: message.into(),
            },
        }
    }

    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                r#"{{"ok":false,"error":{{"code":"{}","message":"{}"}}}}"#,
                self.error.code, self.error.message
            )
        })
    }
}

/// A note the model looked at (or wrote) during a turn. Collected so the reply can
/// show provenance chips: "here is what I read", clickable straight into the Notes
/// tab.
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
    pub fn record(&mut self, note: Touched) {
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
/// Re-exports the authoritative definitions published by `action_exec`.
pub fn specs(app: &AppHandle) -> Vec<ToolSpec> {
    if !super::is_enabled(app) {
        return Vec::new();
    }
    crate::action_exec::grain_space_tool_specs()
}

/// Dispatch one tool call through the unified action execution boundary.
///
/// Read operations (`search_notes`, `get_note`, `list_collections`) execute
/// immediately and return `ToolResult::Text`.
/// Write operations (`save_note`, `append_to_note`) strictly route through
/// `action_exec::run_or_confirm`, which withholds them and returns
/// `ToolResult::Confirm(confirm)` for explicit user approval.
pub async fn dispatch(
    app: &AppHandle,
    call: &ToolCallOut,
    log: &mut TurnLog,
) -> crate::capability::ToolResult {
    dispatch_opt(Some(app), call, log).await
}

/// Optional-AppHandle variant of `dispatch` used by headless test suites.
pub async fn dispatch_opt(
    app: Option<&AppHandle>,
    call: &ToolCallOut,
    log: &mut TurnLog,
) -> crate::capability::ToolResult {
    let args: serde_json::Value = match serde_json::from_str(&call.arguments) {
        Ok(serde_json::Value::Object(map)) => serde_json::Value::Object(map),
        Ok(serde_json::Value::Null) => serde_json::Value::Object(serde_json::Map::new()),
        Ok(_) => {
            return crate::capability::ToolResult::Text(
                ToolErrorResponse::new(
                    MemoryErrorCode::SchemaViolation,
                    "Tool arguments must be a JSON object.",
                )
                .to_json_string(),
            );
        }
        Err(err) => {
            return crate::capability::ToolResult::Text(
                ToolErrorResponse::new(
                    MemoryErrorCode::SchemaViolation,
                    format!("Malformed JSON arguments: {err}"),
                )
                .to_json_string(),
            );
        }
    };

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
                return crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::InvalidArguments,
                        "search_notes needs a query.",
                    )
                    .to_json_string(),
                );
            };
            let Some(app) = app else {
                return crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::Internal,
                        "AppHandle is required for search.",
                    )
                    .to_json_string(),
                );
            };
            match super::search_for_agent(app, &query, SEARCH_LIMIT).await {
                Ok(hits) if hits.is_empty() => {
                    crate::capability::ToolResult::Text(format!("No saved notes match \"{query}\"."))
                }
                Ok(hits) => {
                    let mut out = "Authority: saved user notes (historical; not live provider state).\n"
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
                    crate::capability::ToolResult::Text(out)
                }
                Err(e) => crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::Internal,
                        format!("Could not search the notes: {e}"),
                    )
                    .to_json_string(),
                ),
            }
        }
        "get_note" => {
            let Some(id) = str_arg("id") else {
                return crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::InvalidArguments,
                        "get_note needs an id.",
                    )
                    .to_json_string(),
                );
            };
            let Some(app) = app else {
                return crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::Internal,
                        "AppHandle is required to get a note.",
                    )
                    .to_json_string(),
                );
            };
            match super::get(app, &id).await {
                Ok(note) => {
                    log.record(Touched {
                        note_id: note.id.clone(),
                        title: note.title.clone(),
                        saved_at: note.timestamp,
                    });
                    crate::capability::ToolResult::Text(format!(
                        "authority: saved user note (historical; not live provider state)\ntitle: {}\nsaved: {}\n\n{}",
                        note.title,
                        stamp(note.timestamp),
                        note.body
                    ))
                }
                Err(e) => crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::NoteNotFound,
                        format!("Could not read note '{id}': {e}"),
                    )
                    .to_json_string(),
                ),
            }
        }
        "save_note" => {
            if str_arg("body").is_none() {
                return crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::InvalidArguments,
                        "save_note needs a body.",
                    )
                    .to_json_string(),
                );
            }
            let prepared = crate::action_exec::prepare_grain_space_call("save_note", args);
            match crate::action_exec::run_or_confirm_opt(app, prepared, "Save note").await {
                crate::action_exec::Dispatch::Ran(outcome) => {
                    crate::capability::ToolResult::Text(outcome.model_summary())
                }
                crate::action_exec::Dispatch::AwaitConfirm(interaction) => {
                    crate::capability::ToolResult::Confirm(crate::action_exec::to_agent_confirm(
                        &interaction,
                    ))
                }
            }
        }
        "append_to_note" => {
            if str_arg("id").is_none() || str_arg("text").is_none() {
                return crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::InvalidArguments,
                        "append_to_note needs an id and text.",
                    )
                    .to_json_string(),
                );
            }
            let prepared = crate::action_exec::prepare_grain_space_call("append_to_note", args);
            match crate::action_exec::run_or_confirm_opt(app, prepared, "Add to note").await {
                crate::action_exec::Dispatch::Ran(outcome) => {
                    crate::capability::ToolResult::Text(outcome.model_summary())
                }
                crate::action_exec::Dispatch::AwaitConfirm(interaction) => {
                    crate::capability::ToolResult::Confirm(crate::action_exec::to_agent_confirm(
                        &interaction,
                    ))
                }
            }
        }
        "list_collections" => {
            let Some(app) = app else {
                return crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::Internal,
                        "AppHandle is required to list collections.",
                    )
                    .to_json_string(),
                );
            };
            match super::collections(app).await {
                Ok(list) if list.is_empty() => {
                    crate::capability::ToolResult::Text("There are no collections yet.".to_string())
                }
                Ok(list) => crate::capability::ToolResult::Text(list.join("\n")),
                Err(e) => crate::capability::ToolResult::Text(
                    ToolErrorResponse::new(
                        MemoryErrorCode::Internal,
                        format!("Could not list the collections: {e}"),
                    )
                    .to_json_string(),
                ),
            }
        }
        other => crate::capability::ToolResult::Text(
            ToolErrorResponse::new(
                MemoryErrorCode::SchemaViolation,
                format!("There is no tool called '{other}'."),
            )
            .to_json_string(),
        ),
    }
}

/// Execute one tool call and return text. If a write was attempted that requires
/// confirmation, returns a structured `confirmation_required` error code.
#[allow(dead_code)]
pub async fn execute(app: &AppHandle, call: &ToolCallOut, log: &mut TurnLog) -> String {
    match dispatch(app, call, log).await {
        crate::capability::ToolResult::Text(text) => text,
        crate::capability::ToolResult::Confirm(_) => ToolErrorResponse::new(
            MemoryErrorCode::ConfirmationRequired,
            "Write operations require user confirmation and must be dispatched through action_exec.",
        )
        .to_json_string(),
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
    fn tool_error_response_serializes_cleanly() {
        let err = ToolErrorResponse::new(MemoryErrorCode::InvalidArguments, "save_note needs a body.");
        let raw = err.to_json_string();
        assert!(raw.contains(r#""code":"invalid_arguments""#));
        assert!(raw.contains(r#""message":"save_note needs a body.""#));
        assert!(raw.contains(r#""ok":false"#));

        let parsed: ToolErrorResponse = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed, err);
    }

    #[test]
    fn specs_include_all_canonical_parameters() {
        let defs = crate::action_exec::grain_space_tool_definitions();
        let save = defs.iter().find(|d| d.action_id == "save_note").unwrap();
        let props = save.schema.get("properties").unwrap();
        assert!(props.get("body").is_some());
        assert!(props.get("title").is_some());
        assert!(props.get("summary").is_some());
        assert!(props.get("question").is_some());
        assert!(props.get("entities").is_some());
        assert!(props.get("collection").is_some());
    }
}
