//! Fresh, read-only GitHub source review. Never execute author code or open keys.
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use grain_sdk::submission::SourceSubmission;
use serde_json::Value;

use crate::{prepare::digest, review::Policy};

fn submission(path: &Path) -> Result<SourceSubmission> {
    let id = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("Submission directory needs its extension ID")?;
    grain_extension_checks::read_submission(path, id).map_err(anyhow::Error::msg)
}

pub(super) fn inspect(path: &Path, out: &Path) -> Result<()> {
    let value = submission(path)?;
    crate::prepare::write_artifact(path, out, &serde_json::to_vec(&value)?)
}

pub(super) fn check(path: &Path, policy_path: &Path, pin: &str, gh: &Path) -> Result<()> {
    let bytes = crate::review::read(policy_path, 32 * 1024)?;
    if digest(&bytes) != pin {
        bail!("Protected review policy digest mismatch");
    }
    let policy: Policy = serde_json::from_slice(&bytes)?;
    policy.validate(Utc::now().timestamp())?;
    let source = submission(path)?;
    if digest(&serde_json::to_vec(&source)?) != policy.submission_sha256 {
        bail!("Submission differs from protected review policy");
    }
    let gh = crate::review::verifier(gh, &policy.verifier_sha256)?;
    verify(&gh, &policy, &source)?;
    println!("Current GitHub source review matches the protected policy; no key accessed");
    Ok(())
}

// Only fixed GET requests to GitHub through an independently pinned executable.
// Inherit the operator's CLI authentication, not arbitrary process environment.
fn api(gh: &Path, endpoint: &str, raw: bool, max: u64) -> Result<Vec<u8>> {
    let scratch = tempfile::tempdir()?;
    let mut output = tempfile::tempfile()?;
    let mut cmd = Command::new(gh);
    cmd.env_clear()
        .args([
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            "--header",
            "X-GitHub-Api-Version:2022-11-28",
            "--header",
        ])
        .arg(if raw {
            "Accept:application/vnd.github.raw+json"
        } else {
            "Accept:application/vnd.github+json"
        })
        .arg(endpoint)
        .current_dir(scratch.path())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::from(output.try_clone()?));
    for key in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "HOME",
        "USERPROFILE",
        "GH_CONFIG_DIR",
        "APPDATA",
        "LOCALAPPDATA",
        "GH_TOKEN",
        "GITHUB_TOKEN",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    cmd.env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "")
        .env("TEMP", scratch.path())
        .env("TMP", scratch.path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().context("Start pinned GitHub review reader")?;
    if !crate::review::wait_verifier(&mut child, &output, Duration::from_secs(60), max)?.success() {
        bail!(
            "GitHub review read failed; check read-only CLI authentication and repository access"
        );
    }
    output.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    output.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        bail!("GitHub review response exceeded its limit");
    }
    Ok(bytes)
}

fn validate_pull(pull: &Value, policy: &Policy) -> Result<()> {
    if pull["number"].as_u64() != Some(policy.review_pull_request)
        || pull["state"].as_str() != Some("closed")
        || pull["merged"].as_bool() != Some(true)
        || pull["base"]["repo"]["full_name"].as_str() != Some(&policy.registry_repo)
        || pull["base"]["ref"].as_str() != policy.registry_ref.strip_prefix("refs/heads/")
        || pull["merge_commit_sha"].as_str() != Some(&policy.registry_commit)
        || pull["head"]["sha"].as_str() != Some(&policy.review_head)
        || !pull["user"]["login"]
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case(&policy.submitter))
    {
        bail!(
            "Source review must identify the merged registry commit, reviewed head and submitter"
        );
    }
    Ok(())
}

fn validate_reviews(reviews: &[Value], policy: &Policy, now: i64) -> Result<()> {
    let mut latest: Option<(i64, u64, &Value)> = None;
    for review in reviews {
        if !review["user"]["login"]
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case(&policy.reviewer))
        {
            continue;
        }
        let state = review["state"]
            .as_str()
            .context("Malformed reviewer state")?;
        if matches!(state, "COMMENTED" | "PENDING") {
            continue;
        }
        if !matches!(state, "APPROVED" | "CHANGES_REQUESTED" | "DISMISSED") {
            bail!("Unsupported reviewer state");
        }
        let time = DateTime::parse_from_rfc3339(
            review["submitted_at"]
                .as_str()
                .context("Review has no submission time")?,
        )?
        .timestamp();
        let id = review["id"]
            .as_u64()
            .filter(|id| *id > 0)
            .context("Review has no ID")?;
        if latest
            .as_ref()
            .is_none_or(|previous| (time, id) > (previous.0, previous.1))
        {
            latest = Some((time, id, review));
        }
    }
    let (time, id, review) = latest.context("No effective approval from the protected reviewer")?;
    let approved = DateTime::parse_from_rfc3339(&policy.approved_at)?.timestamp();
    if id != policy.review_id
        || review["user"]["type"].as_str() != Some("User")
        || review["state"].as_str() != Some("APPROVED")
        || review["commit_id"].as_str() != Some(&policy.review_head)
        || time > approved
        || time > now
    {
        bail!("GitHub approval is revoked, superseded, future-dated or for another source head");
    }
    Ok(())
}

pub(super) fn verify(gh: &Path, policy: &Policy, source: &SourceSubmission) -> Result<()> {
    let prefix = format!(
        "repos/{}/pulls/{}",
        policy.registry_repo, policy.review_pull_request
    );
    let pull = serde_json::from_slice(&api(gh, &prefix, false, 1024 * 1024)?)?;
    validate_pull(&pull, policy)?;
    let content = format!(
        "repos/{}/contents/extensions/{}/",
        policy.registry_repo, source.id
    );
    let remote = api(
        gh,
        &format!("{content}submission.toml?ref={}", policy.registry_commit),
        true,
        32 * 1024,
    )?;
    let remote = grain_extension_checks::parse_submission(std::str::from_utf8(&remote)?)
        .map_err(anyhow::Error::msg)?;
    if serde_json::to_vec(&remote)? != serde_json::to_vec(source)? {
        bail!("Merged submission differs from prepared source request");
    }
    let description = api(
        gh,
        &format!("{content}DESCRIPTION.md?ref={}", policy.registry_commit),
        true,
        64 * 1024,
    )?;
    if description.len() as u64 != source.description_size
        || digest(&description) != source.description_sha256
    {
        bail!("Merged description differs from reviewed listing");
    }
    let mut reviews = Vec::new();
    for page in 1..=10 {
        let batch: Vec<Value> = serde_json::from_slice(&api(
            gh,
            &format!("{prefix}/reviews?per_page=100&page={page}"),
            false,
            1024 * 1024,
        )?)?;
        let count = batch.len();
        if count > 100 {
            bail!("Unexpected GitHub review page size");
        }
        reviews.extend(batch);
        if count < 100 {
            return validate_reviews(&reviews, policy, Utc::now().timestamp());
        }
    }
    bail!("Review history exceeds the bounded reader; cannot establish latest approval")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixtures() -> (Policy, Value, Value) {
        let p = crate::review::tests::policy();
        let pull = json!({"number":p.review_pull_request,"state":"closed","merged":true,"base":{"repo":{"full_name":p.registry_repo},"ref":"main"},"merge_commit_sha":p.registry_commit,"head":{"sha":p.review_head},"user":{"login":p.submitter}});
        let review = json!({"id":p.review_id,"user":{"login":p.reviewer,"type":"User"},"state":"APPROVED","commit_id":p.review_head,"submitted_at":p.approved_at});
        (p, pull, review)
    }

    #[test]
    fn approval_binds_merge_head_repository_branch_and_submitter() {
        let (p, pull, _) = fixtures();
        validate_pull(&pull, &p).unwrap();
        for pointer in [
            "/number",
            "/state",
            "/merged",
            "/base/repo/full_name",
            "/base/ref",
            "/merge_commit_sha",
            "/head/sha",
            "/user/login",
        ] {
            let mut bad = pull.clone();
            *bad.pointer_mut(pointer).unwrap() = Value::Null;
            assert!(validate_pull(&bad, &p).is_err(), "{pointer}");
        }
    }

    #[test]
    fn review_requires_latest_effective_approval_and_exact_head() {
        let (p, _, review) = fixtures();
        let now = Utc::now().timestamp();
        validate_reviews(&[review.clone()], &p, now).unwrap();
        for (field, value) in [
            ("state", json!("DISMISSED")),
            ("state", json!("CHANGES_REQUESTED")),
            ("id", json!(999)),
            ("commit_id", json!("d".repeat(40))),
            ("submitted_at", json!("2099-01-01T00:00:00Z")),
        ] {
            let mut bad = review.clone();
            bad[field] = value;
            assert!(validate_reviews(&[bad], &p, now).is_err(), "{field}");
        }
        for state in ["DISMISSED", "CHANGES_REQUESTED", "APPROVED"] {
            let mut newer = review.clone();
            newer["id"] = json!(p.review_id + 1);
            newer["state"] = json!(state);
            assert!(validate_reviews(&[newer, review.clone()], &p, now).is_err());
        }
        let mut comment = review.clone();
        comment["id"] = json!(p.review_id + 1);
        comment["state"] = json!("COMMENTED");
        validate_reviews(&[review.clone(), comment], &p, now).unwrap();
        let mut other = review.clone();
        other["user"]["login"] = json!(p.submitter);
        assert!(validate_reviews(&[other], &p, now).is_err());
        assert!(validate_reviews(&[], &p, now).is_err());
    }
}
