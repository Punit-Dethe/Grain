//! [GRAIN] Phase 3 host executor (`docs/Extensions 2.0/PLAN.md` §8, §12).
//!
//! Turns a retrieved action into a real, safely-executed effect. Built on the
//! pure contracts in `grain_core::execution` (risk, prepared calls, outcomes) and
//! `grain_core::interaction` (what the user sees). The host owns every gate:
//!
//! 1. **prepare** — validate arguments, classify risk from the host floor, build
//!    the exact [`PreparedCall`].
//! 2. **run_or_confirm** — a `Safe` call executes immediately; a `Confirm` call is
//!    withheld, stored in [`PendingCalls`] by token, and returned as an
//!    [`Interaction::Confirm`] for the user to approve (§2.5 — the model cannot
//!    approve, only the user via the host affordance).
//! 3. **resume** — on approval the host revalidates the *exact* prepared call
//!    (time-of-use, §12.7) and replays it; the model is never re-consulted.
//!
//! ## Providers
//!
//! Built-in providers execute in process; third-party providers execute through
//! the extension worker (Phase 3b — the `"action"` worker protocol). Both return
//! the same [`ActionOutcome`]. This module ships the **Grain Space** built-in
//! provider; third-party actions run through the isolated extension worker and
//! return a strict, host-sanitised outcome (§8.1).

use std::sync::Mutex;

use grain_core::execution::{
    default_idempotency_key, ActionOutcome, FailureClass, PreparedCall, RiskClass, SideEffect,
    Stale, SuccessData,
};
use grain_core::interaction::{Field, Interaction};
use serde_json::Value;
use tauri::AppHandle;

/// The built-in Grain Space provider's extension id and namespace. Grain Space is
/// not a packaged extension; it executes in process, but it flows through the
/// same action registry as everything else.
pub const GRAIN_SPACE_EXT_ID: &str = "com.grain.grainspace";
pub const GRAIN_SPACE_NS: &str = "grainspace";

/// How long a confirmation stays valid before the user must ask again (§8.3).
const CONFIRM_TTL_MS: i64 = 120_000;
/// The most confirmations we hold at once — a bound so a user who never answers
/// cannot grow this without limit. Oldest are dropped first.
const MAX_PENDING: usize = 16;

/// The outcome of routing one tool call.
pub enum Dispatch {
    /// It ran (or failed) — feed `outcome` back and/or render it.
    Ran(ActionOutcome),
    /// It is risky and awaits the user. The `Interaction::Confirm` is surfaced;
    /// the exact call is held under its token until [`resume`].
    AwaitConfirm(Interaction),
}

/// Host-owned store of confirmations awaiting the user, keyed by token. Bounded
/// and time-limited; nothing here executes until the user approves it.
static PENDING: Mutex<Option<Vec<PreparedCall>>> = Mutex::new(None);

struct PendingCalls;

impl PendingCalls {
    fn insert(call: PreparedCall) {
        let mut guard = PENDING.lock().unwrap();
        let list = guard.get_or_insert_with(Vec::new);
        list.retain(|existing| existing.token != call.token);
        if list.len() >= MAX_PENDING {
            list.remove(0);
        }
        list.push(call);
    }

    fn take(token: &str) -> Option<PreparedCall> {
        let mut guard = PENDING.lock().unwrap();
        let list = guard.as_mut()?;
        let index = list.iter().position(|call| call.token == token)?;
        Some(list.remove(index))
    }
}

/// Drop a held confirmation without executing it. Agent session teardown uses
/// this so a token can never survive into a later conversation.
pub fn discard(token: &str) -> bool {
    PendingCalls::take(token).is_some()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// A monotonic-ish confirmation token. The nanosecond clock plus the canonical id
/// is enough — tokens are short-lived and single-user.
fn mint_token(canonical_id: &str) -> String {
    let _ = canonical_id; // kept in the signature so call sites express intent
    format!("pc_{}", uuid::Uuid::new_v4().simple())
}

/// Build the exact prepared call for a retrieved action, or an immediate failure
/// outcome when it cannot be prepared (unknown action, missing required argument).
///
/// `risk` and `side_effect` come from the host's classification of the action, not
/// from the model or the arguments.
pub fn prepare(
    canonical_id: &str,
    extension_id: &str,
    action_id: &str,
    provider_name: &str,
    arguments: Value,
    risk: RiskClass,
    side_effect: SideEffect,
    manifest_digest: &str,
) -> PreparedCall {
    let idempotency_key = (side_effect == SideEffect::Write)
        .then(|| default_idempotency_key(canonical_id, &arguments));
    let prepared_at_ms = now_ms();
    PreparedCall {
        token: mint_token(canonical_id),
        canonical_id: canonical_id.to_string(),
        extension_id: extension_id.to_string(),
        action_id: action_id.to_string(),
        provider_name: provider_name.to_string(),
        arguments,
        risk,
        side_effect,
        manifest_digest: manifest_digest.to_string(),
        idempotency_key,
        prepared_at_ms,
        expires_at_ms: prepared_at_ms + CONFIRM_TTL_MS,
    }
}

/// Route a prepared call: `Safe` executes now; `Confirm` is withheld and returned
/// as an interaction for the user.
pub async fn run_or_confirm(app: &AppHandle, prepared: PreparedCall, title: &str) -> Dispatch {
    run_or_confirm_opt(Some(app), prepared, title).await
}

/// Optional-AppHandle variant of `run_or_confirm` used by headless tests and gates.
pub async fn run_or_confirm_opt(
    app: Option<&AppHandle>,
    prepared: PreparedCall,
    title: &str,
) -> Dispatch {
    if prepared.risk.needs_confirmation() {
        let interaction = confirm_interaction(&prepared, title);
        PendingCalls::insert(prepared);
        Dispatch::AwaitConfirm(interaction)
    } else {
        let Some(app) = app else {
            return Dispatch::Ran(failed(
                FailureClass::Internal,
                "AppHandle required for action execution.",
            ));
        };
        Dispatch::Ran(execute_revalidated(app, &prepared).await)
    }
}

/// Approve (or decline) a held confirmation and run the *exact* call. Revalidates
/// at time-of-use; a stale call is refused rather than run behind the user's back.
pub async fn resume(app: &AppHandle, token: &str, approve: bool) -> ActionOutcome {
    resume_opt(Some(app), token, approve).await
}

/// Optional-AppHandle variant of `resume` used by tests when approving or rejecting without AppHandle.
pub async fn resume_opt(app: Option<&AppHandle>, token: &str, approve: bool) -> ActionOutcome {
    let Some(prepared) = PendingCalls::take(token) else {
        return ActionOutcome::Failed {
            class: FailureClass::NotFound,
            message: "That confirmation has expired or was already handled.".to_string(),
        };
    };
    if !approve {
        return ActionOutcome::Cancelled;
    }
    let Some(app) = app else {
        return ActionOutcome::Failed {
            class: FailureClass::Internal,
            message: "AppHandle required to execute approved action.".to_string(),
        };
    };
    let Some(current_digest) = current_manifest_digest(app, &prepared) else {
        return ActionOutcome::Failed {
            class: FailureClass::Cancelled,
            message:
                "The extension action is no longer approved or available — please ask again."
                    .to_string(),
        };
    };
    match prepared.still_valid(&current_digest, now_ms()) {
        Ok(()) => execute(app, &prepared).await,
        Err(Stale::Expired) => ActionOutcome::Failed {
            class: FailureClass::Cancelled,
            message: "That confirmation expired before you approved it — please ask again."
                .to_string(),
        },
        Err(Stale::ManifestChanged) => ActionOutcome::Failed {
            class: FailureClass::Cancelled,
            message: "The extension changed since you were asked — please ask again.".to_string(),
        },
    }
}

/// The manifest digest currently in force for an extension, for time-of-use
/// revalidation. Built-in providers are versioned with the app, so a constant is
/// correct; third-party providers will read the installed pack digest (Phase 3b).
fn current_manifest_digest(app: &AppHandle, prepared: &PreparedCall) -> Option<String> {
    if prepared.extension_id == GRAIN_SPACE_EXT_ID {
        Some("builtin".to_string())
    } else if prepared.extension_id.starts_with("mcp.") {
        // The actual MCP tool-set digest is fetched and compared immediately
        // before the call, on the same short-lived stateless service. Returning
        // the prepared digest here preserves the generic confirmation contract
        // without adding a separate network TOCTOU window.
        crate::grain_mcp::is_enabled_extension(app, &prepared.extension_id)
            .then(|| prepared.manifest_digest.clone())
    } else {
        crate::extension_host::approved_action_digest(
            app,
            &prepared.extension_id,
            &prepared.action_id,
        )
    }
}

/// Revalidate even safe calls. A safe action skips user confirmation, not the
/// approval/update time-of-use gate.
async fn execute_revalidated(app: &AppHandle, prepared: &PreparedCall) -> ActionOutcome {
    let Some(current_digest) = current_manifest_digest(app, prepared) else {
        return failed(
            FailureClass::Cancelled,
            "The extension action is no longer approved or available.",
        );
    };
    match prepared.still_valid(&current_digest, now_ms()) {
        Ok(()) => execute(app, prepared).await,
        Err(Stale::Expired | Stale::ManifestChanged) => failed(
            FailureClass::Cancelled,
            "The extension action changed or expired before it could run.",
        ),
    }
}

/// Execute a prepared call against its provider.
async fn execute(app: &AppHandle, prepared: &PreparedCall) -> ActionOutcome {
    if prepared.extension_id == GRAIN_SPACE_EXT_ID {
        grain_space_execute(app, prepared).await
    } else if let Some(provider_id) = prepared.extension_id.strip_prefix("mcp.") {
        mcp_execute(app, provider_id, prepared).await
    } else {
        third_party_execute(app, prepared).await
    }
}

async fn mcp_execute(app: &AppHandle, provider_id: &str, prepared: &PreparedCall) -> ActionOutcome {
    match crate::grain_mcp::call_tool(
        app,
        provider_id,
        &prepared.action_id,
        &prepared.arguments,
        &prepared.manifest_digest,
    )
    .await
    {
        Ok(output) if output.is_error => ActionOutcome::Failed {
            class: FailureClass::Internal,
            message: output.text,
        },
        Ok(output) => ActionOutcome::Succeeded(SuccessData {
            source: Some(prepared.provider_name.clone()),
            title: Some(prepared.action_id.clone()),
            body: Some(output.text),
            details: Vec::new(),
            receipt: true,
        }),
        Err(error) => ActionOutcome::Failed {
            class: FailureClass::Network,
            message: format!("The MCP action did not run: {error}"),
        },
    }
}

/// Execute a third-party action through the extension worker's `"action"` handler
/// (Phase 3b, Rust half). The worker returns a structured result mapped to an
/// [`ActionOutcome`]; a side-effecting timeout is `UnknownOutcome` (it may have
/// run), a read timeout is a plain failure. The worker-side handler is the
/// extension-runtime handler; malformed or unavailable results fail closed.
async fn third_party_execute(app: &AppHandle, prepared: &PreparedCall) -> ActionOutcome {
    use crate::extension_host::ActionCallError;
    match crate::extension_host::run_action(
        app,
        &prepared.extension_id,
        &prepared.action_id,
        &prepared.arguments,
        prepared.idempotency_key.as_deref(),
    )
    .await
    {
        Ok(value) => parse_worker_outcome(value, prepared),
        Err(ActionCallError::Timeout) => {
            if prepared.side_effect == SideEffect::Write {
                ActionOutcome::UnknownOutcome {
                    message: "The extension did not respond in time — it may or may not have \
                              completed. Do not claim it is done."
                        .to_string(),
                }
            } else {
                failed(
                    FailureClass::Network,
                    "The extension did not respond in time.",
                )
            }
        }
        Err(ActionCallError::Unavailable(message)) => failed(FailureClass::Internal, &message),
    }
}

/// Map the worker's structured `"action"` reply to an [`ActionOutcome`]. Shape:
/// `{ "error": {class?, message} }` | `{ "needsInteraction": <Interaction> }` |
/// `{ "ok": {title?, body?, details?} }`. The envelope is strict; provenance
/// and write receipts are host-owned and worker error text never crosses the
/// trust boundary.
fn parse_worker_outcome(value: Value, prepared: &PreparedCall) -> ActionOutcome {
    let Some(root) = value.as_object() else {
        return failed(
            FailureClass::Internal,
            "The extension returned an invalid action result.",
        );
    };
    let recognized = ["error", "needsInteraction", "ok"]
        .into_iter()
        .filter(|key| root.contains_key(*key))
        .count();
    if recognized != 1 || root.len() != 1 {
        return failed(
            FailureClass::Internal,
            "The extension returned an invalid action result.",
        );
    }

    if let Some(error) = root.get("error") {
        let class = error
            .get("class")
            .and_then(Value::as_str)
            .map(map_failure_class)
            .unwrap_or(FailureClass::Internal);
        // A worker exception may contain credentials, request bodies, or prompt
        // injection. The host maps only its coarse class to user/model text.
        return failed(class, worker_failure_message(class));
    }
    if root.contains_key("needsInteraction") {
        // Initial V2 has no continuation token for extension-authored follow-up.
        // Failing honestly is safer than rendering an interaction that cannot be
        // resumed or accepting a worker-minted confirmation token.
        return failed(
            FailureClass::Internal,
            "This action needs more input, but extension follow-up is not available yet.",
        );
    }

    let ok = &root["ok"];
    let (title, body, details) = if let Some(text) = ok.as_str() {
        (None, bounded_text(text, 4 * 1024), Vec::new())
    } else if ok.is_null() {
        (None, None, Vec::new())
    } else if let Some(object) = ok.as_object() {
        let title = object
            .get("title")
            .and_then(Value::as_str)
            .and_then(|text| bounded_text(text, 160));
        let body = object
            .get("body")
            .and_then(Value::as_str)
            .and_then(|text| bounded_text(text, 4 * 1024));
        let mut details: Vec<Field> = object
            .get("details")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(16)
            .filter_map(|row| {
                let label = bounded_text(row.get("label")?.as_str()?, 80)?;
                let value = bounded_text(row.get("value")?.as_str()?, 600)?;
                Some(Field { label, value })
            })
            .collect();
        // Plain object data is still useful. Promote unreserved scalar fields to
        // bounded details rather than silently claiming success with "Done".
        if title.is_none() && body.is_none() && details.is_empty() {
            details.extend(object.iter().take(16).filter_map(|(label, value)| {
                let label = bounded_text(label, 80)?;
                let value = value_to_string(value).and_then(|text| bounded_text(&text, 600))?;
                Some(Field { label, value })
            }));
        }
        (title, body, details)
    } else {
        return failed(
            FailureClass::Internal,
            "The extension returned an invalid action result.",
        );
    };

    ActionOutcome::Succeeded(SuccessData {
        // Provenance is host-owned; a worker cannot label itself "GitHub".
        source: Some(prepared.provider_name.clone()),
        title,
        body,
        details,
        // A worker cannot hide a host-classified write by returning receipt:false.
        receipt: prepared.side_effect == SideEffect::Write,
    })
}

fn bounded_text(text: &str, max_bytes: usize) -> Option<String> {
    let clean = grain_core::capability_agent::sanitize(text, max_bytes);
    (!clean.is_empty()).then_some(clean)
}

fn worker_failure_message(class: FailureClass) -> &'static str {
    match class {
        FailureClass::Auth => "Connect or reauthorize the account, then try again.",
        FailureClass::Network => "The extension could not reach its service.",
        FailureClass::InvalidArgument => "The extension rejected the action arguments.",
        FailureClass::NotFound => "The requested item or action was not found.",
        FailureClass::RateLimited => "The service is rate-limited; try again shortly.",
        FailureClass::Cancelled => "The action was cancelled.",
        FailureClass::Internal => "The extension action failed.",
    }
}

fn map_failure_class(raw: &str) -> FailureClass {
    match raw {
        "auth" => FailureClass::Auth,
        "network" => FailureClass::Network,
        "invalid" | "invalid_argument" => FailureClass::InvalidArgument,
        "not_found" | "notfound" => FailureClass::NotFound,
        "rate_limited" | "ratelimited" => FailureClass::RateLimited,
        "cancelled" => FailureClass::Cancelled,
        _ => FailureClass::Internal,
    }
}

fn confirm_interaction(prepared: &PreparedCall, title: &str) -> Interaction {
    let details = prepared
        .arguments
        .as_object()
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    value_to_string(value).map(|value| Field {
                        label: key.clone(),
                        value,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let side_effect = match prepared.side_effect {
        SideEffect::Write => "Makes a change".to_string(),
        SideEffect::Read => "Reads your data".to_string(),
        SideEffect::None => String::new(),
    };
    Interaction::Confirm {
        token: prepared.token.clone(),
        title: title.to_string(),
        summary: String::new(),
        details,
        side_effect,
        destinations: Vec::new(),
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

// ── The Grain Space built-in provider ────────────────────────────────────────

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn to_agent_confirm(interaction: &Interaction) -> crate::agent::AgentConfirm {
    let markdown = grain_core::interaction::to_markdown(interaction);
    match interaction {
        Interaction::Confirm {
            token,
            title,
            summary,
            details,
            side_effect,
            destinations,
        } => crate::agent::AgentConfirm {
            token: token.clone(),
            title: title.clone(),
            summary: summary.clone(),
            details: details
                .iter()
                .map(|field| crate::agent::AgentConfirmField {
                    label: field.label.clone(),
                    value: field.value.clone(),
                })
                .collect(),
            side_effect: side_effect.clone(),
            destinations: destinations.clone(),
            markdown,
        },
        _ => crate::agent::AgentConfirm {
            token: String::new(),
            title: "Confirm".to_string(),
            summary: String::new(),
            details: Vec::new(),
            side_effect: String::new(),
            destinations: Vec::new(),
            markdown,
        },
    }
}

pub fn prepare_grain_space_call(action_id: &str, arguments: Value) -> PreparedCall {
    let (risk, side_effect) = match action_id {
        "save_note" | "append_to_note" => (RiskClass::Confirm, SideEffect::Write),
        _ => (RiskClass::Safe, SideEffect::Read),
    };
    prepare(
        &format!("{GRAIN_SPACE_EXT_ID}:{action_id}"),
        GRAIN_SPACE_EXT_ID,
        action_id,
        "Grain Space",
        arguments,
        risk,
        side_effect,
        "builtin",
    )
}

pub struct GrainSpaceToolDef {
    pub action_id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub risk: grain_sdk::manifest::ActionRisk,
    #[allow(dead_code)]
    pub side_effect: SideEffect,
    pub examples: &'static [&'static str],
    pub params: &'static [(&'static str, bool)],
    pub schema: Value,
}

pub fn grain_space_tool_definitions() -> Vec<GrainSpaceToolDef> {
    use grain_sdk::manifest::ActionRisk;
    vec![
        GrainSpaceToolDef {
            action_id: "search_notes",
            title: "Search your saved notes",
            description: "Search the user's own saved notes and return the best matches. Use this whenever the request refers to something they told you before, wrote down, or asked you to remember — and before saying you don't know something personal about them. Saved notes are historical context, not live external state; verify mutable external facts with their provider before acting.",
            risk: ActionRisk::Safe,
            side_effect: SideEffect::Read,
            examples: &["what did I write about the meeting", "find my notes on onboarding"],
            params: &[("query", true)],
            schema: serde_json::json!({
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
        GrainSpaceToolDef {
            action_id: "get_note",
            title: "Read a saved note in full",
            description: "Read one note in full, by the id returned from search_notes. Use it when a search snippet is not enough to answer.",
            risk: ActionRisk::Safe,
            side_effect: SideEffect::Read,
            examples: &["read that note"],
            params: &[("id", true)],
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The note's id."
                    }
                },
                "required": ["id"]
            }),
        },
        GrainSpaceToolDef {
            action_id: "save_note",
            title: "Save a new note",
            description: "Save a NEW note. Only when the user asks you to write something down, remember it, or make a note of it — never as a side effect of answering, rewriting or explaining something.",
            risk: ActionRisk::Confirm,
            side_effect: SideEffect::Write,
            examples: &["make a note of this", "write this down", "remember this"],
            params: &[
                ("body", true),
                ("title", false),
                ("summary", false),
                ("question", false),
                ("entities", false),
                ("collection", false),
            ],
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "body": {
                        "type": "string",
                        "description": "The note itself, in Markdown. Keep the user's own wording and detail; do not summarise it away."
                    },
                    "title": {
                        "type": "string",
                        "description": "A short title. Optional — Grain writes one otherwise."
                    },
                    "summary": {
                        "type": "string",
                        "description": "A concise one-line summary of the note. Optional."
                    },
                    "question": {
                        "type": "string",
                        "description": "The core question or problem this note answers or addresses. Optional."
                    },
                    "entities": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Key entities, topics, or tags mentioned in the note. Optional."
                    },
                    "collection": {
                        "type": "string",
                        "description": "An existing collection to file it under, from list_collections. Optional."
                    }
                },
                "required": ["body"]
            }),
        },
        GrainSpaceToolDef {
            action_id: "append_to_note",
            title: "Add to an existing note",
            description: "Add text to the end of a note that already exists, by id. Use this rather than save_note when the user is adding to something.",
            risk: ActionRisk::Confirm,
            side_effect: SideEffect::Write,
            examples: &["add this to that note"],
            params: &[("id", true), ("text", true)],
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The note's id."
                    },
                    "text": {
                        "type": "string",
                        "description": "What to add, in Markdown."
                    }
                },
                "required": ["id", "text"]
            }),
        },
        GrainSpaceToolDef {
            action_id: "list_collections",
            title: "List note collections",
            description: "List the collections the user files notes under. Use before save_note when they say where a note belongs.",
            risk: ActionRisk::Safe,
            side_effect: SideEffect::Read,
            examples: &["what collections do I have"],
            params: &[],
            schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

pub fn grain_space_tool_specs() -> Vec<crate::llm_client::ToolSpec> {
    grain_space_tool_definitions()
        .into_iter()
        .map(|def| crate::llm_client::ToolSpec {
            name: def.action_id.to_string(),
            description: def.description.to_string(),
            parameters: def.schema,
        })
        .collect()
}

async fn grain_space_execute(app: &AppHandle, prepared: &PreparedCall) -> ActionOutcome {
    const SEARCH_LIMIT: usize = 6;
    let source = Some("Grain Space".to_string());
    let args = &prepared.arguments;
    match prepared.action_id.as_str() {
        "search_notes" => {
            let Some(query) = str_arg(args, "query") else {
                return invalid("search_notes needs a query.");
            };
            match crate::grain_space::search(app, &query, SEARCH_LIMIT).await {
                Ok(hits) if hits.is_empty() => ActionOutcome::Succeeded(SuccessData {
                    source,
                    title: Some("No matching notes".to_string()),
                    body: Some(format!("No saved notes match \"{query}\".")),
                    details: vec![],
                    receipt: false,
                }),
                Ok(hits) => {
                    let details = hits
                        .iter()
                        .map(|hit| Field {
                            label: hit.title.clone(),
                            value: hit.snippet.clone(),
                        })
                        .collect();
                    ActionOutcome::Succeeded(SuccessData {
                        source,
                        title: Some("Notes found".to_string()),
                        body: None,
                        details,
                        receipt: false,
                    })
                }
                Err(e) => failed(
                    FailureClass::Internal,
                    &format!("Could not search notes: {e}"),
                ),
            }
        }
        "get_note" => {
            let Some(id) = str_arg(args, "id") else {
                return invalid("get_note needs an id.");
            };
            match crate::grain_space::get(app, &id).await {
                Ok(note) => ActionOutcome::Succeeded(SuccessData {
                    source,
                    title: Some(note.title.clone()),
                    body: Some(note.body.clone()),
                    details: vec![],
                    receipt: false,
                }),
                Err(e) => failed(
                    FailureClass::NotFound,
                    &format!("Could not read that note: {e}"),
                ),
            }
        }
        "list_collections" => match crate::grain_space::collections(app).await {
            Ok(list) if list.is_empty() => ActionOutcome::Succeeded(SuccessData {
                source,
                title: Some("Collections".to_string()),
                body: Some("There are no collections yet.".to_string()),
                details: vec![],
                receipt: false,
            }),
            Ok(list) => ActionOutcome::Succeeded(SuccessData {
                source,
                title: Some("Collections".to_string()),
                body: Some(list.join(", ")),
                details: vec![],
                receipt: false,
            }),
            Err(e) => failed(
                FailureClass::Internal,
                &format!("Could not list collections: {e}"),
            ),
        },
        "save_note" => {
            let Some(body) = str_arg(args, "body") else {
                return invalid("save_note needs a body.");
            };
            let entities: Vec<String> = args
                .get("entities")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .or_else(|| {
                    str_arg(args, "entities").map(|s| {
                        s.split(',')
                            .map(|part| part.trim().to_string())
                            .filter(|part| !part.is_empty())
                            .collect()
                    })
                })
                .unwrap_or_default();
            let supplied = crate::grain_space::SuppliedMeta {
                title: str_arg(args, "title"),
                summary: str_arg(args, "summary"),
                question: str_arg(args, "question"),
                entities,
                collection: str_arg(args, "collection"),
            };
            match crate::grain_space::save(app, &body, supplied).await {
                Ok(id) => {
                    let title = crate::grain_space::get(app, &id)
                        .await
                        .map(|note| note.title)
                        .unwrap_or_else(|_| "note".to_string());
                    ActionOutcome::Succeeded(SuccessData {
                        source,
                        title: Some("Saved note".to_string()),
                        body: Some(format!("Saved as \"{title}\".")),
                        details: vec![Field {
                            label: "id".to_string(),
                            value: id,
                        }],
                        receipt: true,
                    })
                }
                Err(e) => failed(
                    FailureClass::Internal,
                    &format!("Could not save the note: {e}"),
                ),
            }
        }
        "append_to_note" => {
            let (Some(id), Some(text)) = (str_arg(args, "id"), str_arg(args, "text")) else {
                return invalid("append_to_note needs an id and text.");
            };
            match crate::grain_space::append(app, &id, &text).await {
                Ok(()) => ActionOutcome::Succeeded(SuccessData {
                    source,
                    title: Some("Updated note".to_string()),
                    body: Some("Added to the note.".to_string()),
                    details: vec![Field {
                        label: "id".to_string(),
                        value: id,
                    }],
                    receipt: true,
                }),
                Err(e) => {
                    let class = if e.to_lowercase().contains("not found") {
                        FailureClass::NotFound
                    } else {
                        FailureClass::Internal
                    };
                    failed(class, &format!("Could not update that note: {e}"))
                }
            }
        }
        other => failed(
            FailureClass::NotFound,
            &format!("Grain Space has no action '{other}'."),
        ),
    }
}

/// Standalone vault-level execution for unit and contract testing without AppHandle.
#[allow(dead_code)]
pub fn execute_grain_space_on_vault(
    vault: &crate::grain_space::vault::Vault,
    action_id: &str,
    args: &Value,
) -> ActionOutcome {
    use crate::grain_space::note::Note;
    use crate::grain_space::vault;

    const SEARCH_LIMIT: usize = 6;
    let source = Some("Grain Space".to_string());
    match action_id {
        "search_notes" => {
            let Some(query) = str_arg(args, "query") else {
                return invalid("search_notes needs a query.");
            };
            match vault::search_notes(vault, &query) {
                Ok(notes) if notes.is_empty() => ActionOutcome::Succeeded(SuccessData {
                    source,
                    title: Some("No matching notes".to_string()),
                    body: Some(format!("No saved notes match \"{query}\".")),
                    details: vec![],
                    receipt: false,
                }),
                Ok(notes) => {
                    let details = notes
                        .into_iter()
                        .take(SEARCH_LIMIT)
                        .map(|n| Field {
                            label: n.title,
                            value: if !n.tldr.is_empty() {
                                n.tldr
                            } else {
                                n.body.chars().take(200).collect()
                            },
                        })
                        .collect();
                    ActionOutcome::Succeeded(SuccessData {
                        source,
                        title: Some("Notes found".to_string()),
                        body: None,
                        details,
                        receipt: false,
                    })
                }
                Err(e) => failed(
                    FailureClass::Internal,
                    &format!("Could not search notes: {e}"),
                ),
            }
        }
        "get_note" => {
            let Some(id) = str_arg(args, "id") else {
                return invalid("get_note needs an id.");
            };
            match vault::get_note(vault, &id) {
                Ok(note) => ActionOutcome::Succeeded(SuccessData {
                    source,
                    title: Some(note.title.clone()),
                    body: Some(note.body.clone()),
                    details: vec![],
                    receipt: false,
                }),
                Err(e) => failed(
                    FailureClass::NotFound,
                    &format!("Could not read that note: {e}"),
                ),
            }
        }
        "list_collections" => match vault::list_folders(vault) {
            Ok(list) if list.is_empty() => ActionOutcome::Succeeded(SuccessData {
                source,
                title: Some("Collections".to_string()),
                body: Some("There are no collections yet.".to_string()),
                details: vec![],
                receipt: false,
            }),
            Ok(list) => ActionOutcome::Succeeded(SuccessData {
                source,
                title: Some("Collections".to_string()),
                body: Some(list.join(", ")),
                details: vec![],
                receipt: false,
            }),
            Err(e) => failed(
                FailureClass::Internal,
                &format!("Could not list collections: {e}"),
            ),
        },
        "save_note" => {
            let Some(body) = str_arg(args, "body") else {
                return invalid("save_note needs a body.");
            };
            let mut note = Note::raw(body.trim().to_string());
            if let Some(t) = str_arg(args, "title") {
                note.title = t.chars().take(80).collect();
            }
            if let Some(s) = str_arg(args, "summary") {
                note.tldr = s;
            }
            if let Some(q) = str_arg(args, "question") {
                note.question = q.chars().take(240).collect();
            }
            let entities: Vec<String> = args
                .get("entities")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .or_else(|| {
                    str_arg(args, "entities").map(|s| {
                        s.split(',')
                            .map(|part| part.trim().to_string())
                            .filter(|part| !part.is_empty())
                            .collect()
                    })
                })
                .unwrap_or_default();
            note.entities = entities;
            let id = note.id.clone();
            let title = note.title.clone();
            let save_res = vault::save_note(vault, &note);
            if let Some(col) = str_arg(args, "collection") {
                let _ = vault::move_note_to_folder(vault, &id, Some(&col));
            }
            match save_res {
                Ok(()) => ActionOutcome::Succeeded(SuccessData {
                    source,
                    title: Some("Saved note".to_string()),
                    body: Some(format!("Saved as \"{title}\".")),
                    details: vec![Field {
                        label: "id".to_string(),
                        value: id,
                    }],
                    receipt: true,
                }),
                Err(e) => failed(
                    FailureClass::Internal,
                    &format!("Could not save the note: {e}"),
                ),
            }
        }
        "append_to_note" => {
            let (Some(id), Some(text)) = (str_arg(args, "id"), str_arg(args, "text")) else {
                return invalid("append_to_note needs an id and text.");
            };
            match vault::get_note(vault, &id) {
                Ok(mut note) => {
                    note.body = format!("{}\n\n---\n\n{}", note.body.trim_end(), text.trim());
                    match vault::save_note(vault, &note) {
                        Ok(()) => ActionOutcome::Succeeded(SuccessData {
                            source,
                            title: Some("Updated note".to_string()),
                            body: Some("Added to the note.".to_string()),
                            details: vec![Field {
                                label: "id".to_string(),
                                value: id,
                            }],
                            receipt: true,
                        }),
                        Err(e) => failed(
                            FailureClass::Internal,
                            &format!("Could not update that note: {e}"),
                        ),
                    }
                }
                Err(e) => failed(
                    FailureClass::NotFound,
                    &format!("Could not find that note: {e}"),
                ),
            }
        }
        other => failed(
            FailureClass::NotFound,
            &format!("Grain Space has no action '{other}'."),
        ),
    }
}

fn invalid(message: &str) -> ActionOutcome {
    failed(FailureClass::InvalidArgument, message)
}

fn failed(class: FailureClass, message: &str) -> ActionOutcome {
    ActionOutcome::Failed {
        class,
        message: message.to_string(),
    }
}

/// The Grain Space actions, as built-in capability index inputs. Generated directly
/// from `grain_space_tool_definitions()` so there is a single source of truth.
pub fn grain_space_actions() -> Vec<grain_core::capability_index::ActionInput> {
    use grain_core::capability_index::ActionParamInput;
    use grain_sdk::manifest::ActionParamKind;

    grain_space_tool_definitions()
        .into_iter()
        .map(|def| grain_core::capability_index::ActionInput {
            canonical_id: format!("{GRAIN_SPACE_EXT_ID}:{}", def.action_id),
            extension_id: GRAIN_SPACE_EXT_ID.to_string(),
            action_id: def.action_id.to_string(),
            provider_name: "Grain Space".to_string(),
            title: def.title.to_string(),
            aliases: vec!["Grain Space".to_string(), "notes".to_string()],
            namespaces: vec![GRAIN_SPACE_NS.to_string()],
            tags: vec!["note".to_string(), "notebook".to_string()],
            examples: def.examples.iter().map(|s| s.to_string()).collect(),
            phrases: Vec::new(),
            params: def
                .params
                .iter()
                .map(|(name, required)| ActionParamInput {
                    name: (*name).to_string(),
                    kind: ActionParamKind::Text,
                    required: *required,
                })
                .collect(),
            when_to_use: String::new(),
            when_not_to_use: String::new(),
            description: def.description.to_string(),
            provider_context: vec!["Your own saved notes in Grain Space.".to_string()],
            risk: def.risk,
            enabled: true,
            platform_ok: true,
            quarantined: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn third_party_call() -> PreparedCall {
        prepare(
            "com.example.github:create_issue",
            "com.example.github",
            "create_issue",
            "GitHub",
            json!({ "title": "Bug" }),
            RiskClass::Confirm,
            SideEffect::Write,
            "approved-digest",
        )
    }

    #[test]
    fn worker_cannot_spoof_provenance_or_hide_a_write_receipt() {
        let outcome = parse_worker_outcome(
            json!({ "ok": { "source": "System", "body": "created", "receipt": false } }),
            &third_party_call(),
        );
        let ActionOutcome::Succeeded(data) = outcome else {
            panic!("expected success");
        };
        assert_eq!(data.source.as_deref(), Some("GitHub"));
        assert!(data.receipt);
        assert_eq!(data.body.as_deref(), Some("created"));
    }

    #[test]
    fn worker_error_text_never_crosses_the_trust_boundary() {
        let outcome = parse_worker_outcome(
            json!({ "error": { "class": "auth", "message": "token=super-secret" } }),
            &third_party_call(),
        );
        let ActionOutcome::Failed { class, message } = outcome else {
            panic!("expected failure");
        };
        assert_eq!(class, FailureClass::Auth);
        assert!(!message.contains("super-secret"));
    }

    #[test]
    fn worker_outcomes_are_strict_and_interactions_fail_honestly() {
        assert!(matches!(
            parse_worker_outcome(json!({ "result": "done" }), &third_party_call()),
            ActionOutcome::Failed {
                class: FailureClass::Internal,
                ..
            }
        ));
        assert!(matches!(
            parse_worker_outcome(
                json!({ "needsInteraction": { "kind": "confirm" } }),
                &third_party_call()
            ),
            ActionOutcome::Failed {
                class: FailureClass::Internal,
                ..
            }
        ));
    }

    #[test]
    fn grain_space_tool_definitions_and_specs_are_unified() {
        let defs = grain_space_tool_definitions();
        let specs = grain_space_tool_specs();
        let actions = grain_space_actions();

        assert_eq!(defs.len(), 5);
        assert_eq!(specs.len(), 5);
        assert_eq!(actions.len(), 5);

        let save_def = defs.iter().find(|d| d.action_id == "save_note").unwrap();
        assert_eq!(save_def.risk, grain_sdk::manifest::ActionRisk::Confirm);
        assert_eq!(save_def.side_effect, SideEffect::Write);

        let save_spec = specs.iter().find(|s| s.name == "save_note").unwrap();
        let props = save_spec.parameters.get("properties").unwrap();
        assert!(props.get("body").is_some());
        assert!(props.get("title").is_some());
        assert!(props.get("summary").is_some());
        assert!(props.get("question").is_some());
        assert!(props.get("entities").is_some());
        assert!(props.get("collection").is_some());
    }

    #[test]
    fn grain_space_prepared_calls_classify_risk_and_side_effects() {
        let save_call = prepare_grain_space_call(
            "save_note",
            json!({ "body": "Remember the deploy timeout is 45s", "title": "Deploy timeout" }),
        );
        assert_eq!(save_call.risk, RiskClass::Confirm);
        assert_eq!(save_call.side_effect, SideEffect::Write);
        assert!(save_call.token.starts_with("pc_"));
        assert!(save_call.risk.needs_confirmation());

        let append_call = prepare_grain_space_call(
            "append_to_note",
            json!({ "id": "note-123", "text": "Additional context" }),
        );
        assert_eq!(append_call.risk, RiskClass::Confirm);
        assert_eq!(append_call.side_effect, SideEffect::Write);

        let search_call = prepare_grain_space_call(
            "search_notes",
            json!({ "query": "deploy timeout" }),
        );
        assert_eq!(search_call.risk, RiskClass::Safe);
        assert_eq!(search_call.side_effect, SideEffect::Read);
        assert!(!search_call.risk.needs_confirmation());
    }

    #[test]
    fn pending_calls_store_take_and_discard() {
        let call = prepare_grain_space_call(
            "save_note",
            json!({ "body": "Held confirmation test" }),
        );
        let token = call.token.clone();

        PendingCalls::insert(call);
        assert!(discard(&token));
        assert!(PendingCalls::take(&token).is_none());
    }

    #[test]
    fn execute_grain_space_on_vault_handles_full_lifecycle() {
        let temp = tempfile::tempdir().unwrap();
        let vault = crate::grain_space::vault::Vault::native(temp.path().to_path_buf());

        // 1. Save note with full metadata
        let save_args = json!({
            "body": "Auth endpoints require PKCE verification with S256 challenge.",
            "title": "PKCE Auth Rule",
            "summary": "PKCE auth requires S256 verification.",
            "question": "How is PKCE challenge verified?",
            "entities": ["auth", "pkce", "security"],
            "collection": "Security"
        });
        let outcome = execute_grain_space_on_vault(&vault, "save_note", &save_args);
        let ActionOutcome::Succeeded(data) = outcome else {
            panic!("expected successful save");
        };
        let note_id = data.details.iter().find(|f| f.label == "id").unwrap().value.clone();
        assert!(!note_id.is_empty());

        // 2. Read note back and verify fields
        let get_args = json!({ "id": note_id });
        let get_outcome = execute_grain_space_on_vault(&vault, "get_note", &get_args);
        let ActionOutcome::Succeeded(get_data) = get_outcome else {
            panic!("expected successful get");
        };
        assert_eq!(get_data.title.as_deref(), Some("PKCE Auth Rule"));
        assert!(get_data.body.as_deref().unwrap().contains("PKCE verification"));

        // 3. Append to note
        let append_args = json!({
            "id": note_id,
            "text": "Token refresh window is 300 seconds."
        });
        let append_outcome = execute_grain_space_on_vault(&vault, "append_to_note", &append_args);
        assert!(matches!(append_outcome, ActionOutcome::Succeeded(_)));

        // 4. Verify appended body
        let get_after_append = execute_grain_space_on_vault(&vault, "get_note", &get_args);
        let ActionOutcome::Succeeded(after_data) = get_after_append else {
            panic!("expected successful get after append");
        };
        let body = after_data.body.unwrap();
        assert!(body.contains("PKCE verification"));
        assert!(body.contains("Token refresh window is 300 seconds."));

        // 5. Non-existent note returns NotFound
        let bad_get = execute_grain_space_on_vault(&vault, "get_note", &json!({ "id": "nonexistent" }));
        assert!(matches!(bad_get, ActionOutcome::Failed { class: FailureClass::NotFound, .. }));

        // 6. List collections shows Security
        let colls = execute_grain_space_on_vault(&vault, "list_collections", &json!({}));
        let ActionOutcome::Succeeded(coll_data) = colls else {
            panic!("expected collections");
        };
        assert!(coll_data.body.unwrap().contains("Security"));
    }
}
