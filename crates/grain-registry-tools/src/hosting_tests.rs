//! Signed archive handoff tests reuse the serving fixtures, not another runtime.
use super::*;

fn withdrawn(f: &Fixture) -> (PathBuf, PathBuf, PathBuf) {
    let base = f.tree("base", 1, true);
    let next = f.tree("next", 2, false);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let assembly = f.root.path().join("assembly");
    assemble_with(&base, &next, &assembly, &f.anchor()).unwrap();
    let pin = digest(&fs::read(store.join("current.json")).unwrap());
    promote_with(&assembly, &store, &pin, &f.anchor()).unwrap();
    (store, base, next)
}

fn receipt_pin(bundle: &Path) -> String {
    digest(&fs::read(bundle.join("bundle.json")).unwrap())
}

#[test]
fn bundle_keeps_withdrawn_assets_and_exact_current_metadata_with_signed_proofs() {
    let f = Fixture::new();
    let (store, base, next) = withdrawn(&f);
    let before = fs::read(store.join("current.json")).unwrap();
    let bundle = f.root.path().join("hosting");
    hosting::export(&store, &bundle, &digest(&before), &f.anchor()).unwrap();
    let pin = receipt_pin(&bundle);
    let state = hosting::verify(&bundle, &pin, &f.anchor()).unwrap();
    assert_eq!(state.index_version, 2);
    let old = check(&base, &f.anchor(), true).unwrap();
    for name in old.assets.keys() {
        assert_eq!(
            fs::read(bundle.join("v1").join(name)).unwrap(),
            fs::read(base.join(name)).unwrap()
        );
    }
    for name in DOCS {
        assert_eq!(
            fs::read(bundle.join("v1").join(name)).unwrap(),
            fs::read(next.join(name)).unwrap()
        );
        assert_eq!(
            fs::read(bundle.join("history").join(&old.state.snapshot).join(name)).unwrap(),
            old.docs[name]
        );
    }
    assert_eq!(fs::read(bundle.join("current.json")).unwrap(), before);
    assert_eq!(
        metadata(&bundle.join("v1"), &f.anchor(), true)
            .unwrap()
            .index
            .entries
            .len(),
        1
    );
    // The archive-aware verifier owns this layout; the strict single-snapshot
    // verifier correctly refuses archive assets absent from the active index.
    assert!(check(&bundle.join("v1"), &f.anchor(), true).is_err());
    let mut empty = f.json(&next, "index.json");
    empty["version"] = json!(3);
    empty["entries"] = json!([]);
    let update = f.root.path().join("empty-update");
    fs::create_dir(&update).unwrap();
    f.signed(&update, "index.json", &empty, "publisher");
    let assembly = f.root.path().join("empty-assembly");
    assemble_with(&next, &update, &assembly, &f.anchor()).unwrap();
    promote_with(&assembly, &store, &digest(&before), &f.anchor()).unwrap();
    assert_eq!(hosting::verify(&bundle, &pin, &f.anchor()).unwrap(), state);
    let newest = f.root.path().join("newest-hosting");
    hosting::export(
        &store,
        &newest,
        &digest(&fs::read(store.join("current.json")).unwrap()),
        &f.anchor(),
    )
    .unwrap();
    assert_eq!(
        hosting::verify(&newest, &receipt_pin(&newest), &f.anchor())
            .unwrap()
            .index_version,
        3
    );
    for name in old.assets.keys() {
        assert!(newest.join("v1").join(name).exists());
    }
}

#[test]
fn bundle_refuses_tampered_or_missing_archives_pointers_proofs_and_extra_inputs() {
    let f = Fixture::new();
    let (store, base, _) = withdrawn(&f);
    let bundle = f.root.path().join("hosting");
    let before = fs::read(store.join("current.json")).unwrap();
    hosting::export(&store, &bundle, &digest(&before), &f.anchor()).unwrap();
    let pin = receipt_pin(&bundle);
    let old = check(&base, &f.anchor(), true).unwrap();
    let archive = old
        .assets
        .keys()
        .find(|v| v.ends_with(".mcp.json"))
        .unwrap();
    let asset_path = bundle.join("v1").join(archive);
    let asset_bytes = fs::read(&asset_path).unwrap();
    fs::write(&asset_path, b"tampered archive").unwrap();
    assert!(hosting::verify(&bundle, &pin, &f.anchor()).is_err());
    fs::remove_file(&asset_path).unwrap();
    assert!(hosting::verify(&bundle, &pin, &f.anchor()).is_err());
    fs::write(asset_path, asset_bytes).unwrap();
    for path in [
        bundle.join("current.json"),
        bundle.join("v1/index.json"),
        bundle
            .join("history")
            .join(&old.state.snapshot)
            .join("index.json"),
    ] {
        let original = fs::read(&path).unwrap();
        fs::write(&path, [original.as_slice(), b" "].concat()).unwrap();
        assert!(hosting::verify(&bundle, &pin, &f.anchor()).is_err());
        fs::write(path, original).unwrap();
    }
    for path in [
        bundle.join("extra.txt"),
        bundle.join("v1/media/extra.md"),
        bundle
            .join("history")
            .join(&old.state.snapshot)
            .join("extra.txt"),
    ] {
        fs::write(&path, b"extra").unwrap();
        assert!(hosting::verify(&bundle, &pin, &f.anchor()).is_err());
        fs::remove_file(path).unwrap();
    }
    let proof = bundle.join("history").join(&old.state.snapshot);
    let backup = f.root.path().join("proof-backup");
    fs::rename(&proof, &backup).unwrap();
    assert!(hosting::verify(&bundle, &pin, &f.anchor()).is_err());
    fs::rename(backup, proof).unwrap();
    hosting::verify(&bundle, &pin, &f.anchor()).unwrap();
    assert_eq!(fs::read(store.join("current.json")).unwrap(), before);
}

#[test]
fn receipt_is_independently_pinned_and_cannot_override_signed_selection_or_history() {
    let f = Fixture::new();
    let (store, _, _) = withdrawn(&f);
    let bundle = f.root.path().join("hosting");
    hosting::export(
        &store,
        &bundle,
        &digest(&fs::read(store.join("current.json")).unwrap()),
        &f.anchor(),
    )
    .unwrap();
    let file = bundle.join("bundle.json");
    let original = fs::read(&file).unwrap();
    let pin = digest(&original);
    assert!(hosting::verify(&bundle, &"a".repeat(64), &f.anchor()).is_err());
    assert!(hosting::verify(&bundle, "bad", &f.anchor()).is_err());
    let receipt: Value = serde_json::from_slice(&original).unwrap();
    for pointer in [
        "/schema",
        "/current_sha256",
        "/selected/index_version",
        "/history/0",
    ] {
        let mut changed = receipt.clone();
        *changed.pointer_mut(pointer).unwrap() =
            if pointer == "/schema" || pointer == "/selected/index_version" {
                json!(9)
            } else {
                json!("../escape")
            };
        let bytes = serde_json::to_vec(&changed).unwrap();
        fs::write(&file, &bytes).unwrap();
        assert!(hosting::verify(&bundle, &pin, &f.anchor()).is_err());
        assert!(hosting::verify(&bundle, &digest(&bytes), &f.anchor()).is_err());
    }
    let mut duplicate = receipt.clone();
    let id = duplicate["history"][0].clone();
    duplicate["history"].as_array_mut().unwrap().push(id);
    let bytes = serde_json::to_vec(&duplicate).unwrap();
    fs::write(&file, &bytes).unwrap();
    assert!(hosting::verify(&bundle, &digest(&bytes), &f.anchor()).is_err());
    fs::write(file, original).unwrap();
    hosting::verify(&bundle, &pin, &f.anchor()).unwrap();
}

#[test]
fn corrupt_source_archive_cleans_only_owned_output_and_preserves_current() {
    let f = Fixture::new();
    let (store, base, _) = withdrawn(&f);
    let old = check(&base, &f.anchor(), true).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    let name = old
        .assets
        .keys()
        .find(|v| v.ends_with(".mcp.json"))
        .unwrap();
    let source = store.join("snapshots").join(&old.state.snapshot).join(name);
    let original = fs::read(&source).unwrap();
    fs::write(&source, b"bad archived source").unwrap();
    let out = f.root.path().join("failed-hosting");
    assert!(hosting::export(&store, &out, &digest(&before), &f.anchor()).is_err());
    assert!(!out.exists());
    assert_eq!(fs::read(store.join("current.json")).unwrap(), before);
    fs::write(source, original).unwrap();
    let held = lock(&store).unwrap();
    assert!(hosting::export(&store, &out, &digest(&before), &f.anchor())
        .unwrap_err()
        .to_string()
        .contains("lock"));
    drop(held);
    assert!(!out.exists());
    assert!(hosting::export(&store, &out, &"a".repeat(64), &f.anchor()).is_err());
    assert!(hosting::export(&store, &store.join("inside"), &digest(&before), &f.anchor()).is_err());
    hosting::export(&store, &out, &digest(&before), &f.anchor()).unwrap();
    let pin = receipt_pin(&out);
    assert!(hosting::export(&store, &out, &digest(&before), &f.anchor()).is_err());
    hosting::verify(&out, &pin, &f.anchor()).unwrap();
}

#[test]
fn conflicting_history_refuses_even_when_both_versions_are_currently_withdrawn() {
    let f = Fixture::new();
    let (store, base, next) = withdrawn(&f);
    let mut empty = f.json(&next, "index.json");
    empty["version"] = json!(3);
    empty["entries"] = json!([]);
    let update = f.root.path().join("empty-update");
    fs::create_dir(&update).unwrap();
    f.signed(&update, "index.json", &empty, "publisher");
    let assembly = f.root.path().join("empty-assembly");
    assemble_with(&next, &update, &assembly, &f.anchor()).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    promote_with(&assembly, &store, &digest(&before), &f.anchor()).unwrap();
    // Model inconsistent legacy/imported history, not an admitted promotion.
    let mut changed = f.json(&base, "index.json");
    changed["version"] = json!(4);
    changed["entries"][0]["sha256"] = json!("a".repeat(64));
    f.signed(&base, "index.json", &changed, "publisher");
    let inconsistent = metadata(&base, &f.anchor(), true).unwrap();
    let proof = store.join("snapshots").join(&inconsistent.state.snapshot);
    fs::create_dir(&proof).unwrap();
    for (name, bytes) in &inconsistent.docs {
        fs::write(proof.join(name), bytes).unwrap();
    }
    let current = fs::read(store.join("current.json")).unwrap();
    let out = f.root.path().join("conflicting-hosting");
    assert!(
        hosting::export(&store, &out, &digest(&current), &f.anchor())
            .unwrap_err()
            .to_string()
            .contains("Conflicting historical")
    );
    assert!(!out.exists());
    assert_eq!(fs::read(store.join("current.json")).unwrap(), current);
}

#[test]
fn rotated_current_key_accepts_expired_history_but_expired_active_metadata_refuses() {
    let f = Fixture::new();
    let base = f.tree("base", 1, true);
    let next = f.tree("next", 2, false);
    for name in ["roots.json", "index.json", "revocations.json"] {
        let mut past = f.json(&base, name);
        past["expires"] = json!((Utc::now() - chrono::Duration::days(2)).to_rfc3339());
        f.signed(
            &base,
            name,
            &past,
            if name == "roots.json" {
                "root"
            } else {
                "publisher"
            },
        );
        let mut fresh = f.json(&next, name);
        fresh["version"] = json!(2);
        if name == "roots.json" {
            fresh["publishing_key"] = json!(Fixture::public(f.root.path(), "rotated"));
        }
        f.signed(
            &next,
            name,
            &fresh,
            if name == "roots.json" {
                "root"
            } else {
                "rotated"
            },
        );
    }
    let store = f.root.path().join("store");
    initialize_with(&next, &store, &f.anchor()).unwrap();
    let historical = check(&base, &f.anchor(), false).unwrap();
    copy(
        &historical,
        &store.join("snapshots").join(&historical.state.snapshot),
    )
    .unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    let bundle = f.root.path().join("hosting");
    hosting::export(&store, &bundle, &digest(&before), &f.anchor()).unwrap();
    hosting::verify(&bundle, &receipt_pin(&bundle), &f.anchor()).unwrap();
    let mut expired = f.json(&bundle.join("v1"), "index.json");
    expired["expires"] = json!((Utc::now() - chrono::Duration::days(2)).to_rfc3339());
    f.signed(&bundle.join("v1"), "index.json", &expired, "rotated");
    let aged = metadata(&bundle.join("v1"), &f.anchor(), false).unwrap();
    let pointer = serde_json::to_vec(&aged.state).unwrap();
    fs::write(bundle.join("current.json"), &pointer).unwrap();
    let file = bundle.join("bundle.json");
    let mut receipt: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    receipt["selected"] = serde_json::to_value(&aged.state).unwrap();
    receipt["current_sha256"] = json!(digest(&pointer));
    fs::write(&file, serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert!(hosting::verify(&bundle, &receipt_pin(&bundle), &f.anchor()).is_err());
    assert_eq!(fs::read(store.join("current.json")).unwrap(), before);
}
