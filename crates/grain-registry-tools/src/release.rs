//! Thin operator workflow over the existing review, store and hosting boundaries.
//! No new verifier, approval route, Git writer or automatic retry lives here.
use super::*;

pub(crate) fn stage(
    reviewed: &crate::ReviewedCandidateArgs,
    store: &Path,
    pin: &str,
    out: &Path,
    unlock: &crate::Unlock,
) -> Result<()> {
    let parents = [&reviewed.policy, &reviewed.authority, &reviewed.key].map(|path| {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    });
    let mut inputs = vec![
        reviewed.submission.as_path(),
        reviewed.prepared.as_path(),
        reviewed.candidate.as_path(),
    ];
    inputs.extend(parents);
    stage_with(store, pin, out, &inputs, &app_anchor, |previous, update| {
        crate::review::sign(reviewed.inputs(previous, update, unlock))
    })
}

pub(super) fn stage_with(
    store: &Path,
    pin: &str,
    out: &Path,
    inputs: &[&Path],
    anchor: &Anchor<'_>,
    sign: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<()> {
    let store = directory(store)?;
    // Refuse stale/invalid input before creating work or reaching the signing gate.
    selected(&store, pin, anchor, true)
        .context("Release base verification failed; nothing signed")?;
    let canonical = inputs
        .iter()
        .map(|input| input.canonicalize())
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut forbidden = vec![store.as_path()];
    forbidden.extend(canonical.iter().map(PathBuf::as_path));
    let out = new_output(out, &forbidden)?;
    fs::create_dir(&out)?;
    let result = (|| -> Result<()> {
        let base = out.join("base");
        export_with(&store, &base, pin, anchor).context("Release base capture failed")?;
        println!("Release stage: base captured; source approval/signing pending");
        let update = out.join("signed-update");
        sign(&base.join("v1"), &update)
            .context("Release source approval/provenance/signing failed")?;
        println!("Release stage: signed update ready; local promotion pending");
        let assembly = out.join("assembly");
        assemble_with(&base.join("v1"), &update, &assembly, anchor)
            .context("Release assembly failed; signed update retained")?;
        let expected = check(&assembly.join("v1"), anchor, true)?.state;
        let current = digest(&serde_json::to_vec(&expected)?);
        promote_with(&assembly, &store, pin, anchor)
            .context("Release local promotion failed; inspect current pointer before retrying")?;
        println!("Release stage: local store promoted; remote unchanged. If export fails, re-export without signing again");
        let bundle = out.join("bundle");
        hosting::export(&store, &bundle, &current, anchor).context(
            "Local store promoted but bundle export failed; rerun export-hosting-bundle only",
        )?;
        let receipt = digest(&crate::review::read(
            &bundle.join("bundle.json"),
            384 * 1024,
        )?);
        let selected = hosting::verify(&bundle, &receipt, anchor)?;
        if selected != expected {
            bail!("Exported release differs from the signed assembly");
        }
        write(
            &out.join("release.json"),
            &serde_json::to_vec_pretty(&serde_json::json!({
                "schema": 1,
                "evidence_class": "verified-local-release-not-github-activation",
                "previous_current_sha256": pin,
                "current_sha256": current,
                "bundle_receipt_sha256": receipt,
                "snapshot": selected.snapshot,
                "index_version": selected.index_version
            }))?,
        )?;
        println!("Release stage: hosting bundle verified; receipt SHA256 {receipt}. Review the GitHub publication tuple before activation");
        Ok(())
    })();
    result.with_context(|| format!("Release work retained at {}; inspect current.json before further action; no automatic retry", out.display()))
}
