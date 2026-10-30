use super::*;

#[test]
fn protected_publisher_bootstraps_and_renews_without_credential_failures_writing_output() {
    let f = Fixture::new();
    let seed = empty_seed(&f);
    let unlock = crate::signing_key::password_file(
        &f.root.path().join("operator-password"),
        "controlled publisher password",
    );
    crate::keygen(f.root.path().join("keys"), "protected".into(), &unlock).unwrap();
    let key_path = f.root.path().join("keys/protected.key");
    let mut roots = f.json(&seed, "roots.json");
    roots["publishing_key"] = Fixture::public(f.root.path(), "protected").into();
    f.signed(&seed, "roots.json", &roots, "root");
    let key = crate::signing_key::load(&key_path, &unlock).unwrap();
    let revocations = fs::read(seed.join("revocations.json")).unwrap();
    fs::write(
        seed.join("revocations.json.minisig"),
        crate::signing_key::sign(&key, &revocations).unwrap(),
    )
    .unwrap();
    let mut index = f.json(&seed, "index.json");
    index["generation"] = serde_json::to_value(grain_core::trust::metadata_generation(
        &fs::read(seed.join("roots.json")).unwrap(),
        &revocations,
    ))
    .unwrap();
    let bytes = serde_json::to_vec(&index).unwrap();
    fs::write(
        seed.join("index.json.minisig"),
        crate::signing_key::sign(&key, &bytes).unwrap(),
    )
    .unwrap();
    fs::write(seed.join("index.json"), &bytes).unwrap();
    let out = f.root.path().join("protected-bootstrap");
    bootstrap::create_with(&seed, &key_path, 1, &out, &f.anchor(), &unlock).unwrap();
    let tree = check(&out.join("v1"), &f.anchor(), true).unwrap();
    assert_eq!(tree.state.index_version, 2);
    let refused = f.root.path().join("bad-password-output");
    let wrong =
        crate::signing_key::password_file(&f.root.path().join("wrong-password"), "wrong password");
    assert!(bootstrap::create_with(&seed, &key_path, 1, &refused, &f.anchor(), &wrong).is_err());
    assert!(!refused.exists());
    let renewal = f.root.path().join("protected-renewal");
    assert!(renewal::renew_with(
        &out.join("v1"),
        &tree.state.snapshot,
        &key_path,
        30,
        &renewal,
        &f.anchor(),
        &wrong
    )
    .is_err());
    assert!(!renewal.exists());
    renewal::renew_with(
        &out.join("v1"),
        &tree.state.snapshot,
        &key_path,
        30,
        &renewal,
        &f.anchor(),
        &unlock,
    )
    .unwrap();
    let next = check(&renewal.join("v1"), &f.anchor(), true).unwrap();
    assert_eq!(
        (next.state.index_version, next.state.revocations_version),
        (3, 3)
    );
    assert_eq!(fs::read(seed.join("index.json")).unwrap(), bytes);
}

pub(super) fn empty_seed(f: &Fixture) -> PathBuf {
    let seed = f.tree("current seed", 1, false);
    let mut index = f.json(&seed, "index.json");
    index["entries"] = json!([]);
    f.signed(&seed, "index.json", &index, "publisher");
    for folder in ["blob", "media"] {
        for item in fs::read_dir(seed.join(folder)).unwrap() {
            fs::remove_file(item.unwrap().path()).unwrap();
        }
    }
    seed
}

#[test]
fn clean_bootstrap_signs_empty_catalogue_and_uses_normal_store_renewal_hosting() {
    let f = Fixture::new();
    let seed = empty_seed(&f);
    let roots = fs::read(seed.join("roots.json")).unwrap();
    let mut revocations = f.json(&seed, "revocations.json");
    revocations["entries"] =
        json!([{"id":"com.example.current-policy","state":"revoked","reason":"Current seed rule"}]);
    f.signed(&seed, "revocations.json", &revocations, "publisher");
    f.bind(&seed, "publisher");
    let out = f.root.path().join("clean bootstrap");
    bootstrap::create_with(
        &seed,
        &f.root.path().join("keys/publisher.key"),
        1,
        &out,
        &f.anchor(),
        &crate::Unlock::development(),
    )
    .unwrap();
    let tree = check(&out.join("v1"), &f.anchor(), true).unwrap();
    assert!(tree.index.entries.is_empty() && tree.assets.is_empty());
    assert_eq!(
        (tree.state.index_version, tree.state.revocations_version),
        (2, 2)
    );
    assert_eq!(tree.docs["roots.json"], roots);
    assert_eq!(
        f.json(&out.join("v1"), "revocations.json")["entries"],
        revocations["entries"]
    );
    assert_eq!(fs::read_dir(&out).unwrap().count(), 1);
    let store = f.root.path().join("clean store");
    initialize_with(&out.join("v1"), &store, &f.anchor()).unwrap();
    let pointer = digest(&fs::read(store.join("current.json")).unwrap());
    let renewed = f.root.path().join("renewed current catalogue");
    renewal::renew_with(
        &out.join("v1"),
        &tree.state.snapshot,
        &f.root.path().join("keys/publisher.key"),
        30,
        &renewed,
        &f.anchor(),
        &crate::Unlock::development(),
    )
    .unwrap();
    promote_with(&renewed, &store, &pointer, &f.anchor()).unwrap();
    let pointer = digest(&fs::read(store.join("current.json")).unwrap());
    let bundle = f.root.path().join("clean hosting");
    hosting::export(&store, &bundle, &pointer, &f.anchor()).unwrap();
    let pin = digest(&fs::read(bundle.join("bundle.json")).unwrap());
    hosting::verify(&bundle, &pin, &f.anchor()).unwrap();
    assert!(!bundle.join("legacy").exists());
}

#[test]
fn clean_bootstrap_rejects_nonempty_expired_or_exhausted_seed_before_key() {
    let f = Fixture::new();
    let seed = f.tree("nonempty seed", 1, false);
    let out = f.root.path().join("refused");
    let missing = f.root.path().join("never-key");
    assert!(bootstrap::create_with(
        &seed,
        &missing,
        30,
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
    .unwrap_err()
    .to_string()
    .contains("empty current app seed"));
    let seed = empty_seed(&f);
    let mut index = f.json(&seed, "index.json");
    index["version"] = json!(u64::MAX);
    f.signed(&seed, "index.json", &index, "publisher");
    assert!(bootstrap::create_with(
        &seed,
        &missing,
        30,
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
    .unwrap_err()
    .to_string()
    .contains("version exhausted"));
    let mut roots = f.json(&seed, "roots.json");
    roots["expires"] = json!("2000-01-01T00:00:00Z");
    f.signed(&seed, "roots.json", &roots, "root");
    assert!(bootstrap::create_with(
        &seed,
        &missing,
        30,
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
    .is_err());
    assert!(!out.exists());
}

#[test]
fn clean_bootstrap_rejects_invalid_lifetime_containment_and_wrong_publisher_without_output() {
    let f = Fixture::new();
    let seed = empty_seed(&f);
    let original = fs::read(seed.join("index.json")).unwrap();
    let key = f.root.path().join("keys/rotated.key");
    let out = f.root.path().join("refused");
    for days in [0, 31] {
        assert!(bootstrap::create_with(
            &seed,
            &key,
            days,
            &out,
            &f.anchor(),
            &crate::Unlock::development()
        )
        .unwrap_err()
        .to_string()
        .contains("lifetime"));
    }
    assert!(bootstrap::create_with(
        &seed,
        &key,
        30,
        &seed.join("inside"),
        &f.anchor(),
        &crate::Unlock::development()
    )
    .unwrap_err()
    .to_string()
    .contains("outside"));
    assert!(bootstrap::create_with(
        &seed,
        &key,
        30,
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
    .unwrap_err()
    .to_string()
    .contains("signature does not verify"));
    assert!(!out.exists());
    assert_eq!(fs::read(seed.join("index.json")).unwrap(), original);
}
