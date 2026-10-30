//! Workflow regressions reuse real signed serving fixtures, not an Agent harness.
//! The private signer closure supplies fixture signatures; production always uses
//! review::sign with its live approval/provenance/key gates and app-pinned roots.
use super::*;

fn pin(store: &Path) -> String {
    digest(&fs::read(store.join("current.json")).unwrap())
}

#[test]
fn release_workflow_stages_mcp_then_native_update_with_preserved_history() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    for (version, name) in [(2, "mcp-addition"), (3, "native-update")] {
        let full = f.tree(name, version, true);
        if version == 3 {
            let mut index = f.json(&full, "index.json");
            let mut update = index["entries"][0].clone();
            update["version"] = json!("1.1.0");
            index["entries"].as_array_mut().unwrap().push(update);
            f.signed(&full, "index.json", &index, "publisher");
        }
        let fragment = f.fragment(&full, &format!("fragment-{version}"), false);
        let out = f.root.path().join(format!("release-{version}"));
        let before = pin(&store);
        release::stage_with(
            &store,
            &before,
            &out,
            &[],
            &f.anchor(),
            |previous, update| {
                assert_eq!(
                    digest(&fs::read(previous.join("index.json")).unwrap()),
                    selected(&store, &before, &f.anchor(), true)?
                        .state
                        .index_sha256
                );
                fs::rename(&fragment, update)?;
                Ok(())
            },
        )
        .unwrap();
        let summary: Value =
            serde_json::from_slice(&fs::read(out.join("release.json")).unwrap()).unwrap();
        assert_eq!(summary["previous_current_sha256"], before);
        assert_eq!(summary["current_sha256"], pin(&store));
        assert_eq!(summary["index_version"], version);
        let verified = hosting::inspect(
            &out.join("bundle"),
            summary["bundle_receipt_sha256"].as_str().unwrap(),
            &f.anchor(),
            true,
        )
        .unwrap();
        assert_eq!(verified.history.len(), (version - 1) as usize);
        assert_eq!(verified.selected.index_version, version);
        assert!(
            release::stage_with(&store, &before, &out, &[], &f.anchor(), |_, _| panic!(
                "stale retry signed again"
            ))
            .is_err()
        );
    }
}

#[test]
fn stale_base_or_unsafe_output_refuses_before_signing_and_preserves_inputs() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let before = pin(&store);
    let fresh = f.root.path().join("refused");
    assert!(release::stage_with(
        &store,
        &"0".repeat(64),
        &fresh,
        &[],
        &f.anchor(),
        |_, _| panic!("stale signing")
    )
    .is_err());
    assert!(!fresh.exists());
    for out in [store.join("nested"), base.join("nested"), store.clone()] {
        assert!(
            release::stage_with(&store, &before, &out, &[&base], &f.anchor(), |_, _| panic!(
                "unsafe signing"
            ))
            .is_err()
        );
    }
    assert_eq!(pin(&store), before);
}

#[test]
fn approval_failure_or_corrupt_update_retains_work_without_promoting() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let before = pin(&store);
    for corrupt in [false, true] {
        let out = f
            .root
            .path()
            .join(if corrupt { "corrupt" } else { "denied" });
        let error = release::stage_with(&store, &before, &out, &[], &f.anchor(), |_, update| {
            if !corrupt {
                bail!("source approval refused");
            }
            fs::create_dir(update)?;
            fs::write(update.join("index.json"), b"not signed")?;
            Ok(())
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains(if corrupt {
            "assembly failed"
        } else {
            "signing failed"
        }));
        assert!(out.join("base/v1").is_dir());
        assert!(!out.join("release.json").exists());
        assert_eq!(pin(&store), before);
    }
}

#[test]
fn competing_writer_after_signing_cannot_be_overwritten() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let next = f.tree("next", 2, true);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let before = pin(&store);
    let fragment = f.fragment(&next, "fragment", false);
    let out = f.root.path().join("competing");
    let error = release::stage_with(&store, &before, &out, &[], &f.anchor(), |_, update| {
        let other = f.root.path().join("other");
        assemble_with(&base, &next, &other, &f.anchor())?;
        promote_with(&other, &store, &before, &f.anchor())?;
        fs::rename(&fragment, update)?;
        Ok(())
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("local promotion failed"));
    assert_eq!(
        selected(&store, &pin(&store), &f.anchor(), true)
            .unwrap()
            .state
            .index_version,
        2
    );
    assert!(out.join("signed-update/index.json.minisig").is_file());
    assert!(!out.join("release.json").exists());
}

#[test]
fn export_failure_after_local_promotion_is_recovered_without_resigning() {
    let f = Fixture::new();
    let base = f.tree("base", 1, false);
    let next = f.tree("next", 2, true);
    let fragment = f.fragment(&next, "fragment", false);
    let store = f.root.path().join("store");
    initialize_with(&base, &store, &f.anchor()).unwrap();
    let before = pin(&store);
    let out = f.root.path().join("failed-export");
    let mut calls = 0;
    let error = release::stage_with(&store, &before, &out, &[], &f.anchor(), |_, update| {
        calls += 1;
        fs::rename(&fragment, update)?;
        // An occupied export destination simulates a failed final filesystem stage.
        fs::create_dir(out.join("bundle"))?;
        Ok(())
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("rerun export-hosting-bundle only"));
    assert_eq!(calls, 1);
    let current = pin(&store);
    assert_ne!(current, before);
    assert!(!out.join("release.json").exists());
    let recovered = f.root.path().join("recovered");
    hosting::export(&store, &recovered, &current, &f.anchor()).unwrap();
    let receipt = digest(&fs::read(recovered.join("bundle.json")).unwrap());
    assert_eq!(
        hosting::verify(&recovered, &receipt, &f.anchor())
            .unwrap()
            .index_version,
        2
    );
}
