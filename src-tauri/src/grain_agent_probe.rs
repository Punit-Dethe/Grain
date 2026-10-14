//! Debug-only, one-shot MCP acceptance through the existing headless app path.
//! No webview, listener, audio stack, settings override or second Agent loop.
use std::{io::Read, path::Path, sync::Arc, time::Instant};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

const ALPHA_ID: &str = "com.grain.extensions.alpha";
const MAX_REQUEST_BYTES: u64 = 16 * 1024;

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Catalog {},
    Agent { prompt: String, execute: bool },
}

fn read_request(path: &Path) -> Result<Request, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "Cannot open probe request")?
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read probe request")?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err("Probe request exceeds 16 KiB".into());
    }
    let request: Request = serde_json::from_slice(&bytes).map_err(|_| "Invalid probe request")?;
    if let Request::Agent { prompt, execute } = &request {
        if !execute || prompt.trim().is_empty() || prompt.len() > 4096 {
            return Err("Agent probe needs execute:true and a nonempty prompt under 4 KiB".into());
        }
    }
    Ok(request)
}

pub(crate) fn check_identity(app: &AppHandle) -> Result<(), String> {
    if app.config().identifier != ALPHA_ID {
        return Err("Agent probe requires the tester alpha application identity".into());
    }
    Ok(())
}

async fn run(app: &AppHandle, request: Request) -> Result<Value, String> {
    check_identity(app)?;
    let context = app.state::<Arc<grain_core::AppContext>>();
    if !context.settings().extension_developer_mode {
        return Err("Enable extension developer mode in the alpha before probing".into());
    }
    // Only MCP state is needed. Native workers deliberately remain unavailable;
    // their real-app acceptance stays in the maintained harness.
    app.manage(Arc::new(
        grain_core::extensions::ExtensionsRegistry::load(
            &context.data_dir,
            grain_core::settings_file_exists(&context.data_dir),
        )
        .map_err(|_| "Cannot load the extension registry")?,
    ));
    app.manage(Arc::new(crate::grain_store::StoreState::init(
        &context.data_dir,
    )));
    app.manage(
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|_| "Cannot build model client")?,
    );
    app.manage(crate::grain_mcp::McpHttpClient::build()?);
    app.manage(crate::grain_mcp::connections::State::default());
    app.manage(crate::agent::AgentState::default());
    let started = Instant::now();
    let result = match request {
        Request::Catalog {} => {
            let directory = crate::grain_mcp::directory(app);
            let enabled_count = directory.len();
            let mut catalogs = Vec::new();
            for entry in directory.into_iter().take(16) {
                let provider = entry
                    .extension_id
                    .strip_prefix("mcp.")
                    .ok_or("Invalid MCP identity")?;
                let catalog = crate::grain_mcp::list_tools(app, provider).await?;
                let metadata: Vec<_> = catalog.tools.iter().map(|tool| {
                    let input = crate::capability::mcp_action_input(&entry.extension_id, &catalog.provider_name, tool);
                    json!({"canonical_id":input.canonical_id,"title":input.title,"description":input.description,"provider_name":input.provider_name})
                }).collect();
                catalogs.push(json!({"extension_id":entry.extension_id,"digest":catalog.digest,"tools":catalog.tools,"metadata":metadata}));
            }
            json!({"coverage":{"enabled":enabled_count,"exported":catalogs.len(),"truncated":enabled_count > 16},"catalogs":catalogs})
        }
        Request::Agent { prompt, .. } => {
            let reply = crate::agent::agent_run(
                app.clone(),
                vec![crate::agent::AgentMessage {
                    role: "user".into(),
                    content: prompt,
                }],
                None,
            )
            .await?;
            json!({"reply":reply})
        }
    };
    Ok(
        json!({"schema":1,"profile":ALPHA_ID,"scope":"MCP/backend-only","elapsed_ms":started.elapsed().as_millis(),"result":result}),
    )
}

pub(crate) fn launch(app: AppHandle, path: std::path::PathBuf) {
    tauri::async_runtime::spawn(async move {
        let result = match read_request(&path) {
            Ok(request) => {
                tokio::time::timeout(std::time::Duration::from_secs(180), run(&app, request))
                    .await
                    .unwrap_or_else(|_| {
                        Err("Probe deadline elapsed; do not replay any dispatched action".into())
                    })
            }
            Err(error) => Err(error),
        };
        let code = if result.is_ok() { 0 } else { 1 };
        match result {
            Ok(report) => println!("{report}"),
            Err(error) => println!("{}", json!({"schema":1,"error":error})),
        }
        // A normal window close keeps Grain in the tray. This one-shot owns its
        // process and must release SDK state/DLLs after writing the report.
        crate::INTENTIONAL_QUIT.store(true, std::sync::atomic::Ordering::Relaxed);
        app.exit(code);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_requests_are_explicit_and_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("request.json");
        for raw in [
            r#"{"mode":"agent","prompt":"search pages","execute":false}"#,
            r#"{"mode":"agent","prompt":" ","execute":true}"#,
            r#"{"mode":"catalog","extra":true}"#,
        ] {
            std::fs::write(&path, raw).unwrap();
            assert!(read_request(&path).is_err());
        }
        std::fs::write(&path, r#"{"mode":"catalog"}"#).unwrap();
        assert!(matches!(read_request(&path).unwrap(), Request::Catalog {}));
        std::fs::write(&path, "x".repeat(MAX_REQUEST_BYTES as usize + 1)).unwrap();
        assert!(read_request(&path).is_err());
    }
}
