//! Fixed unauthenticated MCP tests, absent from ordinary/release builds.
//! Reuses production discovery, approvals, SDK, byte limits and cancellation.
use serde::Deserialize;
use tauri::{AppHandle, WebviewWindow};

pub(crate) const PROVIDER_ID: &str = "grain-harness";
pub(crate) const LIVE_ENDPOINT: &str = "https://mcp.deepwiki.com/mcp";
const LIVE_REPOSITORY: &str = "modelcontextprotocol/rust-sdk";

pub(crate) fn endpoint() -> Result<String, String> {
    if super::grain_agent_harness::live_deepwiki_enabled() {
        return Ok(LIVE_ENDPOINT.into());
    }
    let (_, port) = super::grain_agent_harness::mcp_fixture_config()?;
    Ok(format!("https://127.0.0.1:{port}/mcp"))
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
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Enable,
    Disable,
    Discover,
}

#[tauri::command]
pub async fn agent_harness_mcp(
    app: AppHandle,
    window: WebviewWindow,
    operation: Operation,
) -> Result<serde_json::Value, String> {
    super::grain_agent_harness::guard(&app, &window)?;
    endpoint()?;
    match operation {
        Operation::Enable | Operation::Disable => {
            let enabled = matches!(operation, Operation::Enable);
            super::grain_mcp::mcp_set_provider_enabled(app, window, PROVIDER_ID.into(), enabled)
                .await?;
            Ok(serde_json::json!({"enabled": enabled}))
        }
        Operation::Discover => serde_json::to_value(
            super::grain_mcp::mcp_test_provider(app, window, PROVIDER_ID.into()).await?,
        )
        .map_err(|_| "Cannot encode bounded discovery result".into()),
    }
}
