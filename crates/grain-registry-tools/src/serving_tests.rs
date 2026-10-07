//! Real signatures and filesystem operations; disposable anchors are test-only.
#[path = "bootstrap_tests.rs"]
mod bootstrap_tests;
#[path = "hosting_tests.rs"]
mod hosting_tests;
#[path = "publication_tests.rs"]
mod publication_tests;
#[path = "renewal_tests.rs"]
mod renewal_tests;
use super::*;
use serde_json::{json, Value};

struct Fixture {
    root: tempfile::TempDir,
    root_pub: String,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        for name in ["root", "publisher", "rotated"] {
            crate::keygen(root.path().join("keys"), name.into()).unwrap();
        }
        let root_pub = Self::public(root.path(), "root");
        Self { root, root_pub }
    }
    fn public(root: &Path, name: &str) -> String {
        minisign::PublicKeyBox::from_string(
            &fs::read_to_string(root.join(format!("keys/{name}.pub"))).unwrap(),
        )
        .unwrap()
        .into_public_key()
        .unwrap()
        .to_base64()
    }
    fn anchor(&self) -> impl Fn(&[u8], &str) -> Result<Roots> + '_ {
        move |bytes, sig| {
            // Verify the real signature with the client's existing verifier. The
            // fixture root also carries Index-compatible spec/version/expiry.
            let pinned = Roots {
                spec: 1,
                version: 1,
                publishing_key: self.root_pub.clone(),
                base_urls: vec![],
                mirrors: vec![],
                expires: None,
            };
            grain_core::trust::verify_index(
                &pinned,
                bytes,
                sig,
                None,
                Utc::now().timestamp(),
                false,
            )?;
            Ok(serde_json::from_slice(bytes)?)
        }
    }
    fn signed(&self, dir: &Path, name: &str, value: &Value, key: &str) {
        let mut value = value.clone();
        if name == "index.json"
            && dir.join("roots.json").is_file()
            && dir.join("revocations.json").is_file()
        {
            value["generation"] = serde_json::to_value(grain_core::trust::metadata_generation(
                &fs::read(dir.join("roots.json")).unwrap(),
                &fs::read(dir.join("revocations.json")).unwrap(),
            ))
            .unwrap();
        }
        let raw = serde_json::to_vec(&value).unwrap();
        fs::write(dir.join(name), &raw).unwrap();
        fs::write(
            dir.join(format!("{name}.minisig")),
            crate::sign_bytes(&self.root.path().join(format!("keys/{key}.key")), &raw).unwrap(),
        )
        .unwrap();
    }
    fn bind(&self, dir: &Path, key: &str) {
        self.signed(dir, "index.json", &self.json(dir, "index.json"), key);
    }
    fn tree(&self, name: &str, version: u64, mcp: bool) -> PathBuf {
        let dir = self.root.path().join(name);
        fs::create_dir(&dir).unwrap();
        fs::create_dir(dir.join("blob")).unwrap();
        fs::create_dir(dir.join("media")).unwrap();
        let expires = "2099-01-01T00:00:00Z";
        self.signed(&dir,"roots.json",&json!({"spec":1,"version":1,"publishing_key":Self::public(self.root.path(),"publisher"),"base_urls":["https://registry.example.invalid/v1/"],"expires":expires}),"root");
        self.signed(
            &dir,
            "revocations.json",
            &json!({"spec":1,"version":1,"expires":expires,"entries":[]}),
            "publisher",
        );
        let mut entries = vec![];
        for kind in if mcp {
            vec!["native", "mcp"]
        } else {
            vec!["native"]
        } {
            // Serving validates addressed bytes; package semantics are owned by
            // preparation/receiving/host, not duplicated in this assembler.
            let artifact = format!("controlled {kind} artifact").into_bytes();
            let hash = digest(&artifact);
            let ext = if kind == "native" {
                "grainpack"
            } else {
                "mcp.json"
            };
            fs::write(dir.join(format!("blob/{hash}.{ext}")), &artifact).unwrap();
            let body = format!("Description for {kind}");
            let doc = digest(body.as_bytes());
            fs::write(dir.join(format!("media/{doc}.md")), &body).unwrap();
            let image = b"controlled gif fixture";
            let image_hash = digest(image);
            fs::write(dir.join(format!("media/{image_hash}.gif")), image).unwrap();
            entries.push(json!({"id":format!("com.example.{kind}"),"name":kind,"version":"1.0.0","tier":"scripted","trust":"verified","artifact_kind":if kind=="native" {"native"} else {"mcp-descriptor"},"sha256":hash,"size":artifact.len(),"listing":{"sha256":doc,"size":body.len()},"media":[{"sha256":image_hash,"kind":"gif","size":image.len()}]}));
        }
        self.signed(&dir,"index.json",&json!({"spec":1,"version":version,"expires":expires,"entries":entries,"future_envelope":{"retained":true}}),"publisher");
        dir
    }
    fn json(&self, dir: &Path, name: &str) -> Value {
        serde_json::from_slice(&fs::read(dir.join(name)).unwrap()).unwrap()
    }
    fn fragment(&self, full: &Path, name: &str, only_mcp: bool) -> PathBuf {
        let out = self.root.path().join(name);
        fs::create_dir(&out).unwrap();
        for doc in ["index.json", "index.json.minisig"] {
            fs::copy(full.join(doc), out.join(doc)).unwrap();
        }
        for folder in ["blob", "media"] {
            fs::create_dir(out.join(folder)).unwrap();
            let tree = check(full, &self.anchor(), true).unwrap();
            let native = check(&self.root.path().join("base"), &self.anchor(), false).unwrap();
            for asset in tree.assets.keys().filter(|a| {
                a.starts_with(&format!("{folder}/"))
                    && (!only_mcp || !native.assets.contains_key(*a))
            }) {
                fs::copy(full.join(asset), out.join(asset)).unwrap();
            }
        }
        out
    }
}

#[test]
fn authentic_companions_from_different_publications_and_unbound_catalogues_refuse() {
    let f = Fixture::new();
    let base = f.tree("base", 1, true);
    let original = check(&base, &f.anchor(), true).unwrap();
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let selected = fs::read(store.join("current.json")).unwrap();
    for name in ["roots.json", "revocations.json"] {
        let mut value = f.json(&base, name);
        value["version"] = json!(2);
        f.signed(
            &base,
            name,
            &value,
            if name == "roots.json" {
                "root"
            } else {
                "publisher"
            },
        );
        // Each companion is genuinely signed by its authorized role. Binding,
        // not a bad signature, must reject this mixed publication.
        assert!(metadata(&base, &f.anchor(), true)
            .err()
            .unwrap()
            .to_string()
            .contains("hash mismatch"));
        let out = f.root.path().join("refused");
        assert!(assemble_with(&base, &base, &out, &f.anchor()).is_err());
        assert!(!out.exists());
        fs::write(base.join(name), &original.docs[name]).unwrap();
        fs::write(
            base.join(format!("{name}.minisig")),
            &original.docs[&format!("{name}.minisig")],
        )
        .unwrap();
    }
    let mut value = f.json(&base, "index.json");
    value.as_object_mut().unwrap().remove("generation");
    let raw = serde_json::to_vec(&value).unwrap();
    fs::write(base.join("index.json"), &raw).unwrap();
    fs::write(
        base.join("index.json.minisig"),
        crate::sign_bytes(&f.root.path().join("keys/publisher.key"), &raw).unwrap(),
    )
    .unwrap();
    assert!(check(&base, &f.anchor(), true).is_err());
    assert_eq!(fs::read(store.join("current.json")).unwrap(), selected);
}

#[test]
fn assembly_preserves_native_assets_and_exact_signed_bytes_when_adding_mcp() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let next = f.tree("next", 2, true);
    let update = f.fragment(&next, "update", true);
    let out = f.root.path().join("assembly");
    assemble_with(&base, &update, &out, &f.anchor()).unwrap();
    let checked = check(&out.join("v1"), &f.anchor(), true).unwrap();
    assert_eq!(checked.index.entries.len(), 2);
    assert_eq!(
        checked.docs["index.json"],
        fs::read(next.join("index.json")).unwrap()
    );
    assert!(
        serde_json::from_slice::<Value>(&checked.docs["index.json"]).unwrap()["future_envelope"]
            ["retained"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(checked.assets.len(), 5);
    let parent: Parent =
        serde_json::from_slice(&fs::read(out.join("promotion.json")).unwrap()).unwrap();
    assert!(parent.matches(&check(&base, &f.anchor(), true).unwrap().state));
    assert!(assemble_with(&base, &update, &out, &f.anchor()).is_err());
    assert!(assemble_with(&base, &update, &base.join("nested"), &f.anchor()).is_err());
}

#[test]
fn missing_altered_extra_oversized_and_unsafe_asset_refs_refuse() {
    let f = Fixture::new();
    let dir = f.tree("base", 1, true);
    let anchor = f.anchor();
    let tree = check(&dir, &anchor, true).unwrap();
    let name = tree.assets.keys().next().unwrap();
    let file = dir.join(name);
    let original = fs::read(&file).unwrap();
    fs::write(&file, b"changed").unwrap();
    assert!(check(&dir, &anchor, true).is_err());
    fs::remove_file(&file).unwrap();
    assert!(check(&dir, &anchor, true).is_err());
    fs::write(&file, &original).unwrap();
    fs::write(dir.join("media/extra.md"), b"extra").unwrap();
    assert!(check(&dir, &anchor, true).is_err());
    fs::remove_file(dir.join("media/extra.md")).unwrap();
    let native = tree
        .assets
        .iter()
        .find(|(name, _)| name.ends_with(".grainpack"))
        .unwrap();
    let path = dir.join(native.0);
    let bytes = fs::read(&path).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(grain_sdk::PACK_MAX_BYTES + 1)
        .unwrap();
    assert!(check(&dir, &anchor, true).is_err());
    fs::write(&path, bytes).unwrap();
    let original = f.json(&dir, "index.json");
    for pointer in [
        "/entries/0/sha256",
        "/entries/0/listing/sha256",
        "/entries/0/media/0/kind",
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = json!("../escape");
        f.signed(&dir, "index.json", &bad, "publisher");
        assert!(check(&dir, &anchor, true).is_err(), "{pointer}");
    }
    let mut bad = original.clone();
    bad["entries"][0]["size"] = json!(1);
    f.signed(&dir, "index.json", &bad, "publisher");
    assert!(check(&dir, &anchor, true).is_err());
    let mut bad = original.clone();
    let duplicate = bad["entries"][0].clone();
    bad["entries"].as_array_mut().unwrap().push(duplicate);
    f.signed(&dir, "index.json", &bad, "publisher");
    assert!(check(&dir, &anchor, true).is_err());
    f.signed(&dir, "index.json", &original, "publisher");
    check(&dir, &anchor, true).unwrap();
}

#[test]
fn trust_chain_specs_and_exact_expiry_are_mandatory_without_seed_exemption() {
    let f = Fixture::new();
    let dir = f.tree("base", 1, false);
    let anchor = f.anchor();
    for name in ["roots.json", "index.json", "revocations.json"] {
        let original = f.json(&dir, name);
        let key = if name == "roots.json" {
            "root"
        } else {
            "publisher"
        };
        let mut bad = original.clone();
        bad["spec"] = json!(2);
        f.signed(&dir, name, &bad, key);
        assert!(check(&dir, &anchor, true).is_err());
        let mut bad = original.clone();
        bad["expires"] = json!((Utc::now() - chrono::Duration::seconds(1)).to_rfc3339());
        f.signed(&dir, name, &bad, key);
        f.bind(&dir, "publisher");
        assert!(check(&dir, &anchor, true).is_err());
        check(&dir, &anchor, false).unwrap();
        f.signed(
            &dir,
            name,
            &original,
            if key == "root" { "publisher" } else { "root" },
        );
        assert!(check(&dir, &anchor, true).is_err());
        f.signed(&dir, name, &original, key);
        f.bind(&dir, "publisher");
    }
}

#[test]
fn serialized_promotion_retains_old_snapshot_and_refuses_stale_or_forged_parent() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let full = f.tree("next", 2, true);
    let update = f.fragment(&full, "update", true);
    let assembly = f.root.path().join("assembly");
    assemble_with(&base, &update, &assembly, &f.anchor()).unwrap();
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    let pin = digest(&before);
    let old: State = serde_json::from_slice(&before).unwrap();
    let old_index = fs::read(
        store
            .join("snapshots")
            .join(&old.snapshot)
            .join("index.json"),
    )
    .unwrap();
    let parent_file = assembly.join("promotion.json");
    let parent = fs::read(&parent_file).unwrap();
    let mut forged: Value = serde_json::from_slice(&parent).unwrap();
    forged["index_sha256"] = json!("a".repeat(64));
    fs::write(&parent_file, serde_json::to_vec(&forged).unwrap()).unwrap();
    assert!(promote_with(&assembly, &store, &pin, &f.anchor()).is_err());
    assert_eq!(fs::read(store.join("current.json")).unwrap(), before);
    fs::write(parent_file, parent).unwrap();
    promote_with(&assembly, &store, &pin, &f.anchor()).unwrap();
    let after = fs::read(store.join("current.json")).unwrap();
    let new: State = serde_json::from_slice(&after).unwrap();
    assert_eq!(new.index_version, 2);
    assert_eq!(
        fs::read(
            store
                .join("snapshots")
                .join(old.snapshot)
                .join("index.json")
        )
        .unwrap(),
        old_index
    );
    assert!(promote_with(&assembly, &store, &pin, &f.anchor())
        .unwrap_err()
        .to_string()
        .contains("changed"));
    assert!(promote_with(&assembly, &store, &digest(&after), &f.anchor()).is_err());
    assert_eq!(fs::read(store.join("current.json")).unwrap(), after);
    assert!(initialize_with(&base, &store, &f.anchor()).is_err());
}

#[test]
fn contention_and_unreferenced_crash_snapshot_do_not_change_current() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let full = f.tree("next", 2, true);
    let update = f.fragment(&full, "update", true);
    let assembly = f.root.path().join("assembly");
    assemble_with(&base, &update, &assembly, &f.anchor()).unwrap();
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    let held = lock(&store).unwrap();
    assert!(
        promote_with(&assembly, &store, &digest(&before), &f.anchor())
            .unwrap_err()
            .to_string()
            .contains("lock")
    );
    drop(held);
    let next = check(&assembly.join("v1"), &f.anchor(), true).unwrap();
    install(&next, &store, &f.anchor()).unwrap();
    assert_eq!(fs::read(store.join("current.json")).unwrap(), before);
    let staged_index = store
        .join("snapshots")
        .join(&next.state.snapshot)
        .join("index.json");
    let valid_index = fs::read(&staged_index).unwrap();
    fs::write(&staged_index, b"incomplete crash leftover").unwrap();
    assert!(promote_with(&assembly, &store, &digest(&before), &f.anchor()).is_err());
    assert_eq!(fs::read(store.join("current.json")).unwrap(), before);
    fs::write(&staged_index, valid_index).unwrap();
    promote_with(&assembly, &store, &digest(&before), &f.anchor()).unwrap();
    let after = fs::read(store.join("current.json")).unwrap();
    assert_ne!(after, before);
    lock(&store).unwrap(); // Persistent lock file is reusable after process/handle exit.
}

#[test]
fn rollback_same_version_replacement_and_extension_bytes_cannot_be_reissued() {
    let f = Fixture::new();
    let base = f.tree("base", 3, false);
    let next = f.tree("next", 2, false);
    let old = check(&base, &f.anchor(), true).unwrap();
    let new = check(&next, &f.anchor(), true).unwrap();
    assert!(transition(&old, &new, true).is_err());
    let mut document = f.json(&next, "index.json");
    document["version"] = json!(3);
    document["future_envelope"] = json!("changed");
    f.signed(&next, "index.json", &document, "publisher");
    assert!(transition(&old, &check(&next, &f.anchor(), true).unwrap(), true).is_err());
    document["version"] = json!(4);
    document["entries"][0]["sha256"] = json!(digest(b"replacement"));
    document["entries"][0]["size"] = json!(11);
    f.signed(&next, "index.json", &document, "publisher");
    assert!(
        transition(&old, &metadata(&next, &f.anchor(), true).unwrap(), true)
            .unwrap_err()
            .to_string()
            .contains("cannot change")
    );
}

#[test]
fn revocation_strength_is_retained_including_all_version_rules() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let next = f.tree("next", 2, false);
    let mut old = f.json(&base, "revocations.json");
    old["entries"] = json!([{"id":"com.example.native","state":"revoked","reason":"test"}]);
    f.signed(&base, "revocations.json", &old, "publisher");
    f.bind(&base, "publisher");
    let old = check(&base, &f.anchor(), true).unwrap();
    for entries in [
        json!([]),
        json!([{"id":"com.example.native","state":"deprecated"}]),
        json!([{"id":"com.example.native","version":"1.0.0","state":"revoked"}]),
    ] {
        let mut revoked = f.json(&next, "revocations.json");
        revoked["version"] = json!(2);
        revoked["entries"] = entries;
        f.signed(&next, "revocations.json", &revoked, "publisher");
        f.bind(&next, "publisher");
        assert!(transition(&old, &check(&next, &f.anchor(), true).unwrap(), true).is_err());
    }
    let mut revoked = f.json(&base, "revocations.json");
    revoked["version"] = json!(2);
    f.signed(&next, "revocations.json", &revoked, "publisher");
    f.bind(&next, "publisher");
    transition(&old, &check(&next, &f.anchor(), true).unwrap(), true).unwrap();
}

#[test]
fn expired_history_can_be_renewed_but_never_initialized_as_fresh() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let full = f.tree("next", 2, false);
    for name in ["roots.json", "index.json", "revocations.json"] {
        let mut old = f.json(&base, name);
        old["expires"] = json!((Utc::now() - chrono::Duration::days(2)).to_rfc3339());
        f.signed(
            &base,
            name,
            &old,
            if name == "roots.json" {
                "root"
            } else {
                "publisher"
            },
        );
        let mut new = f.json(&full, name);
        new["version"] = json!(2);
        f.signed(
            &full,
            name,
            &new,
            if name == "roots.json" {
                "root"
            } else {
                "publisher"
            },
        );
    }
    f.bind(&base, "publisher");
    f.bind(&full, "publisher");
    let store = f.root.path().join("store");
    assert!(initialize_with(&base, &store, &f.anchor()).is_err());
    // Model a previously initialized snapshot after its metadata has aged out.
    let historical = check(&base, &f.anchor(), false).unwrap();
    fs::create_dir(&store).unwrap();
    fs::create_dir(store.join("snapshots")).unwrap();
    copy(
        &historical,
        &store.join("snapshots").join(&historical.state.snapshot),
    )
    .unwrap();
    pointer(&store, &historical.state, true).unwrap();
    let update = f.fragment(&full, "update", false);
    for name in [
        "roots.json",
        "roots.json.minisig",
        "revocations.json",
        "revocations.json.minisig",
    ] {
        fs::copy(full.join(name), update.join(name)).unwrap();
    }
    let assembly = f.root.path().join("assembly");
    assemble_with(&base, &update, &assembly, &f.anchor()).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    promote_with(&assembly, &store, &digest(&before), &f.anchor()).unwrap();
    let current: State =
        serde_json::from_slice(&fs::read(store.join("current.json")).unwrap()).unwrap();
    assert_eq!(current.roots_version, 2);
    assert_eq!(current.revocations_version, 2);
}

#[test]
fn root_authorized_publishing_key_rotation_requires_matching_new_signatures() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let next = f.tree("next", 2, false);
    let mut roots = f.json(&next, "roots.json");
    roots["version"] = json!(2);
    roots["publishing_key"] = json!(Fixture::public(f.root.path(), "rotated"));
    f.signed(&next, "roots.json", &roots, "root");
    assert!(check(&next, &f.anchor(), true).is_err());
    for name in ["index.json", "revocations.json"] {
        let mut value = f.json(&next, name);
        value["version"] = json!(2);
        f.signed(&next, name, &value, "rotated");
    }
    f.bind(&next, "rotated");
    transition(
        &check(&base, &f.anchor(), true).unwrap(),
        &check(&next, &f.anchor(), true).unwrap(),
        true,
    )
    .unwrap();
}

#[test]
fn removed_entries_leave_prior_snapshot_assets_untouched() {
    let f = Fixture::new();
    let base = f.tree("base", 1, true);
    let mut index = f.json(&base, "index.json");
    index["version"] = json!(2);
    index["entries"] = json!([]);
    let update = f.root.path().join("update");
    fs::create_dir(&update).unwrap();
    f.signed(&update, "index.json", &index, "publisher");
    let assembly = f.root.path().join("assembly");
    assemble_with(&base, &update, &assembly, &f.anchor()).unwrap();
    assert_eq!(
        check(&assembly.join("v1"), &f.anchor(), true)
            .unwrap()
            .assets
            .len(),
        0
    );
    assert_eq!(check(&base, &f.anchor(), true).unwrap().assets.len(), 5);
}

#[test]
fn withdrawn_version_cannot_return_with_changed_bytes_but_exact_restore_can() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let original: State =
        serde_json::from_slice(&fs::read(store.join("current.json")).unwrap()).unwrap();
    let removal = f.root.path().join("removal");
    fs::create_dir(&removal).unwrap();
    let mut index = f.json(&base, "index.json");
    index["version"] = json!(2);
    index["entries"] = json!([]);
    f.signed(&removal, "index.json", &index, "publisher");
    let empty = f.root.path().join("empty");
    assemble_with(&base, &removal, &empty, &f.anchor()).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    promote_with(&empty, &store, &digest(&before), &f.anchor()).unwrap();
    let withdrawn = fs::read(store.join("current.json")).unwrap();
    let replacement = f.tree("replacement", 3, false);
    let mut replaced = f.json(&replacement, "index.json");
    let old_hash = replaced["entries"][0]["sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let bytes = b"changed code after withdrawal";
    replaced["entries"][0]["sha256"] = json!(digest(bytes));
    replaced["entries"][0]["size"] = json!(bytes.len());
    fs::remove_file(replacement.join(format!("blob/{old_hash}.grainpack"))).unwrap();
    fs::write(
        replacement.join(format!("blob/{}.grainpack", digest(bytes))),
        bytes,
    )
    .unwrap();
    f.signed(&replacement, "index.json", &replaced, "publisher");
    let changed = f.root.path().join("changed");
    assemble_with(&empty.join("v1"), &replacement, &changed, &f.anchor()).unwrap();
    assert!(
        promote_with(&changed, &store, &digest(&withdrawn), &f.anchor())
            .unwrap_err()
            .to_string()
            .contains("cannot change")
    );
    assert_eq!(fs::read(store.join("current.json")).unwrap(), withdrawn);
    let restore = f.tree("restore", 3, false);
    let restored = f.root.path().join("restored");
    assemble_with(&empty.join("v1"), &restore, &restored, &f.anchor()).unwrap();
    promote_with(&restored, &store, &digest(&withdrawn), &f.anchor()).unwrap();
    assert!(store.join("snapshots").join(original.snapshot).exists());
    assert_eq!(
        selected(
            &store,
            &digest(&fs::read(store.join("current.json")).unwrap()),
            &f.anchor(),
            true
        )
        .unwrap()
        .index
        .entries
        .len(),
        1
    );
}

#[test]
fn historical_listing_media_and_artifact_kind_are_reserved_even_in_installed_orphans() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let orphan = f.tree("orphan", 2, true);
    let reserved = check(&orphan, &f.anchor(), true).unwrap();
    install(&reserved, &store, &f.anchor()).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    let next = f.tree("next", 3, true);
    let original = f.json(&next, "index.json");
    for (pointer, value) in [
        ("/entries/1/listing/sha256", json!("a".repeat(64))),
        ("/entries/1/media/0/sha256", json!("b".repeat(64))),
        ("/entries/1/artifact_kind", json!("native")),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        f.signed(&next, "index.json", &changed, "publisher");
        let proposal = metadata(&next, &f.anchor(), true).unwrap();
        assert!(historical_versions(&store, &proposal, &f.anchor())
            .unwrap_err()
            .to_string()
            .contains("cannot change"));
        assert_eq!(fs::read(store.join("current.json")).unwrap(), before);
    }
    let mut new_version = original;
    new_version["entries"][1]["version"] = json!("2.0.0");
    new_version["entries"][1]["sha256"] = json!("c".repeat(64));
    f.signed(&next, "index.json", &new_version, "publisher");
    historical_versions(
        &store,
        &metadata(&next, &f.anchor(), true).unwrap(),
        &f.anchor(),
    )
    .unwrap();
}

#[test]
fn historical_signature_and_directory_identity_refuse_without_changing_current() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let next = f.tree("next", 2, true);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    let state: State = serde_json::from_slice(&before).unwrap();
    let dir = store.join("snapshots").join(&state.snapshot);
    let tree = check(&next, &f.anchor(), true).unwrap();
    let file = dir.join("index.json");
    let raw = fs::read(&file).unwrap();
    fs::write(&file, b"invalid history").unwrap();
    assert!(historical_versions(&store, &tree, &f.anchor()).is_err());
    fs::write(&file, raw).unwrap();
    let renamed = store.join("snapshots").join("a".repeat(64));
    fs::rename(&dir, &renamed).unwrap();
    assert!(historical_versions(&store, &tree, &f.anchor())
        .unwrap_err()
        .to_string()
        .contains("identity differs"));
    fs::rename(renamed, dir).unwrap();
    // Unsigned abandoned staging is not publication history.
    let abandoned = store.join("snapshots/.tmp-abandoned");
    fs::create_dir(&abandoned).unwrap();
    fs::write(abandoned.join("index.json"), b"not signed").unwrap();
    historical_versions(&store, &tree, &f.anchor()).unwrap();
    assert!(abandoned.exists());
    let unexpected = store.join("snapshots/not-a-snapshot");
    fs::create_dir(&unexpected).unwrap();
    assert!(historical_versions(&store, &tree, &f.anchor()).is_err());
    assert_eq!(fs::read(store.join("current.json")).unwrap(), before);
}

#[test]
fn export_pins_one_fresh_complete_snapshot_and_retains_it_after_promotion() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let next = f.tree("next", 2, true);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    let out = f.root.path().join("export");
    let held = lock(&store).unwrap();
    assert!(export_with(&store, &out, &digest(&before), &f.anchor())
        .unwrap_err()
        .to_string()
        .contains("lock"));
    drop(held);
    assert!(!out.exists());
    assert!(export_with(&store, &store.join("inside"), &digest(&before), &f.anchor()).is_err());
    assert!(export_with(&store, &out, &"a".repeat(64), &f.anchor()).is_err());
    assert!(!out.exists());
    export_with(&store, &out, &digest(&before), &f.anchor()).unwrap();
    let exported = check(&out.join("v1"), &f.anchor(), true).unwrap();
    let receipt: Value =
        serde_json::from_slice(&fs::read(out.join("snapshot.json")).unwrap()).unwrap();
    assert_eq!(receipt["current_sha256"], digest(&before));
    assert_eq!(receipt["selected"]["snapshot"], exported.state.snapshot);
    assert_eq!(
        exported.docs["index.json"],
        fs::read(base.join("index.json")).unwrap()
    );
    assert!(export_with(&store, &out, &digest(&before), &f.anchor()).is_err());
    let assembly = f.root.path().join("assembly");
    assemble_with(&base, &next, &assembly, &f.anchor()).unwrap();
    promote_with(&assembly, &store, &digest(&before), &f.anchor()).unwrap();
    let after = fs::read(store.join("current.json")).unwrap();
    assert_eq!(
        check(&out.join("v1"), &f.anchor(), true).unwrap().state,
        exported.state
    );
    let fresh = f.root.path().join("fresh-export");
    assert!(export_with(&store, &fresh, &digest(&before), &f.anchor()).is_err());
    assert!(!fresh.exists());
    export_with(&store, &fresh, &digest(&after), &f.anchor()).unwrap();
    assert_eq!(
        check(&fresh.join("v1"), &f.anchor(), true)
            .unwrap()
            .index
            .entries
            .len(),
        2
    );
}

#[test]
fn export_refuses_expired_or_corrupt_selection_without_publishing_output() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let original = fs::read(store.join("current.json")).unwrap();
    let state: State = serde_json::from_slice(&original).unwrap();
    let selected_path = store.join("snapshots").join(state.snapshot);
    let index_file = selected_path.join("index.json");
    let index_raw = fs::read(&index_file).unwrap();
    fs::write(&index_file, b"corrupt").unwrap();
    let out = f.root.path().join("export");
    assert!(export_with(&store, &out, &digest(&original), &f.anchor()).is_err());
    assert!(!out.exists());
    fs::write(index_file, index_raw).unwrap();
    let mut expired = f.json(&base, "index.json");
    expired["expires"] = json!((Utc::now() - chrono::Duration::days(2)).to_rfc3339());
    f.signed(&base, "index.json", &expired, "publisher");
    let aged = check(&base, &f.anchor(), false).unwrap();
    copy(&aged, &store.join("snapshots").join(&aged.state.snapshot)).unwrap();
    pointer(&store, &aged.state, false).unwrap();
    let before = fs::read(store.join("current.json")).unwrap();
    assert!(export_with(&store, &out, &digest(&before), &f.anchor()).is_err());
    assert!(!out.exists());
    assert_eq!(fs::read(store.join("current.json")).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn symlinked_assets_and_snapshot_directories_are_rejected() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let tree = check(&base, &f.anchor(), true).unwrap();
    let name = tree.assets.keys().next().unwrap();
    let original = base.join(name);
    let linked = f.root.path().join("linked-file");
    fs::rename(&original, &linked).unwrap();
    std::os::unix::fs::symlink(&linked, &original).unwrap();
    assert!(check(&base, &f.anchor(), true).is_err());
    let alias = f.root.path().join("alias");
    std::os::unix::fs::symlink(&base, &alias).unwrap();
    assert!(directory(&alias).is_err());
}
