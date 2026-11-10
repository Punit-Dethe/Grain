//! Pure MCP connection admission and host-owned identity. No client, DNS,
//! persistence, credential or approval is created by parsing metadata.
use std::borrow::Cow;
use std::fmt;

use grain_sdk::mcp::{McpAuthentication, McpHeaders, McpPublicOAuthClient, RemoteMcpConnection};
use grain_sdk::network::is_canonical_host;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    InvalidJson,
    InvalidMetadata,
    InvalidEndpoint,
    InvalidIdentity,
    InvalidHeaders,
    InvalidOAuthClient,
}

impl fmt::Display for ContractError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Self::InvalidJson => "MCP connection must use the supported JSON fields and types.",
            Self::InvalidMetadata => "MCP connection display metadata is invalid.",
            Self::InvalidEndpoint => "MCP endpoint must be HTTPS on a DNS host without local names, embedded credentials, query or fragment.",
            Self::InvalidIdentity => "MCP connection/account/source identity is invalid.",
            Self::InvalidHeaders => "MCP headers must be bounded public provider options; authentication, routing and protocol headers belong to Grain.",
            Self::InvalidOAuthClient => "MCP public OAuth clients require OAuth, a public client ID and an exact HTTPS issuer; secrets and endpoint overrides are not supported.",
        })
    }
}
impl std::error::Error for ContractError {}

pub fn validate_oauth_client(
    authentication: McpAuthentication,
    client: Option<&McpPublicOAuthClient>,
) -> Result<(), ContractError> {
    if let Some(client) = client {
        if authentication != (McpAuthentication::OAuth {})
            || !display_text(&client.client_id, 512)
            || canonical_endpoint(&client.issuer).is_err()
        {
            return Err(ContractError::InvalidOAuthClient);
        }
        // Issuers are exact OAuth identifiers, including trailing slash/path.
        // Validate destination shape but never normalize their identity.
    }
    Ok(())
}

pub fn account_binding_changed(old: &RemoteMcpConnection, new: &RemoteMcpConnection) -> bool {
    old.url != new.url
        || old.authentication != new.authentication
        || old.oauth_client != new.oauth_client
}

/// Header names are lower-case to avoid case aliases. Values are public package
/// metadata: no environment substitution, vault lookup or credential injection.
pub fn validate_headers(headers: &McpHeaders) -> Result<(), ContractError> {
    if headers.len() > 16
        || headers
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
            > 2048
    {
        return Err(ContractError::InvalidHeaders);
    }
    for (name, value) in headers {
        if name.is_empty()
            || name.len() > 64
            || name != &name.to_ascii_lowercase()
            || http::header::HeaderName::from_bytes(name.as_bytes()).is_err()
            || value.is_empty()
            || value.len() > 512
            || value != value.trim()
            || !value.bytes().all(|b| (32..=126).contains(&b))
            || http::header::HeaderValue::from_str(value).is_err()
            || [
                "authorization",
                "proxy-authorization",
                "cookie",
                "set-cookie",
                "host",
                "connection",
                "content-length",
                "content-type",
                "content-encoding",
                "content-range",
                "range",
                "accept",
                "accept-encoding",
                "transfer-encoding",
                "te",
                "trailer",
                "upgrade",
                "origin",
                "referer",
                "user-agent",
                "forwarded",
                "via",
                "last-event-id",
                "expect",
            ]
            .contains(&name.as_str())
            || [
                "mcp-",
                "sec-",
                "proxy-",
                "x-forwarded-",
                "x-original-",
                "x-rewrite-",
                "x-http-method",
                "x-method-override",
                "if-",
            ]
            .iter()
            .any(|p| name.starts_with(p))
            || [
                "auth",
                "token",
                "secret",
                "api-key",
                "apikey",
                "password",
                "credential",
            ]
            .iter()
            .any(|p| name.contains(p))
        {
            return Err(ContractError::InvalidHeaders);
        }
    }
    Ok(())
}

pub(crate) fn display_text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value == value.trim()
        && !value
            .chars()
            .any(|ch| ch.is_control() || crate::execution::is_hidden_result_character(ch))
}

pub(crate) fn canonical_endpoint(raw: &str) -> Result<String, ContractError> {
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
        || !is_canonical_host(host)
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
/// connection input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ConnectionSource {
    DevelopmentCatalog { provider_id: String },
    Configured,
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
    use crate::mcp_connections::{ConnectionDefinition, RegistryError};
    use serde_json::{json, Value};

    fn wire() -> Value {
        json!({"name":"Calendar","url":"https://mcp.example.com/mcp",
            "authentication":{"type":"oauth"}})
    }

    fn admit(value: &Value) -> Result<ConnectionDefinition, RegistryError> {
        ConnectionDefinition::parse(&serde_json::to_vec(value).unwrap())
    }

    #[test]
    fn public_oauth_client_is_exact_issuer_metadata_not_secret_configuration() {
        let mut value = wire();
        value["oauthClient"] =
            json!({"clientId":"public-client", "issuer":"https://issuer.example/tenant/"});
        let accepted = admit(&value).unwrap();
        assert_eq!(
            accepted.definition().oauth_client.as_ref().unwrap().issuer,
            "https://issuer.example/tenant/"
        );
        assert_eq!(serde_json::to_value(accepted.definition()).unwrap(), value);
        for field in [
            "clientSecret",
            "redirectUri",
            "authorizationEndpoint",
            "tokenEndpoint",
            "scopes",
        ] {
            let mut bad = value.clone();
            bad["oauthClient"][field] = json!("forbidden");
            assert_eq!(
                admit(&bad),
                Err(RegistryError::InvalidInput(ContractError::InvalidJson))
            );
        }
        for id in ["", " padded", "line\nline", "hidden\u{202e}text"] {
            let mut bad = value.clone();
            bad["oauthClient"]["clientId"] = json!(id);
            assert_eq!(
                admit(&bad),
                Err(RegistryError::InvalidInput(
                    ContractError::InvalidOAuthClient
                ))
            );
        }
        for issuer in [
            "http://issuer.example",
            "https://localhost",
            "https://issuer.example?token=secret",
            "https://issuer.example/#issuer",
        ] {
            let mut bad = value.clone();
            bad["oauthClient"]["issuer"] = json!(issuer);
            assert_eq!(
                admit(&bad),
                Err(RegistryError::InvalidInput(
                    ContractError::InvalidOAuthClient
                ))
            );
        }
        value["authentication"]["type"] = json!("none");
        assert_eq!(
            admit(&value),
            Err(RegistryError::InvalidInput(
                ContractError::InvalidOAuthClient
            ))
        );
    }

    #[test]
    fn public_headers_are_bounded_and_cannot_override_host_controls() {
        let mut value = wire();
        value["headers"] = json!({"x-mcp-toolsets":"all", "notion-version":"2025-09-03"});
        let accepted = admit(&value).unwrap();
        assert_eq!(accepted.definition().headers["x-mcp-toolsets"], "all");
        assert_eq!(serde_json::to_value(accepted.definition()).unwrap(), value);
        for name in [
            "Authorization",
            "authorization",
            "cookie",
            "proxy-authorization",
            "host",
            "origin",
            "content-type",
            "content-length",
            "accept",
            "connection",
            "last-event-id",
            "mcp-session-id",
            "mcp-protocol-version",
            "mcp-method",
            "mcp-param-tenant",
            "sec-fetch-site",
            "x-forwarded-host",
            "x-api-key",
            "x-access-token",
            "bad name",
        ] {
            value["headers"] = json!({name:"private"});
            assert_eq!(
                admit(&value),
                Err(RegistryError::InvalidInput(ContractError::InvalidHeaders)),
                "{name}"
            );
        }
        for bad in ["", " padded", "line\r\ninjected: yes", "\t", "\u{7f}", "é"] {
            value["headers"] = json!({"x-option":bad});
            assert_eq!(
                admit(&value),
                Err(RegistryError::InvalidInput(ContractError::InvalidHeaders))
            );
        }
        value["headers"] = json!({"x-option":"x".repeat(513)});
        assert_eq!(
            admit(&value),
            Err(RegistryError::InvalidInput(ContractError::InvalidHeaders))
        );
        value["headers"] = serde_json::to_value(
            (0..17)
                .map(|n| (format!("x-option-{n}"), "value".to_owned()))
                .collect::<McpHeaders>(),
        )
        .unwrap();
        assert_eq!(
            admit(&value),
            Err(RegistryError::InvalidInput(ContractError::InvalidHeaders))
        );
        value["headers"] = serde_json::to_value(
            (0..5)
                .map(|n| (format!("x-option-{n}"), "x".repeat(512)))
                .collect::<McpHeaders>(),
        )
        .unwrap();
        assert_eq!(
            admit(&value),
            Err(RegistryError::InvalidInput(ContractError::InvalidHeaders))
        );
    }

    #[test]
    fn connection_json_rejects_duplicate_headers_and_oversize() {
        let mut value = wire();
        value["headers"] = json!({"x-option":"one"});
        let raw = serde_json::to_string(&value).unwrap().replace(
            "\"x-option\":\"one\"",
            "\"x-option\":\"one\",\"x-option\":\"two\"",
        );
        assert_eq!(
            ConnectionDefinition::parse(raw.as_bytes()),
            Err(RegistryError::InvalidInput(ContractError::InvalidJson))
        );
        value["url"] = json!(format!("https://mcp.example.com/{}", "p".repeat(1900)));
        value["headers"] = serde_json::to_value(
            (0..4)
                .map(|n| (format!("x-opt-{n}"), "\"".repeat(500)))
                .collect::<McpHeaders>(),
        )
        .unwrap();
        assert_eq!(admit(&value), Err(RegistryError::Limit));
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
        // must retain the same bound so an accepted connection round-trips.
        assert_eq!(
            canonical_endpoint(&format!("https://mcp.example.com/{}", "é".repeat(500))),
            Err(ContractError::InvalidEndpoint)
        );
        let mut value = wire();
        value["url"] = json!("https://user:secret@mcp.example.com");
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
        ] {
            assert_ne!(base.vault_account(), other.vault_account());
        }
    }

    #[test]
    fn identity_delimiters_and_bounds_are_validated() {
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
    }
}
