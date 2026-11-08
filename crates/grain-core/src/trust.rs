//! Integrity checks for the exact bytes owned by a local fixture record.

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustError {
    expected: String,
    actual: String,
}

impl std::fmt::Display for TrustError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "artifact hash mismatch (expected {}, got {})",
            self.expected, self.actual
        )
    }
}

impl std::error::Error for TrustError {}

/// Lowercase-hex SHA-256 of an artifact.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

/// Verify local bytes against the digest owned by the fixture record.
pub fn verify_artifact(bytes: &[u8], expected_sha256: &str) -> Result<(), TrustError> {
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(expected_sha256) {
        Ok(())
    } else {
        Err(TrustError {
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
