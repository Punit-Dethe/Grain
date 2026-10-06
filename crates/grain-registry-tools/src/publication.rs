//! Offline GitHub publication gate. Produces a conditional push handoff, never
//! fetches, signs, changes the checkout, authenticates or updates a remote.
use super::*;

pub(super) const GIT_MAX: u64 = 8 * 1024 * 1024;
pub(super) const PROOFS: &str = ".registry-publication";

pub(crate) struct Request<'a> {
    pub checkout: &'a Path,
    pub repository: &'a str,
    pub branch: &'a str,
    pub base_commit: &'a str,
    pub candidate_commit: &'a str,
    pub previous: &'a Path,
    pub previous_pin: &'a str,
    pub bundle: &'a Path,
    pub bundle_pin: &'a str,
    pub out: &'a Path,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Handoff {
    pub schema: u8,
    pub evidence_class: String,
    pub repository: String,
    pub branch: String,
    pub expected_base_commit: String,
    pub candidate_commit: String,
    pub previous_receipt_sha256: String,
    pub candidate_receipt_sha256: String,
    pub snapshot: String,
    pub push_args: Vec<String>,
    pub remove_environment_prefix: String,
    pub environment: BTreeMap<String, String>,
}

pub(super) fn oid(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn repository_url(repository: &str) -> Result<String> {
    let parts: Vec<_> = repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || part.starts_with(['.', '-'])
                || part.ends_with(['.', '-'])
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
        || repository.ends_with(".git")
    {
        bail!("Expected GitHub repository must be OWNER/REPO");
    }
    Ok(format!("https://github.com/{repository}.git"))
}

pub(super) fn git(root: &Path, args: &[&str], absent: bool) -> Result<String> {
    crate::prepare::git_input(root, args, absent, None, GIT_MAX)
}

pub(super) fn checkout(root: &Path, url: &str, expected_head: Option<&str>) -> Result<()> {
    directory(&root.join(".git"))?;
    crate::review::read(&root.join(".git/config"), 256 * 1024)?;
    for name in ["shallow", "info/grafts", "objects/info/alternates"] {
        if fs::symlink_metadata(root.join(".git").join(name)).is_ok() {
            bail!("Publication requires complete unmodified local Git ancestry");
        }
    }
    if !git(root, &[
        "config", "--local", "--no-includes", "--get-regexp",
        "^(include\\.|includeif\\.|extensions\\.|url\\.|filter\\.|remote\\..*\\.(promisor|pushurl)$)",
    ], true)?.is_empty() {
        bail!("Publication Git config must not redirect, include, filter or borrow objects");
    }
    if Path::new(git(root, &["rev-parse", "--show-toplevel"], false)?.trim()).canonicalize()?
        != root
    {
        bail!("Publication requires the standalone repository root");
    }
    let origin = git(
        root,
        &["config", "--local", "--get-all", "remote.origin.url"],
        false,
    )?;
    if origin.trim() != url && origin.trim() != url.trim_end_matches(".git") {
        bail!("Publication origin differs from independently expected GitHub repository");
    }
    if let Some(candidate) = expected_head {
        if git(root, &["rev-parse", "--verify", "HEAD^{commit}"], false)?.trim() != candidate {
            bail!("Publication HEAD differs from pinned candidate commit");
        }
    }
    Ok(())
}

// Hosting verification has already restricted this inventory to exact signed
// documents, content-addressed files and bounded history proofs. Do not scan
// unrelated checkout files or rely on its index/worktree/attributes for bytes.
fn files(bundle: &Path) -> Result<BTreeMap<String, PathBuf>> {
    fn visit(dir: &Path, prefix: &str, out: &mut BTreeMap<String, PathBuf>) -> Result<()> {
        if prefix.split('/').count() > 5 {
            bail!("Unexpected publication directory depth");
        }
        for item in fs::read_dir(dir)? {
            let item = item?;
            let name = item
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 publication path"))?;
            let name = format!("{prefix}/{name}");
            let kind = item.file_type()?;
            if kind.is_dir() {
                visit(&item.path(), &name, out)?;
            } else if kind.is_file() {
                out.insert(name, item.path());
                if out.len()
                    > MAX_FILES * 2 + MAX_HISTORY * DOCS.len() + legacy::MAX_COMMITS * 4 + 10
                {
                    bail!("Publication file inventory exceeds budget");
                }
            } else {
                bail!("Linked or special publication input");
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::from([
        (format!("{PROOFS}/bundle.json"), bundle.join("bundle.json")),
        (
            format!("{PROOFS}/current.json"),
            bundle.join("current.json"),
        ),
    ]);
    visit(&bundle.join("v1"), "v1", &mut out)?;
    visit(
        &bundle.join("history"),
        &format!("{PROOFS}/history"),
        &mut out,
    )?;
    if bundle.join("legacy").try_exists()? {
        visit(
            &bundle.join("legacy"),
            &format!("{PROOFS}/legacy"),
            &mut out,
        )?;
    }
    Ok(out)
}

pub(super) fn committed(root: &Path, commit: &str, bundle: &Path) -> Result<()> {
    let expected = files(bundle)?;
    let listing = git(
        root,
        &[
            "ls-tree",
            "-r",
            "--full-tree",
            "-z",
            commit,
            "--",
            "v1",
            PROOFS,
        ],
        false,
    )?;
    let mut actual = BTreeMap::new();
    for record in listing.split('\0').filter(|r| !r.is_empty()) {
        let (header, path) = record
            .split_once('\t')
            .context("Malformed Git tree entry")?;
        let fields: Vec<_> = header.split(' ').collect();
        if fields.len() != 3
            || fields[0] != "100644"
            || fields[1] != "blob"
            || !oid(fields[2])
            || actual
                .insert(path.to_owned(), fields[2].to_owned())
                .is_some()
        {
            bail!("Publication requires unique regular non-executable Git blobs");
        }
    }
    if actual.keys().ne(expected.keys()) {
        bail!("Committed publication inventory differs from verified hosting bundle");
    }
    let mut input = Vec::new();
    for path in expected.values() {
        let path = path.to_str().context("Non-UTF8 operator bundle path")?;
        if path.chars().any(char::is_control) {
            bail!("Control character in operator bundle path");
        }
        let path = if cfg!(windows) {
            // canonicalize returns Win32 extended paths; Git's stdin path
            // parser expects drive/UNC names, not //?/C:/... spellings.
            let path = path.strip_prefix("\\\\?\\").unwrap_or(path);
            if let Some(unc) = path.strip_prefix("UNC\\") {
                format!("//{}", unc.replace('\\', "/"))
            } else {
                path.replace('\\', "/")
            }
        } else {
            path.to_owned()
        };
        input.extend(serde_json::to_vec(&path)?);
        input.push(b'\n');
        if input.len() as u64 > GIT_MAX {
            bail!("Publication hashing input exceeds budget");
        }
    }
    let hashes = crate::prepare::git_input(
        root,
        &["hash-object", "--no-filters", "--stdin-paths"],
        false,
        Some(&input),
        GIT_MAX,
    )
    .context("Hash verified publication files without filters")?;
    if hashes.lines().count() != actual.len() {
        bail!("Committed publication hashing count differs from inventory");
    }
    for ((path, expected), actual) in actual.iter().zip(hashes.lines()) {
        if expected != actual {
            bail!("Committed publication bytes differ from verified hosting bundle: {path}");
        }
    }
    Ok(())
}

pub(crate) fn prepare(request: &Request<'_>) -> Result<()> {
    prepare_with(request, &app_anchor)
}

pub(super) fn prepare_with(r: &Request<'_>, anchor: &Anchor<'_>) -> Result<()> {
    let (url, root) = candidate(r)?;
    let previous = directory(r.previous)?;
    let bundle = directory(r.bundle)?;
    let out = new_output(r.out, &[&root, &previous, &bundle])?;
    let old = hosting::inspect(&previous, r.previous_pin, anchor, false)?;
    let next = hosting::inspect(&bundle, r.bundle_pin, anchor, true)?;
    let required: BTreeSet<_> = old
        .history
        .iter()
        .chain(std::iter::once(&old.selected.snapshot))
        .collect();
    if required.iter().any(|id| !next.history.contains(id)) {
        bail!("Publication must retain every previous signed history proof");
    }
    transition(
        &metadata(&previous.join("v1"), anchor, false)?,
        &metadata(&bundle.join("v1"), anchor, true)?,
        true,
    )?;
    committed(&root, r.base_commit, &previous)?;
    committed(&root, r.candidate_commit, &bundle)?;
    // Recheck authenticated inputs after batch hashing. Inputs are protected
    // operator-owned immutable captures, not an arbitrary same-account sandbox.
    if hosting::inspect(&previous, r.previous_pin, anchor, false)?.selected != old.selected
        || hosting::inspect(&bundle, r.bundle_pin, anchor, true)?.selected != next.selected
    {
        bail!("Publication inputs changed during inspection");
    }
    emit(
        r,
        url,
        next.selected.snapshot,
        &out,
        "verified-git-publication-handoff-not-release-approval",
    )
}

fn emit(r: &Request<'_>, url: String, snapshot: String, out: &Path, class: &str) -> Result<()> {
    let reference = format!("refs/heads/{}", r.branch);
    let handoff = Handoff {
        schema: 1,
        evidence_class: class.into(),
        repository: r.repository.into(),
        branch: r.branch.into(),
        expected_base_commit: r.base_commit.into(),
        candidate_commit: r.candidate_commit.into(),
        previous_receipt_sha256: r.previous_pin.into(),
        candidate_receipt_sha256: r.bundle_pin.into(),
        snapshot,
        push_args: vec![
            "--no-replace-objects".into(),
            "-c".into(),
            "push.followTags=false".into(),
            "-c".into(),
            "push.recurseSubmodules=no".into(),
            "push".into(),
            "--porcelain".into(),
            "--no-verify".into(),
            format!("--force-with-lease={reference}:{}", r.base_commit),
            url,
            format!("{}:{reference}", r.candidate_commit),
        ],
        remove_environment_prefix: "GIT_".into(),
        environment: BTreeMap::from([
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            (
                "GIT_CONFIG_GLOBAL".into(),
                if cfg!(windows) { "NUL" } else { "/dev/null" }.into(),
            ),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GIT_NO_LAZY_FETCH".into(), "1".into()),
        ]),
    };
    let raw = serde_json::to_vec_pretty(&handoff)?;
    fs::create_dir(out)?;
    let mut owned = OwnedOutput(out.to_path_buf(), false);
    write(&out.join("publication.json"), &raw)?;
    sync_dir(out)?;
    owned.1 = true;
    println!(
        "Verified publication handoff SHA256 {}; no remote changed and no release approval granted",
        digest(&raw)
    );
    Ok(())
}

fn candidate(r: &Request<'_>) -> Result<(String, PathBuf)> {
    let url = repository_url(r.repository)?;
    if !oid(r.base_commit) || !oid(r.candidate_commit) || r.base_commit == r.candidate_commit {
        bail!("Independent distinct full Git SHA1 base/candidate commits required");
    }
    if r.branch.is_empty()
        || r.branch.starts_with('-')
        || r.branch.len() > 200
        || !r
            .branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
    {
        bail!("Expected publication branch is malformed");
    }
    let root = directory(r.checkout)?;
    let reference = format!("refs/heads/{}", r.branch);
    git(&root, &["check-ref-format", &reference], false)?;
    checkout(&root, &url, Some(r.candidate_commit))?;
    // Read actual commit headers, not a traversal that grafts could reinterpret.
    let commit = git(&root, &["cat-file", "-p", r.candidate_commit], false)?;
    let header = commit
        .split_once("\n\n")
        .context("Malformed publication commit")?
        .0;
    let parents: Vec<_> = header
        .lines()
        .filter_map(|line| line.strip_prefix("parent "))
        .collect();
    if parents != [r.base_commit] {
        bail!("Candidate must have exactly the expected base as its sole parent");
    }
    let changes = git(
        &root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-z",
            r.base_commit,
            r.candidate_commit,
            "--",
        ],
        false,
    )?;
    if changes
        .split('\0')
        .filter(|p| !p.is_empty())
        .any(|p| !p.starts_with("v1/") && !p.starts_with(&format!("{PROOFS}/")))
    {
        bail!("Publication candidate changes unrelated repository paths");
    }
    Ok((url, root))
}

pub(crate) fn prepare_migration(r: &Request<'_>) -> Result<()> {
    prepare_migration_with(r, &app_anchor)
}

pub(super) fn prepare_migration_with(r: &Request<'_>, anchor: &Anchor<'_>) -> Result<()> {
    let (url, root) = candidate(r)?;
    let archive = legacy::inspect(r.previous, r.previous_pin, anchor)?;
    let bundle = directory(r.bundle)?;
    let out = new_output(r.out, &[&root, &archive.path, &bundle])?;
    if archive.repository != r.repository || archive.tip != r.base_commit {
        bail!("Migration base/repository differs from pinned legacy archive");
    }
    let next = hosting::inspect(&bundle, r.bundle_pin, anchor, true)?;
    let tree = metadata(&bundle.join("v1"), anchor, true)?;
    migration::validate_baseline(&tree, &archive, r.previous_pin)?;
    if !next.history.is_empty() {
        bail!("Initial migration cannot invent earlier six-document history");
    }
    // Rebuild old evidence from actual Git ancestry/raw objects. Offline receipt
    // checks alone cannot authenticate its Git commit/source labels.
    let scratch = tempfile::tempdir()?;
    let recaptured = scratch.path().join("archive");
    let manifest = archive.path.join("manifest.json");
    let manifest_pin = digest(&crate::review::read(&manifest, 256 * 1024)?);
    legacy::capture_with(
        &root,
        r.repository,
        &manifest,
        &manifest_pin,
        &recaptured,
        anchor,
    )?;
    if digest(&crate::review::read(
        &recaptured.join("legacy-history.json"),
        8 * 1024 * 1024,
    )?) != r.previous_pin
    {
        bail!("Migration legacy archive differs from pinned Git source history");
    }
    committed(&root, r.candidate_commit, &bundle)?;
    legacy::inspect(&archive.path, r.previous_pin, anchor)?;
    if hosting::inspect(&bundle, r.bundle_pin, anchor, true)?.selected != next.selected {
        bail!("Migration bundle changed during inspection");
    }
    emit(
        r,
        url,
        next.selected.snapshot,
        &out,
        "verified-git-legacy-migration-handoff-not-release-approval",
    )
}
