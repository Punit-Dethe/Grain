//! Provisional author-owned remote MCP metadata. Host identities, credentials,
//! trust, enablement, runtime sessions and tool schemas do not belong here.

use serde::{Deserialize, Serialize};

pub const MCP_DESCRIPTOR_SCHEMA: u8 = 1;
pub const MCP_DESCRIPTOR_MAX_BYTES: usize = 8 * 1024;

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
    StreamableHttp { url: String },
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
