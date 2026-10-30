//! One bounded cache file containing exact signed bytes. Authentication remains
//! the store client's responsibility. Atomic replacement prevents torn pairs
//! and mixed local selections. Hosted generation binding is verified separately
//! through the signed index; a policy-only cache update may remain offline.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};

pub const FILE_NAME: &str = "metadata.cache";
pub const DOCUMENT_MAX_BYTES: u64 = 4 * 1024 * 1024;
pub const SIGNATURE_MAX_BYTES: u64 = 64 * 1024;
const MAGIC: &[u8; 8] = b"GRAINMC1";

#[derive(Clone)]
pub struct SignedDocument {
    pub document: Vec<u8>,
    pub signature: String,
}

pub struct Metadata {
    pub roots: SignedDocument,
    pub index: SignedDocument,
    pub revocations: SignedDocument,
}

impl Metadata {
    pub fn seed() -> Self {
        use crate::trust;
        let pair = |document: &str, signature: &str| SignedDocument {
            document: document.as_bytes().to_vec(),
            signature: signature.into(),
        };
        Self {
            roots: pair(trust::SEED_ROOTS, trust::SEED_ROOTS_SIG),
            index: pair(trust::SEED_INDEX, trust::SEED_INDEX_SIG),
            revocations: pair(trust::SEED_REVOCATIONS, trust::SEED_REVOCATIONS_SIG),
        }
    }

    /// Absence permits bootstrap. Any other read failure must stay distinct.
    pub fn read(dir: &Path) -> Result<Option<Self>> {
        let mut file = match std::fs::File::open(dir.join(FILE_NAME)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("open store metadata cache"),
        };
        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            bail!("invalid store metadata cache header");
        }
        let mut pair = || -> Result<SignedDocument> {
            let document = read_part(&mut file, DOCUMENT_MAX_BYTES)?;
            let signature = String::from_utf8(read_part(&mut file, SIGNATURE_MAX_BYTES)?)?;
            Ok(SignedDocument {
                document,
                signature,
            })
        };
        let metadata = Self {
            roots: pair()?,
            index: pair()?,
            revocations: pair()?,
        };
        let mut extra = [0];
        if file.read(&mut extra)? != 0 {
            bail!("trailing store metadata cache bytes");
        }
        Ok(Some(metadata))
    }

    pub fn write(&self, dir: &Path) -> Result<()> {
        // Bound every input before creating a temporary file. Serialize directly
        // to disk instead of another catalogue-sized buffer or JSON byte arrays.
        for pair in [&self.roots, &self.index, &self.revocations] {
            if pair.document.is_empty()
                || pair.document.len() as u64 > DOCUMENT_MAX_BYTES
                || pair.signature.is_empty()
                || pair.signature.len() as u64 > SIGNATURE_MAX_BYTES
            {
                bail!("store metadata cache component exceeds bounds");
            }
        }
        crate::extensions::atomic_write_with(&dir.join(FILE_NAME), |file| {
            file.write_all(MAGIC)?;
            for pair in [&self.roots, &self.index, &self.revocations] {
                for bytes in [pair.document.as_slice(), pair.signature.as_bytes()] {
                    file.write_all(&(bytes.len() as u32).to_le_bytes())?;
                    file.write_all(bytes)?;
                }
            }
            Ok(())
        })
        .context("persist store metadata cache")
    }
}

fn read_part(reader: &mut impl Read, max: u64) -> Result<Vec<u8>> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as u64;
    if length == 0 || length > max {
        bail!("store metadata cache component exceeds bounds");
    }
    let mut bytes = Vec::new();
    reader.take(length).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != length {
        bail!("truncated store metadata cache");
    }
    Ok(bytes)
}

/// Call only after authenticating both documents. Equal versions identify the
/// same exact signed JSON bytes, rather than allowing replacement by a replay.
pub fn check_identity(
    previous_version: u64,
    previous: &[u8],
    offered_version: u64,
    offered: &[u8],
) -> Result<()> {
    if offered_version < previous_version {
        bail!("store metadata version rollback");
    }
    if offered_version == previous_version && previous != offered {
        bail!("store metadata changed without a version increment");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_signed_seed_survives_atomic_replacement_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Metadata::read(dir.path()).unwrap().is_none());
        Metadata::seed().write(dir.path()).unwrap();
        Metadata::seed().write(dir.path()).unwrap();
        let loaded = Metadata::read(dir.path()).unwrap().unwrap();
        let roots =
            crate::trust::verify_roots(&loaded.roots.document, &loaded.roots.signature).unwrap();
        crate::trust::verify_index(
            &roots,
            &loaded.index.document,
            &loaded.index.signature,
            None,
            0,
            false,
        )
        .unwrap();
        crate::trust::verify_revocations(
            &roots,
            &loaded.revocations.document,
            &loaded.revocations.signature,
        )
        .unwrap();
        assert_eq!(loaded.index.document, crate::trust::SEED_INDEX.as_bytes());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn truncation_bad_lengths_header_signature_encoding_and_trailing_bytes_refuse() {
        let dir = tempfile::tempdir().unwrap();
        Metadata::seed().write(dir.path()).unwrap();
        let path = dir.path().join(FILE_NAME);
        let original = std::fs::read(&path).unwrap();
        for length in [0, 7, 8, 11, original.len() - 1] {
            std::fs::write(&path, &original[..length]).unwrap();
            assert!(Metadata::read(dir.path()).is_err());
        }
        for length in [0u32, (DOCUMENT_MAX_BYTES + 1) as u32, u32::MAX] {
            let mut bad = original.clone();
            bad[8..12].copy_from_slice(&length.to_le_bytes());
            std::fs::write(&path, bad).unwrap();
            assert!(Metadata::read(dir.path()).is_err());
        }
        let mut bad = original.clone();
        bad[0] ^= 1;
        std::fs::write(&path, bad).unwrap();
        assert!(Metadata::read(dir.path()).is_err());
        let mut bad = original.clone();
        bad.push(0);
        std::fs::write(&path, bad).unwrap();
        assert!(Metadata::read(dir.path()).is_err());
        let mut bad = original;
        let signature_start = 16 + u32::from_le_bytes(bad[8..12].try_into().unwrap()) as usize;
        bad[signature_start] = 255;
        std::fs::write(&path, bad).unwrap();
        assert!(Metadata::read(dir.path()).is_err());
    }

    #[test]
    fn interrupted_staging_and_refused_inputs_preserve_selected_cache() {
        let dir = tempfile::tempdir().unwrap();
        Metadata::seed().write(dir.path()).unwrap();
        let before = std::fs::read(dir.path().join(FILE_NAME)).unwrap();
        let mut staged = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        staged.write_all(b"partial uncommitted snapshot").unwrap();
        assert!(Metadata::read(dir.path()).unwrap().is_some());
        drop(staged);
        let mut oversized = Metadata::seed();
        oversized.revocations.signature = "x".repeat(SIGNATURE_MAX_BYTES as usize + 1);
        assert!(oversized.write(dir.path()).is_err());
        assert_eq!(std::fs::read(dir.path().join(FILE_NAME)).unwrap(), before);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_replacement_keeps_target_and_cleans_only_owned_staging() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(FILE_NAME);
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), b"owned test marker").unwrap();
        assert!(Metadata::seed().write(dir.path()).is_err());
        assert_eq!(
            std::fs::read(target.join("keep")).unwrap(),
            b"owned test marker"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(Metadata::read(dir.path()).is_err());
    }

    #[test]
    fn identity_requires_monotonic_versions_and_exact_same_version_bytes() {
        assert!(check_identity(4, b"one", 4, b"one").is_ok());
        assert!(check_identity(4, b"one", 5, b"two").is_ok());
        assert!(check_identity(4, b"one", 3, b"one").is_err());
        assert!(check_identity(4, b"one", 4, b"two").is_err());
    }
}
