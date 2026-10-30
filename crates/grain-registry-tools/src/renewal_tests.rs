use super::*;
use crate::serving::renewal::{renew_with, rewrite_with, MetadataChange};

#[test]
fn exact_revocation_preserves_history_and_leaves_other_versions_available() {
    let f = Fixture::new();
    // Far-future metadata proves revocation does not wait for a renewal window.
    let base = f.tree("base", 4, true);
    let mut index = f.json(&base, "index.json");
    let mut corrected = index["entries"][1].clone();
    corrected["version"] = "1.0.1".into();
    index["entries"].as_array_mut().unwrap().push(corrected);
    index["entries"][1]["future_entry"] = json!({"preserved":true});
    f.signed(&base, "index.json", &index, "publisher");
    let mut rules = f.json(&base, "revocations.json");
    rules["future_envelope"] = json!({"preserved":true});
    rules["entries"] = json!([{"id":"com.example.mcp","version":"1.0.0","state":"deprecated","reason":"prior notice","future_rule":true}]);
    f.signed(&base, "revocations.json", &rules, "publisher");
    f.bind(&base, "publisher");
    let old = check(&base, &f.anchor(), true).unwrap();
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let pointer_pin = digest(&fs::read(store.join("current.json")).unwrap());
    let out = f.root.path().join("revoked");
    rewrite_with(
        &base,
        &old.state.snapshot,
        &f.root.path().join("keys/publisher.key"),
        MetadataChange::Revoke {
            days: 30,
            id: "com.example.mcp",
            version: "1.0.0",
            reason: "Broken release",
        },
        &out,
        &f.anchor(),
        &crate::Unlock::development(),
    )
    .unwrap();
    let next = check(&out.join("v1"), &f.anchor(), true).unwrap();
    assert_eq!(next.state.index_version, 5);
    assert_eq!(next.state.revocations_version, 2);
    assert_eq!(
        next.revocations.state_for("com.example.mcp", "1.0.0"),
        Some(RevocationState::Revoked)
    );
    assert_eq!(next.revocations.state_for("com.example.mcp", "1.0.1"), None);
    assert_eq!(
        next.revocations.state_for("com.example.native", "1.0.0"),
        None
    );
    let mut before: Value = serde_json::from_slice(&old.docs["index.json"]).unwrap();
    let mut after = f.json(&out.join("v1"), "index.json");
    assert_eq!(before["expires"], after["expires"]);
    without_freshness(&mut before);
    without_freshness(&mut after);
    assert_eq!(before, after);
    let after_rules = f.json(&out.join("v1"), "revocations.json");
    assert_eq!(after_rules["entries"][0], rules["entries"][0]);
    assert_eq!(after_rules["future_envelope"], rules["future_envelope"]);
    for (name, bytes) in &old.docs {
        assert_eq!(fs::read(base.join(name)).unwrap(), *bytes);
    }
    for name in old.assets.keys() {
        assert_eq!(
            fs::read(base.join(name)).unwrap(),
            fs::read(out.join("v1").join(name)).unwrap()
        );
    }
    assert_eq!(old.docs["roots.json"], next.docs["roots.json"]);
    promote_with(&out, &store, &pointer_pin, &f.anchor()).unwrap();
    let current = fs::read(store.join("current.json")).unwrap();
    assert!(promote_with(&out, &store, &pointer_pin, &f.anchor()).is_err());
    assert_eq!(current, fs::read(store.join("current.json")).unwrap());
    let bundle = f.root.path().join("bundle");
    hosting::export(&store, &bundle, &digest(&current), &f.anchor()).unwrap();
    hosting::verify(
        &bundle,
        &digest(&fs::read(bundle.join("bundle.json")).unwrap()),
        &f.anchor(),
    )
    .unwrap();
}

#[test]
fn revocation_refuses_unknown_already_revoked_and_invalid_reason_before_key_access() {
    let f = Fixture::new();
    let base = renewable(&f, "base", true);
    let pin = check(&base, &f.anchor(), false).unwrap().state.snapshot;
    let out = f.root.path().join("refused");
    for (id, version, reason, error) in [
        ("com.example.missing", "1.0.0", "broken", "exact release"),
        ("com.example.mcp", "9.0.0", "broken", "exact release"),
        ("com.example.native", "1.0.0", "broken", "already revoked"),
        ("com.example.mcp", "1.0.0", "  ", "plain-text"),
        ("com.example.mcp", "1.0.0", "bad\nreason", "plain-text"),
        ("com.example.mcp", "1.0.0", &"x".repeat(1025), "plain-text"),
    ] {
        let actual = rewrite_with(
            &base,
            &pin,
            &f.root.path().join("never-open-key"),
            MetadataChange::Revoke {
                days: 30,
                id,
                version,
                reason,
            },
            &out,
            &f.anchor(),
            &crate::Unlock::development(),
        )
        .unwrap_err()
        .to_string();
        assert!(actual.contains(error), "expected {error}, got {actual}");
        assert!(!out.exists());
    }
}

#[test]
fn native_revocation_recovers_expired_metadata_and_rejects_duplicate_or_wrong_signer() {
    let f = Fixture::new();
    let base = renewable(&f, "base", true);
    let mut rules = f.json(&base, "revocations.json");
    rules["entries"][0]["state"] = "deprecated".into();
    f.signed(&base, "revocations.json", &rules, "publisher");
    f.bind(&base, "publisher");
    let old = check(&base, &f.anchor(), false).unwrap();
    let out = f.root.path().join("revoked");
    let change = || MetadataChange::Revoke {
        days: 30,
        id: "com.example.native",
        version: "1.0.0",
        reason: "Broken native release",
    };
    assert!(rewrite_with(
        &base,
        &old.state.snapshot,
        &f.root.path().join("keys/rotated.key"),
        change(),
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
    .is_err());
    assert!(!out.exists());
    rewrite_with(
        &base,
        &old.state.snapshot,
        &f.root.path().join("keys/publisher.key"),
        change(),
        &out,
        &f.anchor(),
        &crate::Unlock::development(),
    )
    .unwrap();
    let next = check(&out.join("v1"), &f.anchor(), true).unwrap();
    let refused = f.root.path().join("duplicate");
    let error = rewrite_with(
        &out.join("v1"),
        &next.state.snapshot,
        &f.root.path().join("never-open-key"),
        change(),
        &refused,
        &f.anchor(),
        &crate::Unlock::development(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("already revoked"));
    assert!(!refused.exists());
}

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
    f.bind(&path, "publisher");
    path
}

fn without_freshness(value: &mut Value) {
    value.as_object_mut().unwrap().remove("version");
    value.as_object_mut().unwrap().remove("expires");
    value.as_object_mut().unwrap().remove("generation");
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
        &crate::Unlock::development(),
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
        &crate::Unlock::development(),
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
        assert!(renew_with(
            &base,
            pin,
            &missing_key,
            days,
            &out,
            &f.anchor(),
            &crate::Unlock::development()
        )
        .unwrap_err()
        .to_string()
        .contains(error));
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
        &f.anchor(),
        &crate::Unlock::development()
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
        &f.anchor(),
        &crate::Unlock::development()
    )
    .is_err());
    assert!(!out.exists());
    let key = f.root.path().join("oversized.key");
    fs::write(&key, vec![b'x'; 8193]).unwrap();
    assert!(renew_with(
        &base,
        &pin,
        &key,
        30,
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
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
        f.bind(&base, "publisher");
        let pin = check(&base, &f.anchor(), false).unwrap().state.snapshot;
        assert!(renew_with(
            &base,
            &pin,
            &key,
            30,
            &out,
            &f.anchor(),
            &crate::Unlock::development()
        )
        .unwrap_err()
        .to_string()
        .contains("version exhausted"));
        f.signed(&base, name, &saved, "publisher");
        f.bind(&base, "publisher");
    }
    let mut roots = f.json(&base, "roots.json");
    roots["expires"] = "2020-01-01T00:00:00Z".into();
    f.signed(&base, "roots.json", &roots, "root");
    f.bind(&base, "publisher");
    let pin = check(&base, &f.anchor(), false).unwrap().state.snapshot;
    assert!(renew_with(
        &base,
        &pin,
        &key,
        30,
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
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
    assert!(renew_with(
        &base,
        &pin,
        &key,
        30,
        &base.join("nested"),
        &f.anchor(),
        &crate::Unlock::development()
    )
    .unwrap_err()
    .to_string()
    .contains("outside"));
    let out = f.root.path().join("existing");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("keep"), "owned by someone else").unwrap();
    assert!(renew_with(
        &base,
        &pin,
        &key,
        30,
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
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
    assert!(renew_with(
        &future,
        &pin,
        &key,
        30,
        &refuse,
        &f.anchor(),
        &crate::Unlock::development()
    )
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
    assert!(renew_with(
        &base,
        &old.state.snapshot,
        &key,
        30,
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
    .unwrap_err()
    .to_string()
    .contains("signed hash/size"));
    fs::write(path, before).unwrap();
    let roots = f.json(&base, "roots.json");
    f.signed(&base, "roots.json", &roots, "rotated");
    assert!(renew_with(
        &base,
        &old.state.snapshot,
        &key,
        30,
        &out,
        &f.anchor(),
        &crate::Unlock::development()
    )
    .is_err());
    assert!(!out.exists());
}
