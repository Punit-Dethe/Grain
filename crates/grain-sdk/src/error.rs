//! Typed host-API errors shared by Grain and every extension runtime.

use serde::{Deserialize, Serialize};

/// Stable machine-readable error vocabulary. Codes are additive: existing
/// variants are never repurposed, so extension recovery logic stays valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostErrorCode {
    #[serde(rename = "E_CAPABILITY_DENIED")]
    CapabilityDenied,
    #[serde(rename = "E_TIMEOUT")]
    Timeout,
    #[serde(rename = "E_SESSION_BUSY")]
    SessionBusy,
    #[serde(rename = "E_QUOTA")]
    Quota,
    #[serde(rename = "E_RESPONSE_TOO_LARGE")]
    ResponseTooLarge,
    #[serde(rename = "E_INVALID_MANIFEST")]
    InvalidManifest,
    #[serde(rename = "E_INVALID_ARGUMENT")]
    InvalidArgument,
    #[serde(rename = "E_NOT_IMPLEMENTED")]
    NotImplemented,
    #[serde(rename = "E_UNKNOWN_METHOD")]
    UnknownMethod,
    #[serde(rename = "E_UNAVAILABLE")]
    Unavailable,
    #[serde(rename = "E_INTERNAL")]
    Internal,
}

impl HostErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CapabilityDenied => "E_CAPABILITY_DENIED",
            Self::Timeout => "E_TIMEOUT",
            Self::SessionBusy => "E_SESSION_BUSY",
            Self::Quota => "E_QUOTA",
            Self::ResponseTooLarge => "E_RESPONSE_TOO_LARGE",
            Self::InvalidManifest => "E_INVALID_MANIFEST",
            Self::InvalidArgument => "E_INVALID_ARGUMENT",
            Self::NotImplemented => "E_NOT_IMPLEMENTED",
            Self::UnknownMethod => "E_UNKNOWN_METHOD",
            Self::Unavailable => "E_UNAVAILABLE",
            Self::Internal => "E_INTERNAL",
        }
    }
}

const ERROR_DOCS: &str =
    "https://github.com/Punit-Dethe/Grain/blob/main/docs/Extension%20Platform/ERRORS.md";

/// Server -> extension failure. Human explanation and recovery guidance travel
/// with the stable code so an author never has to infer a fix from prose alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostError {
    pub code: HostErrorCode,
    pub message: String,
    pub hint: String,
    pub docs: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

impl HostError {
    pub fn new(code: HostErrorCode, message: impl Into<String>, hint: impl Into<String>) -> Self {
        let anchor = code.as_str().to_ascii_lowercase();
        Self {
            code,
            message: message.into(),
            hint: hint.into(),
            docs: format!("{ERROR_DOCS}#{anchor}"),
            capability: None,
        }
    }

    pub fn capability_denied(capability: &str, method: &str) -> Self {
        let mut error = Self::new(
            HostErrorCode::CapabilityDenied,
            format!("'{method}' requires the '{capability}' capability."),
            format!(
                "Add \"{capability}\" to permissions in manifest.json, then reload and approve it."
            ),
        );
        error.capability = Some(capability.to_string());
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_error_wire_shape_carries_a_fix() {
        let error = HostError::capability_denied("storage", "storage.get");
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], "E_CAPABILITY_DENIED");
        assert_eq!(json["capability"], "storage");
        assert!(json["message"].as_str().unwrap().contains("storage.get"));
        assert!(!json["hint"].as_str().unwrap().is_empty());
        assert!(json["docs"].as_str().unwrap().starts_with("https://"));
    }
}
