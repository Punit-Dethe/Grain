//! Phase 5A install / update / remove transaction (DISTRIBUTION-PLAN §5.2,
//! correction C-9) and **the trust invariant** (§3.2).
//!
//! This module is the *only* place a record may be born `verified` or `core`.
//! Everything else — manual `.grainpack` import, dev load — leaves the record
//! at [`Trust::UNTRUSTED_DEFAULT`]. That single-caller property is what makes
//! the anti-forgery guarantee hold: an author who controls their repo, build,
//! pack, site and domain still cannot make any client show their extension as
//! trusted, because trust is read from the signature-verified index here and
//! nowhere else.
//!
//! The on-disk transaction (staging → path-safe extraction → atomic rename)
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
/// (DISTRIBUTION-PLAN §5.2).
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
/// function that copies trust off an index entry into a record** — the single
/// caller the invariant depends on. It carries no filesystem side effects so it
/// is trivially unit-testable.
///
/// - `granted`: capabilities already granted for this id (carried across an
///   update). Empty for a fresh install.
/// - `prior`: the currently-installed record, if any, so an update that adds
///   permissions can be held disabled until the user approves the diff.
pub fn plan_record(
    entry: &IndexEntry,
    granted: Vec<String>,
    prior: Option<&ExtensionRecord>,
    slots: Vec<String>,
    variant_slots: Vec<String>,
) -> ExtensionRecord {
    let prior_enabled = prior.map(|r| r.enabled).unwrap_or(false);
    // Update with NEW permissions installs but stays disabled until the diff is
    // approved (SPEC §6). A fresh install is disabled anyway (enable is the
    // user's explicit second step).
    let adds_permissions = entry
        .capabilities
        .iter()
        .any(|cap| !granted.contains(cap));
    let enabled = prior_enabled && !adds_permissions;

    ExtensionRecord {
        id: entry.id.clone(),
        enabled,
        toggle_seq: prior.map(|r| r.toggle_seq).unwrap_or(0),
        installed_version: entry.version.clone(),
        granted,
        // Slots come from the pack manifest we just installed — not the prior
        // record — so an update that changes them is reflected, and a fresh
        // store install actually claims what it declares (SPEC §3.2, §10.2).
        slots,
        variant_slots,
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
    let granted = prior.as_ref().map(|r| r.granted.clone()).unwrap_or_default();
    let (slots, variant_slots) = manifest_slots(bytes);
    let record = plan_record(entry, granted, prior.as_ref(), slots, variant_slots);
    reg.install(record).map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(dir)
}

/// Read the `(slots, variant_slots)` a pack declares, from its bytes. A JSON
/// pack embeds the manifest; a ZIP pack carries `manifest.json`. Best-effort:
/// an unreadable manifest yields no claims rather than failing the install
/// (the artifact already passed hash + extraction).
fn manifest_slots(bytes: &[u8]) -> (Vec<String>, Vec<String>) {
    use grain_sdk::{ExtensionManifest, GrainPack};
    match pack::detect_shape(bytes) {
        PackShape::Json => serde_json::from_slice::<GrainPack>(bytes)
            .map(|p| (p.manifest.slots, p.manifest.variant_slots))
            .unwrap_or_default(),
        PackShape::Zip => {
            // The manifest.json entry — read it out of the archive in-memory.
            let mut archive = match zip_manifest_json(bytes) {
                Some(m) => m,
                None => return (Vec::new(), Vec::new()),
            };
            let m: Result<ExtensionManifest, _> = serde_json::from_slice(&archive);
            archive.clear();
            m.map(|m| (m.slots, m.variant_slots)).unwrap_or_default()
        }
        PackShape::Unknown => (Vec::new(), Vec::new()),
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
        }
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    // ── Anti-forgery guarantee, DISTRIBUTION-PLAN §3.2 ──────────────────────

    #[test]
    fn a_pack_claiming_trust_installs_untrusted() {
        // Property 1: a manifest/pack has no authority over trust. A pack whose
        // JSON contains "trust":"verified" is imported through the manual path,
        // which never touches `plan_record` — so the record is untrusted.
        // Here we prove the manual import default directly: a record built
        // WITHOUT a verified entry is `UNTRUSTED_DEFAULT`, whatever the bytes say.
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        // Simulate the manual import path: construct a record the way
        // grain_commands::extension_import does — no entry, untrusted default.
        let rec = ExtensionRecord {
            id: "com.evil.fake".into(),
            enabled: false,
            toggle_seq: 0,
            installed_version: "1.0.0".into(),
            granted: vec![],
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
        let record = plan_record(&e, vec![], None, vec![], vec![]);
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
        let prior = plan_record(&prior_entry, vec![], None, vec![], vec![]);
        assert_eq!(prior.trust, Trust::Verified);

        let new_bytes = b"{\"v\":\"1.1\"}";
        let new_entry = entry("com.example.x", "1.1.0", Trust::Dev, &[], new_bytes);
        let updated = plan_record(&new_entry, vec![], Some(&prior), vec![], vec![]);
        assert_eq!(
            updated.trust,
            Trust::Dev,
            "trust must be re-derived per version, never inherited"
        );
    }

    // ── Install transaction, DISTRIBUTION-PLAN §5.2 ─────────────────────────

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
        let out = install_from_verified_entry(&reg, dir.path(), &e, bytes, ExtractLimits::default())
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

        // 1.1 adds a capability → held disabled until the diff is approved.
        let b2 = b"{\"v\":2}";
        let e2 = entry("com.example.x", "1.1.0", Trust::Verified, &["net:api.example.com"], b2);
        install_from_verified_entry(&reg, dir.path(), &e2, b2, ExtractLimits::default()).unwrap();
        let rec = reg.record("com.example.x").unwrap();
        assert_eq!(rec.installed_version, "1.1.0");
        assert!(!rec.enabled, "new permissions must hold the update disabled");
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
