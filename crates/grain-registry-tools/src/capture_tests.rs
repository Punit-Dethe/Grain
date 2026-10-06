//! Reuse real signed publication fixtures; capture adds no alternate runtime.
use super::*;
use crate::serving::capture::{capture_with, Receipt};

fn capture(p: &Publication, commit: &str, pin: &str, out: &Path) -> Result<()> {
    capture_with(&p.repo, "example/registry", commit, pin, out, &p.f.anchor())
}

fn refresh_receipt(p: &Publication) -> String {
    let state = metadata(&p.bundle.join("v1"), &p.f.anchor(), false)
        .unwrap()
        .state;
    let pointer = serde_json::to_vec(&state).unwrap();
    fs::write(p.bundle.join("current.json"), &pointer).unwrap();
    let mut receipt: hosting::Receipt =
        serde_json::from_slice(&fs::read(p.bundle.join("bundle.json")).unwrap()).unwrap();
    receipt.selected = state;
    receipt.current_sha256 = digest(&pointer);
    let raw = serde_json::to_vec(&receipt).unwrap();
    fs::write(p.bundle.join("bundle.json"), &raw).unwrap();
    digest(&raw)
}

#[test]
fn capture_reconstructs_previous_and_current_commits_and_feeds_publication_gate() {
    let p = Publication::new();
    fs::write(p.repo.join("v1/index.json"), "ignore dirty worktree").unwrap();
    let prior = p.f.root.path().join("prior-capture");
    let current = p.f.root.path().join("current-capture");
    capture(&p, &p.base, &p.old_pin, &prior).unwrap();
    capture(&p, &p.candidate, &p.pin, &current).unwrap();
    let receipt: Receipt =
        serde_json::from_slice(&fs::read(prior.join("capture.json")).unwrap()).unwrap();
    assert_eq!(receipt.commit, p.base);
    assert_eq!(receipt.bundle_receipt_sha256, p.old_pin);
    assert!(receipt.bytes > 0 && receipt.files >= 8);
    assert_eq!(
        fs::read(prior.join("bundle/v1/index.json")).unwrap(),
        fs::read(p.previous.join("v1/index.json")).unwrap()
    );
    let before = p.request();
    let previous = prior.join("bundle");
    let bundle = current.join("bundle");
    let request = Request {
        previous: &previous,
        bundle: &bundle,
        ..before
    };
    publication::prepare_with(&request, &p.f.anchor()).unwrap();
    assert_eq!(git(&p.repo, &["rev-parse", "HEAD"]), p.candidate);
    assert_eq!(
        fs::read_to_string(p.repo.join("v1/index.json")).unwrap(),
        "ignore dirty worktree"
    );
    assert_eq!(
        hosting::verify(&bundle, &p.pin, &p.f.anchor())
            .unwrap()
            .index_version,
        2
    );
}

#[test]
fn capture_recreates_empty_git_directories_without_executing_checkout_filters() {
    let f = Fixture::new();
    let tree = f.tree("empty", 1, false);
    let mut index = f.json(&tree, "index.json");
    index["entries"] = json!([]);
    f.signed(&tree, "index.json", &index, "publisher");
    for folder in ["blob", "media"] {
        for item in fs::read_dir(tree.join(folder)).unwrap() {
            fs::remove_file(item.unwrap().path()).unwrap();
        }
    }
    let store = f.root.path().join("empty-store");
    initialize_with(&tree, &store, &f.anchor()).unwrap();
    let bundle = f.root.path().join("empty-bundle");
    hosting::export(
        &store,
        &bundle,
        &digest(&fs::read(store.join("current.json")).unwrap()),
        &f.anchor(),
    )
    .unwrap();
    let pin = digest(&fs::read(bundle.join("bundle.json")).unwrap());
    let repo = f.root.path().join("empty-repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=release"]);
    for (name, value) in [
        ("user.name", "Fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
        (
            "remote.origin.url",
            "https://github.com/example/registry.git",
        ),
    ] {
        git(&repo, &["config", "--local", name, value]);
    }
    Publication::install(&bundle, &repo);
    // Even export-ignore attributes must not silently omit committed metadata.
    fs::write(repo.join(".gitattributes"), "v1/* export-ignore\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "empty publication"]);
    let commit = git(&repo, &["rev-parse", "HEAD"]);
    let out = f.root.path().join("empty-capture");
    capture_with(&repo, "example/registry", &commit, &pin, &out, &f.anchor()).unwrap();
    for folder in ["v1/blob", "v1/media", "history"] {
        assert!(out.join("bundle").join(folder).is_dir());
        assert_eq!(
            fs::read_dir(out.join("bundle").join(folder))
                .unwrap()
                .count(),
            0
        );
    }
    hosting::verify(&out.join("bundle"), &pin, &f.anchor()).unwrap();
}

#[test]
fn capture_streams_binary_artifacts_with_embedded_batch_headers_exactly() {
    let mut p = Publication::new();
    let payload = b"binary\0\n0000000000000000000000000000000000000000 blob 999999\n\xff\0tail";
    let hash = digest(payload);
    let v1 = p.bundle.join("v1");
    fs::write(v1.join(format!("blob/{hash}.grainpack")), payload).unwrap();
    let mut index = p.f.json(&v1, "index.json");
    index["version"] = 3.into();
    index["entries"][0]["version"] = "1.0.1".into();
    index["entries"][0]["sha256"] = hash.clone().into();
    index["entries"][0]["size"] = payload.len().into();
    p.f.signed(&v1, "index.json", &index, "publisher");
    p.pin = refresh_receipt(&p);
    Publication::install(&p.bundle, &p.repo);
    p.amend();
    let out = p.f.root.path().join("binary-capture");
    capture(&p, &p.candidate, &p.pin, &out).unwrap();
    assert_eq!(
        fs::read(out.join(format!("bundle/v1/blob/{hash}.grainpack"))).unwrap(),
        payload
    );
    hosting::verify(&out.join("bundle"), &p.pin, &p.f.anchor()).unwrap();
}

#[test]
fn capture_refuses_invalid_signature_receipt_and_missing_withdrawn_assets_with_cleanup() {
    for mode in ["signature", "receipt", "historical-asset"] {
        let mut p = Publication::new();
        let out = p.f.root.path().join("refused-capture");
        if mode == "signature" {
            fs::write(p.repo.join("v1/index.json"), "corrupted metadata").unwrap();
            p.amend();
        }
        if mode == "historical-asset" {
            let file = fs::read_dir(p.repo.join("v1/blob"))
                .unwrap()
                .map(|v| v.unwrap().path())
                .find(|v| v.to_string_lossy().ends_with(".mcp.json"))
                .unwrap();
            let relative = file
                .strip_prefix(&p.repo)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            git(&p.repo, &["rm", &relative]);
            p.amend();
        }
        let pin = if mode == "receipt" {
            "0000000000000000000000000000000000000000000000000000000000000000"
        } else {
            &p.pin
        };
        let error = capture(&p, &p.candidate, pin, &out)
            .unwrap_err()
            .to_string();
        match mode {
            "signature" => assert!(error.contains("signature does not verify"), "{error}"),
            "receipt" => assert!(error.contains("receipt digest differs"), "{error}"),
            _ => assert!(
                error.contains("cannot find") || error.contains("No such file"),
                "{error}"
            ),
        }
        assert!(!out.exists());
        assert_eq!(git(&p.repo, &["rev-parse", "HEAD"]), p.candidate);
    }
}

#[test]
fn capture_refuses_unsafe_paths_modes_oversize_and_missing_baseline_before_output() {
    for mode in ["extra", "executable", "oversize", "legacy"] {
        let mut p = Publication::new();
        match mode {
            "extra" => fs::write(p.repo.join("v1/extra.txt"), "extra").unwrap(),
            "executable" => {
                git(&p.repo, &["update-index", "--chmod=+x", "v1/index.json"]);
            }
            "oversize" => {
                fs::write(p.repo.join("v1/roots.json"), vec![b'x'; 64 * 1024 + 1]).unwrap()
            }
            _ => {
                git(
                    &p.repo,
                    &[
                        "rm",
                        "v1/revocations.json",
                        "v1/revocations.json.minisig",
                        ".registry-publication/bundle.json",
                    ],
                );
            }
        }
        if mode == "executable" {
            git(&p.repo, &["commit", "--amend", "--no-edit"]);
            p.candidate = git(&p.repo, &["rev-parse", "HEAD"]);
        } else {
            p.amend();
        }
        let out = p.f.root.path().join("refused-capture");
        let error = capture(&p, &p.candidate, &p.pin, &out)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(match mode {
                "extra" => "Unexpected committed",
                "executable" => "regular non-executable",
                "oversize" => "byte bound",
                _ => "protected legacy migration required",
            }),
            "{error}"
        );
        assert!(!out.exists());
    }
}

#[test]
fn capture_accepts_expired_previous_proof_without_making_it_a_fresh_candidate() {
    let mut p = Publication::new();
    let v1 = p.bundle.join("v1");
    for name in ["index.json", "revocations.json"] {
        let mut value = p.f.json(&v1, name);
        value["expires"] = "2000-01-01T00:00:00Z".into();
        p.f.signed(&v1, name, &value, "publisher");
    }
    p.pin = refresh_receipt(&p);
    Publication::install(&p.bundle, &p.repo);
    p.amend();
    let out = p.f.root.path().join("expired-capture");
    capture(&p, &p.candidate, &p.pin, &out).unwrap();
    hosting::inspect(&out.join("bundle"), &p.pin, &p.f.anchor(), false).unwrap();
    assert!(hosting::verify(&out.join("bundle"), &p.pin, &p.f.anchor()).is_err());
    let raw: Receipt =
        serde_json::from_slice(&fs::read(out.join("capture.json")).unwrap()).unwrap();
    assert_eq!(
        raw.evidence_class,
        "authenticated-previous-publication-capture-not-release-approval"
    );
}

#[test]
fn capture_refuses_bad_identity_pins_checkout_and_existing_or_contained_outputs() {
    let p = Publication::new();
    let out = p.f.root.path().join("refused-capture");
    for (commit, pin, expected) in [
        ("bad", p.pin.as_str(), "full Git SHA1"),
        (p.candidate.as_str(), "bad", "receipt digest required"),
    ] {
        assert!(capture(&p, commit, pin, &out)
            .unwrap_err()
            .to_string()
            .contains(expected));
        assert!(!out.exists());
    }
    let inside = p.repo.join("contained-capture");
    assert!(capture(&p, &p.candidate, &p.pin, &inside)
        .unwrap_err()
        .to_string()
        .contains("outside"));
    fs::create_dir(&out).unwrap();
    fs::write(out.join("keep"), "preserve").unwrap();
    assert!(capture(&p, &p.candidate, &p.pin, &out)
        .unwrap_err()
        .to_string()
        .contains("already exists"));
    assert_eq!(fs::read_to_string(out.join("keep")).unwrap(), "preserve");
    let alternate = p.f.root.path().join("other-refused-capture");
    git(
        &p.repo,
        &[
            "config",
            "--local",
            "remote.origin.url",
            "https://github.com/other/repository.git",
        ],
    );
    assert!(capture(&p, &p.candidate, &p.pin, &alternate)
        .unwrap_err()
        .to_string()
        .contains("origin differs"));
    assert!(!alternate.exists());
}
