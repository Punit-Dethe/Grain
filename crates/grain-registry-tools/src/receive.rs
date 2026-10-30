//! Receiving boundary for untrusted build output. Independent CI/review pins
//! are caller policy, never values promoted from the bundle into authority.
use std::{collections::HashSet, fs, io::Read, path::Path};

use anyhow::{bail, Context, Result};
use grain_sdk::distribution::ArtifactKind;

use crate::prepare::{digest, Receipt};

const RECEIPT_MAX: u64 = 64 * 1024;

pub(super) struct Verified {
    pub receipt: Receipt,
    // Keep the exact bytes that were checked, rather than reopening a mutable
    // pathname in a later publisher. CLI verification simply drops them.
    pub artifact: Vec<u8>,
    pub description: String,
}

fn hash_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn root(path: &Path) -> Result<std::path::PathBuf> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("Verification roots must be real directories");
    }
    path.canonicalize().context("resolve verification root")
}

fn read(root: &Path, name: &str, max: u64) -> Result<Vec<u8>> {
    let path = root.join(name);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("Prepared inputs must be regular files, not links");
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        bail!("Prepared input exceeds its byte budget");
    }
    Ok(bytes)
}

pub(super) fn verify(
    submission: &Path,
    prepared: &Path,
    receipt_pin: &str,
    producer_pin: &str,
) -> Result<Verified> {
    if !hash_valid(receipt_pin) || !hash_valid(producer_pin) {
        bail!("Independent receipt/producer pins must be lowercase SHA256");
    }
    let submission_root = root(submission)?;
    let prepared_root = root(prepared)?;
    let id = submission_root
        .file_name()
        .and_then(|v| v.to_str())
        .context("invalid submission folder")?;
    let expected = grain_extension_checks::read_submission(&submission_root, id)
        .map_err(anyhow::Error::msg)?;
    let raw = read(&prepared_root, "receipt.json", RECEIPT_MAX)?;
    if digest(&raw) != receipt_pin {
        bail!("Receipt differs from independent digest");
    }
    let receipt: Receipt = serde_json::from_slice(&raw).context("invalid preparation receipt")?;
    receipt.submission.validate().map_err(anyhow::Error::msg)?;
    let expected_bytes = serde_json::to_vec(&expected)?;
    if receipt.schema != 1
        || receipt.evidence_class != "local-preparation/unsigned-not-reviewed"
        || receipt.producer_version != env!("CARGO_PKG_VERSION")
        || receipt.producer_sha256 != producer_pin
        || receipt.submission_sha256 != digest(&expected_bytes)
        || serde_json::to_vec(&receipt.submission)? != expected_bytes
    {
        bail!("Receipt schema/producer/submission does not match independent policy");
    }
    let (name, budget) = match expected.artifact_kind {
        ArtifactKind::Native => ("artifact.grainpack", grain_sdk::PACK_MAX_BYTES),
        ArtifactKind::McpDescriptor => (
            "artifact.mcp.json",
            grain_sdk::mcp::MCP_DESCRIPTOR_MAX_BYTES as u64,
        ),
    };
    if receipt.artifact != name
        || !hash_valid(&receipt.artifact_sha256)
        || receipt.artifact_size == 0
        || receipt.artifact_size > budget
    {
        bail!("Unsupported artifact name/size/digest");
    }
    let allowed = HashSet::from(["receipt.json", "DESCRIPTION.md", name, "media"]);
    for entry in fs::read_dir(&prepared_root)? {
        let entry = entry?;
        if !allowed.contains(entry.file_name().to_str().unwrap_or(""))
            || entry.file_type()?.is_symlink()
        {
            bail!("Unexpected or linked prepared input");
        }
    }
    let artifact = read(&prepared_root, name, budget)?;
    if artifact.len() as u64 != receipt.artifact_size
        || digest(&artifact) != receipt.artifact_sha256
    {
        bail!("Artifact differs from receipt size/digest");
    }
    let (id, version, api, canonical) = match expected.artifact_kind {
        ArtifactKind::Native => {
            let pack: grain_sdk::GrainPack =
                serde_json::from_slice(&artifact).context("invalid native artifact")?;
            pack.validate().map_err(anyhow::Error::msg)?;
            let canonical = serde_json::to_vec(&pack)?;
            (
                pack.manifest.id,
                pack.manifest.version,
                pack.manifest.grain_api,
                canonical,
            )
        }
        ArtifactKind::McpDescriptor => {
            let descriptor = grain_core::mcp::ValidatedDescriptor::parse(&artifact)
                .map_err(|e| anyhow::anyhow!("invalid MCP artifact: {e}"))?
                .descriptor()
                .clone();
            let canonical = serde_json::to_vec(&descriptor)?;
            (
                descriptor.id,
                descriptor.version,
                descriptor.grain_api,
                canonical,
            )
        }
    };
    if id != expected.id
        || version != expected.version
        || api != expected.grain_api
        || canonical != artifact
    {
        bail!("Artifact identity/API/canonical bytes differ from reviewed submission");
    }
    let listing = grain_extension_checks::listing::read_listing(&prepared_root)
        .map_err(anyhow::Error::msg)?;
    if listing.sha256 != expected.description_sha256
        || listing.description.len() as u64 != expected.description_size
        || listing.media != expected.media
    {
        bail!("Prepared DESCRIPTION/media differs from reviewed submission");
    }
    Ok(Verified {
        receipt,
        artifact,
        description: listing.description,
    })
}
