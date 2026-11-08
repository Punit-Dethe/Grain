//! Immutable, hash-addressed publication for local tool fixtures.

use std::path::{Path, PathBuf};

use crate::trust::{self, TrustError};

/// Why an install/update was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// The bytes did not match the expected artifact hash.
    Hash(TrustError),
    /// A filesystem step failed.
    Io(String),
    /// The artifact does not satisfy the local fixture contract.
    Manifest(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Hash(e) => write!(f, "artifact verification failed: {e}"),
            InstallError::Io(e) => write!(f, "io error: {e}"),
            InstallError::Manifest(e) => write!(f, "extension refused: {e}"),
        }
    }
}

impl std::error::Error for InstallError {}

/// Where an extension's versioned payload lives on disk:
/// `<root>/<id>/<version>/`. The previous version's directory survives until
/// the new one is in place, so a bad update is one directory away from rollback
/// (DISTRIBUTION-PLAN Â§5.2).
pub fn version_dir(root: &Path, id: &str, version: &str) -> PathBuf {
    root.join(id).join(version)
}

/// Artifacts are addressed by content, so same-version replacements cannot
/// overwrite another registry owner's bytes.
pub fn artifact_dir(
    root: &Path,
    id: &str,
    version: &str,
    hash: &str,
) -> Result<PathBuf, InstallError> {
    grain_sdk::validate_extension_id(id).map_err(InstallError::Io)?;
    grain_sdk::validate_extension_version(version).map_err(InstallError::Io)?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(InstallError::Manifest("Invalid artifact hash.".into()));
    }
    Ok(version_dir(root, id, version).join(hash.to_ascii_lowercase()))
}

/// Publish complete bytes once. Never replace/delete a published artifact,
/// including on a lost registry race or failed registry persistence.
pub fn publish_artifact(
    root: &Path,
    id: &str,
    version: &str,
    hash: &str,
    bytes: &[u8],
) -> Result<PathBuf, InstallError> {
    use std::io::{Read, Write};
    let destination = artifact_dir(root, id, version, hash)?;
    if bytes.len() as u64 > grain_sdk::PACK_MAX_BYTES {
        return Err(InstallError::Manifest(
            "Tool package exceeds the import budget.".into(),
        ));
    }
    trust::verify_artifact(bytes, hash).map_err(InstallError::Hash)?;
    let verify_existing = || {
        let file = std::fs::File::open(destination.join("pack.grainpack.json"))
            .map_err(|error| InstallError::Io(error.to_string()))?;
        let mut existing = Vec::new();
        file.take(grain_sdk::PACK_MAX_BYTES + 1)
            .read_to_end(&mut existing)
            .map_err(|error| InstallError::Io(error.to_string()))?;
        if existing.len() as u64 > grain_sdk::PACK_MAX_BYTES {
            return Err(InstallError::Manifest(
                "Existing artifact exceeds the import budget.".into(),
            ));
        }
        trust::verify_artifact(&existing, hash).map_err(InstallError::Hash)
    };
    if destination.exists() {
        verify_existing()?;
        return Ok(destination);
    }
    let staging_root = root.join(".staging");
    std::fs::create_dir_all(&staging_root).map_err(|error| InstallError::Io(error.to_string()))?;
    let staging = tempfile::Builder::new()
        .prefix("grainpack-")
        .tempdir_in(&staging_root)
        .map_err(|error| InstallError::Io(error.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging.path().join("pack.grainpack.json"))
        .map_err(|error| InstallError::Io(error.to_string()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| InstallError::Io(error.to_string()))?;
    drop(file);
    std::fs::create_dir_all(destination.parent().expect("artifact parent"))
        .map_err(|error| InstallError::Io(error.to_string()))?;
    if let Err(error) = std::fs::rename(staging.path(), &destination) {
        // Another publisher may have won with the same bytes. Verify it, and
        // let our private staging owner clean up only its own directory.
        if destination.exists() {
            verify_existing()?;
        } else {
            return Err(InstallError::Io(error.to_string()));
        }
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_same_version_publication_preserves_all_hash_owners() {
        let dir = tmp();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(9));
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let root = dir.path().to_path_buf();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let bytes = tool_pack("com.example.tools", "1", &[]);
                    let mut bytes = bytes;
                    // Four callers publish the same bytes, four distinct siblings.
                    if index >= 4 {
                        bytes.extend(std::iter::repeat_n(b' ', index));
                    }
                    let hash = trust::sha256_hex(&bytes);
                    barrier.wait();
                    let path =
                        publish_artifact(&root, "com.example.tools", "1", &hash, &bytes).unwrap();
                    assert_eq!(
                        std::fs::read(path.join("pack.grainpack.json")).unwrap(),
                        bytes
                    );
                    path
                })
            })
            .collect();
        barrier.wait();
        let paths: std::collections::HashSet<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(paths.len(), 5);
        assert_eq!(
            std::fs::read_dir(dir.path().join(".staging"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn corrupt_published_artifact_is_refused_without_overwriting_it() {
        let dir = tmp();
        let bytes = tool_pack("com.example.tools", "1", &[]);
        let hash = trust::sha256_hex(&bytes);
        let path = publish_artifact(dir.path(), "com.example.tools", "1", &hash, &bytes).unwrap();
        std::fs::write(path.join("pack.grainpack.json"), b"corrupt-local").unwrap();
        assert!(publish_artifact(dir.path(), "com.example.tools", "1", &hash, &bytes).is_err());
        assert_eq!(
            std::fs::read(path.join("pack.grainpack.json")).unwrap(),
            b"corrupt-local"
        );
        for hash in ["../escape", &"g".repeat(64)] {
            assert!(artifact_dir(dir.path(), "com.example.tools", "1", hash).is_err());
        }
    }

    #[test]
    fn publication_error_cleans_only_private_staging() {
        let dir = tmp();
        let bytes = tool_pack("com.example.tools", "1", &[]);
        let hash = trust::sha256_hex(&bytes);
        std::fs::write(dir.path().join("com.example.tools"), b"blocked-parent").unwrap();
        assert!(publish_artifact(dir.path(), "com.example.tools", "1", &hash, &bytes).is_err());
        assert_eq!(
            std::fs::read(dir.path().join("com.example.tools")).unwrap(),
            b"blocked-parent"
        );
        assert_eq!(
            std::fs::read_dir(dir.path().join(".staging"))
                .unwrap()
                .count(),
            0
        );
    }

    fn tool_pack(id: &str, version: &str, _capabilities: &[&str]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "id": id, "version": version })).unwrap()
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
}
