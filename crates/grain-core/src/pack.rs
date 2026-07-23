//! Phase 5A pack format v2 + **path-safe extraction** (DISTRIBUTION-PLAN §5.1,
//! §5.2, correction C-5).
//!
//! A `.grainpack` has two physical shapes, detected by the first byte:
//!
//! | Shape | First byte | Contents | Tier |
//! |---|---|---|---|
//! | JSON | `{` | manifest with embedded payloads (today's format, unchanged) | `pack` |
//! | ZIP  | `PK` | `manifest.json`, `entry.js`, assets, per-platform binaries | `scripted`, `native` |
//!
//! One hash over the whole artifact binds every byte (we build the artifact, so
//! per-file hashes would re-answer a question the index signature already
//! answers). What is **not** optional is path-safe extraction: Zed shipped a
//! CVSS 7.4 Zip Slip in exactly this code (F-8). Every rejection below has a
//! test, and the tests were written before the extractor.

use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

/// The physical shape of a `.grainpack`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackShape {
    /// A single JSON document (tier `pack`).
    Json,
    /// A ZIP archive (tiers `scripted` / `native`).
    Zip,
    /// Neither `{` nor `PK` — not a recognised pack.
    Unknown,
}

/// Detect the shape from the leading bytes (DISTRIBUTION-PLAN §5.1).
pub fn detect_shape(bytes: &[u8]) -> PackShape {
    match bytes.first() {
        Some(b'{') => PackShape::Json,
        Some(b'P') if bytes.get(1) == Some(&b'K') => PackShape::Zip,
        _ => PackShape::Unknown,
    }
}

/// Extraction budgets. Defaults are generous for a real extension yet reject the
/// zip-bomb classes (DISTRIBUTION-PLAN §5.2).
#[derive(Debug, Clone, Copy)]
pub struct ExtractLimits {
    pub max_entries: u32,
    pub max_entry_size: u64,
    pub max_total_size: u64,
    /// Max uncompressed:compressed ratio for any single entry.
    pub max_ratio: u64,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        ExtractLimits {
            max_entries: 4_096,
            max_entry_size: 25 * 1024 * 1024, // 25 MiB per file
            max_total_size: 100 * 1024 * 1024, // 100 MiB total
            max_ratio: 200,
        }
    }
}

/// Why an archive was refused. Each variant is a distinct, testable class — no
/// path collapses into a bare error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// Not a ZIP (first bytes are not `PK`).
    NotZip,
    /// The ZIP central directory could not be read.
    BadArchive(String),
    /// An entry name contains a `..` component (Zip Slip).
    Traversal(String),
    /// An entry name is an absolute path.
    AbsolutePath(String),
    /// An entry name carries a Windows drive prefix (`C:\`).
    DriveLetter(String),
    /// An entry is a symlink (could point outside the destination).
    Symlink(String),
    /// After joining, the path would escape the destination.
    Escapes(String),
    /// More entries than [`ExtractLimits::max_entries`].
    TooManyEntries { limit: u32 },
    /// A single entry exceeds [`ExtractLimits::max_entry_size`].
    EntryTooLarge { name: String, limit: u64 },
    /// Total uncompressed size exceeds [`ExtractLimits::max_total_size`].
    TotalTooLarge { limit: u64 },
    /// A single entry's compression ratio exceeds [`ExtractLimits::max_ratio`].
    RatioExceeded { name: String, limit: u64 },
    /// A filesystem error during staging.
    Io(String),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::NotZip => write!(f, "not a ZIP archive"),
            PackError::BadArchive(e) => write!(f, "unreadable archive: {e}"),
            PackError::Traversal(n) => write!(f, "path traversal in entry '{n}'"),
            PackError::AbsolutePath(n) => write!(f, "absolute path in entry '{n}'"),
            PackError::DriveLetter(n) => write!(f, "drive-letter path in entry '{n}'"),
            PackError::Symlink(n) => write!(f, "symlink entry '{n}' rejected"),
            PackError::Escapes(n) => write!(f, "entry '{n}' escapes the destination"),
            PackError::TooManyEntries { limit } => write!(f, "too many entries (limit {limit})"),
            PackError::EntryTooLarge { name, limit } => {
                write!(f, "entry '{name}' exceeds {limit} bytes")
            }
            PackError::TotalTooLarge { limit } => write!(f, "total size exceeds {limit} bytes"),
            PackError::RatioExceeded { name, limit } => {
                write!(f, "entry '{name}' compression ratio exceeds {limit}:1")
            }
            PackError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for PackError {}

/// Validate a single entry name against the traversal classes, returning a safe
/// relative path *within* `dest`. Pure and independently tested.
fn safe_relative_path(dest: &Path, raw_name: &str) -> Result<PathBuf, PackError> {
    // Normalise separators so a `\`-using archive on Unix is still inspected.
    let normalised = raw_name.replace('\\', "/");

    // Windows drive prefix like `C:` or `c:/...`.
    let bytes = normalised.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Err(PackError::DriveLetter(raw_name.to_string()));
    }
    // Absolute path.
    if normalised.starts_with('/') {
        return Err(PackError::AbsolutePath(raw_name.to_string()));
    }

    let rel = Path::new(&normalised);
    let mut safe = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::ParentDir => return Err(PackError::Traversal(raw_name.to_string())),
            Component::RootDir | Component::Prefix(_) => {
                return Err(PackError::AbsolutePath(raw_name.to_string()))
            }
            Component::CurDir => {}
            Component::Normal(part) => safe.push(part),
        }
    }

    // Belt and braces: the lexically joined path must stay under dest.
    let joined = dest.join(&safe);
    if !lexically_within(dest, &joined) {
        return Err(PackError::Escapes(raw_name.to_string()));
    }
    Ok(safe)
}

/// Lexical containment check that does not require the paths to exist (so it is
/// safe to run before anything is written).
fn lexically_within(base: &Path, candidate: &Path) -> bool {
    let base_norm = normalise_lexical(base);
    let cand_norm = normalise_lexical(candidate);
    cand_norm.starts_with(&base_norm)
}

fn normalise_lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Extract a ZIP-shaped `.grainpack` into `dest` (which must already exist),
/// enforcing the path-safety invariants and budgets. The caller stages `dest`
/// in a temp dir and atomically renames on success (§5.2).
pub fn extract_zip(bytes: &[u8], dest: &Path, limits: ExtractLimits) -> Result<(), PackError> {
    if detect_shape(bytes) != PackShape::Zip {
        return Err(PackError::NotZip);
    }
    let reader = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| PackError::BadArchive(e.to_string()))?;

    if archive.len() as u64 > limits.max_entries as u64 {
        return Err(PackError::TooManyEntries {
            limit: limits.max_entries,
        });
    }

    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| PackError::BadArchive(e.to_string()))?;
        let raw_name = file.name().to_string();

        // Reject symlinks by unix mode bits (S_IFLNK == 0o120000).
        if let Some(mode) = file.unix_mode() {
            if mode & 0o170000 == 0o120000 {
                return Err(PackError::Symlink(raw_name));
            }
        }

        let rel = safe_relative_path(dest, &raw_name)?;

        // Directory entry: create and continue.
        if raw_name.ends_with('/') {
            let dir = dest.join(&rel);
            std::fs::create_dir_all(&dir).map_err(|e| PackError::Io(e.to_string()))?;
            continue;
        }

        let uncompressed = file.size();
        let compressed = file.compressed_size().max(1);
        if uncompressed > limits.max_entry_size {
            return Err(PackError::EntryTooLarge {
                name: raw_name,
                limit: limits.max_entry_size,
            });
        }
        if uncompressed / compressed > limits.max_ratio {
            return Err(PackError::RatioExceeded {
                name: raw_name,
                limit: limits.max_ratio,
            });
        }
        total = total.saturating_add(uncompressed);
        if total > limits.max_total_size {
            return Err(PackError::TotalTooLarge {
                limit: limits.max_total_size,
            });
        }

        let out_path = dest.join(&rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PackError::Io(e.to_string()))?;
        }
        // Read with a hard cap so a lying header cannot exhaust memory.
        let mut buf = Vec::new();
        file.take(limits.max_entry_size + 1)
            .read_to_end(&mut buf)
            .map_err(|e| PackError::Io(e.to_string()))?;
        if buf.len() as u64 > limits.max_entry_size {
            return Err(PackError::EntryTooLarge {
                name: raw_name,
                limit: limits.max_entry_size,
            });
        }
        std::fs::write(&out_path, &buf).map_err(|e| PackError::Io(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn zip_with<F: FnOnce(&mut zip::ZipWriter<Cursor<Vec<u8>>>)>(build: F) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        build(&mut w);
        w.finish().unwrap().into_inner()
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn detects_shape_by_first_byte() {
        assert_eq!(detect_shape(b"{\"a\":1}"), PackShape::Json);
        assert_eq!(detect_shape(b"PK\x03\x04"), PackShape::Zip);
        assert_eq!(detect_shape(b"nope"), PackShape::Unknown);
        assert_eq!(detect_shape(b""), PackShape::Unknown);
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        let bytes = zip_with(|w| {
            w.start_file("../evil.txt", SimpleFileOptions::default()).unwrap();
            w.write_all(b"pwned").unwrap();
        });
        let d = tmp();
        assert!(matches!(
            extract_zip(&bytes, d.path(), ExtractLimits::default()),
            Err(PackError::Traversal(_))
        ));
    }

    #[test]
    fn rejects_nested_traversal() {
        let bytes = zip_with(|w| {
            w.start_file("a/b/../../../evil.txt", SimpleFileOptions::default()).unwrap();
            w.write_all(b"x").unwrap();
        });
        let d = tmp();
        assert!(matches!(
            extract_zip(&bytes, d.path(), ExtractLimits::default()),
            Err(PackError::Traversal(_))
        ));
    }

    #[test]
    fn rejects_absolute_path() {
        let bytes = zip_with(|w| {
            w.start_file("/etc/passwd", SimpleFileOptions::default()).unwrap();
            w.write_all(b"x").unwrap();
        });
        let d = tmp();
        assert!(matches!(
            extract_zip(&bytes, d.path(), ExtractLimits::default()),
            Err(PackError::AbsolutePath(_))
        ));
    }

    #[test]
    fn rejects_drive_letter() {
        // Build the safe-path check directly — some zip writers reject a raw
        // `C:\` name, so exercise the classifier that the extractor calls.
        let d = tmp();
        assert!(matches!(
            safe_relative_path(d.path(), "C:\\evil.txt"),
            Err(PackError::DriveLetter(_))
        ));
        assert!(matches!(
            safe_relative_path(d.path(), "d:/evil.txt"),
            Err(PackError::DriveLetter(_))
        ));
    }

    #[test]
    fn rejects_symlink_entry() {
        let bytes = zip_with(|w| {
            w.add_symlink("link", "/etc/passwd", SimpleFileOptions::default()).unwrap();
        });
        let d = tmp();
        assert!(matches!(
            extract_zip(&bytes, d.path(), ExtractLimits::default()),
            Err(PackError::Symlink(_))
        ));
    }

    #[test]
    fn rejects_too_many_entries() {
        let bytes = zip_with(|w| {
            for i in 0..10 {
                w.start_file(format!("f{i}.txt"), SimpleFileOptions::default()).unwrap();
                w.write_all(b"x").unwrap();
            }
        });
        let limits = ExtractLimits {
            max_entries: 5,
            ..Default::default()
        };
        let d = tmp();
        assert!(matches!(
            extract_zip(&bytes, d.path(), limits),
            Err(PackError::TooManyEntries { .. })
        ));
    }

    #[test]
    fn rejects_entry_too_large() {
        let bytes = zip_with(|w| {
            w.start_file("big.bin", SimpleFileOptions::default()).unwrap();
            w.write_all(&vec![7u8; 4096]).unwrap();
        });
        let limits = ExtractLimits {
            max_entry_size: 1024,
            ..Default::default()
        };
        let d = tmp();
        assert!(matches!(
            extract_zip(&bytes, d.path(), limits),
            Err(PackError::EntryTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_total_too_large() {
        let bytes = zip_with(|w| {
            for i in 0..4 {
                w.start_file(format!("f{i}.bin"), SimpleFileOptions::default()).unwrap();
                w.write_all(&vec![3u8; 4096]).unwrap();
            }
        });
        let limits = ExtractLimits {
            max_entry_size: 8192,
            max_total_size: 8192,
            ..Default::default()
        };
        let d = tmp();
        assert!(matches!(
            extract_zip(&bytes, d.path(), limits),
            Err(PackError::TotalTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_ratio_bomb() {
        // Highly compressible: 1 MiB of zeros deflates to almost nothing.
        let bytes = zip_with(|w| {
            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            w.start_file("bomb.bin", opts).unwrap();
            w.write_all(&vec![0u8; 1024 * 1024]).unwrap();
        });
        let limits = ExtractLimits {
            max_ratio: 50,
            max_entry_size: 8 * 1024 * 1024,
            max_total_size: 8 * 1024 * 1024,
            ..Default::default()
        };
        let d = tmp();
        assert!(matches!(
            extract_zip(&bytes, d.path(), limits),
            Err(PackError::RatioExceeded { .. })
        ));
    }

    #[test]
    fn good_archive_round_trips() {
        let bytes = zip_with(|w| {
            w.start_file("manifest.json", SimpleFileOptions::default()).unwrap();
            w.write_all(b"{\"id\":\"com.example.x\"}").unwrap();
            w.add_directory("assets/", SimpleFileOptions::default()).unwrap();
            w.start_file("assets/icon.txt", SimpleFileOptions::default()).unwrap();
            w.write_all(b"icon").unwrap();
        });
        let d = tmp();
        extract_zip(&bytes, d.path(), ExtractLimits::default()).expect("good archive");
        assert_eq!(
            std::fs::read_to_string(d.path().join("manifest.json")).unwrap(),
            "{\"id\":\"com.example.x\"}"
        );
        assert_eq!(
            std::fs::read_to_string(d.path().join("assets/icon.txt")).unwrap(),
            "icon"
        );
    }
}
