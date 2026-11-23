//! Custom MCP connection metadata (remote HTTPS, local loopback HTTP, and local
//! stdio commands). Host identities, credentials, env secret values, trust,
//! enablement, runtime sessions and tool schemas do not belong here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Public provider configuration, never account credentials or protocol state.
pub type McpHeaders = BTreeMap<String, String>;

/// Public OAuth client registration. Grain discovers endpoints and keeps
/// account tokens; this never supplies a client secret or callback override.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpPublicOAuthClient {
    pub client_id: String,
    pub issuer: String,
}

fn deserialize_headers<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<McpHeaders, D::Error> {
    struct Headers;
    impl<'de> serde::de::Visitor<'de> for Headers {
        type Value = McpHeaders;
        fn expecting(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            out.write_str("public header options with unique names")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<McpHeaders, M::Error> {
            let mut headers = McpHeaders::new();
            while let Some((name, value)) = map.next_entry::<String, String>()? {
                if headers.insert(name, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate MCP header option"));
                }
            }
            Ok(headers)
        }
    }
    deserializer.deserialize_map(Headers)
}

/// Direct connection input: no extension package, host identity or credentials.
/// Parsing/endpoint admission belongs to Grain, not to this wire declaration.
pub const MCP_CONNECTION_MAX_BYTES: usize = 8 * 1024;

/// Unified custom MCP connection metadata across all supported transports.
/// Never carries secret env values or auth tokens; those live only in the OS vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "kebab-case", deny_unknown_fields)]
pub enum McpConnection {
    Remote {
        name: String,
        url: String,
        authentication: McpAuthentication,
        #[serde(
            default,
            deserialize_with = "deserialize_headers",
            skip_serializing_if = "BTreeMap::is_empty"
        )]
        headers: McpHeaders,
        #[serde(
            rename = "oauthClient",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        oauth_client: Option<McpPublicOAuthClient>,
    },
    LocalHttp {
        name: String,
        url: String,
        #[serde(
            default,
            deserialize_with = "deserialize_headers",
            skip_serializing_if = "BTreeMap::is_empty"
        )]
        headers: McpHeaders,
    },
    Stdio {
        name: String,
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(rename = "envKeys", default, skip_serializing_if = "Vec::is_empty")]
        env_keys: Vec<String>,
    },
}

impl McpConnection {
    pub fn name(&self) -> &str {
        match self {
            Self::Remote { name, .. } | Self::LocalHttp { name, .. } | Self::Stdio { name, .. } => {
                name
            }
        }
    }

    pub fn transport_kind(&self) -> &'static str {
        match self {
            Self::Remote { .. } => "remote",
            Self::LocalHttp { .. } => "local-http",
            Self::Stdio { .. } => "stdio",
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::LocalHttp { .. } | Self::Stdio { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum McpAuthentication {
    /// Grain discovers issuer/client registration and owns the consent/vault.
    #[serde(rename = "oauth")]
    OAuth {},
    /// No account credentials. This never authorizes downgrade from OAuth.
    #[serde(rename = "none")]
    None {},
}
