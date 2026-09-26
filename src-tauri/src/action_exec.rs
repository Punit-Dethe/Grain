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
//! the same [`ActionOutcome`]. Third-party actions run through the isolated extension worker and
//! return a strict, host-sanitised outcome (§8.1).

use std::sync::Mutex;

pub use grain_core::execution::ActionOutcome;
use grain_core::execution::{
    default_idempotency_key, FailureClass, PreparedCall, RiskClass, SideEffect, Stale, SuccessData,
};
use grain_core::interaction::{Field, Interaction};
use serde_json::Value;
use tauri::AppHandle;

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
    let token = mint_token(canonical_id);
    let idempotency_key = (side_effect == SideEffect::Write)
        .then(|| default_idempotency_key(canonical_id, &arguments, &token));
    let prepared_at_ms = now_ms();
    PreparedCall {
        token,
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

/// Route a prepared call with optional AppHandle: `Confirm` is withheld and returned
/// as an interaction for the user; `Safe` executes now if AppHandle is present.
pub async fn run_or_confirm_opt(
    app: Option<&AppHandle>,
    prepared: PreparedCall,
    title: &str,
) -> Dispatch {
    if prepared.risk.needs_confirmation() {
        let interaction = confirm_interaction(&prepared, title);
        PendingCalls::insert(prepared);
        Dispatch::AwaitConfirm(interaction)
    } else if let Some(app) = app {
        Dispatch::Ran(execute_revalidated(app, &prepared).await)
    } else {
        Dispatch::Ran(failed(
            FailureClass::Internal,
            "App handle required for immediate execution of safe actions",
        ))
    }
}

/// Route a prepared call: `Safe` executes now; `Confirm` is withheld and returned
/// as an interaction for the user.
pub async fn run_or_confirm(app: &AppHandle, prepared: PreparedCall, title: &str) -> Dispatch {
    run_or_confirm_opt(Some(app), prepared, title).await
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
    let Some(current_digest) = current_manifest_digest(app, &prepared) else {
        return ActionOutcome::Failed {
            class: FailureClass::Cancelled,
            message:
                "The extension action is no longer approved or available â€” please ask again."
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
    if prepared.extension_id.starts_with("mcp.") {
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
    if let Some(provider_id) = prepared.extension_id.strip_prefix("mcp.") {
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
    let details: Vec<Field> = prepared
        .arguments
        .as_object()
        .map(|map| {
            map.iter()
                // Optimistic-concurrency state is host-owned. The user confirms
                // the human identity and exact content, not a digest.
                .filter(|(key, _)| key.as_str() != "expected_version")
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

    let summary = match prepared.action_id.as_str() {
        "save_note" => {
            let note_title = prepared
                .arguments
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("note");
            format!("Save note \"{note_title}\"")
        }
        "append_to_note" => {
            let note_title = prepared
                .arguments
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("note");
            format!("Add to \"{note_title}\"")
        }
        "rewrite_note" => {
            let note_title = prepared
                .arguments
                .get("target_title")
                .and_then(Value::as_str)
                .unwrap_or("note");
            format!("Replace \"{note_title}\" with the revised note")
        }
        _ => String::new(),
    };

    Interaction::Confirm {
        token: prepared.token.clone(),
        title: title.to_string(),
        summary,
        details,
        side_effect,
        destinations: Vec::new(),
    }
}

/// Convert an [`Interaction`] to an [`crate::agent::AgentConfirm`] for the agent confirmation panel.
pub(crate) fn to_agent_confirm(interaction: Interaction) -> crate::agent::AgentConfirm {
    let markdown = grain_core::interaction::to_markdown(&interaction);
    match interaction {
        Interaction::Confirm {
            token,
            title,
            summary,
            details,
            side_effect,
            destinations,
        } => crate::agent::AgentConfirm {
            token,
            title,
            summary,
            details: details
                .into_iter()
                .map(|field| crate::agent::AgentConfirmField {
                    label: field.label,
                    value: field.value,
                })
                .collect(),
            side_effect,
            destinations,
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

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn failed(class: FailureClass, message: &str) -> ActionOutcome {
    ActionOutcome::Failed {
        class,
        message: message.to_string(),
    }
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
    fn to_agent_confirm_preserves_interaction_data() {
        let interaction = Interaction::Confirm {
            token: "pc_test123".to_string(),
            title: "Save Note".to_string(),
            summary: "summary".to_string(),
            details: vec![Field {
                label: "title".to_string(),
                value: "Meeting Notes".to_string(),
            }],
            side_effect: "Makes a change".to_string(),
            destinations: vec!["Example provider".to_string()],
        };
        let confirm = to_agent_confirm(interaction);
        assert_eq!(confirm.token, "pc_test123");
        assert_eq!(confirm.title, "Save Note");
        assert_eq!(confirm.details.len(), 1);
        assert_eq!(confirm.details[0].label, "title");
        assert_eq!(confirm.details[0].value, "Meeting Notes");
        assert!(confirm.markdown.contains("Meeting Notes"));
    }
}
