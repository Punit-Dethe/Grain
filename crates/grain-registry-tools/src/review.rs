//! Maintainer-only signing boundary. The policy and verifier pins must come
//! from protected review infrastructure, never from the author/build artifact.
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration as Days, Utc};
use grain_sdk::distribution::{Index, Roots};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::prepare::{digest, new_output, OwnedOutput};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    pub schema: u8,
    pub candidate_sha256: String,
    pub submission_sha256: String,
    pub receipt_sha256: String,
    pub producer_sha256: String,
    pub verifier_sha256: String,
    pub registry_repo: String,
    pub registry_commit: String,
    pub registry_ref: String,
    pub signer_workflow: String,
    pub signer_commit: String,
    pub reviewer: String,
    pub submitter: String,
    pub approved_at: String,
    pub expires_at: String,
    pub publishing_public_key: String,
    pub previous_index_sha256: String,
    pub previous_index_version: u64,
}

pub(super) struct Inputs<'a> {
    pub submission: &'a Path,
    pub prepared: &'a Path,
    pub candidate: &'a Path,
    pub policy: &'a Path,
    pub policy_pin: &'a str,
    pub gh: &'a Path,
    pub attestation: &'a Path,
    pub previous: &'a Path,
    pub key: &'a Path,
    pub out: &'a Path,
}

fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && value != "."
        && value != ".."
}

impl Policy {
    pub(super) fn validate(&self, now: i64) -> Result<()> {
        let hashes = [
            &self.candidate_sha256,
            &self.submission_sha256,
            &self.receipt_sha256,
            &self.producer_sha256,
            &self.verifier_sha256,
            &self.previous_index_sha256,
        ];
        if self.schema != 1
            || hashes.iter().any(|v| !hex(v, 64))
            || !hex(&self.registry_commit, 40)
            || !hex(&self.signer_commit, 40)
        {
            bail!("Unsupported review policy or malformed digest");
        }
        let repo: Vec<_> = self.registry_repo.split('/').collect();
        let workflow = self
            .signer_workflow
            .strip_prefix(&format!("{}/.github/workflows/", self.registry_repo));
        if repo.len() != 2
            || repo.iter().any(|v| !name(v))
            || !workflow.is_some_and(|v| name(v) && (v.ends_with(".yml") || v.ends_with(".yaml")))
            || !self
                .registry_ref
                .strip_prefix("refs/heads/")
                .is_some_and(name)
            || !name(&self.reviewer)
            || !name(&self.submitter)
        {
            bail!(
                "Review policy must pin a registry branch, same-repository workflow and reviewer"
            );
        }
        let approved = DateTime::parse_from_rfc3339(&self.approved_at)?.timestamp();
        let expires = DateTime::parse_from_rfc3339(&self.expires_at)?.timestamp();
        if approved > now || expires <= now || expires <= approved || expires - approved > 7 * 86400
        {
            bail!("Review approval is future-dated, expired or longer than seven days");
        }
        // Validate the independent public-key anchor before any secret-key read.
        minisign::PublicKey::from_base64(&self.publishing_public_key)
            .context("Invalid approved publishing public key")?;
        Ok(())
    }

    fn roots(&self) -> Roots {
        Roots {
            spec: 1,
            version: 1,
            publishing_key: self.publishing_public_key.clone(),
            base_urls: Vec::new(),
            mirrors: Vec::new(),
            expires: Some(self.expires_at.clone()),
        }
    }
}

/// All consumed inputs are ordinary bounded files. Parent trees must also be
/// immutable, operator-owned directories; this is not an OS sandbox.
pub(super) fn read(path: &Path, max: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > max {
        bail!("Unsupported or oversized signing input");
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        bail!("Signing input grew beyond its limit");
    }
    Ok(bytes)
}

fn directory(path: &Path) -> Result<PathBuf> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        bail!("Signing tree must be a real directory");
    }
    Ok(path.canonicalize()?)
}

/// Compare the entire received candidate with independently regenerated bytes.
/// This also refuses ignored JSON fields, alternate encodings and extra files.
pub(super) fn same_tree(expected: &Path, received: &Path, depth: usize) -> Result<()> {
    directory(received)?;
    let mut names = fs::read_dir(expected)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut actual = Vec::new();
    // Bound enumeration before collecting author-controlled names.
    for entry in fs::read_dir(received)? {
        if actual.len() >= 16 {
            bail!("Unexpected candidate inventory");
        }
        actual.push(entry?.file_name());
    }
    names.sort();
    actual.sort();
    if names != actual {
        bail!("Unexpected candidate inventory");
    }
    for name in names {
        let expected = expected.join(&name);
        let received = received.join(&name);
        let meta = fs::symlink_metadata(&expected)?;
        if meta.is_dir() {
            if depth == 0 {
                bail!("Unexpected candidate directory depth");
            }
            same_tree(&expected, &received, depth - 1)?;
        } else {
            let bytes = read(&received, meta.len())?;
            if bytes.len() as u64 != meta.len()
                || digest(&bytes) != digest(&read(&expected, meta.len())?)
            {
                bail!("Candidate bytes differ from independently checked preparation");
            }
        }
    }
    Ok(())
}

fn verifier(path: &Path, pin: &str) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("GitHub verifier must have an absolute path");
    }
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 128 * 1024 * 1024 {
        bail!("Invalid GitHub verifier executable");
    }
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 16384];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > 128 * 1024 * 1024 {
            bail!("GitHub verifier grew beyond its limit");
        }
        hasher.update(&buffer[..n]);
    }
    if format!("{:x}", hasher.finalize()) != pin {
        bail!("GitHub verifier executable digest mismatch");
    }
    Ok(path.canonicalize()?)
}

fn verification_command(gh: &Path, policy: &Policy, scratch: &Path) -> Command {
    let mut command = Command::new(gh);
    command
        .args(["attestation", "verify"])
        .arg(scratch.join("snapshot/candidate.json"))
        .arg("--bundle")
        .arg(scratch.join("attestation.jsonl"))
        .args([
            "--repo",
            &policy.registry_repo,
            "--signer-workflow",
            &policy.signer_workflow,
            "--signer-digest",
            &policy.signer_commit,
            "--source-digest",
            &policy.registry_commit,
            "--source-ref",
            &policy.registry_ref,
            "--deny-self-hosted-runners",
            "--hostname",
            "github.com",
            "--cert-oidc-issuer",
            "https://token.actions.githubusercontent.com",
            "--predicate-type",
            "https://slsa.dev/provenance/v1",
            "--digest-alg",
            "sha256",
            "--format",
            "json",
        ])
        .env_clear()
        .current_dir(scratch)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    // No publishing secrets, user GitHub login or inherited verifier overrides.
    // Bundle verification uses GitHub/Sigstore's standard trust roots.
    for key in ["SystemRoot", "WINDIR", "PATH"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    for key in [
        "HOME",
        "USERPROFILE",
        "GH_CONFIG_DIR",
        "TMP",
        "TEMP",
        "TMPDIR",
    ] {
        command.env(key, scratch);
    }
    command.env("GH_PROMPT_DISABLED", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

fn wait_verifier(
    child: &mut Child,
    output: &fs::File,
    deadline: Duration,
    max: u64,
) -> Result<ExitStatus> {
    let start = Instant::now();
    let status = (|| -> Result<_> {
        loop {
            if output.metadata()?.len() > max || start.elapsed() > deadline {
                bail!("GitHub attestation verifier exceeded its output/time budget");
            }
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    match status {
        Ok(status) => Ok(status),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

fn verify_attestation(gh: &Path, policy: &Policy, scratch: &Path) -> Result<()> {
    let mut output = tempfile::tempfile()?;
    let mut command = verification_command(gh, policy, scratch);
    command.stdout(output.try_clone()?);
    let mut child = command
        .spawn()
        .context("Start pinned GitHub attestation verifier")?;
    let status = wait_verifier(&mut child, &output, Duration::from_secs(60), 1024 * 1024)?;
    if !status.success() {
        bail!("GitHub provenance verification refused; signing key was not read");
    }
    output.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    output.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        bail!("GitHub verifier output exceeded limit");
    }
    let results: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
    if results.is_empty()
        || results.iter().any(|v| {
            !v.get("verificationResult")
                .is_some_and(serde_json::Value::is_object)
        })
    {
        bail!("GitHub verifier returned no verified attestation");
    }
    Ok(())
}

#[derive(Clone)]
struct Prior {
    index: Index,
    // Preserve signed forward-compatible fields when extending the catalogue.
    document: serde_json::Value,
}

fn previous(policy: &Policy, path: &Path) -> Result<Prior> {
    let json = read(&path.join("index.json"), 4 * 1024 * 1024)?;
    if digest(&json) != policy.previous_index_sha256 {
        bail!("Previous index digest mismatch");
    }
    let signature = String::from_utf8(read(&path.join("index.json.minisig"), 8192)?)?;
    let (index, status) = grain_core::trust::verify_index(
        &policy.roots(),
        &json,
        &signature,
        Some(policy.previous_index_version),
        Utc::now().timestamp(),
        false,
    )?;
    if index.spec != grain_sdk::DISTRIBUTION_SPEC
        || index.version != policy.previous_index_version
        || status != grain_core::trust::IndexStatus::Fresh
    {
        bail!("Previous index must match the approved version and be fresh");
    }
    Ok(Prior {
        index,
        document: serde_json::from_slice(&json)?,
    })
}

/// This function only receives owned, already verified snapshots. It is not a
/// CLI trust bypass: the public route must pass policy, prior-index and GH gates.
fn write_signed(
    policy: &Policy,
    mut previous: Prior,
    snapshot: &Path,
    key: &Path,
    out: &Path,
) -> Result<()> {
    policy.validate(Utc::now().timestamp())?;
    let candidate: crate::catalogue::Candidate =
        serde_json::from_slice(&read(&snapshot.join("candidate.json"), 65536)?)?;
    let mut entry = candidate.entry;
    entry.trust = grain_sdk::Trust::Verified;
    entry.author = policy.submitter.clone();
    entry.reviewed_at = DateTime::parse_from_rfc3339(&policy.approved_at)?
        .with_timezone(&Utc)
        .format("%Y-%m-%d")
        .to_string();
    entry.reviewed_commit = entry.source_commit.clone();
    entry.updated_at = Utc::now().format("%Y-%m-%d").to_string();
    // A reviewed version is immutable: changed bytes require a new version.
    if let Some(old) = previous
        .index
        .entries
        .iter()
        .find(|v| v.id == entry.id && v.version == entry.version)
    {
        if serde_json::to_vec(old)? != serde_json::to_vec(&entry)? {
            bail!("Published version already exists with different metadata or bytes");
        }
        bail!("Published version already exists; no repeat signing update");
    }
    let next_version = previous
        .index
        .version
        .checked_add(1)
        .context("Index version exhausted")?;
    let document = previous
        .document
        .as_object_mut()
        .context("Previous index envelope")?;
    let entries = document
        .entry("entries")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context("Previous index entries")?;
    entries.push(serde_json::to_value(entry)?);
    document.insert("version".into(), next_version.into());
    document.insert(
        "expires".into(),
        (Utc::now() + Days::days(30)).to_rfc3339().into(),
    );
    let bytes = serde_json::to_vec_pretty(&previous.document)?;
    if bytes.len() > 4 * 1024 * 1024 {
        bail!("Next index exceeds size limit");
    }
    // Only now can the key be opened, after all public-route admission gates.
    let key_text = String::from_utf8(read(key, 8192)?)?;
    let signature = crate::sign_text(&key_text, &bytes)?;
    let (_, status) = grain_core::trust::verify_index(
        &policy.roots(),
        &bytes,
        &signature,
        Some(next_version),
        Utc::now().timestamp(),
        false,
    )?;
    if status != grain_core::trust::IndexStatus::Fresh {
        bail!("Signed index failed client verification");
    }
    fs::create_dir(out)?;
    let mut owned = OwnedOutput(out.to_owned(), false);
    for folder in ["blob", "media"] {
        fs::create_dir(out.join(folder))?;
        for file in fs::read_dir(snapshot.join(folder))? {
            let file = file?;
            fs::copy(file.path(), out.join(folder).join(file.file_name()))?;
        }
    }
    fs::write(out.join("index.json"), bytes)?;
    fs::write(out.join("index.json.minisig"), signature)?;
    owned.1 = true;
    Ok(())
}

pub(super) fn sign(input: Inputs<'_>) -> Result<()> {
    if !hex(input.policy_pin, 64) {
        bail!("Independent review policy digest required");
    }
    let raw = read(input.policy, 65536)?;
    if digest(&raw) != input.policy_pin {
        bail!("Protected review policy digest mismatch");
    }
    let policy: Policy = serde_json::from_slice(&raw)?;
    policy.validate(Utc::now().timestamp())?;
    let submission = directory(input.submission)?;
    let prepared = directory(input.prepared)?;
    let candidate = directory(input.candidate)?;
    let prior = directory(input.previous)?;
    let policy_parent = input
        .policy
        .canonicalize()?
        .parent()
        .context("Policy parent")?
        .to_owned();
    let out = new_output(
        input.out,
        &[&submission, &prepared, &candidate, &prior, &policy_parent],
    )?;
    let checked = crate::receive::verify(
        &submission,
        &prepared,
        &policy.receipt_sha256,
        &policy.producer_sha256,
    )?;
    if digest(&serde_json::to_vec(&checked.receipt.submission)?) != policy.submission_sha256 {
        bail!("Prepared submission differs from approved source review");
    }
    let scratch = tempfile::tempdir()?;
    let snapshot = scratch.path().join("snapshot");
    crate::catalogue::stage(
        checked,
        &submission,
        &prepared,
        &policy.receipt_sha256,
        &policy.producer_sha256,
        &snapshot,
    )?;
    let raw_candidate = read(&snapshot.join("candidate.json"), 65536)?;
    if digest(&raw_candidate) != policy.candidate_sha256 {
        bail!("Candidate digest differs from approved review");
    }
    same_tree(&snapshot, &candidate, 1)?;
    let index = previous(&policy, &prior)?;
    let gh = verifier(input.gh, &policy.verifier_sha256)?;
    fs::write(
        scratch.path().join("attestation.jsonl"),
        read(input.attestation, 16 * 1024 * 1024)?,
    )?;
    verify_attestation(&gh, &policy, scratch.path())?;
    write_signed(&policy, index, &snapshot, input.key, &out)?;
    println!(
        "signed catalogue update at {}; not uploaded or a complete hosted registry",
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_output_and_deadline_failures_reap_the_owned_child() {
        use std::io::Write;
        for oversized in [false, true] {
            let mut output = tempfile::tempfile().unwrap();
            if oversized {
                output.write_all(b"over limit").unwrap();
            }
            // A real short-lived test-binary child, not simulated attestation evidence.
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .arg("--list")
                .stdout(output.try_clone().unwrap())
                .stderr(Stdio::null())
                .stdin(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let mut child = command.spawn().unwrap();
            let deadline = if oversized {
                Duration::from_secs(60)
            } else {
                Duration::ZERO
            };
            assert!(wait_verifier(&mut child, &output, deadline, 1).is_err());
            assert!(
                child.try_wait().unwrap().is_some(),
                "owned child was not reaped"
            );
        }
    }

    fn policy() -> Policy {
        Policy {
            schema: 1,
            candidate_sha256: "a".repeat(64),
            submission_sha256: "b".repeat(64),
            receipt_sha256: "c".repeat(64),
            producer_sha256: "d".repeat(64),
            verifier_sha256: "e".repeat(64),
            registry_repo: "example/registry".into(),
            registry_commit: "a".repeat(40),
            registry_ref: "refs/heads/main".into(),
            signer_workflow: "example/registry/.github/workflows/trusted-build.yml".into(),
            signer_commit: "b".repeat(40),
            reviewer: "reviewer".into(),
            submitter: "example".into(),
            approved_at: (Utc::now() - Days::minutes(1)).to_rfc3339(),
            expires_at: (Utc::now() + Days::days(1)).to_rfc3339(),
            publishing_public_key: grain_core::trust::ROOT_PUBKEY_A.into(),
            previous_index_sha256: "f".repeat(64),
            previous_index_version: 7,
        }
    }

    #[test]
    fn strict_policy_refuses_bad_pins_identities_and_approval_times() {
        let now = Utc::now().timestamp();
        policy().validate(now).unwrap();
        for field in [
            "schema",
            "candidate_sha256",
            "submission_sha256",
            "receipt_sha256",
            "producer_sha256",
            "verifier_sha256",
            "registry_repo",
            "registry_commit",
            "registry_ref",
            "signer_workflow",
            "signer_commit",
            "reviewer",
            "submitter",
            "publishing_public_key",
            "previous_index_sha256",
        ] {
            let mut value = serde_json::to_value(policy()).unwrap();
            value[field] = if field == "schema" {
                serde_json::json!(2)
            } else {
                serde_json::json!("invalid/input")
            };
            assert!(
                serde_json::from_value::<Policy>(value)
                    .unwrap()
                    .validate(now)
                    .is_err(),
                "{field}"
            );
        }
        for (approved, expires) in [
            (now + 1, now + 60),
            (now - 60, now),
            (now - 60, now + 8 * 86400),
            (now - 60, now - 61),
        ] {
            let mut p = policy();
            p.approved_at = DateTime::from_timestamp(approved, 0).unwrap().to_rfc3339();
            p.expires_at = DateTime::from_timestamp(expires, 0).unwrap().to_rfc3339();
            assert!(p.validate(now).is_err());
        }
        let mut value = serde_json::to_value(policy()).unwrap();
        value["skip_attestation"] = true.into();
        assert!(serde_json::from_value::<Policy>(value).is_err());
    }

    #[test]
    fn verifier_args_pin_certificate_identity_and_strip_inherited_secrets() {
        let command =
            verification_command(Path::new("/operator/gh"), &policy(), Path::new("/owned"));
        let args: Vec<_> = command
            .get_args()
            .map(|v| v.to_string_lossy().into_owned())
            .collect();
        for flag in [
            "--repo",
            "--signer-workflow",
            "--signer-digest",
            "--source-digest",
            "--source-ref",
            "--deny-self-hosted-runners",
            "--cert-oidc-issuer",
            "--predicate-type",
        ] {
            assert!(args.iter().any(|v| v == flag), "{flag}");
        }
        assert!(!args
            .iter()
            .any(|v| v == "--custom-trusted-root" || v == "--no-public-good"));
        let env: Vec<_> = command
            .get_envs()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(!env
            .iter()
            .any(|v| v == "GH_TOKEN" || v == "GITHUB_TOKEN" || v == "GRAIN_PUBLISHING_KEY"));
        assert!(env.iter().any(|v| v == "GH_CONFIG_DIR"));
    }

    #[test]
    fn exact_inventory_refuses_changes_extra_files_and_nested_trees() {
        let root = tempfile::tempdir().unwrap();
        let expected = root.path().join("expected");
        let received = root.path().join("received");
        for dir in [&expected, &received] {
            fs::create_dir(dir).unwrap();
            fs::write(dir.join("candidate.json"), b"draft").unwrap();
        }
        same_tree(&expected, &received, 1).unwrap();
        fs::write(received.join("candidate.json"), b"drift").unwrap();
        assert!(same_tree(&expected, &received, 1).is_err());
        fs::write(received.join("candidate.json"), b"draft").unwrap();
        fs::write(received.join("hidden.js"), b"author code").unwrap();
        assert!(same_tree(&expected, &received, 1).is_err());
        fs::remove_file(received.join("hidden.js")).unwrap();
        for dir in [&expected, &received] {
            fs::create_dir(dir.join("nested")).unwrap();
        }
        assert!(same_tree(&expected, &received, 0).is_err());
    }

    #[test]
    fn executable_pin_and_bounded_file_checks_refuse_before_starting_children() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("gh");
        fs::write(&path, b"not a trusted verifier").unwrap();
        assert!(verifier(Path::new("relative-gh"), &digest(b"not a trusted verifier")).is_err());
        assert!(verifier(&path, &"a".repeat(64)).is_err());
        verifier(&path, &digest(b"not a trusted verifier")).unwrap(); // hashing only, never executes it
        assert!(read(&path, 4).is_err());
        assert!(read(root.path(), 65536).is_err());
    }

    #[test]
    fn real_minisign_native_and_mcp_updates_verify_and_preserve_previous_index() {
        for mcp in [false, true] {
            let fixture = crate::prepare::tests::fixture(mcp, true);
            // Reuse the real preparation/author fixture; no alternate host or crypto implementation.
            crate::prepare::prepare(
                fixture.submission.clone(),
                fixture.submission.parent().unwrap().join("tools"),
                fixture.out.clone(),
            )
            .unwrap();
            let pins = crate::prepare::tests::receipt_pins(&fixture);
            let root = fixture.out.parent().unwrap();
            let snapshot = root.join("candidate");
            crate::catalogue::prepare(
                &fixture.submission,
                &fixture.out,
                &pins.0,
                &pins.1,
                &snapshot,
            )
            .unwrap();
            let keydir = root.join("keys");
            crate::keygen(keydir.clone(), "test".into()).unwrap();
            let mut p = policy();
            p.publishing_public_key = minisign::PublicKeyBox::from_string(
                &fs::read_to_string(keydir.join("test.pub")).unwrap(),
            )
            .unwrap()
            .into_public_key()
            .unwrap()
            .to_base64();
            let prior = Index {
                spec: 1,
                version: 7,
                expires: (Utc::now() + Days::days(2)).to_rfc3339(),
                entries: Vec::new(),
            };
            let mut prior_document = serde_json::to_value(&prior).unwrap();
            let mut existing = serde_json::from_slice::<crate::catalogue::Candidate>(
                &fs::read(snapshot.join("candidate.json")).unwrap(),
            )
            .unwrap()
            .entry;
            existing.id = "com.example.existing".into();
            let mut existing_document = serde_json::to_value(existing).unwrap();
            existing_document["future_metadata"] = serde_json::json!({"retained": true});
            prior_document["entries"] = serde_json::json!([existing_document.clone()]);
            prior_document["future_envelope"] = "retained".into();
            let old = serde_json::to_vec_pretty(&prior_document).unwrap();
            let previous_dir = root.join("previous");
            fs::create_dir(&previous_dir).unwrap();
            fs::write(previous_dir.join("index.json"), &old).unwrap();
            fs::write(
                previous_dir.join("index.json.minisig"),
                crate::sign_bytes(&keydir.join("test.key"), &old).unwrap(),
            )
            .unwrap();
            p.previous_index_sha256 = digest(&old);
            let previous = previous(&p, &previous_dir).unwrap();
            let output = root.join("signed");
            write_signed(
                &p,
                previous.clone(),
                &snapshot,
                &keydir.join("test.key"),
                &output,
            )
            .unwrap();
            let bytes = fs::read(output.join("index.json")).unwrap();
            let sig = fs::read_to_string(output.join("index.json.minisig")).unwrap();
            let (index, status) = grain_core::trust::verify_index(
                &p.roots(),
                &bytes,
                &sig,
                Some(8),
                Utc::now().timestamp(),
                false,
            )
            .unwrap();
            assert_eq!(status, grain_core::trust::IndexStatus::Fresh);
            assert_eq!(index.version, 8);
            assert_eq!(index.entries.len(), 2);
            assert_eq!(index.entries[1].trust, grain_sdk::Trust::Verified);
            assert_eq!(index.entries[1].author, "example");
            assert_eq!(index.entries[1].reviewed_commit, fixture.value.commit);
            let new_document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(new_document["entries"][0], existing_document);
            assert_eq!(new_document["future_envelope"], "retained");
            assert_eq!(fs::read(previous_dir.join("index.json")).unwrap(), old);
            assert!(output.join("blob").read_dir().unwrap().next().is_some());
            assert_eq!(output.join("media").read_dir().unwrap().count(), 2);
            assert!(!output.join("candidate.json").exists());
            let refused = root.join("refused");
            assert!(write_signed(
                &p,
                Prior {
                    index,
                    document: new_document
                },
                &snapshot,
                &root.join("missing.key"),
                &refused
            )
            .unwrap_err()
            .to_string()
            .contains("already exists"));
            assert!(!refused.exists());
            let mut wrong = policy();
            wrong.publishing_public_key = grain_core::trust::ROOT_PUBKEY_A.into();
            assert!(write_signed(
                &wrong,
                previous,
                &snapshot,
                &keydir.join("test.key"),
                &refused
            )
            .is_err());
            assert!(!refused.exists());
            p.previous_index_version = 8;
            assert!(super::previous(&p, &previous_dir).is_err());
            p.previous_index_version = 7;
            fs::write(
                previous_dir.join("index.json.minisig"),
                b"invalid signature",
            )
            .unwrap();
            assert!(super::previous(&p, &previous_dir).is_err());
        }
    }
}
