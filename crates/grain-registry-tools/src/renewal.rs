//! Operator metadata edits: no new releases, approval, root rotation or publication.
use super::*;

pub(crate) enum MetadataChange<'a> {
    Renew {
        days: u32,
    },
    Revoke {
        days: u32,
        id: &'a str,
        version: &'a str,
        reason: &'a str,
    },
}

pub(super) fn renew_with(
    v1: &Path,
    pin: &str,
    key: &Path,
    days: u32,
    out: &Path,
    anchor: &Anchor<'_>,
    unlock: &crate::Unlock,
) -> Result<()> {
    rewrite_with(
        v1,
        pin,
        key,
        MetadataChange::Renew { days },
        out,
        anchor,
        unlock,
    )
}

pub(super) fn rewrite_with(
    v1: &Path,
    pin: &str,
    key: &Path,
    change: MetadataChange<'_>,
    out: &Path,
    anchor: &Anchor<'_>,
    unlock: &crate::Unlock,
) -> Result<()> {
    let days = match &change {
        MetadataChange::Renew { days } | MetadataChange::Revoke { days, .. } => *days,
    };
    if !hex(pin) || !(1..=30).contains(&days) {
        bail!("Renewal needs an independent snapshot digest and one to thirty days");
    }
    // Expired index/revocations can be recovered, but their authenticity, complete
    // assets and exact independently pinned source remain mandatory.
    let mut tree = check(v1, anchor, false)?;
    if tree.state.snapshot != pin {
        bail!("Renewal source snapshot changed");
    }
    if let MetadataChange::Revoke {
        id,
        version,
        reason,
        ..
    } = &change
    {
        if reason.trim().is_empty() || reason.len() > 1024 || reason.chars().any(char::is_control) {
            bail!("Revocation needs a nonempty plain-text reason of at most 1024 bytes");
        }
        if !tree
            .index
            .entries
            .iter()
            .any(|entry| entry.id == *id && entry.version == *version)
        {
            bail!("Revocation must name an exact release in the signed snapshot");
        }
        if tree.revocations.state_for(id, version) == Some(RevocationState::Revoked) {
            bail!("Release is already revoked");
        }
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
        if matches!(change, MetadataChange::Renew { .. })
            && deadline.timestamp() <= old_expiry.timestamp()
        {
            bail!("Renewal must extend both metadata expirations");
        }
        object.insert("version".into(), version.into());
        // An urgent revocation must work even immediately after renewal, and
        // must not shorten the lifetime already advertised to clients.
        if deadline.timestamp() > old_expiry.timestamp() {
            object.insert("expires".into(), deadline.to_rfc3339().into());
        }
        if name == "revocations.json" {
            if let MetadataChange::Revoke {
                id,
                version,
                reason,
                ..
            } = &change
            {
                object.entry("entries").or_insert_with(|| serde_json::json!([]))
                    .as_array_mut().context("Revocation entries must be an array")?
                    .push(serde_json::json!({"id":id,"version":version,"state":"revoked","reason":reason}));
            }
        }
        let bytes = serde_json::to_vec(&value)?;
        if bytes.len() > 4 * 1024 * 1024 {
            bail!("Renewed document exceeds metadata size limit");
        }
        pending.insert(name.to_owned(), bytes);
    }
    let generation = grain_core::trust::metadata_generation(
        &tree.docs["roots.json"],
        &pending["revocations.json"],
    );
    let mut index: serde_json::Value = serde_json::from_slice(&pending["index.json"])?;
    index["generation"] = serde_json::to_value(generation)?;
    let bytes = serde_json::to_vec(&index)?;
    if bytes.len() > 4 * 1024 * 1024 {
        bail!("Renewed document exceeds metadata size limit");
    }
    pending.insert("index.json".into(), bytes);
    // Read the bounded non-linked key only after every public-input precondition.
    {
        let signing_key = crate::signing_key::load(key, unlock)?;
        for (name, bytes) in pending {
            let signature = crate::signing_key::sign(&signing_key, &bytes)?;
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
    }
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
        "Prepared signed metadata: index {}, revocations {}, snapshot {}; hosting unchanged",
        next.state.index_version, next.state.revocations_version, next.state.snapshot
    );
    Ok(())
}
