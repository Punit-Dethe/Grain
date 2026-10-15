//! Local draft replacement only. Source approval and publication are separate.
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use grain_sdk::submission::SourceSubmission;

struct DraftLock(PathBuf);
impl Drop for DraftLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(super) fn write(
    extensions: &Path,
    candidate: &SourceSubmission,
    toml: &str,
    description: &str,
    update_from: Option<&str>,
) -> Result<PathBuf> {
    // Cooperating CLI calls cannot replace the same draft concurrently. A killed
    // process leaves a visible lock for manual inspection, never an implicit retry.
    let lock_path = extensions
        .parent()
        .context("registry root missing")?
        .join(format!(".grain-submit-{}.lock", candidate.id));
    let lock_file = fs::OpenOptions::new().write(true).create_new(true).open(&lock_path)
        .with_context(|| format!("Submission is locked or not writable: {}. If an earlier command was interrupted, inspect its staging files and remove this lock only when no submission command is running.", lock_path.display()))?;
    let _lock = DraftLock(lock_path);
    let _lock_file = lock_file;
    let target = extensions.join(&candidate.id);
    if let Some(expected) = update_from {
        let metadata = fs::symlink_metadata(&target)
            .context("--update-from requires an existing valid submission")?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            bail!("Existing submission must be a real directory");
        }
        // Do not replace directories containing hand-maintained unrelated files.
        for file in fs::read_dir(&target)? {
            let file = file?;
            if !file.file_type()?.is_file()
                || !matches!(
                    file.file_name().to_str(),
                    Some("submission.toml" | "DESCRIPTION.md")
                )
            {
                bail!("Existing submission contains unexpected files; refusing to replace it");
            }
        }
        let previous = grain_extension_checks::read_submission(&target, &candidate.id)
            .map_err(anyhow::Error::msg)
            .context("Existing submission is invalid; left unchanged")?;
        if previous.version != expected {
            bail!(
                "Existing submission is version {}, not {expected}; left unchanged",
                previous.version
            );
        }
        if previous.source_repo != candidate.source_repo
            || previous.artifact_kind != candidate.artifact_kind
        {
            bail!("Source repository or project kind changed; this needs a maintainer transfer review, not --update-from");
        }
        if previous.version == candidate.version
            || previous.commit == candidate.commit
            || previous.tag == candidate.tag
        {
            bail!("An update needs a different version, source commit and tag; left unchanged");
        }
    } else if fs::symlink_metadata(&target).is_ok() {
        bail!("Existing submissions are never overwritten implicitly; use --update-from <existing-version>");
    }

    // Prepare and validate both files before making the old draft unavailable.
    // Same-volume directory moves keep partial file pairs out of the live path.
    let staging = tempfile::Builder::new()
        .prefix(".grain-submit-")
        .tempdir_in(extensions)?;
    let prepared = staging.path().join(&candidate.id);
    fs::create_dir(&prepared)?;
    fs::write(prepared.join("DESCRIPTION.md"), description)?;
    fs::write(prepared.join("submission.toml"), toml)?;
    grain_extension_checks::read_submission(&prepared, &candidate.id)
        .map_err(anyhow::Error::msg)?;
    install(staging, &prepared, &target, update_from.is_some())?;
    Ok(target)
}

fn install(staging: tempfile::TempDir, prepared: &Path, target: &Path, update: bool) -> Result<()> {
    let backup = staging.path().join("previous");
    if update {
        fs::rename(target, &backup)
            .context("Move old draft aside; original draft was not replaced")?;
    }
    if let Err(error) = fs::rename(prepared, target) {
        if update {
            if let Err(restore_error) = fs::rename(&backup, target) {
                let recovery = staging.keep();
                bail!("Draft replacement failed: {error}; restore failed: {restore_error}. Original draft retained at {}", recovery.join("previous").display());
            }
        }
        return Err(error).context("Draft replacement failed; original submission preserved");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_directory_install_restores_the_old_draft() {
        let registry = tempfile::tempdir().unwrap();
        let target = registry.path().join("com.example.tools");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("submission.toml"), "original").unwrap();
        let stage = tempfile::tempdir_in(registry.path()).unwrap();
        let missing = stage.path().join("missing");
        assert!(install(stage, &missing, &target, true).is_err());
        assert_eq!(
            fs::read(target.join("submission.toml")).unwrap(),
            b"original"
        );
        assert_eq!(fs::read_dir(registry.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn linked_submission_is_not_replaced() {
        let registry = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let extensions = registry.path().join("extensions");
        fs::create_dir(&extensions).unwrap();
        let candidate: SourceSubmission = serde_json::from_value(serde_json::json!({
            "schema":1,"artifact_kind":"mcp-descriptor","id":"com.example.tools",
            "version":"0.1.1","grain_api":"^1.0","source_repo":"https://github.com/example/tools",
            "tag":"v0.1.1","commit":"a".repeat(40),"summary":"Tools","categories":["tools"],
            "license":"MIT","contact":"maintainer","description_sha256":"b".repeat(64),"description_size":5
        })).unwrap();
        std::os::unix::fs::symlink(external.path(), extensions.join(&candidate.id)).unwrap();
        assert!(write(&extensions, &candidate, "unused", "unused", Some("0.1.0")).is_err());
        assert_eq!(fs::read_dir(external.path()).unwrap().count(), 0);
    }
}
