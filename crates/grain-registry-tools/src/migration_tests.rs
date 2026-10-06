//! Connected archive/sign/store/export/capture/Git migration regressions.
//! Real signatures and Git; production keys/approval/hosting are never used.
use super::*;

struct Migration {
    l: Legacy,
    pin: String,
    baseline: PathBuf,
    store: PathBuf,
    bundle: PathBuf,
    bundle_pin: String,
}

impl Migration {
    fn new() -> Self {
        Self::with_days(30)
    }
    fn with_days(days: u32) -> Self {
        let l = Legacy::new();
        l.run().unwrap();
        let pin = digest(&fs::read(l.out.join("legacy-history.json")).unwrap());
        let baseline = l.f.root.path().join("migration");
        migration::sign_with(
            &l.out,
            &pin,
            &l.f.root.path().join("keys/publisher.key"),
            days,
            &baseline,
            &l.f.anchor(),
        )
        .unwrap();
        let store = l.f.root.path().join("migrated store ü");
        initialize_with(&baseline.join("v1"), &store, &l.f.anchor()).unwrap();
        let bundle = l.f.root.path().join("migrated bundle ü");
        let pointer = digest(&fs::read(store.join("current.json")).unwrap());
        hosting::export(&store, &bundle, &pointer, &l.f.anchor()).unwrap();
        let bundle_pin = digest(&fs::read(bundle.join("bundle.json")).unwrap());
        Self {
            l,
            pin,
            baseline,
            store,
            bundle,
            bundle_pin,
        }
    }
    fn next(&self, name: &str, version: &str) -> PathBuf {
        let next = self.l.f.tree(name, 4, false);
        let mut index = self.l.f.json(&next, "index.json");
        index["entries"][0]["version"] = json!(version);
        index["legacy_history_sha256"] = json!(self.pin);
        self.l.f.signed(&next, "index.json", &index, "publisher");
        for name in ["revocations.json", "revocations.json.minisig"] {
            fs::copy(self.baseline.join("v1").join(name), next.join(name)).unwrap();
        }
        next
    }
    fn committed(&self) -> String {
        Publication::install(&self.bundle, &self.l.repo);
        copy_dir(
            &self.bundle.join("legacy"),
            &self.l.repo.join(".registry-publication/legacy"),
        );
        git(&self.l.repo, &["add", "."]);
        git(&self.l.repo, &["add", "--renormalize", "."]);
        git(&self.l.repo, &["commit", "-m", "empty signed migration"]);
        git(&self.l.repo, &["rev-parse", "HEAD"])
    }
    fn request<'a>(&'a self, candidate: &'a str, out: &'a Path) -> Request<'a> {
        Request {
            checkout: &self.l.repo,
            repository: "example/registry",
            branch: "release",
            base_commit: &self.l.commits[1],
            candidate_commit: candidate,
            previous: &self.l.out,
            previous_pin: &self.pin,
            bundle: &self.bundle,
            bundle_pin: &self.bundle_pin,
            out,
        }
    }
}

#[test]
fn migration_renewal_preserves_signed_archive_binding_retirements_and_history() {
    let m = Migration::with_days(1);
    let before = fs::read(m.store.join("current.json")).unwrap();
    let old = check(&m.baseline.join("v1"), &m.l.f.anchor(), true).unwrap();
    let renewal = m.l.f.root.path().join("renewed migration");
    renewal::renew_with(
        &m.baseline.join("v1"),
        &old.state.snapshot,
        &m.l.f.root.path().join("keys/publisher.key"),
        30,
        &renewal,
        &m.l.f.anchor(),
    )
    .unwrap();
    let renewed = check(&renewal.join("v1"), &m.l.f.anchor(), true).unwrap();
    assert_eq!(renewed.legacy_pin, old.legacy_pin);
    assert_eq!(
        serde_json::to_value(&renewed.revocations.entries).unwrap(),
        serde_json::to_value(&old.revocations.entries).unwrap()
    );
    assert_eq!(renewed.state.index_version, old.state.index_version + 1);
    assert_eq!(
        renewed.state.revocations_version,
        old.state.revocations_version + 1
    );
    promote_with(&renewal, &m.store, &digest(&before), &m.l.f.anchor()).unwrap();
    let out = m.l.f.root.path().join("renewed migration bundle");
    let pin = digest(&fs::read(m.store.join("current.json")).unwrap());
    hosting::export(&m.store, &out, &pin, &m.l.f.anchor()).unwrap();
    let receipt_pin = digest(&fs::read(out.join("bundle.json")).unwrap());
    let receipt = hosting::inspect(&out, &receipt_pin, &m.l.f.anchor(), true).unwrap();
    assert_eq!(receipt.history, vec![old.state.snapshot]);
    assert_eq!(
        fs::read(out.join("legacy/legacy-history.json")).unwrap(),
        fs::read(m.l.out.join("legacy-history.json")).unwrap()
    );
}

#[test]
fn migration_reauthenticates_archive_and_cannot_trust_forged_unsigned_reservations() {
    let l = Legacy::new();
    l.run().unwrap();
    let file = l.out.join("legacy-history.json");
    let raw = fs::read(&file).unwrap();
    let pin = digest(&raw);
    let archive = legacy::inspect(&l.out, &pin, &l.f.anchor()).unwrap();
    assert_eq!(archive.reserved.len(), 2);
    for field in [
        "reservations",
        "conflicted_versions",
        "assets",
        "asset_sources",
    ] {
        let mut value: Value = serde_json::from_slice(&raw).unwrap();
        if value[field].is_array() {
            value[field] = json!([]);
        } else {
            value[field] = json!({});
        }
        let forged = serde_json::to_vec(&value).unwrap();
        fs::write(&file, &forged).unwrap();
        assert!(legacy::inspect(&l.out, &digest(&forged), &l.f.anchor())
            .err()
            .unwrap()
            .to_string()
            .contains("authenticated history"));
    }
    fs::write(&file, &raw).unwrap();
    let proof = l.out.join(format!("proofs/{}/index.json", l.commits[0]));
    let mut bytes = fs::read(&proof).unwrap();
    bytes.push(b' ');
    fs::write(proof, bytes).unwrap();
    assert!(legacy::inspect(&l.out, &pin, &l.f.anchor())
        .err()
        .unwrap()
        .to_string()
        .contains("signature does not verify"));
}

#[test]
fn migration_signs_only_empty_current_baseline_with_all_legacy_versions_revoked() {
    let m = Migration::new();
    let tree = check(&m.baseline.join("v1"), &m.l.f.anchor(), true).unwrap();
    assert!(tree.index.entries.is_empty());
    assert_eq!(tree.state.index_version, 3);
    assert_eq!(tree.state.revocations_version, 2);
    assert_eq!(tree.legacy_pin.as_ref(), Some(&m.pin));
    assert_eq!(tree.revocations.entries.len(), 2);
    assert!(tree
        .revocations
        .entries
        .iter()
        .all(|r| r.version.is_some() && r.state == RevocationState::Revoked));
    assert_eq!(
        m.l.f.json(&m.baseline.join("v1"), "index.json")["future_envelope"],
        json!({"retained":true})
    );
    assert_eq!(
        fs::read(m.baseline.join("v1/roots.json")).unwrap(),
        fs::read(
            m.l.out
                .join(format!("proofs/{}/roots.json", m.l.commits[1]))
        )
        .unwrap()
    );
    let archive = legacy::inspect(&m.bundle.join("legacy"), &m.pin, &m.l.f.anchor()).unwrap();
    for name in archive.assets.keys() {
        assert!(m.bundle.join("v1").join(name).is_file());
    }
    assert_eq!(
        fs::read(m.baseline.join("legacy/legacy-history.json")).unwrap(),
        fs::read(m.l.out.join("legacy-history.json")).unwrap()
    );
}

#[test]
fn migration_refuses_invalid_public_inputs_before_missing_key_and_wrong_signer_before_output() {
    let l = Legacy::new();
    l.run().unwrap();
    let pin = digest(&fs::read(l.out.join("legacy-history.json")).unwrap());
    let out = l.f.root.path().join("refused");
    let absent = l.f.root.path().join("never-open-key");
    assert!(
        migration::sign_with(&l.out, &pin, &absent, 0, &out, &l.f.anchor())
            .unwrap_err()
            .to_string()
            .contains("lifetime")
    );
    assert!(
        migration::sign_with(&l.out, &"0".repeat(64), &absent, 30, &out, &l.f.anchor())
            .unwrap_err()
            .to_string()
            .contains("independent pin")
    );
    assert!(migration::sign_with(
        &l.out,
        &pin,
        &absent,
        30,
        &l.out.join("inside"),
        &l.f.anchor()
    )
    .unwrap_err()
    .to_string()
    .contains("outside"));
    assert!(migration::sign_with(
        &l.out,
        &pin,
        &l.f.root.path().join("keys/rotated.key"),
        30,
        &out,
        &l.f.anchor()
    )
    .unwrap_err()
    .to_string()
    .contains("signature does not verify"));
    assert!(!out.exists());
    let key = l.f.root.path().join("too-large.key");
    fs::write(&key, vec![b'x'; 8193]).unwrap();
    assert!(migration::sign_with(&l.out, &pin, &key, 30, &out, &l.f.anchor()).is_err());
    assert!(!out.exists());
}

#[test]
fn migration_rejects_reused_versions_and_preserves_pointer_on_failed_promotion() {
    let m = Migration::new();
    let before = fs::read(m.store.join("current.json")).unwrap();
    let update = m.next("reused", "1.0.0");
    let assembly = m.l.f.root.path().join("reused-assembly");
    assemble_with(&m.baseline.join("v1"), &update, &assembly, &m.l.f.anchor()).unwrap();
    assert!(
        promote_with(&assembly, &m.store, &digest(&before), &m.l.f.anchor())
            .unwrap_err()
            .to_string()
            .contains("permanently reserved")
    );
    assert_eq!(fs::read(m.store.join("current.json")).unwrap(), before);
    assert_eq!(fs::read_dir(m.store.join("snapshots")).unwrap().count(), 1);
}

#[test]
fn migration_keeps_archive_through_genuine_new_version_promotion_and_export() {
    let m = Migration::new();
    let before = digest(&fs::read(m.store.join("current.json")).unwrap());
    let update = m.next("new tools version", "2.0.0");
    let assembly = m.l.f.root.path().join("next assembly");
    assemble_with(&m.baseline.join("v1"), &update, &assembly, &m.l.f.anchor()).unwrap();
    promote_with(&assembly, &m.store, &before, &m.l.f.anchor()).unwrap();
    let pin = digest(&fs::read(m.store.join("current.json")).unwrap());
    let exported = m.l.f.root.path().join("exported current");
    export_with(&m.store, &exported, &pin, &m.l.f.anchor()).unwrap();
    assert!(exported.join("legacy/legacy-history.json").is_file());
    let bundle = m.l.f.root.path().join("new hosting");
    hosting::export(&m.store, &bundle, &pin, &m.l.f.anchor()).unwrap();
    let receipt_pin = digest(&fs::read(bundle.join("bundle.json")).unwrap());
    let receipt = hosting::inspect(&bundle, &receipt_pin, &m.l.f.anchor(), true).unwrap();
    assert_eq!(receipt.history.len(), 1);
    let archive = legacy::inspect(&bundle.join("legacy"), &m.pin, &m.l.f.anchor()).unwrap();
    for name in archive.assets.keys() {
        assert!(bundle.join("v1").join(name).is_file());
    }
    assert_eq!(
        fs::read(m.store.join("legacy/legacy-history.json")).unwrap(),
        fs::read(m.l.out.join("legacy-history.json")).unwrap()
    );
}

#[test]
fn migration_cannot_remove_binding_weaken_retirement_or_initialize_without_archive() {
    let m = Migration::new();
    let update = m.next("missing binding", "2.0.0");
    for name in ["index.json", "revocations.json"] {
        let mut value = m.l.f.json(&update, name);
        value
            .as_object_mut()
            .unwrap()
            .remove("legacy_history_sha256");
        m.l.f.signed(&update, name, &value, "publisher");
    }
    let assembly = m.l.f.root.path().join("refused assembly");
    assert!(
        assemble_with(&m.baseline.join("v1"), &update, &assembly, &m.l.f.anchor())
            .unwrap_err()
            .to_string()
            .contains("signed legacy archive binding")
    );
    assert!(!assembly.exists());
    let update = m.next("weak retirement", "2.0.0");
    let mut revs = m.l.f.json(&update, "revocations.json");
    revs["version"] = json!(3);
    revs["entries"] = json!([]);
    m.l.f
        .signed(&update, "revocations.json", &revs, "publisher");
    assert!(
        assemble_with(&m.baseline.join("v1"), &update, &assembly, &m.l.f.anchor())
            .unwrap_err()
            .to_string()
            .contains("prior revocation")
    );
    let isolated = m.l.f.root.path().join("missing archive source");
    fs::create_dir(&isolated).unwrap();
    copy_dir(&m.baseline.join("v1"), &isolated.join("v1"));
    let store = m.l.f.root.path().join("refused store");
    assert!(initialize_with(&isolated.join("v1"), &store, &m.l.f.anchor()).is_err());
    assert!(!store.exists());
}

#[test]
fn migration_hosting_refuses_missing_archive_asset_and_forged_receipt_variants() {
    let m = Migration::new();
    let receipt = m.bundle.join("legacy/legacy-history.json");
    let raw = fs::read(&receipt).unwrap();
    fs::write(&receipt, b"{}").unwrap();
    assert!(
        hosting::inspect(&m.bundle, &m.bundle_pin, &m.l.f.anchor(), true)
            .err()
            .unwrap()
            .to_string()
            .contains("Legacy receipt")
    );
    fs::write(receipt, raw).unwrap();
    let archive = legacy::inspect(&m.bundle.join("legacy"), &m.pin, &m.l.f.anchor()).unwrap();
    let asset = archive.assets.keys().next().unwrap();
    fs::remove_file(m.bundle.join("v1").join(asset)).unwrap();
    assert!(hosting::inspect(&m.bundle, &m.bundle_pin, &m.l.f.anchor(), true).is_err());
}

#[test]
fn migration_conditional_handoff_and_raw_capture_preserve_complete_original_history() {
    let m = Migration::new();
    let candidate = m.committed();
    let out = m.l.f.root.path().join("initial handoff");
    publication::prepare_migration_with(&m.request(&candidate, &out), &m.l.f.anchor()).unwrap();
    let receipt: Handoff =
        serde_json::from_slice(&fs::read(out.join("publication.json")).unwrap()).unwrap();
    assert!(receipt.evidence_class.contains("legacy-migration-handoff"));
    assert!(receipt.push_args.contains(&format!(
        "--force-with-lease=refs/heads/release:{}",
        m.l.commits[1]
    )));
    let capture = m.l.f.root.path().join("captured migrated history");
    crate::serving::capture::capture_with(
        &m.l.repo,
        "example/registry",
        &candidate,
        &m.bundle_pin,
        &capture,
        &m.l.f.anchor(),
    )
    .unwrap();
    hosting::inspect(
        &capture.join("bundle"),
        &m.bundle_pin,
        &m.l.f.anchor(),
        true,
    )
    .unwrap();
    assert_eq!(
        fs::read(capture.join("bundle/legacy/legacy-history.json")).unwrap(),
        fs::read(m.l.out.join("legacy-history.json")).unwrap()
    );
    assert_eq!(git(&m.l.repo, &["rev-parse", "HEAD"]), candidate);
}

#[test]
fn migration_handoff_refuses_wrong_base_active_entries_and_changed_committed_proof() {
    let m = Migration::new();
    let candidate = m.committed();
    let out = m.l.f.root.path().join("refused handoff");
    let mut request = m.request(&candidate, &out);
    request.base_commit = &m.l.commits[0];
    assert!(
        publication::prepare_migration_with(&request, &m.l.f.anchor())
            .unwrap_err()
            .to_string()
            .contains("sole parent")
    );
    assert!(!out.exists());
    fs::write(
        m.l.repo
            .join(".registry-publication/legacy/legacy-history.json"),
        "wrong bytes",
    )
    .unwrap();
    git(&m.l.repo, &["add", "."]);
    git(&m.l.repo, &["commit", "--amend", "--no-edit"]);
    let changed = git(&m.l.repo, &["rev-parse", "HEAD"]);
    assert!(
        publication::prepare_migration_with(&m.request(&changed, &out), &m.l.f.anchor())
            .unwrap_err()
            .to_string()
            .contains("Committed publication bytes differ")
    );
    assert!(!out.exists());
    let archive = legacy::inspect(&m.l.out, &m.pin, &m.l.f.anchor()).unwrap();
    let update = m.next("active during migration", "2.0.0");
    let tree = check(&update, &m.l.f.anchor(), true).unwrap();
    assert!(migration::validate_baseline(&tree, &archive, &m.pin)
        .unwrap_err()
        .to_string()
        .contains("empty next-version"));
}

#[test]
fn migration_expired_roots_and_index_exhaustion_refuse_before_key_access() {
    for exhausted in [false, true] {
        let mut l = Legacy::new();
        let dir = l.repo.join("v1");
        let name = if exhausted {
            "index.json"
        } else {
            "roots.json"
        };
        let mut value = l.f.json(&dir, name);
        if exhausted {
            value["version"] = json!(u64::MAX);
        } else {
            value["expires"] = json!("2000-01-01T00:00:00Z");
        }
        l.f.signed(
            &dir,
            name,
            &value,
            if exhausted { "publisher" } else { "root" },
        );
        l.amend();
        l.run().unwrap();
        let pin = digest(&fs::read(l.out.join("legacy-history.json")).unwrap());
        let out = l.f.root.path().join("refused migration");
        let error = migration::sign_with(
            &l.out,
            &pin,
            &l.f.root.path().join("never-key"),
            30,
            &out,
            &l.f.anchor(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains(if exhausted {
                "version exhausted"
            } else {
                "Expired roots"
            }),
            "{error}"
        );
        assert!(!out.exists());
    }
}

#[test]
fn migration_archive_refuses_unreferenced_files_and_invalid_source_path_labels() {
    let l = Legacy::new();
    l.run().unwrap();
    let receipt = l.out.join("legacy-history.json");
    let raw = fs::read(&receipt).unwrap();
    let pin = digest(&raw);
    fs::write(l.out.join("extra.json"), "unreviewed extra").unwrap();
    assert!(legacy::inspect(&l.out, &pin, &l.f.anchor())
        .err()
        .unwrap()
        .to_string()
        .contains("Unexpected"));
    fs::remove_file(l.out.join("extra.json")).unwrap();
    let mut value: Value = serde_json::from_slice(&raw).unwrap();
    let first = value["asset_sources"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    value["asset_sources"][first] = json!("../escaped-source");
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::write(&receipt, &bytes).unwrap();
    assert!(legacy::inspect(&l.out, &digest(&bytes), &l.f.anchor()).is_err());
    fs::write(&receipt, raw).unwrap();
    let proof = l
        .out
        .join(format!("proofs/{}/index.json.minisig", l.commits[0]));
    fs::remove_file(proof).unwrap();
    assert!(legacy::inspect(&l.out, &pin, &l.f.anchor()).is_err());
}

#[test]
fn migration_signed_bindings_cannot_mismatch_or_change_archive() {
    let m = Migration::new();
    let next = m.next("mismatched signed bind", "2.0.0");
    let mut index = m.l.f.json(&next, "index.json");
    index["legacy_history_sha256"] = json!("0".repeat(64));
    m.l.f.signed(&next, "index.json", &index, "publisher");
    assert!(check(&next, &m.l.f.anchor(), true)
        .err()
        .unwrap()
        .to_string()
        .contains("same legacy archive"));
    let mut revs = m.l.f.json(&next, "revocations.json");
    revs["legacy_history_sha256"] = json!("0".repeat(64));
    m.l.f.signed(&next, "revocations.json", &revs, "publisher");
    let out = m.l.f.root.path().join("changed binding assembly");
    assert!(
        assemble_with(&m.baseline.join("v1"), &next, &out, &m.l.f.anchor())
            .unwrap_err()
            .to_string()
            .contains("replace signed legacy")
    );
    assert!(!out.exists());
}

#[test]
fn migration_detects_removed_or_corrupted_resident_archive_before_export() {
    let m = Migration::new();
    let pin = digest(&fs::read(m.store.join("current.json")).unwrap());
    let file = m.store.join("legacy/legacy-history.json");
    let raw = fs::read(&file).unwrap();
    fs::write(&file, "corrupted").unwrap();
    let out = m.l.f.root.path().join("refused export");
    assert!(hosting::export(&m.store, &out, &pin, &m.l.f.anchor()).is_err());
    assert!(!out.exists());
    assert!(export_with(&m.store, &out, &pin, &m.l.f.anchor()).is_err());
    assert!(!out.exists());
    fs::write(file, raw).unwrap();
    hosting::export(&m.store, &out, &pin, &m.l.f.anchor()).unwrap();
}

#[test]
fn migration_normal_publication_retains_archive_and_signed_reservations_after_initial_handoff() {
    let m = Migration::new();
    let initial = m.committed();
    let pointer = digest(&fs::read(m.store.join("current.json")).unwrap());
    let update = m.next("normal publication next", "2.0.0");
    let assembly = m.l.f.root.path().join("normal assembly");
    assemble_with(&m.baseline.join("v1"), &update, &assembly, &m.l.f.anchor()).unwrap();
    promote_with(&assembly, &m.store, &pointer, &m.l.f.anchor()).unwrap();
    let pin = digest(&fs::read(m.store.join("current.json")).unwrap());
    let bundle = m.l.f.root.path().join("normal bundle");
    hosting::export(&m.store, &bundle, &pin, &m.l.f.anchor()).unwrap();
    Publication::install(&bundle, &m.l.repo);
    // The legacy copy already exists unchanged; only normal metadata advances.
    git(&m.l.repo, &["add", "."]);
    git(&m.l.repo, &["add", "--renormalize", "."]);
    git(&m.l.repo, &["commit", "-m", "normal new tools version"]);
    let candidate = git(&m.l.repo, &["rev-parse", "HEAD"]);
    let out = m.l.f.root.path().join("normal handoff");
    let bundle_pin = digest(&fs::read(bundle.join("bundle.json")).unwrap());
    let request = Request {
        checkout: &m.l.repo,
        repository: "example/registry",
        branch: "release",
        base_commit: &initial,
        candidate_commit: &candidate,
        previous: &m.bundle,
        previous_pin: &m.bundle_pin,
        bundle: &bundle,
        bundle_pin: &bundle_pin,
        out: &out,
    };
    publication::prepare_with(&request, &m.l.f.anchor()).unwrap();
    assert!(out.join("publication.json").is_file());
    assert_eq!(
        fs::read(bundle.join("legacy/legacy-history.json")).unwrap(),
        fs::read(m.bundle.join("legacy/legacy-history.json")).unwrap()
    );
}
