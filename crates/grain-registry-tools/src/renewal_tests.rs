use super::*;
use crate::serving::renewal::renew_with;

fn renewable(f: &Fixture, name: &str, expired: bool) -> PathBuf {
    let path = f.tree(name, 4, true);
    let expires = (Utc::now() + chrono::Duration::days(if expired { -1 } else { 1 })).to_rfc3339();
    let mut index = f.json(&path, "index.json");
    index["expires"] = expires.clone().into();
    index["entries"][0]["future_entry"] = json!({"unchanged": [1, 2]});
    f.signed(&path, "index.json", &index, "publisher");
    f.signed(
        &path,
        "revocations.json",
        &json!({"spec":1,"version":7,"expires":expires,
            "entries":[{"id":"com.example.native","state":"revoked","reason":"controlled","future_rule":true}],
            "future_envelope":{"retained":true}}),
        "publisher",
    );
    path
}

fn without_freshness(value: &mut Value) {
    value.as_object_mut().unwrap().remove("version");
    value.as_object_mut().unwrap().remove("expires");
}

#[test]
fn renewal_recovers_expired_metadata_preserving_rules_assets_and_future_fields() {
    let f = Fixture::new();
    let base = renewable(&f, "base", true);
    let old = check(&base, &f.anchor(), false).unwrap();
    assert!(check(&base, &f.anchor(), true).is_err());
    let out = f.root.path().join("renewed");
    renew_with(
        &base,
        &old.state.snapshot,
        &f.root.path().join("keys/publisher.key"),
        30,
        &out,
        &f.anchor(),
    )
    .unwrap();
    let next = check(&out.join("v1"), &f.anchor(), true).unwrap();
    assert_eq!(next.state.index_version, 5);
    assert_eq!(next.state.revocations_version, 8);
    assert_eq!(
        next.revocations.state_for("com.example.native", "2.0.0"),
        Some(RevocationState::Revoked)
    );
    for name in ["index.json", "revocations.json"] {
        let mut before: Value = serde_json::from_slice(&old.docs[name]).unwrap();
        let mut after: Value = serde_json::from_slice(&next.docs[name]).unwrap();
        without_freshness(&mut before);
        without_freshness(&mut after);
        assert_eq!(after, before, "only version/expiry may change: {name}");
        assert_eq!(fs::read(base.join(name)).unwrap(), old.docs[name]);
    }
    for name in ["roots.json", "roots.json.minisig"] {
        assert_eq!(next.docs[name], old.docs[name]);
    }
    assert_eq!(
        next.assets.keys().collect::<Vec<_>>(),
        old.assets.keys().collect::<Vec<_>>()
    );
    for name in old.assets.keys() {
        assert_eq!(
            fs::read(base.join(name)).unwrap(),
            fs::read(out.join("v1").join(name)).unwrap()
        );
    }
    let parent: Parent =
        serde_json::from_slice(&fs::read(out.join("promotion.json")).unwrap()).unwrap();
    assert!(parent.matches(&old.state));

    // Model an authentic snapshot already expired in a protected store. Fresh
    // initialization refuses it; no public CLI accepts this test-only bootstrap.
    let store = f.root.path().join("store");
    fs::create_dir(&store).unwrap();
    fs::create_dir(store.join("snapshots")).unwrap();
    copy(&old, &store.join("snapshots").join(&old.state.snapshot)).unwrap();
    let pin = pointer(&store, &old.state, true).unwrap();
    promote_with(&out, &store, &pin, &f.anchor()).unwrap();
    assert!(store.join("snapshots").join(&old.state.snapshot).is_dir());
    let new_pin = digest(&fs::read(store.join("current.json")).unwrap());
    let bundle = f.root.path().join("hosting");
    hosting::export(&store, &bundle, &new_pin, &f.anchor()).unwrap();
    let receipt = digest(&fs::read(bundle.join("bundle.json")).unwrap());
    hosting::verify(&bundle, &receipt, &f.anchor()).unwrap();
}

#[test]
fn renewal_uses_existing_stale_parent_promotion_guard_for_fresh_sources() {
    let f = Fixture::new();
    let base = renewable(&f, "base", false);
    let old = check(&base, &f.anchor(), true).unwrap();
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let pin = digest(&fs::read(store.join("current.json")).unwrap());
    let out = f.root.path().join("renewed");
    renew_with(
        &base,
        &old.state.snapshot,
        &f.root.path().join("keys/publisher.key"),
        30,
        &out,
        &f.anchor(),
    )
    .unwrap();
    promote_with(&out, &store, &pin, &f.anchor()).unwrap();
    let after = fs::read(store.join("current.json")).unwrap();
    assert!(promote_with(&out, &store, &pin, &f.anchor())
        .unwrap_err()
        .to_string()
        .contains("pointer changed"));
    assert_eq!(fs::read(store.join("current.json")).unwrap(), after);
}

#[test]
fn renewal_refuses_stale_pins_and_lifetime_errors_before_key_access() {
    let f = Fixture::new();
    let base = renewable(&f, "base", true);
    let old = check(&base, &f.anchor(), false).unwrap();
    let missing_key = f.root.path().join("key-must-not-be-opened");
    let out = f.root.path().join("refused");
    for (pin, days, error) in [
        ("bad", 30, "independent snapshot"),
        (old.state.snapshot.as_str(), 0, "one to thirty"),
        (old.state.snapshot.as_str(), 31, "one to thirty"),
        ("a".repeat(64).as_str(), 30, "snapshot changed"),
    ] {
        assert!(
            renew_with(&base, pin, &missing_key, days, &out, &f.anchor())
                .unwrap_err()
                .to_string()
                .contains(error)
        );
        assert!(!out.exists());
    }
    let mut altered = f.json(&base, "index.json");
    altered["future_envelope"] = json!({"changed":true});
    f.signed(&base, "index.json", &altered, "publisher");
    assert!(renew_with(
        &base,
        &old.state.snapshot,
        &missing_key,
        30,
        &out,
        &f.anchor()
    )
    .unwrap_err()
    .to_string()
    .contains("snapshot changed"));
    assert!(!out.exists());
}

#[test]
fn renewal_refuses_wrong_or_oversized_keys_without_emitting_output() {
    let f = Fixture::new();
    let base = renewable(&f, "base", true);
    let pin = check(&base, &f.anchor(), false).unwrap().state.snapshot;
    let out = f.root.path().join("refused");
    assert!(renew_with(
        &base,
        &pin,
        &f.root.path().join("keys/rotated.key"),
        30,
        &out,
        &f.anchor()
    )
    .is_err());
    assert!(!out.exists());
    let key = f.root.path().join("oversized.key");
    fs::write(&key, vec![b'x'; 8193]).unwrap();
    assert!(renew_with(&base, &pin, &key, 30, &out, &f.anchor())
        .unwrap_err()
        .to_string()
        .contains("oversized"));
    assert!(!out.exists());
}

#[test]
fn renewal_rejects_expired_roots_and_version_exhaustion_before_key_access() {
    let f = Fixture::new();
    let base = renewable(&f, "base", true);
    let key = f.root.path().join("key-must-not-be-opened");
    let out = f.root.path().join("refused");
    for name in ["index.json", "revocations.json"] {
        let saved = f.json(&base, name);
        let mut value = saved.clone();
        value["version"] = u64::MAX.into();
        f.signed(&base, name, &value, "publisher");
        let pin = check(&base, &f.anchor(), false).unwrap().state.snapshot;
        assert!(renew_with(&base, &pin, &key, 30, &out, &f.anchor())
            .unwrap_err()
            .to_string()
            .contains("version exhausted"));
        f.signed(&base, name, &saved, "publisher");
    }
    let mut roots = f.json(&base, "roots.json");
    roots["expires"] = "2020-01-01T00:00:00Z".into();
    f.signed(&base, "roots.json", &roots, "root");
    let pin = check(&base, &f.anchor(), false).unwrap().state.snapshot;
    assert!(renew_with(&base, &pin, &key, 30, &out, &f.anchor())
        .unwrap_err()
        .to_string()
        .contains("root-authorized"));
    assert!(!out.exists());
}

#[test]
fn renewal_does_not_shorten_expiry_or_overwrite_and_nest_output() {
    let f = Fixture::new();
    let base = renewable(&f, "base", true);
    let pin = check(&base, &f.anchor(), false).unwrap().state.snapshot;
    let key = f.root.path().join("key-must-not-be-opened");
    assert!(
        renew_with(&base, &pin, &key, 30, &base.join("nested"), &f.anchor())
            .unwrap_err()
            .to_string()
            .contains("outside")
    );
    let out = f.root.path().join("existing");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("keep"), "owned by someone else").unwrap();
    assert!(renew_with(&base, &pin, &key, 30, &out, &f.anchor())
        .unwrap_err()
        .to_string()
        .contains("already exists"));
    assert_eq!(
        fs::read(out.join("keep")).unwrap(),
        b"owned by someone else"
    );
    let future = f.tree("far-future", 1, true);
    let pin = check(&future, &f.anchor(), false).unwrap().state.snapshot;
    let refuse = f.root.path().join("refused");
    assert!(renew_with(&future, &pin, &key, 30, &refuse, &f.anchor())
        .unwrap_err()
        .to_string()
        .contains("extend both"));
    assert!(!refuse.exists());
}

#[test]
fn renewal_refuses_corrupt_assets_and_untrusted_roots_before_signing() {
    let f = Fixture::new();
    let base = renewable(&f, "base", true);
    let old = check(&base, &f.anchor(), false).unwrap();
    let key = f.root.path().join("key-must-not-be-opened");
    let out = f.root.path().join("refused");
    let name = old.assets.keys().next().unwrap();
    let path = base.join(name);
    let before = fs::read(&path).unwrap();
    fs::write(&path, "changed").unwrap();
    assert!(
        renew_with(&base, &old.state.snapshot, &key, 30, &out, &f.anchor())
            .unwrap_err()
            .to_string()
            .contains("signed hash/size")
    );
    fs::write(path, before).unwrap();
    let roots = f.json(&base, "roots.json");
    f.signed(&base, "roots.json", &roots, "rotated");
    assert!(renew_with(&base, &old.state.snapshot, &key, 30, &out, &f.anchor()).is_err());
    assert!(!out.exists());
}
