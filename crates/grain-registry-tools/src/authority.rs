//! Operator-owned registry ownership. No author manifest can grant publication.
use std::{collections::BTreeSet, path::Path};

use anyhow::{bail, Context, Result};
use grain_sdk::{distribution::ArtifactKind, submission::SourceSubmission};
use serde::Deserialize;

use crate::{
    prepare::digest,
    review::{name, Policy},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    schema: u8,
    registry_repo: String,
    registry_ref: String,
    publishing_public_key: String,
    reviewers: Vec<String>,
    extensions: Vec<Owner>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    id: String,
    source_repo: String,
    artifact_kind: ArtifactKind,
    maintainers: Vec<String>,
    #[serde(default)]
    allow_maintainer_approval: bool,
}

fn names(values: &[String]) -> bool {
    !values.is_empty()
        && values.len() <= 32
        && values.iter().all(|value| name(value))
        && values
            .iter()
            .map(|v| v.to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
            .len()
            == values.len()
}

fn repository(value: &str) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    parts.len() == 2 && parts.iter().all(|part| name(part)) && !value.ends_with(".git")
}

fn contains(values: &[String], value: &str) -> bool {
    values.iter().any(|v| v.eq_ignore_ascii_case(value))
}

impl Authority {
    fn check(&self, policy: &Policy, source: &SourceSubmission) -> Result<()> {
        if self.schema != 1
            || !repository(&self.registry_repo)
            || !names(&self.reviewers)
            || self.extensions.is_empty()
            || self.extensions.len() > 1024
        {
            bail!("Invalid registry ownership configuration");
        }
        let mut ids = BTreeSet::new();
        for entry in &self.extensions {
            grain_sdk::validate_extension_id(&entry.id).map_err(anyhow::Error::msg)?;
            if !ids.insert(&entry.id)
                || !names(&entry.maintainers)
                || !entry
                    .source_repo
                    .strip_prefix("https://github.com/")
                    .is_some_and(repository)
            {
                bail!("Duplicate ID or invalid extension ownership entry");
            }
        }
        if self.registry_repo != policy.registry_repo
            || self.registry_ref != policy.registry_ref
            || self.publishing_public_key != policy.publishing_public_key
            || !contains(&self.reviewers, &policy.reviewer)
        {
            bail!("Reviewer, registry or publishing identity is not authorized");
        }
        let owner = self
            .extensions
            .iter()
            .find(|e| e.id == source.id)
            .context("Extension ID has no approved owner; enroll it before signing")?;
        if !owner.source_repo.eq_ignore_ascii_case(&source.source_repo)
            || owner.artifact_kind != source.artifact_kind
            || !contains(&owner.maintainers, &policy.submitter)
        {
            bail!("Extension source, kind or submitting maintainer is not authorized");
        }
        if policy.maintainer_approval.is_some()
            && (!owner.allow_maintainer_approval || !contains(&owner.maintainers, &policy.reviewer))
        {
            bail!("Explicit maintainer approval is not permitted for this extension/reviewer");
        }
        Ok(())
    }
}

pub(super) fn outside(path: &Path, forbidden: &[&Path]) -> Result<()> {
    let path = path.canonicalize()?;
    for root in forbidden {
        if path.starts_with(root.canonicalize()?) {
            bail!("Ownership file must be outside author/build inputs");
        }
    }
    Ok(())
}

pub(super) fn authorize(path: &Path, policy: &Policy, source: &SourceSubmission) -> Result<()> {
    policy.validate(chrono::Utc::now().timestamp())?;
    source.validate().map_err(anyhow::Error::msg)?;
    let raw = crate::review::read(path, 128 * 1024)?;
    if digest(&raw) != policy.authority_sha256 {
        bail!("Ownership file changed since approval; review and approve the current policy");
    }
    let authority: Authority = serde_json::from_slice(&raw)?;
    authority.check(policy, source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn fixture() -> (crate::review::Policy, SourceSubmission, Value) {
        let p = crate::review::tests::policy();
        let source: SourceSubmission = serde_json::from_value(json!({
            "schema":1,"artifact_kind":"mcp-descriptor","id":"com.example.tools","version":"0.1.0",
            "grain_api":"^1.0","source_repo":"https://github.com/example/tools","tag":"v0.1.0",
            "commit":"a".repeat(40),"summary":"Tools","categories":["tools"],"license":"MIT",
            "contact":"maintainer","description_sha256":"b".repeat(64),"description_size":12
        }))
        .unwrap();
        let config = json!({"schema":1,"registry_repo":p.registry_repo,"registry_ref":p.registry_ref,
            "publishing_public_key":p.publishing_public_key,"reviewers":["reviewer","example"],
            "extensions":[{"id":source.id,"source_repo":source.source_repo,"artifact_kind":"mcp-descriptor",
                "maintainers":["example"],"allow_maintainer_approval":false}]});
        (p, source, config)
    }

    #[test]
    fn ownership_authorizes_external_review_and_explicit_first_party_releases() {
        let (mut p, mut source, mut config) = fixture();
        for kind in [ArtifactKind::Native, ArtifactKind::McpDescriptor] {
            source.artifact_kind = kind;
            config["extensions"][0]["artifact_kind"] = serde_json::to_value(kind).unwrap();
            let authority: Authority = serde_json::from_value(config.clone()).unwrap();
            authority.check(&p, &source).unwrap();
        }
        p.reviewer = "example".into();
        p.review_id = 0;
        p.maintainer_approval = Some("Operator approved exact candidate".into());
        assert!(serde_json::from_value::<Authority>(config.clone())
            .unwrap()
            .check(&p, &source)
            .is_err());
        config["extensions"][0]["allow_maintainer_approval"] = json!(true);
        serde_json::from_value::<Authority>(config.clone())
            .unwrap()
            .check(&p, &source)
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("owners.json");
        let bytes = serde_json::to_vec(&config).unwrap();
        fs_write(&path, &bytes);
        p.authority_sha256 = digest(&bytes);
        authorize(&path, &p, &source).unwrap();
        // Removing an owner/reviewer or changing any protected policy invalidates approval.
        config["reviewers"] = json!(["reviewer"]);
        fs_write(&path, &serde_json::to_vec(&config).unwrap());
        assert!(authorize(&path, &p, &source).is_err());
        p.authority_sha256 = digest(&serde_json::to_vec(&config).unwrap());
        assert!(authorize(&path, &p, &source).is_err());
    }

    #[test]
    fn takeover_wrong_kind_duplicates_and_unlisted_reviewers_are_refused() {
        let (p, source, config) = fixture();
        for (pointer, value) in [
            ("/registry_repo", json!("attacker/registry")),
            ("/registry_ref", json!("refs/heads/other")),
            ("/publishing_public_key", json!("other")),
            ("/reviewers", json!(["attacker"])),
            ("/reviewers", json!(["reviewer", "REVIEWER"])),
            ("/extensions/0/id", json!("com.other.tools")),
            (
                "/extensions/0/source_repo",
                json!("https://github.com/attacker/tools"),
            ),
            ("/extensions/0/artifact_kind", json!("native")),
            ("/extensions/0/maintainers", json!(["attacker"])),
        ] {
            let mut bad = config.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(
                serde_json::from_value::<Authority>(bad)
                    .unwrap()
                    .check(&p, &source)
                    .is_err(),
                "{pointer}"
            );
        }
        let mut duplicate = config.clone();
        duplicate["extensions"]
            .as_array_mut()
            .unwrap()
            .push(config["extensions"][0].clone());
        assert!(serde_json::from_value::<Authority>(duplicate)
            .unwrap()
            .check(&p, &source)
            .is_err());
    }

    fn fs_write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn ownership_inputs_are_bounded_strict_and_outside_author_files() {
        let (mut p, source, config) = fixture();
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("owners.json");
        let mut unknown = config.clone();
        unknown["ignore_source_review"] = json!(true);
        assert!(serde_json::from_value::<Authority>(unknown).is_err());
        fs_write(&path, &vec![b' '; 128 * 1024 + 1]);
        p.authority_sha256 = digest(&std::fs::read(&path).unwrap());
        assert!(authorize(&path, &p, &source).is_err());
        assert!(outside(&path, &[root.path()]).is_err());
        let other = tempfile::tempdir().unwrap();
        outside(&path, &[other.path()]).unwrap();
        let mut removed = config.clone();
        removed["extensions"] = json!([]);
        assert!(serde_json::from_value::<Authority>(removed)
            .unwrap()
            .check(&p, &source)
            .is_err());
    }
}
