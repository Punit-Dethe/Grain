//! Optional author convenience: existing Git/GH tooling owns branches and forks.
//! Only an already committed/pushed, scoped submission may become a draft PR.
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use serde_json::Value;

pub(super) fn run(cwd: &Path, mut args: impl Iterator<Item = String>) -> Result<String> {
    let mut options = BTreeMap::new();
    while let Some(flag) = args.next() {
        if !matches!(
            flag.as_str(),
            "--registry" | "--id" | "--repo" | "--head-repo" | "--base"
        ) {
            bail!("unknown pr option '{flag}'");
        }
        let value = args
            .next()
            .with_context(|| format!("{flag} requires a value"))?;
        if options.insert(flag, value).is_some() {
            bail!("pr options may be supplied only once");
        }
    }
    for key in ["--registry", "--id", "--repo", "--head-repo"] {
        if !options.contains_key(key) {
            bail!("pr requires {key}");
        }
    }
    let registry = cwd.join(&options["--registry"]).canonicalize()?;
    publish(
        &registry,
        &options["--id"],
        &options["--repo"],
        &options["--head-repo"],
        options.get("--base").map(String::as_str).unwrap_or("main"),
        |args| command("gh", args, &registry, "GitHub PR operation"),
    )
}

fn repository(value: &str) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 100
                && !p.starts_with('-')
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
        && !value.ends_with(".git")
}

fn component(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn oid(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn pr_url(value: &str, target: &str) -> Result<String> {
    let value = value.trim();
    let prefix = format!("https://github.com/{target}/pull/");
    if !value
        .get(..prefix.len())
        .is_some_and(|part| part.eq_ignore_ascii_case(&prefix))
    {
        bail!("GitHub did not return an exact target PR URL; inspect before retrying");
    }
    let number = &value[prefix.len()..];
    if number.is_empty()
        || number.len() > 20
        || !number.bytes().all(|b| b.is_ascii_digit())
        || number.bytes().all(|b| b == b'0')
    {
        bail!("Invalid GitHub PR URL; inspect before retrying");
    }
    Ok(value.to_owned())
}

fn publish(
    root: &Path,
    id: &str,
    target: &str,
    head_repo: &str,
    base: &str,
    mut gh: impl FnMut(&[&str]) -> Result<String>,
) -> Result<String> {
    grain_sdk::validate_extension_id(id).map_err(anyhow::Error::msg)?;
    if !repository(target) || !repository(head_repo) {
        bail!("Expected GitHub OWNER/REPO identities");
    }
    let git = |args: &[&str]| command("git", args, root, "Local Git validation");
    if Path::new(git(&["rev-parse", "--show-toplevel"])?.trim()).canonicalize()?
        != root.canonicalize()?
    {
        bail!("--registry must be the registry checkout root");
    }
    if !git(&["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty() {
        bail!("Registry checkout has uncommitted files; commit the submission on a dedicated branch first. Nothing changed");
    }
    git(&["check-ref-format", "--branch", base])?;
    let branch = git(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        .context("Use a named submission branch; detached HEAD cannot open a PR")?;
    let branch = branch.trim();
    if branch == base {
        bail!("Use a dedicated submission branch, not the target base branch");
    }
    let head = git(&["rev-parse", "--verify", "HEAD"])?;
    let head = head.trim();
    if !oid(head) {
        bail!("Expected a full GitHub-compatible commit ID");
    }
    let folder = root.join("extensions").join(id);
    let source =
        grain_extension_checks::read_submission(&folder, id).map_err(anyhow::Error::msg)?;
    for file in ["submission.toml", "DESCRIPTION.md"] {
        let path = format!("extensions/{id}/{file}");
        if git(&["cat-file", "blob", &format!("{head}:{path}")])?.as_bytes()
            != fs::read(folder.join(file))?
        {
            bail!("Submission differs from committed bytes; commit it before opening a PR");
        }
    }
    let head_endpoint = format!("repos/{head_repo}/git/ref/heads/{}", component(branch));
    let remote: Value = serde_json::from_str(&gh(&[
        "api",
        "--hostname",
        "github.com",
        "--method",
        "GET",
        &head_endpoint,
    ])?)?;
    if remote["object"]["sha"].as_str() != Some(head) {
        bail!("Push this exact submission branch to --head-repo first; no PR created");
    }
    if !head_repo.eq_ignore_ascii_case(target) {
        let endpoint = format!("repos/{head_repo}");
        let repo: Value = serde_json::from_str(&gh(&[
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            &endpoint,
        ])?)?;
        if !repo["parent"]["full_name"]
            .as_str()
            .is_some_and(|p| p.eq_ignore_ascii_case(target))
        {
            bail!("--head-repo must be the target registry or its direct GitHub fork");
        }
        if repo["owner"]["type"].as_str() != Some("User") {
            bail!("The current GH CLI requires a personal fork for --head; open an organization-fork PR manually");
        }
    }
    let base_endpoint = format!("repos/{target}/git/ref/heads/{}", component(base));
    let remote: Value = serde_json::from_str(&gh(&[
        "api",
        "--hostname",
        "github.com",
        "--method",
        "GET",
        &base_endpoint,
    ])?)?;
    let base_sha = remote["object"]["sha"]
        .as_str()
        .filter(|v| oid(v))
        .context("Invalid target base commit")?;
    let range = format!("{base_sha}...{head}");
    let changed = git(&["diff", "--no-renames", "--name-only", "-z", &range, "--"]).context(
        "Fetch the target base/history first; the helper never fetches or rewrites your branch",
    )?;
    let allowed = [
        format!("extensions/{id}/submission.toml"),
        format!("extensions/{id}/DESCRIPTION.md"),
    ];
    let paths: Vec<_> = changed.split('\0').filter(|s| !s.is_empty()).collect();
    if paths.is_empty() || paths.iter().any(|p| !allowed.iter().any(|v| v == p)) {
        bail!("PR must change only this extension's submission and DESCRIPTION; split unrelated work into another branch");
    }
    let owner = head_repo.split('/').next().unwrap();
    let qualified = if head_repo.eq_ignore_ascii_case(target) {
        branch.to_owned()
    } else {
        format!("{owner}:{branch}")
    };
    let open: Vec<Value> = serde_json::from_str(&gh(&[
        "pr",
        "list",
        "--repo",
        target,
        "--base",
        base,
        "--head",
        branch,
        "--state",
        "open",
        "--limit",
        "100",
        "--json",
        "url,headRepository,headRepositoryOwner,headRefOid",
    ])?)?;
    // GH's list --head accepts a branch name, unlike create's owner:branch.
    // Filter exact repository identity ourselves; another fork can use the same name.
    if open.len() >= 100 {
        bail!("Too many PRs for this branch name; inspect them manually");
    }
    let matching: Vec<_> = open
        .iter()
        .filter(|pr| {
            let actual = format!(
                "{}/{}",
                pr["headRepositoryOwner"]["login"].as_str().unwrap_or(""),
                pr["headRepository"]["name"].as_str().unwrap_or("")
            );
            actual.eq_ignore_ascii_case(head_repo)
        })
        .collect();
    if !matching.is_empty() {
        if matching.len() != 1 {
            bail!("Ambiguous existing PRs; inspect them manually");
        }
        let pr = matching[0];
        if pr["headRefOid"].as_str() != Some(head) {
            bail!("Existing PR differs from the checked branch; inspect before retrying");
        }
        return pr_url(pr["url"].as_str().unwrap_or(""), target).map(|url| format!("Existing submission PR: {url}\nPushed commits update it; its description and review state were left unchanged."));
    }
    // A fixed explicit --head prevents GH from implicitly pushing or forking.
    let title = format!("Submit {id} {}", source.version);
    let body = format!("Submit `{id}` version `{}`.\n\nSource: {}\nPinned commit: `{}`\n\nSource submission only. ID enrollment, maintainer review, CI provenance, signing and publication remain separate.\n", source.version, source.source_repo, source.commit);
    let scratch = tempfile::tempdir()?;
    let body_path = scratch.path().join("body.md");
    fs::write(&body_path, body)?;
    let url = gh(&["pr", "create", "--repo", target, "--base", base, "--head", &qualified,
        "--draft", "--title", &title, "--body-file", body_path.to_str().context("PR body path is not UTF-8")?])
        .context("PR creation outcome unconfirmed; inspect GitHub for this branch before repeating. No automatic retry")?;
    let url = pr_url(&url, target)?;
    let state: Value = serde_json::from_str(&gh(&["pr", "view", &url, "--repo", target, "--json", "headRefOid"])
        .with_context(|| format!("Draft PR created at {url}, but head verification failed; inspect it before retrying"))?)?;
    if state["headRefOid"].as_str() != Some(head) {
        bail!("Draft PR created at {url}, but its head changed; inspect it before further action");
    }
    Ok(format!(
        "Draft submission PR: {url}\nNo merge, approval, signing or publication performed."
    ))
}

// Bounded/reaped children, fixed argv and no shell. Raw network/credential errors
// are withheld; stage plus exit status identifies the failed author operation.
fn command(program: &str, args: &[&str], cwd: &Path, stage: &str) -> Result<String> {
    let operation = if program == "gh" && args.first() == Some(&"pr") {
        format!("pr {}", args.get(1).unwrap_or(&"operation"))
    } else {
        args.first().unwrap_or(&"command").to_string()
    };
    let stage = format!("{stage} [{program} {operation}]");
    let mut output = tempfile::tempfile()?;
    let errors = tempfile::tempfile()?;
    let mut cmd = Command::new(program);
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            cmd.env_remove(key);
        }
    }
    if program == "git" {
        cmd.args([
            "--no-pager",
            "--no-replace-objects",
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
        ]);
    }
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_HOST", "github.com")
        .env("GIT_NO_LAZY_FETCH", "1")
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(errors.try_clone()?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("{stage}: install {program} and check authentication"))?;
    let start = Instant::now();
    let status = (|| -> Result<_> {
        loop {
            if output.metadata()?.len() > 256 * 1024
                || errors.metadata()?.len() > 256 * 1024
                || start.elapsed() > Duration::from_secs(90)
            {
                bail!("{stage} exceeded output/time limits; inspect before retrying");
            }
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    let status = match status {
        Ok(s) => s,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    if !status.success() {
        bail!("{stage} failed ({status}); check gh auth status and the selected repository/branch. Raw child output withheld");
    }
    output.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    output.take(256 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 256 * 1024 {
        bail!("{stage} output exceeds limit");
    }
    String::from_utf8(bytes).context("Tool returned invalid UTF-8")
}

#[cfg(test)]
#[path = "pull_request_tests.rs"]
mod tests;
