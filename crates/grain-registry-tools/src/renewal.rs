//! Operator renewal only: no new entries, approval, root rotation or publication.
use super::*;

pub(super) fn renew_with(
    v1: &Path,
    pin: &str,
    key: &Path,
    days: u32,
    out: &Path,
    anchor: &Anchor<'_>,
) -> Result<()> {
    if !hex(pin) || !(1..=30).contains(&days) {
        bail!("Renewal needs an independent snapshot digest and one to thirty days");
    }
    // Expired index/revocations can be recovered, but their authenticity, complete
    // assets and exact independently pinned source remain mandatory.
    let mut tree = check(v1, anchor, false)?;
    if tree.state.snapshot != pin {
        bail!("Renewal source snapshot changed");
    }
    let out = new_output(out, &[&tree.path])?;
    let roots: Roots = serde_json::from_slice(&tree.docs["roots.json"])?;
    if let Some(expires) = &roots.expires {
        expiry(expires, true).context("Expired roots require root-authorized recovery")?;
    }
    let parent = Parent::of(&tree.state);
    let deadline = Utc::now() + chrono::Duration::days(i64::from(days));
    let mut pending = BTreeMap::new();
    for name in ["index.json", "revocations.json"] {
        // Preserve future envelope/entry fields: typed deserialization is for
        // validation, never for replacing a signed document's contents.
        let mut value: serde_json::Value = serde_json::from_slice(&tree.docs[name])?;
        let object = value.as_object_mut().context("Renewal document envelope")?;
        let version = object["version"]
            .as_u64()
            .and_then(|v| v.checked_add(1))
            .context("Renewal metadata version exhausted")?;
        let old_expiry = DateTime::parse_from_rfc3339(
            object["expires"]
                .as_str()
                .context("Renewal metadata expiry")?,
        )?;
        if deadline.timestamp() <= old_expiry.timestamp() {
            bail!("Renewal must extend both metadata expirations");
        }
        object.insert("version".into(), version.into());
        object.insert("expires".into(), deadline.to_rfc3339().into());
        let bytes = serde_json::to_vec(&value)?;
        if bytes.len() > 4 * 1024 * 1024 {
            bail!("Renewed document exceeds metadata size limit");
        }
        pending.insert(name.to_owned(), bytes);
    }
    // Read the bounded non-linked key only after every public-input precondition.
    // Existing maintainer signing supports its unencrypted key format; production
    // key custody/authorization is an external release gate, not granted here.
    let key_text = String::from_utf8(crate::review::read(key, 8192)?)?;
    for (name, bytes) in pending {
        let signature = crate::sign_text(&key_text, &bytes)?;
        if name == "index.json" {
            let (_, status) = grain_core::trust::verify_index(
                &roots,
                &bytes,
                &signature,
                None,
                Utc::now().timestamp(),
                false,
            )?;
            if status != grain_core::trust::IndexStatus::Fresh {
                bail!("Renewed index is not fresh");
            }
        } else {
            grain_core::trust::verify_revocations(&roots, &bytes, &signature)?;
        }
        tree.docs
            .insert(format!("{name}.minisig"), signature.into_bytes());
        tree.docs.insert(name, bytes);
    }
    drop(key_text);
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    copy(&tree, &out.join("v1"))?;
    let next = check(&out.join("v1"), anchor, true)?;
    transition(&tree, &next, true)?;
    // The existing promotion still independently binds this parent to the locked
    // selected store and enforces version/history/revocation monotonicity.
    write(&out.join("promotion.json"), &serde_json::to_vec(&parent)?)?;
    sync_dir(&out)?;
    owned.1 = true;
    println!(
        "Renewed signed metadata: index {}, revocations {}, snapshot {}; hosting unchanged",
        next.state.index_version, next.state.revocations_version, next.state.snapshot
    );
    Ok(())
}
