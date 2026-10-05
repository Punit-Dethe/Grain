//! Provisional source-pointer submissions. No trust, credentials or build
//! authority comes from this metadata; E4 verifies the pinned source/build.
use serde::{Deserialize, Serialize};

use crate::distribution::ArtifactKind;

pub const SUBMISSION_SCHEMA: u8 = 1;
pub const SUBMISSION_MAX_BYTES: usize = 32 * 1024;
pub const DESCRIPTION_MAX_BYTES: usize = 64 * 1024;
pub const LISTING_MEDIA_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const LISTING_TOTAL_MAX_BYTES: usize = 16 * 1024 * 1024;
pub const LISTING_MEDIA_MAX_COUNT: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListingAsset {
    /// A single basename under the source project's media/ directory.
    pub name: String,
    pub sha256: String,
    pub size: u64,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSubmission {
    pub schema: u8,
    pub artifact_kind: ArtifactKind,
    pub id: String,
    pub version: String,
    pub grain_api: String,
    pub source_repo: String,
    pub tag: String,
    pub commit: String,
    pub summary: String,
    pub categories: Vec<String>,
    pub license: String,
    pub contact: String,
    pub description_sha256: String,
    pub description_size: u64,
    #[serde(default)]
    pub media: Vec<ListingAsset>,
}

impl SourceSubmission {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SUBMISSION_SCHEMA {
            return Err("Unsupported submission schema; migrate legacy source pointers.".into());
        }
        crate::validate_extension_id(&self.id)?;
        crate::validate_extension_version(&self.version)?;
        if (self.artifact_kind == ArtifactKind::McpDescriptor
            && self.grain_api != crate::compatibility::EXTENSION_API_REQUIREMENT)
            || (self.artifact_kind == ArtifactKind::Native
                && !crate::compatibility::api_requirement_supported(
                    &self.grain_api,
                    crate::GRAIN_API_VERSION,
                ))
        {
            return Err("Unsupported submission Grain API profile.".into());
        }
        // Explicit current registry source profile; no credential/query/fragment,
        // URL rewriting or shell execution. Ownership/tag-to-commit is E4's job.
        let path = self
            .source_repo
            .strip_prefix("https://github.com/")
            .ok_or("source_repo must be a canonical HTTPS GitHub owner/repository URL.")?;
        let parts: Vec<_> = path.split('/').collect();
        if parts.len() != 2
            || !repo_part(parts[0], 39, false)
            || !repo_part(parts[1], 100, true)
            || parts[1].ends_with(".git")
        {
            return Err(
                "source_repo must be a canonical HTTPS GitHub owner/repository URL.".into(),
            );
        }
        if !tag_supported(&self.tag) {
            return Err(
                "tag must be a supported literal Git tag name, not a revision expression.".into(),
            );
        }
        if self.commit.len() != 40 || !hex(&self.commit) || self.commit.bytes().all(|b| b == b'0') {
            return Err("commit must be a full nonzero lowercase 40-character commit ID.".into());
        }
        for (field, value, max) in [
            ("summary", &self.summary, 2048),
            ("license", &self.license, 128),
            ("contact", &self.contact, 320),
        ] {
            if !display(value, max) {
                return Err(format!("{field} must contain bounded visible text."));
            }
        }
        if self.categories != ["tools"] {
            return Err("Tools-only submissions require exactly the tools category.".into());
        }
        validate_listing_metadata(&self.description_sha256, self.description_size, &self.media)
    }
}

pub fn validate_listing_metadata(
    description_sha256: &str,
    description_size: u64,
    media: &[ListingAsset],
) -> Result<(), String> {
    if !hex256(description_sha256)
        || description_size == 0
        || description_size > DESCRIPTION_MAX_BYTES as u64
    {
        return Err("DESCRIPTION.md requires its bounded size and SHA256 digest.".into());
    }
    if media.len() > LISTING_MEDIA_MAX_COUNT {
        return Err("Too many listing media assets.".into());
    }
    let mut total = description_size;
    let mut names = std::collections::HashSet::new();
    let mut cover = false;
    for asset in media {
        let valid_name = !asset.name.is_empty()
            && asset.name.len() <= 100
            && asset
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            && !asset.name.starts_with('.')
            && !asset.name.contains("..");
        if !valid_name
            || !matches!(asset.kind.as_str(), "webp" | "gif")
            || !asset.name.ends_with(&format!(".{}", asset.kind))
            || !hex256(&asset.sha256)
            || asset.size == 0
            || asset.size > LISTING_MEDIA_MAX_BYTES as u64
            || !names.insert(asset.name.to_ascii_lowercase())
        {
            return Err("Invalid, duplicate or oversized listing media asset.".into());
        }
        if asset.name.to_ascii_lowercase().starts_with("cover.") {
            if cover {
                return Err("Only one cover asset is allowed.".into());
            }
            cover = true;
        }
        total += asset.size;
    }
    if total > LISTING_TOTAL_MAX_BYTES as u64 {
        return Err("Listing exceeds its total byte budget.".into());
    }
    Ok(())
}

fn repo_part(value: &str, max: usize, repository: bool) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || (repository && b"_.".contains(&b)))
        && !value.starts_with('-')
        && !value.ends_with('-')
}
fn tag_supported(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 128
        && !tag.starts_with('-')
        && !tag.ends_with('.')
        && !tag.contains("..")
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
        && tag
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}
fn display(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value == value.trim()
        && !value.chars().any(|ch| {
            ch.is_control()
                || matches!(ch as u32, 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0xFEFF)
        })
}
fn hex(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn hex256(value: &str) -> bool {
    value.len() == 64 && hex(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn sample() -> SourceSubmission {
        SourceSubmission {
            schema: 1,
            artifact_kind: ArtifactKind::McpDescriptor,
            id: "com.example.remote".into(),
            version: "1.0.0".into(),
            grain_api: "^1.0".into(),
            source_repo: "https://github.com/example/remote-tools".into(),
            tag: "release/v1.0.0".into(),
            commit: "1".repeat(40),
            summary: "Remote tools".into(),
            categories: vec!["tools".into()],
            license: "MIT".into(),
            contact: "maintainer@example.com".into(),
            description_sha256: "a".repeat(64),
            description_size: 20,
            media: Vec::new(),
        }
    }
    #[test]
    fn source_pointer_rejects_ambiguous_sources_revisions_and_legacy_fields() {
        let mut value = sample();
        assert!(value.validate().is_ok());
        for repo in [
            "http://github.com/example/tools",
            "https://github.com/user:token@example/tools",
            "https://github.com/example/tools?token=x",
            "https://github.com/example/tools.git",
            "https://github.com/example/tools/../other",
            "https://other.example/tools",
            "https://github.com/example/..",
        ] {
            value.source_repo = repo.into();
            assert!(value.validate().is_err(), "{repo}");
        }
        value = sample();
        for tag in [
            "--upload-pack=evil",
            "v1..v2",
            "v1^{}",
            "/v1",
            "v1/",
            "v1//two",
            ".hidden",
            "v1.lock",
            "v1.",
        ] {
            value.tag = tag.into();
            assert!(value.validate().is_err(), "{tag}");
        }
        value = sample();
        for commit in [
            "abc1234".to_string(),
            "0".repeat(40),
            "A".repeat(40),
            "1".repeat(41),
        ] {
            value.commit = commit;
            assert!(value.validate().is_err());
        }
        let mut raw = serde_json::to_value(sample()).unwrap();
        raw["trust"] = serde_json::json!("core");
        assert!(serde_json::from_value::<SourceSubmission>(raw).is_err());
    }
    #[test]
    fn kind_api_category_and_text_rules_match_the_provisional_contract() {
        let mut value = sample();
        value.grain_api = "1.0".into();
        assert!(value.validate().is_err());
        value.artifact_kind = ArtifactKind::Native;
        assert!(value.validate().is_ok());
        value.categories = vec!["prompts".into()];
        assert!(value.validate().is_err());
        value.categories = vec!["tools".into(), "tools".into()];
        assert!(value.validate().is_err());
        value = sample();
        value.summary = "quote \"and\" backslash \\".into();
        assert!(value.validate().is_ok());
        for summary in [
            "".into(),
            "bad\ncontrol".into(),
            "hidden\u{202e}".into(),
            "a".repeat(2049),
        ] {
            value.summary = summary;
            assert!(value.validate().is_err());
        }
    }
    #[test]
    fn media_metadata_refuses_traversal_duplicate_cover_and_resource_overflow() {
        let mut value = sample();
        let asset = ListingAsset {
            name: "cover.webp".into(),
            sha256: "b".repeat(64),
            size: 123,
            kind: "webp".into(),
        };
        value.media = vec![asset.clone()];
        assert!(value.validate().is_ok());
        for name in [
            "../outside.webp",
            ".hidden.webp",
            "nested/cover.webp",
            "bad.exe",
        ] {
            value.media[0].name = name.into();
            assert!(value.validate().is_err());
        }
        value.media = vec![
            asset.clone(),
            ListingAsset {
                name: "Cover.gif".into(),
                kind: "gif".into(),
                ..asset.clone()
            },
        ];
        assert!(value.validate().is_err());
        value.media = vec![asset.clone(); 7];
        assert!(value.validate().is_err());
        value.media = (0..5)
            .map(|n| ListingAsset {
                name: format!("{n}.webp"),
                size: LISTING_MEDIA_MAX_BYTES as u64,
                ..asset.clone()
            })
            .collect();
        assert!(value.validate().is_err());
        value.media = vec![ListingAsset {
            size: LISTING_MEDIA_MAX_BYTES as u64 + 1,
            ..asset
        }];
        assert!(value.validate().is_err());
    }
}
