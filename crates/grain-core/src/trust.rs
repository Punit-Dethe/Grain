//! Phase 5A trust rails — the **verifying** side of the extension registry.
//!
//! This module is the whole reason an author cannot forge trust. It pins the
//! root public keys in the binary, verifies `roots.json` against them, then
//! verifies `index.json` / `revocations.json` against the publishing key that
//! the (now-trusted) `roots.json` names. The producing side — key generation
//! and signing — lives in the separate `grain-registry-tools` crate and never
//! ships in the app.
//!
//! Crypto shape: non-prehashed Ed25519 in minisign format, the same shape
//! Grain's own updater already trusts (C-10). Verification is `minisign-verify`
//! (pure Rust, zero-dependency); hashing is `sha2`.
//!
//! # The client rules, in order (DISTRIBUTION-PLAN §2.1)
//!
//! 1. Verify the signature against the pinned root key (via `roots.json`).
//!    Reject before reading any entry.
//! 2. Reject if `version` is lower than the stored copy (**rollback**).
//! 3. If `expires` has passed: keep the cached copy, mark the store offline,
//!    refuse *new* installs (**indefinite freeze**). The seed is exempt until
//!    the first successful refresh.
//! 4. One signature covers the whole catalogue (**mix-and-match**).

use chrono::DateTime;
use grain_sdk::distribution::{
    Index, Revocations, Roots, EXPIRY_CLOCK_SKEW_SECS,
};
use grain_sdk::DISTRIBUTION_SPEC;
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};

/// Root public key **A**, pinned in the binary. Development key (custody
/// decision, PHASE5A §Step 1); replaced by a passphrase-encrypted key on
/// removable media as the final migration step, which re-pins this constant.
pub const ROOT_PUBKEY_A: &str = "RWS/IbxLWqqJLfHQWl6ZxD+Num5sgD55ozULy2TwgPuKgG0jZlE9GwjA";

/// Root public key **B**, the spare (DISTRIBUTION-PLAN §2.2). Pinned from day
/// one so losing one root drive is an inconvenience, not a re-release.
pub const ROOT_PUBKEY_B: &str = "RWSBLR8dz4PCQU/GDLkq4LC08RH6ei6mfQTjIOSDt8D/VoB8zbrSCbev";

/// Last-resort bootstrap base URL, used only if `roots.json` cannot be fetched
/// and no cached copy exists. Absolute bases normally come from the signed
/// `roots.json`, so moving hosts is a signed-file change, not an app release.
pub const BOOTSTRAP_BASE_URL: &str =
    "https://github.com/Punit-Dethe/grain-extensions/releases/download/v1/";

// ---- The seed, embedded in the binary (DISTRIBUTION-PLAN §5.3) ------------
// First open is instant and offline works. The seed index is expiry-exempt
// until the first successful refresh (see [`verify_index`]).

/// Embedded seed `roots.json` and its detached signature.
pub const SEED_ROOTS: &str = include_str!("../seed/roots.json");
pub const SEED_ROOTS_SIG: &str = include_str!("../seed/roots.json.minisig");
/// Embedded seed `index.json` and its detached signature.
pub const SEED_INDEX: &str = include_str!("../seed/index.json");
pub const SEED_INDEX_SIG: &str = include_str!("../seed/index.json.minisig");
/// Embedded seed `revocations.json` and its detached signature.
pub const SEED_REVOCATIONS: &str = include_str!("../seed/revocations.json");
pub const SEED_REVOCATIONS_SIG: &str = include_str!("../seed/revocations.json.minisig");

/// Why a document was refused. Every variant is a distinct, testable failure —
/// no path collapses into a bare "invalid".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustError {
    /// The detached signature did not verify against the expected key.
    BadSignature,
    /// A pinned/publishing key string could not be parsed.
    BadKey,
    /// The `.minisig` text could not be parsed.
    BadSignatureFormat,
    /// The document bytes are not valid JSON for the target type.
    BadJson(String),
    /// `version` is lower than the copy already stored — a rollback attempt.
    Rollback { stored: u64, offered: u64 },
    /// The document's `expires` timestamp could not be parsed.
    BadExpiry(String),
    /// A computed artifact hash did not match the index entry.
    HashMismatch { expected: String, actual: String },
    /// The `spec` major is newer than this client understands — the caller
    /// should surface "update Grain", not treat this as corruption.
    SpecTooNew { spec: u32 },
}

impl std::fmt::Display for TrustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustError::BadSignature => write!(f, "signature does not verify"),
            TrustError::BadKey => write!(f, "malformed public key"),
            TrustError::BadSignatureFormat => write!(f, "malformed signature file"),
            TrustError::BadJson(e) => write!(f, "invalid document JSON: {e}"),
            TrustError::Rollback { stored, offered } => {
                write!(f, "rollback: offered version {offered} < stored {stored}")
            }
            TrustError::BadExpiry(s) => write!(f, "unparseable expiry: {s}"),
            TrustError::HashMismatch { expected, actual } => {
                write!(f, "artifact hash mismatch: expected {expected}, got {actual}")
            }
            TrustError::SpecTooNew { spec } => write!(f, "index spec {spec} needs a newer Grain"),
        }
    }
}

impl std::error::Error for TrustError {}

/// Outcome of verifying a freshly fetched index (DISTRIBUTION-PLAN §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexStatus {
    /// Signature and freshness are good; use these entries and allow installs.
    Fresh,
    /// Signature is good but `expires` has passed: keep serving the cached copy
    /// with an "offline — last updated" note and refuse **new** installs.
    Expired,
    /// The `spec` major is newer than we understand: show "update Grain".
    NeedsNewerClient,
}

/// Verify a detached minisign signature over `data` with a base64 public key.
fn verify_with_key(pubkey_b64: &str, data: &[u8], sig_text: &str) -> Result<(), TrustError> {
    let pk = PublicKey::from_base64(pubkey_b64).map_err(|_| TrustError::BadKey)?;
    let sig = Signature::decode(sig_text).map_err(|_| TrustError::BadSignatureFormat)?;
    pk.verify(data, &sig, false).map_err(|_| TrustError::BadSignature)
}

/// **Step 1.** Verify and parse `roots.json` against *either* pinned root key.
/// This is the trust anchor: everything else is verified with the publishing
/// key this returns.
pub fn verify_roots(roots_json: &[u8], sig_text: &str) -> Result<Roots, TrustError> {
    let ok = verify_with_key(ROOT_PUBKEY_A, roots_json, sig_text).is_ok()
        || verify_with_key(ROOT_PUBKEY_B, roots_json, sig_text).is_ok();
    if !ok {
        return Err(TrustError::BadSignature);
    }
    serde_json::from_slice(roots_json).map_err(|e| TrustError::BadJson(e.to_string()))
}

/// **Step 2.** Verify and parse `index.json` against the publishing key named in
/// a *verified* [`Roots`], applying the rollback and expiry rules in order.
///
/// - `stored_version`: the highest index `version` this client has accepted, if
///   any. Rollback is rejected against it.
/// - `now_unix`: current time in seconds (injected so the verifier stays pure).
/// - `is_seed`: the shipped seed is exempt from expiry until the first refresh.
pub fn verify_index(
    roots: &Roots,
    index_json: &[u8],
    sig_text: &str,
    stored_version: Option<u64>,
    now_unix: i64,
    is_seed: bool,
) -> Result<(Index, IndexStatus), TrustError> {
    // Rule 1 — signature first, before any entry is read.
    verify_with_key(&roots.publishing_key, index_json, sig_text)?;

    let index: Index =
        serde_json::from_slice(index_json).map_err(|e| TrustError::BadJson(e.to_string()))?;

    // Forward compatibility: an unknown spec major is a message, not an error.
    if index.spec > DISTRIBUTION_SPEC {
        return Ok((index, IndexStatus::NeedsNewerClient));
    }

    // Rule 2 — rollback.
    if let Some(stored) = stored_version {
        if index.version < stored {
            return Err(TrustError::Rollback {
                stored,
                offered: index.version,
            });
        }
    }

    // Rule 3 — expiry. The seed is exempt until the first refresh; a valid
    // clock-skew window keeps a slightly-wrong clock from bricking the store.
    let status = if is_seed {
        IndexStatus::Fresh
    } else {
        let expires = parse_rfc3339(&index.expires)?;
        if now_unix > expires + EXPIRY_CLOCK_SKEW_SECS {
            IndexStatus::Expired
        } else {
            IndexStatus::Fresh
        }
    };

    Ok((index, status))
}

/// Verify and parse `revocations.json` against the publishing key. Revocations
/// are enforced from cache at enable time, so this never gates on expiry.
pub fn verify_revocations(
    roots: &Roots,
    revocations_json: &[u8],
    sig_text: &str,
) -> Result<Revocations, TrustError> {
    verify_with_key(&roots.publishing_key, revocations_json, sig_text)?;
    serde_json::from_slice(revocations_json).map_err(|e| TrustError::BadJson(e.to_string()))
}

/// Lowercase-hex SHA-256 of an artifact's bytes — the hash bound to trust in an
/// index entry `(id, version, sha256)`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

/// Verify a downloaded artifact against the SHA-256 in its index entry. This is
/// the second half of "verified and installed are the same bytes".
pub fn verify_artifact(bytes: &[u8], expected_sha256: &str) -> Result<(), TrustError> {
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(expected_sha256) {
        Ok(())
    } else {
        Err(TrustError::HashMismatch {
            expected: expected_sha256.to_string(),
            actual,
        })
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn parse_rfc3339(s: &str) -> Result<i64, TrustError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp())
        .map_err(|_| TrustError::BadExpiry(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The embedded seed must verify with the pinned keys as shipped — otherwise
    // the app boots with a dead store.
    #[test]
    fn seed_roots_verify_against_pinned_key() {
        let roots = verify_roots(SEED_ROOTS.as_bytes(), SEED_ROOTS_SIG).expect("seed roots verify");
        assert_eq!(roots.spec, 1);
        assert!(!roots.publishing_key.is_empty());
        assert!(!roots.base_urls.is_empty());
    }

    #[test]
    fn seed_index_verifies_and_is_fresh_as_seed() {
        let roots = verify_roots(SEED_ROOTS.as_bytes(), SEED_ROOTS_SIG).unwrap();
        let (index, status) = verify_index(
            &roots,
            SEED_INDEX.as_bytes(),
            SEED_INDEX_SIG,
            None,
            0,
            true,
        )
        .expect("seed index verify");
        assert_eq!(index.spec, 1);
        assert_eq!(status, IndexStatus::Fresh);
    }

    #[test]
    fn seed_revocations_verify() {
        let roots = verify_roots(SEED_ROOTS.as_bytes(), SEED_ROOTS_SIG).unwrap();
        let rev = verify_revocations(&roots, SEED_REVOCATIONS.as_bytes(), SEED_REVOCATIONS_SIG)
            .expect("seed revocations verify");
        assert_eq!(rev.spec, 1);
    }

    #[test]
    fn a_flipped_byte_fails_before_any_entry_is_read() {
        // Property 3 of the anti-forgery guarantee (DISTRIBUTION-PLAN §3.2):
        // a modified index fails verification.
        let roots = verify_roots(SEED_ROOTS.as_bytes(), SEED_ROOTS_SIG).unwrap();
        let mut tampered = SEED_INDEX.as_bytes().to_vec();
        // flip a byte in the middle of the document
        let mid = tampered.len() / 2;
        tampered[mid] ^= 0xFF;
        let err = verify_index(&roots, &tampered, SEED_INDEX_SIG, None, 0, false)
            .expect_err("tampered index must fail");
        assert_eq!(err, TrustError::BadSignature);
    }

    #[test]
    fn roots_signed_by_neither_root_key_is_rejected() {
        // The signature is valid minisign but over the *index* with the
        // *publishing* key — not a root key over roots. Must be refused.
        let bad = verify_roots(SEED_INDEX.as_bytes(), SEED_INDEX_SIG);
        assert!(matches!(bad, Err(TrustError::BadSignature)));
    }

    #[test]
    fn rollback_is_rejected() {
        let roots = verify_roots(SEED_ROOTS.as_bytes(), SEED_ROOTS_SIG).unwrap();
        // seed index version is 1; claim we have already accepted version 5.
        let err = verify_index(&roots, SEED_INDEX.as_bytes(), SEED_INDEX_SIG, Some(5), 0, false)
            .expect_err("rollback must be rejected");
        assert_eq!(err, TrustError::Rollback { stored: 5, offered: 1 });
    }

    #[test]
    fn expired_non_seed_reports_expired_not_error() {
        let roots = verify_roots(SEED_ROOTS.as_bytes(), SEED_ROOTS_SIG).unwrap();
        // The seed expires 2099; ask "is it expired" from the year ~2100.
        let far_future = 4_102_444_800; // 2100-01-01
        let (_index, status) = verify_index(
            &roots,
            SEED_INDEX.as_bytes(),
            SEED_INDEX_SIG,
            None,
            far_future,
            false,
        )
        .expect("expiry is a status, not an error");
        assert_eq!(status, IndexStatus::Expired);
    }

    #[test]
    fn artifact_hash_roundtrip() {
        let bytes = b"a grainpack's bytes";
        let h = sha256_hex(bytes);
        assert!(verify_artifact(bytes, &h).is_ok());
        assert!(matches!(
            verify_artifact(bytes, "deadbeef"),
            Err(TrustError::HashMismatch { .. })
        ));
    }
}
