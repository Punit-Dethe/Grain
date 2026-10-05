//! No-secret preparation, never signing/review authority. Run only in the
//! disposable unprivileged build job; a later trusted job must verify provenance.
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use grain_sdk::{distribution::ArtifactKind, submission::SourceSubmission};
use serde::Serialize;
use sha2::{Digest, Sha256};

const GIT_OUTPUT_MAX: u64 = 256 * 1024;

#[derive(Serialize)]
struct Receipt<'a> {
    schema: u8,
    evidence_class: &'static str,
    producer_version: &'static str,
    producer_sha256: String,
    submission_sha256: String,
    submission: &'a SourceSubmission,
    artifact: &'static str,
    artifact_sha256: String,
    artifact_size: usize,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// Fixed local Git operations only. No shell, fetch, checkout, build or hooks.
// Bound temporary output and wall time; drop its file and reap only this child.
fn git(root: &Path, args: &[&str], absent_allowed: bool) -> Result<String> {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(name);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .args([
            "--no-replace-objects",
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
        ])
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut output = tempfile::tempfile()?;
    command.stdout(Stdio::from(output.try_clone()?));
    let mut child = command.spawn().context("start local Git inspection")?;
    let started = Instant::now();
    let status = loop {
        // On every error path, reap our own child before returning.
        let observed = (|| -> Result<_> {
            if output.metadata()?.len() > GIT_OUTPUT_MAX
                || started.elapsed() > Duration::from_secs(30)
            {
                bail!("Local Git inspection exceeded its output/time budget");
            }
            Ok(child.try_wait()?)
        })();
        match observed {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    };
    if !(status.success() || absent_allowed && status.code() == Some(1)) {
        bail!("Local Git inspection failed; require a complete standalone checkout");
    }
    output.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    output.take(GIT_OUTPUT_MAX + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > GIT_OUTPUT_MAX {
        bail!("Local Git inspection exceeded its output budget");
    }
    String::from_utf8(bytes).context("Git inspection returned invalid UTF-8")
}

fn verify_checkout(root: &Path, source: &SourceSubmission) -> Result<()> {
    let metadata = fs::symlink_metadata(root.join(".git"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("Source requires a standalone checkout, not a linked worktree/submodule");
    }
    if fs::symlink_metadata(root.join(".git/config"))?
        .file_type()
        .is_symlink()
    {
        bail!("Source Git config must not be a symbolic link");
    }
    // Includes/filters can launch author-configured programs during status.
    // Global/system config is disabled; reject those local extensions too.
    if !git(
        root,
        &[
            "config",
            "--local",
            "--no-includes",
            "--get-regexp",
            "^(filter\\.|include\\.|includeif\\.|extensions\\.|remote\\..*\\.promisor$)",
        ],
        true,
    )?
    .is_empty()
    {
        bail!("Source Git configuration must not contain includes or filters, repository extensions or promisor remotes");
    }
    let top = git(root, &["rev-parse", "--show-toplevel"], false)?;
    if Path::new(top.trim()).canonicalize()? != root {
        bail!("Source must be the repository root");
    }
    let origin = git(
        root,
        &["config", "--local", "--get-all", "remote.origin.url"],
        false,
    )?;
    if origin.trim() != source.source_repo {
        bail!("Source origin does not match the submitted repository");
    }
    let head = git(
        root,
        &["rev-parse", "--verify", "--end-of-options", "HEAD^{commit}"],
        false,
    )?;
    let tag = format!("refs/tags/{}^{{commit}}", source.tag);
    let tagged = git(
        root,
        &["rev-parse", "--verify", "--end-of-options", &tag],
        false,
    )?;
    if head.trim() != source.commit || tagged.trim() != source.commit {
        bail!("Source HEAD and exact tag must resolve to the submitted commit");
    }
    let modes = git(root, &["ls-files", "--stage", "-z"], false)?;
    if modes
        .split('\0')
        .filter(|v| !v.is_empty())
        .any(|v| !v.starts_with("100644 ") && !v.starts_with("100755 "))
    {
        bail!("Source index must not contain symlinks, submodules or conflicts");
    }
    let flags = git(root, &["ls-files", "-v", "-z"], false)?;
    if flags
        .split('\0')
        .filter(|v| !v.is_empty())
        .any(|v| !v.starts_with("H "))
    {
        bail!("Source index must not hide files with assume-unchanged/skip-worktree");
    }
    if !git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=normal",
            "--ignore-submodules=all",
        ],
        false,
    )?
    .is_empty()
    {
        bail!("Source tracked files/index must be clean, with no untracked inputs");
    }
    let definition = if source.artifact_kind == ArtifactKind::Native {
        "manifest.json"
    } else {
        "mcp.json"
    };
    git(
        root,
        &[
            "ls-files",
            "--error-unmatch",
            "--",
            definition,
            "DESCRIPTION.md",
        ],
        false,
    )?;
    for asset in &source.media {
        git(
            root,
            &[
                "ls-files",
                "--error-unmatch",
                "--",
                &format!("media/{}", asset.name),
            ],
            false,
        )?;
    }
    Ok(())
}

fn new_output(out: &Path, roots: &[&Path]) -> Result<PathBuf> {
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .context("output parent must already exist")?;
    let name = out
        .file_name()
        .context("output requires a file/directory name")?;
    let out = parent.join(name);
    if roots.iter().any(|root| out.starts_with(root)) {
        bail!("Output must be outside the source and submission roots");
    }
    if out.try_exists()? || fs::symlink_metadata(&out).is_ok() {
        bail!("Output already exists; use a new output path");
    }
    Ok(out)
}

pub(super) fn write_artifact(src: &Path, out: &Path, bytes: &[u8]) -> Result<()> {
    let root = src.canonicalize()?;
    let out = new_output(out, &[&root])?;
    let mut staging = tempfile::NamedTempFile::new_in(out.parent().unwrap())?;
    staging.write_all(bytes)?;
    staging.as_file().sync_all()?;
    staging
        .persist_noclobber(out)
        .map_err(|error| error.error)?;
    Ok(())
}

struct OwnedOutput(PathBuf, bool);
impl Drop for OwnedOutput {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

pub(super) fn prepare(submission_dir: PathBuf, src: PathBuf, out: PathBuf) -> Result<()> {
    for root in [&submission_dir, &src] {
        if fs::symlink_metadata(root)?.file_type().is_symlink() {
            bail!("Preparation roots must not be symbolic links");
        }
    }
    let submission_dir = submission_dir.canonicalize()?;
    let src = src.canonicalize()?;
    let id = submission_dir
        .file_name()
        .and_then(|s| s.to_str())
        .context("invalid submission folder")?;
    let submission =
        grain_extension_checks::read_submission(&submission_dir, id).map_err(anyhow::Error::msg)?;
    verify_checkout(&src, &submission)?;
    let identity = grain_extension_checks::project_identity(&src).map_err(anyhow::Error::msg)?;
    if identity.kind != submission.artifact_kind
        || identity.id != submission.id
        || identity.version != submission.version
        || identity.grain_api != submission.grain_api
    {
        bail!("Source project kind/id/version/API differs from submission");
    }
    let listing =
        grain_extension_checks::listing::read_listing(&src).map_err(anyhow::Error::msg)?;
    if listing.sha256 != submission.description_sha256
        || listing.description.len() as u64 != submission.description_size
        || listing.media != submission.media
    {
        bail!("Pinned source DESCRIPTION/media differs from submission");
    }
    let (artifact, bytes) = match identity.kind {
        ArtifactKind::Native => (
            "artifact.grainpack",
            serde_json::to_vec(
                &grain_extension_checks::build_pack(&src).map_err(anyhow::Error::msg)?,
            )?,
        ),
        ArtifactKind::McpDescriptor => {
            let report = grain_extension_checks::doctor(&src);
            if !report.is_clean() {
                bail!("doctor found problems:\n{report}");
            }
            (
                "artifact.mcp.json",
                serde_json::to_vec(
                    &grain_extension_checks::read_mcp_descriptor(&src)
                        .map_err(anyhow::Error::msg)?,
                )?,
            )
        }
    };
    let out = new_output(&out, &[&src, &submission_dir])?;
    fs::create_dir(&out)?;
    let mut owned = OwnedOutput(out.clone(), false);
    fs::write(out.join(artifact), &bytes)?;
    fs::write(out.join("DESCRIPTION.md"), &listing.description)?;
    if !listing.media.is_empty() {
        fs::create_dir(out.join("media"))?;
    }
    for asset in &listing.media {
        let path = src.join("media").join(&asset.name);
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!("Media changed into a link during preparation");
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(grain_sdk::submission::LISTING_MEDIA_MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != asset.size || digest(&bytes) != asset.sha256 {
            bail!("Media changed during preparation");
        }
        fs::write(out.join("media").join(&asset.name), bytes)?;
    }
    // A second source inspection catches changes during collection. This is
    // not an OS snapshot: CI must keep its owned checkout immutable throughout.
    verify_checkout(&src, &submission)?;
    let again =
        grain_extension_checks::read_submission(&submission_dir, id).map_err(anyhow::Error::msg)?;
    let submitted = serde_json::to_vec(&submission)?;
    if serde_json::to_vec(&again)? != submitted {
        bail!("Submission changed during preparation");
    }
    let mut producer = fs::File::open(std::env::current_exe()?)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = producer.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let receipt = Receipt {
        schema: 1,
        evidence_class: "local-preparation/unsigned-not-reviewed",
        producer_version: env!("CARGO_PKG_VERSION"),
        producer_sha256: format!("{:x}", hash.finalize()),
        submission_sha256: digest(&submitted),
        submission: &submission,
        artifact,
        artifact_sha256: digest(&bytes),
        artifact_size: bytes.len(),
    };
    // Completion marker last. Signing will require independently trusted CI
    // identity/review/digest checks, never this self-authored receipt alone.
    fs::write(
        out.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    owned.1 = true;
    println!(
        "prepared {} (unsigned; reviewed CI/signing handoff still required)",
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        _owned: tempfile::TempDir,
        src: PathBuf,
        submission: PathBuf,
        out: PathBuf,
        value: SourceSubmission,
    }
    fn command(root: &Path, args: &[&str]) {
        let mut command = Command::new("git");
        command
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "tag.gpgsign=false",
                "-c",
                "core.hooksPath=",
            ])
            .args(args)
            .current_dir(root)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let status = command.status().unwrap();
        assert!(status.success(), "fixture git operation failed: {args:?}");
    }
    fn fixture(mcp: bool, media: bool) -> Fixture {
        let owned = tempfile::tempdir().unwrap();
        let mut args = vec!["init", "Tools", "--id", "com.example.tools"];
        if mcp {
            args.extend(["--mcp-url", "https://tools.example.com/mcp"]);
        }
        grain_ext_cli::run(args.into_iter().map(String::from), owned.path()).unwrap();
        let src = owned.path().join("tools");
        if !mcp {
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image::RgbaImage::new(512, 512))
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            fs::write(src.join("icon.png"), bytes.into_inner()).unwrap();
            fs::create_dir(src.join("dist")).unwrap();
            fs::write(
                src.join("dist/main.js"),
                "grain.actions.register('hello', () => ({ok:{title:'Hello',body:'Hello'}}));",
            )
            .unwrap();
            // Preparation packages built bytes but never invokes this script.
            fs::write(
                src.join("package.json"),
                r#"{"scripts":{"build":"exit 93"}}"#,
            )
            .unwrap();
        }
        if media {
            fs::create_dir(src.join("media")).unwrap();
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image::RgbaImage::new(16, 16))
                .write_to(&mut bytes, image::ImageFormat::WebP)
                .unwrap();
            fs::write(src.join("media/cover.webp"), bytes.into_inner()).unwrap();
        }
        command(&src, &["init", "--quiet"]);
        command(
            &src,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/tools",
            ],
        );
        command(&src, &["add", "."]);
        command(&src, &["commit", "--quiet", "-m", "Fixture"]);
        command(&src, &["tag", "-a", "v0.1.0", "-m", "Fixture tag"]);
        let commit = git(&src, &["rev-parse", "HEAD"], false)
            .unwrap()
            .trim()
            .to_string();
        let listing = grain_extension_checks::listing::read_listing(&src).unwrap();
        let value = SourceSubmission {
            schema: 1,
            artifact_kind: if mcp {
                ArtifactKind::McpDescriptor
            } else {
                ArtifactKind::Native
            },
            id: "com.example.tools".into(),
            version: "0.1.0".into(),
            grain_api: "^1.0".into(),
            source_repo: "https://github.com/example/tools".into(),
            tag: "v0.1.0".into(),
            commit,
            summary: "Tools".into(),
            categories: vec!["tools".into()],
            license: "MIT".into(),
            contact: "Fixture".into(),
            description_sha256: listing.sha256,
            description_size: listing.description.len() as u64,
            media: listing.media,
        };
        let submission = owned.path().join("com.example.tools");
        fs::create_dir(&submission).unwrap();
        fs::copy(
            src.join("DESCRIPTION.md"),
            submission.join("DESCRIPTION.md"),
        )
        .unwrap();
        fs::write(
            submission.join("submission.toml"),
            grain_extension_checks::serialize_submission(&value).unwrap(),
        )
        .unwrap();
        Fixture {
            src,
            submission,
            out: owned.path().join("prepared"),
            value,
            _owned: owned,
        }
    }
    fn update(f: &Fixture) {
        fs::write(
            f.submission.join("submission.toml"),
            grain_extension_checks::serialize_submission(&f.value).unwrap(),
        )
        .unwrap();
    }
    fn rejected(f: &Fixture, fragment: &str) {
        let error = prepare(f.submission.clone(), f.src.clone(), f.out.clone()).unwrap_err();
        assert!(
            error.to_string().contains(fragment),
            "unexpected refusal: {error:#}"
        );
        assert!(!f.out.exists(), "invalid preparation created output");
    }
    #[test]
    fn native_and_mcp_receipts_bind_exact_canonical_bytes_without_building_or_signing() {
        for mcp in [false, true] {
            let f = fixture(mcp, true);
            prepare(f.submission.clone(), f.src.clone(), f.out.clone()).unwrap();
            let receipt: serde_json::Value =
                serde_json::from_slice(&fs::read(f.out.join("receipt.json")).unwrap()).unwrap();
            assert_eq!(
                receipt["evidence_class"],
                "local-preparation/unsigned-not-reviewed"
            );
            let expected = if mcp {
                serde_json::to_vec(&grain_extension_checks::read_mcp_descriptor(&f.src).unwrap())
                    .unwrap()
            } else {
                serde_json::to_vec(&grain_extension_checks::build_pack(&f.src).unwrap()).unwrap()
            };
            let artifact = receipt["artifact"].as_str().unwrap();
            assert_eq!(fs::read(f.out.join(artifact)).unwrap(), expected);
            assert_eq!(receipt["artifact_sha256"], digest(&expected));
            assert_eq!(receipt["artifact_size"], expected.len());
            assert_eq!(receipt["submission"]["commit"], f.value.commit);
            assert_eq!(receipt["producer_sha256"].as_str().unwrap().len(), 64);
            assert_eq!(
                fs::read(f.out.join("DESCRIPTION.md")).unwrap(),
                fs::read(f.src.join("DESCRIPTION.md")).unwrap()
            );
            assert_eq!(
                fs::read(f.out.join("media/cover.webp")).unwrap(),
                fs::read(f.src.join("media/cover.webp")).unwrap()
            );
            let saved = fs::read(f.out.join("receipt.json")).unwrap();
            assert!(prepare(f.submission.clone(), f.src.clone(), f.out.clone())
                .unwrap_err()
                .to_string()
                .contains("already exists"));
            assert_eq!(fs::read(f.out.join("receipt.json")).unwrap(), saved);
        }
    }
    #[test]
    fn wrong_origin_head_or_exact_tag_refuses_before_output() {
        let mut f = fixture(true, false);
        command(
            &f.src,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/example/other",
            ],
        );
        rejected(&f, "origin");
        command(
            &f.src,
            &["remote", "set-url", "origin", &f.value.source_repo],
        );
        f.value.tag = "missing".into();
        update(&f);
        rejected(&f, "Git inspection failed");
        f.value.tag = "v0.1.0".into();
        f.value.commit = "1".repeat(40);
        update(&f);
        rejected(&f, "submitted commit");
        fs::write(f.src.join("README.md"), "Next committed source").unwrap();
        command(&f.src, &["add", "README.md"]);
        command(&f.src, &["commit", "--quiet", "-m", "Second"]);
        f.value.commit = git(&f.src, &["rev-parse", "HEAD"], false)
            .unwrap()
            .trim()
            .into();
        update(&f);
        rejected(&f, "exact tag");
    }
    #[test]
    fn dirty_untracked_and_hidden_index_inputs_refuse() {
        let f = fixture(true, false);
        let saved = fs::read(f.src.join("DESCRIPTION.md")).unwrap();
        fs::write(f.src.join("DESCRIPTION.md"), "Changed").unwrap();
        rejected(&f, "clean");
        fs::write(f.src.join("DESCRIPTION.md"), saved).unwrap();
        fs::write(f.src.join("extra.txt"), "untracked").unwrap();
        rejected(&f, "untracked");
        fs::remove_file(f.src.join("extra.txt")).unwrap();
        command(
            &f.src,
            &["update-index", "--assume-unchanged", "DESCRIPTION.md"],
        );
        rejected(&f, "hide files");
        command(
            &f.src,
            &["update-index", "--no-assume-unchanged", "DESCRIPTION.md"],
        );
        command(
            &f.src,
            &["update-index", "--skip-worktree", "DESCRIPTION.md"],
        );
        rejected(&f, "hide files");
    }
    #[test]
    fn source_identity_description_and_media_must_match_submission() {
        let mut f = fixture(true, true);
        f.value.version = "0.2.0".into();
        update(&f);
        rejected(&f, "kind/id/version/API");
        f.value.version = "0.1.0".into();
        f.value.media[0].sha256 = "1".repeat(64);
        update(&f);
        rejected(&f, "DESCRIPTION/media");
        f.value.media.clear();
        update(&f);
        rejected(&f, "DESCRIPTION/media");
        let g = fixture(true, false);
        fs::write(g.submission.join("DESCRIPTION.md"), "tampered").unwrap();
        rejected(&g, "DESCRIPTION");
    }
    #[test]
    fn local_git_filters_submodules_and_unbuilt_native_refuse() {
        let f = fixture(true, false);
        command(&f.src, &["config", "filter.fake.clean", "exit 99"]);
        rejected(&f, "includes or filters");
        command(&f.src, &["config", "--unset", "filter.fake.clean"]);
        command(&f.src, &["config", "include.path", "missing-config"]);
        rejected(&f, "includes or filters");
        command(&f.src, &["config", "--unset", "include.path"]);
        command(&f.src, &["config", "extensions.worktreeConfig", "true"]);
        rejected(&f, "repository extensions");
        command(
            &f.src,
            &["config", "--local", "--unset", "extensions.worktreeConfig"],
        );
        command(
            &f.src,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{},nested", f.value.commit),
            ],
        );
        rejected(&f, "submodules");
        let n = fixture(false, false);
        fs::remove_file(n.src.join("dist/main.js")).unwrap();
        rejected(&n, "run the project build first");
    }
    #[test]
    fn unsafe_index_and_oversized_git_output_are_refused_with_no_prepared_artifact() {
        let f = fixture(true, false);
        let blob = git(&f.src, &["rev-parse", "HEAD:DESCRIPTION.md"], false).unwrap();
        command(
            &f.src,
            &[
                "update-index",
                "--cacheinfo",
                &format!("120000,{},DESCRIPTION.md", blob.trim()),
            ],
        );
        rejected(&f, "symlinks");
        let g = fixture(true, false);
        fs::write(
            g.src.join("large.txt"),
            vec![b'x'; GIT_OUTPUT_MAX as usize + 1],
        )
        .unwrap();
        command(&g.src, &["add", "large.txt"]);
        command(&g.src, &["commit", "--quiet", "-m", "Large output fixture"]);
        let error = git(&g.src, &["cat-file", "blob", "HEAD:large.txt"], false).unwrap_err();
        assert!(error.to_string().contains("budget"));
        assert!(!g.out.exists());
    }
    #[test]
    fn shared_build_pack_embeds_icon_and_refuses_paths_or_existing_output() {
        let f = fixture(false, false);
        let out = f._owned.path().join("built.grainpack");
        crate::build_pack(f.src.clone(), out.clone()).unwrap();
        let bytes = fs::read(&out).unwrap();
        assert_eq!(
            bytes,
            serde_json::to_vec(&grain_extension_checks::build_pack(&f.src).unwrap()).unwrap()
        );
        let pack: grain_sdk::GrainPack = serde_json::from_slice(&bytes).unwrap();
        assert!(pack.payloads.icon_png.is_some());
        assert!(crate::build_pack(f.src.clone(), out.clone()).is_err());
        assert_eq!(fs::read(&out).unwrap(), bytes);
        let manifest_path = f.src.join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["entry"] = "../outside.js".into();
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(crate::build_pack(f.src.clone(), f.out.clone()).is_err());
        assert!(!f.out.exists());
        assert!(write_artifact(&f.src, &manifest_path, b"overwrite").is_err());
    }
}
