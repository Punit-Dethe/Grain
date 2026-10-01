//! [GRAIN] The extension registry (SPEC §4.2, §5.1, §10.1) — Phase 1.
//!
//! Persists which extensions are installed, enabled, and in what **toggle
//! order** (SPEC §4.4: the order the user *enabled* them in — first enabled
//! sits top; re-enabling moves to the end). Stored as owned JSON
//! (`extensions.json`) in the same data dir as settings but **physically
//! separate from `AppSettings`**, so core settings migrations never touch
//! extension state and vice versa.
//!
//! Two kinds of entry:
//! - **Built-ins** (Snippets, Context Awareness, Agent) are *not stored here*.
//!   They are descriptors in code, and their enabled state delegates to the
//!   core settings flags (`snippets_enabled`, `context_awareness_enabled`,
//!   `agent_enabled`) — manifest-first per PLAN.md D4: the registry and UI are
//!   new; the implementation stays where it is. Their toggle order is tracked
//!   here by id (a toggle bumps the sequence without creating a record's
//!   install data).
//! - **Installed packs** imported as `.grainpack` files are full records.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use anyhow::{Context, Result};
use grain_sdk::Trust;
use serde::{Deserialize, Serialize};

pub const EXTENSIONS_FILE: &str = "extensions.json";
const REGISTRY_MAX_BYTES: usize = 8 * 1024 * 1024;

// Prepared calls and workers live only in this process. A checked process-wide
// sequence also prevents a registry reload from recreating an old live identity.
static NEXT_EXECUTION_GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_execution_generation() -> Result<u64> {
    NEXT_EXECUTION_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .map_err(|_| anyhow::anyhow!("extension execution generation exhausted"))
}

/// Retired visual-only pack. Kept only long enough to remove stale local
/// registry state; both Agent layouts now belong to native settings.
const RETIRED_AGENT_CENTER_VARIANT_ID: &str = "grain.agent-center-layout";

/// Reserved occupant id standing for Grain's own built-in behaviour in a slot.
/// SPEC §3.2: "core defaults are occupants" — so a slot is never *free*, and the
/// first extension to claim one still faces an explicit takeover prompt rather
/// than silently displacing shipped behaviour.
pub const CORE_DEFAULT: &str = "grain.core";

/// Ids reserved for Grain's own always-present features. They are not installed
/// and cannot be uninstalled — each has a tab in the Extensions hub whose first
/// row is its master switch — but the ids stay reserved so a third-party pack
/// can never claim one.
pub const BUILTIN_SNIPPETS: &str = "grain.snippets";
pub const BUILTIN_CONTEXT: &str = "grain.context-awareness";
pub const BUILTIN_AGENT: &str = "grain.agent";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionRecord {
    pub id: String,
    pub enabled: bool,
    /// Host-owned identity for live calls/workers, independent of toggle order.
    /// Never read from disk or caller input; assigned at registry boundaries.
    #[serde(skip)]
    pub execution_generation: u64,
    /// Position in toggle order (SPEC §4.4): set from `next_toggle_seq` every
    /// time the extension is enabled, so re-enabling moves it to the end.
    #[serde(default)]
    pub toggle_seq: u64,
    #[serde(default)]
    pub installed_version: String,
    /// SHA-256 of the exact installed artifact. Manual imports and signed-store
    /// installs carry `Some` and are rechecked on every load; unpacked developer
    /// projects carry `None` because their source is intentionally live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_sha256: Option<String>,
    /// Granted capability names (empty for A-inert packs, which need none).
    #[serde(default)]
    pub granted: Vec<String>,
    /// Fingerprint of the prompt layers the user approved, from
    /// [`prompt_layers_fingerprint`].
    ///
    /// **A prompt layer's text is part of the permission surface.** It changes
    /// what the model does to the user's own words, and unlike a capability it
    /// is granted implicitly by installing — so without this, an approved pack
    /// could change its wording in a routine update and nothing would ask again.
    /// That is CVE-2025-54136's exact shape ("approval did not survive later
    /// changes"), and the VS Code marketplace's 2025 incidents were mostly
    /// ordinary version bumps.
    ///
    /// `None` means "never approved" — which is what a registry written before
    /// this field reads back as, so old records re-prompt rather than
    /// grandfathering text nobody ever saw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_layers_approved: Option<String>,
    /// Fingerprint of the actions the user approved, from
    /// [`actions_fingerprint`].
    ///
    /// Same argument as `prompt_layers_approved`, with a sharper edge: an action
    /// has a **side effect**, so the thing an update can quietly change is not
    /// wording but what happens. The two failure shapes this closes are a
    /// `confirm → safe` downgrade (the read-back disappears) and a widened
    /// `when` (the action starts being offered where it was not).
    ///
    /// Kept separate from `prompt_layers_approved` so changing a sentence does
    /// not re-ask about what an extension can *do*, and vice versa. Both are
    /// carried in one approval sheet — two sheets in a row is how a user learns
    /// to click through.
    ///
    /// `None` means "never approved", so a registry written before this field
    /// re-prompts rather than grandfathering actions nobody ever saw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions_approved: Option<String>,
    /// Fingerprint of the complete host-owned authentication declaration.
    /// An endpoint, scope, API host, or client-id change therefore disables an
    /// update until the user approves the new connection contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication_approved: Option<String>,
    /// Host-owned pointer to one native OAuth grant in the OS vault. No tokens
    /// or provider account names are stored here. None means no active grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication_session: Option<String>,
    /// [GRAIN] Fingerprint of the Extension Mode hand-off contract the user
    /// approved, from [`recommendation_fingerprint`].
    ///
    /// The sharpest of the three, because what it gates is **disclosure**: being
    /// picked in Extension Mode means receiving the full transcript of what the
    /// user just said, verbatim. `recommend` is the declaration that decides
    /// when that happens, so widening it in an update widens what an already
    /// approved extension hears — the same shape as a widened `when`, one level
    /// up (`docs/Extensions V1/PLAN.md` §G1, §G3).
    ///
    /// `None` means "never approved", so an extension that has not been reviewed
    /// under this contract simply is not ranked. Registries written before this
    /// field read back that way, which is the correct default: nobody was ever
    /// shown a hand-off sheet, so nobody agreed to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommend_approved: Option<String>,
    /// Exclusive positions this pack's manifest *declares* (SPEC §3.2). Copied
    /// from the manifest at install so occupancy is answerable from memory —
    /// no pack file is ever read to decide who owns a slot.
    ///
    /// These are **claimed on enable**: turning the pack on takes the position.
    #[serde(default)]
    pub slots: Vec<String>,
    /// A load-unpacked project currently overriding this id. The effective
    /// record stays at the normal map key, so every capability/slot/lifecycle
    /// path sees exactly one extension. Any installed version is parked here
    /// with its settings preserved; unload assigns a fresh execution identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev: Option<DevOverride>,
    /// [GRAIN] Phase 5A trust rung (SPEC §7.4, DISTRIBUTION-PLAN §3.2). **The
    /// only path that may set this above [`Trust::UNTRUSTED_DEFAULT`] is
    /// [`crate::install::install_from_verified_entry`]**, which reads it from a
    /// signature-verified index entry bound to `(id, version, sha256)`. Every
    /// other construction site — manual import, dev load, healing — leaves it at
    /// the untrusted default. `#[serde(default)]` means a registry written
    /// before 5A reads back as untrusted, never accidentally verified.
    #[serde(default = "default_trust")]
    pub trust: Trust,
}

/// Fingerprint the declared prompt layers, for the approval check on
/// [`ExtensionRecord::prompt_layers_approved`].
///
/// SHA-256 rather than a cheap hash, because the input is attacker-controlled:
/// the whole point is to detect a *deliberate* change, so a function whose
/// collisions can be constructed would let an update carry new wording under the
/// old approval.
///
/// Covers every field that reaches the model or decides when it does — id, text
/// and the whole match — with length-prefixed framing so two layers cannot be
/// re-cut into one another and hash the same.
pub fn prompt_layers_fingerprint(layers: &[grain_sdk::manifest::PromptLayerDecl]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let field = |h: &mut Sha256, s: &str| {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    };
    hasher.update((layers.len() as u64).to_le_bytes());
    for layer in layers {
        field(&mut hasher, layer.id.trim());
        field(
            &mut hasher,
            match layer.target {
                grain_sdk::manifest::PromptTarget::Additive => "additive",
                grain_sdk::manifest::PromptTarget::Main => "main",
                grain_sdk::manifest::PromptTarget::Context => "context",
            },
        );
        field(&mut hasher, layer.text.trim());
        // Each list is framed by its own length and closed with a separator, so
        // moving a value from `app` to `website` — which changes which surfaces
        // the layer fires on — changes the digest.
        for list in [&layer.when.app, &layer.when.website, &layer.when.category] {
            hasher.update((list.len() as u64).to_le_bytes());
            for value in list {
                field(&mut hasher, value);
            }
            hasher.update([0xfeu8]);
        }
        field(&mut hasher, layer.when.field.as_deref().unwrap_or(""));
        hasher.update([0xffu8]);
    }
    format!("{:x}", hasher.finalize())
}

/// Fingerprint the Extension Mode hand-off contract, for the approval check on
/// [`ExtensionRecord::recommend_approved`].
///
/// Same length-prefixed construction as the two above, and the same reason for
/// SHA-256: the input is attacker-controlled and the whole point is to detect a
/// deliberate change.
///
/// Covers everything that decides **whether this extension can be handed the
/// user's words, and whether it can be handed them without asking**: the kind,
/// the full `recommend` block, the Auto-send declaration, and `needs`. The
/// tempting economy is to hash only `examples`; then an author flips
/// `autoSend.eligible` in a patch release and the hand-off stops asking.
pub fn recommendation_fingerprint(manifest: &grain_sdk::manifest::ExtensionManifest) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let field = |h: &mut Sha256, s: &str| {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    };
    field(&mut hasher, manifest.kind.as_str());
    match &manifest.recommend {
        None => hasher.update([0x00u8]),
        Some(r) => {
            hasher.update([0x01u8]);
            field(&mut hasher, r.purpose.trim());
            // Each list framed by its own length and closed with a separator, so
            // a phrase moved from `entities` to `examples` — which changes what
            // the extension is offered for — changes the digest.
            for list in [&r.examples, &r.aliases, &r.entities] {
                hasher.update((list.len() as u64).to_le_bytes());
                for value in list {
                    field(&mut hasher, value.trim());
                }
                hasher.update([0xfeu8]);
            }
        }
    }
    match &manifest.auto_send {
        None => hasher.update([0x00u8]),
        Some(a) => {
            hasher.update([if a.eligible { 0x02u8 } else { 0x01u8 }]);
            field(&mut hasher, a.note.as_deref().unwrap_or("").trim());
        }
    }
    hasher.update((manifest.needs.len() as u64).to_le_bytes());
    for need in &manifest.needs {
        field(&mut hasher, need.trim());
    }
    format!("{:x}", hasher.finalize())
}

/// Fingerprint the extension's single authentication declaration. Its BTreeMap
/// provider parameters make the JSON encoding deterministic across platforms.
pub fn authentication_fingerprint(declaration: &grain_sdk::manifest::AuthenticationDecl) -> String {
    use sha2::{Digest, Sha256};
    let encoded = serde_json::to_vec(declaration)
        .expect("authentication declaration is always JSON serializable");
    format!("{:x}", Sha256::digest(encoded))
}

#[cfg(test)]
mod authentication_fingerprint_tests {
    use super::*;

    fn declaration() -> grain_sdk::AuthenticationDecl {
        grain_sdk::AuthenticationDecl {
            auth_type: grain_sdk::AuthenticationType::OAuth2Pkce,
            provider_name: "GitHub".into(),
            client_id: "public".into(),
            authorization_endpoint: "https://github.com/login/oauth/authorize".into(),
            token_endpoint: "https://github.com/login/oauth/access_token".into(),
            scopes: vec!["read:user".into()],
            redirect_methods: vec![grain_sdk::RedirectMethod::Loopback],
            api_hosts: vec!["api.github.com".into()],
            authorization_parameters: Default::default(),
        }
    }

    #[test]
    fn scope_or_host_changes_require_new_authentication_approval() {
        let original = declaration();
        let fingerprint = authentication_fingerprint(&original);
        let mut changed = original.clone();
        changed.scopes.push("repo".into());
        assert_ne!(fingerprint, authentication_fingerprint(&changed));
        let mut changed = original;
        changed.api_hosts = vec!["uploads.github.com".into()];
        assert_ne!(fingerprint, authentication_fingerprint(&changed));
    }
}

/// Identity of an exact native call, distinct from persistent declaration approval.
/// Stream the manifest/source into the hash instead of retaining another source copy.
pub fn native_call_fingerprint(
    record: &ExtensionRecord,
    manifest: &grain_sdk::ExtensionManifest,
) -> Result<String, serde_json::Error> {
    use sha2::{Digest, Sha256};
    struct HashWriter(Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(
        &mut writer,
        &(
            "grain.native.call.v3",
            manifest,
            record.execution_generation,
            record.toggle_seq,
            &record.granted,
            &record.installed_version,
            &record.artifact_sha256,
            &record.authentication_approved,
            &record.authentication_session,
        ),
    )?;
    Ok(format!("{:x}", writer.0.finalize()))
}

/// Consent refers to the complete reviewed package and this runtime registry
/// identity. No pending-approval cache or secret is retained by the host.
pub fn approval_fingerprint(
    record: &ExtensionRecord,
    pack: &grain_sdk::GrainPack,
) -> Result<String, serde_json::Error> {
    use sha2::{Digest, Sha256};
    struct HashWriter(Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(
        &mut writer,
        &(
            "grain.extension.review.v1",
            record.execution_generation,
            record,
            pack,
        ),
    )?;
    Ok(format!("{:x}", writer.0.finalize()))
}

/// Fingerprint the declared actions, for the approval check on
/// [`ExtensionRecord::actions_approved`].
///
/// Same construction as [`prompt_layers_fingerprint`] and for the same reason —
/// the input is attacker-controlled, so the hash has to resist a *constructed*
/// collision, not just a careless one.
///
/// Covers everything that decides **what happens and when**: id, title, risk,
/// the whole `when`, every utterance, every parameter, and `agentRules`.
/// Deliberately NOT a subset — the tempting economy is to hash only the risk
/// tier, and then a widened `when` or a new utterance ships under the old
/// approval.
pub fn actions_fingerprint(actions: &[grain_sdk::manifest::ActionDecl]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let field = |h: &mut Sha256, s: &str| {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    };
    hasher.update((actions.len() as u64).to_le_bytes());
    for action in actions {
        field(&mut hasher, action.id.trim());
        field(&mut hasher, action.title.trim());
        // The single most important byte in here: `safe` means no read-back.
        field(
            &mut hasher,
            if action.risk.is_safe() {
                "safe"
            } else {
                "confirm"
            },
        );
        // Each list framed by its own length and closed with a separator, so a
        // value moved between fields — which changes WHERE the action is
        // offered — changes the digest.
        for list in [
            &action.when.app,
            &action.when.website,
            &action.when.category,
        ] {
            hasher.update((list.len() as u64).to_le_bytes());
            for value in list {
                field(&mut hasher, value);
            }
            hasher.update([0xfeu8]);
        }
        field(&mut hasher, action.when.field.as_deref().unwrap_or(""));
        hasher.update((action.utterances.len() as u64).to_le_bytes());
        for utterance in &action.utterances {
            field(&mut hasher, utterance.trim());
        }
        hasher.update([0xfeu8]);
        hasher.update((action.params.len() as u64).to_le_bytes());
        for param in &action.params {
            field(&mut hasher, param.name.trim());
            field(&mut hasher, &format!("{:?}", param.kind));
            hasher.update([u8::from(param.resolve), u8::from(param.required)]);
        }
        hasher.update([0xfeu8]);
        field(&mut hasher, action.agent_rules.as_deref().unwrap_or(""));
        hasher.update([0xffu8]);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod action_fingerprint_tests {
    use grain_sdk::manifest::{
        ActionDecl, ActionParamDecl, ActionParamKind, ActionRisk, LayerWhen,
    };

    fn action(risk: ActionRisk, when: LayerWhen, utterances: &[&str]) -> ActionDecl {
        ActionDecl {
            id: "next".into(),
            title: "Skip to the next track".into(),
            risk,
            when,
            utterances: utterances.iter().map(|s| (*s).to_string()).collect(),
            params: Vec::new(),
            agent_rules: None,
        }
    }

    fn site(host: &str) -> LayerWhen {
        LayerWhen {
            website: vec![host.into()],
            ..Default::default()
        }
    }

    #[test]
    fn dropping_the_read_back_requires_renewed_approval() {
        // The attack this exists for: ship `confirm`, get approved, then quietly
        // become `safe` in a patch release so the action fires with no prompt.
        let approved = super::actions_fingerprint(&[action(
            ActionRisk::Confirm,
            LayerWhen::default(),
            &["send it"],
        )]);
        let downgraded = super::actions_fingerprint(&[action(
            ActionRisk::Safe,
            LayerWhen::default(),
            &["send it"],
        )]);
        assert_ne!(approved, downgraded);
    }

    #[test]
    fn widening_where_an_action_is_offered_requires_renewed_approval() {
        // Identical wording, identical risk — only the scope grew, from one site
        // to every dictation surface. That is an escalation.
        let scoped =
            super::actions_fingerprint(&[action(ActionRisk::Safe, site("jira."), &["next song"])]);
        let everywhere = super::actions_fingerprint(&[action(
            ActionRisk::Safe,
            LayerWhen::default(),
            &["next song"],
        )]);
        assert_ne!(scoped, everywhere);
    }

    #[test]
    fn teaching_an_action_new_language_requires_renewed_approval() {
        // A new utterance changes what the action captures out of everything the
        // user says, which is the routing equivalent of widening `when`.
        let taught = super::actions_fingerprint(&[action(
            ActionRisk::Safe,
            LayerWhen::default(),
            &["next song", "do it"],
        )]);
        let original = super::actions_fingerprint(&[action(
            ActionRisk::Safe,
            LayerWhen::default(),
            &["next song"],
        )]);
        assert_ne!(taught, original);
    }

    #[test]
    fn unresolving_a_parameter_requires_renewed_approval() {
        // `resolve: true` is what keeps a span bounded by the extension's own
        // catalogue. Flipping it off sends raw ASR output wherever the parameter
        // goes, with no other visible change.
        let bounded = ActionParamDecl {
            name: "artist".into(),
            kind: ActionParamKind::Entity,
            resolve: true,
            required: true,
        };
        let raw = ActionParamDecl {
            resolve: false,
            ..bounded.clone()
        };
        let with = |param: ActionParamDecl| {
            let mut a = action(ActionRisk::Safe, LayerWhen::default(), &["play {artist}"]);
            a.params = vec![param];
            super::actions_fingerprint(&[a])
        };
        assert_ne!(with(bounded), with(raw));
    }

    #[test]
    fn an_unchanged_declaration_keeps_its_approval() {
        let once =
            super::actions_fingerprint(&[action(ActionRisk::Safe, site("jira."), &["next song"])]);
        let twice =
            super::actions_fingerprint(&[action(ActionRisk::Safe, site("jira."), &["next song"])]);
        assert_eq!(once, twice);
    }
}

#[cfg(test)]
mod prompt_layer_fingerprint_tests {
    use grain_sdk::manifest::{LayerWhen, PromptLayerDecl};

    fn layer(id: &str, text: &str, when: LayerWhen) -> PromptLayerDecl {
        PromptLayerDecl {
            id: id.into(),
            target: Default::default(),
            when,
            text: text.into(),
        }
    }

    #[test]
    fn the_fingerprint_tracks_every_field_that_reaches_the_model() {
        let base = vec![layer("a", "Write tersely.", LayerWhen::default())];
        let fp = super::prompt_layers_fingerprint(&base);

        // Same declaration, same digest — an update that changes nothing must
        // not nag the user.
        assert_eq!(fp, super::prompt_layers_fingerprint(&base));

        for changed in [
            vec![layer("a", "Write at length.", LayerWhen::default())],
            vec![layer("b", "Write tersely.", LayerWhen::default())],
            vec![layer(
                "a",
                "Write tersely.",
                LayerWhen {
                    app: vec!["code".into()],
                    ..Default::default()
                },
            )],
            vec![
                layer("a", "Write tersely.", LayerWhen::default()),
                layer("b", "And politely.", LayerWhen::default()),
            ],
        ] {
            assert_ne!(fp, super::prompt_layers_fingerprint(&changed));
        }
    }

    /// The escalation that changes no text at all.
    ///
    /// `when: {website: ["jira."]}` → `when: {}` leaves every instruction
    /// byte-identical while taking a layer from one site to EVERY dictation the
    /// user makes. A digest over the text alone would wave that through, which
    /// is why the whole effective declaration is covered: what it says AND
    /// where it may say it.
    #[test]
    fn widening_where_a_layer_applies_requires_renewed_approval() {
        let text = "Write in imperative mood.";
        let scoped = vec![layer(
            "a",
            text,
            LayerWhen {
                website: vec!["jira.".into()],
                ..Default::default()
            },
        )];
        let everywhere = vec![layer("a", text, LayerWhen::default())];
        assert_eq!(scoped[0].text, everywhere[0].text, "the text is unchanged");
        assert_ne!(
            super::prompt_layers_fingerprint(&scoped),
            super::prompt_layers_fingerprint(&everywhere),
            "widening the scope must not pass as the approved declaration"
        );
    }

    /// Narrowing is a change too. It is harmless, but the digest is a statement
    /// about the declaration, not a risk assessment — a rule that only fires
    /// sometimes is not the rule the user read.
    #[test]
    fn narrowing_where_a_layer_applies_also_changes_the_fingerprint() {
        let broad = vec![layer("a", "Be terse.", LayerWhen::default())];
        let narrow = vec![layer(
            "a",
            "Be terse.",
            LayerWhen {
                field: Some("single_line".into()),
                ..Default::default()
            },
        )];
        assert_ne!(
            super::prompt_layers_fingerprint(&broad),
            super::prompt_layers_fingerprint(&narrow)
        );
    }

    #[test]
    fn moving_a_target_between_lists_changes_the_fingerprint() {
        // `app: ["acme.com"]` and `website: ["acme.com"]` fire on different
        // surfaces, so they must not share a digest — the length-prefixed
        // per-list framing is what makes that true.
        let as_app = vec![layer(
            "a",
            "Write tersely.",
            LayerWhen {
                app: vec!["acme.com".into()],
                ..Default::default()
            },
        )];
        let as_site = vec![layer(
            "a",
            "Write tersely.",
            LayerWhen {
                website: vec!["acme.com".into()],
                ..Default::default()
            },
        )];
        assert_ne!(
            super::prompt_layers_fingerprint(&as_app),
            super::prompt_layers_fingerprint(&as_site)
        );
    }

    #[test]
    fn turning_an_additive_rule_into_a_replacement_requires_approval() {
        let additive = vec![layer("a", "Write tersely.", LayerWhen::default())];
        let mut replacement = additive.clone();
        replacement[0].target = grain_sdk::manifest::PromptTarget::Main;
        assert_ne!(
            super::prompt_layers_fingerprint(&additive),
            super::prompt_layers_fingerprint(&replacement)
        );
    }
}

/// A record's trust when it did not come from a verified index entry. This is
/// the serde default too, so pre-5A registries and any locally-sourced pack are
/// untrusted by construction — never `verified`/`core`.
fn default_trust() -> Trust {
    Trust::UNTRUSTED_DEFAULT
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DevOverride {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced: Option<Box<ExtensionRecord>>,
}

/// Why an enable was refused: the slot and who holds it (`grain.core` for a
/// built-in default). Mirrors the `needsPermissions` shape the permission sheet
/// already uses, so the frontend flow is the familiar one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotConflict {
    pub slot: String,
    pub current_occupant: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    /// Runtime-only removal epoch. Vacant-id writes conservatively fail after
    /// any removal, without retaining a tombstone for every historical id.
    #[serde(skip)]
    removal_epoch: u64,
    #[serde(default)]
    tool_only_migration_version: u32,
    /// Persistent refusal reasons. A restart or old grant must not reactivate
    /// an incompatible package; a validated replacement clears quarantine.
    #[serde(default)]
    quarantined: HashMap<String, String>,
    /// Installed pack records, keyed by extension id.
    #[serde(default)]
    records: HashMap<String, ExtensionRecord>,
    /// Toggle sequence for BUILT-INS (id → seq); their enabled state lives in
    /// settings, but their position in toggle order is registry business.
    #[serde(default)]
    builtin_toggle_seq: HashMap<String, u64>,
    #[serde(default)]
    next_toggle_seq: u64,
    /// Slot → current occupant id (SPEC §3.2). Every known slot is present
    /// after `load`, holding `CORE_DEFAULT` until an extension takes it.
    #[serde(default)]
    slot_claims: HashMap<String, String>,
}

pub struct ExtensionsRegistry {
    path: PathBuf,
    state: RwLock<RegistryFile>,
    save_gate: std::sync::Mutex<()>,
}

/// Only the edited identity and affected claims are copied for rollback; other
/// records and artifact contents are never duplicated. Readers are excluded
/// until persistence succeeds or this snapshot has been restored.
struct RecordEditSnapshot {
    record: Option<ExtensionRecord>,
    quarantine: Option<String>,
    slots: Vec<(String, Option<String>)>,
    next_toggle_seq: u64,
    removal_epoch: u64,
}

impl RecordEditSnapshot {
    fn capture(state: &RegistryFile, id: &str, additional_slots: &[String]) -> Self {
        let mut slots: Vec<_> = state
            .slot_claims
            .iter()
            .filter(|(slot, occupant)| occupant.as_str() == id || additional_slots.contains(slot))
            .map(|(slot, occupant)| (slot.clone(), Some(occupant.clone())))
            .collect();
        for slot in additional_slots {
            if !slots.iter().any(|(captured, _)| captured == slot) {
                slots.push((slot.clone(), None));
            }
        }
        Self {
            record: state.records.get(id).cloned(),
            quarantine: state.quarantined.get(id).cloned(),
            slots,
            next_toggle_seq: state.next_toggle_seq,
            removal_epoch: state.removal_epoch,
        }
    }

    fn restore(self, state: &mut RegistryFile, id: &str) {
        match self.record {
            Some(record) => {
                state.records.insert(id.to_owned(), record);
            }
            None => {
                state.records.remove(id);
            }
        }
        match self.quarantine {
            Some(reason) => {
                state.quarantined.insert(id.to_owned(), reason);
            }
            None => {
                state.quarantined.remove(id);
            }
        }
        for (slot, occupant) in self.slots {
            match occupant {
                Some(occupant) => {
                    state.slot_claims.insert(slot, occupant);
                }
                None => {
                    state.slot_claims.remove(&slot);
                }
            }
        }
        state.next_toggle_seq = self.next_toggle_seq;
        state.removal_epoch = self.removal_epoch;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordRevision {
    active: Option<u64>,
    parked: Option<u64>,
    vacant_epoch: Option<u64>,
}

impl RecordRevision {
    fn capture(state: &RegistryFile, id: &str) -> Self {
        let record = state.records.get(id);
        Self {
            active: record.map(|record| record.execution_generation),
            parked: record
                .and_then(|record| record.dev.as_ref())
                .and_then(|dev| dev.replaced.as_ref())
                .map(|record| record.execution_generation),
            vacant_epoch: (record.is_none()
                || record.is_some_and(|record| {
                    record
                        .dev
                        .as_ref()
                        .is_some_and(|dev| dev.replaced.is_none())
                }))
            .then_some(state.removal_epoch),
        }
    }

    fn check(self, state: &RegistryFile, id: &str) -> Result<()> {
        if self != Self::capture(state, id) {
            anyhow::bail!(
                "Extension changed while this operation was pending; start a fresh request."
            );
        }
        Ok(())
    }
}

impl ExtensionsRegistry {
    /// Checkpoint only after archival and settings persistence succeed. Validation
    /// still runs on every startup; this version never bypasses the live profile.
    pub fn finish_tool_only_migration(&self) -> Result<()> {
        let _save = self.save_gate.lock().unwrap();
        let mut state = self.state.write().unwrap();
        if state.tool_only_migration_version == 1 {
            return Ok(());
        }
        if state.tool_only_migration_version > 1 {
            anyhow::bail!("unsupported extension migration version");
        }
        let previous = state.tool_only_migration_version;
        state.tool_only_migration_version = 1;
        let saved = persist_registry(&self.path, &state);
        if saved.is_err() {
            state.tool_only_migration_version = previous;
        }
        saved
    }

    pub fn tool_only_migration_version(&self) -> u32 {
        self.state.read().unwrap().tool_only_migration_version
    }

    pub fn quarantine_reason(&self, id: &str) -> Option<String> {
        self.state.read().unwrap().quarantined.get(id).cloned()
    }

    /// Disable before any cleanup, preserve the artifact/user data, and make
    /// old grants and parked developer records inert. Safe to repeat on boot.
    pub fn quarantine(&self, id: &str, reason: &str) -> Result<()> {
        fn retire(record: &mut ExtensionRecord) {
            record.enabled = false;
            record.granted.clear();
            record.slots.clear();
            record.prompt_layers_approved = None;
            record.actions_approved = None;
            if let Some(parked) = record.dev.as_mut().and_then(|dev| dev.replaced.as_mut()) {
                retire(parked);
            }
        }
        {
            let mut state = self.state.write().unwrap();
            if let Some(record) = state.records.get_mut(id) {
                record.execution_generation = next_execution_generation()?;
                retire(record);
                state.quarantined.insert(id.to_string(), reason.to_string());
                for occupant in state.slot_claims.values_mut() {
                    if occupant == id {
                        *occupant = CORE_DEFAULT.to_string();
                    }
                }
            }
        }
        self.save()
    }

    /// Load (or initialize) the registry.
    ///
    /// `settings_file_preexisted` is retained for call compatibility. Visual
    /// extension records are retired during load; Grain now owns its appearance.
    pub fn load(data_dir: &Path, _settings_file_preexisted: bool) -> Result<Self> {
        let path = data_dir.join(EXTENSIONS_FILE);
        let mut state = read_registry(&path)?;
        if state.tool_only_migration_version > 1 {
            anyhow::bail!("unsupported extension migration version; registry preserved, extensions unavailable");
        }
        if state.records.iter().any(|(id, record)| id != &record.id) {
            anyhow::bail!("inconsistent extension registry identity; registry preserved, extensions unavailable");
        }
        for record in state.records.values_mut() {
            record.execution_generation = next_execution_generation()?;
        }
        let reg = Self {
            path,
            state: RwLock::new(state),
            save_gate: std::sync::Mutex::new(()),
        };
        reg.heal_slots();
        reg.save()?;
        Ok(reg)
    }

    /// Reconcile persisted slot state with the current contract. Visual slots
    /// and variants are deliberately discarded: appearance is native-only.
    fn heal_slots(&self) {
        let mut state = self.state.write().unwrap();
        state.records.remove(RETIRED_AGENT_CENTER_VARIANT_ID);
        for record in state.records.values_mut() {
            record
                .slots
                .retain(|slot| grain_sdk::manifest::KNOWN_SLOTS.contains(&slot.as_str()));
        }
        state
            .slot_claims
            .retain(|slot, _| grain_sdk::manifest::KNOWN_SLOTS.contains(&slot.as_str()));
        for slot in grain_sdk::manifest::KNOWN_SLOTS {
            state
                .slot_claims
                .entry((*slot).to_string())
                .or_insert_with(|| CORE_DEFAULT.to_string());
        }
    }

    fn save(&self) -> Result<()> {
        let _save = self.save_gate.lock().unwrap();
        let state = self.state.read().unwrap();
        persist_registry(&self.path, &state)
    }

    /// Caller holds save_gate -> state write. Enabling/replacement is tentative
    /// until the file commits; refusal-only mutations may remain memory-first.
    fn persist_record_edit(
        &self,
        state: &mut RegistryFile,
        id: &str,
        rollback: Option<RecordEditSnapshot>,
    ) -> Result<()> {
        let saved = persist_registry(&self.path, state);
        if saved.is_err() {
            if let Some(snapshot) = rollback {
                snapshot.restore(state, id);
            }
        }
        saved
    }

    /// All installed pack records (unordered; callers sort by toggle_seq).
    pub fn records(&self) -> Vec<ExtensionRecord> {
        self.state
            .read()
            .unwrap()
            .records
            .values()
            .cloned()
            .collect()
    }

    pub fn record(&self, id: &str) -> Option<ExtensionRecord> {
        self.state.read().unwrap().records.get(id).cloned()
    }

    /// Run a synchronous admission check while registry mutations are excluded.
    /// Keep this short: no await, filesystem I/O, or reentrant registry calls.
    /// Native admission acquires this lock before the worker/pending locks.
    pub fn with_record_locked<R>(
        &self,
        id: &str,
        admit: impl FnOnce(Option<&ExtensionRecord>) -> R,
    ) -> R {
        let state = self.state.read().unwrap();
        admit(state.records.get(id))
    }

    pub fn record_revision(&self, id: &str) -> RecordRevision {
        RecordRevision::capture(&self.state.read().unwrap(), id)
    }

    /// Change the effective account without changing enablement, grants or
    /// toggle order. Failed login publication restores the old identity;
    /// failed logout persistence still invalidates the live identity.
    pub fn set_authentication_session_if_current<G>(
        &self,
        id: &str,
        revision: RecordRevision,
        session: Option<String>,
        owner: impl FnOnce() -> Option<G>,
    ) -> Result<u64> {
        if session.as_ref().is_some_and(|session| {
            session.len() != 32 || !session.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            anyhow::bail!("invalid native authentication session");
        }
        // Saves already acquire save_gate -> state. Keep the same ordering and
        // exclude readers during tentative login publication; no whole-state
        // clone or credential/vault I/O is needed.
        let _save = self.save_gate.lock().unwrap();
        let connecting = session.is_some();
        let mut state = self.state.write().unwrap();
        revision.check(&state, id)?;
        if session.is_some() && state.quarantined.contains_key(id) {
            anyhow::bail!("extension is quarantined");
        }
        let record = state
            .records
            .get_mut(id)
            .context("extension is not installed")?;
        if session.is_some()
            && (!record.enabled || !record.granted.iter().any(|grant| grant == "auth"))
        {
            anyhow::bail!("native authentication is not enabled/granted");
        }
        let generation = next_execution_generation()?;
        let Some(_owner) = owner() else {
            anyhow::bail!("authentication was cancelled or replaced");
        };
        let prior_session = std::mem::replace(&mut record.authentication_session, session);
        let prior_generation = record.execution_generation;
        record.execution_generation = generation;
        let saved = persist_registry(&self.path, &state);
        if connecting && saved.is_err() {
            // The unique candidate grant must never become usable when its
            // pointer was not committed. Logout remains memory-first even
            // on save failure, invalidating live calls before vault cleanup.
            let record = state
                .records
                .get_mut(id)
                .expect("record held under write lock");
            record.authentication_session = prior_session;
            record.execution_generation = prior_generation;
        }
        saved.map(|_| generation)
    }

    /// Short synchronous publication boundary, ordered registry -> runtime.
    /// No filesystem work, awaits, or registry reentry inside `publish`.
    pub fn with_current_record<R>(
        &self,
        id: &str,
        revision: RecordRevision,
        publish: impl FnOnce(Option<&ExtensionRecord>) -> R,
    ) -> Result<R> {
        let state = self.state.read().unwrap();
        revision.check(&state, id)?;
        Ok(publish(state.records.get(id)))
    }

    /// The installed record beneath a dev override, or the normal record when
    /// no override is active. A dev-only project has no installed record.
    pub fn installed_record(&self, id: &str) -> Option<ExtensionRecord> {
        let state = self.state.read().unwrap();
        let record = state.records.get(id)?;
        match &record.dev {
            Some(dev) => dev.replaced.as_deref().cloned(),
            None => Some(record.clone()),
        }
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.state.read().unwrap().records.contains_key(id)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.state
            .read()
            .unwrap()
            .records
            .get(id)
            .map(|r| r.enabled)
            .unwrap_or(false)
    }

    pub fn dev_path(&self, id: &str) -> Option<PathBuf> {
        self.state
            .read()
            .unwrap()
            .records
            .get(id)
            .and_then(|record| record.dev.as_ref())
            .map(|dev| dev.path.clone())
    }

    pub fn dev_overrides_installed(&self, id: &str) -> bool {
        self.state
            .read()
            .unwrap()
            .records
            .get(id)
            .and_then(|record| record.dev.as_ref())
            .is_some_and(|dev| dev.replaced.is_some())
    }

    pub fn dev_records(&self) -> Vec<(String, PathBuf)> {
        self.state
            .read()
            .unwrap()
            .records
            .values()
            .filter_map(|record| {
                record
                    .dev
                    .as_ref()
                    .map(|dev| (record.id.clone(), dev.path.clone()))
            })
            .collect()
    }

    // ── Slots (SPEC §3.2: at most one enabled occupant per slot) ───────────

    /// Who currently holds `slot` — an extension id, or `CORE_DEFAULT` for
    /// Grain's own behaviour.
    pub fn slot_occupant(&self, slot: &str) -> Option<String> {
        self.state.read().unwrap().slot_claims.get(slot).cloned()
    }

    /// Every slot currently held by `id`.
    pub fn slots_held(&self, id: &str) -> Vec<String> {
        let state = self.state.read().unwrap();
        let mut held: Vec<String> = state
            .slot_claims
            .iter()
            .filter(|(_, occupant)| occupant.as_str() == id)
            .map(|(slot, _)| slot.clone())
            .collect();
        held.sort();
        held
    }

    /// The first slot this extension *claims* that somebody else holds, if any.
    /// The gate for enabling: a conflict must reach the user as a takeover
    /// prompt, never be resolved silently or by load order.
    pub fn slot_conflict(&self, id: &str) -> Option<SlotConflict> {
        let state = self.state.read().unwrap();
        let rec = state.records.get(id)?;
        rec.slots.iter().find_map(|slot| {
            match state.slot_claims.get(slot) {
                Some(occupant) if occupant != id => Some(SlotConflict {
                    slot: slot.clone(),
                    current_occupant: occupant.clone(),
                }),
                // A valid slot can only be unclaimed in stale/corrupt state.
                _ => None,
            }
        })
    }

    /// Hand `slot` to `challenger`, returning whoever was displaced. The
    /// displaced extension is disabled in the same transaction — SPEC §3.2 has
    /// no state where two enabled extensions both believe they own a slot.
    /// Core defaults are displaced without disabling anything (there is no
    /// record to disable; core simply stops rendering that position).
    pub fn take_slot(&self, challenger: &str, slot: &str) -> Result<Option<String>> {
        let displaced = {
            let mut state = self.state.write().unwrap();
            let declares = state
                .records
                .get(challenger)
                .map(|r| r.slots.iter().any(|s| s == slot))
                .unwrap_or(false);
            if !declares {
                anyhow::bail!("'{challenger}' does not declare slot '{slot}'");
            }
            let displaced_generation = next_execution_generation()?;
            let previous = state
                .slot_claims
                .insert(slot.to_string(), challenger.to_string());
            match previous {
                Some(prev) if prev != CORE_DEFAULT && prev != challenger => {
                    if let Some(rec) = state.records.get_mut(&prev) {
                        rec.execution_generation = displaced_generation;
                        rec.enabled = false;
                    }
                    Some(prev)
                }
                _ => None,
            }
        };
        self.save()?;
        Ok(displaced)
    }

    /// Release every slot `id` holds, back to Grain's default where one exists.
    /// Called on disable and uninstall — SPEC §6: "slots released".
    fn release_slots_locked(state: &mut RegistryFile, id: &str) {
        let held: Vec<String> = state
            .slot_claims
            .iter()
            .filter(|(_, occupant)| occupant.as_str() == id)
            .map(|(slot, _)| slot.clone())
            .collect();
        for slot in held {
            if grain_sdk::manifest::KNOWN_SLOTS.contains(&slot.as_str()) {
                state.slot_claims.insert(slot, CORE_DEFAULT.to_string());
            } else {
                state.slot_claims.remove(&slot);
            }
        }
    }

    /// Force a slot's occupant without the takeover checks. Only for reconciling
    /// a slot whose truth lives outside the registry — today just the centre
    /// variant, whose real state is `agent_panel_position` (SPEC §10.2: enabling
    /// it adds it to the dropdown; *selecting* it takes the slot).
    pub fn set_slot_claim(&self, slot: &str, occupant: &str) -> Result<()> {
        {
            let mut state = self.state.write().unwrap();
            if state.slot_claims.get(slot).map(String::as_str) == Some(occupant) {
                return Ok(());
            }
            state
                .slot_claims
                .insert(slot.to_string(), occupant.to_string());
        }
        self.save()
    }

    /// Enable/disable an installed pack. Enabling assigns the next toggle
    /// sequence (SPEC §4.4: re-enabling moves to the end of toggle order) and
    /// claims the slots the pack declares; disabling releases them.
    ///
    /// Enabling into an occupied slot is refused here as well as at the command
    /// layer, so a caller that forgets to check cannot steal a slot by accident.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        self.set_enabled_if_current(id, enabled, None, None, || Some(()))
    }

    pub fn set_enabled_at_revision(
        &self,
        id: &str,
        enabled: bool,
        revision: RecordRevision,
    ) -> Result<bool> {
        self.set_enabled_if_current(id, enabled, None, Some(revision), || Some(()))
    }

    /// A late resource observer may disable only the execution it sampled.
    pub fn disable_if_generation(&self, id: &str, generation: u64) -> Result<bool> {
        self.disable_if_generation_guarded(id, generation, || Some(()))
    }

    /// Acquire an observer's runtime ownership guard while the registry write
    /// lock is held. Drop it before persistence, never hold it across I/O/await.
    /// Callers must follow registry -> runtime lock order and never reenter us.
    pub fn disable_if_generation_guarded<G>(
        &self,
        id: &str,
        generation: u64,
        owner: impl FnOnce() -> Option<G>,
    ) -> Result<bool> {
        self.set_enabled_if_current(id, false, Some(generation), None, owner)
    }

    fn set_enabled_if_current<G>(
        &self,
        id: &str,
        enabled: bool,
        expected_generation: Option<u64>,
        expected_revision: Option<RecordRevision>,
        owner: impl FnOnce() -> Option<G>,
    ) -> Result<bool> {
        if enabled {
            if let Some(reason) = self.quarantine_reason(id) {
                anyhow::bail!("extension is quarantined: {reason}");
            }
            if let Some(c) = self.slot_conflict(id) {
                anyhow::bail!("slot '{}' is occupied by '{}'", c.slot, c.current_occupant);
            }
        }
        let _save = self.save_gate.lock().unwrap();
        let mut state = self.state.write().unwrap();
        let changed = {
            if let Some(revision) = expected_revision {
                revision.check(&state, id)?;
            }
            if expected_generation.is_some_and(|generation| {
                state
                    .records
                    .get(id)
                    .is_none_or(|record| record.execution_generation != generation)
            }) {
                return Ok(false);
            }
            if enabled {
                if let Some(reason) = state.quarantined.get(id) {
                    anyhow::bail!("extension is quarantined: {reason}");
                }
                if let Some(record) = state.records.get(id) {
                    for slot in &record.slots {
                        if let Some(occupant) = state
                            .slot_claims
                            .get(slot)
                            .filter(|occupant| occupant.as_str() != id)
                        {
                            anyhow::bail!("slot '{slot}' is occupied by '{occupant}'");
                        }
                    }
                }
            }
            let next = state.next_toggle_seq;
            let activating = enabled && state.records.get(id).is_some_and(|record| !record.enabled);
            let following = if activating {
                Some(next.checked_add(1).context("toggle sequence exhausted")?)
            } else {
                None
            };
            let rollback = activating.then(|| {
                RecordEditSnapshot::capture(
                    &state,
                    id,
                    state
                        .records
                        .get(id)
                        .map(|record| record.slots.as_slice())
                        .unwrap_or_default(),
                )
            });
            let Some(_owner) = owner() else {
                return Ok(false);
            };
            let declared = match state.records.get_mut(id) {
                Some(rec) if rec.enabled != enabled || !enabled => {
                    rec.execution_generation = next_execution_generation()?;
                    rec.enabled = enabled;
                    if enabled {
                        rec.toggle_seq = next;
                    }
                    Some(rec.slots.clone())
                }
                _ => None,
            };
            match declared {
                Some(slots) => {
                    if enabled {
                        state.next_toggle_seq = following.expect("checked toggle sequence");
                        for slot in slots {
                            state.slot_claims.insert(slot, id.to_string());
                        }
                    } else {
                        Self::release_slots_locked(&mut state, id);
                    }
                    Some(rollback)
                }
                None => None,
            }
        };
        if let Some(rollback) = changed {
            self.persist_record_edit(&mut state, id, rollback)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Record a built-in's enable moment so it participates in toggle order
    /// (its actual enabled bit lives in settings; call this when flipping it on).
    pub fn touch_builtin_toggle(&self, id: &str) -> Result<()> {
        {
            let mut state = self.state.write().unwrap();
            let seq = state.next_toggle_seq;
            state.builtin_toggle_seq.insert(id.to_string(), seq);
            state.next_toggle_seq += 1;
        }
        self.save()
    }

    /// Toggle-order position for any id (built-in or pack); `u64::MAX` for
    /// never-toggled (sorts last, stable).
    pub fn toggle_seq(&self, id: &str) -> u64 {
        let state = self.state.read().unwrap();
        state
            .records
            .get(id)
            .map(|r| r.toggle_seq)
            .or_else(|| state.builtin_toggle_seq.get(id).copied())
            .unwrap_or(u64::MAX)
    }

    /// Install a pack record (import path lands in the next chunk; the
    /// centre-variant import uses this today via `load`).
    pub fn install(&self, record: ExtensionRecord) -> Result<()> {
        self.install_owned(record, None, false).map(|_| ())
    }

    pub fn install_if_current(
        &self,
        record: ExtensionRecord,
        revision: RecordRevision,
    ) -> Result<u64> {
        self.install_owned(record, Some(revision), false)
    }

    /// Grant and enable the reviewed tool-only record in one mutation. No caller
    /// may use this to activate a quarantined or retired slot-bearing package.
    pub fn approve_and_enable_if_current(
        &self,
        record: ExtensionRecord,
        revision: RecordRevision,
    ) -> Result<u64> {
        self.install_owned(record, Some(revision), true)
    }

    fn install_owned(
        &self,
        mut record: ExtensionRecord,
        revision: Option<RecordRevision>,
        approve: bool,
    ) -> Result<u64> {
        let _save = self.save_gate.lock().unwrap();
        let mut state = self.state.write().unwrap();
        let record_id = record.id.clone();
        let rollback;
        let generation;
        {
            if let Some(revision) = revision {
                revision.check(&state, &record.id)?;
            }
            let reserved_generation = next_execution_generation()?;
            // A disabled update keeps its refusal live even if persistence
            // fails. Enabled/approved replacements must roll back instead.
            rollback = (approve || record.enabled)
                .then(|| RecordEditSnapshot::capture(&state, &record.id, &[]));
            if approve {
                if !state.records.contains_key(&record.id) {
                    anyhow::bail!("Extension is not installed.");
                }
                if let Some(reason) = state.quarantined.get(&record.id) {
                    anyhow::bail!("extension is quarantined: {reason}");
                }
                if !record.slots.is_empty() {
                    anyhow::bail!("Only tool-only packages can be approved.");
                }
                if !record.enabled {
                    let next = state
                        .next_toggle_seq
                        .checked_add(1)
                        .ok_or_else(|| anyhow::anyhow!("toggle sequence exhausted"))?;
                    record.toggle_seq = state.next_toggle_seq;
                    state.next_toggle_seq = next;
                }
                record.enabled = true;
            }
            record.execution_generation = reserved_generation;
            generation = record.execution_generation;
            let id = record.id.clone();
            state.quarantined.remove(&id);
            // A store/manual install arriving while this id is overridden
            // updates the parked installed record, never the effective dev
            // record. The author can keep testing without losing the update.
            if state
                .records
                .get(&id)
                .is_some_and(|active| active.dev.is_some())
            {
                if record.dev.is_some() {
                    if !record.enabled {
                        Self::release_slots_locked(&mut state, &id);
                    }
                    // Mutating the effective dev record (for example after a
                    // capability grant) must not turn it into its own parked
                    // installed version.
                    state.records.insert(id, record);
                } else {
                    state
                        .records
                        .get_mut(&id)
                        .and_then(|active| active.dev.as_mut())
                        .expect("dev record exists")
                        .replaced = Some(Box::new(record));
                }
                return self
                    .persist_record_edit(&mut state, &record_id, rollback)
                    .map(|_| generation);
            }
            let declared = record.slots.clone();
            if !record.enabled {
                Self::release_slots_locked(&mut state, &id);
            }
            state.records.insert(id.clone(), record);
            // An update may drop a slot it used to declare; holding a claim on
            // a slot you no longer declare would block everyone else forever.
            let stale: Vec<String> = state
                .slot_claims
                .iter()
                .filter(|(slot, occupant)| occupant.as_str() == id && !declared.contains(slot))
                .map(|(slot, _)| slot.clone())
                .collect();
            for slot in stale {
                if grain_sdk::manifest::KNOWN_SLOTS.contains(&slot.as_str()) {
                    state.slot_claims.insert(slot, CORE_DEFAULT.to_string());
                } else {
                    state.slot_claims.remove(&slot);
                }
            }
            // A newly declared slot is NOT auto-claimed on update: an already
            // enabled pack must not gain a position the user never granted it.
            // It stays a pending conflict until the user takes the slot.
        }
        self.persist_record_edit(&mut state, &record_id, rollback)
            .map(|_| generation)
    }

    /// Make `record` the effective load-unpacked extension for its id. Any
    /// installed record is parked verbatim; replacing one dev path preserves
    /// that original backup rather than nesting overrides.
    pub fn load_dev(&self, record: ExtensionRecord, path: PathBuf) -> Result<()> {
        self.load_dev_owned(record, path, None).map(|_| ())
    }

    pub fn load_dev_if_current(
        &self,
        record: ExtensionRecord,
        path: PathBuf,
        revision: RecordRevision,
    ) -> Result<u64> {
        self.load_dev_owned(record, path, Some(revision))
    }

    fn load_dev_owned(
        &self,
        mut record: ExtensionRecord,
        path: PathBuf,
        revision: Option<RecordRevision>,
    ) -> Result<u64> {
        let _save = self.save_gate.lock().unwrap();
        let mut state = self.state.write().unwrap();
        let record_id = record.id.clone();
        let rollback;
        let generation;
        {
            if let Some(revision) = revision {
                revision.check(&state, &record.id)?;
            }
            rollback = RecordEditSnapshot::capture(&state, &record.id, &[]);
            record.execution_generation = next_execution_generation()?;
            generation = record.execution_generation;
            let id = record.id.clone();
            // A development override is a distinct account owner. Preserve a
            // session only when reloading the same explicit project directory.
            record.authentication_session = state.records.get(&id).and_then(|previous| {
                previous
                    .dev
                    .as_ref()
                    .filter(|dev| dev.path == path)
                    .and_then(|_| previous.authentication_session.clone())
            });
            let replaced =
                state
                    .records
                    .remove(&id)
                    .and_then(|mut previous| match previous.dev.take() {
                        Some(dev) => dev.replaced,
                        None => Some(Box::new(previous)),
                    });
            Self::release_slots_locked(&mut state, &id);
            record.dev = Some(DevOverride { path, replaced });
            state.records.insert(id, record);
        }
        self.persist_record_edit(&mut state, &record_id, Some(rollback))
            .map(|_| generation)
    }

    /// Remove a load-unpacked override and restore its parked installed record,
    /// if any. A restored enabled record reclaims only still-free slots; if a
    /// different extension took one meanwhile, it is restored disabled so no
    /// takeover happens silently.
    pub fn unload_dev(&self, id: &str) -> Result<bool> {
        let _save = self.save_gate.lock().unwrap();
        let mut state = self.state.write().unwrap();
        let rollback;
        let changed = {
            if state
                .records
                .get(id)
                .is_none_or(|record| record.dev.is_none())
            {
                return Ok(false);
            }
            let restored_slots = state
                .records
                .get(id)
                .and_then(|record| record.dev.as_ref())
                .and_then(|dev| dev.replaced.as_ref())
                .map(|record| record.slots.as_slice())
                .unwrap_or_default();
            rollback = RecordEditSnapshot::capture(&state, id, restored_slots);
            // Reserve before removing the active record, so exhaustion cannot
            // partially unload an override or resurrect its parked identity.
            let generation = next_execution_generation()?;
            state.removal_epoch = generation;
            let Some(mut active) = state.records.remove(id) else {
                return Ok(false);
            };
            let Some(dev) = active.dev.take() else {
                state.records.insert(id.to_string(), active);
                return Ok(false);
            };
            Self::release_slots_locked(&mut state, id);

            if let Some(mut replaced) = dev.replaced.map(|record| *record) {
                replaced.execution_generation = generation;
                if replaced.enabled {
                    let contested = replaced.slots.iter().any(|slot| {
                        state
                            .slot_claims
                            .get(slot)
                            .is_some_and(|occupant| occupant != CORE_DEFAULT && occupant != id)
                    });
                    if contested {
                        replaced.enabled = false;
                    } else {
                        for slot in &replaced.slots {
                            state.slot_claims.insert(slot.clone(), id.to_string());
                        }
                    }
                }
                state.records.insert(id.to_string(), replaced);
            }
            true
        };
        if changed {
            self.persist_record_edit(&mut state, id, Some(rollback))?;
        }
        Ok(changed)
    }

    pub fn uninstall(&self, id: &str) -> Result<bool> {
        self.uninstall_with_record(id)
            .map(|removed| removed.is_some())
    }

    /// Return the removed installed owner under the same mutation lock. A dev
    /// override's record/account is never returned for installed cleanup.
    pub fn uninstall_with_record(&self, id: &str) -> Result<Option<ExtensionRecord>> {
        let removed = {
            let mut state = self.state.write().unwrap();
            state.removal_epoch = next_execution_generation()?;
            let dev_active = state
                .records
                .get(id)
                .is_some_and(|record| record.dev.is_some());
            if dev_active {
                // Keep the effective development project and its own account.
                state
                    .records
                    .get_mut(id)
                    .and_then(|record| record.dev.as_mut())
                    .expect("dev record exists")
                    .replaced
                    .take()
                    .map(|record| *record)
            } else {
                let removed = state.records.remove(id);
                if removed.is_some() {
                    Self::release_slots_locked(&mut state, id);
                }
                removed
            }
        };
        if removed.is_some() {
            self.save()?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewed_activation_and_normal_enable_refuse_stale_or_retired_owners() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("tools", &[])).unwrap();
        let old = reg.record_revision("tools");
        reg.set_enabled("tools", false).unwrap(); // explicit refusal while already off
        assert!(reg.set_enabled_at_revision("tools", true, old).is_err());
        assert!(reg
            .approve_and_enable_if_current(reg.record("tools").unwrap(), old)
            .is_err());
        assert!(!reg.is_enabled("tools"));
        let revision = reg.record_revision("tools");
        let mut record = reg.record("tools").unwrap();
        record.granted.push("storage".into());
        reg.approve_and_enable_if_current(record, revision).unwrap();
        let current = reg.record("tools").unwrap();
        assert!(current.enabled);
        assert_eq!(current.granted, vec!["storage"]);
        assert!(reg
            .approve_and_enable_if_current(current, revision)
            .is_err());
        reg.quarantine("tools", "retired").unwrap();
        assert!(reg
            .approve_and_enable_if_current(
                reg.record("tools").unwrap(),
                reg.record_revision("tools")
            )
            .is_err());
        reg.install(pack("slots", &["output.destination"])).unwrap();
        assert!(reg
            .approve_and_enable_if_current(
                reg.record("slots").unwrap(),
                reg.record_revision("slots")
            )
            .is_err());
    }

    #[test]
    fn revision_refuses_stale_writers_and_late_runtime_publication() {
        for mode in [
            "disable",
            "reenable",
            "remove",
            "reinstall",
            "quarantine",
            "restore",
            "dev_replace",
        ] {
            let dir = tmp();
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            reg.install(pack("tools", &[])).unwrap();
            reg.set_enabled("tools", true).unwrap();
            if mode == "restore" || mode == "dev_replace" {
                reg.load_dev(reg.record("tools").unwrap(), dir.path().join("project"))
                    .unwrap();
            }
            let revision = reg.record_revision("tools");
            let stale = reg.record("tools").unwrap();
            match mode {
                "disable" => {
                    reg.set_enabled("tools", false).unwrap();
                }
                "reenable" => {
                    reg.set_enabled("tools", false).unwrap();
                    reg.set_enabled("tools", true).unwrap();
                }
                "remove" => {
                    reg.uninstall("tools").unwrap();
                }
                "reinstall" => {
                    reg.uninstall("tools").unwrap();
                    reg.install(stale.clone()).unwrap();
                }
                "quarantine" => {
                    reg.quarantine("tools", "retired").unwrap();
                }
                "restore" => {
                    reg.unload_dev("tools").unwrap();
                }
                "dev_replace" => {
                    reg.load_dev(stale.clone(), dir.path().join("replacement"))
                        .unwrap();
                }
                _ => unreachable!(),
            }
            let before = serde_json::to_value(reg.record("tools")).unwrap();
            assert!(
                reg.install_if_current(stale.clone(), revision).is_err(),
                "{mode}"
            );
            assert!(
                reg.load_dev_if_current(stale, dir.path().join("late"), revision)
                    .is_err(),
                "{mode}"
            );
            let mut published = false;
            assert!(
                reg.with_current_record("tools", revision, |_| published = true)
                    .is_err(),
                "{mode}"
            );
            assert!(!published, "{mode}: no token/worker publication callback");
            assert_eq!(serde_json::to_value(reg.record("tools")).unwrap(), before);
        }
    }

    #[test]
    fn vacant_revision_remembers_removal_without_retaining_tombstones() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let before = reg.record_revision("tools");
        reg.install(pack("tools", &[])).unwrap();
        reg.uninstall("tools").unwrap();
        assert!(reg.install_if_current(pack("tools", &[]), before).is_err());
        let before = reg.record_revision("tools");
        reg.uninstall("tools").unwrap(); // removal intent also defeats a late first install
        assert!(reg.install_if_current(pack("tools", &[]), before).is_err());
        let fresh = reg.record_revision("tools");
        reg.install_if_current(pack("tools", &[]), fresh).unwrap();
        let unrelated = reg.record_revision("tools");
        reg.uninstall("other").unwrap();
        reg.with_current_record("tools", unrelated, |_| ()).unwrap();
        let persisted = fs::read_to_string(&reg.path).unwrap();
        assert!(!persisted.contains("removal_epoch"));
        assert!(!persisted.contains("execution_generation"));
        reg.load_dev(pack("dev_only", &[]), dir.path().join("project"))
            .unwrap();
        let vacant_parked = reg.record_revision("dev_only");
        reg.uninstall("dev_only").unwrap();
        assert!(reg
            .install_if_current(pack("dev_only", &[]), vacant_parked)
            .is_err());
        assert!(reg.record("dev_only").unwrap().dev.is_some());
        assert!(reg.installed_record("dev_only").is_none());
    }

    #[test]
    fn revision_includes_parked_updates_and_removal() {
        for remove in [false, true] {
            let dir = tmp();
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            reg.install(pack("tools", &[])).unwrap();
            reg.load_dev(pack("tools", &[]), dir.path().join("project"))
                .unwrap();
            let before = reg.record_revision("tools");
            let active = reg.record("tools").unwrap();
            if remove {
                reg.uninstall("tools").unwrap();
            } else {
                reg.install(pack("tools", &[])).unwrap();
            }
            assert_eq!(
                reg.record("tools").unwrap().execution_generation,
                active.execution_generation
            );
            assert!(reg.install_if_current(active, before).is_err());
            reg.unload_dev("tools").unwrap();
            assert_eq!(reg.record("tools").is_none(), remove);
        }
    }

    #[test]
    fn concurrent_revision_writers_have_one_winner_and_save_safely() {
        let dir = tmp();
        let reg = std::sync::Arc::new(ExtensionsRegistry::load(dir.path(), false).unwrap());
        reg.install(pack("tools", &[])).unwrap();
        let revision = reg.record_revision("tools");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let threads: Vec<_> = (0..2)
            .map(|index| {
                let reg = reg.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut record = pack("tools", &[]);
                    record.installed_version = index.to_string();
                    barrier.wait();
                    reg.install_if_current(record, revision).is_ok()
                })
            })
            .collect();
        barrier.wait();
        let wins = threads
            .into_iter()
            .map(|thread| usize::from(thread.join().unwrap()))
            .sum::<usize>();
        assert_eq!(wins, 1);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(5));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let reg = reg.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    for _ in 0..25 {
                        reg.save().unwrap();
                    }
                })
            })
            .collect();
        barrier.wait();
        for thread in threads {
            thread.join().unwrap();
        }
        let loaded = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert_eq!(
            loaded.record("tools").unwrap().installed_version,
            reg.record("tools").unwrap().installed_version
        );
    }

    #[test]
    fn publication_holds_registry_ownership_until_worker_is_visible() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("tools", &[])).unwrap();
        let revision = reg.record_revision("tools");
        let generation = reg
            .with_current_record("tools", revision, |record| {
                assert!(reg.state.try_write().is_err());
                record.unwrap().execution_generation
            })
            .unwrap();
        assert_eq!(
            generation,
            reg.record("tools").unwrap().execution_generation
        );
        assert!(reg.state.try_write().is_ok());
    }

    #[test]
    fn failed_save_keeps_new_revision_and_disabled_slot_release() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("tools", &["output.destination"])).unwrap();
        reg.take_slot("tools", "output.destination").unwrap();
        reg.set_enabled("tools", true).unwrap();
        let revision = reg.record_revision("tools");
        let mut record = reg.record("tools").unwrap();
        record.enabled = false;
        fs::remove_file(&reg.path).unwrap();
        fs::create_dir(&reg.path).unwrap();
        assert!(reg.install_if_current(record, revision).is_err());
        assert!(!reg.is_enabled("tools"));
        assert!(reg.slots_held("tools").is_empty());
        assert!(reg.with_current_record("tools", revision, |_| ()).is_err());
    }

    #[test]
    fn quarantine_survives_restart_and_disables_parked_dev_state() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut legacy = pack("com.x.old", &["prompt.main"]);
        legacy.enabled = true;
        legacy.granted = vec!["capture:screen-image".into(), "resident".into()];
        legacy.prompt_layers_approved = Some("old approval".into());
        reg.install(legacy.clone()).unwrap();
        reg.load_dev(legacy, dir.path().join("project")).unwrap();
        reg.quarantine("com.x.old", "retired context access")
            .unwrap();
        reg.quarantine("com.x.old", "retired context access")
            .unwrap();
        drop(reg);
        let reg = ExtensionsRegistry::load(dir.path(), true).unwrap();
        assert!(!reg.is_enabled("com.x.old"));
        assert!(reg.record("com.x.old").unwrap().granted.is_empty());
        assert!(reg.slots_held("com.x.old").is_empty());
        assert!(reg.set_enabled("com.x.old", true).is_err());
        reg.unload_dev("com.x.old").unwrap();
        assert!(!reg.is_enabled("com.x.old"));
        assert!(reg.set_enabled("com.x.old", true).is_err());
    }

    #[test]
    fn interrupted_binding_retirement_preserves_chords_and_checkpoints_after_commit() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut settings = crate::settings::AppSettings::default();
        let core_before = serde_json::to_value(&settings.bindings).unwrap();
        let binding = crate::settings::ShortcutBinding {
            id: "ext:com.example.old:open".into(),
            name: "Open".into(),
            description: "Edited".into(),
            default_binding: "".into(),
            current_binding: "Ctrl+Shift+J".into(),
        };
        settings.bindings.insert(binding.id.clone(), binding);
        archive_retired_bindings(dir.path(), &settings).unwrap();
        assert_eq!(reg.tool_only_migration_version(), 0);
        archive_retired_bindings(dir.path(), &settings).unwrap(); // interrupted before settings commit
        retire_extension_bindings(&mut settings);
        assert_eq!(
            serde_json::to_value(&settings.bindings).unwrap(),
            core_before
        );
        reg.finish_tool_only_migration().unwrap();
        reg.finish_tool_only_migration().unwrap();
        drop(reg);
        let reg = ExtensionsRegistry::load(dir.path(), true).unwrap();
        assert_eq!(reg.tool_only_migration_version(), 1);
        let archive: Vec<serde_json::Value> = serde_json::from_slice(
            &fs::read(dir.path().join("retired-extension-bindings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(archive.len(), 1);
        assert_eq!(archive[0][1]["current_binding"], "Ctrl+Shift+J");
    }

    #[test]
    fn a_failed_checkpoint_write_remains_retryable_in_the_same_process() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let original = fs::read(&reg.path).unwrap();
        fs::remove_file(&reg.path).unwrap();
        fs::create_dir(&reg.path).unwrap(); // deterministic rename failure
        assert!(reg.finish_tool_only_migration().is_err());
        assert_eq!(reg.tool_only_migration_version(), 0);
        fs::remove_dir(&reg.path).unwrap();
        fs::write(&reg.path, original).unwrap();
        reg.finish_tool_only_migration().unwrap();
        assert_eq!(reg.tool_only_migration_version(), 1);
    }

    #[test]
    fn a_corrupt_binding_archive_cannot_be_overwritten_or_marked_migrated() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut settings = crate::settings::AppSettings::default();
        let binding = crate::settings::ShortcutBinding {
            id: "ext:com.example.old:open".into(),
            name: "Open".into(),
            description: "".into(),
            default_binding: "".into(),
            current_binding: "Ctrl+J".into(),
        };
        settings.bindings.insert(binding.id.clone(), binding);
        let path = dir.path().join("retired-extension-bindings.json");
        fs::write(&path, "preserve malformed archive").unwrap();
        assert!(archive_retired_bindings(dir.path(), &settings).is_err());
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "preserve malformed archive"
        );
        assert_eq!(reg.tool_only_migration_version(), 0);
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn synchronous_admission_holds_read_lock_only_until_callback_returns() {
        let dir = tmp();
        let registry = ExtensionsRegistry::load(dir.path(), false).unwrap();
        registry.install(pack("tools", &[])).unwrap();
        registry.set_enabled("tools", true).unwrap();
        let admitted = registry.with_record_locked("tools", |record| {
            assert!(
                registry.state.try_write().is_err(),
                "mutation excluded during admission"
            );
            record.unwrap().enabled
        });
        assert!(admitted);
        assert!(
            registry.state.try_write().is_ok(),
            "no lock retained by the result"
        );
        registry.set_enabled("tools", false).unwrap();
        assert!(!registry.with_record_locked("tools", |record| record.unwrap().enabled));
        assert!(registry.with_record_locked("missing", |record| record.is_none()));
    }

    #[test]
    fn load_retires_visual_extension_state() {
        let dir = tmp();
        let stale = r#"{
          "records": {
            "grain.agent-center-layout": {
              "id": "grain.agent-center-layout", "enabled": true,
              "slots": [], "variant_slots": ["agent.reply-surface"]
            },
            "com.example.visual": {
              "id": "com.example.visual", "enabled": true,
              "slots": ["pill.theme", "output.destination"],
              "variant_slots": ["overlay.recording"]
            }
          },
          "slot_claims": {
            "pill.theme": "com.example.visual",
            "agent.reply-surface": "grain.agent-center-layout",
            "output.destination": "com.example.visual"
          }
        }"#;
        fs::write(dir.path().join(EXTENSIONS_FILE), stale).unwrap();

        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(!reg.is_installed(RETIRED_AGENT_CENTER_VARIANT_ID));
        assert_eq!(
            reg.record("com.example.visual").unwrap().slots,
            vec!["output.destination".to_string()]
        );
        assert_eq!(reg.slot_occupant("pill.theme"), None);
        assert_eq!(reg.slot_occupant("agent.reply-surface"), None);
        assert_eq!(
            reg.slot_occupant("output.destination").as_deref(),
            Some("com.example.visual")
        );
    }

    #[test]
    fn toggle_order_is_enable_order_and_reenable_moves_to_end() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        for id in ["a", "b"] {
            reg.install(pack(id, &[])).unwrap();
        }
        reg.set_enabled("a", true).unwrap();
        reg.set_enabled("b", true).unwrap();
        assert!(
            reg.toggle_seq("a") < reg.toggle_seq("b"),
            "first enabled = top"
        );

        // Built-ins participate in the same ordering space.
        reg.touch_builtin_toggle(BUILTIN_SNIPPETS).unwrap();
        assert!(reg.toggle_seq("b") < reg.toggle_seq(BUILTIN_SNIPPETS));

        // Disable + re-enable moves to the end (SPEC §4.4).
        reg.set_enabled("a", false).unwrap();
        reg.set_enabled("a", true).unwrap();
        assert!(reg.toggle_seq("a") > reg.toggle_seq(BUILTIN_SNIPPETS));

        // Never-toggled sorts last.
        assert_eq!(reg.toggle_seq("never"), u64::MAX);
    }

    fn pack(id: &str, slots: &[&str]) -> ExtensionRecord {
        ExtensionRecord {
            id: id.into(),
            enabled: false,
            execution_generation: 0,
            toggle_seq: 0,
            installed_version: "1".into(),
            artifact_sha256: None,
            granted: vec![],
            prompt_layers_approved: None,
            actions_approved: None,
            authentication_approved: None,
            authentication_session: None,
            recommend_approved: None,
            slots: slots.iter().map(|s| s.to_string()).collect(),
            dev: None,
            trust: Trust::UNTRUSTED_DEFAULT,
        }
    }

    #[test]
    fn native_account_publication_is_revision_checked_and_persistent() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut record = pack("account", &[]);
        record.enabled = true;
        record.granted = vec!["auth".into()];
        reg.install(record).unwrap();
        let before = reg.record("account").unwrap();
        let revision = reg.record_revision("account");
        let session = "a".repeat(32);
        assert!(reg
            .set_authentication_session_if_current(
                "account",
                revision,
                Some(session.clone()),
                || None::<()>
            )
            .is_err());
        assert_eq!(
            reg.record("account").unwrap().execution_generation,
            before.execution_generation
        );
        let generation = reg
            .set_authentication_session_if_current(
                "account",
                revision,
                Some(session.clone()),
                || Some(()),
            )
            .unwrap();
        let current = reg.record("account").unwrap();
        assert_ne!(generation, before.execution_generation);
        assert_eq!(current.granted, before.granted);
        assert_eq!(current.toggle_seq, before.toggle_seq);
        assert!(current.enabled);
        assert!(reg
            .set_authentication_session_if_current("account", revision, None, || Some(()))
            .is_err());
        let restarted = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert_eq!(
            restarted.record("account").unwrap().authentication_session,
            Some(session)
        );
        let revision = restarted.record_revision("account");
        restarted
            .set_authentication_session_if_current("account", revision, None, || Some(()))
            .unwrap();
        assert!(ExtensionsRegistry::load(dir.path(), false)
            .unwrap()
            .record("account")
            .unwrap()
            .authentication_session
            .is_none());
    }

    #[test]
    fn native_login_refuses_disabled_ungranted_and_invalid_sessions() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("account", &[])).unwrap();
        let revision = reg.record_revision("account");
        assert!(reg
            .set_authentication_session_if_current(
                "account",
                revision,
                Some("a".repeat(32)),
                || Some(())
            )
            .is_err());
        reg.set_enabled("account", true).unwrap();
        let revision = reg.record_revision("account");
        assert!(reg
            .set_authentication_session_if_current(
                "account",
                revision,
                Some("a".repeat(32)),
                || Some(())
            )
            .is_err());
        assert!(reg
            .set_authentication_session_if_current(
                "account",
                revision,
                Some("../account".into()),
                || Some(())
            )
            .is_err());
        // Explicit disconnect remains possible without an auth grant.
        reg.set_authentication_session_if_current("account", revision, None, || Some(()))
            .unwrap();
    }

    #[test]
    fn native_logout_save_failure_keeps_the_in_memory_account_disconnected() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut record = pack("account", &[]);
        record.authentication_session = Some("a".repeat(32));
        reg.install(record).unwrap();
        let revision = reg.record_revision("account");
        fs::remove_file(&reg.path).unwrap();
        fs::create_dir(&reg.path).unwrap();
        assert!(reg
            .set_authentication_session_if_current("account", revision, None, || Some(()))
            .is_err());
        assert!(reg
            .record("account")
            .unwrap()
            .authentication_session
            .is_none());
        assert!(reg
            .with_current_record("account", revision, |_| ())
            .is_err());
    }

    #[test]
    fn installed_and_development_accounts_are_separate_and_exactly_removed() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut installed = pack("account", &[]);
        installed.enabled = true;
        installed.granted = vec!["auth".into()];
        installed.authentication_session = Some("a".repeat(32));
        reg.install(installed.clone()).unwrap();
        let path = dir.path().join("dev");
        reg.load_dev(installed.clone(), path.clone()).unwrap();
        assert!(reg
            .record("account")
            .unwrap()
            .authentication_session
            .is_none());
        let revision = reg.record_revision("account");
        reg.set_authentication_session_if_current(
            "account",
            revision,
            Some("b".repeat(32)),
            || Some(()),
        )
        .unwrap();
        reg.load_dev(installed.clone(), path.clone()).unwrap();
        assert_eq!(
            reg.record("account").unwrap().authentication_session,
            Some("b".repeat(32))
        );
        reg.unload_dev("account").unwrap();
        assert_eq!(
            reg.record("account").unwrap().authentication_session,
            Some("a".repeat(32))
        );
        reg.load_dev(installed.clone(), path).unwrap();
        let revision = reg.record_revision("account");
        reg.set_authentication_session_if_current(
            "account",
            revision,
            Some("b".repeat(32)),
            || Some(()),
        )
        .unwrap();
        assert_eq!(
            reg.uninstall_with_record("account")
                .unwrap()
                .unwrap()
                .authentication_session,
            Some("a".repeat(32))
        );
        assert_eq!(
            reg.record("account").unwrap().authentication_session,
            Some("b".repeat(32))
        );
        reg.load_dev(installed, dir.path().join("different-dev"))
            .unwrap();
        assert!(reg
            .record("account")
            .unwrap()
            .authentication_session
            .is_none());
    }

    #[test]
    fn native_confirmation_expires_across_enablement_source_and_grant_changes() {
        use crate::execution::{PreparedCall, RiskClass, SideEffect, Stale};
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let manifest: grain_sdk::ExtensionManifest = serde_json::from_value(serde_json::json!({
            "id": "com.example.tools", "name": "Tools", "version": "1.0.0",
            "tier": "scripted", "entry_source": "grain.actions({write: () => 'hello'});",
            "contributes": {"actions": [{"id": "write", "title": "Test write", "risk": "confirm"}]}
        }))
        .unwrap();
        let mut installed = pack("com.example.tools", &[]);
        installed.actions_approved = Some(actions_fingerprint(&manifest.contributes.actions));
        reg.install(installed).unwrap();
        reg.set_enabled("com.example.tools", true).unwrap();
        let record = reg.record("com.example.tools").unwrap();
        let digest = native_call_fingerprint(&record, &manifest).unwrap();
        let call = PreparedCall {
            token: "confirmation".into(),
            canonical_id: "com.example.tools:write".into(),
            extension_id: record.id.clone(),
            action_id: "write".into(),
            provider_name: "Tools".into(),
            arguments: serde_json::json!({}),
            risk: RiskClass::Confirm,
            side_effect: SideEffect::Write,
            manifest_digest: digest.clone(),
            idempotency_key: None,
            prepared_at_ms: 0,
            expires_at_ms: 1000,
        };
        let reloaded = serde_json::from_slice(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert_eq!(
            call.still_valid(&native_call_fingerprint(&record, &reloaded).unwrap(), 1),
            Ok(())
        );
        reg.set_enabled("com.example.tools", false).unwrap();
        reg.set_enabled("com.example.tools", true).unwrap();
        assert_eq!(
            call.still_valid(
                &native_call_fingerprint(&reg.record(&record.id).unwrap(), &manifest).unwrap(),
                1
            ),
            Err(Stale::ManifestChanged)
        );
        let mut edited = manifest.clone();
        edited.entry_source.push_str("\n// changed implementation");
        assert_eq!(
            actions_fingerprint(&edited.contributes.actions),
            actions_fingerprint(&manifest.contributes.actions)
        );
        assert_eq!(
            call.still_valid(&native_call_fingerprint(&record, &edited).unwrap(), 1),
            Err(Stale::ManifestChanged)
        );
        let mut changed = record.clone();
        changed.granted.push("storage".into());
        assert_eq!(
            call.still_valid(&native_call_fingerprint(&changed, &manifest).unwrap(), 1),
            Err(Stale::ManifestChanged)
        );
        changed = record.clone();
        changed.authentication_session = Some("a".repeat(32));
        assert_eq!(
            call.still_valid(&native_call_fingerprint(&changed, &manifest).unwrap(), 1),
            Err(Stale::ManifestChanged)
        );
        changed = record.clone();
        changed.authentication_approved = Some("different-account-contract".into());
        assert_ne!(
            digest,
            native_call_fingerprint(&changed, &manifest).unwrap()
        );
        changed = record;
        changed.artifact_sha256 = Some("replacement-artifact".into());
        assert_ne!(
            digest,
            native_call_fingerprint(&changed, &manifest).unwrap()
        );
    }

    #[test]
    fn dev_only_record_disappears_on_unload() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("project"))
            .unwrap();

        let expected = dir.path().join("project");
        assert_eq!(
            reg.dev_path("com.x.dev").as_deref(),
            Some(expected.as_path())
        );
        assert!(!reg.dev_overrides_installed("com.x.dev"));
        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert!(!reg.is_installed("com.x.dev"));
    }

    #[test]
    fn dev_override_restores_settings_with_a_fresh_execution_identity() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut installed = pack("com.x.dev", &[]);
        installed.enabled = true;
        installed.toggle_seq = 7;
        installed.granted = vec!["storage".into()];
        reg.install(installed).unwrap();
        let original = reg.record("com.x.dev").unwrap();

        let mut dev = pack("com.x.dev", &[]);
        dev.installed_version = "dev-2".into();
        reg.load_dev(dev, dir.path().join("project")).unwrap();
        assert!(reg.dev_overrides_installed("com.x.dev"));
        assert_eq!(reg.record("com.x.dev").unwrap().installed_version, "dev-2");

        assert!(reg.unload_dev("com.x.dev").unwrap());
        let restored = reg.record("com.x.dev").unwrap();
        assert!(restored.enabled);
        assert_eq!(restored.toggle_seq, 7);
        assert_eq!(restored.granted, vec!["storage"]);
        assert!(restored.dev.is_none());
        assert_ne!(restored.execution_generation, original.execution_generation);
        assert_eq!(
            serde_json::to_value(&restored).unwrap(),
            serde_json::to_value(&original).unwrap(),
            "persistent settings and toggle order survive restoration"
        );
    }

    #[test]
    fn identical_replacements_and_restoration_never_resurrect_a_native_confirmation() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let manifest: grain_sdk::ExtensionManifest = serde_json::from_value(serde_json::json!({
            "id": "com.x.dev", "name": "Tools", "version": "1", "tier": "scripted",
            "entry_source": "grain.actions({hello: () => 'hello'});"
        }))
        .unwrap();
        let mut installed = pack("com.x.dev", &[]);
        installed.enabled = true;
        installed.toggle_seq = 7;
        reg.install(installed).unwrap();
        let original = reg.record("com.x.dev").unwrap();
        let mut identities = std::collections::HashSet::new();
        let mut observe = || {
            let record = reg.record("com.x.dev").unwrap();
            assert_eq!(record.toggle_seq, 7, "identity does not reorder settings");
            let digest = native_call_fingerprint(&record, &manifest).unwrap();
            assert!(
                identities.insert(digest),
                "a prior confirmation cannot become current again"
            );
        };
        observe();
        reg.install(original.clone()).unwrap();
        observe();
        reg.load_dev(original.clone(), dir.path().join("one"))
            .unwrap();
        observe();
        let active_generation = reg.record("com.x.dev").unwrap().execution_generation;
        reg.install(original.clone()).unwrap();
        assert_eq!(
            active_generation,
            reg.record("com.x.dev").unwrap().execution_generation,
            "updating only the parked installed copy leaves the dev instance current"
        );
        let active = reg.record("com.x.dev").unwrap();
        reg.install(active).unwrap();
        observe();
        reg.load_dev(original.clone(), dir.path().join("one"))
            .unwrap();
        observe();
        reg.unload_dev("com.x.dev").unwrap();
        observe();
        reg.uninstall("com.x.dev").unwrap();
        reg.install(original).unwrap();
        observe();
    }

    #[test]
    fn execution_generation_is_host_owned_and_registry_reload_renews_it() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut input = serde_json::to_value(pack("tools", &[])).unwrap();
        input["execution_generation"] = serde_json::json!(u64::MAX);
        let input: ExtensionRecord = serde_json::from_value(input).unwrap();
        assert_eq!(
            input.execution_generation, 0,
            "disk input cannot supply authority"
        );
        reg.install(input).unwrap();
        let first = reg.record("tools").unwrap();
        assert_ne!(first.execution_generation, 0);
        assert_ne!(first.execution_generation, u64::MAX);
        let mut forged = first.clone();
        forged.execution_generation = first.execution_generation;
        reg.install(forged).unwrap();
        let second = reg.record("tools").unwrap();
        assert_ne!(first.execution_generation, second.execution_generation);
        assert!(!fs::read_to_string(dir.path().join(EXTENSIONS_FILE))
            .unwrap()
            .contains("execution_generation"));
        let reloaded = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let third = reloaded.record("tools").unwrap();
        assert_ne!(second.execution_generation, third.execution_generation);
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(third).unwrap()
        );
    }

    #[test]
    fn resource_disable_refuses_stale_execution_generations() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("tools", &[])).unwrap();
        reg.set_enabled("tools", true).unwrap();
        let first = reg.record("tools").unwrap();
        reg.install(first.clone()).unwrap();
        assert!(!reg
            .disable_if_generation("tools", first.execution_generation)
            .unwrap());
        assert!(reg.record("tools").unwrap().enabled);
        let second = reg.record("tools").unwrap();
        reg.set_enabled("tools", false).unwrap();
        reg.set_enabled("tools", true).unwrap();
        assert!(!reg
            .disable_if_generation("tools", second.execution_generation)
            .unwrap());
        let third = reg.record("tools").unwrap();
        reg.load_dev(third.clone(), dir.path().join("project"))
            .unwrap();
        reg.unload_dev("tools").unwrap();
        assert!(!reg
            .disable_if_generation("tools", third.execution_generation)
            .unwrap());
        let current = reg.record("tools").unwrap();
        assert!(current.enabled);
        assert!(reg
            .disable_if_generation("tools", current.execution_generation)
            .unwrap());
        assert!(!reg.record("tools").unwrap().enabled);
        assert!(!reg
            .disable_if_generation("missing", current.execution_generation)
            .unwrap());
    }

    #[test]
    fn failed_enabled_replacement_preserves_owner_but_failed_disable_invalidates_it() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("tools", &[])).unwrap();
        reg.set_enabled("tools", true).unwrap();
        let before = reg.record("tools").unwrap();
        // Make atomic rename fail without touching anything outside this fixture.
        let path = dir.path().join(EXTENSIONS_FILE);
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(reg.install(before.clone()).is_err());
        let replacement = reg.record("tools").unwrap();
        assert_eq!(
            before.execution_generation,
            replacement.execution_generation
        );
        assert!(reg
            .disable_if_generation("tools", replacement.execution_generation)
            .is_err());
        let disabled = reg.record("tools").unwrap();
        assert!(!disabled.enabled);
        assert_ne!(
            replacement.execution_generation,
            disabled.execution_generation
        );
    }

    #[test]
    fn replacing_a_dev_path_does_not_nest_or_lose_the_installed_backup() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut installed = pack("com.x.dev", &[]);
        installed.installed_version = "store".into();
        reg.install(installed).unwrap();

        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("one"))
            .unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("two"))
            .unwrap();
        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert_eq!(reg.record("com.x.dev").unwrap().installed_version, "store");
    }

    #[test]
    fn replacing_a_dev_only_path_still_disappears_on_unload() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("one"))
            .unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("two"))
            .unwrap();

        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert!(!reg.is_installed("com.x.dev"));
    }

    #[test]
    fn uninstall_during_dev_override_removes_only_installed_copy() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut installed = pack("com.x.dev", &[]);
        installed.installed_version = "store".into();
        reg.install(installed).unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("project"))
            .unwrap();

        assert!(reg.uninstall("com.x.dev").unwrap());
        assert!(reg.dev_path("com.x.dev").is_some());
        assert!(!reg.dev_overrides_installed("com.x.dev"));
        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert!(!reg.is_installed("com.x.dev"));
    }

    #[test]
    fn updating_effective_dev_record_preserves_its_installed_backup() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut installed = pack("com.x.dev", &[]);
        installed.installed_version = "store".into();
        reg.install(installed).unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("project"))
            .unwrap();

        let mut active = reg.record("com.x.dev").unwrap();
        active.granted.push("storage".into());
        reg.install(active).unwrap();
        assert_eq!(reg.record("com.x.dev").unwrap().granted, vec!["storage"]);

        reg.unload_dev("com.x.dev").unwrap();
        assert_eq!(reg.record("com.x.dev").unwrap().installed_version, "store");
    }

    #[test]
    fn core_defaults_occupy_every_known_slot() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        for slot in grain_sdk::manifest::KNOWN_SLOTS {
            assert_eq!(
                reg.slot_occupant(slot).as_deref(),
                Some(CORE_DEFAULT),
                "slot '{slot}' must not start free"
            );
        }
        assert_eq!(reg.slot_occupant("pill.theme"), None);
    }

    #[test]
    fn claim_conflicts_then_takeover_then_release() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("a", &["output.destination"])).unwrap();
        reg.install(pack("b", &["output.destination"])).unwrap();

        assert_eq!(
            reg.slot_conflict("a"),
            Some(SlotConflict {
                slot: "output.destination".into(),
                current_occupant: CORE_DEFAULT.into(),
            })
        );
        assert!(reg.set_enabled("a", true).is_err(), "no silent steal");

        // Takeover from core displaces nobody, then enabling succeeds.
        assert_eq!(reg.take_slot("a", "output.destination").unwrap(), None);
        assert!(reg.set_enabled("a", true).unwrap());
        assert_eq!(
            reg.slot_occupant("output.destination").as_deref(),
            Some("a")
        );

        // A second claimant sees the real occupant, not the core default.
        assert_eq!(
            reg.slot_conflict("b").unwrap().current_occupant,
            "a".to_string()
        );
        // Takeover disables the incumbent in the same transaction: SPEC §3.2
        // has no state where two enabled extensions both own a slot.
        assert_eq!(
            reg.take_slot("b", "output.destination").unwrap().as_deref(),
            Some("a")
        );
        assert!(!reg.is_enabled("a"));
        assert!(reg.set_enabled("b", true).unwrap());

        // Disable releases back to Grain's default — never to the loser.
        reg.set_enabled("b", false).unwrap();
        assert_eq!(
            reg.slot_occupant("output.destination").as_deref(),
            Some(CORE_DEFAULT)
        );
        assert!(reg.slots_held("b").is_empty());
    }

    #[test]
    fn taking_an_undeclared_slot_is_refused() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("a", &["output.destination"])).unwrap();
        assert!(reg.take_slot("a", "prompt.main").is_err());
        assert!(reg.take_slot("ghost", "output.destination").is_err());
        assert_eq!(
            reg.slot_occupant("output.destination").as_deref(),
            Some(CORE_DEFAULT)
        );
    }

    #[test]
    fn an_update_that_drops_a_slot_releases_it() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("a", &["output.destination"])).unwrap();
        reg.take_slot("a", "output.destination").unwrap();
        reg.set_enabled("a", true).unwrap();

        let mut v2 = pack("a", &["prompt.main"]);
        v2.enabled = true;
        reg.install(v2).unwrap();
        assert_eq!(
            reg.slot_occupant("output.destination").as_deref(),
            Some(CORE_DEFAULT),
            "a dropped slot must not stay held forever"
        );
        // The newly declared slot is not auto-granted — it needs a takeover.
        assert_eq!(
            reg.slot_occupant("prompt.main").as_deref(),
            Some(CORE_DEFAULT)
        );
        assert!(reg.slot_conflict("a").is_some());
    }

    #[test]
    fn uninstall_releases_slots() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("a", &["output.destination"])).unwrap();
        reg.take_slot("a", "output.destination").unwrap();
        assert_eq!(reg.slots_held("a").len(), 1);

        reg.uninstall("a").unwrap();
        assert_eq!(
            reg.slot_occupant("output.destination").as_deref(),
            Some(CORE_DEFAULT)
        );
    }

    #[test]
    fn slot_claims_survive_a_reload() {
        let dir = tmp();
        {
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            reg.install(pack("a", &["output.destination"])).unwrap();
            reg.take_slot("a", "output.destination").unwrap();
            reg.set_enabled("a", true).unwrap();
        }
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert_eq!(
            reg.slot_occupant("output.destination").as_deref(),
            Some("a")
        );
        assert_eq!(reg.slots_held("a"), vec!["output.destination".to_string()]);
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = tmp();
        {
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            let mut rec = pack("x", &[]);
            rec.installed_version = "2.0".into();
            reg.install(rec).unwrap();
            reg.set_enabled("x", true).unwrap();
        }
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(reg.is_enabled("x"));
        assert_eq!(reg.record("x").unwrap().installed_version, "2.0");
        // Damaged state remains recoverable and cannot silently initialize.
        let path = dir.path().join(EXTENSIONS_FILE);
        fs::write(&path, "{not json").unwrap();
        assert!(ExtensionsRegistry::load(dir.path(), false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{not json");
    }

    #[test]
    fn registry_load_refuses_future_versions_and_inconsistent_identity_without_writes() {
        let dir = tmp();
        let path = dir.path().join(EXTENSIONS_FILE);
        for value in [
            serde_json::json!({"tool_only_migration_version":2}),
            serde_json::json!({"records":{"one":{"id":"other","enabled":true}}}),
            serde_json::json!({"records":[]}),
        ] {
            let bytes = serde_json::to_vec(&value).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert!(ExtensionsRegistry::load(dir.path(), false).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
        fs::remove_file(&path).unwrap();
        let fresh = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(fresh.records().is_empty());
    }

    #[test]
    fn oversized_registry_and_encoding_refuse_before_replacing_state() {
        let dir = tmp();
        let path = dir.path().join(EXTENSIONS_FILE);
        let file = fs::File::create(&path).unwrap();
        file.set_len(REGISTRY_MAX_BYTES as u64 + 1).unwrap();
        drop(file);
        assert!(ExtensionsRegistry::load(dir.path(), false).is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            REGISTRY_MAX_BYTES as u64 + 1
        );
        fs::remove_file(&path).unwrap();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let original = fs::read(&path).unwrap();
        let mut oversized = RegistryFile::default();
        oversized
            .quarantined
            .insert("large".into(), "x".repeat(REGISTRY_MAX_BYTES));
        assert!(persist_registry(&path, &oversized).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(reg.records().is_empty());
    }

    #[test]
    fn failed_owned_atomic_writer_preserves_destination_and_unowned_pending_files() {
        use std::io::Write;
        let dir = tmp();
        let path = dir.path().join(EXTENSIONS_FILE);
        fs::write(&path, b"original").unwrap();
        let orphan = path.with_extension(format!("{}.0.pending", std::process::id()));
        fs::write(&orphan, b"recoverable previous process bytes").unwrap();
        assert!(atomic_write_with(&path, |file| {
            file.write_all(b"partial new state")?;
            anyhow::bail!("injected write failure")
        })
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(
            fs::read(&orphan).unwrap(),
            b"recoverable previous process bytes"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        atomic_write(&path, b"complete new state").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"complete new state");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn atomic_publication_recovers_after_a_short_windows_delete_lock() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tmp();
        let path = dir.path().join(EXTENSIONS_FILE);
        fs::write(&path, b"original").unwrap();
        // A real Windows reader that permits reads/writes but denies rename.
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(80));
            drop(held);
        });
        let writes = std::cell::Cell::new(0);
        let saved = atomic_write_with(&path, |file| {
            use std::io::Write;
            writes.set(writes.get() + 1);
            file.write_all(b"complete new state")?;
            Ok(())
        });
        release.join().unwrap();
        saved.unwrap();
        assert_eq!(writes.get(), 1, "publication must not replay the writer");
        assert_eq!(fs::read(&path).unwrap(), b"complete new state");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn atomic_publication_refuses_a_persistent_windows_lock_and_preserves_state() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tmp();
        let path = dir.path().join(EXTENSIONS_FILE);
        fs::write(&path, b"original").unwrap();
        let orphan = dir.path().join("unowned.pending");
        fs::write(&orphan, b"unowned recovery bytes").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        let start = std::time::Instant::now();
        let error = atomic_write(&path, b"new state").unwrap_err();
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
        let io = error.downcast_ref::<std::io::Error>().unwrap();
        assert!(matches!(io.raw_os_error(), Some(5 | 32 | 33)));
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read(&orphan).unwrap(), b"unowned recovery bytes");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        drop(held);
        atomic_write(&path, b"fresh state").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"fresh state");
    }

    #[test]
    fn failed_native_account_switch_preserves_old_pointer_generation_and_restart() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut record = pack("account", &[]);
        record.enabled = true;
        record.granted = vec!["auth".into()];
        record.authentication_session = Some("a".repeat(32));
        reg.install(record).unwrap();
        let revision = reg.record_revision("account");
        let before = reg.record("account").unwrap();
        let original = fs::read(&reg.path).unwrap();
        fs::remove_file(&reg.path).unwrap();
        fs::create_dir(&reg.path).unwrap();
        assert!(reg
            .set_authentication_session_if_current(
                "account",
                revision,
                Some("b".repeat(32)),
                || Some(())
            )
            .is_err());
        assert_eq!(reg.record_revision("account"), revision);
        assert_eq!(
            reg.record("account").unwrap().authentication_session,
            before.authentication_session
        );
        assert_eq!(
            reg.record("account").unwrap().execution_generation,
            before.execution_generation
        );
        fs::remove_dir(&reg.path).unwrap();
        fs::write(&reg.path, original).unwrap();
        assert_eq!(
            ExtensionsRegistry::load(dir.path(), false)
                .unwrap()
                .record("account")
                .unwrap()
                .authentication_session,
            Some("a".repeat(32))
        );
        reg.set_authentication_session_if_current(
            "account",
            revision,
            Some("b".repeat(32)),
            || Some(()),
        )
        .unwrap();
        assert_eq!(
            ExtensionsRegistry::load(dir.path(), false)
                .unwrap()
                .record("account")
                .unwrap()
                .authentication_session,
            Some("b".repeat(32))
        );
    }

    fn block_registry_save(reg: &ExtensionsRegistry) -> Vec<u8> {
        let saved = fs::read(&reg.path).unwrap();
        fs::remove_file(&reg.path).unwrap();
        fs::create_dir(&reg.path).unwrap();
        saved
    }

    fn restore_registry_file(reg: &ExtensionsRegistry, saved: &[u8]) {
        fs::remove_dir(&reg.path).unwrap();
        fs::write(&reg.path, saved).unwrap();
    }

    #[test]
    fn failed_enable_and_grant_restore_record_and_toggle_order_before_retry() {
        for grant in [false, true] {
            let dir = tmp();
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            reg.install(pack("other", &[])).unwrap();
            reg.set_enabled("other", true).unwrap();
            let record = pack("tools", &[]);
            reg.install(record).unwrap();
            let revision = reg.record_revision("tools");
            let before = serde_json::to_value(reg.record("tools")).unwrap();
            let other = serde_json::to_value(reg.record("other")).unwrap();
            let next = reg.state.read().unwrap().next_toggle_seq;
            let saved = block_registry_save(&reg);
            let activate = || {
                if grant {
                    let mut approved = reg.record("tools").unwrap();
                    approved.granted = vec!["auth".into()];
                    approved.authentication_approved = Some("reviewed-declaration".into());
                    reg.approve_and_enable_if_current(approved, revision)
                        .map(|_| true)
                } else {
                    reg.set_enabled_at_revision("tools", true, revision)
                }
            };
            assert!(activate().is_err());
            assert_eq!(reg.record_revision("tools"), revision);
            assert_eq!(serde_json::to_value(reg.record("tools")).unwrap(), before);
            assert_eq!(serde_json::to_value(reg.record("other")).unwrap(), other);
            assert_eq!(reg.state.read().unwrap().next_toggle_seq, next);
            assert_eq!(
                reg.slot_occupant("output.destination").as_deref(),
                Some(CORE_DEFAULT)
            );
            restore_registry_file(&reg, &saved);
            let restarted = ExtensionsRegistry::load(dir.path(), false).unwrap();
            assert!(!restarted.is_enabled("tools"));
            assert!(restarted.record("tools").unwrap().granted.is_empty());
            assert!(activate().unwrap());
            assert!(reg.is_enabled("tools"));
            assert_ne!(reg.record_revision("tools"), revision);
            let restarted = ExtensionsRegistry::load(dir.path(), false).unwrap();
            assert!(restarted.is_enabled("tools"));
            if grant {
                assert_eq!(restarted.record("tools").unwrap().granted, vec!["auth"]);
            }
        }
    }

    #[test]
    fn failed_enabled_install_restores_absence_parked_owner_and_quarantine() {
        for mode in ["new", "installed", "parked", "quarantined", "oversized"] {
            let dir = tmp();
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            if mode != "new" {
                let mut installed = pack("tools", &[]);
                installed.enabled = true;
                installed.installed_version = "committed".into();
                installed.authentication_session = Some("a".repeat(32));
                reg.install(installed).unwrap();
            }
            if mode == "parked" {
                reg.load_dev(pack("tools", &[]), dir.path().join("project"))
                    .unwrap();
            }
            if mode == "quarantined" {
                reg.quarantine("tools", "retired capabilities").unwrap();
            }
            let revision = reg.record_revision("tools");
            let before = serde_json::to_value(reg.record("tools")).unwrap();
            let quarantine = reg.quarantine_reason("tools");
            let saved = fs::read(&reg.path).unwrap();
            if mode != "oversized" {
                block_registry_save(&reg);
            }
            let mut candidate = pack("tools", &[]);
            candidate.enabled = true;
            candidate.installed_version = if mode == "oversized" {
                "x".repeat(REGISTRY_MAX_BYTES)
            } else {
                "candidate".into()
            };
            candidate.granted = vec!["auth".into()];
            candidate.authentication_session = Some("b".repeat(32));
            assert!(
                reg.install_if_current(candidate, revision).is_err(),
                "{mode}"
            );
            assert_eq!(reg.record_revision("tools"), revision, "{mode}");
            assert_eq!(
                serde_json::to_value(reg.record("tools")).unwrap(),
                before,
                "{mode}"
            );
            assert_eq!(reg.quarantine_reason("tools"), quarantine, "{mode}");
            if mode == "oversized" {
                assert_eq!(fs::read(&reg.path).unwrap(), saved);
                assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
            } else {
                restore_registry_file(&reg, &saved);
            }
            let restarted = ExtensionsRegistry::load(dir.path(), false).unwrap();
            assert_eq!(
                serde_json::to_value(restarted.record("tools")).unwrap(),
                before,
                "{mode}"
            );
            assert_eq!(restarted.quarantine_reason("tools"), quarantine);
        }
    }

    #[test]
    fn failed_dev_switch_and_restore_keep_separate_account_owners_and_restart_state() {
        for unload in [false, true] {
            let dir = tmp();
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            let mut installed = pack("tools", &["output.destination"]);
            installed.enabled = true;
            installed.granted = vec!["auth".into()];
            installed.authentication_session = Some("a".repeat(32));
            reg.install(installed).unwrap();
            reg.take_slot("tools", "output.destination").unwrap();
            let mut dev = pack("tools", &[]);
            dev.enabled = true;
            dev.granted = vec!["auth".into()];
            reg.load_dev(dev.clone(), dir.path().join("project-a"))
                .unwrap();
            reg.set_authentication_session_if_current(
                "tools",
                reg.record_revision("tools"),
                Some("b".repeat(32)),
                || Some(()),
            )
            .unwrap();
            reg.set_slot_claim("legacy-dev-claim", "tools").unwrap();
            let before = serde_json::to_value(reg.record("tools")).unwrap();
            let revision = reg.record_revision("tools");
            let epoch = reg.state.read().unwrap().removal_epoch;
            let saved = block_registry_save(&reg);
            if unload {
                assert!(reg.unload_dev("tools").is_err());
            } else {
                assert!(reg
                    .load_dev_if_current(dev.clone(), dir.path().join("project-b"), revision)
                    .is_err());
            }
            assert_eq!(reg.record_revision("tools"), revision);
            assert_eq!(reg.state.read().unwrap().removal_epoch, epoch);
            assert_eq!(serde_json::to_value(reg.record("tools")).unwrap(), before);
            assert_eq!(
                reg.slot_occupant("legacy-dev-claim").as_deref(),
                Some("tools")
            );
            assert_eq!(
                reg.slot_occupant("output.destination").as_deref(),
                Some(CORE_DEFAULT)
            );
            restore_registry_file(&reg, &saved);
            let restarted = ExtensionsRegistry::load(dir.path(), false).unwrap();
            assert_eq!(
                serde_json::to_value(restarted.record("tools")).unwrap(),
                before
            );
            if unload {
                assert!(reg.unload_dev("tools").unwrap());
                assert_eq!(
                    reg.record("tools").unwrap().authentication_session,
                    Some("a".repeat(32))
                );
                assert_eq!(
                    reg.slot_occupant("output.destination").as_deref(),
                    Some("tools")
                );
            } else {
                reg.load_dev_if_current(dev, dir.path().join("project-b"), revision)
                    .unwrap();
                assert!(reg
                    .record("tools")
                    .unwrap()
                    .authentication_session
                    .is_none());
                assert_eq!(
                    reg.installed_record("tools")
                        .unwrap()
                        .authentication_session,
                    Some("a".repeat(32))
                );
            }
            assert!(reg.slot_occupant("legacy-dev-claim").is_none());
            assert_ne!(reg.record_revision("tools"), revision);
        }
    }

    #[test]
    fn concurrent_approval_and_disable_finish_disabled_without_stale_publication() {
        for _ in 0..32 {
            let dir = tmp();
            let reg = std::sync::Arc::new(ExtensionsRegistry::load(dir.path(), false).unwrap());
            reg.install(pack("tools", &[])).unwrap();
            let revision = reg.record_revision("tools");
            let mut approved = reg.record("tools").unwrap();
            approved.granted = vec!["auth".into()];
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
            let approving = {
                let reg = reg.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    reg.approve_and_enable_if_current(approved, revision)
                })
            };
            let disabling = {
                let reg = reg.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    reg.set_enabled("tools", false)
                })
            };
            barrier.wait();
            let _ = approving.join().unwrap();
            assert!(disabling.join().unwrap().unwrap());
            assert!(!reg.is_enabled("tools"));
            assert!(reg
                .set_enabled_at_revision("tools", true, revision)
                .is_err());
            assert!(!ExtensionsRegistry::load(dir.path(), false)
                .unwrap()
                .is_enabled("tools"));
        }
    }

    #[test]
    fn enable_counter_exhaustion_changes_no_identity_or_claim() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("tools", &[])).unwrap();
        reg.state.write().unwrap().next_toggle_seq = u64::MAX;
        let revision = reg.record_revision("tools");
        assert!(reg
            .set_enabled("tools", true)
            .unwrap_err()
            .to_string()
            .contains("toggle sequence exhausted"));
        assert_eq!(reg.record_revision("tools"), revision);
        assert!(!reg.is_enabled("tools"));
        assert!(reg.slots_held("tools").is_empty());
    }
}

// ── Tier-A pack application (Phase 1: prompt packs) ─────────────────────────

/// Apply a prompt pack to settings: entries land in the user's prompt list
/// under `ext:<extid>:<id>` (SPEC #15 — namespaced, so collisions with user
/// prompts or other packs are unrepresentable). Idempotent: re-applying the
/// same pack replaces its own entries.
pub fn apply_prompt_pack(
    _settings: &mut crate::settings::AppSettings,
    _ext_id: &str,
    _entries: &[grain_sdk::PromptPackEntry],
) {
    // Deserialization/migration compatibility only. No caller, including an
    // old import/restore command, may activate a retired prompt payload.
}

/// Detach legacy extension-owned prompt state after the host archives it.
/// First-party and user-authored entries without the reserved prefix survive.
pub fn retire_extension_prompts(settings: &mut crate::settings::AppSettings) {
    settings
        .post_process_prompts
        .retain(|prompt| !prompt.id.starts_with("ext:"));
    if settings
        .post_process_selected_prompt_id
        .as_ref()
        .is_some_and(|id| id.starts_with("ext:"))
    {
        settings.post_process_selected_prompt_id = settings
            .post_process_prompts
            .first()
            .map(|prompt| prompt.id.clone());
    }
}

/// Archive before detaching prompt state. Existing edited entries are never
/// overwritten; rerunning after interruption deduplicates exact records.
pub fn archive_retired_prompts(
    data_dir: &Path,
    settings: &crate::settings::AppSettings,
) -> Result<()> {
    let prompts: Vec<_> = settings
        .post_process_prompts
        .iter()
        .filter(|p| p.id.starts_with("ext:"))
        .collect();
    if prompts.is_empty() {
        return Ok(());
    }
    let path = data_dir.join("retired-extension-prompts.json");
    let mut preserved: Vec<crate::settings::LLMPrompt> = if path.exists() {
        if fs::metadata(&path)?.len() > 8 * 1024 * 1024 {
            anyhow::bail!("retired prompt archive exceeds 8 MiB");
        }
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        Vec::new()
    };
    for prompt in prompts {
        if !preserved.iter().any(|old| {
            old.id == prompt.id && old.name == prompt.name && old.prompt == prompt.prompt
        }) {
            preserved.push(prompt.clone());
        }
    }
    let bytes = serde_json::to_vec_pretty(&preserved)?;
    if bytes.len() > 8 * 1024 * 1024 {
        anyhow::bail!("retired prompt archive exceeds 8 MiB");
    }
    atomic_write(&path, &bytes)
}

/// Preserve custom chords before detaching the extension binding namespace.
/// Archives are inert and deduplicate exact key/value pairs on interrupted restart.
pub fn archive_retired_bindings(
    data_dir: &Path,
    settings: &crate::settings::AppSettings,
) -> Result<()> {
    let retired: Vec<serde_json::Value> = settings
        .bindings
        .iter()
        .filter(|(key, _)| key.starts_with("ext:"))
        .map(|(key, binding)| serde_json::to_value((key, binding)))
        .collect::<std::result::Result<_, _>>()?;
    if retired.is_empty() {
        return Ok(());
    }
    let path = data_dir.join("retired-extension-bindings.json");
    let mut preserved: Vec<serde_json::Value> = if path.exists() {
        if fs::metadata(&path)?.len() > 8 * 1024 * 1024 {
            anyhow::bail!("retired binding archive exceeds 8 MiB");
        }
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        Vec::new()
    };
    for value in retired {
        if !preserved.contains(&value) {
            preserved.push(value);
        }
    }
    let bytes = serde_json::to_vec_pretty(&preserved)?;
    if bytes.len() > 8 * 1024 * 1024 {
        anyhow::bail!("retired binding archive exceeds 8 MiB");
    }
    atomic_write(&path, &bytes)
}

pub fn retire_extension_bindings(settings: &mut crate::settings::AppSettings) {
    settings.bindings.retain(|id, _| !id.starts_with("ext:"));
}

/// Refuse damaged state rather than silently discarding installed metadata.
/// The app's existing load-error path leaves extensions unavailable while core
/// Grain continues. Only a genuinely absent file initializes a fresh registry.
fn read_registry(path: &Path) -> Result<RegistryFile> {
    use std::io::Read;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A dangling link is existing, unresolved state, not a fresh install.
            // Preserve it rather than replacing the link with default metadata.
            return match fs::symlink_metadata(path) {
                Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => {
                    Ok(RegistryFile::default())
                }
                _ => Err(error).with_context(|| {
                    format!(
                        "unresolved {}; registry preserved, extensions unavailable",
                        path.display()
                    )
                }),
            };
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "read {}; registry preserved, extensions unavailable",
                    path.display()
                )
            })
        }
    };
    if file.metadata()?.len() > REGISTRY_MAX_BYTES as u64 {
        anyhow::bail!(
            "extension registry exceeds 8 MiB; registry preserved, extensions unavailable"
        );
    }
    let mut raw = Vec::new();
    file.take(REGISTRY_MAX_BYTES as u64 + 1)
        .read_to_end(&mut raw)?;
    if raw.len() > REGISTRY_MAX_BYTES {
        anyhow::bail!(
            "extension registry exceeds 8 MiB; registry preserved, extensions unavailable"
        );
    }
    serde_json::from_slice(&raw).with_context(|| {
        format!(
            "invalid {}; registry preserved, extensions unavailable",
            path.display()
        )
    })
}

/// Stream JSON directly into the owned staging file, bounding encoded output
/// without a second full serialized registry allocation.
fn persist_registry(path: &Path, state: &RegistryFile) -> Result<()> {
    use std::io::Write;
    atomic_write_with(path, |file| {
        let mut writer = RegistryWriter {
            file: std::io::BufWriter::new(file),
            remaining: REGISTRY_MAX_BYTES,
        };
        serde_json::to_writer_pretty(&mut writer, state)
            .context("encode extension registry (8 MiB limit)")?;
        writer.flush().context("flush extension registry")
    })
}

struct RegistryWriter<'a> {
    file: std::io::BufWriter<&'a mut fs::File>,
    remaining: usize,
}

impl std::io::Write for RegistryWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(std::io::Error::other("extension registry exceeds 8 MiB"));
        }
        let written = std::io::Write::write(&mut self.file, bytes)?;
        self.remaining -= written;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut self.file)
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    atomic_write_with(path, |file| {
        file.write_all(bytes).context("write extension state")
    })
}

fn atomic_write_with(path: &Path, write: impl FnOnce(&mut fs::File) -> Result<()>) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::Builder::new()
        .prefix(".grain-extension-")
        .suffix(".pending")
        .tempfile_in(parent)
        .with_context(|| format!("stage {}", path.display()))?;
    write(file.as_file_mut())?;
    file.as_file()
        .sync_all()
        .with_context(|| format!("sync {}", path.display()))?;
    // Keep the same synced staging file across a short Windows sharing lock.
    // This retries only publication, never serialization, mutation or a tool.
    // Every terminal failure drops only our staging file; there is no delete/
    // copy fallback and no operation after successful atomic replacement.
    #[cfg(not(windows))]
    return file
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error)
        .with_context(|| format!("persist {}", path.display()));

    #[cfg(windows)]
    {
        let mut delays = [10, 20, 40, 80, 100].into_iter(); // 250ms total sleep ceiling
        loop {
            match file.persist(path) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    if matches!(error.error.raw_os_error(), Some(5 | 32 | 33)) {
                        if let Some(delay) = delays.next() {
                            file = error.file;
                            std::thread::sleep(std::time::Duration::from_millis(delay));
                            continue;
                        }
                    }
                    return Err(error.error).with_context(|| format!("persist {}", path.display()));
                }
            }
        }
    }
}

/// Remove a pack's prompts (disable/uninstall). If the removed pack's prompt
/// was the SELECTED one, selection falls back to the first remaining prompt —
/// never a dangling id (the §10.2 restore principle, applied to prompts).
pub fn remove_prompt_pack(settings: &mut crate::settings::AppSettings, ext_id: &str) {
    let prefix = format!("ext:{ext_id}:");
    settings
        .post_process_prompts
        .retain(|p| !p.id.starts_with(&prefix));
    if let Some(sel) = &settings.post_process_selected_prompt_id {
        if sel.starts_with(&prefix) {
            settings.post_process_selected_prompt_id =
                settings.post_process_prompts.first().map(|p| p.id.clone());
        }
    }
}

#[cfg(test)]
mod pack_tests {
    use super::*;
    use crate::settings::AppSettings;
    use grain_sdk::PromptPackEntry;

    #[test]
    fn retirement_archives_edited_prompts_once_and_heals_active_selection() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = AppSettings::default();
        let user = crate::settings::LLMPrompt {
            id: "user".into(),
            name: "User".into(),
            prompt: "My words".into(),
        };
        settings.post_process_prompts.push(user);
        settings
            .post_process_prompts
            .push(crate::settings::LLMPrompt {
                id: "ext:com.x.old:formal".into(),
                name: "Edited".into(),
                prompt: "User edited pack text".into(),
            });
        settings.post_process_selected_prompt_id = Some("ext:com.x.old:formal".into());
        archive_retired_prompts(dir.path(), &settings).unwrap();
        archive_retired_prompts(dir.path(), &settings).unwrap(); // restart before settings commit
        retire_extension_prompts(&mut settings);
        retire_extension_prompts(&mut settings);
        assert!(settings.post_process_prompts.iter().any(|p| p.id == "user"));
        assert!(!settings
            .post_process_prompts
            .iter()
            .any(|p| p.id.starts_with("ext:")));
        assert!(!settings
            .post_process_selected_prompt_id
            .as_deref()
            .unwrap_or("")
            .starts_with("ext:"));
        let archived: Vec<crate::settings::LLMPrompt> = serde_json::from_slice(
            &fs::read(dir.path().join("retired-extension-prompts.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].prompt, "User edited pack text");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    fn entries() -> Vec<PromptPackEntry> {
        vec![PromptPackEntry {
            id: "formal".into(),
            name: "Formal".into(),
            prompt: "Rewrite formally.".into(),
        }]
    }

    #[test]
    fn retired_apply_cannot_change_user_prompts() {
        let mut settings = AppSettings::default();
        let before = serde_json::to_value(&settings).unwrap();
        apply_prompt_pack(&mut settings, "com.x.zh", &entries());
        apply_prompt_pack(&mut settings, "com.x.zh", &entries());
        assert_eq!(serde_json::to_value(settings).unwrap(), before);
    }

    #[test]
    fn remove_clears_entries_and_heals_selection() {
        let mut s = AppSettings::default();
        apply_prompt_pack(&mut s, "com.x.zh", &entries());
        s.post_process_selected_prompt_id = Some("ext:com.x.zh:formal".into());
        remove_prompt_pack(&mut s, "com.x.zh");
        assert!(!s
            .post_process_prompts
            .iter()
            .any(|p| p.id.starts_with("ext:com.x.zh:")));
        // Selection healed to a real prompt, not left dangling.
        let sel = s.post_process_selected_prompt_id.clone();
        assert!(
            sel.is_none()
                || s.post_process_prompts
                    .iter()
                    .any(|p| Some(&p.id) == sel.as_ref())
        );
        assert_ne!(sel.as_deref(), Some("ext:com.x.zh:formal"));
    }
}
