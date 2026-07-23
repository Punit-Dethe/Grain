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
//!   mix-and-match protection.
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
    #[serde(default)]
    pub entries: Vec<IndexEntry>,
}

/// One published extension version (DISTRIBUTION-PLAN §2.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub tier: Tier,
    /// Set ONLY here, by us. The single source of an extension's trust.
    pub trust: Trust,
    #[serde(default)]
    pub capabilities: Vec<String>,
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
