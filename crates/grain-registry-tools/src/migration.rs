//! Current empty-catalogue migration. Genuine publisher signing, no historical
//! revocation fabrication, source approval, trust rotation or deployment.
use super::*;

fn seed_revocations() -> Result<Revocations> {
    let roots = grain_core::trust::verify_roots(
        grain_core::trust::SEED_ROOTS.as_bytes(),
        grain_core::trust::SEED_ROOTS_SIG,
    )?;
    Ok(grain_core::trust::verify_revocations(
        &roots,
        grain_core::trust::SEED_REVOCATIONS.as_bytes(),
        grain_core::trust::SEED_REVOCATIONS_SIG,
    )?)
}

pub(crate) fn sign(path: &Path, pin: &str, key: &Path, days: u32, out: &Path) -> Result<()> {
    sign_with(path, pin, key, days, out, &app_anchor)
}

pub(super) fn sign_with(
    path: &Path,
    pin: &str,
    key: &Path,
    days: u32,
    out: &Path,
    anchor: &Anchor<'_>,
) -> Result<()> {
    if !(1..=30).contains(&days) {
        bail!("Migration lifetime must be one through thirty days");
    }
    let archive = legacy::inspect(path, pin, anchor)?;
    let out = new_output(out, &[&archive.path])?;
    let roots: Roots = serde_json::from_slice(&archive.latest_docs["roots.json"])?;
    if roots.version != archive.max_roots {
        bail!("Legacy tip rolls back roots; root-authorized recovery required");
    }
    if let Some(expires) = &roots.expires {
        expiry(expires, true).context("Expired roots require root-authorized recovery")?;
    }
    let version = archive
        .max_index
        .checked_add(1)
        .context("Migration index version exhausted")?;
    let expires = (Utc::now() + chrono::Duration::days(i64::from(days))).to_rfc3339();
    let mut index: serde_json::Value = serde_json::from_slice(&archive.latest_docs["index.json"])?;
    index["version"] = version.into();
    index["expires"] = expires.clone().into();
    index["entries"] = serde_json::json!([]);
    index["legacy_history_sha256"] = pin.into();
    let entries: Vec<_> = archive
        .reserved
        .iter()
        .map(|(id, version)| {
            serde_json::json!({
                "id": id, "version": version, "state": "revoked",
                "reason": "Retired legacy extension contract; publish a new tool-only version."
            })
        })
        .collect();
    let seed = seed_revocations()?;
    let revocation_version = seed
        .version
        .checked_add(1)
        .context("Migration seed revocation version exhausted")?;
    let mut revocations: serde_json::Value =
        serde_json::from_str(grain_core::trust::SEED_REVOCATIONS)?;
    revocations["version"] = revocation_version.into();
    revocations["expires"] = expires.into();
    revocations["legacy_history_sha256"] = pin.into();
    revocations["entries"]
        .as_array_mut()
        .context("Seed revocation entries")?
        .extend(entries);
    let mut docs = BTreeMap::from([
        (
            "roots.json".to_owned(),
            archive.latest_docs["roots.json"].clone(),
        ),
        (
            "roots.json.minisig".to_owned(),
            archive.latest_docs["roots.json.minisig"].clone(),
        ),
        ("index.json".to_owned(), serde_json::to_vec(&index)?),
        (
            "revocations.json".to_owned(),
            serde_json::to_vec(&revocations)?,
        ),
    ]);
    if docs["index.json"].len() > 4 * 1024 * 1024
        || docs["revocations.json"].len() > 4 * 1024 * 1024
    {
        bail!("Migration metadata exceeds bound");
    }
    // Only after all public-input preconditions. No key discovery or fallback.
    let key_text = String::from_utf8(
        crate::review::read(key, 8192).context("Read protected migration publishing key")?,
    )?;
    for name in ["index.json", "revocations.json"] {
        let sig = crate::sign_text(&key_text, &docs[name])?;
        grain_core::trust::verify_publisher_signature(&roots, &docs[name], &sig)?;
        docs.insert(format!("{name}.minisig"), sig.into_bytes());
    }
    drop(key_text);
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    let v1 = out.join("v1");
    for dir in [&v1, &v1.join("blob"), &v1.join("media")] {
        fs::create_dir(dir)?;
    }
    for (name, raw) in docs {
        write(&v1.join(name), &raw)?;
    }
    let tree = check(&v1, anchor, true)?;
    legacy::enforce(&archive, &tree)?;
    legacy::copy_archive(&archive, pin, &out.join("legacy"), anchor)?;
    validate_baseline(
        &tree,
        &legacy::inspect(&out.join("legacy"), pin, anchor)?,
        pin,
    )?;
    write(
        &out.join("migration.json"),
        &serde_json::to_vec(&serde_json::json!({
            "schema":1,"evidence_class":"signed-empty-legacy-migration-not-release-approval",
            "legacy_history_sha256":pin,"snapshot":tree.state.snapshot,
            "index_version":tree.state.index_version,"revocations_version":revocation_version
        }))?,
    )?;
    for dir in [&v1.join("blob"), &v1.join("media"), &v1, &out] {
        sync_dir(dir)?;
    }
    owned.1 = true;
    println!("Signed empty migration baseline: index {version}, {} retired versions; no production approval or hosting changed", archive.reserved.len());
    Ok(())
}

pub(super) fn validate_baseline(tree: &Tree, archive: &legacy::Archive, pin: &str) -> Result<()> {
    if tree.legacy_pin.as_deref() != Some(pin)
        || !tree.index.entries.is_empty()
        || tree.state.index_version
            != archive
                .max_index
                .checked_add(1)
                .context("Migration index version exhausted")?
        || tree.state.revocations_version
            != seed_revocations()?
                .version
                .checked_add(1)
                .context("Migration seed revocation version exhausted")?
        || tree.docs["roots.json"] != archive.latest_docs["roots.json"]
        || tree.docs["roots.json.minisig"] != archive.latest_docs["roots.json.minisig"]
    {
        bail!("Initial migration must use an empty next-version catalogue and unchanged authenticated roots");
    }
    for old in seed_revocations()?.entries {
        if !tree.revocations.entries.iter().any(|r| {
            r.id == old.id
                && (r.version.is_none() || r.version == old.version)
                && (r.state == RevocationState::Revoked || r.state == old.state)
        }) {
            bail!("Initial migration cannot erase or weaken app-seeded revocations");
        }
    }
    legacy::enforce(archive, tree)
}
