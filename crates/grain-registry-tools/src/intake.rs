//! CI handoff to human review, reusing the signer’s validators and GH verifier.
use std::{fs, path::PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::{
    prepare::{digest, new_output, OwnedOutput},
    review::{read, Policy},
};

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long)]
    pub submission: PathBuf,
    /// Downloaded attested-source-<id> directory; never execute anything from it.
    #[arg(long)]
    pub evidence: PathBuf,
    /// Operator-owned schema-3 policy template; computed/approval fields must be empty.
    #[arg(long)]
    pub template: PathBuf,
    #[arg(long)]
    pub authority: PathBuf,
    #[arg(long)]
    pub previous: PathBuf,
    /// Independently selected Grain tool source commit, not copied from received evidence.
    #[arg(long)]
    pub tooling_commit: String,
    #[arg(long)]
    pub gh: PathBuf,
    #[arg(long)]
    pub out: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Producer {
    schema: u8,
    evidence_class: String,
    grain_commit: String,
    producer_sha256: String,
    receipt_sha256: String,
    submission_sha256: String,
    candidate_sha256: String,
}

pub(crate) fn prepare(args: &Args) -> Result<()> {
    prepare_with(args, |gh, policy, scratch| {
        crate::review::verify_attestation(gh, policy, scratch)
            .context("Candidate attestation refused")?;
        crate::review::verify_producer_attestation(gh, policy, scratch)
            .context("Producer attestation refused")
    })
}

fn prepare_with(
    args: &Args,
    provenance: impl FnOnce(&std::path::Path, &Policy, &std::path::Path) -> Result<()>,
) -> Result<()> {
    let mut policy: Policy = serde_json::from_slice(&read(&args.template, 65536)?)?;
    if policy.maintainer_approval.is_some()
        || !policy.approved_at.is_empty()
        || !policy.expires_at.is_empty()
        || [
            &policy.candidate_sha256,
            &policy.submission_sha256,
            &policy.receipt_sha256,
            &policy.authority_sha256,
            &policy.previous_index_sha256,
        ]
        .iter()
        .any(|v| !v.is_empty())
        || policy.previous_index_version != 0
    {
        bail!("Use an unapproved template with empty computed/approval fields; approval is a separate operator action");
    }
    if !crate::review::hex(&args.tooling_commit, 40) {
        bail!("Independent full tooling commit required");
    }
    let raw = read(&args.evidence.join("producer.json"), 16384)?;
    let producer: Producer = serde_json::from_slice(&raw)?;
    if producer.schema != 1
        || producer.evidence_class != "pinned-remote-source-build/not-human-approved"
        || producer.grain_commit != args.tooling_commit
        || producer.producer_sha256 != policy.producer_sha256
    {
        bail!("CI producer differs from the independently selected tooling identity");
    }
    let prepared = args.evidence.join("prepared");
    let checked = crate::receive::verify(
        &args.submission,
        &prepared,
        &producer.receipt_sha256,
        &policy.producer_sha256,
    )?;
    let source = checked.receipt.submission.clone();
    policy.submission_sha256 = digest(&serde_json::to_vec(&source)?);
    policy.receipt_sha256 = producer.receipt_sha256;
    policy.candidate_sha256 = producer.candidate_sha256;
    policy.authority_sha256 = digest(&read(&args.authority, 128 * 1024)?);
    let previous = read(&args.previous.join("index.json"), 4 * 1024 * 1024)?;
    policy.previous_index_sha256 = digest(&previous);
    policy.previous_index_version =
        serde_json::from_slice::<grain_sdk::distribution::Index>(&previous)?.version;
    policy.validate_inputs()?;
    if producer.submission_sha256 != policy.submission_sha256 {
        bail!("CI producer does not bind the reviewed submission");
    }
    crate::authority::outside(
        &args.authority,
        &[&args.submission, &args.evidence, &args.previous],
    )?;
    crate::authority::outside(
        &args.template,
        &[&args.submission, &args.evidence, &args.previous],
    )?;
    crate::authority::check_candidate(&args.authority, &policy, &source)?;
    crate::review::verify_previous(&policy, &args.previous)?;
    let roots = [
        &args.submission,
        &args.evidence,
        &args.previous,
        &args.template,
        &args.authority,
    ]
    .map(|path| path.canonicalize())
    .into_iter()
    .collect::<std::io::Result<Vec<_>>>()?;
    let mut forbidden = vec![roots[0].as_path(), roots[1].as_path(), roots[2].as_path()];
    forbidden.extend(roots[3..].iter().filter_map(|p| p.parent()));
    let out = new_output(&args.out, &forbidden)?;
    let gh = crate::review::verifier(&args.gh, &policy.verifier_sha256)?;
    let scratch = tempfile::tempdir()?;
    let snapshot = scratch.path().join("snapshot");
    crate::catalogue::stage(
        checked,
        &args.submission,
        &prepared,
        &policy.receipt_sha256,
        &policy.producer_sha256,
        &snapshot,
    )?;
    crate::review::same_tree(&snapshot, &args.evidence.join("candidate"), 1)?;
    if digest(&read(&snapshot.join("candidate.json"), 65536)?) != policy.candidate_sha256 {
        bail!("CI candidate differs from its producer identity");
    }
    fs::write(scratch.path().join("producer.json"), raw)?;
    fs::write(
        scratch.path().join("attestation.jsonl"),
        read(&args.evidence.join("attestation.jsonl"), 16 * 1024 * 1024)?,
    )?;
    provenance(&gh, &policy, scratch.path())
        .context("CI provenance refused; no review policy emitted")?;
    // Authority can change while the network verifier runs. Recheck before output.
    crate::authority::check_candidate(&args.authority, &policy, &source)?;
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    fs::write(out.join("policy.json"), serde_json::to_vec_pretty(&policy)?)?;
    fs::write(
        out.join("review.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema": 1, "evidence_class": "verified-ci-candidate-not-human-approved",
            "source": source, "tooling_commit": args.tooling_commit,
            "candidate_sha256": policy.candidate_sha256,
            "previous_index_sha256": policy.previous_index_sha256,
            "previous_index_version": policy.previous_index_version,
            "approval": "pending; inspect source/listing/permissions, then approve the exact policy independently"
        }))?,
    )?;
    owned.1 = true;
    println!("CI payload and candidate/producer provenance verified; unapproved review draft at {}. No key read, signing or publication", out.display());
    Ok(())
}

#[cfg(test)]
#[path = "intake_tests.rs"]
mod tests;
