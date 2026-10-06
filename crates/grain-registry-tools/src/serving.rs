//! Complete local serving snapshots. No author execution or hosting.
//! Renewal signing is isolated in the maintainer-only renewal child module.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use grain_sdk::distribution::{ArtifactKind, Index, RevocationState, Revocations, Roots};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::prepare::{digest, new_output, OwnedOutput};
#[path = "capture.rs"]
pub(crate) mod capture;
#[path = "hosting.rs"]
mod hosting;
#[path = "publication.rs"]
pub(crate) mod publication;
#[path = "renewal.rs"]
mod renewal;

const DOCS: [&str; 6] = [
    "roots.json",
    "roots.json.minisig",
    "index.json",
    "index.json.minisig",
    "revocations.json",
    "revocations.json.minisig",
];
const MAX_FILES: usize = 8192;
const MAX_TOTAL: u64 = 1024 * 1024 * 1024;
const MAX_HISTORY: usize = 4096;
const MAX_HISTORY_DOCS: u64 = 128 * 1024 * 1024;
type Anchor<'a> = dyn Fn(&[u8], &str) -> Result<Roots> + 'a;

#[derive(Clone)]
struct Asset {
    hash: String,
    size: Option<u64>,
    max: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema: u8,
    snapshot: String,
    index_sha256: String,
    index_version: u64,
    roots_sha256: String,
    roots_version: u64,
    revocations_sha256: String,
    revocations_version: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parent {
    schema: u8,
    index_sha256: String,
    roots_sha256: String,
    revocations_sha256: String,
}

impl Parent {
    fn of(state: &State) -> Self {
        Self {
            schema: 1,
            index_sha256: state.index_sha256.clone(),
            roots_sha256: state.roots_sha256.clone(),
            revocations_sha256: state.revocations_sha256.clone(),
        }
    }
    fn matches(&self, state: &State) -> bool {
        self.schema == 1
            && self.index_sha256 == state.index_sha256
            && self.roots_sha256 == state.roots_sha256
            && self.revocations_sha256 == state.revocations_sha256
    }
}

struct Tree {
    path: PathBuf,
    docs: BTreeMap<String, Vec<u8>>,
    assets: BTreeMap<String, Asset>,
    state: State,
    index: Index,
    revocations: Revocations,
}

fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn directory(path: &Path) -> Result<PathBuf> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        bail!("Serving directory must be real, not linked");
    }
    Ok(path.canonicalize()?)
}

fn expiry(expires: &str, require_fresh: bool) -> Result<()> {
    let expires = DateTime::parse_from_rfc3339(expires)?.timestamp();
    if require_fresh && expires <= Utc::now().timestamp() {
        bail!("Serving metadata must be unexpired; no seed or clock-skew exemption");
    }
    Ok(())
}

fn app_anchor(bytes: &[u8], sig: &str) -> Result<Roots> {
    Ok(grain_core::trust::verify_roots(bytes, sig)?)
}

fn documents(path: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    DOCS.into_iter()
        .map(|name| {
            let max = if name.ends_with(".minisig") {
                8192
            } else if name == "roots.json" {
                64 * 1024
            } else {
                4 * 1024 * 1024
            };
            Ok((name.to_owned(), crate::review::read(&path.join(name), max)?))
        })
        .collect()
}

fn asset(
    assets: &mut BTreeMap<String, Asset>,
    folder: &str,
    hash: &str,
    ext: &str,
    size: u64,
    max: u64,
) -> Result<()> {
    if !hex(hash) || size > max {
        bail!("Invalid serving asset digest/size");
    }
    let name = format!("{folder}/{hash}.{ext}");
    let value = Asset {
        hash: hash.into(),
        size: (size != 0).then_some(size),
        max,
    };
    if let Some(old) = assets.get(&name) {
        if old.size.is_some() && value.size.is_some() && old.size != value.size {
            bail!("Conflicting sizes for one asset");
        }
        if value.size.is_none() {
            return Ok(());
        }
    }
    assets.insert(name, value);
    if assets.len() > MAX_FILES {
        bail!("Serving snapshot exceeds file budget");
    }
    Ok(())
}

// Stream one file and, optionally, the exact same bytes into an owned staging file.
// Never verify a pathname and later copy unchecked replacement bytes from it.
fn stream(path: &Path, value: &Asset, out: Option<&Path>) -> Result<u64> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() == 0 || meta.len() > value.max
    {
        bail!("Missing, linked or oversized serving asset");
    }
    let mut input = fs::File::open(path)?.take(value.max + 1);
    let mut output = out.map(fs::File::create_new).transpose()?;
    let mut sha = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > value.max {
            bail!("Serving asset grew beyond its budget");
        }
        sha.update(&buffer[..n]);
        if let Some(file) = &mut output {
            file.write_all(&buffer[..n])?;
        }
    }
    if value.size.is_some_and(|expected| expected != size)
        || format!("{:x}", sha.finalize()) != value.hash
    {
        bail!("Serving asset bytes differ from signed hash/size");
    }
    if let Some(file) = output {
        file.sync_all()?;
    }
    Ok(size)
}

fn inventory(path: &Path, allowed: &BTreeSet<String>, folders: bool) -> Result<()> {
    for item in fs::read_dir(path)? {
        let item = item?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Non-UTF8 serving path"))?;
        if !allowed.contains(&name) || item.file_type()?.is_symlink() {
            bail!("Unexpected or linked serving input");
        }
        if folders {
            directory(&item.path())?;
        } else if !item.file_type()?.is_file() {
            bail!("Serving input must be a regular file");
        }
    }
    Ok(())
}

fn metadata(path: &Path, anchor: &Anchor<'_>, require_fresh: bool) -> Result<Tree> {
    let path = directory(path)?;
    let docs = documents(&path)?;
    let roots = anchor(
        &docs["roots.json"],
        std::str::from_utf8(&docs["roots.json.minisig"])?,
    )?;
    if roots.spec != 1 || roots.version == 0 {
        bail!("Unsupported serving roots version/spec");
    }
    if let Some(expires) = &roots.expires {
        expiry(expires, require_fresh)?;
    }
    let (index, status) = grain_core::trust::verify_index(
        &roots,
        &docs["index.json"],
        std::str::from_utf8(&docs["index.json.minisig"])?,
        None,
        Utc::now().timestamp(),
        false,
    )?;
    let revocations = grain_core::trust::verify_revocations(
        &roots,
        &docs["revocations.json"],
        std::str::from_utf8(&docs["revocations.json.minisig"])?,
    )?;
    if index.spec != 1
        || index.version == 0
        || (require_fresh && status != grain_core::trust::IndexStatus::Fresh)
        || revocations.spec != 1
        || revocations.version == 0
    {
        bail!("Unsupported serving index/revocation version/spec");
    }
    expiry(&index.expires, require_fresh)?;
    expiry(&revocations.expires, require_fresh)?;
    let mut identities = BTreeSet::new();
    let mut assets = BTreeMap::new();
    for entry in &index.entries {
        grain_sdk::validate_extension_id(&entry.id).map_err(anyhow::Error::msg)?;
        grain_sdk::validate_extension_version(&entry.version).map_err(anyhow::Error::msg)?;
        if !identities.insert((&entry.id, &entry.version)) {
            bail!("Duplicate serving extension version");
        }
        if matches!(
            entry.trust,
            grain_sdk::Trust::Dev | grain_sdk::Trust::Experimental
        ) {
            bail!("Unpublishable serving trust level");
        }
        entry.validate_listing().map_err(anyhow::Error::msg)?;
        let (ext, max) = match entry.artifact_kind {
            ArtifactKind::Native => ("grainpack", grain_sdk::PACK_MAX_BYTES),
            ArtifactKind::McpDescriptor => {
                ("mcp.json", grain_sdk::mcp::MCP_DESCRIPTOR_MAX_BYTES as u64)
            }
        };
        asset(&mut assets, "blob", &entry.sha256, ext, entry.size, max)?;
        let hash = entry.detail_document_hash();
        if !hash.is_empty() {
            asset(
                &mut assets,
                "media",
                hash,
                "md",
                entry.listing.as_ref().map_or(0, |v| v.size),
                64 * 1024,
            )?;
        }
        for media in &entry.media {
            if !matches!(media.kind.as_str(), "webp" | "gif") {
                bail!("Unsupported serving media kind");
            }
            asset(
                &mut assets,
                "media",
                &media.sha256,
                &media.kind,
                media.size,
                4 * 1024 * 1024,
            )?;
        }
    }
    for entry in &revocations.entries {
        grain_sdk::validate_extension_id(&entry.id).map_err(anyhow::Error::msg)?;
        if let Some(v) = &entry.version {
            grain_sdk::validate_extension_version(v).map_err(anyhow::Error::msg)?;
        }
    }
    let mut hashes = docs
        .iter()
        .map(|(name, bytes)| (name.clone(), digest(bytes)))
        .collect::<BTreeMap<_, _>>();
    hashes.extend(
        assets
            .iter()
            .map(|(name, value)| (name.clone(), value.hash.clone())),
    );
    let state = State {
        schema: 1,
        snapshot: digest(&serde_json::to_vec(&hashes)?),
        index_sha256: digest(&docs["index.json"]),
        index_version: index.version,
        roots_sha256: digest(&docs["roots.json"]),
        roots_version: roots.version,
        revocations_sha256: digest(&docs["revocations.json"]),
        revocations_version: revocations.version,
    };
    Ok(Tree {
        path,
        docs,
        assets,
        state,
        index,
        revocations,
    })
}

fn check(path: &Path, anchor: &Anchor<'_>, require_fresh: bool) -> Result<Tree> {
    let tree = metadata(path, anchor, require_fresh)?;
    let path = &tree.path;
    let mut top = DOCS.iter().map(|v| v.to_string()).collect::<BTreeSet<_>>();
    top.extend(["blob".into(), "media".into()]);
    // Metadata files were already checked; distinguish the two asset directories.
    for item in fs::read_dir(path)? {
        let item = item?;
        if !top.contains(item.file_name().to_str().unwrap_or("")) || item.file_type()?.is_symlink()
        {
            bail!("Unexpected serving-tree root input");
        }
    }
    let mut total: u64 = tree.docs.values().map(|v| v.len() as u64).sum();
    for folder in ["blob", "media"] {
        let dir = directory(&path.join(folder))?;
        let allowed = tree
            .assets
            .keys()
            .filter_map(|v| v.strip_prefix(&format!("{folder}/")).map(str::to_owned))
            .collect();
        inventory(&dir, &allowed, false)?;
    }
    for (name, value) in &tree.assets {
        total += stream(&path.join(name), value, None)?;
        if total > MAX_TOTAL {
            bail!("Serving snapshot exceeds total byte budget");
        }
    }
    Ok(tree)
}

fn immutable_versions(old: &Index, new: &Index) -> Result<()> {
    let published = new
        .entries
        .iter()
        .map(|v| ((v.id.as_str(), v.version.as_str()), v))
        .collect::<BTreeMap<_, _>>();
    for entry in &old.entries {
        if let Some(next) = published.get(&(entry.id.as_str(), entry.version.as_str())) {
            if next.sha256 != entry.sha256
                || next.artifact_kind != entry.artifact_kind
                || next.detail_document_hash() != entry.detail_document_hash()
                || !next
                    .media
                    .iter()
                    .map(|m| (&m.sha256, &m.kind))
                    .eq(entry.media.iter().map(|m| (&m.sha256, &m.kind)))
            {
                bail!("Published extension version cannot change artifact/listing bytes");
            }
        }
    }
    Ok(())
}

fn transition(old: &Tree, new: &Tree, require_change: bool) -> Result<()> {
    for (old_version, new_version, old_hash, new_hash) in [
        (
            old.state.index_version,
            new.state.index_version,
            &old.state.index_sha256,
            &new.state.index_sha256,
        ),
        (
            old.state.roots_version,
            new.state.roots_version,
            &old.state.roots_sha256,
            &new.state.roots_sha256,
        ),
        (
            old.state.revocations_version,
            new.state.revocations_version,
            &old.state.revocations_sha256,
            &new.state.revocations_sha256,
        ),
    ] {
        if new_version < old_version || (new_version == old_version && old_hash != new_hash) {
            bail!("Serving metadata rollback or same-version replacement");
        }
    }
    immutable_versions(&old.index, &new.index)?;
    let strength = |state| {
        if state == RevocationState::Revoked {
            2u8
        } else {
            1u8
        }
    };
    let mut rules = BTreeMap::new();
    for entry in &new.revocations.entries {
        rules
            .entry((entry.id.as_str(), entry.version.as_deref()))
            .and_modify(|v: &mut u8| *v = (*v).max(strength(entry.state)))
            .or_insert(strength(entry.state));
    }
    for entry in &old.revocations.entries {
        let required = strength(entry.state);
        let retained = rules
            .get(&(entry.id.as_str(), None))
            .copied()
            .unwrap_or(0)
            .max(
                rules
                    .get(&(entry.id.as_str(), entry.version.as_deref()))
                    .copied()
                    .unwrap_or(0),
            );
        if retained < required {
            bail!("Promotion cannot erase or weaken a prior revocation");
        }
    }
    if require_change && Parent::of(&old.state).matches(&new.state) {
        bail!("No metadata change to promote");
    }
    Ok(())
}

fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::File::create_new(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn copy(tree: &Tree, out: &Path) -> Result<()> {
    fs::create_dir(out)?;
    fs::create_dir(out.join("blob"))?;
    fs::create_dir(out.join("media"))?;
    for (name, bytes) in &tree.docs {
        write(&out.join(name), bytes)?;
    }
    for (name, asset) in &tree.assets {
        stream(&tree.path.join(name), asset, Some(&out.join(name)))?;
    }
    sync_dir(&out.join("blob"))?;
    sync_dir(&out.join("media"))?;
    sync_dir(out)
}

pub(super) fn verify(path: &Path) -> Result<()> {
    let tree = check(path, &app_anchor, true)?;
    println!(
        "Complete pinned-root serving tree verified: index {}, snapshot {}",
        tree.state.index_version, tree.state.snapshot
    );
    Ok(())
}

pub(super) fn renew(v1: &Path, pin: &str, key: &Path, days: u32, out: &Path) -> Result<()> {
    renewal::renew_with(v1, pin, key, days, out, &app_anchor)
}

pub(super) fn assemble(base: &Path, update: &Path, out: &Path) -> Result<()> {
    assemble_with(base, update, out, &app_anchor)
}

fn assemble_with(base: &Path, update: &Path, out: &Path, anchor: &Anchor<'_>) -> Result<()> {
    let base = check(base, anchor, false)?;
    let update = directory(update)?;
    let out = new_output(out, &[&base.path, &update])?;
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    let work = tempfile::tempdir_in(&out)?;
    let merged = work.path().join("v1");
    fs::create_dir(&merged)?;
    for folder in ["blob", "media"] {
        fs::create_dir(merged.join(folder))?;
    }
    // Only known signed documents and content-addressed files may enter merging.
    let allowed = DOCS
        .iter()
        .map(|s| s.to_string())
        .chain(["blob".into(), "media".into()])
        .collect::<BTreeSet<_>>();
    for file in fs::read_dir(&update)? {
        let file = file?;
        if !allowed.contains(file.file_name().to_str().unwrap_or(""))
            || file.file_type()?.is_symlink()
        {
            bail!("Unexpected update-fragment input");
        }
    }
    for name in DOCS {
        let src = update.join(name);
        let bytes = if src.try_exists()? {
            crate::review::read(
                &src,
                if name.ends_with(".minisig") {
                    8192
                } else {
                    4 * 1024 * 1024
                },
            )?
        } else {
            base.docs[name].clone()
        };
        write(&merged.join(name), &bytes)?;
    }
    let next = metadata(&merged, anchor, true)?;
    transition(&base, &next, false)?;
    // Only files referenced by the authenticated next catalogue are copied.
    // Withdrawn assets survive in the old immutable snapshot, never deleted here.
    for folder in ["blob", "media"] {
        let dir = update.join(folder);
        if !dir.try_exists()? {
            continue;
        }
        directory(&dir)?;
        let allowed = next
            .assets
            .keys()
            .filter_map(|v| v.strip_prefix(&format!("{folder}/")).map(str::to_owned))
            .collect();
        inventory(&dir, &allowed, false)?;
    }
    let mut total: u64 = next.docs.values().map(|v| v.len() as u64).sum();
    for (name, value) in &next.assets {
        let supplied = update.join(name);
        let source = if supplied.try_exists()? {
            supplied
        } else {
            base.path.join(name)
        };
        total += stream(&source, value, Some(&merged.join(name)))?;
        if total > MAX_TOTAL {
            bail!("Serving snapshot exceeds total byte budget");
        }
    }
    check(&merged, anchor, true)?;
    fs::rename(&merged, out.join("v1"))?;
    write(
        &out.join("promotion.json"),
        &serde_json::to_vec(&Parent::of(&base.state))?,
    )?;
    drop(work);
    sync_dir(&out)?;
    owned.1 = true;
    println!(
        "Complete serving assembly at {}; unsigned parent record is not approval",
        out.display()
    );
    Ok(())
}

fn lock(store: &Path) -> Result<fs::File> {
    let path = store.join("promotion.lock");
    if let Ok(meta) = fs::symlink_metadata(&path) {
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != 0 {
            bail!("Invalid serving lock file");
        }
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock().map_err(|_| {
        anyhow::anyhow!("Another promotion owns the store lock, or locking is unavailable")
    })?;
    Ok(file) // Released on drop. Never unlink a lock inode that another caller may hold.
}

fn pointer(store: &Path, state: &State, initial: bool) -> Result<String> {
    let bytes = serde_json::to_vec(state)?;
    let mut file = tempfile::NamedTempFile::new_in(store)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    if initial {
        file.persist_noclobber(store.join("current.json"))
            .map_err(|e| e.error)?;
    } else {
        file.persist(store.join("current.json"))
            .map_err(|e| e.error)?;
    }
    sync_dir(store)
        .context("Pointer committed; directory sync failed: inspect current.json before retry")?;
    Ok(digest(&bytes))
}

fn install(tree: &Tree, store: &Path, anchor: &Anchor<'_>) -> Result<()> {
    let snapshots = directory(&store.join("snapshots"))?;
    let final_path = snapshots.join(&tree.state.snapshot);
    if final_path.try_exists()? {
        if check(&final_path, anchor, true)?.state != tree.state {
            bail!("Existing immutable snapshot differs");
        }
        return Ok(());
    }
    let stage = tempfile::tempdir_in(&snapshots)?;
    let v1 = stage.path().join("v1");
    copy(tree, &v1)?;
    if check(&v1, anchor, true)?.state != tree.state {
        bail!("Copied serving snapshot differs");
    }
    fs::rename(v1, final_path)?;
    drop(stage);
    sync_dir(&snapshots)?;
    Ok(()) // Crash leftovers remain unreferenced until a verified pointer commit.
}

pub(super) fn initialize(v1: &Path, out: &Path) -> Result<()> {
    initialize_with(v1, out, &app_anchor)
}

fn initialize_with(v1: &Path, out: &Path, anchor: &Anchor<'_>) -> Result<()> {
    let tree = check(v1, anchor, true)?;
    let out = new_output(out, &[&tree.path])?;
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    let _lock = lock(&out)?;
    fs::create_dir(out.join("snapshots"))?;
    install(&tree, &out, anchor)?;
    let pin = pointer(&out, &tree.state, true)?;
    owned.1 = true;
    println!("Local serving store initialized; current SHA256 {pin}; not hosted");
    Ok(())
}

pub(super) fn promote(assembly: &Path, store: &Path, pin: &str) -> Result<()> {
    promote_with(assembly, store, pin, &app_anchor)
}

// The operator-owned store is append-only. Reserve identities even in a fully
// verified but not yet selected snapshot: after an interrupted operation there
// may already be external readers. No mutable second history database is needed.
fn visit_history(
    store: &Path,
    anchor: &Anchor<'_>,
    mut visit: impl FnMut(Tree) -> Result<()>,
) -> Result<usize> {
    let mut count = 0;
    let mut bytes = 0u64;
    let snapshots = directory(&store.join("snapshots"))?;
    for item in fs::read_dir(&snapshots)? {
        let item = item?;
        let name = item.file_name();
        let name = name.to_str().context("Non-UTF8 snapshot history path")?;
        directory(&item.path())?;
        if name.starts_with(".tmp") {
            // tempfile staging left by a crash was never installed. Do not read
            // its unsigned bytes, select it, reserve its versions or delete it.
            continue;
        }
        if !hex(name) {
            bail!("Unexpected snapshot history path");
        }
        count += 1;
        if count > MAX_HISTORY {
            bail!("Snapshot history exceeds count budget; audited archival required");
        }
        // Authenticate one bounded catalogue at a time; do not reread all old
        // packages or retain historical catalogues in memory.
        let historical = metadata(&item.path(), anchor, false)?;
        if historical.state.snapshot != name {
            bail!("Snapshot history identity differs from its signed metadata");
        }
        bytes += historical
            .docs
            .values()
            .map(|v| v.len() as u64)
            .sum::<u64>();
        if bytes > MAX_HISTORY_DOCS {
            bail!("Snapshot history exceeds metadata budget; audited archival required");
        }
        visit(historical)?;
    }
    Ok(count)
}

fn historical_versions(store: &Path, next: &Tree, anchor: &Anchor<'_>) -> Result<()> {
    let count = visit_history(store, anchor, |historical| {
        immutable_versions(&historical.index, &next.index)
    })?;
    let snapshots = directory(&store.join("snapshots"))?;
    if count == MAX_HISTORY && !snapshots.join(&next.state.snapshot).try_exists()? {
        bail!("Snapshot history count budget exhausted; audited archival required");
    }
    Ok(())
}

pub(super) fn export_hosting_bundle(store: &Path, out: &Path, pin: &str) -> Result<()> {
    hosting::export(store, out, pin, &app_anchor)
}

pub(super) fn verify_hosting_bundle(bundle: &Path, pin: &str) -> Result<()> {
    hosting::verify(bundle, pin, &app_anchor)?;
    println!(
        "Hosting bundle verified against pinned receipt and app roots; not deployment approval"
    );
    Ok(())
}

fn selected(store: &Path, pin: &str, anchor: &Anchor<'_>, fresh: bool) -> Result<Tree> {
    if !hex(pin) {
        bail!("Independent current-pointer digest required");
    }
    let raw = crate::review::read(&store.join("current.json"), 8192)?;
    if digest(&raw) != pin {
        bail!("Current serving pointer changed; rebase before promotion/export");
    }
    let state: State = serde_json::from_slice(&raw)?;
    if state.schema != 1 || !hex(&state.snapshot) {
        bail!("Malformed serving pointer");
    }
    let tree = check(
        &directory(&store.join("snapshots"))?.join(&state.snapshot),
        anchor,
        fresh,
    )?;
    if tree.state != state {
        bail!("Current serving snapshot mismatch");
    }
    Ok(tree)
}

pub(super) fn export(store: &Path, out: &Path, pin: &str) -> Result<()> {
    export_with(store, out, pin, &app_anchor)
}

fn export_with(store: &Path, out: &Path, pin: &str, anchor: &Anchor<'_>) -> Result<()> {
    let store = directory(store)?;
    let out = new_output(out, &[&store])?;
    let _lock = lock(&store)?;
    let tree = selected(&store, pin, anchor, true)?;
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    copy(&tree, &out.join("v1"))?;
    if check(&out.join("v1"), anchor, true)?.state != tree.state {
        bail!("Exported serving snapshot differs");
    }
    write(
        &out.join("snapshot.json"),
        &serde_json::to_vec(&serde_json::json!({
            "schema": 1, "current_sha256": pin, "selected": tree.state
        }))?,
    )?;
    sync_dir(&out)?;
    owned.1 = true;
    println!("Verified selected snapshot exported; unsigned receipt is not approval; hosting not changed");
    Ok(())
}

fn promote_with(assembly: &Path, store: &Path, pin: &str, anchor: &Anchor<'_>) -> Result<()> {
    if !hex(pin) {
        bail!("Independent current-pointer digest required");
    }
    let assembly = directory(assembly)?;
    let store = directory(store)?;
    if assembly.starts_with(&store) || store.starts_with(&assembly) {
        bail!("Assembly must be outside the serving store");
    }
    let allowed = BTreeSet::from(["promotion.json", "v1"]);
    for item in fs::read_dir(&assembly)? {
        let item = item?;
        if !allowed.contains(item.file_name().to_str().unwrap_or(""))
            || item.file_type()?.is_symlink()
        {
            bail!("Unexpected assembly input");
        }
    }
    let parent: Parent = serde_json::from_slice(&crate::review::read(
        &assembly.join("promotion.json"),
        8192,
    )?)?;
    let next = check(&assembly.join("v1"), anchor, true)?;
    let _lock = lock(&store)?;
    let old = selected(&store, pin, anchor, false)?;
    if !parent.matches(&old.state) {
        bail!("Serving assembly parent or current snapshot mismatch");
    }
    transition(&old, &next, true)?;
    historical_versions(&store, &next, anchor)?;
    install(&next, &store, anchor)?;
    // All store writers must honor the lock and leave installed snapshots intact.
    // This operator-owned local store is not a sandbox against external mutation.
    let pin = pointer(&store, &next.state, false)?;
    println!("Local serving pointer promoted; current SHA256 {pin}; hosting not changed");
    Ok(())
}

#[cfg(test)]
#[path = "serving_tests.rs"]
mod tests;
