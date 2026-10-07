//! Clean unpublished catalogue initialization from the app's current trust seed.
use super::*;

pub(crate) fn create(key: &Path, days: u32, out: &Path) -> Result<()> {
    let seed = seed()?;
    create_with(seed.path(), key, days, out, &app_anchor)
}

pub(super) fn seed() -> Result<tempfile::TempDir> {
    let seed = tempfile::tempdir()?;
    for folder in ["blob", "media"] {
        fs::create_dir(seed.path().join(folder))?;
    }
    for (name, raw) in [
        ("roots.json", grain_core::trust::SEED_ROOTS),
        ("roots.json.minisig", grain_core::trust::SEED_ROOTS_SIG),
        ("index.json", grain_core::trust::SEED_INDEX),
        ("index.json.minisig", grain_core::trust::SEED_INDEX_SIG),
        ("revocations.json", grain_core::trust::SEED_REVOCATIONS),
        (
            "revocations.json.minisig",
            grain_core::trust::SEED_REVOCATIONS_SIG,
        ),
    ] {
        write(&seed.path().join(name), raw.as_bytes())?;
    }
    Ok(seed)
}

pub(super) fn create_with(
    seed: &Path,
    key: &Path,
    days: u32,
    out: &Path,
    anchor: &Anchor<'_>,
) -> Result<()> {
    if !(1..=30).contains(&days) {
        bail!("Bootstrap lifetime must be one through thirty days");
    }
    let mut tree = check_with_seed(seed, anchor, true, true)?;
    if !tree.index.entries.is_empty() {
        bail!("Bootstrap requires an empty current app seed");
    }
    let out = new_output(out, &[&tree.path])?;
    let deadline = (Utc::now() + chrono::Duration::days(i64::from(days))).to_rfc3339();
    let roots: Roots = serde_json::from_slice(&tree.docs["roots.json"])?;
    let mut pending = BTreeMap::new();
    for name in ["index.json", "revocations.json"] {
        let mut value: serde_json::Value = serde_json::from_slice(&tree.docs[name])?;
        let version = value["version"]
            .as_u64()
            .and_then(|v| v.checked_add(1))
            .context("Bootstrap metadata version exhausted")?;
        value["version"] = version.into();
        value["expires"] = deadline.clone().into();
        let raw = serde_json::to_vec(&value)?;
        if raw.len() > 4 * 1024 * 1024 {
            bail!("Bootstrap metadata exceeds bound");
        }
        pending.insert(name, raw);
    }
    let generation = grain_core::trust::metadata_generation(
        &tree.docs["roots.json"],
        &pending["revocations.json"],
    );
    let mut index: serde_json::Value = serde_json::from_slice(&pending["index.json"])?;
    index["generation"] = serde_json::to_value(generation)?;
    let bytes = serde_json::to_vec(&index)?;
    if bytes.len() > 4 * 1024 * 1024 {
        bail!("Bootstrap metadata exceeds bound");
    }
    pending.insert("index.json", bytes);
    // Current trust seed only; no old registry, archive, settings or key discovery.
    let key_text = String::from_utf8(
        crate::review::read(key, 8192).context("Read explicit bootstrap publisher key")?,
    )?;
    for (name, raw) in pending {
        let sig = crate::sign_text(&key_text, &raw)?;
        grain_core::trust::verify_publisher_signature(&roots, &raw, &sig)?;
        tree.docs
            .insert(format!("{name}.minisig"), sig.into_bytes());
        tree.docs.insert(name.into(), raw);
    }
    drop(key_text);
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    copy(&tree, &out.join("v1"))?;
    let verified = check(&out.join("v1"), anchor, true)?;
    if !verified.index.entries.is_empty() {
        bail!("Bootstrap output must remain empty");
    }
    sync_dir(&out)?;
    owned.1 = true;
    println!(
        "Clean signed catalogue prepared: index {}, revocations {}; hosting unchanged",
        verified.state.index_version, verified.state.revocations_version
    );
    Ok(())
}
