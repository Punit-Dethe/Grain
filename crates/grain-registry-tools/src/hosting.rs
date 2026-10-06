//! Offline static-host handoff. History is proof, never the selected catalogue.
use super::*;

const RECEIPT_MAX: u64 = 384 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub(super) schema: u8,
    pub(super) current_sha256: String,
    pub(super) selected: State,
    pub(super) history: Vec<String>,
}

type Pool = BTreeMap<String, (Asset, PathBuf)>;
type Versions = BTreeMap<(String, String), String>;

fn reserve(versions: &mut Versions, tree: &Tree) -> Result<()> {
    for entry in &tree.index.entries {
        let key = (entry.id.clone(), entry.version.clone());
        let identity = digest(&serde_json::to_vec(&(
            &entry.sha256,
            entry.artifact_kind,
            entry.detail_document_hash(),
            entry
                .media
                .iter()
                .map(|m| (&m.sha256, &m.kind))
                .collect::<Vec<_>>(),
        ))?);
        if versions.get(&key).is_some_and(|old| old != &identity) {
            bail!("Conflicting historical extension version identities");
        }
        versions.insert(key, identity);
        if versions.len() > MAX_FILES {
            bail!("Hosting version history exceeds identity budget");
        }
    }
    Ok(())
}

fn merge(pool: &mut Pool, tree: &Tree) -> Result<()> {
    for (name, value) in &tree.assets {
        if let Some((old, _)) = pool.get(name) {
            if old.size.is_some() && value.size.is_some() && old.size != value.size {
                bail!("Historical asset sizes conflict");
            }
            if old.size.is_some() || value.size.is_none() {
                continue;
            }
        }
        pool.insert(name.clone(), (value.clone(), tree.path.join(name)));
        if pool.len() > MAX_FILES {
            bail!("Hosting asset pool exceeds file budget");
        }
    }
    Ok(())
}

fn names(path: &Path, allowed: &BTreeSet<String>) -> Result<()> {
    for item in fs::read_dir(path)? {
        let item = item?;
        if !allowed.contains(item.file_name().to_str().unwrap_or(""))
            || item.file_type()?.is_symlink()
        {
            bail!("Unexpected or linked hosting bundle input");
        }
    }
    Ok(())
}

fn proof_names() -> BTreeSet<String> {
    DOCS.iter().map(|v| v.to_string()).collect()
}

pub(super) fn verify(bundle: &Path, pin: &str, anchor: &Anchor<'_>) -> Result<State> {
    Ok(inspect(bundle, pin, anchor, true)?.selected)
}

// Only the authenticated previous publication may be expired. A new candidate
// and the public verify-hosting-bundle entry point always require freshness.
pub(super) fn inspect(
    bundle: &Path,
    pin: &str,
    anchor: &Anchor<'_>,
    fresh: bool,
) -> Result<Receipt> {
    if !hex(pin) {
        bail!("Independent hosting receipt digest required");
    }
    let bundle = directory(bundle)?;
    names(
        &bundle,
        &BTreeSet::from([
            "bundle.json".into(),
            "current.json".into(),
            "v1".into(),
            "history".into(),
        ]),
    )?;
    let raw = crate::review::read(&bundle.join("bundle.json"), RECEIPT_MAX)?;
    if digest(&raw) != pin {
        bail!("Hosting receipt digest differs from independent pin");
    }
    let receipt: Receipt = serde_json::from_slice(&raw)?;
    if receipt.schema != 1
        || !hex(&receipt.current_sha256)
        || receipt.history.len() >= MAX_HISTORY
        || receipt.history.windows(2).any(|v| v[0] >= v[1])
        || receipt
            .history
            .iter()
            .any(|v| !hex(v) || v == &receipt.selected.snapshot)
    {
        bail!("Malformed hosting receipt or history");
    }
    let pointer = crate::review::read(&bundle.join("current.json"), 8192)?;
    if digest(&pointer) != receipt.current_sha256
        || serde_json::from_slice::<State>(&pointer)? != receipt.selected
    {
        bail!("Hosting current pointer differs from pinned receipt");
    }
    let v1 = directory(&bundle.join("v1"))?;
    let current = metadata(&v1, anchor, fresh)?;
    if current.state != receipt.selected {
        bail!("Hosting selected metadata differs from pinned receipt");
    }
    let mut top = proof_names();
    top.extend(["blob".into(), "media".into()]);
    names(&v1, &top)?;
    let history = directory(&bundle.join("history"))?;
    inventory(&history, &receipt.history.iter().cloned().collect(), true)?;
    let mut pool = Pool::new();
    merge(&mut pool, &current)?;
    let mut versions = Versions::new();
    reserve(&mut versions, &current)?;
    let mut metadata_bytes: u64 = current.docs.values().map(|v| v.len() as u64).sum();
    for id in &receipt.history {
        let historical = metadata(&history.join(id), anchor, false)?;
        inventory(&historical.path, &proof_names(), false)?;
        if &historical.state.snapshot != id {
            bail!("Hosting history identity differs from signed proof");
        }
        metadata_bytes += historical
            .docs
            .values()
            .map(|v| v.len() as u64)
            .sum::<u64>();
        if metadata_bytes > MAX_HISTORY_DOCS {
            bail!("Hosting history exceeds metadata budget");
        }
        immutable_versions(&historical.index, &current.index)?;
        reserve(&mut versions, &historical)?;
        merge(&mut pool, &historical)?;
    }
    for folder in ["blob", "media"] {
        let dir = directory(&v1.join(folder))?;
        let allowed = pool
            .keys()
            .filter_map(|v| v.strip_prefix(&format!("{folder}/")).map(str::to_owned))
            .collect();
        inventory(&dir, &allowed, false)?;
    }
    let mut bytes = metadata_bytes + raw.len() as u64 + pointer.len() as u64;
    for (name, (asset, _)) in &pool {
        bytes += stream(&v1.join(name), asset, None)?;
        if bytes > MAX_TOTAL {
            bail!("Hosting bundle exceeds total byte budget");
        }
    }
    Ok(receipt)
}

pub(super) fn export(store: &Path, out: &Path, pin: &str, anchor: &Anchor<'_>) -> Result<()> {
    let store = directory(store)?;
    let out = new_output(out, &[&store])?;
    let _lock = lock(&store)?;
    let current = selected(&store, pin, anchor, true)?;
    let pointer = crate::review::read(&store.join("current.json"), 8192)?;
    if digest(&pointer) != pin {
        bail!("Current pointer changed during capture");
    }
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    let v1 = out.join("v1");
    let history = out.join("history");
    for dir in [&v1, &history, &v1.join("blob"), &v1.join("media")] {
        fs::create_dir(dir)?;
    }
    for (name, bytes) in &current.docs {
        write(&v1.join(name), bytes)?;
    }
    let mut pool = Pool::new();
    merge(&mut pool, &current)?;
    let mut versions = Versions::new();
    reserve(&mut versions, &current)?;
    let mut receipt = Receipt {
        schema: 1,
        current_sha256: pin.into(),
        selected: current.state.clone(),
        history: vec![],
    };
    let mut metadata_bytes: u64 = current.docs.values().map(|v| v.len() as u64).sum();
    visit_history(&store, anchor, |historical| {
        if historical.state.snapshot == current.state.snapshot {
            return Ok(());
        }
        immutable_versions(&historical.index, &current.index)?;
        reserve(&mut versions, &historical)?;
        merge(&mut pool, &historical)?;
        let id = historical.state.snapshot.clone();
        let proof = history.join(&id);
        fs::create_dir(&proof)?;
        for (name, bytes) in &historical.docs {
            write(&proof.join(name), bytes)?;
            metadata_bytes += bytes.len() as u64;
        }
        sync_dir(&proof)?;
        receipt.history.push(id);
        Ok(())
    })?;
    receipt.history.sort();
    let raw = serde_json::to_vec(&receipt)?;
    if raw.len() as u64 > RECEIPT_MAX {
        bail!("Hosting receipt exceeds byte budget");
    }
    let mut bytes = metadata_bytes + raw.len() as u64 + pointer.len() as u64;
    for (name, (asset, source)) in &pool {
        bytes += stream(source, asset, Some(&v1.join(name)))?;
        if bytes > MAX_TOTAL {
            bail!("Hosting bundle exceeds total byte budget");
        }
    }
    write(&out.join("current.json"), &pointer)?;
    write(&out.join("bundle.json"), &raw)?;
    let receipt_pin = digest(&raw);
    if verify(&out, &receipt_pin, anchor)? != current.state {
        bail!("Copied hosting bundle differs");
    }
    for dir in [&v1.join("blob"), &v1.join("media"), &v1, &history, &out] {
        sync_dir(dir)?;
    }
    owned.1 = true;
    println!("Static hosting bundle prepared; receipt SHA256 {receipt_pin}; no hosting changed or approval granted");
    Ok(())
}
