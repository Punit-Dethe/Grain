use super::*;
use publication::{Commit, InitialRequest};

struct Initial {
    f: Fixture,
    seed: PathBuf,
    tree: PathBuf,
    repo: PathBuf,
    bundle: PathBuf,
    base: String,
    candidate: String,
    pin: String,
    out: PathBuf,
}

impl Initial {
    fn new() -> Self {
        let f = Fixture::new();
        let seed = bootstrap_tests::empty_seed(&f);
        let tree = f.root.path().join("bootstrap");
        bootstrap::create_with(
            &seed,
            &f.root.path().join("keys/publisher.key"),
            1,
            &tree,
            &f.anchor(),
        )
        .unwrap();
        let repo = f.root.path().join("checkout");
        fs::create_dir(&repo).unwrap();
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
        fs::write(repo.join("README.md"), "preserve unrelated source\n").unwrap();
        fs::create_dir(repo.join("v1")).unwrap();
        fs::write(
            repo.join("v1/obsolete.json"),
            "experimental data; no migration",
        )
        .unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "experimental base"]);
        let base = git(&repo, &["rev-parse", "HEAD"]);
        git(&repo, &["rm", "-r", "v1"]);
        let mut initial = Self {
            seed,
            tree,
            repo,
            base,
            f,
            bundle: PathBuf::new(),
            candidate: String::new(),
            pin: String::new(),
            out: PathBuf::new(),
        };
        initial.out = initial.f.root.path().join("handoff");
        initial.export("original");
        git(&initial.repo, &["add", "."]);
        git(&initial.repo, &["commit", "-m", "clean first publication"]);
        initial.candidate = git(&initial.repo, &["rev-parse", "HEAD"]);
        initial
    }
    fn export(&mut self, label: &str) {
        let store = self.f.root.path().join(format!("store-{label}"));
        initialize_with(&self.tree.join("v1"), &store, &self.f.anchor()).unwrap();
        self.bundle = self.f.root.path().join(format!("bundle-{label}"));
        let pointer = digest(&fs::read(store.join("current.json")).unwrap());
        hosting::export(&store, &self.bundle, &pointer, &self.f.anchor()).unwrap();
        self.pin = digest(&fs::read(self.bundle.join("bundle.json")).unwrap());
        Publication::install(&self.bundle, &self.repo);
    }
    fn request(&self) -> InitialRequest<'_> {
        InitialRequest {
            commit: Commit {
                checkout: &self.repo,
                repository: "example/registry",
                branch: "release",
                base_commit: &self.base,
                candidate_commit: &self.candidate,
            },
            bundle: &self.bundle,
            bundle_pin: &self.pin,
            out: &self.out,
        }
    }
    fn run(&self) -> Result<()> {
        publication::prepare_initial_with(&self.request(), &self.seed, &self.f.anchor())
    }
    fn refused(&self, error: &str) {
        assert!(
            self.run().unwrap_err().to_string().contains(error),
            "{error}"
        );
        assert!(!self.out.exists());
        assert_eq!(git(&self.repo, &["rev-parse", "HEAD"]), self.candidate);
    }
}

#[test]
fn first_publication_replaces_experimental_bytes_without_fabricated_previous_proof() {
    let p = Initial::new();
    fs::write(p.repo.join("v1/index.json"), "ignore dirty worktree").unwrap();
    p.run().unwrap();
    let raw = fs::read(p.out.join("publication.json")).unwrap();
    let handoff: Handoff = serde_json::from_slice(&raw).unwrap();
    assert!(handoff.previous_receipt_sha256.is_none());
    assert!(serde_json::from_slice::<Value>(&raw)
        .unwrap()
        .get("previous_receipt_sha256")
        .is_none());
    assert_eq!(
        handoff.evidence_class,
        "verified-git-initial-publication-handoff-not-release-approval"
    );
    assert_eq!(handoff.candidate_receipt_sha256, p.pin);
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
    let mut args = handoff.push_args;
    let url = args
        .iter()
        .position(|a| a == "https://github.com/example/registry.git")
        .unwrap();
    args[url] = remote.to_str().unwrap().into(); // Disposable remote only.
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    git(&p.repo, &refs);
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/release"]),
        p.candidate
    );
    assert_eq!(
        git(&remote, &["show", "refs/heads/release:README.md"]),
        "preserve unrelated source"
    );
    assert!(!invoke(
        &remote,
        &["cat-file", "-e", "refs/heads/release:v1/obsolete.json"]
    )
    .status
    .success());
    // A newer independent writer, rather than the expected base, must win.
    fs::write(p.repo.join("README.md"), "competitor").unwrap();
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
    assert!(!invoke(&p.repo, &refs).status.success());
    assert_eq!(git(&remote, &["rev-parse", "refs/heads/release"]), winner);
}

#[test]
fn first_publication_refuses_policy_versions_roots_and_excessive_lifetime() {
    for mode in ["version", "policy", "roots", "lifetime", "deadline"] {
        let mut p = Initial::new();
        let dir = p.tree.join("v1");
        let (name, key) = if mode == "roots" {
            ("roots.json", "root")
        } else {
            ("revocations.json", "publisher")
        };
        let mut value = p.f.json(&dir, name);
        match mode {
            "version" => value["version"] = json!(3),
            "policy" => {
                value["entries"] = json!([{"id":"com.example.injected", "state":"revoked", "reason":"different policy"}])
            }
            "roots" => value["base_urls"] = json!(["https://different.example.invalid/v1/"]),
            "lifetime" => value["expires"] = json!("2099-01-01T00:00:00Z"),
            _ => value["expires"] = json!((Utc::now() + chrono::Duration::days(2)).to_rfc3339()),
        }
        p.f.signed(&dir, name, &value, key);
        p.export("changed");
        p.refused(match mode {
            "roots" => "exact current app seed roots",
            "lifetime" => "thirty days",
            "deadline" => "share one expiry",
            _ => "current seed policy",
        });
    }
}

#[test]
fn first_publication_refuses_pin_tamper_parent_unrelated_paths_and_output_reuse() {
    let mut p = Initial::new();
    p.pin = "0".repeat(64);
    p.refused("receipt digest differs");
    p.pin = digest(&fs::read(p.bundle.join("bundle.json")).unwrap());
    fs::write(p.repo.join("v1/index.json"), "bad committed bytes").unwrap();
    git(&p.repo, &["add", "."]);
    git(&p.repo, &["commit", "--amend", "--no-edit"]);
    p.candidate = git(&p.repo, &["rev-parse", "HEAD"]);
    p.refused("Committed publication bytes differ");
    fs::write(p.repo.join("README.md"), "unrelated edit").unwrap();
    git(&p.repo, &["add", "."]);
    git(&p.repo, &["commit", "--amend", "--no-edit"]);
    p.candidate = git(&p.repo, &["rev-parse", "HEAD"]);
    p.refused("unrelated repository paths");
    let mut p = Initial::new();
    p.base = "0".repeat(40);
    p.refused("sole parent");
    let p = Initial::new();
    fs::create_dir(&p.out).unwrap();
    fs::write(p.out.join("keep"), "operator owned").unwrap();
    assert!(p.run().unwrap_err().to_string().contains("already exists"));
    assert_eq!(
        fs::read_to_string(p.out.join("keep")).unwrap(),
        "operator owned"
    );
}

#[test]
fn first_publication_refuses_nonempty_catalogue_and_inherited_history() {
    let p = Publication::new();
    let seed = bootstrap_tests::empty_seed(&p.f);
    let request = InitialRequest {
        commit: Commit {
            checkout: &p.repo,
            repository: "example/registry",
            branch: "release",
            base_commit: &p.base,
            candidate_commit: &p.candidate,
        },
        bundle: &p.bundle,
        bundle_pin: &p.pin,
        out: &p.out,
    };
    assert!(
        publication::prepare_initial_with(&request, &seed, &p.f.anchor())
            .unwrap_err()
            .to_string()
            .contains("ordinary update gate")
    );
    assert!(!p.out.exists());
    let mut p = Initial::new();
    let store = p.f.root.path().join("inherited seed store");
    initialize_with(&p.seed, &store, &p.f.anchor()).unwrap();
    let pointer = digest(&fs::read(store.join("current.json")).unwrap());
    let assembly = p.f.root.path().join("inherited assembly");
    assemble_with(&p.seed, &p.tree.join("v1"), &assembly, &p.f.anchor()).unwrap();
    promote_with(&assembly, &store, &pointer, &p.f.anchor()).unwrap();
    let pointer = digest(&fs::read(store.join("current.json")).unwrap());
    p.bundle = p.f.root.path().join("inherited bundle");
    hosting::export(&store, &p.bundle, &pointer, &p.f.anchor()).unwrap();
    p.pin = digest(&fs::read(p.bundle.join("bundle.json")).unwrap());
    p.refused("empty bootstrap");
    let mut p = Initial::new();
    let nonempty = p.f.tree("nonempty first", 2, false);
    p.tree = p.f.root.path().join("nonempty wrapper");
    fs::create_dir(&p.tree).unwrap();
    copy_dir(&nonempty, &p.tree.join("v1"));
    p.export("nonempty");
    p.refused("empty bootstrap");
}
