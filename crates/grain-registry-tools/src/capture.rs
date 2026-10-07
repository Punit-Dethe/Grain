//! Capture previous publication proof from pinned Git objects, never checkout
//! bytes. No legacy trust exemption, author execution, signing or deployment.
use super::*;
use std::io::BufReader;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema: u8,
    pub evidence_class: String,
    pub repository: String,
    pub commit: String,
    pub bundle_receipt_sha256: String,
    pub snapshot: String,
    pub files: usize,
    pub bytes: u64,
}

pub(super) struct Blob {
    pub(super) oid: String,
    pub(super) size: u64,
}

fn document_limit(name: &str) -> Option<u64> {
    if !DOCS.contains(&name) {
        return None;
    }
    Some(if name.ends_with(".minisig") {
        8192
    } else if name == "roots.json" {
        64 * 1024
    } else {
        4 * 1024 * 1024
    })
}

fn addressed_limit(name: &str, kinds: &[(&str, u64)]) -> Option<u64> {
    kinds.iter().find_map(|(suffix, limit)| {
        name.strip_suffix(suffix)
            .filter(|hash| hex(hash))
            .map(|_| *limit)
    })
}

// Every path is an allowlisted relative file before any directories are made.
// Git empty directories are absent; reconstruction creates only known folders.
pub(super) fn destination(name: &str) -> Result<(PathBuf, u64)> {
    let parts: Vec<_> = name.split('/').collect();
    let limit = match parts.as_slice() {
        ["v1", doc] => document_limit(doc),
        ["v1", "blob", file] => addressed_limit(
            file,
            &[
                (".grainpack", grain_sdk::PACK_MAX_BYTES),
                (".mcp.json", grain_sdk::mcp::MCP_DESCRIPTOR_MAX_BYTES as u64),
            ],
        ),
        ["v1", "media", file] => addressed_limit(
            file,
            &[
                (".md", 64 * 1024),
                (".webp", 4 * 1024 * 1024),
                (".gif", 4 * 1024 * 1024),
            ],
        ),
        [publication::PROOFS, "bundle.json"] => Some(384 * 1024),
        [publication::PROOFS, "current.json"] => Some(8192),
        [publication::PROOFS, "history", id, doc] if hex(id) => document_limit(doc),
        _ => None,
    }
    .context("Unexpected committed publication path")?;
    let relative = name
        .strip_prefix(&format!("{}/", publication::PROOFS))
        .unwrap_or(name);
    Ok((PathBuf::from(relative), limit))
}

fn inventory(root: &Path, commit: &str) -> Result<BTreeMap<PathBuf, Blob>> {
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
            publication::PROOFS,
        ],
        false,
    )?;
    let mut blobs = BTreeMap::new();
    let mut bytes = 0_u64;
    for record in listing.split('\0').filter(|v| !v.is_empty()) {
        let (header, name) = record
            .split_once('\t')
            .context("Malformed publication Git inventory")?;
        let fields: Vec<_> = header.split_ascii_whitespace().collect();
        if fields.len() != 4
            || fields[0] != "100644"
            || fields[1] != "blob"
            || !publication::oid(fields[2])
        {
            bail!("Publication capture requires regular non-executable Git blobs");
        }
        let (path, limit) = destination(name)?;
        let size: u64 = fields[3].parse()?;
        if size == 0 || size > limit {
            bail!("Committed publication file exceeds its byte bound");
        }
        bytes = bytes
            .checked_add(size)
            .context("Publication byte counter exhausted")?;
        if bytes > MAX_TOTAL {
            bail!("Publication capture exceeds total byte budget");
        }
        if blobs
            .insert(
                path,
                Blob {
                    oid: fields[2].into(),
                    size,
                },
            )
            .is_some()
        {
            bail!("Duplicate committed publication path");
        }
        if blobs.len() > MAX_FILES + MAX_HISTORY * DOCS.len() + 8 {
            bail!("Publication capture exceeds file budget");
        }
    }
    // The old four-document registry is not a complete publication baseline.
    for name in DOCS
        .into_iter()
        .map(|doc| format!("v1/{doc}"))
        .chain(["bundle.json".into(), "current.json".into()])
    {
        if !blobs.contains_key(Path::new(&name)) {
            bail!("Incomplete publication baseline: missing {name}; complete signed publication proof required");
        }
    }
    Ok(blobs)
}

fn header(reader: &mut impl Read) -> Result<String> {
    let mut bytes = Vec::with_capacity(96);
    loop {
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            return String::from_utf8(bytes).context("Non-UTF8 Git batch header");
        }
        bytes.push(byte[0]);
        if bytes.len() > 128 {
            bail!("Git batch header exceeds bound");
        }
    }
}

pub(super) fn materialize(
    root: &Path,
    blobs: &BTreeMap<PathBuf, Blob>,
    bundle: &Path,
) -> Result<u64> {
    let mut input = Vec::with_capacity(blobs.len() * 41);
    for blob in blobs.values() {
        input.extend(blob.oid.as_bytes());
        input.push(b'\n');
    }
    let file = crate::prepare::git_file(
        root,
        &["cat-file", "--batch"],
        false,
        Some(&input),
        MAX_TOTAL + publication::GIT_MAX,
    )?;
    let mut reader = BufReader::new(file);
    let mut total = 0;
    let mut directories = BTreeSet::new();
    for (path, blob) in blobs {
        let line = header(&mut reader)?;
        if line != format!("{} blob {}", blob.oid, blob.size) {
            bail!("Git batch result differs from pinned publication inventory");
        }
        let destination = bundle.join(path);
        let parent = destination
            .parent()
            .context("Missing publication destination parent")?;
        fs::create_dir_all(parent)?;
        directories.insert(parent.to_owned());
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        // Read exactly the declared bytes. Binary artifacts may contain newlines,
        // NULs or strings that resemble Git protocol headers.
        let copied = std::io::copy(&mut reader.by_ref().take(blob.size), &mut output)?;
        if copied != blob.size {
            bail!("Truncated Git publication blob");
        }
        let mut separator = [0];
        reader.read_exact(&mut separator)?;
        if separator[0] != b'\n' {
            bail!("Malformed Git batch blob separator");
        }
        output.sync_all()?;
        total += copied;
    }
    let mut extra = [0];
    if reader.read(&mut extra)? != 0 {
        bail!("Unexpected trailing Git batch output");
    }
    for dir in directories.iter().rev() {
        sync_dir(dir)?;
    }
    Ok(total)
}

pub(crate) fn capture(
    root: &Path,
    repository: &str,
    commit: &str,
    pin: &str,
    out: &Path,
) -> Result<()> {
    capture_with(root, repository, commit, pin, out, &app_anchor)
}

pub(super) fn capture_with(
    root: &Path,
    repository: &str,
    commit: &str,
    pin: &str,
    out: &Path,
    anchor: &Anchor<'_>,
) -> Result<()> {
    let url = publication::repository_url(repository)?;
    if !publication::oid(commit) {
        bail!("Independent full Git SHA1 capture commit required");
    }
    if !hex(pin) {
        bail!("Independent hosting receipt digest required");
    }
    let root = directory(root)?;
    let out = new_output(out, &[&root])?;
    publication::checkout(&root, &url, None)?;
    if publication::git(&root, &["cat-file", "-t", commit], false)?.trim() != "commit" {
        bail!("Pinned capture object must be an actual Git commit");
    }
    let blobs = inventory(&root, commit)?;
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    let bundle = out.join("bundle");
    for dir in [
        &bundle,
        &bundle.join("v1"),
        &bundle.join("v1/blob"),
        &bundle.join("v1/media"),
        &bundle.join("history"),
    ] {
        fs::create_dir(dir)?;
    }
    let bytes = materialize(&root, &blobs, &bundle)?;
    // Expired authentic previous proof is recoverable, never made active here.
    // Publishing/ordinary hosting verification independently requires freshness.
    let selected = hosting::inspect(&bundle, pin, anchor, false)?.selected;
    publication::committed(&root, commit, &bundle)?;
    let receipt = Receipt {
        schema: 1,
        evidence_class: "authenticated-previous-publication-capture-not-release-approval".into(),
        repository: repository.into(),
        commit: commit.into(),
        bundle_receipt_sha256: pin.into(),
        snapshot: selected.snapshot,
        files: blobs.len(),
        bytes,
    };
    let raw = serde_json::to_vec_pretty(&receipt)?;
    write(&out.join("capture.json"), &raw)?;
    for dir in [&bundle.join("v1"), &bundle.join("history"), &bundle, &out] {
        sync_dir(dir)?;
    }
    owned.1 = true;
    println!("Authenticated previous publication captured; handoff SHA256 {}; no selection, trust or remote changed", digest(&raw));
    Ok(())
}
