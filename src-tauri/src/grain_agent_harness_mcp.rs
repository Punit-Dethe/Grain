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
const LIVE_REPOSITORY: &str = "modelcontextprotocol/rust-sdk";

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
            if !ACCOUNT_IDS.contains(&id) {
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

// Four fixed handoffs; taking/cancelling one cannot consume another provider.
static AUTHORIZATION: std::sync::Mutex<[Option<(uuid::Uuid, String)>; 4]> =
    std::sync::Mutex::new([None, None, None, None]);

fn account_slot(id: &str) -> Result<usize, String> {
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
    if !ACCOUNT_IDS.contains(&id) {
        return Ok(None);
    }
    if !super::grain_agent_harness::mcp_auth_enabled() {
        return Err("MCP account fixture is not enabled".into());
    }
    let (_, port) = account_config(id)?;
    validate_consent_url(raw, port)?;
    let owner = uuid::Uuid::new_v4();
    let slot = account_slot(id)?;
    AUTHORIZATION
        .lock()
        .map_err(|_| "Consent handoff unavailable")?[slot] = Some((owner, raw.into()));
    Ok(Some(ConsentGuard { owner, slot }))
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
