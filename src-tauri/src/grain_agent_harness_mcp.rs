//! Fixed MCP acceptance peers, absent from ordinary/release builds.
//! Reuses production discovery, approvals, SDK, byte limits and cancellation.
use serde::Deserialize;
use tauri::{AppHandle, WebviewWindow};

pub(crate) const PROVIDER_ID: &str = "grain-harness";
pub(crate) const AUTH_PROVIDER_ID: &str = "grain-harness-auth";
pub(crate) const CLIENT_PROVIDER_ID: &str = "grain-harness-auth-client";
pub(crate) const PEER_PROVIDER_ID: &str = "grain-harness-auth-peer";
pub(crate) const PEER_CLIENT_PROVIDER_ID: &str = "grain-harness-auth-peer-client";
pub(crate) const ACCOUNT_IDS: [&str; 4] = [
    AUTH_PROVIDER_ID,
    CLIENT_PROVIDER_ID,
    PEER_PROVIDER_ID,
    PEER_CLIENT_PROVIDER_ID,
];
pub(crate) const LIVE_ENDPOINT: &str = "https://mcp.deepwiki.com/mcp";
pub(crate) const LINEAR_PROVIDER_ID: &str = "grain-harness-linear";
pub(crate) const LINEAR_ENDPOINT: &str = "https://mcp.linear.app/mcp/readonly";
const LIVE_REPOSITORY: &str = "modelcontextprotocol/rust-sdk";

pub(crate) fn linear_endpoint() -> Result<&'static str, String> {
    if !super::grain_agent_harness::live_linear_enabled() {
        return Err("Live Linear preflight is not enabled".into());
    }
    Ok(LINEAR_ENDPOINT)
}

pub(crate) fn endpoint() -> Result<String, String> {
    if super::grain_agent_harness::live_deepwiki_enabled() {
        return Ok(LIVE_ENDPOINT.into());
    }
    let (_, port) = super::grain_agent_harness::mcp_fixture_config()?;
    Ok(format!("https://127.0.0.1:{port}/mcp"))
}

pub(crate) fn auth_endpoint(id: &str) -> Result<String, String> {
    if !super::grain_agent_harness::mcp_auth_enabled() {
        return Err("MCP account fixture is not enabled".into());
    }
    let (_, port) = account_config(id)?;
    Ok(format!("https://127.0.0.1:{port}/account-mcp"))
}

fn account_config(id: &str) -> Result<(std::path::PathBuf, u16), String> {
    match id {
        AUTH_PROVIDER_ID | CLIENT_PROVIDER_ID => super::grain_agent_harness::mcp_fixture_config(),
        PEER_PROVIDER_ID | PEER_CLIENT_PROVIDER_ID => super::grain_agent_harness::mcp_peer_config(),
        _ => Err("Unknown MCP account fixture".into()),
    }
}

pub(crate) fn client() -> Result<(String, reqwest_mcp::Client), String> {
    if super::grain_agent_harness::live_deepwiki_enabled() {
        return Ok((
            LIVE_ENDPOINT.into(),
            super::grain_mcp::McpHttpClient::builder()
                .no_proxy()
                .build()
                .map_err(|_| "Cannot build public MCP test client")?,
        ));
    }
    let (root, _) = super::grain_agent_harness::mcp_fixture_config()?;
    let path = root
        .join("mcp-tls/ca.pem")
        .canonicalize()
        .map_err(|_| "MCP test certificate is missing")?;
    if !path.starts_with(&root) {
        return Err("MCP test certificate escaped its owned root".into());
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "Cannot open MCP test certificate")?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read MCP test certificate")?;
    if bytes.len() > 8192 {
        return Err("MCP test certificate is oversized".into());
    }
    let cert =
        reqwest_mcp::Certificate::from_pem(&bytes).map_err(|_| "Invalid MCP test certificate")?;
    let client = super::grain_mcp::McpHttpClient::builder()
        .tls_certs_only([cert])
        .no_proxy()
        .build()
        .map_err(|_| "Cannot build scoped MCP test client")?;
    Ok((endpoint()?, client))
}

/// Validate only the opt-in public test path. No arbitrary prompt/repository,
/// credentials or provider writes can be sent by this test adapter.
pub(crate) fn validate_live_post(
    uri: &str,
    message: &rmcp::model::ClientJsonRpcMessage,
    has_auth: bool,
) -> Result<(), String> {
    if uri == LINEAR_ENDPOINT {
        linear_endpoint()?;
        let value =
            serde_json::to_value(message).map_err(|_| "Invalid Linear preflight message")?;
        return validate_linear_method(&value);
    }
    if uri != LIVE_ENDPOINT {
        return Ok(());
    }
    if !super::grain_agent_harness::live_deepwiki_enabled() || has_auth {
        return Err("Public MCP test requires its isolated no-account marker".into());
    }
    let value = serde_json::to_value(message).map_err(|_| "Invalid public MCP test message")?;
    if let Some(action) = validated_live_action(&value)? {
        super::grain_agent_harness::observe("mcp-live-attempt", "mcp.grain-harness", action, "");
    }
    Ok(())
}

// This preflight has no account-read allowance. Even an unexpected successful
// callback cannot make any tool executable through the real Agent adapter.
fn validate_linear_method(value: &serde_json::Value) -> Result<(), String> {
    if [
        "initialize",
        "notifications/initialized",
        "server/discover",
        "tools/list",
        "ping",
    ]
    .contains(&value["method"].as_str().unwrap_or_default())
    {
        Ok(())
    } else {
        Err("Linear consent preflight does not permit tool execution".into())
    }
}

fn validated_live_action(value: &serde_json::Value) -> Result<Option<&str>, String> {
    let method = value["method"]
        .as_str()
        .ok_or("Missing public MCP method")?;
    if method != "tools/call" {
        return if [
            "initialize",
            "notifications/initialized",
            "server/discover",
            "tools/list",
            "ping",
        ]
        .contains(&method)
        {
            Ok(None)
        } else {
            Err("Unexpected public MCP test method".into())
        };
    }
    let name = value["params"]["name"]
        .as_str()
        .ok_or("Missing public MCP tool")?;
    if !["read_wiki_structure", "read_wiki_contents"].contains(&name)
        || value["params"]["arguments"] != serde_json::json!({"repoName": LIVE_REPOSITORY})
    {
        return Err("Public MCP test admits only fixed repository documentation reads".into());
    }
    Ok(Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_test_refuses_questions_other_repositories_and_extra_arguments() {
        for name in ["read_wiki_structure", "read_wiki_contents"] {
            let good = serde_json::json!({"method":"tools/call","params":{"name":name,"arguments":{"repoName":LIVE_REPOSITORY}}});
            assert_eq!(validated_live_action(&good).unwrap(), Some(name));
            for args in [
                serde_json::json!({"repoName":"private/repo"}),
                serde_json::json!({"repoName":LIVE_REPOSITORY,"question":"private"}),
                serde_json::json!({}),
            ] {
                let mut bad = good.clone();
                bad["params"]["arguments"] = args;
                assert!(validated_live_action(&bad).is_err());
            }
        }
        assert!(validated_live_action(&serde_json::json!({"method":"tools/call","params":{"name":"ask_question","arguments":{"repoName":LIVE_REPOSITORY}}})).is_err());
        assert!(validated_live_action(&serde_json::json!({"method":"resources/read"})).is_err());
    }

    #[test]
    fn account_consent_requires_the_exact_owned_origin_and_path() {
        assert!(validate_consent_url("https://127.0.0.1:9001/authorize?state=owned", 9001).is_ok());
        for bad in [
            "http://127.0.0.1:9001/authorize",
            "https://127.0.0.1:9002/authorize",
            "https://localhost:9001/authorize",
            "https://127.0.0.1:9001/token",
            "https://user@127.0.0.1:9001/authorize",
            "https://127.0.0.1:9001/authorize#fragment",
        ] {
            assert!(validate_consent_url(bad, 9001).is_err());
        }
    }

    #[test]
    fn linear_preflight_refuses_writes_broader_scopes_and_unowned_callbacks() {
        let mut good = reqwest_mcp::Url::parse("https://mcp.linear.app/authorize").unwrap();
        good.query_pairs_mut().extend_pairs([
            ("response_type", "code"),
            ("client_id", "test-client"),
            ("state", "test-state-owned-1234567890"),
            ("scope", "read"),
            ("resource", LINEAR_ENDPOINT),
            ("code_challenge_method", "S256"),
            (
                "code_challenge",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            ),
            ("redirect_uri", "http://127.0.0.1:9100/mcp/oauth/callback"),
        ]);
        assert!(validate_linear_consent_url(good.as_str()).is_ok());
        for (key, replacement) in [
            ("scope", "read write"),
            ("scope", "write"),
            ("scope", "openid"),
            ("resource", "https://mcp.linear.app/mcp"),
            ("code_challenge_method", "plain"),
            ("code_challenge", "short"),
            ("state", "short"),
            ("client_id", ""),
            ("redirect_uri", "http://localhost:9100/mcp/oauth/callback"),
            ("redirect_uri", "http://127.0.0.1:7124/mcp/oauth/callback"),
            (
                "redirect_uri",
                "http://127.0.0.1:9100/mcp/oauth/callback?code=private",
            ),
        ] {
            let pairs: Vec<_> = good
                .query_pairs()
                .map(|(k, v)| {
                    let v = if k == key {
                        replacement.to_string()
                    } else {
                        v.into_owned()
                    };
                    (k.into_owned(), v)
                })
                .collect();
            let mut bad = good.clone();
            bad.set_query(None);
            bad.query_pairs_mut().extend_pairs(pairs);
            assert!(validate_linear_consent_url(bad.as_str()).is_err());
        }
        let mut duplicate = good.clone();
        duplicate.query_pairs_mut().append_pair("scope", "read");
        assert!(validate_linear_consent_url(duplicate.as_str()).is_err());
        for method in [
            "tools/call",
            "resources/read",
            "prompts/get",
            "notifications/message",
        ] {
            assert!(validate_linear_method(&serde_json::json!({"method":method})).is_err());
        }
        assert!(validate_linear_method(&serde_json::json!({"method":"initialize"})).is_ok());
    }

    #[test]
    fn fixed_consent_handoffs_keep_provider_and_generation_ownership() {
        let primary = account_slot(AUTH_PROVIDER_ID).unwrap();
        let peer = account_slot(PEER_PROVIDER_ID).unwrap();
        assert_ne!(primary, peer);
        assert!(account_slot("linear").is_err());
        let old = uuid::Uuid::new_v4();
        let current = uuid::Uuid::new_v4();
        let peer_owner = uuid::Uuid::new_v4();
        {
            let mut slots = AUTHORIZATION.lock().unwrap();
            slots[primary] = Some((current, "primary-owned".into()));
            slots[peer] = Some((peer_owner, "peer-owned".into()));
        }
        drop(ConsentGuard {
            owner: old,
            slot: primary,
        });
        assert!(AUTHORIZATION.lock().unwrap()[primary].is_some());
        drop(ConsentGuard {
            owner: current,
            slot: primary,
        });
        assert!(AUTHORIZATION.lock().unwrap()[primary].is_none());
        assert!(AUTHORIZATION.lock().unwrap()[peer].is_some());
        drop(ConsentGuard {
            owner: peer_owner,
            slot: peer,
        });
        assert!(AUTHORIZATION.lock().unwrap().iter().all(Option::is_none));
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Enable,
    Disable,
    Discover,
    Connect,
    Disconnect,
    Authorization,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    #[default]
    Tools,
    Account,
    Client,
    Peer,
    PeerClient,
    Linear,
}

#[tauri::command]
pub async fn agent_harness_mcp(
    app: AppHandle,
    window: WebviewWindow,
    operation: Operation,
    target: Option<Target>,
) -> Result<serde_json::Value, String> {
    super::grain_agent_harness::guard(&app, &window)?;
    let id = match target.unwrap_or_default() {
        Target::Tools => {
            endpoint()?;
            PROVIDER_ID
        }
        Target::Account => {
            auth_endpoint(AUTH_PROVIDER_ID)?;
            AUTH_PROVIDER_ID
        }
        Target::Client => {
            auth_endpoint(CLIENT_PROVIDER_ID)?;
            CLIENT_PROVIDER_ID
        }
        Target::Peer => {
            auth_endpoint(PEER_PROVIDER_ID)?;
            PEER_PROVIDER_ID
        }
        Target::PeerClient => {
            auth_endpoint(PEER_CLIENT_PROVIDER_ID)?;
            PEER_CLIENT_PROVIDER_ID
        }
        Target::Linear => {
            linear_endpoint()?;
            LINEAR_PROVIDER_ID
        }
    };
    match operation {
        Operation::Enable | Operation::Disable => {
            let enabled = matches!(operation, Operation::Enable);
            super::grain_mcp::mcp_set_provider_enabled(app, window, id.into(), enabled).await?;
            Ok(serde_json::json!({"enabled": enabled}))
        }
        Operation::Discover => {
            serde_json::to_value(super::grain_mcp::mcp_test_provider(app, window, id.into()).await?)
                .map_err(|_| "Cannot encode bounded discovery result".into())
        }
        Operation::Connect => {
            super::grain_mcp::mcp_connect_provider(app, window, id.into()).await?;
            Ok(serde_json::json!({"connected": true}))
        }
        Operation::Disconnect => {
            super::grain_mcp::mcp_disconnect_provider(app, window, id.into()).await?;
            Ok(serde_json::json!({"connected": false}))
        }
        Operation::Authorization => {
            if !ACCOUNT_IDS.contains(&id) && id != LINEAR_PROVIDER_ID {
                return Err("MCP account fixture is not enabled".into());
            }
            let mut slot = AUTHORIZATION
                .lock()
                .map_err(|_| "Consent handoff unavailable")?;
            Ok(slot[account_slot(id)?]
                .take()
                .map(|(_, url)| serde_json::Value::String(url))
                .unwrap_or(serde_json::Value::Null))
        }
    }
}

// Five fixed handoffs; taking/cancelling one cannot consume another provider.
static AUTHORIZATION: std::sync::Mutex<[Option<(uuid::Uuid, String)>; 5]> =
    std::sync::Mutex::new([None, None, None, None, None]);

fn account_slot(id: &str) -> Result<usize, String> {
    if id == LINEAR_PROVIDER_ID {
        return Ok(4);
    }
    ACCOUNT_IDS
        .iter()
        .position(|candidate| *candidate == id)
        .ok_or_else(|| "Unknown MCP account fixture".into())
}

pub(crate) struct ConsentGuard {
    owner: uuid::Uuid,
    slot: usize,
}
impl Drop for ConsentGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = AUTHORIZATION.lock() {
            if slot[self.slot]
                .as_ref()
                .is_some_and(|(owner, _)| *owner == self.owner)
            {
                slot[self.slot] = None;
            }
        }
    }
}

/// Replace only the fixed fixture's browser handoff, never SDK state/PKCE/exchange.
/// Consumed privately by the runner; the URL is not status or report evidence.
pub(crate) fn capture_authorization(id: &str, raw: &str) -> Result<Option<ConsentGuard>, String> {
    if id == LINEAR_PROVIDER_ID {
        linear_endpoint()?;
        validate_linear_consent_url(raw)?;
    } else if !ACCOUNT_IDS.contains(&id) {
        return Ok(None);
    } else {
        if !super::grain_agent_harness::mcp_auth_enabled() {
            return Err("MCP account fixture is not enabled".into());
        }
        let (_, port) = account_config(id)?;
        validate_consent_url(raw, port)?;
    }
    let owner = uuid::Uuid::new_v4();
    let slot = account_slot(id)?;
    AUTHORIZATION
        .lock()
        .map_err(|_| "Consent handoff unavailable")?[slot] = Some((owner, raw.into()));
    Ok(Some(ConsentGuard { owner, slot }))
}

fn validate_linear_consent_url(raw: &str) -> Result<(), String> {
    let rejected = || "Linear preflight requires exact read-only SDK consent".to_string();
    let url = reqwest_mcp::Url::parse(raw).map_err(|_| rejected())?;
    if raw.len() > 8192
        || url.origin().ascii_serialization() != "https://mcp.linear.app"
        || url.path() != "/authorize"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(rejected());
    }
    let pairs: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
    if pairs.len() != url.query_pairs().count()
        || pairs.len() != 8
        || pairs.get("scope").map(|s| s.as_ref()) != Some("read")
        || pairs.get("resource").map(|s| s.as_ref()) != Some(LINEAR_ENDPOINT)
        || pairs.get("response_type").map(|s| s.as_ref()) != Some("code")
        || pairs.get("code_challenge_method").map(|s| s.as_ref()) != Some("S256")
        || !pairs
            .get("state")
            .is_some_and(|s| s.len() >= 16 && s.len() <= 4096)
        || !pairs
            .get("client_id")
            .is_some_and(|s| !s.is_empty() && s.len() <= 4096)
        || !pairs.get("code_challenge").is_some_and(|s| {
            s.len() == 43
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    {
        return Err(rejected());
    }
    let redirect = reqwest_mcp::Url::parse(pairs.get("redirect_uri").ok_or_else(rejected)?)
        .map_err(|_| rejected())?;
    if redirect.scheme() != "http"
        || redirect.host_str() != Some("127.0.0.1")
        || !redirect
            .port()
            .is_some_and(|p| p != 0 && p != 7124 && p != 17124)
        || redirect.path() != "/mcp/oauth/callback"
        || !redirect.username().is_empty()
        || redirect.password().is_some()
        || redirect.query().is_some()
        || redirect.fragment().is_some()
    {
        return Err(rejected());
    }
    Ok(())
}

fn validate_consent_url(raw: &str, port: u16) -> Result<(), String> {
    let url = reqwest_mcp::Url::parse(raw).map_err(|_| "Invalid fixture consent URL")?;
    if raw.len() > 8192
        || url.scheme() != "https"
        || url.host_str() != Some("127.0.0.1")
        || url.port() != Some(port)
        || url.path() != "/authorize"
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("MCP consent requires its exact HTTPS fixture origin/path".into());
    }
    Ok(())
}
