//! Provisional author-owned remote MCP metadata. Host identities, credentials,
//! trust, enablement, runtime sessions and tool schemas do not belong here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Public provider configuration, never account credentials or protocol state.
pub type McpHeaders = BTreeMap<String, String>;

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

pub const MCP_DESCRIPTOR_SCHEMA: u8 = 1;
pub const MCP_DESCRIPTOR_MAX_BYTES: usize = 8 * 1024;

/// Direct connection input: no extension package, host identity or credentials.
/// Parsing/endpoint admission belongs to Grain, not to this wire declaration.
pub const MCP_CONNECTION_MAX_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteMcpConnection {
    pub name: String,
    pub url: String,
    pub authentication: McpAuthentication,
    #[serde(
        default,
        deserialize_with = "deserialize_headers",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub headers: McpHeaders,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpDescriptor {
    pub schema: u8,
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub grain_api: String,
    pub transport: McpTransport,
    pub authentication: McpAuthentication,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum McpTransport {
    #[serde(rename = "streamable-http")]
    StreamableHttp {
        url: String,
        #[serde(
            default,
            deserialize_with = "deserialize_headers",
            skip_serializing_if = "BTreeMap::is_empty"
        )]
        headers: McpHeaders,
    },
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
