//! Pure MCP descriptor admission and host-owned identity. No client, DNS,
//! persistence, credential or approval is created by parsing metadata.
use std::borrow::Cow;
use std::fmt;

use grain_sdk::manifest::network_capability_host;
use grain_sdk::mcp::{
    McpDescriptor, McpTransport, MCP_DESCRIPTOR_MAX_BYTES, MCP_DESCRIPTOR_SCHEMA,
};
use grain_sdk::{validate_extension_id, validate_extension_version, GRAIN_API_VERSION};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    TooLarge,
    InvalidJson,
    UnsupportedSchema,
    UnsupportedApi,
    InvalidMetadata,
    InvalidEndpoint,
    InvalidIdentity,
}

impl fmt::Display for ContractError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Self::TooLarge => "MCP descriptor exceeds its 8 KiB limit.",
            Self::InvalidJson => "MCP descriptor must use the supported JSON fields and types.",
            Self::UnsupportedSchema => "MCP descriptor schema is unsupported.",
            Self::UnsupportedApi => "MCP descriptor requires an unsupported Grain API profile.",
            Self::InvalidMetadata => "MCP descriptor identity, version or display metadata is invalid.",
            Self::InvalidEndpoint => "MCP endpoint must be HTTPS on a DNS host without local names, embedded credentials, query or fragment.",
            Self::InvalidIdentity => "MCP connection/account/source identity is invalid.",
        })
    }
}
impl std::error::Error for ContractError {}

/// Only validation can create this value; callers cannot mutate the endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedDescriptor(McpDescriptor);

impl ValidatedDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() > MCP_DESCRIPTOR_MAX_BYTES {
            return Err(ContractError::TooLarge);
        }
        let wire = serde_json::from_slice(bytes).map_err(|_| ContractError::InvalidJson)?;
        Self::validate(wire)
    }

    pub fn validate(mut descriptor: McpDescriptor) -> Result<Self, ContractError> {
        if descriptor.schema != MCP_DESCRIPTOR_SCHEMA {
            return Err(ContractError::UnsupportedSchema);
        }
        // Explicit provisional profile, not a guessed semver range evaluator.
        if descriptor.grain_api != format!("^{GRAIN_API_VERSION}") {
            return Err(ContractError::UnsupportedApi);
        }
        if validate_extension_id(&descriptor.id).is_err()
            || validate_extension_version(&descriptor.version).is_err()
            || !display_text(&descriptor.name, 120)
            || !display_text(&descriptor.description, 2048)
        {
            return Err(ContractError::InvalidMetadata);
        }
        let McpTransport::StreamableHttp { url } = &mut descriptor.transport;
        *url = canonical_endpoint(url)?;
        if serde_json::to_vec(&descriptor)
            .map_err(|_| ContractError::InvalidJson)?
            .len()
            > MCP_DESCRIPTOR_MAX_BYTES
        {
            return Err(ContractError::TooLarge);
        }
        Ok(Self(descriptor))
    }

    pub fn descriptor(&self) -> &McpDescriptor {
        &self.0
    }

    pub fn endpoint(&self) -> &str {
        let McpTransport::StreamableHttp { url } = &self.0.transport;
        url
    }
}

fn display_text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value == value.trim()
        && !value
            .chars()
            .any(|ch| ch.is_control() || crate::execution::is_hidden_result_character(ch))
}

fn canonical_endpoint(raw: &str) -> Result<String, ContractError> {
    if raw.len() > 2048
        || raw.contains('\\')
        || raw.chars().any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(ContractError::InvalidEndpoint);
    }
    let url = url::Url::parse(raw).map_err(|_| ContractError::InvalidEndpoint)?;
    let Some(url::Host::Domain(host)) = url.host() else {
        return Err(ContractError::InvalidEndpoint);
    };
    // Empty userinfo is still userinfo; the URL parser exposes an empty username.
    let authority = raw
        .split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(rest));
    if url.scheme() != "https"
        || authority.is_none_or(|part| part.is_empty())
        || network_capability_host(&format!("net:{host}")).is_none()
        || !host.contains('.')
        || host.ends_with('.')
        || [".localhost", ".local", ".internal", ".home.arpa"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
        || authority.is_some_and(|part| part.contains('@'))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port() == Some(0)
    {
        return Err(ContractError::InvalidEndpoint);
    }
    // Parser normalizes scheme/host/default port. Preserve meaningful path
    // case and trailing slash; only the root resource uses no trailing slash.
    let mut canonical = url.to_string();
    if url.path() == "/" {
        canonical.pop();
    }
    if canonical.len() > 2048 {
        return Err(ContractError::InvalidEndpoint);
    }
    Ok(canonical)
}

/// Constructed by trusted host acquisition code, never deserialized from an
/// author descriptor. A source label is provenance, not proof of verification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ConnectionSource {
    DevelopmentCatalog {
        provider_id: String,
    },
    Configured,
    Store {
        extension_id: String,
        version: String,
        artifact_sha256: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConnectionIdentity {
    connection_id: String,
    account_id: String,
    source: ConnectionSource,
}

impl ConnectionIdentity {
    pub fn catalog(provider_id: &str) -> Result<Self, ContractError> {
        Self::new(
            provider_id,
            provider_id,
            ConnectionSource::DevelopmentCatalog {
                provider_id: provider_id.into(),
            },
        )
    }

    pub fn new(
        connection_id: &str,
        account_id: &str,
        source: ConnectionSource,
    ) -> Result<Self, ContractError> {
        if !scope_id(connection_id) || !scope_id(account_id) {
            return Err(ContractError::InvalidIdentity);
        }
        match &source {
            ConnectionSource::DevelopmentCatalog { provider_id } => {
                if provider_id != connection_id || provider_id != account_id {
                    return Err(ContractError::InvalidIdentity);
                }
            }
            ConnectionSource::Configured => {}
            ConnectionSource::Store {
                extension_id,
                version,
                artifact_sha256,
            } => {
                if validate_extension_id(extension_id).is_err()
                    || validate_extension_version(version).is_err()
                    || artifact_sha256.len() != 64
                    || !artifact_sha256
                        .bytes()
                        .all(|ch| ch.is_ascii_digit() || (b'a'..=b'f').contains(&ch))
                {
                    return Err(ContractError::InvalidIdentity);
                }
            }
        }
        Ok(Self {
            connection_id: connection_id.into(),
            account_id: account_id.into(),
            source,
        })
    }

    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
    pub fn account_id(&self) -> &str {
        &self.account_id
    }
    pub fn source(&self) -> &ConnectionSource {
        &self.source
    }

    /// Preserve the deployed development catalog vault keys exactly. New
    /// acquisition paths have disjoint, delimiter-safe owner namespaces.
    pub fn vault_account(&self) -> Cow<'_, str> {
        match &self.source {
            ConnectionSource::DevelopmentCatalog { provider_id } => Cow::Borrowed(provider_id),
            ConnectionSource::Configured => Cow::Owned(format!(
                "mcp:v1:configured:{}:{}",
                self.connection_id, self.account_id
            )),
            ConnectionSource::Store { extension_id, .. } => Cow::Owned(format!(
                "mcp:v1:store:{extension_id}:{}:{}",
                self.connection_id, self.account_id
            )),
        }
    }
}

fn scope_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'-')
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_sdk::mcp::McpAuthentication;
    use serde_json::{json, Value};

    fn wire() -> Value {
        json!({"schema":1,"id":"com.example.calendar","name":"Calendar",
            "description":"Read calendar events.","version":"1.0.0","grainApi":"^1.0",
            "transport":{"type":"streamable-http","url":"https://mcp.example.com/mcp"},
            "authentication":{"type":"oauth"}})
    }
    fn admit(value: &Value) -> Result<ValidatedDescriptor, ContractError> {
        ValidatedDescriptor::parse(&serde_json::to_vec(value).unwrap())
    }
    fn store(version: &str, hash: &str) -> ConnectionSource {
        ConnectionSource::Store {
            extension_id: "com.example.calendar".into(),
            version: version.into(),
            artifact_sha256: hash.into(),
        }
    }

    #[test]
    fn descriptor_roundtrip_is_metadata_only_and_auth_is_explicit() {
        let accepted = admit(&wire()).unwrap();
        assert_eq!(serde_json::to_value(accepted.descriptor()).unwrap(), wire());
        assert_eq!(accepted.endpoint(), "https://mcp.example.com/mcp");
        let mut value = wire();
        value["authentication"]["type"] = json!("none");
        assert_eq!(
            admit(&value).unwrap().descriptor().authentication,
            McpAuthentication::None {}
        );
    }

    #[test]
    fn author_fields_cannot_supply_host_ownership_or_secrets() {
        for field in [
            "source",
            "trusted",
            "enabled",
            "connectionId",
            "accountId",
            "headers",
            "clientId",
            "clientSecret",
            "tokens",
            "tools",
            "command",
            "args",
            "env",
            "capabilities",
            "settings",
            "prompts",
            "spaces",
            "main",
            "signature",
        ] {
            let mut value = wire();
            value[field] = json!("forged-secret");
            assert_eq!(
                admit(&value),
                Err(ContractError::InvalidJson),
                "root {field}"
            );
        }
        for object in ["transport", "authentication"] {
            for field in [
                "headers",
                "clientId",
                "clientSecret",
                "issuer",
                "source",
                "enabled",
                "env",
                "urlOverride",
            ] {
                let mut value = wire();
                value[object][field] = json!("forged-secret");
                assert_eq!(
                    admit(&value),
                    Err(ContractError::InvalidJson),
                    "{object}.{field}"
                );
            }
        }
        let mut value = wire();
        value["authentication"] = json!({"type":"none","headers":{}});
        assert_eq!(admit(&value), Err(ContractError::InvalidJson));
    }

    #[test]
    fn missing_duplicate_future_and_wrong_type_fields_are_refused() {
        for field in wire().as_object().unwrap().keys() {
            let mut value = wire();
            value.as_object_mut().unwrap().remove(field);
            assert_eq!(
                admit(&value),
                Err(ContractError::InvalidJson),
                "missing {field}"
            );
            let mut value = wire();
            value[field] = json!(false);
            assert_eq!(
                admit(&value),
                Err(ContractError::InvalidJson),
                "type {field}"
            );
        }
        let raw = serde_json::to_string(&wire()).unwrap();
        let duplicate = format!("{{\"schema\":1,{}", &raw[1..]);
        assert_eq!(
            ValidatedDescriptor::parse(duplicate.as_bytes()),
            Err(ContractError::InvalidJson)
        );
        for duplicate in [
            raw.replace("\"type\":\"oauth\"", "\"type\":\"oauth\",\"type\":\"none\""),
            raw.replace("\"url\":", "\"url\":\"https://other.example/mcp\",\"url\":"),
        ] {
            assert_eq!(
                ValidatedDescriptor::parse(duplicate.as_bytes()),
                Err(ContractError::InvalidJson)
            );
        }
        let mut value = wire();
        value["schema"] = json!(2);
        assert_eq!(admit(&value), Err(ContractError::UnsupportedSchema));
        for api in ["1.0", "*", "^2.0", "^1.0 ", ">=1.0"] {
            let mut value = wire();
            value["grainApi"] = json!(api);
            assert_eq!(admit(&value), Err(ContractError::UnsupportedApi));
        }
        for transport in ["stdio", "sse", "native"] {
            let mut value = wire();
            value["transport"]["type"] = json!(transport);
            assert_eq!(admit(&value), Err(ContractError::InvalidJson));
        }
        let mut value = wire();
        value["authentication"]["type"] = json!("automatic");
        assert_eq!(admit(&value), Err(ContractError::InvalidJson));
    }

    #[test]
    fn input_and_display_limits_apply_without_echoing_rejected_values() {
        assert_eq!(
            ValidatedDescriptor::parse(&vec![b' '; MCP_DESCRIPTOR_MAX_BYTES + 1]),
            Err(ContractError::TooLarge)
        );
        let raw = serde_json::to_vec(&wire()).unwrap();
        let mut exact = raw.clone();
        exact.resize(MCP_DESCRIPTOR_MAX_BYTES, b' ');
        assert!(ValidatedDescriptor::parse(&exact).is_ok());
        exact.push(b' ');
        assert_eq!(
            ValidatedDescriptor::parse(&exact),
            Err(ContractError::TooLarge)
        );
        for (field, value) in [
            ("name", "".into()),
            ("name", " Calendar".into()),
            ("name", "x".repeat(121)),
            ("description", "x".repeat(2049)),
            ("description", "line\nline".into()),
            ("description", "secret\u{202e}text".into()),
            ("id", "Example.Calendar".into()),
            ("version", "../1.0".into()),
        ] {
            let mut descriptor = wire();
            descriptor[field] = json!(value);
            let error = admit(&descriptor).unwrap_err();
            assert_eq!(error, ContractError::InvalidMetadata);
            assert!(!error.to_string().contains(&value) || value.is_empty());
        }
        let mut descriptor = wire();
        descriptor["name"] = json!("é".repeat(61));
        assert_eq!(admit(&descriptor), Err(ContractError::InvalidMetadata));
        let mut descriptor = wire();
        descriptor["name"] = json!("x".repeat(120));
        descriptor["description"] = json!("x".repeat(2048));
        assert!(admit(&descriptor).is_ok());
    }

    #[test]
    fn endpoints_are_canonical_but_meaningful_paths_remain_distinct() {
        for (raw, expected) in [
            ("HTTPS://MCP.EXAMPLE.COM:443", "https://mcp.example.com"),
            ("https://mcp.example.com/", "https://mcp.example.com"),
            (
                "https://mcp.example.com:8443/MCP/",
                "https://mcp.example.com:8443/MCP/",
            ),
            (
                "https://mcp.example.com/a/../mcp",
                "https://mcp.example.com/mcp",
            ),
            (
                "https://bücher.example/mcp",
                "https://xn--bcher-kva.example/mcp",
            ),
            (
                "https://api.githubcopilot.com/mcp/",
                "https://api.githubcopilot.com/mcp/",
            ),
        ] {
            assert_eq!(canonical_endpoint(raw).unwrap(), expected);
        }
        assert_ne!(
            canonical_endpoint("https://mcp.example.com/mcp/").unwrap(),
            canonical_endpoint("https://mcp.example.com/mcp").unwrap()
        );
    }

    #[test]
    fn unsafe_or_ambiguous_endpoint_forms_are_refused() {
        for endpoint in [
            "http://mcp.example.com",
            "file:///mcp",
            "https://localhost/mcp",
            "https://a.localhost/mcp",
            "https://a.local/mcp",
            "https://a.internal/mcp",
            "https://a.home.arpa/mcp",
            "https://a.example./mcp",
            "https://127.0.0.1/mcp",
            "https://2130706433/mcp",
            "https://0x7f000001/mcp",
            "https://[::1]/mcp",
            "https://user:secret@mcp.example.com/mcp",
            "https://@mcp.example.com/mcp",
            "https://mcp.example.com/mcp?token=secret",
            "https://mcp.example.com/mcp?",
            "https://mcp.example.com/mcp#",
            "https://mcp.example.com:0/mcp",
            "https://mcp.example.com:65536/mcp",
            "https://mcp.example.com/white space",
            "https://mcp.example.com/line\nfeed",
            "https://mcp.example.com\\mcp",
            "https:////mcp.example.com/mcp",
            "https:mcp.example.com/mcp",
            "https://bad_host.example/mcp",
            "https://a..example/mcp",
            "https://-a.example/mcp",
        ] {
            assert_eq!(
                canonical_endpoint(endpoint),
                Err(ContractError::InvalidEndpoint),
                "{endpoint}"
            );
        }
        assert_eq!(
            canonical_endpoint(&format!("https://mcp.example.com/{}", "a".repeat(2048))),
            Err(ContractError::InvalidEndpoint)
        );
        // URL percent encoding expands Unicode paths; the normalized value
        // must retain the same bound so an accepted descriptor round-trips.
        assert_eq!(
            canonical_endpoint(&format!("https://mcp.example.com/{}", "é".repeat(500))),
            Err(ContractError::InvalidEndpoint)
        );
        let mut value = wire();
        value["transport"]["url"] = json!("https://user:secret@mcp.example.com");
        assert!(!admit(&value).unwrap_err().to_string().contains("secret"));
    }

    #[test]
    fn catalog_keys_are_preserved_and_cannot_claim_another_account() {
        for provider in [
            "linear",
            "notion",
            "atlassian",
            "github",
            "slack",
            "google-calendar",
            "grain-harness-auth-peer",
        ] {
            let owner = ConnectionIdentity::catalog(provider).unwrap();
            assert_eq!(owner.connection_id(), provider);
            assert_eq!(owner.account_id(), provider);
            assert_eq!(owner.vault_account(), provider);
        }
        for (connection, account) in [("linear", "notion"), ("notion", "linear")] {
            assert_eq!(
                ConnectionIdentity::new(
                    connection,
                    account,
                    ConnectionSource::DevelopmentCatalog {
                        provider_id: "linear".into()
                    }
                ),
                Err(ContractError::InvalidIdentity)
            );
        }
    }

    #[test]
    fn connection_account_and_acquisition_sources_have_disjoint_keys() {
        let configured = |connection, account| {
            ConnectionIdentity::new(connection, account, ConnectionSource::Configured).unwrap()
        };
        let base = configured("linear", "linear");
        assert_eq!(base.vault_account(), "mcp:v1:configured:linear:linear");
        for other in [
            configured("linear", "work"),
            configured("work", "linear"),
            ConnectionIdentity::catalog("linear").unwrap(),
            ConnectionIdentity::new("linear", "linear", store("1.0.0", &"a".repeat(64))).unwrap(),
        ] {
            assert_ne!(base.vault_account(), other.vault_account());
        }
        let a = ConnectionIdentity::new("linear", "work", store("1.0.0", &"a".repeat(64))).unwrap();
        let b = ConnectionIdentity::new("linear", "work", store("2.0.0", &"b".repeat(64))).unwrap();
        assert_eq!(a.vault_account(), b.vault_account()); // Host must invalidate changed provenance; no implicit account migration.
        assert_ne!(a.source(), b.source());
        assert_eq!(
            a.vault_account(),
            "mcp:v1:store:com.example.calendar:linear:work"
        );
    }

    #[test]
    fn identity_delimiters_bounds_and_store_provenance_are_validated() {
        for invalid in [
            "",
            "Linear",
            "-linear",
            "linear-",
            "linear:work",
            "a/b",
            "é",
            &"a".repeat(65),
        ] {
            assert_eq!(
                ConnectionIdentity::catalog(invalid),
                Err(ContractError::InvalidIdentity)
            );
            assert_eq!(
                ConnectionIdentity::new("linear", invalid, ConnectionSource::Configured),
                Err(ContractError::InvalidIdentity)
            );
        }
        assert!(ConnectionIdentity::catalog(&"a".repeat(64)).is_ok());
        for source in [
            store("1.0.0", &"a".repeat(63)),
            store("1.0.0", &"A".repeat(64)),
            store("1.0.0", &"g".repeat(64)),
            store("../version", &"a".repeat(64)),
            ConnectionSource::Store {
                extension_id: "BAD:id".into(),
                version: "1.0.0".into(),
                artifact_sha256: "a".repeat(64),
            },
        ] {
            assert_eq!(
                ConnectionIdentity::new("linear", "work", source),
                Err(ContractError::InvalidIdentity)
            );
        }
    }
}
