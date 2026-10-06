//! Historical evidence only. Never manufacture missing revocations or admit old
//! capabilities. Protected migration must independently consume these proofs.
use super::*;
use capture::Blob;

pub(super) const LEGACY_DOCS: [&str; 4] = [
    "roots.json",
    "roots.json.minisig",
    "index.json",
    "index.json.minisig",
];
pub(super) const MAX_COMMITS: usize = 128;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub schema: u8,
    pub tip_commit: String,
    // Exact ordered catalogue-changing ancestry, not a cherry-picked subset.
    pub commits: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    commit: String,
    documents: BTreeMap<String, String>,
    roots_version: u64,
    index_version: u64,
    index_expires: String,
    assets: Vec<String>,
    recovered_assets: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    id: String,
    version: String,
    artifact_kind: String,
    artifact_sha256: String,
    document_sha256: String,
    media: Vec<(String, String)>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reservation {
    identity: Identity,
    commits: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    schema: u8,
    evidence_class: String,
    repository: String,
    manifest_sha256: String,
    tip_commit: String,
    proofs: Vec<Proof>,
    reservations: Vec<Reservation>,
    conflicted_versions: Vec<(String, String)>,
    assets: BTreeMap<String, String>,
    asset_sources: BTreeMap<String, String>,
    bytes: u64,
}

pub(super) struct Archive {
    pub path: PathBuf,
    pub assets: BTreeMap<String, Asset>,
    pub reserved: BTreeSet<(String, String)>,
    pub repository: String,
    pub tip: String,
    pub max_index: u64,
    pub max_roots: u64,
    pub latest_docs: BTreeMap<String, Vec<u8>>,
    pub data_bytes: u64,
}

pub(super) fn inspect(path: &Path, pin: &str, anchor: &Anchor<'_>) -> Result<Archive> {
    if !hex(pin) {
        bail!("Independent legacy receipt digest required");
    }
    let path = directory(path)?;
    for item in fs::read_dir(&path)? {
        let item = item?;
        if !["legacy-history.json", "manifest.json", "proofs", "assets"]
            .contains(&item.file_name().to_str().unwrap_or(""))
            || item.file_type()?.is_symlink()
        {
            bail!("Unexpected or linked legacy archive input");
        }
    }
    let raw = crate::review::read(&path.join("legacy-history.json"), 8 * 1024 * 1024)?;
    if digest(&raw) != pin {
        bail!("Legacy receipt differs from independent pin");
    }
    let receipt: Receipt = serde_json::from_slice(&raw)?;
    let manifest_raw = crate::review::read(&path.join("manifest.json"), 256 * 1024)?;
    let manifest: Manifest = serde_json::from_slice(&manifest_raw)?;
    publication::repository_url(&receipt.repository)?;
    if receipt.schema != 1
        || receipt.evidence_class
            != "authenticated-legacy-history-not-release-approval-no-revocation-proof"
        || manifest.schema != 1
        || manifest.commits.is_empty()
        || manifest.commits.len() > MAX_COMMITS
        || !publication::oid(&manifest.tip_commit)
        || manifest.commits.iter().any(|c| !publication::oid(c))
        || manifest.commits.iter().collect::<BTreeSet<_>>().len() != manifest.commits.len()
        || receipt.tip_commit != manifest.tip_commit
        || receipt.manifest_sha256 != digest(&manifest_raw)
        || receipt.proofs.iter().map(|p| &p.commit).collect::<Vec<_>>()
            != manifest.commits.iter().collect::<Vec<_>>()
    {
        bail!("Malformed legacy archive or manifest binding");
    }
    let proofs = directory(&path.join("proofs"))?;
    super::inventory(&proofs, &manifest.commits.iter().cloned().collect(), true)?;
    let mut assets = BTreeMap::new();
    let mut versions: BTreeMap<Identity, Vec<String>> = BTreeMap::new();
    let mut variants: BTreeMap<(String, String), BTreeSet<Identity>> = BTreeMap::new();
    let mut metadata_bytes = 0_u64;
    let mut refs = 0_usize;
    let mut max_index = 0;
    let mut max_roots = 0;
    let mut latest_docs = BTreeMap::new();
    for proof in &receipt.proofs {
        let dir = directory(&proofs.join(&proof.commit))?;
        super::inventory(
            &dir,
            &LEGACY_DOCS.iter().map(|n| n.to_string()).collect(),
            false,
        )?;
        let (docs, roots, index) = proof_metadata(&dir, anchor)?;
        metadata_bytes += docs.values().map(|d| d.len() as u64).sum::<u64>();
        if metadata_bytes > MAX_HISTORY_DOCS {
            bail!("Legacy history exceeds metadata budget");
        }
        let referenced = index_assets(&index, false)?;
        if proof.documents != docs.iter().map(|(n, d)| (n.clone(), digest(d))).collect()
            || proof.roots_version != roots.version
            || proof.index_version != index.version
            || proof.index_expires != index.expires
            || proof.assets != referenced.keys().cloned().collect::<Vec<_>>()
            || proof.recovered_assets.windows(2).any(|v| v[0] >= v[1])
            || proof
                .recovered_assets
                .iter()
                .any(|n| !referenced.contains_key(n))
        {
            bail!("Legacy proof receipt differs from authenticated documents");
        }
        for (name, value) in referenced {
            if let Some(old) = assets.get(&name) {
                let old: &Asset = old;
                if old.size.is_some() && value.size.is_some() && old.size != value.size {
                    bail!("Legacy signed asset sizes conflict");
                }
                if old.size.is_some() || value.size.is_none() {
                    continue;
                }
            }
            assets.insert(name, value);
            if assets.len() > MAX_FILES {
                bail!("Legacy asset pool exceeds budget");
            }
        }
        for entry in &index.entries {
            refs += 1;
            if refs > MAX_FILES * 16 {
                bail!("Legacy reservation reference budget exceeded");
            }
            let value = identity(entry);
            variants
                .entry((entry.id.clone(), entry.version.clone()))
                .or_default()
                .insert(value.clone());
            versions
                .entry(value)
                .or_default()
                .push(proof.commit.clone());
            if versions.len() > MAX_FILES {
                bail!("Legacy version reservation budget exceeded");
            }
        }
        max_index = max_index.max(index.version);
        max_roots = max_roots.max(roots.version);
        latest_docs = docs;
    }
    let reserved = variants.keys().cloned().collect();
    let expected_versions: Vec<_> = versions
        .into_iter()
        .map(|(identity, commits)| Reservation { identity, commits })
        .collect();
    let expected_conflicts: Vec<_> = variants
        .into_iter()
        .filter_map(|(k, v)| (v.len() > 1).then_some(k))
        .collect();
    if receipt.reservations != expected_versions
        || receipt.conflicted_versions != expected_conflicts
        || receipt.assets
            != assets
                .iter()
                .map(|(n, a)| (n.clone(), a.hash.clone()))
                .collect()
        || receipt.asset_sources.keys().collect::<Vec<_>>() != assets.keys().collect::<Vec<_>>()
        || receipt
            .asset_sources
            .values()
            .any(|c| !manifest.commits.contains(c))
    {
        bail!("Legacy reservations or asset sources differ from authenticated history");
    }
    let asset_root = directory(&path.join("assets"))?;
    super::inventory(
        &asset_root,
        &BTreeSet::from(["blob".into(), "media".into()]),
        true,
    )?;
    for folder in ["blob", "media"] {
        let dir = directory(&asset_root.join(folder))?;
        let names = assets
            .keys()
            .filter_map(|n| n.strip_prefix(&format!("{folder}/")).map(str::to_owned))
            .collect();
        super::inventory(&dir, &names, false)?;
    }
    let mut bytes = metadata_bytes + manifest_raw.len() as u64;
    for (name, value) in &assets {
        bytes += stream(&asset_root.join(name), value, None)?;
        if bytes > MAX_TOTAL {
            bail!("Legacy history exceeds byte budget");
        }
    }
    if receipt.bytes != bytes {
        bail!("Legacy byte receipt differs from captured data");
    }
    Ok(Archive {
        path,
        assets,
        reserved,
        repository: receipt.repository,
        tip: receipt.tip_commit,
        max_index,
        max_roots,
        latest_docs,
        data_bytes: bytes + raw.len() as u64,
    })
}

pub(super) fn copy_archive(
    archive: &Archive,
    pin: &str,
    out: &Path,
    anchor: &Anchor<'_>,
) -> Result<()> {
    fs::create_dir(out)?;
    for dir in ["proofs", "assets", "assets/blob", "assets/media"] {
        fs::create_dir(out.join(dir))?;
    }
    for (name, bound) in [
        ("legacy-history.json", 8 * 1024 * 1024),
        ("manifest.json", 256 * 1024),
    ] {
        write(
            &out.join(name),
            &crate::review::read(&archive.path.join(name), bound)?,
        )?;
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(out.join("manifest.json"))?)?;
    if manifest.commits.len() > MAX_COMMITS || manifest.commits.iter().any(|c| !publication::oid(c))
    {
        bail!("Copied legacy manifest paths are invalid");
    }
    for commit in manifest.commits {
        let dir = out.join("proofs").join(&commit);
        fs::create_dir(&dir)?;
        let (docs, _, _) = proof_metadata(&archive.path.join("proofs").join(commit), anchor)?;
        for (name, raw) in docs {
            write(&dir.join(name), &raw)?;
        }
        sync_dir(&dir)?;
    }
    for (name, value) in &archive.assets {
        stream(
            &archive.path.join("assets").join(name),
            value,
            Some(&out.join("assets").join(name)),
        )?;
    }
    inspect(out, pin, anchor)?;
    for dir in ["proofs", "assets/blob", "assets/media", "assets", ""] {
        sync_dir(&out.join(dir))?;
    }
    Ok(())
}

pub(super) fn enforce(archive: &Archive, tree: &Tree) -> Result<()> {
    for entry in &tree.index.entries {
        if archive
            .reserved
            .contains(&(entry.id.clone(), entry.version.clone()))
        {
            bail!("Legacy extension versions are permanently reserved; publish a new tool-only version");
        }
    }
    for (id, version) in &archive.reserved {
        if !tree.revocations.entries.iter().any(|r| {
            r.id == *id
                && r.version.as_ref().is_none_or(|v| v == version)
                && r.state == RevocationState::Revoked
        }) {
            bail!("Migrated metadata must retain every legacy version revocation");
        }
    }
    Ok(())
}

pub(crate) fn verify(path: &Path, pin: &str) -> Result<()> {
    let archive = inspect(path, pin, &app_anchor)?;
    println!("Authenticated legacy archive verified: {} reserved versions; historical evidence, no activation", archive.reserved.len());
    Ok(())
}

fn proof_metadata(
    path: &Path,
    anchor: &Anchor<'_>,
) -> Result<(BTreeMap<String, Vec<u8>>, Roots, Index)> {
    let mut documents = BTreeMap::new();
    for name in LEGACY_DOCS {
        documents.insert(
            name.to_owned(),
            crate::review::read(
                &path.join(name),
                if name.ends_with(".minisig") {
                    8192
                } else if name == "roots.json" {
                    64 * 1024
                } else {
                    4 * 1024 * 1024
                },
            )?,
        );
    }
    let roots = anchor(
        &documents["roots.json"],
        std::str::from_utf8(&documents["roots.json.minisig"])?,
    )?;
    grain_core::trust::verify_publisher_signature(
        &roots,
        &documents["index.json"],
        std::str::from_utf8(&documents["index.json.minisig"])?,
    )?;
    // Historical view only; keep the authenticated raw bytes exactly unchanged.
    let mut view: serde_json::Value = serde_json::from_slice(&documents["index.json"])?;
    if let Some(entries) = view
        .get_mut("entries")
        .and_then(serde_json::Value::as_array_mut)
    {
        for entry in entries {
            if entry.get("tier").and_then(serde_json::Value::as_str) == Some("builtin") {
                entry["tier"] = serde_json::Value::String("pack".into());
            }
        }
    }
    let index: Index = serde_json::from_value(view)?;
    if roots.spec != 1 || roots.version == 0 || index.spec != 1 || index.version == 0 {
        bail!("Unsupported legacy roots/index version/spec");
    }
    if let Some(expires) = &roots.expires {
        expiry(expires, false)?;
    }
    expiry(&index.expires, false)?;
    Ok((documents, roots, index))
}

fn identity(entry: &grain_sdk::distribution::IndexEntry) -> Identity {
    Identity {
        id: entry.id.clone(),
        version: entry.version.clone(),
        artifact_kind: match entry.artifact_kind {
            ArtifactKind::Native => "native",
            ArtifactKind::McpDescriptor => "mcp-descriptor",
        }
        .into(),
        artifact_sha256: entry.sha256.clone(),
        document_sha256: entry.detail_document_hash().into(),
        media: entry
            .media
            .iter()
            .map(|m| (m.sha256.clone(), m.kind.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

fn inventory(root: &Path, commit: &str) -> Result<BTreeMap<String, Blob>> {
    let listing = publication::git(
        root,
        &[
            "ls-tree",
            "-r",
            "-l",
            "--full-tree",
            "-z",
            commit,
            "--",
            "v1",
        ],
        false,
    )?;
    let mut blobs = BTreeMap::new();
    for record in listing.split('\0').filter(|v| !v.is_empty()) {
        let (header, name) = record
            .split_once('\t')
            .context("Malformed legacy Git inventory")?;
        let fields: Vec<_> = header.split_ascii_whitespace().collect();
        if fields.len() != 4
            || fields[0] != "100644"
            || fields[1] != "blob"
            || !publication::oid(fields[2])
        {
            bail!("Legacy history requires regular non-executable Git blobs");
        }
        if name.starts_with("v1/revocations.") {
            bail!("Legacy-only archive must not discard existing revocation evidence");
        }
        let (_, limit) = capture::destination(name)?;
        let size: u64 = fields[3]
            .parse()
            .context("Legacy committed blob unavailable or invalid size")?;
        if size == 0 || size > limit {
            bail!("Legacy committed file exceeds its byte bound");
        }
        let name = name.strip_prefix("v1/").context("Unexpected legacy path")?;
        if blobs
            .insert(
                name.into(),
                Blob {
                    oid: fields[2].into(),
                    size,
                },
            )
            .is_some()
            || blobs.len() > MAX_FILES + LEGACY_DOCS.len()
        {
            bail!("Legacy file inventory exceeds budget or duplicates a path");
        }
    }
    for name in LEGACY_DOCS {
        if !blobs.contains_key(name) {
            bail!("Incomplete legacy signed proof: missing {name}");
        }
    }
    Ok(blobs)
}

pub(crate) fn capture(
    root: &Path,
    repository: &str,
    manifest: &Path,
    pin: &str,
    out: &Path,
) -> Result<()> {
    capture_with(root, repository, manifest, pin, out, &app_anchor)
}

pub(super) fn capture_with(
    root: &Path,
    repository: &str,
    manifest: &Path,
    pin: &str,
    out: &Path,
    anchor: &Anchor<'_>,
) -> Result<()> {
    let url = publication::repository_url(repository)?;
    if !hex(pin) {
        bail!("Independent legacy manifest digest required");
    }
    let manifest_path = manifest.canonicalize()?;
    let raw = crate::review::read(manifest, 256 * 1024)?;
    if digest(&raw) != pin {
        bail!("Legacy manifest differs from independent pin");
    }
    let manifest: Manifest = serde_json::from_slice(&raw)?;
    if manifest.schema != 1
        || !publication::oid(&manifest.tip_commit)
        || manifest.commits.is_empty()
        || manifest.commits.len() > MAX_COMMITS
        || manifest.commits.iter().any(|id| !publication::oid(id))
        || manifest.commits.iter().collect::<BTreeSet<_>>().len() != manifest.commits.len()
    {
        bail!("Malformed or oversized legacy history manifest");
    }
    let root = directory(root)?;
    let out = new_output(out, &[&root, &manifest_path])?;
    publication::checkout(&root, &url, None)?;
    if publication::git(&root, &["cat-file", "-t", &manifest.tip_commit], false)?.trim() != "commit"
    {
        bail!("Legacy tip must be an actual Git commit");
    }
    // Complete reachable history for these documents, including signature-only
    // updates and merges. Git output/time are bounded; no fetch or lazy objects.
    let mut args = vec![
        "rev-list",
        "--full-history",
        "--topo-order",
        "--reverse",
        &manifest.tip_commit,
        "--",
    ];
    let paths = DOCS.map(|doc| format!("v1/{doc}"));
    args.extend(paths.iter().map(String::as_str));
    let history = publication::git(&root, &args, false)?;
    if history.lines().collect::<Vec<_>>()
        != manifest
            .commits
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    {
        bail!("Legacy manifest must cover exact complete catalogue-changing ancestry");
    }
    // Early signed catalogues sometimes preceded their asset commit. Recover
    // only exact content-addressed bytes present in this independently pinned
    // history, never rebuild a package or pretend it existed in an earlier tree.
    let mut available: BTreeMap<String, (Blob, String)> = BTreeMap::new();
    let mut available_bytes = 0_u64;
    for commit in &manifest.commits {
        for (name, blob) in inventory(&root, commit)? {
            if LEGACY_DOCS.contains(&name.as_str()) {
                continue;
            }
            if let Some((old, _)) = available.get(&name) {
                if old.oid != blob.oid {
                    bail!("Legacy committed address changed bytes between proofs");
                }
            } else {
                available_bytes += blob.size;
                if available_bytes > MAX_TOTAL || available.len() >= MAX_FILES {
                    bail!("Legacy recovery inventory exceeds budget");
                }
                available.insert(name, (blob, commit.clone()));
            }
        }
    }
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    for dir in ["proofs", "assets", "assets/blob", "assets/media"] {
        fs::create_dir(out.join(dir))?;
    }
    let mut receipt = Receipt {
        schema: 1,
        evidence_class: "authenticated-legacy-history-not-release-approval-no-revocation-proof"
            .into(),
        repository: repository.into(),
        manifest_sha256: pin.into(),
        tip_commit: manifest.tip_commit,
        proofs: vec![],
        reservations: vec![],
        conflicted_versions: vec![],
        assets: BTreeMap::new(),
        asset_sources: BTreeMap::new(),
        bytes: raw.len() as u64,
    };
    let mut versions: BTreeMap<Identity, Vec<String>> = BTreeMap::new();
    let mut variant_counts: BTreeMap<(String, String), BTreeSet<Identity>> = BTreeMap::new();
    let mut metadata_bytes = 0_u64;
    let mut reservation_refs = 0_usize;
    for commit in manifest.commits {
        let blobs = inventory(&root, &commit)?;
        let proof = out.join("proofs").join(&commit);
        fs::create_dir(&proof)?;
        let docs = LEGACY_DOCS
            .iter()
            .map(|name| {
                let blob = &blobs[*name];
                (
                    PathBuf::from(name),
                    Blob {
                        oid: blob.oid.clone(),
                        size: blob.size,
                    },
                )
            })
            .collect();
        let bytes = capture::materialize(&root, &docs, &proof)?;
        metadata_bytes += bytes;
        receipt.bytes += bytes;
        if metadata_bytes > MAX_HISTORY_DOCS || receipt.bytes > MAX_TOTAL {
            bail!("Legacy history exceeds byte budget");
        }
        let (documents, roots, index) = proof_metadata(&proof, anchor)?;
        let assets = index_assets(&index, false)?;
        let mut selected = BTreeMap::new();
        let mut recovered_assets = vec![];
        for (name, asset) in &assets {
            let (blob, source) = available.get(name).with_context(|| {
                format!("Missing historically referenced committed asset {name} at {commit}")
            })?;
            if !blobs.contains_key(name) {
                recovered_assets.push(name.clone());
            }
            if asset.size.is_some_and(|size| size != blob.size) {
                bail!("Legacy committed asset differs from signed size");
            }
            receipt.asset_sources.insert(name.clone(), source.clone());
            if !receipt.assets.contains_key(name) {
                receipt.bytes += blob.size;
                if receipt.bytes > MAX_TOTAL || receipt.assets.len() >= MAX_FILES {
                    bail!("Legacy asset pool exceeds budget");
                }
                selected.insert(
                    PathBuf::from(name),
                    Blob {
                        oid: blob.oid.clone(),
                        size: blob.size,
                    },
                );
            }
        }
        if !selected.is_empty() {
            capture::materialize(&root, &selected, &out.join("assets"))?;
        }
        for (name, asset) in &assets {
            // Deduplication cannot waive the signed size/hash checks.
            stream(&out.join("assets").join(name), asset, None)?;
            receipt.assets.insert(name.clone(), asset.hash.clone());
        }
        for entry in &index.entries {
            reservation_refs += 1;
            if reservation_refs > MAX_FILES * 16 {
                bail!("Legacy reservation reference budget exceeded");
            }
            let identity = identity(entry);
            variant_counts
                .entry((entry.id.clone(), entry.version.clone()))
                .or_default()
                .insert(identity.clone());
            versions.entry(identity).or_default().push(commit.clone());
            if versions.len() > MAX_FILES {
                bail!("Legacy version reservation budget exceeded");
            }
        }
        receipt.proofs.push(Proof {
            commit,
            documents: documents
                .iter()
                .map(|(name, raw)| (name.to_string(), digest(raw)))
                .collect(),
            roots_version: roots.version,
            index_version: index.version,
            index_expires: index.expires,
            assets: assets.into_keys().collect(),
            recovered_assets,
        });
    }
    receipt.reservations = versions
        .into_iter()
        .map(|(identity, commits)| Reservation { identity, commits })
        .collect();
    receipt.conflicted_versions = variant_counts
        .into_iter()
        .filter_map(|(version, variants)| (variants.len() > 1).then_some(version))
        .collect();
    write(&out.join("manifest.json"), &raw)?;
    let raw = serde_json::to_vec_pretty(&receipt)?;
    if raw.len() > 8 * 1024 * 1024 {
        bail!("Legacy receipt exceeds byte budget");
    }
    write(&out.join("legacy-history.json"), &raw)?;
    for dir in ["proofs", "assets/blob", "assets/media", "assets", ""] {
        sync_dir(&out.join(dir))?;
    }
    owned.1 = true;
    println!("Legacy history preserved: {} proofs, {} identity variants, {} conflicts; receipt SHA256 {}; no activation or revocation proof", receipt.proofs.len(), receipt.reservations.len(), receipt.conflicted_versions.len(), digest(&raw));
    Ok(())
}
