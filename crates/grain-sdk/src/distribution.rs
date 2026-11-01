//! Distribution metadata — the signed catalogue the store reads (SPEC §7.4,
//! DISTRIBUTION-PLAN §2.1).
//!
//! **Types only — no crypto lives here.** `grain-sdk` is the dependency leaf
//! (SPEC §7.3): it may describe the *shape* of `roots.json` / `index.json` /
//! `revocations.json`, but the code that runs Ed25519/SHA-256 lives in
//! `grain-core` (`trust` module), so the leaf stays free of a crypto crate.
//!
//! Three files, each a JSON document with a detached minisign (`.minisig`)
//! signature that travels beside it:
//!
//! - [`Roots`] — signed by an offline **root** key. Carries the publishing
//!   public key and the absolute base URLs, so moving hosts is a signed
//!   metadata change, not an app release. The root *public* keys are pinned in
//!   the binary; this file is verified against them.
//! - [`Index`] — the whole catalogue, signed by the **publishing** key named in
//!   [`Roots`]. One signature over the whole file gives rollback, freeze and
//!   catalogue integrity. Its [`MetadataGeneration`] binds exact companion JSON
//!   bytes; new hosted acquisitions require a matching authenticated generation.
//! - [`Revocations`] — the signed kill switch, also publishing-key-signed.
//!
//! Unknown JSON fields are ignored on read (forward compatibility): a client
//! reads the subset of a newer document it understands.

use serde::{Deserialize, Serialize};

use crate::manifest::Tier;

/// The `spec` major this client understands. An `Index`/`Roots`/`Revocations`
/// carrying a higher major is **a message, not an error** — the store shows
/// "update Grain to browse the store" (DISTRIBUTION-PLAN §2.1).
pub const DISTRIBUTION_SPEC: u32 = 1;

/// Generous clock-skew allowance for `expires` comparisons, in seconds
/// (DISTRIBUTION-PLAN §2.1: a wrong system clock must not brick the store).
pub const EXPIRY_CLOCK_SKEW_SECS: i64 = 24 * 60 * 60;

/// The trust rung shown on a store card. **The only place trust is ever set is
/// verified index metadata** (DISTRIBUTION-PLAN §3.2): the manifest has no
/// trust field, the installer cannot read one from pack bytes, and trust is
/// bound to `(id, version, sha256)`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Trust {
    /// Loaded from a local folder in developer mode. Client-only — **never
    /// appears in a published index**; never promotable.
    Dev,
    /// Automation passed but no human read it. **Reserved and currently
    /// unreachable** (DISTRIBUTION-PLAN §3.1): the policy is off. The value
    /// exists so switching it on later is a 5B policy change, not a redesign.
    Experimental,
    /// A human read this exact version's source. The store's baseline.
    Verified,
    /// Written and maintained by us, built from the `grain` repo by our own CI.
    Core,
}

impl Trust {
    /// The trust a pack installs with when it did **not** come from a verified
    /// index entry (e.g. a local file the user imported directly). Never
    /// `verified`/`core` — that is the anti-forgery guarantee.
    pub const UNTRUSTED_DEFAULT: Trust = Trust::Dev;
}

/// The signed catalogue envelope (DISTRIBUTION-PLAN §2.1). One signature covers
/// this whole document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Index {
    /// Format major. A higher value than [`DISTRIBUTION_SPEC`] means "update
    /// Grain", not "reject".
    pub spec: u32,
    /// Monotonic. A refresh whose `version` is lower than the stored copy is a
    /// rollback attack and is rejected.
    pub version: u64,
    /// RFC 3339. Past `expires` → keep serving the cached copy, mark the store
    /// offline, refuse *new* installs. The seed shipped in the app is exempt
    /// until the first successful refresh.
    pub expires: String,
    /// Exact companion document hashes covered by the catalogue signature.
    /// Required for hosted acquisition; only the embedded offline seed omits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<MetadataGeneration>,
    #[serde(default)]
    pub entries: Vec<IndexEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetadataGeneration {
    pub roots_sha256: String,
    pub revocations_sha256: String,
}

/// One published extension version (DISTRIBUTION-PLAN §2.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Signed artifact format. The native package encoding omits this field;
    /// unknown formats fail parsing rather than entering the native loader.
    #[serde(default, skip_serializing_if = "ArtifactKind::is_native")]
    pub artifact_kind: ArtifactKind,
    pub id: String,
    pub name: String,
    pub version: String,
    pub tier: Tier,
    /// Set ONLY here, by us. The single source of an extension's trust.
    pub trust: Trust,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// One-line summary, from the manifest — shown on the store card so a name
    /// alone never has to carry the install decision (DISTRIBUTION-PLAN §2.3).
    #[serde(default)]
    pub description: String,
    /// SHA-256 of the artifact, lowercase hex. Bound to trust with `(id,
    /// version, sha256)`.
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    /// Minimum contract semver the running app must satisfy. A card whose
    /// requirement exceeds the app greys with "needs Grain x.y" rather than
    /// hiding.
    #[serde(default, rename = "min_grain_api")]
    pub min_grain_api: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub source_commit: String,
    /// The GitHub account that submitted it.
    #[serde(default)]
    pub author: String,
    /// What a human actually read — shown on the card.
    #[serde(default)]
    pub reviewed_at: String,
    #[serde(default)]
    pub reviewed_commit: String,
    #[serde(default)]
    pub updated_at: String,
    /// Fetched by CI, never by the client.
    #[serde(default)]
    pub stars: u64,
    /// Install count — the popularity signal shown on the card and detail page.
    /// Maintained by the publish pipeline (like `stars`), never written by the
    /// client: it reads this number straight from the signed index it already
    /// fetched, so there is no per-card query and nothing phoned home.
    #[serde(default)]
    pub installs: u64,
    /// User-facing DESCRIPTION document, fetched lazily and dropped on close.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listing: Option<ListingDocument>,
    /// Square listing identity. Never substituted with a gallery cover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<MediaRef>,
    /// Wide gallery media, referenced by hash. First image is the store-card
    /// cover; remaining detail previews load only when selected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<MediaRef>,
    /// What KIND of thing this is, from the submission's declared categories
    /// (see [`CATEGORIES`]). Carried in the signed index so the store can filter
    /// without fetching anything per card.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    #[default]
    Native,
    McpDescriptor,
}

impl ArtifactKind {
    fn is_native(&self) -> bool {
        *self == Self::Native
    }
}

impl IndexEntry {
    /// Validate DESCRIPTION and its signed, bounded media references.
    pub fn validate_listing(&self) -> Result<(), String> {
        let Some(document) = &self.listing else {
            return if self.media.is_empty() && self.icon.is_none() {
                Ok(())
            } else {
                Err("Listing media requires a DESCRIPTION document.".into())
            };
        };
        if self.media.len() > crate::submission::LISTING_MEDIA_MAX_COUNT {
            return Err("Too many listing media assets.".into());
        }
        if self
            .media
            .iter()
            .any(|asset| !matches!(asset.kind.as_str(), "webp" | "gif") || asset.sha256.len() != 64)
        {
            return Err("Invalid listing media digest/kind.".into());
        }
        let mut media = self
            .media
            .iter()
            .enumerate()
            .map(|(index, asset)| crate::submission::ListingAsset {
                // Original filenames are author/review metadata. Catalogue blobs
                // are addressed only by digest, in the reviewed presentation order.
                name: format!("image{index}.{}", asset.kind),
                sha256: asset.sha256.clone(),
                size: asset.size,
                kind: asset.kind.clone(),
            })
            .collect::<Vec<_>>();
        if let Some(icon) = &self.icon {
            media.push(crate::submission::ListingAsset {
                name: crate::submission::LISTING_ICON_NAME.into(),
                sha256: icon.sha256.clone(),
                size: icon.size,
                kind: icon.kind.clone(),
            });
        }
        crate::submission::validate_listing_metadata(&document.sha256, document.size, &media)
    }

    pub fn detail_document_hash(&self) -> &str {
        self.listing
            .as_ref()
            .map_or("", |document| document.sha256.as_str())
    }

    /// Cheap catalogue eligibility check, before any artifact or media fetch.
    /// Signed metadata does not replace validation of the downloaded manifest.
    pub fn validate_tool_only(&self) -> Result<(), String> {
        self.validate_listing()?;
        if self.artifact_kind != ArtifactKind::Native {
            return Err("MCP descriptors require the MCP acquisition path.".into());
        }
        if let Some(permission) = self
            .capabilities
            .iter()
            .find(|permission| !crate::manifest::tool_permission_allowed(permission))
        {
            return Err(format!(
                "Retired or unsupported extension permission: {permission}"
            ));
        }
        if self.categories.iter().any(|category| category != "tools") {
            return Err("Only the tools extension category is supported.".into());
        }
        Ok(())
    }

    /// Installation eligibility, before downloading or staging an artifact.
    /// A future API requirement does not hide an otherwise valid tool card.
    pub fn validate_installable(&self) -> Result<(), String> {
        self.validate_tool_only()?;
        if !crate::compatibility::minimum_api_supported(
            &self.min_grain_api,
            crate::GRAIN_API_VERSION,
        ) {
            return Err(crate::compatibility::UNSUPPORTED_EXTENSION_API.into());
        }
        Ok(())
    }

    /// MCP metadata cannot declare native permissions or host contributions.
    pub fn validate_mcp_installable(&self) -> Result<(), String> {
        self.validate_listing()?;
        if self.artifact_kind != ArtifactKind::McpDescriptor
            || !matches!(self.trust, Trust::Verified | Trust::Core)
            || !self.capabilities.is_empty()
            || self.categories.iter().any(|category| category != "tools")
            || self.size == 0
            || self.size > crate::mcp::MCP_DESCRIPTOR_MAX_BYTES as u64
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|ch| ch.is_ascii_digit() || (b'a'..=b'f').contains(&ch))
        {
            return Err("Unsupported MCP catalogue metadata.".into());
        }
        crate::validate_extension_id(&self.id)?;
        crate::validate_extension_version(&self.version)?;
        if !crate::compatibility::minimum_api_supported(
            &self.min_grain_api,
            crate::GRAIN_API_VERSION,
        ) {
            return Err(crate::compatibility::UNSUPPORTED_EXTENSION_API.into());
        }
        Ok(())
    }
}

/// The category vocabulary a submission may declare. Deliberately short: these
/// answer "what kind of thing is this", which is the only question a filter row
/// can usefully ask before you have opened anything. A longer taxonomy would
/// need the user to already know the catalogue to use it.
pub const CATEGORIES: &[(&str, &str)] = &[("tools", "Adds tools to Grain.")];

/// True when `c` is one of [`CATEGORIES`].
pub fn is_category(c: &str) -> bool {
    CATEGORIES.iter().any(|(name, _)| *name == c)
}

/// A separately reviewed user-facing DESCRIPTION, stored in media/<sha256>.md.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListingDocument {
    pub sha256: String,
    pub size: u64,
}

/// One screenshot or GIF in `media/<sha256>.<ext>` (DISTRIBUTION-PLAN §2.3).
/// Content-addressed, so integrity is verified for free; WEBP or GIF only.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MediaRef {
    /// SHA-256 of the media bytes, lowercase hex.
    pub sha256: String,
    /// `webp` | `gif`.
    pub kind: String,
    #[serde(default)]
    pub size: u64,
}

/// Signed by a root key; verified against the keys pinned in the binary. Names
/// the publishing key and where the artifacts live (DISTRIBUTION-PLAN §2.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Roots {
    pub spec: u32,
    pub version: u64,
    /// The minisign public key that signs `index.json` and `revocations.json`.
    pub publishing_key: String,
    /// Absolute base URLs the client fetches from, in order. Relative artifact
    /// paths are resolved against these.
    #[serde(default)]
    pub base_urls: Vec<String>,
    /// Additional fallback mirrors.
    #[serde(default)]
    pub mirrors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// A negative state delivered by the signed kill switch (DISTRIBUTION-PLAN
/// §3.1).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RevocationState {
    /// No new installs; existing installs keep working. For abandonment.
    Deprecated,
    /// The kill switch: disable on next refresh, name the reason, offer
    /// one-click removal. Data is kept unless the user purges it.
    Revoked,
}

/// The signed revocation list.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Revocations {
    pub spec: u32,
    pub version: u64,
    pub expires: String,
    #[serde(default)]
    pub entries: Vec<RevocationEntry>,
}

/// One revocation. A `None` `version` applies to every version of the id.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RevocationEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub state: RevocationState,
    #[serde(default)]
    pub reason: String,
}

impl Revocations {
    /// The strongest state published against this `(id, version)`, if any.
    /// `Revoked` beats `Deprecated`; an all-versions entry (`version: None`)
    /// matches every version.
    pub fn state_for(&self, id: &str, version: &str) -> Option<RevocationState> {
        let mut result: Option<RevocationState> = None;
        for e in &self.entries {
            if e.id != id {
                continue;
            }
            let matches = match &e.version {
                None => true,
                Some(v) => v == version,
            };
            if !matches {
                continue;
            }
            result = Some(match (result, e.state) {
                (Some(RevocationState::Revoked), _) | (_, RevocationState::Revoked) => {
                    RevocationState::Revoked
                }
                _ => RevocationState::Deprecated,
            });
        }
        result
    }
}

#[cfg(test)]
mod tool_catalogue_tests {
    use super::*;

    #[test]
    fn catalog_minimum_api_is_enforced_before_artifact_acquisition() {
        let mut entry = tool_entry();
        for supported in ["0.9", "1.0", "1.0.0"] {
            entry.min_grain_api = supported.into();
            entry.validate_installable().unwrap();
        }
        for unsupported in ["", "1.0.1", "2.0", "^1.0", "latest", "01.0"] {
            entry.min_grain_api = unsupported.into();
            entry.validate_tool_only().unwrap();
            assert_eq!(
                entry.validate_installable().unwrap_err(),
                crate::compatibility::UNSUPPORTED_EXTENSION_API
            );
        }
    }

    fn tool_entry() -> IndexEntry {
        serde_json::from_value(serde_json::json!({
            "id": "com.example.tools", "name": "Tools", "version": "1.0.0",
            "tier": "scripted", "trust": "verified", "sha256": "00", "min_grain_api": "1.0",
            "capabilities": ["storage", "auth", "net:api.example.com"],
            "categories": ["tools"]
        }))
        .unwrap()
    }

    #[test]
    fn description_listing_validates_both_artifact_kinds() {
        let mut entry = tool_entry();
        entry.listing = Some(ListingDocument {
            sha256: "a".repeat(64),
            size: 20,
        });
        entry.media.push(MediaRef {
            sha256: "b".repeat(64),
            size: 30,
            kind: "webp".into(),
        });
        entry.validate_installable().unwrap();
        assert_eq!(entry.detail_document_hash(), "a".repeat(64));
        entry.artifact_kind = ArtifactKind::McpDescriptor;
        entry.capabilities.clear();
        entry.sha256 = "c".repeat(64);
        entry.size = 500;
        entry.min_grain_api = "1.0".into();
        entry.validate_mcp_installable().unwrap();
        let roundtrip: IndexEntry =
            serde_json::from_value(serde_json::to_value(&entry).unwrap()).unwrap();
        assert_eq!(
            roundtrip.detail_document_hash(),
            entry.detail_document_hash()
        );
        assert_eq!(roundtrip.media[0].size, 30);
    }

    #[test]
    fn media_requires_a_bounded_description_listing() {
        let mut entry = tool_entry();
        entry.validate_listing().unwrap();
        assert!(entry.detail_document_hash().is_empty());
        let asset = MediaRef {
            sha256: "b".repeat(64),
            kind: "webp".into(),
            size: 30,
        };
        entry.media.push(asset.clone());
        assert!(entry.validate_listing().is_err());
        entry.media.clear();
        entry.icon = Some(MediaRef {
            kind: "png".into(),
            ..asset
        });
        assert!(entry.validate_listing().is_err());
    }

    #[test]
    fn explicit_listing_refuses_invalid_description_media_and_total_budgets() {
        let mut valid = tool_entry();
        valid.listing = Some(ListingDocument {
            sha256: "a".repeat(64),
            size: 20,
        });
        for size in [0, crate::submission::DESCRIPTION_MAX_BYTES as u64 + 1] {
            let mut invalid = valid.clone();
            invalid.listing.as_mut().unwrap().size = size;
            assert!(invalid.validate_listing().is_err());
        }
        for hash in ["bad".into(), "A".repeat(64)] {
            let mut invalid = valid.clone();
            invalid.listing.as_mut().unwrap().sha256 = hash;
            assert!(invalid.validate_listing().is_err());
        }
        let image = MediaRef {
            sha256: "b".repeat(64),
            kind: "gif".into(),
            size: 4 * 1024 * 1024,
        };
        for media in [
            vec![MediaRef {
                kind: "svg".into(),
                ..image.clone()
            }],
            vec![MediaRef {
                size: 0,
                ..image.clone()
            }],
            vec![MediaRef {
                size: u64::MAX,
                ..image.clone()
            }],
            vec![image.clone(); 4], // Four maximum images plus DESCRIPTION exceed total.
            vec![MediaRef { size: 1, ..image }; 7],
        ] {
            let mut invalid = valid.clone();
            invalid.media = media;
            assert!(invalid.validate_listing().is_err());
        }
        assert!(
            serde_json::from_value::<ListingDocument>(serde_json::json!({
                "sha256": "a".repeat(64), "size": 20, "approved": true
            }))
            .is_err()
        );
    }

    #[test]
    fn tool_catalogue_accepts_only_supported_runtime_metadata() {
        let entry = tool_entry();
        entry.validate_tool_only().unwrap();
        let mut uncategorized = entry.clone();
        uncategorized.categories.clear();
        uncategorized.validate_tool_only().unwrap();
        for permission in [
            "capture",
            "transform:transcript",
            "surface:workspace",
            "context:selection",
            "prompt:layers",
            "net:*",
            "future:unknown",
        ] {
            let mut retired = entry.clone();
            retired.capabilities.push(permission.into());
            assert!(retired.validate_tool_only().is_err(), "{permission}");
        }
        for tier in ["native", "pack", "future-runtime"] {
            let mut wire = serde_json::to_value(&entry).unwrap();
            wire["tier"] = serde_json::json!(tier);
            assert!(
                serde_json::from_value::<IndexEntry>(wire).is_err(),
                "{tier}"
            );
        }
        for category in ["prompts", "dictation", "future-category"] {
            let mut retired = entry.clone();
            retired.categories.push(category.into());
            assert!(retired.validate_tool_only().is_err(), "{category}");
        }
    }

    #[test]
    fn artifact_discriminator_supports_native_and_mcp_and_refuses_unknown_formats() {
        let native = tool_entry();
        assert_eq!(native.artifact_kind, ArtifactKind::Native);
        let mut wire = serde_json::to_value(&native).unwrap();
        assert!(wire.get("artifact_kind").is_none());
        wire["artifact_kind"] = serde_json::json!("future-executable");
        assert!(serde_json::from_value::<IndexEntry>(wire.clone()).is_err());
        wire["artifact_kind"] = serde_json::json!("mcp-descriptor");
        let descriptor: IndexEntry = serde_json::from_value(wire).unwrap();
        assert!(descriptor.validate_installable().is_err());
    }
}
