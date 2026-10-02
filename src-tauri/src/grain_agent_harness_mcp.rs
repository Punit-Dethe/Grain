//! Fixed unauthenticated MCP fixture, absent from ordinary/release builds.
//! Reuses production discovery, approvals, SDK, byte limits and cancellation.
use serde::Deserialize;
use tauri::{AppHandle, WebviewWindow};

pub(crate) const PROVIDER_ID: &str = "grain-harness";

pub(crate) fn endpoint() -> Result<String, String> {
    let (_, port) = super::grain_agent_harness::mcp_fixture_config()?;
    Ok(format!("https://127.0.0.1:{port}/mcp"))
}

pub(crate) fn client() -> Result<(String, reqwest_mcp::Client), String> {
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
