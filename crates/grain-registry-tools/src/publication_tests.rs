//! Real Minisign, Git objects and a disposable bare remote. No GitHub write,
//! public CLI trust override, author code or new Agent harness is involved.
#[path = "capture_tests.rs"]
mod capture_tests;
#[path = "initial_publication_tests.rs"]
mod initial_publication_tests;
use super::*;
use publication::{Handoff, Request};
use std::process::{Command, Output};

fn invoke(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .output()
        .unwrap()
}

fn git(root: &Path, args: &[&str]) -> String {
    let result = invoke(root, args);
    assert!(
        result.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}

fn copy_dir(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for item in fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_dir() {
            copy_dir(&item.path(), &target.join(item.file_name()));
        } else {
            fs::copy(item.path(), target.join(item.file_name())).unwrap();
        }
    }
}

struct Publication {
    f: Fixture,
    repo: PathBuf,
    previous: PathBuf,
    bundle: PathBuf,
    old_pin: String,
    pin: String,
    base: String,
    candidate: String,
    out: PathBuf,
}

impl Publication {
    fn new() -> Self {
        let f = Fixture::new();
        let base_tree = f.tree("base", 1, true);
        let next = f.tree("next", 2, false);
        let store = f.root.path().join("store");
        initialize_with(&base_tree, &store, &f.anchor()).unwrap();
        let previous = f.root.path().join("previous capture ü");
        let current_pin = digest(&fs::read(store.join("current.json")).unwrap());
        hosting::export(&store, &previous, &current_pin, &f.anchor()).unwrap();
        let assembly = f.root.path().join("assembly");
        assemble_with(&base_tree, &next, &assembly, &f.anchor()).unwrap();
        promote_with(&assembly, &store, &current_pin, &f.anchor()).unwrap();
        let bundle = f.root.path().join("next capture ü");
        let current_pin = digest(&fs::read(store.join("current.json")).unwrap());
        hosting::export(&store, &bundle, &current_pin, &f.anchor()).unwrap();
        let old_pin = digest(&fs::read(previous.join("bundle.json")).unwrap());
        let pin = digest(&fs::read(bundle.join("bundle.json")).unwrap());
        let repo = f.root.path().join("checkout");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "--initial-branch=release"]);
        // Local disposable identity only. Never changes machine/user identity.
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
        fs::write(repo.join("README.md"), "preserve repository files\n").unwrap();
        Self::install(&previous, &repo);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "previous"]);
        let base = git(&repo, &["rev-parse", "HEAD"]);
        Self::install(&bundle, &repo);
        git(&repo, &["add", "."]);
        // Windows CopyFile can preserve indistinguishable size/mtime tuples.
        // Force tracked fixture bytes into the index instead of trusting stat.
        git(&repo, &["add", "--renormalize", "."]);
        git(&repo, &["commit", "-m", "candidate"]);
        let candidate = git(&repo, &["rev-parse", "HEAD"]);
        let out = f.root.path().join("handoff");
        Self {
            f,
            repo,
            previous,
            bundle,
            old_pin,
            pin,
            base,
            candidate,
            out,
        }
    }
    fn install(bundle: &Path, repo: &Path) {
        copy_dir(&bundle.join("v1"), &repo.join("v1"));
        let proofs = repo.join(".registry-publication");
        fs::create_dir_all(&proofs).unwrap();
        for name in ["bundle.json", "current.json"] {
            fs::copy(bundle.join(name), proofs.join(name)).unwrap();
        }
        copy_dir(&bundle.join("history"), &proofs.join("history"));
    }
    fn request(&self) -> Request<'_> {
        Request {
            checkout: &self.repo,
            repository: "example/registry",
            branch: "release",
            base_commit: &self.base,
            candidate_commit: &self.candidate,
            previous: &self.previous,
            previous_pin: &self.old_pin,
            bundle: &self.bundle,
            bundle_pin: &self.pin,
            out: &self.out,
        }
    }
    fn run(&self) -> Result<()> {
        publication::prepare_with(&self.request(), &self.f.anchor())
    }
    fn amend(&mut self) {
        git(&self.repo, &["add", "."]);
        git(&self.repo, &["add", "--renormalize", "."]);
        git(&self.repo, &["commit", "--amend", "--no-edit"]);
        self.candidate = git(&self.repo, &["rev-parse", "HEAD"]);
    }
    fn refused(&self, expected: &str) {
        assert!(self.run().unwrap_err().to_string().contains(expected));
        assert!(!self.out.exists());
        assert_eq!(git(&self.repo, &["rev-parse", "HEAD"]), self.candidate);
    }
}

#[test]
fn signed_publication_binds_committed_bytes_and_preserves_withdrawn_assets() {
    let p = Publication::new();
    // Dirty worktree is intentionally not used as the publication authority.
    fs::write(p.repo.join("v1/index.json"), "not the committed catalogue").unwrap();
    p.run().unwrap();
    let receipt: Handoff =
        serde_json::from_slice(&fs::read(p.out.join("publication.json")).unwrap()).unwrap();
    assert_eq!(receipt.expected_base_commit, p.base);
    assert_eq!(receipt.candidate_commit, p.candidate);
    assert_eq!(receipt.candidate_receipt_sha256, p.pin);
    assert_eq!(
        receipt.push_args.last().unwrap(),
        &format!("{}:refs/heads/release", p.candidate)
    );
    assert!(receipt
        .push_args
        .contains(&format!("--force-with-lease=refs/heads/release:{}", p.base)));
    assert!(!receipt
        .push_args
        .iter()
        .any(|a| a == "--force" || a.starts_with('+')));
    assert_eq!(
        git(&p.repo, &["show", "HEAD:README.md"]),
        "preserve repository files"
    );
    assert_eq!(
        fs::read_to_string(p.repo.join("v1/index.json")).unwrap(),
        "not the committed catalogue"
    );
}

#[test]
fn conditional_handoff_advances_exact_base_and_rejects_competing_remote_writer() {
    let p = Publication::new();
    p.run().unwrap();
    let receipt: Handoff =
        serde_json::from_slice(&fs::read(p.out.join("publication.json")).unwrap()).unwrap();
    let remote = p.f.root.path().join("remote.git");
    git(
        p.f.root.path(),
        &["init", "--bare", remote.to_str().unwrap()],
    );
    git(
        &p.repo,
        &[
            "push",
            remote.to_str().unwrap(),
            &format!("{}:refs/heads/release", p.base),
        ],
    );
    assert_eq!(git(&remote, &["rev-parse", "refs/heads/release"]), p.base);
    let mut args = receipt.push_args;
    let url = args
        .iter()
        .position(|a| a == "https://github.com/example/registry.git")
        .unwrap();
    args[url] = remote.to_str().unwrap().into(); // Test-only local remote substitution.
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    git(&p.repo, &refs);
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/release"]),
        p.candidate
    );
    // A later independent writer wins. The old handoff must not rewind it.
    fs::write(p.repo.join("README.md"), "competitor\n").unwrap();
    git(&p.repo, &["add", "README.md"]);
    git(&p.repo, &["commit", "-m", "competing writer"]);
    let winner = git(&p.repo, &["rev-parse", "HEAD"]);
    git(
        &p.repo,
        &[
            "push",
            remote.to_str().unwrap(),
            &format!("{winner}:refs/heads/release"),
        ],
    );
    let refused = invoke(&p.repo, &refs);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stdout).contains("stale info"));
    assert_eq!(git(&remote, &["rev-parse", "refs/heads/release"]), winner);
    assert!(
        git(&remote, &["ls-tree", "-r", "refs/heads/release", "v1/blob"]).contains(".mcp.json")
    );
}

#[test]
fn publication_refuses_committed_tamper_extra_missing_and_executable_files() {
    for mode in ["tamper", "extra", "missing", "executable"] {
        let mut p = Publication::new();
        match mode {
            "tamper" => fs::write(p.repo.join("v1/index.json"), "tampered").unwrap(),
            "extra" => fs::write(p.repo.join("v1/extra.json"), "extra").unwrap(),
            "missing" => {
                git(&p.repo, &["rm", "v1/index.json.minisig"]);
            }
            _ => {
                git(&p.repo, &["update-index", "--chmod=+x", "v1/index.json"]);
            }
        }
        // Adding all would restore the executable index mode on some systems.
        if mode == "executable" {
            git(&p.repo, &["commit", "--amend", "--no-edit"]);
            p.candidate = git(&p.repo, &["rev-parse", "HEAD"]);
        } else {
            p.amend();
        }
        p.refused(match mode {
            "tamper" => "bytes differ",
            "executable" => "regular non-executable",
            _ => "inventory differs",
        });
    }
}

#[test]
fn publication_refuses_unrelated_changes_wrong_parent_origin_and_config_redirects() {
    let mut p = Publication::new();
    fs::write(p.repo.join("README.md"), "unrelated change").unwrap();
    p.amend();
    p.refused("unrelated repository");
    let p = Publication::new();
    let mut request = p.request();
    request.base_commit = "0000000000000000000000000000000000000000";
    assert!(publication::prepare_with(&request, &p.f.anchor())
        .unwrap_err()
        .to_string()
        .contains("sole parent"));
    git(
        &p.repo,
        &[
            "config",
            "--local",
            "remote.origin.url",
            "https://github.com/other/repository.git",
        ],
    );
    p.refused("origin differs");
    git(
        &p.repo,
        &[
            "config",
            "--local",
            "remote.origin.url",
            "https://github.com/example/registry.git",
        ],
    );
    git(
        &p.repo,
        &[
            "config",
            "--local",
            "url.https://elsewhere.invalid/.insteadOf",
            "https://github.com/",
        ],
    );
    p.refused("must not redirect");
}

#[test]
fn publication_refuses_erased_history_even_with_new_authentic_receipt() {
    let mut p = Publication::new();
    let stripped_store = p.f.root.path().join("stripped-store");
    initialize_with(
        &p.f.root.path().join("next"),
        &stripped_store,
        &p.f.anchor(),
    )
    .unwrap();
    let stripped = p.f.root.path().join("stripped-bundle");
    hosting::export(
        &stripped_store,
        &stripped,
        &digest(&fs::read(stripped_store.join("current.json")).unwrap()),
        &p.f.anchor(),
    )
    .unwrap();
    p.bundle = stripped;
    p.pin = digest(&fs::read(p.bundle.join("bundle.json")).unwrap());
    p.refused("retain every previous");
}

#[test]
fn publication_refuses_bad_pins_arguments_existing_output_and_containment() {
    let p = Publication::new();
    for (field, value, expected) in [
        (
            "repository",
            "https://github.com/example/registry",
            "OWNER/REPO",
        ),
        ("branch", "--force", "malformed"),
        ("branch", "release:other", "malformed"),
        ("candidate", "bad", "full Git SHA1"),
        ("previous_pin", "bad", "receipt digest required"),
        (
            "pin",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "differs from independent pin",
        ),
    ] {
        let mut r = p.request();
        match field {
            "repository" => r.repository = value,
            "branch" => r.branch = value,
            "candidate" => r.candidate_commit = value,
            "previous_pin" => r.previous_pin = value,
            _ => r.bundle_pin = value,
        }
        let error = publication::prepare_with(&r, &p.f.anchor())
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{field}: {error}");
        assert!(!p.out.exists());
    }
    let contained = p.repo.join("new-output");
    let mut r = p.request();
    r.out = &contained;
    assert!(publication::prepare_with(&r, &p.f.anchor())
        .unwrap_err()
        .to_string()
        .contains("outside"));
    fs::create_dir(&p.out).unwrap();
    fs::write(p.out.join("keep"), "preserve").unwrap();
    assert!(p.run().unwrap_err().to_string().contains("already exists"));
    assert_eq!(fs::read_to_string(p.out.join("keep")).unwrap(), "preserve");
}

#[test]
fn previous_publication_can_be_expired_but_candidate_verification_stays_fresh() {
    let p = Publication::new();
    let v1 = p.previous.join("v1");
    for name in ["index.json", "revocations.json"] {
        let mut value = p.f.json(&v1, name);
        value["expires"] = "2000-01-01T00:00:00Z".into();
        p.f.signed(&v1, name, &value, "publisher");
    }
    let expired = metadata(&v1, &p.f.anchor(), false).unwrap();
    let pointer = serde_json::to_vec(&expired.state).unwrap();
    fs::write(p.previous.join("current.json"), &pointer).unwrap();
    let raw = serde_json::to_vec(&hosting::Receipt {
        schema: 1,
        current_sha256: digest(&pointer),
        selected: expired.state.clone(),
        history: vec![],
    })
    .unwrap();
    fs::write(p.previous.join("bundle.json"), &raw).unwrap();
    let pin = digest(&raw);
    assert_eq!(
        hosting::inspect(&p.previous, &pin, &p.f.anchor(), false)
            .unwrap()
            .selected,
        expired.state
    );
    assert!(hosting::verify(&p.previous, &pin, &p.f.anchor())
        .unwrap_err()
        .to_string()
        .contains("Unsupported serving index/revocation version/spec"));
    assert!(hosting::inspect(&p.previous, &pin, &p.f.anchor(), true).is_err());
    // Old signatures remain mandatory even when expiry is allowed.
    fs::write(v1.join("index.json.minisig"), "not a signature").unwrap();
    assert!(hosting::inspect(&p.previous, &pin, &p.f.anchor(), false).is_err());
}

#[test]
fn publication_rejects_missing_baseline_and_reinterpreted_ancestry() {
    let mut p = Publication::new();
    git(&p.repo, &["rm", ".registry-publication/bundle.json"]);
    p.amend();
    p.refused("inventory differs");
    let p = Publication::new();
    fs::write(
        p.repo.join(".git/info/grafts"),
        format!("{} {}\n", p.candidate, p.base),
    )
    .unwrap();
    p.refused("unmodified local Git ancestry");
    // A wrong expected parent is tested separately; a graft must never make it pass.
}
