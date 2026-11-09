//! Test controls for a disposable real Agent host. Absent from normal builds.
//! No arbitrary paths, executable tools, credentials, or confirmation bypass.

#[cfg(not(debug_assertions))]
compile_error!("agent-harness must never be enabled in a release build");

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, WebviewWindow};

const APP_ID: &str = "com.grain.agent-harness";
const MAX_EVENTS: usize = 4096;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Marker {
    schema: u32,
    run_id: String,
    model_port: u16,
    #[serde(default)]
    mcp_port: Option<u16>,
    #[serde(default)]
    mcp_peer_port: Option<u16>,
    #[serde(default)]
    mcp_live_deepwiki: bool,
    #[serde(default)]
    mcp_live_huggingface: bool,
    #[serde(default)]
    mcp_live_linear: bool,
    #[serde(default)]
    mcp_linear_consent: bool,
    #[serde(default)]
    mcp_auth: bool,
    #[serde(default)]
    mcp_issuer_rotation: bool,
}

struct Config {
    root: PathBuf,
    data: PathBuf,
    marker: Marker,
    started: Instant,
}

static CONFIG: OnceLock<Config> = OnceLock::new();
static EVENTS: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static OVERFLOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn read_marker(root: &Path) -> Result<Marker, String> {
    if !root.is_absolute() || !root.is_dir() {
        return Err("Harness root must be an existing absolute directory".into());
    }
    use std::io::Read;
    let file = std::fs::File::open(root.join(".grain-agent-harness.json"))
        .map_err(|_| "Harness marker is missing")?;
    let mut bytes = Vec::new();
    file.take(1025)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read harness marker")?;
    if bytes.len() > 1024 {
        return Err("Harness marker is oversized".into());
    }
    let marker: Marker = serde_json::from_slice(&bytes).map_err(|_| "Invalid harness marker")?;
    if marker.schema != 1
        || (marker.mcp_live_huggingface
            && (marker.mcp_live_deepwiki
                || marker.mcp_live_linear
                || marker.mcp_auth
                || marker.mcp_port.is_some()
                || marker.mcp_peer_port.is_some()))
        || (marker.mcp_linear_consent && !marker.mcp_live_linear)
        || (marker.mcp_live_linear
            && (marker.mcp_live_deepwiki
                || marker.mcp_auth
                || marker.mcp_port.is_some()
                || marker.mcp_peer_port.is_some()))
        || (marker.mcp_live_deepwiki && marker.mcp_port.is_some())
        || (marker.mcp_auth && (marker.mcp_port.is_none() || marker.mcp_live_deepwiki))
        || (marker.mcp_peer_port.is_some() && !marker.mcp_auth)
        || (marker.mcp_issuer_rotation && (!marker.mcp_auth || marker.mcp_peer_port.is_none()))
        || uuid::Uuid::parse_str(&marker.run_id).is_err()
        || marker.model_port == 0
        || marker
            .mcp_port
            .is_some_and(|port| port == 0 || port == marker.model_port)
        || marker.mcp_peer_port.is_some_and(|port| {
            port == 0 || port == marker.model_port || Some(port) == marker.mcp_port
        })
    {
        return Err("Invalid harness run identity or model port".into());
    }
    Ok(marker)
}

pub fn init() {
    CONFIG.get_or_init(|| {
        let raw = std::env::var_os("GRAIN_AGENT_HARNESS_ROOT")
            .expect("Harness refuses to start without an isolated root");
        let root = PathBuf::from(raw);
        let marker = read_marker(&root).expect("Harness refuses an invalid isolation marker");
        let root = root.canonicalize().expect("canonical harness root");
        let data = root.join("data");
        std::fs::create_dir_all(&data).expect("create harness data directory");
        assert_eq!(
            data.canonicalize().unwrap().parent(),
            Some(root.as_path()),
            "Harness data must not escape its root"
        );
        std::env::set_var("HF_HOME", data.join("huggingface"));
        Config {
            root,
            data,
            marker,
            started: Instant::now(),
        }
    });
}

fn config() -> &'static Config {
    CONFIG.get().expect("harness initialized before Tauri")
}

pub(super) fn mcp_fixture_config() -> Result<(PathBuf, u16), String> {
    let value = CONFIG.get().ok_or("MCP fixture is not initialized")?;
    let port = value.marker.mcp_port.ok_or("MCP fixture is not enabled")?;
    Ok((value.root.clone(), port))
}

pub(super) fn live_deepwiki_enabled() -> bool {
    CONFIG
        .get()
        .is_some_and(|value| value.marker.mcp_live_deepwiki)
}

pub(super) fn live_huggingface_enabled() -> bool {
    CONFIG
        .get()
        .is_some_and(|value| value.marker.mcp_live_huggingface)
}

pub(super) fn mcp_issuer_rotation_enabled() -> bool {
    CONFIG
        .get()
        .is_some_and(|value| value.marker.mcp_issuer_rotation)
}

pub(super) fn mcp_auth_enabled() -> bool {
    CONFIG.get().is_some_and(|value| value.marker.mcp_auth)
}

pub(super) fn live_linear_enabled() -> bool {
    CONFIG
        .get()
        .is_some_and(|value| value.marker.mcp_live_linear)
}

pub(super) fn linear_consent_enabled() -> bool {
    CONFIG
        .get()
        .is_some_and(|value| value.marker.mcp_linear_consent)
}

pub(super) fn mcp_peer_config() -> Result<(PathBuf, u16), String> {
    let value = CONFIG.get().ok_or("MCP peer fixture is not initialized")?;
    let port = value
        .marker
        .mcp_peer_port
        .ok_or("MCP peer fixture is not enabled")?;
    Ok((value.root.clone(), port))
}

pub fn vault_service(service: &str) -> String {
    format!("{service}.agent-harness.{}", config().marker.run_id)
}

// Portable-compatible API without editing the Handy-derived module.
pub mod portable {
    use super::*;
    pub fn init() {
        super::init();
    }
    pub fn is_portable() -> bool {
        true
    }
    pub fn data_dir() -> Option<&'static PathBuf> {
        Some(&config().data)
    }
    pub fn app_data_dir(_: &AppHandle) -> Result<PathBuf, tauri::Error> {
        Ok(config().data.clone())
    }
    pub fn app_log_dir(_: &AppHandle) -> Result<PathBuf, tauri::Error> {
        Ok(config().data.join("logs"))
    }
    pub fn resolve_app_data(app: &AppHandle, relative: &str) -> Result<PathBuf, tauri::Error> {
        Ok(app_data_dir(app)?.join(relative))
    }
    pub fn store_path(relative: &str) -> PathBuf {
        config().data.join(relative)
    }
}

pub fn configure(app: &AppHandle) -> Result<(), String> {
    if app.config().identifier != APP_ID {
        return Err(
            "Harness requires the separate com.grain.agent-harness application identifier".into(),
        );
    }
    let context = app.state::<std::sync::Arc<grain_core::AppContext>>();
    if context.data_dir != config().data {
        return Err("Harness profile isolation failed".into());
    }
    let mut settings = context.settings();
    settings.bindings = crate::settings::get_default_settings().bindings;
    for binding in settings.bindings.values_mut() {
        binding.current_binding.clear();
    }
    settings.keyboard_implementation = crate::settings::KeyboardImplementation::HandyKeys;
    settings.always_on_microphone = false;
    settings.start_hidden = false;
    settings.agent_enabled = true;
    settings.agent_screen_image = false;
    settings.extension_developer_mode = true;
    // Preserve only exact anonymous records belonging to this isolated peer.
    // A prefix alone would also retain unrelated/custom remote destinations.
    let configured_keys: Vec<String> = if config().marker.mcp_port.is_some() {
        grain_core::mcp_connections::ConnectionRegistry::load(&context.data_dir)
            .and_then(|registry| registry.list())
            // Corrupt metadata must remain preserved and command-visible; it
            // cannot authorize enablement or abort unrelated Agent startup.
            .unwrap_or_default()
            .into_iter()
            .filter(|record| {
                (record.definition().url == crate::grain_agent_harness_mcp::CONFIGURED_ENDPOINT
                    && matches!(
                        record.definition().authentication,
                        grain_sdk::mcp::McpAuthentication::None {}
                    ))
                    || (mcp_auth_enabled()
                        && record.definition().url
                            == crate::grain_agent_harness_mcp::CONFIGURED_AUTH_ENDPOINT
                        && matches!(
                            record.definition().authentication,
                            grain_sdk::mcp::McpAuthentication::OAuth {}
                        ))
            })
            .map(|record| record.identity().vault_account().into_owned())
            .collect()
    } else {
        Vec::new()
    };
    // Preserve only this run's explicitly enabled peer across real restarts.
    // Ordinary provider entries remain excluded from the isolated host.
    settings.mcp_enabled_providers.retain(|id| {
        configured_keys.contains(id)
            || (live_linear_enabled() && id == crate::grain_agent_harness_mcp::LINEAR_PROVIDER_ID)
            || ((config().marker.mcp_port.is_some()
                || live_deepwiki_enabled()
                || live_huggingface_enabled())
                && id == crate::grain_agent_harness_mcp::PROVIDER_ID)
            || (mcp_auth_enabled()
                && [
                    crate::grain_agent_harness_mcp::AUTH_PROVIDER_ID,
                    crate::grain_agent_harness_mcp::CLIENT_PROVIDER_ID,
                ]
                .contains(&id.as_str()))
            || (mcp_peer_config().is_ok()
                && [
                    crate::grain_agent_harness_mcp::PEER_PROVIDER_ID,
                    crate::grain_agent_harness_mcp::PEER_CLIENT_PROVIDER_ID,
                ]
                .contains(&id.as_str()))
    });
    settings.mcp_oauth_client_ids.retain(|id, _| {
        configured_keys.contains(id)
            || (mcp_auth_enabled() && id == crate::grain_agent_harness_mcp::CLIENT_PROVIDER_ID)
            || (mcp_peer_config().is_ok()
                && id == crate::grain_agent_harness_mcp::PEER_CLIENT_PROVIDER_ID)
    });
    settings.post_process_api_keys.0.clear();
    settings.post_process_fallback_enabled = false;
    settings.post_process_provider_id = "custom".into();
    settings.post_process_providers = vec![crate::settings::PostProcessProvider {
        api_format: crate::settings::PostProcessApiFormat::OpenAi,
        id: "custom".into(),
        label: "Harness scripted model".into(),
        base_url: format!("http://127.0.0.1:{}/v1", config().marker.model_port),
        allow_base_url_edit: false,
        models_endpoint: None,
        supports_structured_output: false,
        enabled: true,
    }];
    settings.post_process_models.clear();
    settings
        .post_process_models
        .insert("custom".into(), "harness-scripted".into());
    context
        .replace_settings(settings)
        .map_err(|e| e.to_string())?;
    // Ordinary init falls back to default accelerators when bindings are
    // absent. Own the same production manager without registering any core
    // accelerator; only Agent's subsequent transient Escape is exercised.
    app.manage(crate::shortcut::handy_keys::HandyKeysState::new(
        app.clone(),
    )?);
    app.manage(crate::commands::ShortcutsInitialized);
    Ok(())
}

pub(super) fn guard(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    if window.label() != "main"
        || app.config().identifier != APP_ID
        || app
            .state::<std::sync::Arc<grain_core::AppContext>>()
            .data_dir
            != config().data
    {
        return Err("Harness controls require the isolated main window".into());
    }
    Ok(())
}

pub fn observe(phase: &str, action: &str) {
    let mut events = EVENTS.lock().unwrap();
    if events.len() >= MAX_EVENTS {
        OVERFLOW.store(true, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    events.push(json!({"phase": phase, "extension": "mcp.grain-harness", "action": action, "elapsedMs": config().started.elapsed().as_millis() as u64}));
}

#[tauri::command]
pub fn agent_harness_status(app: AppHandle, window: WebviewWindow) -> Result<Value, String> {
    guard(&app, &window)?;
    if OVERFLOW.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("Harness evidence buffer overflowed".into());
    }
    Ok(json!({
        "schema": 1, "runId": config().marker.run_id, "profile": config().data,
        "applicationId": app.config().identifier,
        "shortcutBindings": context_shortcut_bindings(&app),
        "agent": crate::agent::harness_snapshot(&app),
        "events": *EVENTS.lock().unwrap(),
    }))
}

fn context_shortcut_bindings(app: &AppHandle) -> Vec<String> {
    crate::settings::get_settings(app)
        .bindings
        .into_values()
        .filter(|binding| !binding.current_binding.is_empty())
        .map(|binding| binding.id)
        .collect()
}

#[tauri::command]
pub fn agent_harness_set_mcp_development(
    app: AppHandle,
    window: WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    guard(&app, &window)?;
    if !enabled {
        crate::grain_mcp::invalidate_development_sessions();
    }
    app.state::<std::sync::Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.extension_developer_mode = enabled)
        .map_err(|error| error.to_string())?;
    crate::refresh_webview_log_streaming(&app);
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Instruction {
    McpWorkflow,
    McpRead,
    McpConfiguredRead,
    McpConfiguredAccountA,
    McpConfiguredAccountB,
    McpConfiguredDisabledA,
    McpConfiguredDisabledB,
    McpConfiguredUnknown,
    McpExcluded,
    McpPreview,
    McpUnknown,
    McpCatalogRefusal,
    McpConformance,
    McpLiveRead,
    McpLiveLarge,
    McpHfRead,
    McpAccountA,
    McpAccountB,
    McpClientA,
    McpClientB,
    McpPeerB,
    McpPeerClientB,
    McpDisabledOwner,
    Hello,
    ModelWait,
}

#[tauri::command]
pub async fn agent_harness_submit(
    app: AppHandle,
    window: WebviewWindow,
    instruction: Instruction,
) -> Result<(), String> {
    guard(&app, &window)?;
    let text = match instruction {
        Instruction::McpWorkflow => "Harness request: mcp_workflow",
        Instruction::McpRead => "Harness request: mcp_read",
        Instruction::McpConfiguredAccountA => "Harness request: mcp_configured_account_a",
        Instruction::McpConfiguredAccountB => "Harness request: mcp_configured_account_b",
        Instruction::McpConfiguredDisabledA => "Harness request: mcp_configured_disabled_a",
        Instruction::McpConfiguredDisabledB => "Harness request: mcp_configured_disabled_b",
        Instruction::McpConfiguredRead => "Harness request: mcp_configured_read",
        Instruction::McpConfiguredUnknown => "Harness request: mcp_configured_unknown",
        Instruction::McpExcluded => "Harness request: mcp_excluded",
        Instruction::McpPreview => "Harness request: mcp_preview",
        Instruction::McpUnknown => "Harness request: mcp_unknown",
        Instruction::McpCatalogRefusal => "Harness request: mcp_catalog_refusal",
        Instruction::McpConformance => "Harness request: mcp_conformance",
        Instruction::McpLiveRead => "Harness request: mcp_live_read",
        Instruction::McpLiveLarge => "Harness request: mcp_live_large",
        Instruction::McpHfRead => "Harness request: mcp_hf_read",
        Instruction::McpAccountA => "Harness request: mcp_account_a",
        Instruction::McpAccountB => "Harness request: mcp_account_b",
        Instruction::McpClientA => "Harness request: mcp_client_a",
        Instruction::McpClientB => "Harness request: mcp_client_b",
        Instruction::McpPeerB => "Harness request: mcp_peer_b",
        Instruction::McpPeerClientB => "Harness request: mcp_peer_client_b",
        Instruction::McpDisabledOwner => "Harness request: mcp_disabled_owner",
        Instruction::Hello => "Harness request: hello",
        Instruction::ModelWait => "Harness request: model_wait",
    };
    crate::agent::harness_submit_instruction(&app, text.into());
    Ok(())
}

#[tauri::command]
pub fn agent_harness_shutdown(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    guard(&app, &window)?;
    // Use the same intentional-quit ownership as the tray command; otherwise
    // Grain's keep-alive policy can veto this explicit harness shutdown.
    crate::INTENTIONAL_QUIT.store(true, std::sync::atomic::Ordering::Relaxed);
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolation_marker_rejects_missing_relative_and_invalid_identity() {
        assert!(read_marker(Path::new("relative")).is_err());
        let root = tempfile::tempdir().unwrap();
        assert!(read_marker(root.path()).is_err());
        let marker = root.path().join(".grain-agent-harness.json");
        std::fs::write(
            &marker,
            r#"{"schema":1,"runId":"not-uuid","modelPort":9000}"#,
        )
        .unwrap();
        assert!(read_marker(root.path()).is_err());
        std::fs::write(
            &marker,
            format!(
                r#"{{"schema":1,"runId":"{}","modelPort":9000}}"#,
                uuid::Uuid::new_v4()
            ),
        )
        .unwrap();
        assert!(read_marker(root.path()).is_ok());
        for port in [0, 9000] {
            std::fs::write(
                &marker,
                format!(
                    r#"{{"schema":1,"runId":"{}","modelPort":9000,"mcpPort":{port}}}"#,
                    uuid::Uuid::new_v4(),
                ),
            )
            .unwrap();
            assert!(read_marker(root.path()).is_err());
        }
    }

    #[test]
    fn public_mcp_marker_is_explicit_and_excludes_a_local_peer() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join(".grain-agent-harness.json");
        for (live, port, valid) in [
            (true, None, true),
            (true, Some(9001), false),
            (false, Some(9001), true),
        ] {
            std::fs::write(&marker, serde_json::to_vec(&serde_json::json!({"schema":1,"runId":uuid::Uuid::new_v4(),"modelPort":9000,"mcpLiveDeepwiki":live,"mcpPort":port})).unwrap()).unwrap();
            assert_eq!(read_marker(root.path()).is_ok(), valid);
        }
    }

    #[test]
    fn public_nested_marker_excludes_all_other_fixture_and_account_sources() {
        let root = tempfile::tempdir().unwrap();
        let marker = serde_json::json!({"schema":1,"runId":uuid::Uuid::new_v4(),"modelPort":9000,"mcpLiveHuggingface":true});
        let path = root.path().join(".grain-agent-harness.json");
        std::fs::write(&path, serde_json::to_vec(&marker).unwrap()).unwrap();
        assert!(read_marker(root.path()).is_ok());
        for (key, value) in [
            ("mcpLiveDeepwiki", serde_json::json!(true)),
            ("mcpLiveLinear", serde_json::json!(true)),
            ("mcpLinearConsent", serde_json::json!(true)),
            ("mcpAuth", serde_json::json!(true)),
            ("mcpPort", serde_json::json!(9001)),
            ("mcpPeerPort", serde_json::json!(9002)),
            ("authPort", serde_json::json!(9003)),
        ] {
            let mut mixed = marker.clone();
            mixed[key] = value;
            std::fs::write(&path, serde_json::to_vec(&mixed).unwrap()).unwrap();
            assert!(read_marker(root.path()).is_err());
        }
    }

    #[test]
    fn authenticated_mcp_marker_requires_only_its_owned_local_peer() {
        let root = tempfile::tempdir().unwrap();
        for (port, live, valid) in [
            (None, false, false),
            (Some(9001), false, true),
            (Some(9001), true, false),
        ] {
            std::fs::write(root.path().join(".grain-agent-harness.json"), serde_json::to_vec(&serde_json::json!({"schema":1,"runId":uuid::Uuid::new_v4(),"modelPort":9000,"mcpPort":port,"mcpLiveDeepwiki":live,"mcpAuth":true})).unwrap()).unwrap();
            assert_eq!(read_marker(root.path()).is_ok(), valid);
        }
    }

    #[test]
    fn issuer_rotation_marker_requires_both_owned_authenticated_peers() {
        let root = tempfile::tempdir().unwrap();
        for (auth, peer, valid) in [
            (false, None, false),
            (true, None, false),
            (true, Some(9002), true),
        ] {
            let marker = serde_json::json!({"schema":1,"runId":uuid::Uuid::new_v4(),"modelPort":9000,"mcpPort":9001,"mcpPeerPort":peer,"mcpAuth":auth,"mcpIssuerRotation":true});
            std::fs::write(
                root.path().join(".grain-agent-harness.json"),
                serde_json::to_vec(&marker).unwrap(),
            )
            .unwrap();
            assert_eq!(read_marker(root.path()).is_ok(), valid);
        }
    }

    #[test]
    fn second_mcp_origin_requires_auth_and_a_distinct_owned_port() {
        let root = tempfile::tempdir().unwrap();
        for (auth, port, valid) in [
            (false, 9004, false),
            (true, 9004, true),
            (true, 0, false),
            (true, 9000, false),
            (true, 9001, false),
            (true, 9003, false),
        ] {
            std::fs::write(root.path().join(".grain-agent-harness.json"), serde_json::to_vec(&serde_json::json!({"schema":1,"runId":uuid::Uuid::new_v4(),"modelPort":9000,"mcpPort":9003,"mcpAuth":auth,"mcpPeerPort":port})).unwrap()).unwrap();
            assert_eq!(read_marker(root.path()).is_ok(), valid);
        }
    }

    #[test]
    fn live_linear_marker_cannot_mix_account_or_fixture_sources() {
        let root = tempfile::tempdir().unwrap();
        let marker = serde_json::json!({"schema":1,"runId":uuid::Uuid::new_v4(),"modelPort":9000,"mcpLiveLinear":true});
        std::fs::write(
            root.path().join(".grain-agent-harness.json"),
            serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
        assert!(read_marker(root.path()).is_ok());
        let mut consent = marker.clone();
        consent["mcpLinearConsent"] = serde_json::json!(true);
        std::fs::write(
            root.path().join(".grain-agent-harness.json"),
            serde_json::to_vec(&consent).unwrap(),
        )
        .unwrap();
        assert!(read_marker(root.path()).is_ok());
        consent["mcpLiveLinear"] = serde_json::json!(false);
        std::fs::write(
            root.path().join(".grain-agent-harness.json"),
            serde_json::to_vec(&consent).unwrap(),
        )
        .unwrap();
        assert!(read_marker(root.path()).is_err());
        for (key, value) in [
            ("mcpLiveDeepwiki", serde_json::json!(true)),
            ("mcpAuth", serde_json::json!(true)),
            ("mcpPort", serde_json::json!(9001)),
            ("mcpPeerPort", serde_json::json!(9002)),
            ("authPort", serde_json::json!(9003)),
        ] {
            let mut mixed = marker.clone();
            mixed[key] = value;
            std::fs::write(
                root.path().join(".grain-agent-harness.json"),
                serde_json::to_vec(&mixed).unwrap(),
            )
            .unwrap();
            assert!(read_marker(root.path()).is_err());
        }
    }
}
