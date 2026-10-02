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

pub const FIXTURE_ID: &str = "com.grain.harness.lifecycle";
const APP_ID: &str = "com.grain.agent-harness";
const MAX_EVENTS: usize = 4096;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Marker {
    schema: u32,
    run_id: String,
    model_port: u16,
    #[serde(default)]
    store_port: Option<u16>,
    #[serde(default)]
    auth_port: Option<u16>,
    #[serde(default)]
    mcp_port: Option<u16>,
    #[serde(default)]
    mcp_live_deepwiki: bool,
    #[serde(default)]
    mcp_auth: bool,
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
        || (marker.mcp_live_deepwiki && marker.mcp_port.is_some())
        || (marker.mcp_auth && (marker.mcp_port.is_none() || marker.mcp_live_deepwiki))
        || uuid::Uuid::parse_str(&marker.run_id).is_err()
        || marker.model_port == 0
        || marker.model_port == crate::events_server::EVENTS_PORT
        || marker.model_port == 7124 // Ordinary Grain event listener.
        || marker.store_port.is_some_and(|port| {
            port == 0 || port == marker.model_port || port == 7124
                || port == crate::events_server::EVENTS_PORT
        })
        || marker.auth_port.is_some_and(|port| {
            port == 0 || port == marker.model_port || Some(port) == marker.store_port
                || port == 7124 || port == crate::events_server::EVENTS_PORT
        })
        || marker.mcp_port.is_some_and(|port| {
            port == 0 || port == marker.model_port || Some(port) == marker.store_port
                || Some(port) == marker.auth_port || port == 7124
                || port == crate::events_server::EVENTS_PORT
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

pub(super) fn auth_fixture_config() -> Result<(PathBuf, u16), String> {
    let value = CONFIG
        .get()
        .ok_or("Native auth fixture is not initialized")?;
    let port = value
        .marker
        .auth_port
        .ok_or("Native auth fixture is not enabled")?;
    Ok((value.root.clone(), port))
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

pub(super) fn mcp_auth_enabled() -> bool {
    CONFIG.get().is_some_and(|value| value.marker.mcp_auth)
}

/// Fixed, public test key only. No production root or signing key is replaced.
pub(crate) fn store_base(data: &Path) -> Option<String> {
    assert_eq!(
        data,
        config().data,
        "Store fixture requires the owned profile"
    );
    config()
        .marker
        .store_port
        .map(|port| format!("http://127.0.0.1:{port}/"))
}

pub(crate) const STORE_PUBLISHING_KEY: &str =
    "RWRncmFpbi1oMeKKiXB1MzK9cv70E+awsu8bSq3aeqLBQfIzcSpodrNR";

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
    settings.agent_autocopy = crate::settings::AgentAutocopy::Off;
    settings.agent_context_mode = crate::settings::AgentContextMode::Off;
    settings.agent_screen_image = false;
    settings.agent_quick_enabled = false;
    settings.agent_panel_position = crate::settings::AgentPanelPosition::Center;
    settings.extension_developer_mode = true;
    // Preserve only this run's explicitly enabled peer across real restarts.
    // Ordinary provider entries remain excluded from the isolated host.
    settings.mcp_enabled_providers.retain(|id| {
        ((config().marker.mcp_port.is_some() || live_deepwiki_enabled())
            && id == crate::grain_agent_harness_mcp::PROVIDER_ID)
            || (mcp_auth_enabled()
                && [
                    crate::grain_agent_harness_mcp::AUTH_PROVIDER_ID,
                    crate::grain_agent_harness_mcp::CLIENT_PROVIDER_ID,
                ]
                .contains(&id.as_str()))
    });
    settings.mcp_oauth_client_ids.retain(|id, _| {
        mcp_auth_enabled() && id == crate::grain_agent_harness_mcp::CLIENT_PROVIDER_ID
    });
    settings.post_process_api_keys.0.clear();
    settings.stt_api_keys.0.clear();
    settings.post_process_smart_rotation = false;
    settings.post_process_provider_id = "custom".into();
    settings.post_process_providers = vec![crate::settings::PostProcessProvider {
        id: "custom".into(),
        label: "Harness scripted model".into(),
        base_url: format!("http://127.0.0.1:{}/v1", config().marker.model_port),
        allow_base_url_edit: false,
        models_endpoint: None,
        supports_structured_output: false,
        enabled: true,
        quota_limit: None,
        quota_used_today: 0,
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

pub fn observe(phase: &str, extension: &str, action: &str, token: &str) {
    if ![
        FIXTURE_ID,
        super::grain_agent_harness_auth::FIXTURE_ID,
        super::grain_agent_harness_auth::PEER_ID,
        "mcp.grain-harness",
    ]
    .contains(&extension)
    {
        return;
    }
    let mut events = EVENTS.lock().unwrap();
    if events.len() >= MAX_EVENTS {
        OVERFLOW.store(true, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    events.push(json!({"phase": phase, "extension": extension, "action": action, "worker": worker_identity(token), "elapsedMs": config().started.elapsed().as_millis() as u64}));
}

pub fn worker_identity(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(token.as_bytes());
    digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Record host classification only: never retain tool arguments or result text.
pub fn observe_outcome(
    extension: &str,
    action: &str,
    outcome: &grain_core::execution::ActionOutcome,
) {
    use grain_core::execution::ActionOutcome;
    if ![
        FIXTURE_ID,
        super::grain_agent_harness_auth::FIXTURE_ID,
        super::grain_agent_harness_auth::PEER_ID,
    ]
    .contains(&extension)
    {
        return;
    }
    let status = match outcome {
        ActionOutcome::Succeeded(_) => "succeeded",
        ActionOutcome::Failed { .. } => "failed",
        ActionOutcome::Cancelled => "cancelled",
        ActionOutcome::UnknownOutcome { .. } => "unknownOutcome",
        ActionOutcome::ResultUnavailable { .. } => "resultUnavailable",
        ActionOutcome::ToolReportedError { .. } => "toolReportedError",
        ActionOutcome::NeedsInteraction(_) => "needsInteraction",
    };
    let mut events = EVENTS.lock().unwrap();
    if events.len() >= MAX_EVENTS {
        OVERFLOW.store(true, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    events.push(json!({"phase": "outcome", "extension": extension, "action": action, "outcome": status, "elapsedMs": config().started.elapsed().as_millis() as u64}));
}

#[tauri::command]
pub fn agent_harness_status(app: AppHandle, window: WebviewWindow) -> Result<Value, String> {
    guard(&app, &window)?;
    if OVERFLOW.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("Harness evidence buffer overflowed".into());
    }
    let registry = app.try_state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>();
    let record = registry
        .as_ref()
        .and_then(|registry| registry.record(FIXTURE_ID));
    let owner = record.as_ref().map(|record| {
        record.dev.as_ref().map_or("installed", |dev| {
            if Path::new(&dev.path)
                .file_name()
                .is_some_and(|name| name == "fixture-b")
            {
                "fixture-b"
            } else {
                "fixture"
            }
        })
    });
    Ok(json!({
        "schema": 1, "runId": config().marker.run_id, "profile": config().data,
        "applicationId": app.config().identifier, "eventsPort": crate::events_server::EVENTS_PORT,
        "shortcutBindings": context_shortcut_bindings(&app),
        "worker": crate::extension_host::harness_snapshot(FIXTURE_ID),
        "agent": crate::agent::harness_snapshot(&app),
        "tokenCount": crate::events_server::token_count(),
        "registryAvailable": registry.is_some(),
        "fixtureEnabled": record.as_ref().is_some_and(|record| record.enabled),
        "fixtureOwner": owner,
        "fixtureInstalled": registry.as_ref().is_some_and(|registry| registry.installed_record(FIXTURE_ID).is_some()),
        "fixtureApproved": record.as_ref().is_some_and(|record| record.actions_approved.is_some()),
        "fixtureVersion": record.as_ref().map(|record| &record.installed_version),
        "fixtureTrust": record.as_ref().map(|record| record.trust),
        "store": app.try_state::<std::sync::Arc<crate::grain_store::StoreState>>()
            .map(|state| state.harness_snapshot()),
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

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureOperation {
    CaptureConfirmation,
    Load,
    Register,
    LoadSecond,
    Import,
    RemoveInstalled,
    Unload,
    Enable,
    Disable,
}

#[tauri::command]
pub async fn agent_harness_fixture(
    app: AppHandle,
    window: WebviewWindow,
    operation: FixtureOperation,
) -> Result<Value, String> {
    guard(&app, &window)?;
    match operation {
        FixtureOperation::CaptureConfirmation => {
            // In-memory test replay only; never included in status or reports.
            return crate::agent::harness_pending_confirmation(&app)
                .map(|token| json!({"token": token}))
                .ok_or_else(|| "No owned pending confirmation".into());
        }
        FixtureOperation::Load | FixtureOperation::Register | FixtureOperation::LoadSecond => {
            let root = config()
                .root
                .join(if matches!(operation, FixtureOperation::LoadSecond) {
                    "fixture-b"
                } else {
                    "fixture"
                })
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if !root.starts_with(&config().root) {
                return Err("Fixture escaped harness root".into());
            }
            let loaded = crate::dev_extensions::load_project(&root)?;
            if loaded.pack.manifest.id != FIXTURE_ID
                || !loaded.pack.manifest.permissions.is_empty()
                || loaded.pack.manifest.contributes.authentication.is_some()
            {
                return Err("Harness only admits its permission-free native fixture".into());
            }
            crate::grain_commands::load_unpacked_project(&app, &root)?;
            if !matches!(operation, FixtureOperation::Load) {
                return agent_harness_status(app, window);
            }
            let reg = app.state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>();
            let record = reg.record(FIXTURE_ID).ok_or("Fixture was not registered")?;
            let digest = grain_core::extensions::approval_fingerprint(&record, &loaded.pack)
                .map_err(|e| e.to_string())?;
            crate::grain_commands::extension_grant(
                app.clone(),
                window.clone(),
                FIXTURE_ID.into(),
                vec![],
                digest,
            )?;
        }
        FixtureOperation::Import => {
            let path = config()
                .root
                .join("fixture.grainpack")
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if !path.starts_with(&config().root) {
                return Err("Fixture package escaped harness root".into());
            }
            use std::io::Read;
            let mut raw = Vec::new();
            std::fs::File::open(&path)
                .map_err(|e| e.to_string())?
                .take(grain_sdk::PACK_MAX_BYTES + 1)
                .read_to_end(&mut raw)
                .map_err(|e| e.to_string())?;
            if raw.len() as u64 > grain_sdk::PACK_MAX_BYTES {
                return Err("Harness package exceeds the storage limit".into());
            }
            let pack: grain_sdk::GrainPack =
                serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
            pack.validate()?;
            if pack.manifest.id != FIXTURE_ID
                || !pack.manifest.permissions.is_empty()
                || pack.manifest.contributes.authentication.is_some()
            {
                return Err("Harness only imports its permission-free native fixture".into());
            }
            crate::grain_commands::extension_import_pack(
                app.clone(),
                window.clone(),
                path.to_string_lossy().into_owned(),
            )?;
        }
        FixtureOperation::RemoveInstalled => {
            crate::grain_commands::extension_uninstall(
                app.clone(),
                window.clone(),
                FIXTURE_ID.into(),
                false,
            )
            .await?;
        }
        FixtureOperation::Unload => crate::grain_commands::extension_unload_dev(
            app.clone(),
            window.clone(),
            FIXTURE_ID.into(),
        )?,
        FixtureOperation::Enable | FixtureOperation::Disable => {
            crate::grain_commands::extension_set_enabled(
                app.clone(),
                window.clone(),
                FIXTURE_ID.into(),
                matches!(operation, FixtureOperation::Enable),
            )?
        }
    }
    agent_harness_status(app, window)
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Instruction {
    NativeStaged,
    McpStaged,
    McpSchemaBudget,
    NativeWorkflow,
    McpWorkflow,
    NativeDirectory,
    McpRead,
    McpExcluded,
    McpPreview,
    McpUnknown,
    McpCatalogRefusal,
    McpConformance,
    McpLiveRead,
    McpLiveLarge,
    McpAccountA,
    McpAccountB,
    McpClientA,
    McpClientB,
    Hello,
    SlowHello,
    ModelWait,
    DeadlineHello,
    LostReply,
    ToolError,
    ThrownError,
    MalformedResult,
    OversizedResult,
    OversizedRaw,
    InvalidArguments,
    MissingArguments,
    WrongArguments,
    OversizedArguments,
    MalformedArguments,
    TypedValues,
    TypedOmitted,
    TypedNull,
    TypedZero,
    TypedNumberNull,
    TypedNumberOmitted,
    TypedNumberZero,
    AccountReadA,
    AccountReadB,
    AccountReadBPeer,
    AccountReadAInstalled,
    AccountReadBInstalled,
    AccountReadADeveloperA,
    AccountReadBDeveloperA,
    AccountReadADeveloperB,
    AccountReadBDeveloperB,
}

#[tauri::command]
pub async fn agent_harness_submit(
    app: AppHandle,
    window: WebviewWindow,
    instruction: Instruction,
) -> Result<(), String> {
    guard(&app, &window)?;
    let text = match instruction {
        Instruction::NativeStaged => "Harness request: native_staged",
        Instruction::McpStaged => "Harness request: mcp_staged",
        Instruction::McpSchemaBudget => "Harness request: mcp_schema_budget",
        Instruction::NativeWorkflow => "Harness request: native_workflow",
        Instruction::McpWorkflow => "Harness request: mcp_workflow",
        Instruction::NativeDirectory => "Harness request: native_directory",
        Instruction::McpRead => "Harness request: mcp_read",
        Instruction::McpExcluded => "Harness request: mcp_excluded",
        Instruction::McpPreview => "Harness request: mcp_preview",
        Instruction::McpUnknown => "Harness request: mcp_unknown",
        Instruction::McpCatalogRefusal => "Harness request: mcp_catalog_refusal",
        Instruction::McpConformance => "Harness request: mcp_conformance",
        Instruction::McpLiveRead => "Harness request: mcp_live_read",
        Instruction::McpLiveLarge => "Harness request: mcp_live_large",
        Instruction::McpAccountA => "Harness request: mcp_account_a",
        Instruction::McpAccountB => "Harness request: mcp_account_b",
        Instruction::McpClientA => "Harness request: mcp_client_a",
        Instruction::McpClientB => "Harness request: mcp_client_b",
        Instruction::Hello => "Harness request: hello",
        Instruction::SlowHello => "Harness request: slow_hello",
        Instruction::ModelWait => "Harness request: model_wait",
        Instruction::DeadlineHello => "Harness request: deadline_hello",
        Instruction::LostReply => "Harness request: lost_reply",
        Instruction::ToolError => "Harness request: tool_error",
        Instruction::ThrownError => "Harness request: thrown_error",
        Instruction::MalformedResult => "Harness request: malformed_result",
        Instruction::OversizedResult => "Harness request: oversized_result",
        Instruction::OversizedRaw => "Harness request: oversized_raw",
        Instruction::InvalidArguments => "Harness request: invalid_arguments",
        Instruction::MissingArguments => "Harness request: missing_arguments",
        Instruction::WrongArguments => "Harness request: wrong_arguments",
        Instruction::OversizedArguments => "Harness request: oversized_arguments",
        Instruction::MalformedArguments => "Harness request: malformed_arguments",
        Instruction::TypedValues => "Harness request: typed_values",
        Instruction::TypedOmitted => "Harness request: typed_omitted",
        Instruction::TypedNull => "Harness request: typed_null",
        Instruction::TypedZero => "Harness request: typed_zero",
        Instruction::TypedNumberNull => "Harness request: typed_number_null",
        Instruction::TypedNumberOmitted => "Harness request: typed_number_omitted",
        Instruction::TypedNumberZero => "Harness request: typed_number_zero",
        Instruction::AccountReadA => "Harness request: account_read_a",
        Instruction::AccountReadB => "Harness request: account_read_b",
        Instruction::AccountReadBPeer => "Harness request: account_read_b_peer",
        Instruction::AccountReadAInstalled => "Harness request: account_read_a_installed",
        Instruction::AccountReadBInstalled => "Harness request: account_read_b_installed",
        Instruction::AccountReadADeveloperA => "Harness request: account_read_a_developer_a",
        Instruction::AccountReadBDeveloperA => "Harness request: account_read_b_developer_a",
        Instruction::AccountReadADeveloperB => "Harness request: account_read_a_developer_b",
        Instruction::AccountReadBDeveloperB => "Harness request: account_read_b_developer_b",
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
        for port in [0, 9000, 7124, crate::events_server::EVENTS_PORT] {
            std::fs::write(
                &marker,
                format!(
                    r#"{{"schema":1,"runId":"{}","modelPort":9000,"storePort":{port}}}"#,
                    uuid::Uuid::new_v4()
                ),
            )
            .unwrap();
            assert!(read_marker(root.path()).is_err());
        }
        for port in [0, 9000, 9001, 7124, crate::events_server::EVENTS_PORT] {
            std::fs::write(&marker, format!(
                r#"{{"schema":1,"runId":"{}","modelPort":9000,"storePort":9001,"authPort":{port}}}"#,
                uuid::Uuid::new_v4(),
            )).unwrap();
            assert!(read_marker(root.path()).is_err());
        }
        for port in [0, 9000, 9001, 9002, 7124, crate::events_server::EVENTS_PORT] {
            std::fs::write(&marker, format!(
                r#"{{"schema":1,"runId":"{}","modelPort":9000,"storePort":9001,"authPort":9002,"mcpPort":{port}}}"#,
                uuid::Uuid::new_v4(),
            )).unwrap();
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
}
