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
//!   new; the implementation stays where it is.
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
    /// Fingerprint of the actions the user approved, from
    /// [`actions_fingerprint`]. Changed actions require renewed approval.
    /// `None` means no actions have been approved.
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
    /// A load-unpacked project currently overriding this id. The effective
    /// record stays at the normal map key, so every capability/lifecycle
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
/// Uses length-prefixed SHA-256 framing: the input is attacker-controlled,
/// so the hash must resist deliberately constructed collisions.
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    /// Runtime-only removal epoch. Vacant-id writes conservatively fail after
    /// any removal, without retaining a tombstone for every historical id.
    #[serde(skip)]
    removal_epoch: u64,
    /// Persistent refusal reasons. A restart or old grant must not reactivate
    /// an incompatible package; a validated replacement clears quarantine.
    #[serde(default)]
    quarantined: HashMap<String, String>,
    /// Installed pack records, keyed by extension id.
    #[serde(default)]
    records: HashMap<String, ExtensionRecord>,
    #[serde(default)]
    next_toggle_seq: u64,
}

pub struct ExtensionsRegistry {
    path: PathBuf,
    state: RwLock<RegistryFile>,
    save_gate: std::sync::Mutex<()>,
}

/// Only the edited identity is copied for rollback; other
/// records and artifact contents are never duplicated. Readers are excluded
/// until persistence succeeds or this snapshot has been restored.
struct RecordEditSnapshot {
    record: Option<ExtensionRecord>,
    quarantine: Option<String>,
    next_toggle_seq: u64,
    removal_epoch: u64,
}

impl RecordEditSnapshot {
    fn capture(state: &RegistryFile, id: &str) -> Self {
        Self {
            record: state.records.get(id).cloned(),
            quarantine: state.quarantined.get(id).cloned(),
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
    pub fn quarantine_reason(&self, id: &str) -> Option<String> {
        self.state.read().unwrap().quarantined.get(id).cloned()
    }

    /// Disable before any cleanup, preserve the artifact/user data, and make
    /// old grants and parked developer records inert. Safe to repeat on boot.
    pub fn quarantine(&self, id: &str, reason: &str) -> Result<()> {
        fn retire(record: &mut ExtensionRecord) {
            record.enabled = false;
            record.granted.clear();
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
            }
        }
        self.save()
    }

    /// Load (or initialize) the registry.
    pub fn load(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join(EXTENSIONS_FILE);
        let mut state = read_registry(&path)?;
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
        reg.save()?;
        Ok(reg)
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

    /// Enable/disable an installed pack. Enabling assigns the next toggle
    /// sequence, so re-enabling moves it to the end of toggle order.
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
            }
            let next = state.next_toggle_seq;
            let activating = enabled && state.records.get(id).is_some_and(|record| !record.enabled);
            let following = if activating {
                Some(next.checked_add(1).context("toggle sequence exhausted")?)
            } else {
                None
            };
            let rollback = activating.then(|| RecordEditSnapshot::capture(&state, id));
            let Some(_owner) = owner() else {
                return Ok(false);
            };
            let changed = match state.records.get_mut(id) {
                Some(rec) if rec.enabled != enabled || !enabled => {
                    rec.execution_generation = next_execution_generation()?;
                    rec.enabled = enabled;
                    if enabled {
                        rec.toggle_seq = next;
                    }
                    true
                }
                _ => false,
            };
            if changed {
                if enabled {
                    state.next_toggle_seq = following.expect("checked toggle sequence");
                }
                Some(rollback)
            } else {
                None
            }
        };
        if let Some(rollback) = changed {
            self.persist_record_edit(&mut state, id, rollback)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Install or replace a pack record.
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
    /// may use this to activate a quarantined package.
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
                .then(|| RecordEditSnapshot::capture(&state, &record.id));
            if approve {
                if !state.records.contains_key(&record.id) {
                    anyhow::bail!("Extension is not installed.");
                }
                if let Some(reason) = state.quarantined.get(&record.id) {
                    anyhow::bail!("extension is quarantined: {reason}");
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
            state.records.insert(id, record);
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
            rollback = RecordEditSnapshot::capture(&state, &record.id);
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
            record.dev = Some(DevOverride { path, replaced });
            state.records.insert(id, record);
        }
        self.persist_record_edit(&mut state, &record_id, Some(rollback))
            .map(|_| generation)
    }

    /// Remove a load-unpacked override and restore its parked installed record,
    /// if any, with a fresh execution identity.
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
            rollback = RecordEditSnapshot::capture(&state, id);
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

            if let Some(mut replaced) = dev.replaced.map(|record| *record) {
                replaced.execution_generation = generation;
                state.records.insert(id.to_string(), replaced);
            } else {
                // No owner remains. Its refusal must not poison a later,
                // independently validated installation/development project.
                state.quarantined.remove(id);
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
        let _save = self.save_gate.lock().unwrap();
        let mut state = self.state.write().unwrap();
        let rollback = RecordEditSnapshot::capture(&state, id);
        let removed = {
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
                    state.quarantined.remove(id);
                }
                removed
            }
        };
        if removed.is_some() {
            self.persist_record_edit(&mut state, id, Some(rollback))?;
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        reg.install(pack("tools")).unwrap();
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
            let reg = ExtensionsRegistry::load(dir.path()).unwrap();
            reg.install(pack("tools")).unwrap();
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let before = reg.record_revision("tools");
        reg.install(pack("tools")).unwrap();
        reg.uninstall("tools").unwrap();
        assert!(reg.install_if_current(pack("tools"), before).is_err());
        let before = reg.record_revision("tools");
        reg.uninstall("tools").unwrap(); // removal intent also defeats a late first install
        assert!(reg.install_if_current(pack("tools"), before).is_err());
        let fresh = reg.record_revision("tools");
        reg.install_if_current(pack("tools"), fresh).unwrap();
        let unrelated = reg.record_revision("tools");
        reg.uninstall("other").unwrap();
        reg.with_current_record("tools", unrelated, |_| ()).unwrap();
        let persisted = fs::read_to_string(&reg.path).unwrap();
        assert!(!persisted.contains("removal_epoch"));
        assert!(!persisted.contains("execution_generation"));
        reg.load_dev(pack("dev_only"), dir.path().join("project"))
            .unwrap();
        let vacant_parked = reg.record_revision("dev_only");
        reg.uninstall("dev_only").unwrap();
        assert!(reg
            .install_if_current(pack("dev_only"), vacant_parked)
            .is_err());
        assert!(reg.record("dev_only").unwrap().dev.is_some());
        assert!(reg.installed_record("dev_only").is_none());
    }

    #[test]
    fn revision_includes_parked_updates_and_removal() {
        for remove in [false, true] {
            let dir = tmp();
            let reg = ExtensionsRegistry::load(dir.path()).unwrap();
            reg.install(pack("tools")).unwrap();
            reg.load_dev(pack("tools"), dir.path().join("project"))
                .unwrap();
            let before = reg.record_revision("tools");
            let active = reg.record("tools").unwrap();
            if remove {
                reg.uninstall("tools").unwrap();
            } else {
                reg.install(pack("tools")).unwrap();
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
        let reg = std::sync::Arc::new(ExtensionsRegistry::load(dir.path()).unwrap());
        reg.install(pack("tools")).unwrap();
        let revision = reg.record_revision("tools");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let threads: Vec<_> = (0..2)
            .map(|index| {
                let reg = reg.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut record = pack("tools");
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
        let loaded = ExtensionsRegistry::load(dir.path()).unwrap();
        assert_eq!(
            loaded.record("tools").unwrap().installed_version,
            reg.record("tools").unwrap().installed_version
        );
    }

    #[test]
    fn publication_holds_registry_ownership_until_worker_is_visible() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        reg.install(pack("tools")).unwrap();
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
    fn failed_save_keeps_new_revision_and_disabled_update() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        reg.install(pack("tools")).unwrap();
        reg.set_enabled("tools", true).unwrap();
        let revision = reg.record_revision("tools");
        let mut record = reg.record("tools").unwrap();
        record.enabled = false;
        fs::remove_file(&reg.path).unwrap();
        fs::create_dir(&reg.path).unwrap();
        assert!(reg.install_if_current(record, revision).is_err());
        assert!(!reg.is_enabled("tools"));
        assert!(reg.with_current_record("tools", revision, |_| ()).is_err());
    }

    #[test]
    fn quarantine_survives_restart_and_disables_parked_dev_state() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut legacy = pack("com.x.old");
        legacy.enabled = true;
        legacy.granted = vec!["capture:screen-image".into(), "resident".into()];
        reg.install(legacy.clone()).unwrap();
        reg.load_dev(legacy, dir.path().join("project")).unwrap();
        reg.quarantine("com.x.old", "retired context access")
            .unwrap();
        reg.quarantine("com.x.old", "retired context access")
            .unwrap();
        drop(reg);
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        assert!(!reg.is_enabled("com.x.old"));
        assert!(reg.record("com.x.old").unwrap().granted.is_empty());
        assert!(reg.set_enabled("com.x.old", true).is_err());
        reg.unload_dev("com.x.old").unwrap();
        assert!(!reg.is_enabled("com.x.old"));
        assert!(reg.set_enabled("com.x.old", true).is_err());
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn synchronous_admission_holds_read_lock_only_until_callback_returns() {
        let dir = tmp();
        let registry = ExtensionsRegistry::load(dir.path()).unwrap();
        registry.install(pack("tools")).unwrap();
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
    fn toggle_order_is_enable_order_and_reenable_moves_to_end() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        for id in ["a", "b"] {
            reg.install(pack(id)).unwrap();
        }
        reg.set_enabled("a", true).unwrap();
        reg.set_enabled("b", true).unwrap();
        assert!(
            reg.record("a").unwrap().toggle_seq < reg.record("b").unwrap().toggle_seq,
            "first enabled = top"
        );

        // Disable + re-enable moves to the end (SPEC §4.4).
        reg.set_enabled("a", false).unwrap();
        reg.set_enabled("a", true).unwrap();
        assert!(reg.record("a").unwrap().toggle_seq > reg.record("b").unwrap().toggle_seq);
    }

    fn pack(id: &str) -> ExtensionRecord {
        ExtensionRecord {
            id: id.into(),
            enabled: false,
            execution_generation: 0,
            toggle_seq: 0,
            installed_version: "1".into(),
            artifact_sha256: None,
            granted: vec![],
            actions_approved: None,
            authentication_approved: None,
            authentication_session: None,
            dev: None,
            trust: Trust::UNTRUSTED_DEFAULT,
        }
    }

    #[test]
    fn native_account_publication_is_revision_checked_and_persistent() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut record = pack("account");
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
        let restarted = ExtensionsRegistry::load(dir.path()).unwrap();
        assert_eq!(
            restarted.record("account").unwrap().authentication_session,
            Some(session)
        );
        let revision = restarted.record_revision("account");
        restarted
            .set_authentication_session_if_current("account", revision, None, || Some(()))
            .unwrap();
        assert!(ExtensionsRegistry::load(dir.path())
            .unwrap()
            .record("account")
            .unwrap()
            .authentication_session
            .is_none());
    }

    #[test]
    fn native_login_refuses_disabled_ungranted_and_invalid_sessions() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        reg.install(pack("account")).unwrap();
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut record = pack("account");
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut installed = pack("account");
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let manifest: grain_sdk::ExtensionManifest = serde_json::from_value(serde_json::json!({
            "id": "com.example.tools", "name": "Tools", "version": "1.0.0",
            "tier": "scripted", "entry_source": "grain.actions({write: () => 'hello'});",
            "contributes": {"actions": [{"id": "write", "title": "Test write", "risk": "confirm"}]}
        }))
        .unwrap();
        let mut installed = pack("com.example.tools");
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        reg.load_dev(pack("com.x.dev"), dir.path().join("project"))
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut installed = pack("com.x.dev");
        installed.enabled = true;
        installed.toggle_seq = 7;
        installed.granted = vec!["storage".into()];
        reg.install(installed).unwrap();
        let original = reg.record("com.x.dev").unwrap();

        let mut dev = pack("com.x.dev");
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let manifest: grain_sdk::ExtensionManifest = serde_json::from_value(serde_json::json!({
            "id": "com.x.dev", "name": "Tools", "version": "1", "tier": "scripted",
            "entry_source": "grain.actions({hello: () => 'hello'});"
        }))
        .unwrap();
        let mut installed = pack("com.x.dev");
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut input = serde_json::to_value(pack("tools")).unwrap();
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
        let reloaded = ExtensionsRegistry::load(dir.path()).unwrap();
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        reg.install(pack("tools")).unwrap();
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        reg.install(pack("tools")).unwrap();
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut installed = pack("com.x.dev");
        installed.installed_version = "store".into();
        reg.install(installed).unwrap();

        reg.load_dev(pack("com.x.dev"), dir.path().join("one"))
            .unwrap();
        reg.load_dev(pack("com.x.dev"), dir.path().join("two"))
            .unwrap();
        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert_eq!(reg.record("com.x.dev").unwrap().installed_version, "store");
    }

    #[test]
    fn replacing_a_dev_only_path_still_disappears_on_unload() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        reg.load_dev(pack("com.x.dev"), dir.path().join("one"))
            .unwrap();
        reg.load_dev(pack("com.x.dev"), dir.path().join("two"))
            .unwrap();

        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert!(!reg.is_installed("com.x.dev"));
    }

    #[test]
    fn uninstall_during_dev_override_removes_only_installed_copy() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut installed = pack("com.x.dev");
        installed.installed_version = "store".into();
        reg.install(installed).unwrap();
        reg.load_dev(pack("com.x.dev"), dir.path().join("project"))
            .unwrap();

        assert!(reg.uninstall("com.x.dev").unwrap());
        assert!(reg.dev_path("com.x.dev").is_some());
        assert!(!reg.dev_overrides_installed("com.x.dev"));
        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert!(!reg.is_installed("com.x.dev"));
    }

    #[test]
    fn final_owner_removal_clears_only_its_quarantine_and_allows_fresh_dev() {
        for parked in [false, true] {
            let dir = tmp();
            let reg = ExtensionsRegistry::load(dir.path()).unwrap();
            reg.install(pack("tools")).unwrap();
            reg.install(pack("other")).unwrap();
            reg.quarantine("other", "other owner's refusal").unwrap();
            if parked {
                reg.load_dev(pack("tools"), dir.path().join("old-project"))
                    .unwrap();
            }
            reg.quarantine("tools", "old owner's retired capabilities")
                .unwrap();
            assert!(reg.uninstall("tools").unwrap());
            if parked {
                assert!(reg.dev_path("tools").is_some());
                assert!(reg.quarantine_reason("tools").is_some());
                assert!(reg.set_enabled("tools", true).is_err());
                assert!(reg.unload_dev("tools").unwrap());
            }
            assert!(reg.record("tools").is_none());
            assert!(reg.quarantine_reason("tools").is_none());
            let restarted = ExtensionsRegistry::load(dir.path()).unwrap();
            assert!(restarted.quarantine_reason("tools").is_none());
            assert_eq!(
                restarted.quarantine_reason("other").as_deref(),
                Some("other owner's refusal")
            );
            restarted
                .load_dev(pack("tools"), dir.path().join("fresh-project"))
                .unwrap();
            assert!(restarted.set_enabled("tools", true).unwrap());
        }
    }

    #[test]
    fn failed_uninstall_restores_owner_quarantine_and_revision() {
        for mode in ["plain", "quarantined", "parked"] {
            let dir = tmp();
            let reg = ExtensionsRegistry::load(dir.path()).unwrap();
            reg.install(pack("tools")).unwrap();
            reg.install(pack("other")).unwrap();
            if mode == "parked" {
                reg.load_dev(pack("tools"), dir.path().join("project"))
                    .unwrap();
            }
            if mode != "plain" {
                reg.quarantine("tools", "owned refusal").unwrap();
            }
            let before = serde_json::to_value(reg.record("tools")).unwrap();
            let other = serde_json::to_value(reg.record("other")).unwrap();
            let revision = reg.record_revision("tools");
            let refusal = reg.quarantine_reason("tools");
            let saved = block_registry_save(&reg);
            assert!(reg.uninstall_with_record("tools").is_err());
            assert_eq!(serde_json::to_value(reg.record("tools")).unwrap(), before);
            assert_eq!(serde_json::to_value(reg.record("other")).unwrap(), other);
            assert_eq!(reg.record_revision("tools"), revision);
            assert_eq!(reg.quarantine_reason("tools"), refusal);
            restore_registry_file(&reg, &saved);
            let restarted = ExtensionsRegistry::load(dir.path()).unwrap();
            assert_eq!(
                serde_json::to_value(restarted.record("tools")).unwrap(),
                before
            );
            assert_eq!(restarted.quarantine_reason("tools"), refusal);
            assert!(reg.uninstall_with_record("tools").unwrap().is_some());
        }
    }

    #[test]
    fn updating_effective_dev_record_preserves_its_installed_backup() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut installed = pack("com.x.dev");
        installed.installed_version = "store".into();
        reg.install(installed).unwrap();
        reg.load_dev(pack("com.x.dev"), dir.path().join("project"))
            .unwrap();

        let mut active = reg.record("com.x.dev").unwrap();
        active.granted.push("storage".into());
        reg.install(active).unwrap();
        assert_eq!(reg.record("com.x.dev").unwrap().granted, vec!["storage"]);

        reg.unload_dev("com.x.dev").unwrap();
        assert_eq!(reg.record("com.x.dev").unwrap().installed_version, "store");
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = tmp();
        {
            let reg = ExtensionsRegistry::load(dir.path()).unwrap();
            let mut rec = pack("x");
            rec.installed_version = "2.0".into();
            reg.install(rec).unwrap();
            reg.set_enabled("x", true).unwrap();
        }
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        assert!(reg.is_enabled("x"));
        assert_eq!(reg.record("x").unwrap().installed_version, "2.0");
        // Damaged state remains recoverable and cannot silently initialize.
        let path = dir.path().join(EXTENSIONS_FILE);
        fs::write(&path, "{not json").unwrap();
        assert!(ExtensionsRegistry::load(dir.path()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{not json");
    }

    #[test]
    fn registry_load_refuses_inconsistent_identity_without_writes() {
        let dir = tmp();
        let path = dir.path().join(EXTENSIONS_FILE);
        for value in [
            serde_json::json!({"records":{"one":{"id":"other","enabled":true}}}),
            serde_json::json!({"records":[]}),
        ] {
            let bytes = serde_json::to_vec(&value).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert!(ExtensionsRegistry::load(dir.path()).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
        fs::remove_file(&path).unwrap();
        let fresh = ExtensionsRegistry::load(dir.path()).unwrap();
        assert!(fresh.records().is_empty());
    }

    #[test]
    fn oversized_registry_and_encoding_refuse_before_replacing_state() {
        let dir = tmp();
        let path = dir.path().join(EXTENSIONS_FILE);
        let file = fs::File::create(&path).unwrap();
        file.set_len(REGISTRY_MAX_BYTES as u64 + 1).unwrap();
        drop(file);
        assert!(ExtensionsRegistry::load(dir.path()).is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            REGISTRY_MAX_BYTES as u64 + 1
        );
        fs::remove_file(&path).unwrap();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
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
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        let mut record = pack("account");
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
            ExtensionsRegistry::load(dir.path())
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
            ExtensionsRegistry::load(dir.path())
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
            let reg = ExtensionsRegistry::load(dir.path()).unwrap();
            reg.install(pack("other")).unwrap();
            reg.set_enabled("other", true).unwrap();
            let record = pack("tools");
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
            restore_registry_file(&reg, &saved);
            let restarted = ExtensionsRegistry::load(dir.path()).unwrap();
            assert!(!restarted.is_enabled("tools"));
            assert!(restarted.record("tools").unwrap().granted.is_empty());
            assert!(activate().unwrap());
            assert!(reg.is_enabled("tools"));
            assert_ne!(reg.record_revision("tools"), revision);
            let restarted = ExtensionsRegistry::load(dir.path()).unwrap();
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
            let reg = ExtensionsRegistry::load(dir.path()).unwrap();
            if mode != "new" {
                let mut installed = pack("tools");
                installed.enabled = true;
                installed.installed_version = "committed".into();
                installed.authentication_session = Some("a".repeat(32));
                reg.install(installed).unwrap();
            }
            if mode == "parked" {
                reg.load_dev(pack("tools"), dir.path().join("project"))
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
            let mut candidate = pack("tools");
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
            let restarted = ExtensionsRegistry::load(dir.path()).unwrap();
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
            let reg = ExtensionsRegistry::load(dir.path()).unwrap();
            let mut installed = pack("tools");
            installed.enabled = true;
            installed.granted = vec!["auth".into()];
            installed.authentication_session = Some("a".repeat(32));
            reg.install(installed).unwrap();
            let mut dev = pack("tools");
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
            restore_registry_file(&reg, &saved);
            let restarted = ExtensionsRegistry::load(dir.path()).unwrap();
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
            assert_ne!(reg.record_revision("tools"), revision);
        }
    }

    #[test]
    fn concurrent_approval_and_disable_finish_disabled_without_stale_publication() {
        for _ in 0..32 {
            let dir = tmp();
            let reg = std::sync::Arc::new(ExtensionsRegistry::load(dir.path()).unwrap());
            reg.install(pack("tools")).unwrap();
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
            assert!(!ExtensionsRegistry::load(dir.path())
                .unwrap()
                .is_enabled("tools"));
        }
    }

    #[test]
    fn enable_counter_exhaustion_changes_no_identity() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path()).unwrap();
        reg.install(pack("tools")).unwrap();
        reg.state.write().unwrap().next_toggle_seq = u64::MAX;
        let revision = reg.record_revision("tools");
        assert!(reg
            .set_enabled("tools", true)
            .unwrap_err()
            .to_string()
            .contains("toggle sequence exhausted"));
        assert_eq!(reg.record_revision("tools"), revision);
        assert!(!reg.is_enabled("tools"));
    }
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

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    atomic_write_with(path, |file| {
        file.write_all(bytes).context("write extension state")
    })
}

pub(crate) fn atomic_write_with(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> Result<()>,
) -> Result<()> {
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
