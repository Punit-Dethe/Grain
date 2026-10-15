//! Real local Git/submission validation; only GitHub responses are substituted.
use super::*;
use serde_json::json;

const ID: &str = "com.example.tools";
const TARGET: &str = "example/registry";
const FORK: &str = "author/registry";
const URL: &str = "https://github.com/example/registry/pull/123";

struct Fixture {
    _temp: tempfile::TempDir,
    root: std::path::PathBuf,
    base: String,
}
impl Fixture {
    fn git(&self, args: &[&str]) -> String {
        command("git", args, &self.root, "Fixture Git")
            .unwrap()
            .trim()
            .to_owned()
    }
    fn commit(&self) {
        self.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.hooksPath=",
            "commit",
            "-qm",
            "fixture",
        ]);
    }
    fn new(mcp: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("registry");
        fs::create_dir(&root).unwrap();
        let mut f = Self {
            _temp: temp,
            root,
            base: String::new(),
        };
        f.git(&["init", "--initial-branch=main"]);
        f.git(&["config", "--local", "core.autocrlf", "false"]);
        let source = if mcp {
            crate::init_mcp_project(
                f._temp.path(),
                "Tools",
                Some(ID),
                "https://tools.example/mcp",
                "oauth",
            )
            .unwrap()
        } else {
            crate::init_project(f._temp.path(), "Tools", Some(ID)).unwrap()
        };
        fs::write(
            f.root.join("README.md"),
            fs::read(source.root.join("DESCRIPTION.md")).unwrap(),
        )
        .unwrap();
        f.git(&["add", "README.md"]);
        f.commit();
        f.base = f.git(&["rev-parse", "HEAD"]);
        f.git(&["switch", "-c", "submission/one"]);
        crate::run(
            [
                "submit",
                "--registry",
                f.root.to_str().unwrap(),
                "--repo",
                "https://github.com/example/tools",
                "--tag",
                "v1.0.0",
                "--commit",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "--contact",
                "fixture",
            ]
            .map(String::from),
            &source.root,
        )
        .unwrap();
        f.git(&["add", "extensions"]);
        f.commit();
        f
    }
    fn response(&self, args: &[&str], head: &str) -> Result<String> {
        Ok(match args {
            ["api", ..] if args.last().unwrap().contains("/git/ref/heads/main") => {
                json!({"object":{"sha":self.base}}).to_string()
            }
            ["api", ..]
                if args
                    .last()
                    .unwrap()
                    .contains("/git/ref/heads/submission%2Fone") =>
            {
                json!({"object":{"sha":head}}).to_string()
            }
            ["api", ..] => {
                json!({"parent":{"full_name":TARGET},"owner":{"type":"User"}}).to_string()
            }
            ["pr", "list", ..] => {
                assert_eq!(
                    args[args.iter().position(|v| *v == "--head").unwrap() + 1],
                    "submission/one"
                );
                "[]".into()
            }
            ["pr", "create", ..] => {
                assert!(args.contains(&"--draft"));
                assert_eq!(
                    args[args.iter().position(|v| *v == "--head").unwrap() + 1],
                    "author:submission/one"
                );
                let body = fs::read_to_string(
                    args[args.iter().position(|v| *v == "--body-file").unwrap() + 1],
                )?;
                assert!(body.contains(ID) && body.contains("publication remain separate"));
                URL.into()
            }
            ["pr", "view", ..] => json!({"headRefOid":head}).to_string(),
            _ => panic!("unexpected GH operation {args:?}"),
        })
    }
}

#[test]
fn native_and_mcp_create_draft_pr_without_changing_checkout_or_source() {
    for mcp in [false, true] {
        let f = Fixture::new(mcp);
        let head = f.git(&["rev-parse", "HEAD"]);
        let index = f.git(&["ls-files", "--stage"]);
        let mut creates = 0;
        let result = publish(&f.root, ID, TARGET, FORK, "main", |args| {
            if args.starts_with(&["pr", "create"]) {
                creates += 1;
            }
            f.response(args, &head)
        })
        .unwrap();
        assert!(result.contains(URL));
        assert_eq!(creates, 1);
        assert_eq!(f.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(f.git(&["ls-files", "--stage"]), index);
        assert!(f.git(&["status", "--porcelain"]).is_empty());
        assert!(!f.root.join(".git/FETCH_HEAD").exists());
    }
}

#[test]
fn existing_pr_is_returned_without_overwriting_description_or_review_state() {
    let f = Fixture::new(true);
    let head = f.git(&["rev-parse", "HEAD"]);
    let result = publish(&f.root, ID, TARGET, FORK, "main", |args| {
        if args.starts_with(&["pr", "list"]) {
            return Ok(
                json!([{"url":"https://github.com/example/registry/pull/122","headRefOid":head,
            "headRepository":{"name":"registry"},"headRepositoryOwner":{"login":"another-author"}},
            {"url":URL,"headRefOid":head,
            "headRepository":{"name":"registry"},"headRepositoryOwner":{"login":"author"}}])
                .to_string(),
            );
        }
        assert!(!args.starts_with(&["pr", "create"]) && !args.starts_with(&["pr", "edit"]));
        f.response(args, &head)
    })
    .unwrap();
    assert!(result.contains("Existing submission PR"));
}

#[test]
fn dirty_hidden_uncommitted_and_unrelated_work_cannot_be_submitted() {
    let f = Fixture::new(true);
    fs::write(f.root.join("unrelated"), "local work").unwrap();
    assert!(publish(&f.root, ID, TARGET, FORK, "main", |_| panic!(
        "dirty checkout reached GH"
    ))
    .is_err());
    fs::remove_file(f.root.join("unrelated")).unwrap();
    let path = format!("extensions/{ID}/submission.toml");
    f.git(&["update-index", "--assume-unchanged", &path]);
    let file = f.root.join(&path);
    let old = fs::read(&file).unwrap();
    let mut hidden = old.clone();
    hidden.extend_from_slice(b"\n# hidden local edit\n");
    fs::write(&file, hidden).unwrap();
    assert!(f.git(&["status", "--porcelain"]).is_empty());
    assert!(publish(&f.root, ID, TARGET, FORK, "main", |_| panic!(
        "hidden edit reached GH"
    ))
    .is_err());
    fs::write(file, old).unwrap();
    f.git(&["update-index", "--no-assume-unchanged", &path]);
    // Default rename detection would hide the deleted out-of-scope README.
    fs::remove_file(f.root.join("README.md")).unwrap();
    f.git(&["add", "README.md"]);
    f.commit();
    let head = f.git(&["rev-parse", "HEAD"]);
    assert!(f
        .git(&[
            "diff",
            "--find-renames",
            "--name-status",
            &format!("{}...{head}", f.base)
        ])
        .contains("R100"));
    let error = publish(&f.root, ID, TARGET, FORK, "main", |args| {
        assert!(!args.starts_with(&["pr", "create"]));
        f.response(args, &head)
    })
    .unwrap_err();
    assert!(error.to_string().contains("only this extension"));
}

#[test]
fn wrong_remote_fork_or_uncertain_create_never_retries_or_claims_success() {
    let f = Fixture::new(true);
    let head = f.git(&["rev-parse", "HEAD"]);
    assert!(publish(&f.root, ID, TARGET, FORK, "main", |args| f
        .response(args, &"f".repeat(40)))
    .is_err());
    assert!(publish(&f.root, ID, TARGET, FORK, "main", |args| {
        if args.last() == Some(&"repos/author/registry") {
            return Ok(json!({"parent":{"full_name":"unrelated/registry"}}).to_string());
        }
        f.response(args, &head)
    })
    .is_err());
    let mut creates = 0;
    let error = publish(&f.root, ID, TARGET, FORK, "main", |args| {
        if args.starts_with(&["pr", "create"]) {
            creates += 1;
            bail!("lost response");
        }
        f.response(args, &head)
    })
    .unwrap_err();
    assert_eq!(creates, 1);
    assert!(format!("{error:#}").contains("No automatic retry"));
    let error = publish(&f.root, ID, TARGET, FORK, "main", |args| {
        if args.starts_with(&["pr", "view"]) {
            return Ok(json!({"headRefOid":"f".repeat(40)}).to_string());
        }
        f.response(args, &head)
    })
    .unwrap_err();
    assert!(error.to_string().contains("head changed"));
    assert!(error.to_string().contains(URL));
}

#[test]
fn malformed_options_and_identities_fail_before_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    for args in [
        vec!["--unexpected", "x"],
        vec!["--registry"],
        vec!["--repo", "a/b", "--repo", "c/d"],
    ] {
        assert!(run(temp.path(), args.into_iter().map(String::from)).is_err());
    }
    for repo in [
        "a/b/c",
        "-a/b",
        "a/../b",
        "https://github.com/a/b",
        "a/b.git",
    ] {
        assert!(!repository(repo));
    }
    assert_eq!(component("feature/one%two"), "feature%2Fone%25two");
    assert!(pr_url("https://evil.invalid/example/registry/pull/1", TARGET).is_err());
    assert!(pr_url("https://github.com/EXAMPLE/Registry/pull/123", TARGET).is_ok());
}
