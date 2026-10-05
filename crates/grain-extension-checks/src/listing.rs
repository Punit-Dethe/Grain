//! Bounded author listing snapshot. Rendering and external publishing are separate.
use grain_sdk::submission::*;
use sha2::{Digest, Sha256};
use std::{fs, io::Cursor, path::Path};

pub struct Listing {
    pub description: String,
    pub sha256: String,
    pub media: Vec<ListingAsset>,
}

pub fn read_listing(root: &Path) -> Result<Listing, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("open listing root: {e}"))?;
    reject_link(&root.join("DESCRIPTION.md"))?;
    let path = super::safe_project_file(&root, "DESCRIPTION.md", "listing description")?;
    reject_link(&path)?;
    let description =
        super::read_bounded_utf8(&path, DESCRIPTION_MAX_BYTES as u64, "DESCRIPTION.md")?;
    validate_description(&description)?;
    let mut media = Vec::new();
    let mut total = description.len();
    let directory = root.join("media");
    if directory.try_exists().map_err(|e| e.to_string())? {
        reject_link(&directory)?;
        if !directory.is_dir() {
            return Err("media must be a directory.".into());
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if files.len() >= LISTING_MEDIA_MAX_COUNT {
                return Err("Too many listing media assets.".into());
            }
            files.push(entry.path());
        }
        files.sort_by_key(|p| {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (!name.to_ascii_lowercase().starts_with("cover."), name)
        });
        for path in files {
            reject_link(&path)?;
            if !path.is_file() {
                return Err(
                    "media must contain only supported image files, without nested directories."
                        .into(),
                );
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("media filename must be UTF-8")?
                .to_string();
            let kind = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let format = match kind {
                "webp" => image::ImageFormat::WebP,
                "gif" => image::ImageFormat::Gif,
                _ => {
                    return Err(
                        "Listing media supports lowercase .webp and .gif files only.".into(),
                    )
                }
            };
            let bytes =
                super::read_bounded(&path, LISTING_MEDIA_MAX_BYTES as u64, "listing media")?;
            total += bytes.len();
            if total > LISTING_TOTAL_MAX_BYTES {
                return Err("Listing exceeds its total byte budget.".into());
            }
            let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(2048);
            limits.max_image_height = Some(2048);
            limits.max_alloc = Some(16 * 1024 * 1024);
            reader.limits(limits);
            reader.decode().map_err(|_| {
                "Listing image is invalid or exceeds its 2048px/16MiB decode limit."
            })?;
            media.push(ListingAsset {
                name,
                sha256: digest(&bytes),
                size: bytes.len() as u64,
                kind: kind.into(),
            });
        }
    }
    let listing = Listing {
        sha256: digest(description.as_bytes()),
        description,
        media,
    };
    validate_listing_metadata(
        &listing.sha256,
        listing.description.len() as u64,
        &listing.media,
    )?;
    Ok(listing)
}

fn reject_link(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("inspect listing path: {e}"))?;
    if metadata.file_type().is_symlink() {
        return Err("Listing files/directories must not be symbolic links.".into());
    }
    Ok(())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_description(description: &str) -> Result<(), String> {
    if description.trim().is_empty()
        || description.chars().any(|c| {
            (c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                || super::forbidden_unicode_name(c).is_some()
        })
    {
        return Err("DESCRIPTION.md must contain visible, bounded UTF-8 Markdown.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn listing() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("DESCRIPTION.md"),
            "# Listing\n\nUser-facing description.\n",
        )
        .unwrap();
        root
    }
    fn image(path: &Path, width: u32, format: image::ImageFormat) {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(width, 1))
            .write_to(&mut bytes, format)
            .unwrap();
        fs::write(path, bytes.into_inner()).unwrap();
    }
    #[test]
    fn description_is_distinct_and_real_media_is_bounded_sorted_and_hashed() {
        let root = listing();
        fs::write(
            root.path().join("README.md"),
            "Developer instructions are different.",
        )
        .unwrap();
        fs::create_dir(root.path().join("media")).unwrap();
        image(
            &root.path().join("media/z.webp"),
            16,
            image::ImageFormat::WebP,
        );
        image(
            &root.path().join("media/cover.gif"),
            16,
            image::ImageFormat::Gif,
        );
        let value = read_listing(root.path()).unwrap();
        assert!(value.description.contains("User-facing"));
        assert_eq!(value.sha256, digest(value.description.as_bytes()));
        assert_eq!(
            value
                .media
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            ["cover.gif", "z.webp"]
        );
        for asset in value.media {
            let bytes = fs::read(root.path().join("media").join(asset.name)).unwrap();
            assert_eq!(asset.sha256, digest(&bytes));
            assert_eq!(asset.size, bytes.len() as u64);
        }
    }
    #[test]
    fn invalid_empty_oversized_description_and_image_data_are_refused() {
        let root = listing();
        for bytes in [
            vec![0xff],
            b"  \n".to_vec(),
            vec![b'x'; DESCRIPTION_MAX_BYTES + 1],
            b"hidden\xe2\x80\xae".to_vec(),
        ] {
            fs::write(root.path().join("DESCRIPTION.md"), bytes).unwrap();
            assert!(read_listing(root.path()).is_err());
        }
        fs::write(root.path().join("DESCRIPTION.md"), "# Valid").unwrap();
        fs::create_dir(root.path().join("media")).unwrap();
        let path = root.path().join("media/cover.webp");
        fs::write(&path, b"not an image").unwrap();
        assert!(read_listing(root.path()).is_err());
        image(&path, 2049, image::ImageFormat::WebP);
        assert!(read_listing(root.path()).is_err());
        fs::write(&path, vec![b'x'; LISTING_MEDIA_MAX_BYTES + 1]).unwrap();
        assert!(read_listing(root.path()).is_err());
    }
    #[test]
    fn unexpected_nested_and_excess_media_are_refused() {
        let root = listing();
        let media = root.path().join("media");
        fs::create_dir(&media).unwrap();
        fs::create_dir(media.join("nested")).unwrap();
        assert!(read_listing(root.path()).is_err());
        fs::remove_dir(media.join("nested")).unwrap();
        fs::write(media.join("script.svg"), b"<svg/>").unwrap();
        assert!(read_listing(root.path()).is_err());
        fs::remove_file(media.join("script.svg")).unwrap();
        for n in 0..7 {
            image(&media.join(format!("{n}.gif")), 16, image::ImageFormat::Gif);
        }
        assert!(read_listing(root.path()).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn listing_symlinks_are_refused_even_inside_the_project() {
        let root = listing();
        fs::rename(
            root.path().join("DESCRIPTION.md"),
            root.path().join("real.md"),
        )
        .unwrap();
        std::os::unix::fs::symlink("real.md", root.path().join("DESCRIPTION.md")).unwrap();
        assert!(read_listing(root.path()).is_err());
    }
}

/// Validate a submitted review description against its source snapshot. This
/// proves byte identity only, never Markdown render safety or source ownership.
pub fn verify_submission_listing(root: &Path, submission: &SourceSubmission) -> Result<(), String> {
    let path = root.join("DESCRIPTION.md");
    reject_link(&path)?;
    let bytes = super::read_bounded(&path, DESCRIPTION_MAX_BYTES as u64, "DESCRIPTION.md")?;
    validate_description(std::str::from_utf8(&bytes).map_err(|_| "DESCRIPTION.md must be UTF-8")?)?;
    if bytes.len() as u64 != submission.description_size
        || digest(&bytes) != submission.description_sha256
    {
        return Err("DESCRIPTION.md differs from the submitted source snapshot.".into());
    }
    Ok(())
}
