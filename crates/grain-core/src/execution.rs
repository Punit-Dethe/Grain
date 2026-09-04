//! [GRAIN] Phase 3 execution contract (`docs/Extensions 2.0/PLAN.md` §8.3, §8.5,
//! §12).
//!
//! The pure, host-side-agnostic types the executor is built on: how risky an
//! action is, the exact prepared call the host will run, and the structured
//! outcome it produces. No `AppHandle`, no worker, no network — just the contract
//! and the decisions that must be identical everywhere and testable without a
//! running app.
//!
//! Design points confirmed against how production agents handle execution (Codex
//! CLI approval/sandbox split; verified-tool-call and idempotency literature):
//!
//! - **Approval is separate from reach.** [`RiskClass`] decides *when to ask*;
//!   Grain's capability system already bounds *what an extension can reach*. They
//!   compose, they are not the same axis.
//! - **The risk floor only tightens.** A host `Confirm` can never be downgraded to
//!   `Safe` by a manifest, a prompt, model output, or a user setting (§12.5). A
//!   user may make a `Safe` action require confirmation; never the reverse.
//! - **Approve the exact prepared call, and replay it — never re-ask the model.**
//!   Confirmation resumes [`PreparedCall`] by token; the model is not consulted
//!   again, so its nondeterminism cannot change what runs.
//! - **Idempotency + no blind retry.** A side-effecting call carries an
//!   idempotency key; an ambiguous timeout is [`ActionOutcome::UnknownOutcome`],
//!   never an automatic retry that could double-fire.
//! - **Time-of-check to time-of-use.** A prepared call is revalidated
//!   ([`PreparedCall::still_valid`]) immediately before execution; a changed
//!   manifest digest or an expired call forces a fresh confirmation (§12.7).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::interaction::{Field, Interaction, NoticeLevel};

/// How the host classifies an action for approval. Not confidence — a high match
/// score never retires `Confirm` (ASR can reverse intent: "cancel my order" heard
/// as "schedule my order" scores well and parses cleanly).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskClass {
    /// Cheap and obvious when wrong: skip a track, read a value, open an app.
    Safe,
    /// Sends, deletes, spends, posts. Always confirmed, however confident the
    /// router was.
    Confirm,
}

impl RiskClass {
    /// The host floor from the action's declared risk. This is the *minimum*; it
    /// can be tightened below, never loosened.
    pub fn floor(declared: grain_sdk::manifest::ActionRisk) -> Self {
        match declared {
            grain_sdk::manifest::ActionRisk::Safe => RiskClass::Safe,
            grain_sdk::manifest::ActionRisk::Confirm => RiskClass::Confirm,
        }
    }

    /// Apply a user policy that may only make things *stricter*. `user_requires_confirm`
    /// escalates a `Safe` floor to `Confirm`; nothing can pull a `Confirm` floor
    /// down to `Safe` (§12.5).
    pub fn tightened(self, user_requires_confirm: bool) -> Self {
        match self {
            RiskClass::Confirm => RiskClass::Confirm,
            RiskClass::Safe if user_requires_confirm => RiskClass::Confirm,
            RiskClass::Safe => RiskClass::Safe,
        }
    }

    pub fn needs_confirmation(self) -> bool {
        matches!(self, RiskClass::Confirm)
    }
}

/// What performing an action does to the world — the axis retry and parallelism
/// policy reads, distinct from [`RiskClass`] (a read can be `Confirm` for privacy;
/// a write can be `Safe` if trivially reversible).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SideEffect {
    /// Changes nothing observable: a computation, a formatting.
    None,
    /// Reads state without changing it.
    Read,
    /// Sends, writes, deletes, spends.
    Write,
}

/// The exact call the host will run, frozen at prepare time and approved as a
/// unit. Confirmation approves *this*, and the host re-runs *this* — not a fresh
/// model decision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedCall {
    /// Stable id shared with the [`Interaction::Confirm`] token, so the user's
    /// approval resumes exactly this call.
    pub token: String,
    pub canonical_id: String,
    pub extension_id: String,
    pub action_id: String,
    /// Host-owned display identity. Worker output cannot replace this and spoof
    /// another extension in composed results.
    pub provider_name: String,
    /// Normalised, schema-valid arguments. Validated in Rust before this exists.
    pub arguments: Value,
    pub risk: RiskClass,
    pub side_effect: SideEffect,
    /// The manifest digest in force at prepare time — rechecked at execution to
    /// catch an install/update that changed the action underneath us (§12.7/§12.13).
    pub manifest_digest: String,
    /// Present for side-effecting calls so a retry that arrives after a delayed
    /// success cannot double-fire; the executor dedupes on it within a window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    pub prepared_at_ms: i64,
    pub expires_at_ms: i64,
}

/// Why a prepared call can no longer run as-is — always forces a fresh
/// confirmation rather than executing something the user did not approve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stale {
    Expired,
    ManifestChanged,
}

impl PreparedCall {
    pub fn is_expired(&self, now_ms: i64) -> bool {
        now_ms >= self.expires_at_ms
    }

    /// Time-of-use revalidation: the manifest is unchanged and the call has not
    /// expired. A material change is not an error to swallow — it is a new
    /// confirmation (§8.3, §12.7).
    pub fn still_valid(&self, current_manifest_digest: &str, now_ms: i64) -> Result<(), Stale> {
        if self.is_expired(now_ms) {
            return Err(Stale::Expired);
        }
        if self.manifest_digest != current_manifest_digest {
            return Err(Stale::ManifestChanged);
        }
        Ok(())
    }

    /// A retry is safe only when re-running cannot double a side effect. The
    /// current host forwards write idempotency keys to providers for their own
    /// dedupe, but does not yet own a durable dedupe ledger, so writes remain
    /// non-retryable. After an ambiguous timeout the outcome is
    /// [`ActionOutcome::UnknownOutcome`], never a silent retry.
    pub fn is_retry_safe(&self) -> bool {
        self.side_effect != SideEffect::Write
    }
}

/// A stable, coarse failure class so the UI and logs never see a raw extension
/// exception or a secret (§8.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// The account/connection the action needs is not available.
    Auth,
    /// A network or upstream-service failure.
    Network,
    /// The arguments were rejected.
    InvalidArgument,
    /// The target does not exist.
    NotFound,
    /// The provider is rate-limited.
    RateLimited,
    /// The user (or the host) cancelled.
    Cancelled,
    /// Anything else — still a safe, generic message.
    Internal,
}

/// The bounded data a successful action returns for rendering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuccessData {
    /// Which extension produced this — kept so several results stay legible when
    /// composed ("this is GitHub, this is Teams").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<Field>,
    /// True when a side effect happened (render as a receipt); false for a read
    /// (render as a result).
    #[serde(default)]
    pub receipt: bool,
}

/// The structured result of one execution (§8.5). Raw exceptions and secrets
/// never reach here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ActionOutcome {
    Succeeded(SuccessData),
    Failed {
        class: FailureClass,
        message: String,
    },
    Cancelled,
    /// A side-effecting call whose result is genuinely unknown — e.g. an
    /// ambiguous timeout. The user is told plainly; the host does not retry.
    UnknownOutcome {
        message: String,
    },
    /// The action needs the user before it can proceed (a follow-up field, a
    /// choice). Carried through to the interaction surface.
    NeedsInteraction(Interaction),
}

impl ActionOutcome {
    /// Render this outcome as a user-facing [`Interaction`]. The one place an
    /// outcome becomes something shown — markdown now, native cards later, both
    /// over the same enum.
    pub fn to_interaction(&self, action_title: &str) -> Interaction {
        match self {
            ActionOutcome::Succeeded(data) if data.receipt => Interaction::Receipt {
                source: data.source.clone(),
                action: action_title.to_string(),
                summary: data
                    .body
                    .clone()
                    .or_else(|| data.title.clone())
                    .unwrap_or_default(),
                details: data.details.clone(),
            },
            ActionOutcome::Succeeded(data) => Interaction::Result {
                source: data.source.clone(),
                title: data.title.clone(),
                body: data.body.clone(),
                details: data.details.clone(),
            },
            ActionOutcome::Failed { message, .. } => Interaction::Notice {
                level: NoticeLevel::Error,
                message: message.clone(),
            },
            ActionOutcome::Cancelled => Interaction::Notice {
                level: NoticeLevel::Info,
                message: "Cancelled.".to_string(),
            },
            ActionOutcome::UnknownOutcome { message } => Interaction::Notice {
                level: NoticeLevel::Warning,
                message: message.clone(),
            },
            ActionOutcome::NeedsInteraction(interaction) => interaction.clone(),
        }
    }

    /// The short line fed back to the *model* as the tool result, so it can keep
    /// reasoning. Distinct from the user-facing interaction, and never a raw
    /// exception. For an unknown outcome it says so honestly, so the model does
    /// not claim success.
    pub fn model_summary(&self) -> String {
        let raw = match self {
            ActionOutcome::Succeeded(data) => data
                .body
                .clone()
                .or_else(|| data.title.clone())
                .unwrap_or_else(|| "Done.".to_string()),
            ActionOutcome::Failed { class, message } => format!("Failed ({class:?}): {message}"),
            ActionOutcome::Cancelled => "The user cancelled.".to_string(),
            ActionOutcome::UnknownOutcome { message } => {
                format!("Outcome unknown — do not claim it succeeded: {message}")
            }
            ActionOutcome::NeedsInteraction(_) => {
                "Waiting on the user before this can proceed.".to_string()
            }
        };
        // Tool results are untrusted evidence. Bound and strip invisible/control
        // text before they re-enter the model context.
        crate::capability_agent::sanitize(&raw, 4 * 1024)
    }
}

/// How the user answered a pending confirmation, in the interim chat surface
/// where the agent asks in prose and the user replies yes/no (Amendment A — no
/// approve/deny button). The host, not the model, reads this and resumes the
/// exact prepared call, so confirmation stays host-gated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirmation {
    Yes,
    No,
    /// Neither — most likely a new, unrelated request; the stale confirmation is
    /// dropped and the turn proceeds normally.
    Unclear,
}

/// Classify a user reply to a pending confirmation. Deliberately conservative:
/// only a clear affirmative runs a withheld action; anything ambiguous is
/// `Unclear` (never treated as approval).
pub fn classify_confirmation(said: &str) -> Confirmation {
    const YES: &[&str] = &[
        "yes",
        "yeah",
        "yep",
        "yup",
        "sure",
        "ok",
        "okay",
        "confirm",
        "confirmed",
        "proceed",
        "go ahead",
        "do it",
        "please do",
        "go for it",
        "yes please",
        "sounds good",
        "affirmative",
    ];
    const NO: &[&str] = &[
        "no",
        "nope",
        "nah",
        "cancel",
        "stop",
        "dont",
        "don't",
        "never mind",
        "nevermind",
        "negative",
        "no thanks",
        "skip it",
    ];
    let s = said.trim().to_lowercase();
    let starts = |phrase: &str| {
        s == phrase
            || s.strip_prefix(phrase)
                .is_some_and(|rest| rest.starts_with([' ', ',', '.', '!', ';']))
    };
    if YES.iter().any(|phrase| starts(phrase)) {
        Confirmation::Yes
    } else if NO.iter().any(|phrase| starts(phrase)) {
        Confirmation::No
    } else {
        Confirmation::Unclear
    }
}

/// A stable idempotency key for one prepared side-effecting operation.
///
/// Retries of the same prepared call share `operation_id`, while a later,
/// intentional call with identical arguments gets a distinct key. Only
/// meaningful for [`SideEffect::Write`].
pub fn default_idempotency_key(
    canonical_id: &str,
    arguments: &Value,
    operation_id: &str,
) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(canonical_id.as_bytes());
    hasher.update([0]);
    // `serde_json::Map` is deterministic without the preserve_order feature,
    // so the serialised arguments are a stable operation identity.
    hasher.update(arguments.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(operation_id.as_bytes());
    format!("idem_{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_sdk::manifest::ActionRisk;
    use serde_json::json;

    fn prepared(risk: RiskClass, side_effect: SideEffect) -> PreparedCall {
        PreparedCall {
            token: "pc_1".into(),
            canonical_id: "github.create_issue".into(),
            extension_id: "com.grain.github".into(),
            action_id: "create_issue".into(),
            provider_name: "GitHub".into(),
            arguments: json!({ "repo": "acme/web", "title": "bug" }),
            risk,
            side_effect,
            manifest_digest: "digest_v1".into(),
            idempotency_key: None,
            prepared_at_ms: 1_000,
            expires_at_ms: 61_000,
        }
    }

    #[test]
    fn the_risk_floor_only_tightens() {
        // A confirm floor cannot be loosened by any user policy.
        assert_eq!(
            RiskClass::floor(ActionRisk::Confirm).tightened(false),
            RiskClass::Confirm
        );
        // A safe floor can be escalated by the user, never the reverse.
        assert_eq!(
            RiskClass::floor(ActionRisk::Safe).tightened(true),
            RiskClass::Confirm
        );
        assert_eq!(
            RiskClass::floor(ActionRisk::Safe).tightened(false),
            RiskClass::Safe
        );
        assert!(RiskClass::Confirm.needs_confirmation());
        assert!(!RiskClass::Safe.needs_confirmation());
    }

    #[test]
    fn a_prepared_call_is_revalidated_at_time_of_use() {
        let call = prepared(RiskClass::Confirm, SideEffect::Write);
        assert_eq!(call.still_valid("digest_v1", 30_000), Ok(()));
        // The extension was updated between confirm and execute.
        assert_eq!(
            call.still_valid("digest_v2", 30_000),
            Err(Stale::ManifestChanged)
        );
        // The confirmation went stale.
        assert_eq!(call.still_valid("digest_v1", 61_000), Err(Stale::Expired));
        assert!(call.is_expired(61_000));
    }

    #[test]
    fn retry_safety_follows_side_effect_and_idempotency() {
        assert!(prepared(RiskClass::Safe, SideEffect::Read).is_retry_safe());
        assert!(prepared(RiskClass::Safe, SideEffect::None).is_retry_safe());
        // A bare write must not be retried after an ambiguous timeout.
        assert!(!prepared(RiskClass::Confirm, SideEffect::Write).is_retry_safe());
        // A key is forwarded to providers, but the host has no durable dedupe
        // ledger yet, so it must not retry a write on that basis.
        let mut safe_write = prepared(RiskClass::Confirm, SideEffect::Write);
        safe_write.idempotency_key = Some("idem_x".into());
        assert!(!safe_write.is_retry_safe());
    }

    #[test]
    fn a_write_success_renders_as_a_receipt_a_read_as_a_result() {
        let write = ActionOutcome::Succeeded(SuccessData {
            source: Some("GitHub".into()),
            title: Some("Created issue".into()),
            body: Some("acme/web #125".into()),
            details: vec![],
            receipt: true,
        });
        assert!(matches!(
            write.to_interaction("Create an issue"),
            Interaction::Receipt { .. }
        ));

        let read = ActionOutcome::Succeeded(SuccessData {
            source: Some("GitHub".into()),
            title: Some("Open issues".into()),
            body: None,
            details: vec![],
            receipt: false,
        });
        assert!(matches!(
            read.to_interaction("List issues"),
            Interaction::Result { .. }
        ));
    }

    #[test]
    fn an_unknown_outcome_never_reads_as_success_to_the_model() {
        let unknown = ActionOutcome::UnknownOutcome {
            message: "the request timed out after sending".into(),
        };
        assert!(unknown.model_summary().starts_with("Outcome unknown"));
        assert!(matches!(
            unknown.to_interaction("Send"),
            Interaction::Notice {
                level: NoticeLevel::Warning,
                ..
            }
        ));
    }

    #[test]
    fn a_failure_never_leaks_a_raw_exception_shape() {
        let failed = ActionOutcome::Failed {
            class: FailureClass::Auth,
            message: "Connect your GitHub account to do this.".into(),
        };
        assert!(matches!(
            failed.to_interaction("Create an issue"),
            Interaction::Notice {
                level: NoticeLevel::Error,
                ..
            }
        ));
        assert!(failed.model_summary().contains("Auth"));
    }

    #[test]
    fn confirmation_reads_a_clear_yes_or_no_and_nothing_else() {
        use Confirmation::*;
        assert_eq!(classify_confirmation("yes"), Yes);
        assert_eq!(classify_confirmation("Yes, go ahead"), Yes);
        assert_eq!(classify_confirmation("go ahead"), Yes);
        assert_eq!(classify_confirmation("  do it! "), Yes);
        assert_eq!(classify_confirmation("no"), No);
        assert_eq!(classify_confirmation("No thanks"), No);
        assert_eq!(classify_confirmation("cancel"), No);
        // Anything not a clear yes/no is never treated as approval.
        assert_eq!(classify_confirmation("what will it do?"), Unclear);
        assert_eq!(
            classify_confirmation("actually search my notes instead"),
            Unclear
        );
        assert_eq!(classify_confirmation("yesterday's meeting"), Unclear);
    }

    #[test]
    fn retries_share_a_key_but_separate_operations_do_not() {
        let args = json!({ "to": "jack", "body": "hi" });
        let a = default_idempotency_key("mail.send", &args, "operation-a");
        let b = default_idempotency_key("mail.send", &args, "operation-a");
        let c = default_idempotency_key("mail.send", &args, "operation-b");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn the_outcome_contract_round_trips_through_json() {
        let outcome = ActionOutcome::Failed {
            class: FailureClass::RateLimited,
            message: "try again shortly".into(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"status\":\"failed\""));
        let back: ActionOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(back, outcome);
    }
}
