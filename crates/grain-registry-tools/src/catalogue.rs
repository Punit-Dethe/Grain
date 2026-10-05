//! Unsigned catalogue staging. Source review and signing authority stay external.
use std::{fs, io::Read, path::Path};

use anyhow::{bail, Result};
use grain_sdk::distribution::{ArtifactKind, IndexEntry, ListingDocument};
use serde::Serialize;

use crate::prepare::{digest, new_output, OwnedOutput};

#[derive(Serialize)]
struct Candidate<'a> {
    schema: u8,
    evidence_class: &'static str,
    receipt_sha256: &'a str,
    producer_sha256: &'a str,
    entry: IndexEntry,
}

pub(super) fn prepare(
    submission: &Path,
    prepared: &Path,
    receipt_pin: &str,
    producer_pin: &str,
    out: &Path,
) -> Result<()> {
    let checked = crate::receive::verify(submission, prepared, receipt_pin, producer_pin)?;
    let source = &checked.receipt.submission;
    let (name, capabilities) = match source.artifact_kind {
        ArtifactKind::Native => {
            let pack: grain_sdk::GrainPack = serde_json::from_slice(&checked.artifact)?;
            (pack.manifest.name, pack.manifest.permissions)
        }
        ArtifactKind::McpDescriptor => {
            let descriptor: grain_sdk::mcp::McpDescriptor =
                serde_json::from_slice(&checked.artifact)?;
            (descriptor.name, Vec::new())
        }
    };
    let entry = IndexEntry {
        artifact_kind: source.artifact_kind,
        id: source.id.clone(),
        name,
        version: source.version.clone(),
        tier: grain_sdk::Tier::Scripted,
        // A draft has no trust or human-review date. Only a later independently
        // authorized signer may set review/trust/index envelope metadata.
        trust: grain_sdk::Trust::Dev,
        capabilities,
        description: source.summary.clone(),
        sha256: checked.receipt.artifact_sha256.clone(),
        size: checked.artifact.len() as u64,
        min_grain_api: source.grain_api.trim().trim_start_matches('^').into(),
        repo: source.source_repo.clone(),
        source_commit: source.commit.clone(),
        author: String::new(),
        reviewed_at: String::new(),
        reviewed_commit: String::new(),
        updated_at: String::new(),
        stars: 0,
        installs: 0,
        readme: String::new(),
        listing: Some(ListingDocument {
            sha256: source.description_sha256.clone(),
            size: source.description_size,
        }),
        media: source
            .media
            .iter()
            .map(|asset| grain_sdk::MediaRef {
                sha256: asset.sha256.clone(),
                size: asset.size,
                kind: asset.kind.clone(),
            })
            .collect(),
        categories: source.categories.clone(),
        extends: Vec::new(),
    };
    entry.validate_listing().map_err(anyhow::Error::msg)?;
    let submission = submission.canonicalize()?;
    let prepared = prepared.canonicalize()?;
    let out = new_output(out, &[&submission, &prepared])?;
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    fs::create_dir(out.join("blob"))?;
    fs::create_dir(out.join("media"))?;
    let suffix = match source.artifact_kind {
        ArtifactKind::Native => "grainpack",
        ArtifactKind::McpDescriptor => "mcp.json",
    };
    fs::write(
        out.join("blob")
            .join(format!("{}.{}", entry.sha256, suffix)),
        &checked.artifact,
    )?;
    fs::write(
        out.join("media")
            .join(format!("{}.md", source.description_sha256)),
        checked.description.as_bytes(),
    )?;
    for asset in &source.media {
        let path = prepared.join("media").join(&asset.name);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            bail!("Media changed into an unsupported file during staging");
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(asset.size + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != asset.size || digest(&bytes) != asset.sha256 {
            bail!("Media changed during catalogue staging");
        }
        fs::write(
            out.join("media")
                .join(format!("{}.{}", asset.sha256, asset.kind)),
            bytes,
        )?;
    }
    // Completion marker last. No index.json or signature is produced.
    let candidate = Candidate {
        schema: 1,
        evidence_class: "unsigned-catalogue-candidate/not-reviewed",
        receipt_sha256: receipt_pin,
        producer_sha256: producer_pin,
        entry,
    };
    fs::write(
        out.join("candidate.json"),
        serde_json::to_vec_pretty(&candidate)?,
    )?;
    owned.1 = true;
    println!(
        "staged unsigned catalogue candidate at {}; review/CI/signing authority still required",
        out.display()
    );
    Ok(())
}
