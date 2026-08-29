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
//! provider; third-party execution reports honestly that it is not wired yet and
//! never fabricates success (§8.1).

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

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// A monotonic-ish confirmation token. The nanosecond clock plus the canonical id
/// is enough — tokens are short-lived and single-user.
fn mint_token(canonical_id: &str) -> String {
    format!("pc_{}_{}", now_ms(), canonical_id.replace('.', "_"))
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
    if prepared.risk.needs_confirmation() {
        let interaction = confirm_interaction(&prepared, title);
        PendingCalls::insert(prepared);
        Dispatch::AwaitConfirm(interaction)
    } else {
        Dispatch::Ran(execute(app, &prepared).await)
    }
}

/// Approve (or decline) a held confirmation and run the *exact* call. Revalidates
/// at time-of-use; a stale call is refused rather than run behind the user's back.
pub async fn resume(app: &AppHandle, token: &str, approve: bool) -> ActionOutcome {
    let Some(prepared) = PendingCalls::take(token) else {
        return ActionOutcome::Failed {
            class: FailureClass::NotFound,
            message: "That confirmation has expired or was already handled.".to_string(),
        };
    };
    if !approve {
        return ActionOutcome::Cancelled;
    }
    match prepared.still_valid(current_manifest_digest(&prepared.extension_id), now_ms()) {
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
fn current_manifest_digest(extension_id: &str) -> &'static str {
    if extension_id == GRAIN_SPACE_EXT_ID {
        "builtin"
    } else {
        // Third-party execution is not wired yet; a mismatch here would only ever
        // refuse, which is the safe direction.
        "unwired"
    }
}

/// Execute a prepared call against its provider.
async fn execute(app: &AppHandle, prepared: &PreparedCall) -> ActionOutcome {
    if prepared.extension_id == GRAIN_SPACE_EXT_ID {
        grain_space_execute(app, prepared).await
    } else {
        third_party_execute(app, prepared).await
    }
}

/// Execute a third-party action through the extension worker's `"action"` handler
/// (Phase 3b, Rust half). The worker returns a structured result mapped to an
/// [`ActionOutcome`]; a side-effecting timeout is `UnknownOutcome` (it may have
/// run), a read timeout is a plain failure. The worker-side handler is the
/// `ui/grain-2.0` counterpart; until it exists, a call resolves to a timeout or
/// failure — never a fabricated success.
async fn third_party_execute(app: &AppHandle, prepared: &PreparedCall) -> ActionOutcome {
    use crate::extension_host::ActionCallError;
    match crate::extension_host::run_action(
        app,
        &prepared.extension_id,
        &prepared.action_id,
        &prepared.arguments,
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
                failed(FailureClass::Network, "The extension did not respond in time.")
            }
        }
        Err(ActionCallError::Unavailable(message)) => failed(FailureClass::Internal, &message),
    }
}

/// Map the worker's structured `"action"` reply to an [`ActionOutcome`]. Shape:
/// `{ "error": {class?, message} }` | `{ "needsInteraction": <Interaction> }` |
/// `{ "ok": {source?, title?, body?, details?, receipt?} }`. Lenient — an
/// unrecognised value is treated as a plain success body.
fn parse_worker_outcome(value: Value, prepared: &PreparedCall) -> ActionOutcome {
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("The action failed.")
            .to_string();
        let class = error
            .get("class")
            .and_then(Value::as_str)
            .map(map_failure_class)
            .unwrap_or(FailureClass::Internal);
        return ActionOutcome::Failed { class, message };
    }
    if let Some(needs) = value.get("needsInteraction") {
        if let Ok(interaction) = serde_json::from_value::<Interaction>(needs.clone()) {
            return ActionOutcome::NeedsInteraction(interaction);
        }
    }
    let ok = value.get("ok").unwrap_or(&value);
    let source = ok.get("source").and_then(Value::as_str).map(str::to_string);
    let title = ok.get("title").and_then(Value::as_str).map(str::to_string);
    let body = ok
        .get("body")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| ok.as_str().map(str::to_string));
    let details = ok
        .get("details")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let label = row.get("label").and_then(Value::as_str)?;
                    let val = row.get("value").and_then(Value::as_str)?;
                    Some(Field {
                        label: label.to_string(),
                        value: val.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let receipt = ok
        .get("receipt")
        .and_then(Value::as_bool)
        .unwrap_or(prepared.side_effect == SideEffect::Write);
    ActionOutcome::Succeeded(SuccessData {
        source,
        title,
        body,
        details,
        receipt,
    })
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
                Err(e) => failed(FailureClass::Internal, &format!("Could not search notes: {e}")),
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
                Err(e) => failed(FailureClass::NotFound, &format!("Could not read that note: {e}")),
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
            Err(e) => failed(FailureClass::Internal, &format!("Could not list collections: {e}")),
        },
        "save_note" => {
            let Some(body) = str_arg(args, "body") else {
                return invalid("save_note needs a body.");
            };
            let supplied = crate::grain_space::SuppliedMeta {
                title: str_arg(args, "title"),
                summary: None,
                question: None,
                entities: Vec::new(),
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
                        details: vec![],
                        receipt: true,
                    })
                }
                Err(e) => failed(FailureClass::Internal, &format!("Could not save the note: {e}")),
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
                    details: vec![],
                    receipt: true,
                }),
                Err(e) => failed(FailureClass::Internal, &format!("Could not update that note: {e}")),
            }
        }
        other => failed(FailureClass::NotFound, &format!("Grain Space has no action '{other}'.")),
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

/// The Grain Space actions, as built-in capability index inputs. Registered in
/// `extension_host::refresh_index` when Grain Space is enabled, so they are
/// retrieved and executed through the one action path like any other.
pub fn grain_space_actions() -> Vec<grain_core::capability_index::ActionInput> {
    use grain_sdk::manifest::ActionRisk;

    let make = |action_id: &str, title: &str, risk: ActionRisk, examples: &[&str], params: &[&str]| {
        grain_core::capability_index::ActionInput {
            canonical_id: format!("{GRAIN_SPACE_NS}.{action_id}"),
            extension_id: GRAIN_SPACE_EXT_ID.to_string(),
            action_id: action_id.to_string(),
            provider_name: "Grain Space".to_string(),
            title: title.to_string(),
            aliases: vec!["Grain Space".to_string(), "notes".to_string()],
            namespaces: vec![GRAIN_SPACE_NS.to_string()],
            tags: vec!["note".to_string(), "notebook".to_string()],
            examples: examples.iter().map(|s| s.to_string()).collect(),
            phrases: Vec::new(),
            param_names: params.iter().map(|s| s.to_string()).collect(),
            when_to_use: String::new(),
            when_not_to_use: String::new(),
            description: String::new(),
            provider_context: vec!["Your own saved notes in Grain Space.".to_string()],
            risk,
            enabled: true,
            platform_ok: true,
            quarantined: false,
        }
    };

    vec![
        make(
            "search_notes",
            "Search your saved notes",
            ActionRisk::Safe,
            &["what did I write about the meeting", "find my notes on onboarding"],
            &["query"],
        ),
        make("get_note", "Read a saved note in full", ActionRisk::Safe, &["read that note"], &["id"]),
        make(
            "save_note",
            "Save a new note",
            ActionRisk::Confirm,
            &["make a note of this", "write this down", "remember this"],
            &["body", "title", "collection"],
        ),
        make(
            "append_to_note",
            "Add to an existing note",
            ActionRisk::Confirm,
            &["add this to that note"],
            &["id", "text"],
        ),
        make("list_collections", "List note collections", ActionRisk::Safe, &["what collections do I have"], &[]),
    ]
}
