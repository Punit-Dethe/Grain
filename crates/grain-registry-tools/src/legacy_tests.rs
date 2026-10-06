//! Real signatures/Git history; no public trust override or author execution.
use super::*;
use crate::serving::legacy::{capture_with, Manifest};
#[path = "migration_tests.rs"]
mod migration_tests;

struct Legacy {
    f: Fixture,
    repo: PathBuf,
    commits: Vec<String>,
    manifest: PathBuf,
    out: PathBuf,
}

impl Legacy {
    fn new() -> Self {
        let f = Fixture::new();
        let repo = f.tree("checkout", 1, true);
        let v1 = repo.join("v1");
        fs::create_dir(&v1).unwrap();
        for name in DOCS.into_iter().chain(["blob", "media"]) {
            fs::rename(repo.join(name), v1.join(name)).unwrap();
        }
        for name in ["revocations.json", "revocations.json.minisig"] {
            fs::remove_file(v1.join(name)).unwrap();
        }
        let mut index = f.json(&v1, "index.json");
        index["expires"] = json!("2000-01-01T00:00:00Z");
        index["entries"][0]["capabilities"] = json!(["capture:screen", "transform:transcript"]);
        index["entries"][0]["legacy_unknown"] = json!({"preserve": true});
        f.signed(&v1, "index.json", &index, "publisher");
        git(&repo, &["init", "--initial-branch=release"]);
        for (name, value) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@example.invalid"),
            ("core.autocrlf", "false"),
            ("commit.gpgSign", "false"),
            (
                "remote.origin.url",
                "https://github.com/example/registry.git",
            ),
        ] {
            git(&repo, &["config", "--local", name, value]);
        }
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "legacy first"]);
        let first = git(&repo, &["rev-parse", "HEAD"]);
        let bytes = b"changed historical native bytes\0\n";
        let hash = digest(bytes);
        fs::write(v1.join(format!("blob/{hash}.grainpack")), bytes).unwrap();
        index["version"] = json!(2);
        index["entries"].as_array_mut().unwrap().pop();
        index["entries"][0]["sha256"] = json!(hash);
        index["entries"][0]["size"] = json!(bytes.len());
        f.signed(&v1, "index.json", &index, "publisher");
        git(&repo, &["add", "."]);
        git(&repo, &["add", "--renormalize", "."]);
        git(
            &repo,
            &["commit", "-m", "legacy changed version and withdrawal"],
        );
        let second = git(&repo, &["rev-parse", "HEAD"]);
        let manifest = f.root.path().join("manifest.json");
        let out = f.root.path().join("legacy capture ü");
        let l = Self {
            f,
            repo,
            commits: vec![first, second],
            manifest,
            out,
        };
        l.write_manifest(&l.commits, &l.commits[1]);
        l
    }
    fn write_manifest(&self, commits: &[String], tip: &str) {
        fs::write(
            &self.manifest,
            serde_json::to_vec(&Manifest {
                schema: 1,
                tip_commit: tip.into(),
                commits: commits.to_vec(),
            })
            .unwrap(),
        )
        .unwrap();
    }
    fn pin(&self) -> String {
        digest(&fs::read(&self.manifest).unwrap())
    }
    fn run(&self) -> Result<()> {
        capture_with(
            &self.repo,
            "example/registry",
            &self.manifest,
            &self.pin(),
            &self.out,
            &self.f.anchor(),
        )
    }
    fn refused(&self, message: &str) {
        let error = self.run().unwrap_err().to_string();
        assert!(error.contains(message), "expected {message}, got {error}");
        assert!(!self.out.exists());
    }
    fn amend(&mut self) {
        git(&self.repo, &["add", "."]);
        git(&self.repo, &["add", "--renormalize", "."]);
        git(&self.repo, &["commit", "--amend", "--no-edit"]);
        self.commits[1] = git(&self.repo, &["rev-parse", "HEAD"]);
        self.write_manifest(&self.commits, &self.commits[1]);
    }
}

#[test]
fn legacy_preserves_expired_raw_proofs_withdrawals_and_conflicted_versions_without_activation() {
    let l = Legacy::new();
    let raw = publication::git(
        &l.repo,
        &["show", &format!("{}:v1/index.json", l.commits[0])],
        false,
    )
    .unwrap();
    fs::write(l.repo.join("v1/index.json"), "ignore dirty bytes").unwrap();
    l.run().unwrap();
    assert_eq!(
        fs::read(l.out.join(format!("proofs/{}/index.json", l.commits[0]))).unwrap(),
        raw.as_bytes()
    );
    let receipt: Value =
        serde_json::from_slice(&fs::read(l.out.join("legacy-history.json")).unwrap()).unwrap();
    assert_eq!(receipt["proofs"].as_array().unwrap().len(), 2);
    assert_eq!(receipt["reservations"].as_array().unwrap().len(), 3);
    assert_eq!(
        receipt["conflicted_versions"],
        json!([["com.example.native", "1.0.0"]])
    );
    assert_eq!(
        receipt["proofs"][0]["index_expires"],
        "2000-01-01T00:00:00Z"
    );
    assert!(receipt["evidence_class"]
        .as_str()
        .unwrap()
        .contains("no-revocation-proof"));
    assert!(!l.out.join("v1").exists() && !l.out.join("current.json").exists());
    assert_eq!(git(&l.repo, &["rev-parse", "HEAD"]), l.commits[1]);
    assert_eq!(
        fs::read_to_string(l.repo.join("v1/index.json")).unwrap(),
        "ignore dirty bytes"
    );
    for (name, hash) in receipt["assets"].as_object().unwrap() {
        assert_eq!(
            digest(&fs::read(l.out.join("assets").join(name)).unwrap()),
            hash.as_str().unwrap()
        );
    }
}

#[test]
fn legacy_refuses_omitted_reordered_duplicate_and_unpinned_history() {
    let l = Legacy::new();
    l.write_manifest(&l.commits[1..], &l.commits[1]);
    l.refused("complete catalogue-changing ancestry");
    let mut reversed = l.commits.clone();
    reversed.reverse();
    l.write_manifest(&reversed, &l.commits[1]);
    l.refused("complete catalogue-changing ancestry");
    l.write_manifest(&[l.commits[0].clone(), l.commits[0].clone()], &l.commits[1]);
    l.refused("Malformed");
    l.write_manifest(&l.commits, &l.commits[1]);
    assert!(capture_with(
        &l.repo,
        "example/registry",
        &l.manifest,
        &"0".repeat(64),
        &l.out,
        &l.f.anchor()
    )
    .unwrap_err()
    .to_string()
    .contains("independent pin"));
    assert!(!l.out.exists());
}

#[test]
fn legacy_refuses_signature_tampering_without_leaving_partial_output() {
    let mut l = Legacy::new();
    let file = l.repo.join("v1/index.json");
    let mut raw = fs::read(&file).unwrap();
    raw.push(b' ');
    fs::write(file, raw).unwrap();
    l.amend();
    l.refused("signature does not verify");
}

#[test]
fn legacy_refuses_missing_withdrawn_and_corrupted_addressed_assets() {
    let mut l = Legacy::new();
    let index: Index =
        serde_json::from_slice(&fs::read(l.repo.join("v1/index.json")).unwrap()).unwrap();
    fs::write(
        l.repo
            .join(format!("v1/blob/{}.grainpack", index.entries[0].sha256)),
        "incorrect bytes",
    )
    .unwrap();
    l.amend();
    l.refused("signed size");
    let l = Legacy::new();
    let first: Value = serde_json::from_str(
        &publication::git(
            &l.repo,
            &["show", &format!("{}:v1/index.json", l.commits[0])],
            false,
        )
        .unwrap(),
    )
    .unwrap();
    let oid = publication::git(
        &l.repo,
        &[
            "rev-parse",
            &format!(
                "{}:v1/blob/{}.mcp.json",
                l.commits[0],
                first["entries"][1]["sha256"].as_str().unwrap()
            ),
        ],
        false,
    )
    .unwrap();
    // Remove the object itself only inside this disposable complete fixture.
    fs::remove_file(
        l.repo
            .join(".git/objects")
            .join(&oid.trim()[..2])
            .join(&oid.trim()[2..]),
    )
    .unwrap();
    l.refused("committed blob unavailable");
}

#[test]
fn legacy_refuses_executable_assets_and_existing_revocation_evidence() {
    let mut l = Legacy::new();
    let index: Index =
        serde_json::from_slice(&fs::read(l.repo.join("v1/index.json")).unwrap()).unwrap();
    let file = format!("v1/blob/{}.grainpack", index.entries[0].sha256);
    git(&l.repo, &["update-index", "--chmod=+x", &file]);
    git(&l.repo, &["commit", "--amend", "--no-edit"]);
    l.commits[1] = git(&l.repo, &["rev-parse", "HEAD"]);
    l.write_manifest(&l.commits, &l.commits[1]);
    l.refused("non-executable");
    let mut l = Legacy::new();
    fs::write(
        l.repo.join("v1/revocations.json"),
        "must not discard known revocations",
    )
    .unwrap();
    l.amend();
    l.refused("revocation evidence");
}

#[test]
fn legacy_deduplication_cannot_hide_changed_committed_bytes_for_same_address() {
    let mut l = Legacy::new();
    let index: Index =
        serde_json::from_slice(&fs::read(l.repo.join("v1/index.json")).unwrap()).unwrap();
    let path = l.repo.join(format!(
        "v1/media/{}.md",
        index.entries[0].detail_document_hash()
    ));
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] ^= 1;
    fs::write(path, bytes).unwrap();
    l.amend();
    l.refused("address changed bytes between proofs");
}

#[test]
fn legacy_validates_even_historical_sizes_unknown_manifest_fields_and_output_boundaries() {
    let mut l = Legacy::new();
    let dir = l.repo.join("v1");
    let mut index = l.f.json(&dir, "index.json");
    index["entries"][0]["size"] = json!(grain_sdk::PACK_MAX_BYTES + 1);
    l.f.signed(&dir, "index.json", &index, "publisher");
    l.amend();
    l.refused("digest/size");
    let l = Legacy::new();
    fs::write(
        &l.manifest,
        json!({"schema":1,"tip_commit":l.commits[1],"commits":l.commits,"approval":true})
            .to_string(),
    )
    .unwrap();
    l.refused("unknown field");
    l.write_manifest(&l.commits, &l.commits[1]);
    assert!(capture_with(
        &l.repo,
        "example/registry",
        &l.manifest,
        &l.pin(),
        &l.repo.join("contained"),
        &l.f.anchor()
    )
    .unwrap_err()
    .to_string()
    .contains("outside"));
    l.run().unwrap();
    let original = fs::read(l.out.join("legacy-history.json")).unwrap();
    assert!(l.run().unwrap_err().to_string().contains("already exists"));
    assert_eq!(
        fs::read(l.out.join("legacy-history.json")).unwrap(),
        original
    );
}

#[test]
fn legacy_covers_signature_only_changes_and_rejects_shallow_or_redirecting_git() {
    let mut l = Legacy::new();
    let mut signature = fs::read(l.repo.join("v1/index.json.minisig")).unwrap();
    signature.extend(b"\n");
    fs::write(l.repo.join("v1/index.json.minisig"), signature).unwrap();
    git(&l.repo, &["add", "."]);
    git(&l.repo, &["commit", "-m", "signature-only change"]);
    l.commits.push(git(&l.repo, &["rev-parse", "HEAD"]));
    l.write_manifest(&l.commits[..2], &l.commits[2]);
    l.refused("complete catalogue-changing ancestry");
    l.write_manifest(&l.commits, &l.commits[2]);
    l.run().unwrap();
    let l = Legacy::new();
    fs::write(l.repo.join(".git/shallow"), &l.commits[0]).unwrap();
    l.refused("complete unmodified");
    let l = Legacy::new();
    git(
        &l.repo,
        &[
            "config",
            "--local",
            "remote.origin.url",
            "https://github.com/elsewhere/registry.git",
        ],
    );
    l.refused("origin differs");
}

#[test]
fn legacy_recovers_missing_asset_only_from_pinned_history_and_records_source() {
    let mut l = Legacy::new();
    let dir = l.repo.join("v1");
    let index: Index = serde_json::from_slice(&fs::read(dir.join("index.json")).unwrap()).unwrap();
    let name = format!("media/{}.md", index.entries[0].detail_document_hash());
    fs::remove_file(dir.join(&name)).unwrap();
    l.amend();
    l.run().unwrap();
    let receipt: Value =
        serde_json::from_slice(&fs::read(l.out.join("legacy-history.json")).unwrap()).unwrap();
    assert_eq!(receipt["asset_sources"][&name], l.commits[0]);
    assert_eq!(receipt["proofs"][1]["recovered_assets"], json!([name]));
    assert!(l.out.join("assets").join(name).is_file());
    // Recovery cannot substitute an unreferenced package or omit an absent hash.
    let mut l = Legacy::new();
    let dir = l.repo.join("v1");
    let mut index = l.f.json(&dir, "index.json");
    index["entries"][0]["sha256"] = json!("0".repeat(64));
    l.f.signed(&dir, "index.json", &index, "publisher");
    l.amend();
    l.refused("Missing historically referenced committed asset");
}

#[test]
fn legacy_authenticates_retired_tier_without_widening_active_sdk_or_trust() {
    let mut l = Legacy::new();
    let dir = l.repo.join("v1");
    let mut index = l.f.json(&dir, "index.json");
    index["entries"][0]["tier"] = json!("builtin");
    l.f.signed(&dir, "index.json", &index, "publisher");
    l.amend();
    let bytes = fs::read(dir.join("index.json")).unwrap();
    let roots: Roots = serde_json::from_slice(&fs::read(dir.join("roots.json")).unwrap()).unwrap();
    let signature = fs::read_to_string(dir.join("index.json.minisig")).unwrap();
    grain_core::trust::verify_publisher_signature(&roots, &bytes, &signature).unwrap();
    assert!(grain_core::trust::verify_index(
        &roots,
        &bytes,
        &signature,
        None,
        Utc::now().timestamp(),
        false
    )
    .is_err());
    l.run().unwrap();
    assert_eq!(
        fs::read(l.out.join(format!("proofs/{}/index.json", l.commits[1]))).unwrap(),
        bytes
    );
    let mut modified = bytes;
    modified.push(b' ');
    assert!(grain_core::trust::verify_publisher_signature(&roots, &modified, &signature).is_err());
}

#[test]
fn legacy_rejects_unknown_artifact_formats_and_publisher_key_mismatch() {
    let mut l = Legacy::new();
    let dir = l.repo.join("v1");
    let mut index = l.f.json(&dir, "index.json");
    index["entries"][0]["artifact_kind"] = json!("unknown-format");
    l.f.signed(&dir, "index.json", &index, "publisher");
    l.amend();
    l.refused("unknown variant");
    let mut l = Legacy::new();
    let dir = l.repo.join("v1");
    let index = l.f.json(&dir, "index.json");
    l.f.signed(&dir, "index.json", &index, "rotated");
    l.amend();
    l.refused("signature does not verify");
}
