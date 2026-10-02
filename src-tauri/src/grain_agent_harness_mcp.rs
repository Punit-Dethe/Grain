//! Fixed MCP acceptance peers, absent from ordinary/release builds.
//! Reuses production discovery, approvals, SDK, byte limits and cancellation.
use serde::Deserialize;
use tauri::{AppHandle, WebviewWindow};

pub(crate) const PROVIDER_ID: &str = "grain-harness";
pub(crate) const AUTH_PROVIDER_ID: &str = "grain-harness-auth";
pub(crate) const CLIENT_PROVIDER_ID: &str = "grain-harness-auth-client";
pub(crate) const LIVE_ENDPOINT: &str = "https://mcp.deepwiki.com/mcp";
const LIVE_REPOSITORY: &str = "modelcontextprotocol/rust-sdk";

pub(crate) fn endpoint() -> Result<String, String> {
    if super::grain_agent_harness::live_deepwiki_enabled() {
        return Ok(LIVE_ENDPOINT.into());
    }
    let (_, port) = super::grain_agent_harness::mcp_fixture_config()?;
    Ok(format!("https://127.0.0.1:{port}/mcp"))
}

pub(crate) fn auth_endpoint() -> Result<String, String> {
    if !super::grain_agent_harness::mcp_auth_enabled() {
        return Err("MCP account fixture is not enabled".into());
    }
    let (_, port) = super::grain_agent_harness::mcp_fixture_config()?;
    Ok(format!("https://127.0.0.1:{port}/account-mcp"))
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
            auth_endpoint()?;
            AUTH_PROVIDER_ID
        }
        Target::Client => {
            auth_endpoint()?;
            CLIENT_PROVIDER_ID
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
            if ![AUTH_PROVIDER_ID, CLIENT_PROVIDER_ID].contains(&id) {
                return Err("MCP account fixture is not enabled".into());
            }
            let mut slot = AUTHORIZATION
                .lock()
                .map_err(|_| "Consent handoff unavailable")?;
            Ok(slot
                .take()
                .map(|(_, url)| serde_json::Value::String(url))
                .unwrap_or(serde_json::Value::Null))
        }
    }
}

static AUTHORIZATION: std::sync::Mutex<Option<(uuid::Uuid, String)>> = std::sync::Mutex::new(None);

pub(crate) struct ConsentGuard(uuid::Uuid);
impl Drop for ConsentGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = AUTHORIZATION.lock() {
            if slot.as_ref().is_some_and(|(owner, _)| *owner == self.0) {
                *slot = None;
            }
        }
    }
}

/// Replace only the fixed fixture's browser handoff, never SDK state/PKCE/exchange.
/// Consumed privately by the runner; the URL is not status or report evidence.
pub(crate) fn capture_authorization(id: &str, raw: &str) -> Result<Option<ConsentGuard>, String> {
    if ![AUTH_PROVIDER_ID, CLIENT_PROVIDER_ID].contains(&id) {
        return Ok(None);
    }
    if !super::grain_agent_harness::mcp_auth_enabled() {
        return Err("MCP account fixture is not enabled".into());
    }
    let (_, port) = super::grain_agent_harness::mcp_fixture_config()?;
    validate_consent_url(raw, port)?;
    let owner = uuid::Uuid::new_v4();
    *AUTHORIZATION
        .lock()
        .map_err(|_| "Consent handoff unavailable")? = Some((owner, raw.into()));
    Ok(Some(ConsentGuard(owner)))
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
