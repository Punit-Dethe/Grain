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
//! Extension providers execute through MCP.

use std::sync::Mutex;

pub use grain_core::execution::ActionOutcome;
use grain_core::execution::{
    default_idempotency_key, FailureClass, PreparedCall, RiskClass, SideEffect, Stale, SuccessData,
};
use grain_core::interaction::{Field, Interaction};
use serde_json::Value;
use tauri::AppHandle;

/// How long a confirmation stays valid before the user must ask again (§8.3).
pub(crate) const CONFIRM_TTL_MS: i64 = 120_000;
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
        list.retain(|existing| existing.token != call.token && !existing.is_expired(now_ms()));
        if list.len() >= MAX_PENDING {
            list.remove(0);
        }
        list.push(call);
    }

    fn take(token: &str) -> Option<PreparedCall> {
        let mut guard = PENDING.lock().unwrap();
        let list = guard.as_mut()?;
        let index = list.iter().position(|call| call.token == token)?;
        let call = list.remove(index);
        if list.is_empty() {
            *guard = None;
        }
        Some(call)
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

/// Current Agent policy: enabling a tool extension authorizes its calls without
/// per-call dialogs. Keep risk/side-effect classification and every execution
/// revalidation; this policy does not make writes into reads or bypass consent.
/// The Agent runs whole frames through [`run_agent_calls`]; this single-call
/// form remains for the executor contract tests.
#[cfg(test)]
pub(crate) async fn run_agent_call(
    app: Option<&AppHandle>,
    prepared: PreparedCall,
) -> ActionOutcome {
    log::info!(
        "[GRAIN] agent-tool-trace {}",
        serde_json::json!({
            "phase": "auto-approved", "tool": prepared.canonical_id,
            "risk": prepared.risk, "side_effect": prepared.side_effect,
        })
    );
    match app {
        Some(app) => execute_revalidated(app, &prepared).await,
        None => failed(
            FailureClass::Internal,
            "App handle required for Agent execution",
        ),
    }
}

/// Run every action requested in one model message under the same policy as
/// [`run_agent_call`]. Each call is revalidated on its own. MCP calls to one
/// provider share a connection and revalidation and are sent concurrently;
/// different providers run concurrently with each other. Outcomes are returned
/// in input order.
pub(crate) async fn run_agent_calls(
    app: &AppHandle,
    prepared: Vec<PreparedCall>,
) -> Vec<ActionOutcome> {
    let mut outcomes: Vec<Option<ActionOutcome>> = prepared.iter().map(|_| None).collect();
    let mut mcp: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
    for (index, call) in prepared.iter().enumerate() {
        log::info!(
            "[GRAIN] agent-tool-trace {}",
            serde_json::json!({
                "phase": "auto-approved", "tool": call.canonical_id,
                "risk": call.risk, "side_effect": call.side_effect,
            })
        );
        let Some(provider_id) = call.extension_id.strip_prefix("mcp.") else {
            outcomes[index] = Some(failed(
                FailureClass::NotFound,
                "Only MCP connections provide Agent tools.",
            ));
            continue;
        };
        if crate::grain_mcp::is_enabled_extension(app, &call.extension_id)
            && call.still_valid(&call.manifest_digest, now_ms()).is_ok()
        {
            mcp.entry(provider_id.to_string()).or_default().push(index);
        } else {
            outcomes[index] = Some(failed(
                FailureClass::Cancelled,
                "The extension action changed or expired before it could run.",
            ));
        }
    }
    let prepared = &prepared;
    let mcp_groups = mcp.into_iter().map(|(provider_id, indices)| async move {
        let batch: Vec<_> = indices
            .iter()
            .map(|&index| crate::grain_mcp::McpBatchCall {
                tool_name: prepared[index].action_id.clone(),
                arguments: prepared[index].arguments.clone(),
                expected_digest: prepared[index].manifest_digest.clone(),
            })
            .collect();
        let results = crate::grain_mcp::call_tools(app, &provider_id, &batch).await;
        indices
            .into_iter()
            .zip(results)
            .map(|(index, result)| (index, mcp_outcome(result, &prepared[index])))
            .collect::<Vec<_>>()
    });
    for (index, outcome) in futures_util::future::join_all(mcp_groups)
        .await
        .into_iter()
        .flatten()
    {
        outcomes[index] = Some(outcome);
    }
    outcomes
        .into_iter()
        .map(|outcome| {
            outcome.unwrap_or_else(|| failed(FailureClass::Internal, "The action did not run."))
        })
        .collect()
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
            message: "The extension action is no longer approved or available. Please ask again."
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

/// Check that the MCP connection is still enabled. The transport verifies the
/// loaded tool digest again before dispatch on the same short-lived service.
fn current_manifest_digest(app: &AppHandle, prepared: &PreparedCall) -> Option<String> {
    crate::grain_mcp::is_enabled_extension(app, &prepared.extension_id)
        .then(|| prepared.manifest_digest.clone())
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
        failed(
            FailureClass::NotFound,
            "Only MCP connections provide Agent tools.",
        )
    }
}

async fn mcp_execute(app: &AppHandle, provider_id: &str, prepared: &PreparedCall) -> ActionOutcome {
    mcp_outcome(
        crate::grain_mcp::call_tool(
            app,
            provider_id,
            &prepared.action_id,
            &prepared.arguments,
            &prepared.manifest_digest,
        )
        .await,
        prepared,
    )
}

/// Shared by the production executor and protocol regression tests. Outcome
/// certainty comes from the dispatch boundary, never from an exception string.
pub(crate) fn mcp_outcome(
    result: Result<crate::grain_mcp::McpCallOutput, grain_core::execution::ExecutionFailure>,
    prepared: &PreparedCall,
) -> ActionOutcome {
    match result {
        Ok(output) if output.is_error => ActionOutcome::ToolReportedError {
            message: format!("The MCP tool reported an error. Partial effects may have occurred; do not repeat automatically.\n{}", output.text),
        },
        Ok(output) => {
            let mut details = Vec::new();
            if output.truncated {
                details.push(Field { label: "Result".into(), value: "Some result data was omitted because of the size limit.".into() });
            }
            if !output.unsupported_content.is_empty() {
                details.push(Field { label: "Omitted content".into(), value: output.unsupported_content.join(", ") });
            }
            ActionOutcome::Succeeded(SuccessData {
                source: Some(prepared.provider_name.clone()),
                title: Some(prepared.action_id.clone()),
                body: Some(output.text),
                structured_content: output.structured_content,
                details,
                receipt: prepared.side_effect == SideEffect::Write,
            })
        },
        Err(error) => error.into_outcome(),
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

    fn mcp_call() -> PreparedCall {
        prepare(
            "mcp.github:create_issue",
            "mcp.github",
            "create_issue",
            "GitHub",
            json!({ "title": "Bug" }),
            RiskClass::Confirm,
            SideEffect::Write,
            "approved-digest",
        )
    }

    #[tokio::test]
    async fn agent_policy_never_stores_a_per_call_confirmation() {
        let prepared = mcp_call();
        let token = prepared.token.clone();
        assert!(prepared.risk.needs_confirmation());
        assert_eq!(prepared.side_effect, SideEffect::Write);
        let outcome = run_agent_call(None, prepared).await;
        // No host means no execution, even though policy permits the call.
        assert!(matches!(
            outcome,
            ActionOutcome::Failed {
                class: FailureClass::Internal,
                ..
            }
        ));
        assert!(!discard(&token));
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
