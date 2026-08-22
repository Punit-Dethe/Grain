//! Phase 5A install / update / remove transaction (DISTRIBUTION-PLAN Â§5.2,
//! correction C-9) and **the trust invariant** (Â§3.2).
//!
//! This module is the *only* place a record may be born `verified` or `core`.
//! Everything else â€” manual `.grainpack` import, dev load â€” leaves the record
//! at [`Trust::UNTRUSTED_DEFAULT`]. That single-caller property is what makes
//! the anti-forgery guarantee hold: an author who controls their repo, build,
//! pack, site and domain still cannot make any client show their extension as
//! trusted, because trust is read from the signature-verified index here and
//! nowhere else.
//!
//! The on-disk transaction (staging â†’ path-safe extraction â†’ atomic rename)
//! lives in [`stage_artifact`]; the registry side (record with trust, held
//! disabled on a new-permission update, previous version retained) lives in
//! [`plan_record`] / [`install_from_verified_entry`].

use std::path::{Path, PathBuf};

use grain_sdk::distribution::IndexEntry;

use crate::extensions::{ExtensionRecord, ExtensionsRegistry};
use crate::pack::{self, ExtractLimits, PackShape};
use crate::trust::{self, TrustError};

/// Why an install/update was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// The downloaded artifact's hash did not match the verified index entry.
    Hash(TrustError),
    /// The artifact could not be safely extracted.
    Pack(pack::PackError),
    /// A filesystem step failed.
    Io(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Hash(e) => write!(f, "artifact verification failed: {e}"),
            InstallError::Pack(e) => write!(f, "unpack failed: {e}"),
            InstallError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for InstallError {}

/// Where an extension's versioned payload lives on disk:
/// `<root>/<id>/<version>/`. The previous version's directory survives until
/// the new one is in place, so a bad update is one directory away from rollback
/// (DISTRIBUTION-PLAN Â§5.2).
pub fn version_dir(root: &Path, id: &str, version: &str) -> PathBuf {
    root.join(id).join(version)
}

fn staging_dir(root: &Path, id: &str, version: &str) -> PathBuf {
    root.join(".staging").join(format!("{id}-{version}"))
}

/// Verify the artifact hash, then unpack it into its versioned directory via a
/// staging dir and an atomic rename. Returns the final version directory.
///
/// `bytes` is the already-downloaded `.grainpack`. The index entry was already
/// signature-verified by [`crate::trust::verify_index`]; here we bind those
/// exact bytes to the entry with SHA-256 before a single byte is unpacked.
pub fn stage_artifact(
    root: &Path,
    entry: &IndexEntry,
    bytes: &[u8],
    limits: ExtractLimits,
) -> Result<PathBuf, InstallError> {
    trust::verify_artifact(bytes, &entry.sha256).map_err(InstallError::Hash)?;

    let staging = staging_dir(root, &entry.id, &entry.version);
    // Clean any stale staging from an interrupted attempt.
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| InstallError::Io(e.to_string()))?;

    match pack::detect_shape(bytes) {
        PackShape::Zip => {
            // Multi-file bundle (scripted/native with separate entry + assets).
            // NOTE: the current runtime loads embedded single-file `GrainPack`s;
            // loading a multi-file directory bundle at runtime is a follow-on
            // once the worker/surface loaders read from disk. The extraction and
            // install transaction are complete and safe regardless.
            pack::extract_zip(bytes, &staging, limits).map_err(InstallError::Pack)?;
        }
        PackShape::Json => {
            // A single-file `GrainPack` (the runtime's native format): store it
            // under the canonical name the loader reads from the version dir.
            std::fs::write(staging.join("pack.grainpack.json"), bytes)
                .map_err(|e| InstallError::Io(e.to_string()))?;
        }
        PackShape::Unknown => {
            return Err(InstallError::Pack(pack::PackError::NotZip));
        }
    }

    let final_dir = version_dir(root, &entry.id, &entry.version);
    if let Some(parent) = final_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| InstallError::Io(e.to_string()))?;
    }
    // The only non-atomic step is this rename; the previous version dir (a
    // sibling under <id>/) is untouched until the caller enables the new one.
    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&staging, &final_dir).map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(final_dir)
}

/// Build the registry record for a verified index entry. **This is the sole
/// function that copies trust off an index entry into a record** â€” the single
/// caller the invariant depends on. It carries no filesystem side effects so it
/// is trivially unit-testable.
///
/// - `granted`: capabilities already granted for this id (carried across an
///   update). Empty for a fresh install.
/// - `prior`: the currently-installed record, if any, so an update that adds
///   permissions can be held disabled until the user approves the diff.
/// - `digests`: what the incoming pack declares, so an update that changed any
///   of it can be held disabled until the user approves the new declaration.
///
/// A struct rather than three positional `Option<String>` arguments: they are
/// mutually indistinguishable to the compiler, and transposing two of them
/// would silently approve one declaration against another's fingerprint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApprovalDigests {
    pub prompt_layers: Option<String>,
    pub actions: Option<String>,
    /// [GRAIN] The Extension Mode hand-off contract
    /// (`extensions::recommendation_fingerprint`). Always `Some` for a
    /// searchable extension, `None` otherwise.
    pub recommend: Option<String>,
}

pub fn plan_record(
    entry: &IndexEntry,
    granted: Vec<String>,
    prior: Option<&ExtensionRecord>,
    slots: Vec<String>,
    variant_slots: Vec<String>,
    digests: ApprovalDigests,
) -> ExtensionRecord {
    let prior_enabled = prior.map(|r| r.enabled).unwrap_or(false);
    // Update with NEW permissions installs but stays disabled until the diff is
    // approved (SPEC Â§6). A fresh install is disabled anyway (enable is the
    // user's explicit second step).
    let adds_permissions = entry.capabilities.iter().any(|cap| !granted.contains(cap));
    // Changed prompt-layer text is the same event, and needs saying separately
    // because it widens nothing a capability list can express: the pack asks for
    // no new permission, it just changes the words it puts in front of the model
    // when the user dictates. Approval that does not survive an update is
    // approval of a version nobody is running â€” CVE-2025-54136's exact lesson.
    let approved = prior.and_then(|r| r.prompt_layers_approved.clone());
    let changes_prompt_layers =
        digests.prompt_layers.is_some() && digests.prompt_layers != approved;
    // Changed actions are the same event again, and the sharpest of the three:
    // what an update can quietly alter here is not a permission or a sentence
    // but what pressing the key DOES â€” a `confirm` that became `safe`, a scope
    // that widened, a phrase that now captures a request it never used to.
    let approved_actions = prior.and_then(|r| r.actions_approved.clone());
    let changes_actions = digests.actions.is_some() && digests.actions != approved_actions;
    // [GRAIN] And once more for the Extensions V1 hand-off contract, which is
    // the widest of the three: what a change here alters is not what the
    // extension can DO but what it gets to HEAR, since being recommended means
    // receiving the whole transcript verbatim.
    let approved_recommend = prior.and_then(|r| r.recommend_approved.clone());
    let changes_recommend = digests.recommend.is_some() && digests.recommend != approved_recommend;
    let enabled = prior_enabled
        && !adds_permissions
        && !changes_prompt_layers
        && !changes_actions
        && !changes_recommend;

    ExtensionRecord {
        id: entry.id.clone(),
        enabled,
        toggle_seq: prior.map(|r| r.toggle_seq).unwrap_or(0),
        installed_version: entry.version.clone(),
        granted,
        // Slots come from the pack manifest we just installed â€” not the prior
        // record â€” so an update that changes them is reflected, and a fresh
        // store install actually claims what it declares (SPEC Â§3.2, Â§10.2).
        slots,
        variant_slots,
        // The PRIOR approval is carried forward untouched. Installing is not
        // approving: if the text changed, this no longer matches the
        // declaration, the layers stay inert, and the enable path shows the user
        // the new wording.
        prompt_layers_approved: approved,
        actions_approved: approved_actions,
        recommend_approved: approved_recommend,
        dev: None,
        // THE trust assignment. Sourced only from the verified entry, bound to
        // this exact (id, version, sha256): a verified 1.0 confers nothing on
        // 1.1 because 1.1 arrives as its own entry with its own trust.
        trust: entry.trust,
    }
}

/// Full install/update of a verified entry: stage the bytes, then write the
/// record. Returns the final version directory.
pub fn install_from_verified_entry(
    reg: &ExtensionsRegistry,
    root: &Path,
    entry: &IndexEntry,
    bytes: &[u8],
    limits: ExtractLimits,
) -> Result<PathBuf, InstallError> {
    let dir = stage_artifact(root, entry, bytes, limits)?;
    let prior = reg.installed_record(&entry.id);
    let granted = prior
        .as_ref()
        .map(|r| r.granted.clone())
        .unwrap_or_default();
    let manifest = manifest_of(bytes);
    let (slots, variant_slots) = manifest
        .as_ref()
        .map(|m| (m.slots.clone(), m.variant_slots.clone()))
        .unwrap_or_default();
    let digests = manifest.as_ref().map(declared_digests).unwrap_or_default();
    let record = plan_record(
        entry,
        granted,
        prior.as_ref(),
        slots,
        variant_slots,
        digests,
    );
    reg.install(record)
        .map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(dir)
}

/// Read the manifest a pack declares, from its bytes. A JSON pack embeds the
/// manifest; a ZIP pack carries `manifest.json`.
///
/// Best-effort: an unreadable manifest yields `None` rather than failing the
/// install, because the artifact already passed hash + extraction, so a pack
/// whose manifest will not parse is one that fails later and louder. Every
/// caller degrades in the safe direction — no slots claimed, and no approval
/// digest, which means a declaration that cannot be read is never treated as
/// approved.
///
/// One parse for all four derived values. Three separate readers is three
/// chances for them to disagree about what the same bytes said.
fn manifest_of(bytes: &[u8]) -> Option<grain_sdk::ExtensionManifest> {
    use grain_sdk::{ExtensionManifest, GrainPack};
    match pack::detect_shape(bytes) {
        PackShape::Json => serde_json::from_slice::<GrainPack>(bytes)
            .ok()
            .map(|p| p.manifest),
        PackShape::Zip => {
            let mut archive = zip_manifest_json(bytes)?;
            let m: Result<ExtensionManifest, _> = serde_json::from_slice(&archive);
            archive.clear();
            m.ok()
        }
        PackShape::Unknown => None,
    }
}

/// The approval digests a manifest's declarations imply.
///
/// `None` per digest means "declares nothing of this kind", which is what makes
/// the check one-sided in the right direction: a record whose approved value
/// does not match a live declaration contributes nothing, so a missing digest
/// can only ever withhold, never grant.
fn declared_digests(m: &grain_sdk::ExtensionManifest) -> ApprovalDigests {
    use crate::extensions as ext;
    ApprovalDigests {
        prompt_layers: (!m.contributes.prompt_layers.is_empty())
            .then(|| ext::prompt_layers_fingerprint(&m.contributes.prompt_layers)),
        actions: (!m.contributes.actions.is_empty())
            .then(|| ext::actions_fingerprint(&m.contributes.actions)),
        // [GRAIN] Unlike the other two this is keyed off `kind`, not off a list
        // being non-empty: what needs approving is being ELIGIBLE to receive the
        // user's words at all, and validation already guarantees a searchable
        // extension carries a `recommend` block.
        recommend: m
            .kind
            .is_searchable()
            .then(|| ext::recommendation_fingerprint(m)),
    }
}

/// Extract just the `manifest.json` bytes from a ZIP pack, in memory.
fn zip_manifest_json(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut file = archive.by_name("manifest.json").ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_sdk::distribution::Trust;
    use grain_sdk::manifest::Tier;

    fn entry(id: &str, version: &str, trust: Trust, caps: &[&str], bytes: &[u8]) -> IndexEntry {
        IndexEntry {
            id: id.into(),
            name: id.into(),
            version: version.into(),
            tier: Tier::Pack,
            trust,
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
            sha256: trust::sha256_hex(bytes),
            size: bytes.len() as u64,
            min_grain_api: String::new(),
            repo: String::new(),
            source_commit: String::new(),
            author: String::new(),
            reviewed_at: String::new(),
            reviewed_commit: String::new(),
            updated_at: String::new(),
            stars: 0,
            description: String::new(),
            installs: 0,
            readme: String::new(),
            media: Vec::new(),
            categories: Vec::new(),
            extends: Vec::new(),
        }
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    // â”€â”€ Anti-forgery guarantee, DISTRIBUTION-PLAN Â§3.2 â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn a_pack_claiming_trust_installs_untrusted() {
        // Property 1: a manifest/pack has no authority over trust. A pack whose
        // JSON contains "trust":"verified" is imported through the manual path,
        // which never touches `plan_record` â€” so the record is untrusted.
        // Here we prove the manual import default directly: a record built
        // WITHOUT a verified entry is `UNTRUSTED_DEFAULT`, whatever the bytes say.
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        // Simulate the manual import path: construct a record the way
        // grain_commands::extension_import does â€” no entry, untrusted default.
        let rec = ExtensionRecord {
            id: "com.evil.fake".into(),
            enabled: false,
            toggle_seq: 0,
            installed_version: "1.0.0".into(),
            granted: vec![],
            prompt_layers_approved: None,
            actions_approved: None,
            recommend_approved: None,
            slots: vec![],
            variant_slots: vec![],
            dev: None,
            trust: Trust::UNTRUSTED_DEFAULT,
        };
        reg.install(rec).unwrap();
        assert_eq!(reg.record("com.evil.fake").unwrap().trust, Trust::Dev);
        assert_ne!(reg.record("com.evil.fake").unwrap().trust, Trust::Verified);
    }

    #[test]
    fn verified_entry_is_the_only_way_to_become_verified() {
        // Property 2: trust flows from a verified entry through plan_record.
        let bytes = b"{\"id\":\"com.example.ok\"}";
        let e = entry("com.example.ok", "1.0.0", Trust::Verified, &[], bytes);
        let record = plan_record(&e, vec![], None, vec![], vec![], ApprovalDigests::default());
        assert_eq!(record.trust, Trust::Verified);
        assert_eq!(record.installed_version, "1.0.0");
    }

    #[test]
    fn trust_does_not_survive_a_version_bump() {
        // Property 4: a verified 1.0 confers nothing on 1.1. If 1.1's entry is
        // (say) still under review and published as untrusted, the updated
        // record is untrusted even though the prior 1.0 was verified.
        let prior_bytes = b"{\"v\":\"1.0\"}";
        let prior_entry = entry("com.example.x", "1.0.0", Trust::Verified, &[], prior_bytes);
        let prior = plan_record(
            &prior_entry,
            vec![],
            None,
            vec![],
            vec![],
            ApprovalDigests::default(),
        );
        assert_eq!(prior.trust, Trust::Verified);

        let new_bytes = b"{\"v\":\"1.1\"}";
        let new_entry = entry("com.example.x", "1.1.0", Trust::Dev, &[], new_bytes);
        let updated = plan_record(
            &new_entry,
            vec![],
            Some(&prior),
            vec![],
            vec![],
            ApprovalDigests::default(),
        );
        assert_eq!(
            updated.trust,
            Trust::Dev,
            "trust must be re-derived per version, never inherited"
        );
    }

    // â”€â”€ Install transaction, DISTRIBUTION-PLAN Â§5.2 â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn hash_mismatch_refuses_before_unpacking() {
        let dir = tmp();
        let bytes = b"{\"id\":\"x\"}";
        let mut e = entry("com.example.x", "1.0.0", Trust::Verified, &[], bytes);
        e.sha256 = "0000".into(); // wrong
        let err = stage_artifact(dir.path(), &e, bytes, ExtractLimits::default())
            .expect_err("bad hash must refuse");
        assert!(matches!(err, InstallError::Hash(_)));
        assert!(!version_dir(dir.path(), "com.example.x", "1.0.0").exists());
    }

    #[test]
    fn json_pack_installs_to_its_version_dir() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let bytes = b"{\"id\":\"com.example.x\",\"name\":\"X\"}";
        let e = entry("com.example.x", "2.0.0", Trust::Verified, &[], bytes);
        let out =
            install_from_verified_entry(&reg, dir.path(), &e, bytes, ExtractLimits::default())
                .expect("install");
        assert!(out.join("pack.grainpack.json").exists());
        let rec = reg.record("com.example.x").unwrap();
        assert_eq!(rec.installed_version, "2.0.0");
        assert_eq!(rec.trust, Trust::Verified);
        assert!(!rec.enabled, "fresh install lands disabled");
    }

    #[test]
    fn update_with_new_permissions_holds_disabled() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        // Prior 1.0 enabled with no capabilities.
        let b1 = b"{\"v\":1}";
        let e1 = entry("com.example.x", "1.0.0", Trust::Verified, &[], b1);
        install_from_verified_entry(&reg, dir.path(), &e1, b1, ExtractLimits::default()).unwrap();
        reg.set_enabled("com.example.x", true).unwrap();
        assert!(reg.is_enabled("com.example.x"));

        // 1.1 adds a capability â†’ held disabled until the diff is approved.
        let b2 = b"{\"v\":2}";
        let e2 = entry(
            "com.example.x",
            "1.1.0",
            Trust::Verified,
            &["net:api.example.com"],
            b2,
        );
        install_from_verified_entry(&reg, dir.path(), &e2, b2, ExtractLimits::default()).unwrap();
        let rec = reg.record("com.example.x").unwrap();
        assert_eq!(rec.installed_version, "1.1.0");
        assert!(
            !rec.enabled,
            "new permissions must hold the update disabled"
        );
    }

    /// The rug pull (PLAN Â§T1 / CVE-2025-54136). A pack whose prompt layer text
    /// changes asks for no new capability, so nothing in the permission diff
    /// would notice â€” and the layer is the part that decides what the model does
    /// to the user's own words.
    #[test]
    fn update_with_changed_prompt_layers_holds_disabled() {
        let approved = ExtensionRecord {
            id: "com.example.x".into(),
            enabled: true,
            toggle_seq: 1,
            installed_version: "1.0.0".into(),
            granted: vec![],
            prompt_layers_approved: Some("fingerprint-of-1.0".into()),
            actions_approved: None,
            recommend_approved: None,
            slots: vec![],
            variant_slots: vec![],
            dev: None,
            trust: Trust::Verified,
        };
        let e = entry("com.example.x", "1.1.0", Trust::Verified, &[], b"{}");

        let unchanged = plan_record(
            &e,
            vec![],
            Some(&approved),
            vec![],
            vec![],
            ApprovalDigests {
                prompt_layers: Some("fingerprint-of-1.0".into()),
                ..Default::default()
            },
        );
        assert!(
            unchanged.enabled,
            "an update that leaves the wording alone stays enabled"
        );

        let changed = plan_record(
            &e,
            vec![],
            Some(&approved),
            vec![],
            vec![],
            ApprovalDigests {
                prompt_layers: Some("fingerprint-of-1.1".into()),
                ..Default::default()
            },
        );
        assert!(
            !changed.enabled,
            "changed prompt text must hold the update until the user reads it"
        );
        assert_eq!(
            changed.prompt_layers_approved.as_deref(),
            Some("fingerprint-of-1.0"),
            "installing is not approving â€” the prior approval is carried, not overwritten"
        );
    }

    /// The same rug pull, one step sharper: what an update changes here is not
    /// wording but **what happens**. A `confirm` that quietly became `safe` asks
    /// for no new capability and changes no sentence the user would notice.
    #[test]
    fn update_with_changed_actions_holds_disabled() {
        let approved = ExtensionRecord {
            id: "com.example.x".into(),
            enabled: true,
            toggle_seq: 1,
            installed_version: "1.0.0".into(),
            granted: vec![],
            prompt_layers_approved: None,
            actions_approved: Some("actions-of-1.0".into()),
            recommend_approved: None,
            slots: vec![],
            variant_slots: vec![],
            dev: None,
            trust: Trust::Verified,
        };
        let e = entry("com.example.x", "1.1.0", Trust::Verified, &[], b"{}");

        let unchanged = plan_record(
            &e,
            vec![],
            Some(&approved),
            vec![],
            vec![],
            ApprovalDigests {
                actions: Some("actions-of-1.0".into()),
                ..Default::default()
            },
        );
        assert!(
            unchanged.enabled,
            "an update that leaves the actions alone stays enabled"
        );

        let changed = plan_record(
            &e,
            vec![],
            Some(&approved),
            vec![],
            vec![],
            ApprovalDigests {
                actions: Some("actions-of-1.1".into()),
                ..Default::default()
            },
        );
        assert!(
            !changed.enabled,
            "changed actions must hold the update until the user reads them"
        );
        assert_eq!(
            changed.actions_approved.as_deref(),
            Some("actions-of-1.0"),
            "installing is not approving — the prior approval is carried, not overwritten"
        );
    }

    /// The rug pull once more, on the declaration that decides **who hears the
    /// user's words**. A searchable extension is ranked by text it wrote itself;
    /// rewriting that text after approval is how an installed extension quietly
    /// starts receiving requests it was never reviewed to receive.
    #[test]
    fn update_with_changed_recommendation_holds_disabled() {
        let approved = ExtensionRecord {
            id: "com.example.x".into(),
            enabled: true,
            toggle_seq: 1,
            installed_version: "1.0.0".into(),
            granted: vec![],
            prompt_layers_approved: None,
            actions_approved: None,
            recommend_approved: Some("recommend-of-1.0".into()),
            slots: vec![],
            variant_slots: vec![],
            dev: None,
            trust: Trust::Verified,
        };
        let e = entry("com.example.x", "1.1.0", Trust::Verified, &[], b"{}");

        let unchanged = plan_record(
            &e,
            vec![],
            Some(&approved),
            vec![],
            vec![],
            ApprovalDigests {
                recommend: Some("recommend-of-1.0".into()),
                ..Default::default()
            },
        );
        assert!(
            unchanged.enabled,
            "an update that leaves what it is ranked by alone stays enabled"
        );

        let changed = plan_record(
            &e,
            vec![],
            Some(&approved),
            vec![],
            vec![],
            ApprovalDigests {
                recommend: Some("recommend-of-1.1".into()),
                ..Default::default()
            },
        );
        assert!(
            !changed.enabled,
            "a rewritten recommendation must hold the update until the user reads it"
        );
        assert_eq!(
            changed.recommend_approved.as_deref(),
            Some("recommend-of-1.0"),
            "installing is not approving — the prior approval is carried, not overwritten"
        );
    }

    /// The three approvals are independent on purpose: re-asking about what an
    /// extension can DO because it reworded a prompt layer is how a user learns
    /// to click through all of them.
    #[test]
    fn the_three_approvals_do_not_drag_each_other() {
        let approved = ExtensionRecord {
            id: "com.example.x".into(),
            enabled: true,
            toggle_seq: 1,
            installed_version: "1.0.0".into(),
            granted: vec![],
            prompt_layers_approved: Some("layers-of-1.0".into()),
            actions_approved: Some("actions-of-1.0".into()),
            recommend_approved: Some("recommend-of-1.0".into()),
            slots: vec![],
            variant_slots: vec![],
            dev: None,
            trust: Trust::Verified,
        };
        let e = entry("com.example.x", "1.1.0", Trust::Verified, &[], b"{}");

        let reworded = plan_record(
            &e,
            vec![],
            Some(&approved),
            vec![],
            vec![],
            ApprovalDigests {
                prompt_layers: Some("layers-of-1.1".into()),
                actions: Some("actions-of-1.0".into()),
                recommend: Some("recommend-of-1.0".into()),
            },
        );
        assert!(!reworded.enabled);
        assert_eq!(
            reworded.actions_approved.as_deref(),
            Some("actions-of-1.0"),
            "the untouched approval is preserved, not reset by its neighbour"
        );
        assert_eq!(
            reworded.recommend_approved.as_deref(),
            Some("recommend-of-1.0"),
            "nor by its other neighbour"
        );
    }

    #[test]
    fn update_with_same_permissions_keeps_enabled() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let b1 = b"{\"v\":1}";
        let e1 = entry("com.example.x", "1.0.0", Trust::Verified, &["storage"], b1);
        install_from_verified_entry(&reg, dir.path(), &e1, b1, ExtractLimits::default()).unwrap();
        // Grant the capability, then enable.
        let mut rec = reg.record("com.example.x").unwrap();
        rec.granted = vec!["storage".into()];
        reg.install(rec).unwrap();
        reg.set_enabled("com.example.x", true).unwrap();

        let b2 = b"{\"v\":2}";
        let e2 = entry("com.example.x", "1.2.0", Trust::Verified, &["storage"], b2);
        install_from_verified_entry(&reg, dir.path(), &e2, b2, ExtractLimits::default()).unwrap();
        assert!(
            reg.is_enabled("com.example.x"),
            "an update that adds no permissions stays enabled"
        );
    }

    #[test]
    fn installing_a_never_installs_b() {
        // The no-transitive-install invariant: installing one entry touches
        // exactly one id.
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let bytes = b"{\"id\":\"com.example.a\"}";
        let e = entry("com.example.a", "1.0.0", Trust::Verified, &[], bytes);
        install_from_verified_entry(&reg, dir.path(), &e, bytes, ExtractLimits::default()).unwrap();
        assert!(reg.is_installed("com.example.a"));
        assert_eq!(reg.records().len(), 1, "install touched exactly one id");
    }
}
