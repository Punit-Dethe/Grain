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
        || uuid::Uuid::parse_str(&marker.run_id).is_err()
        || marker.model_port == 0
        || marker.model_port == crate::events_server::EVENTS_PORT
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
    settings.mcp_enabled_providers.clear();
    settings.mcp_oauth_client_ids.clear();
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

fn guard(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
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
    if extension != FIXTURE_ID {
        return;
    }
    let mut events = EVENTS.lock().unwrap();
    if events.len() >= MAX_EVENTS {
        OVERFLOW.store(true, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    events.push(json!({"phase": phase, "action": action, "worker": worker_identity(token), "elapsedMs": config().started.elapsed().as_millis() as u64}));
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
    if extension != FIXTURE_ID {
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
    events.push(json!({"phase": "outcome", "action": action, "outcome": status, "elapsedMs": config().started.elapsed().as_millis() as u64}));
}

#[tauri::command]
pub fn agent_harness_status(app: AppHandle, window: WebviewWindow) -> Result<Value, String> {
    guard(&app, &window)?;
    if OVERFLOW.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("Harness evidence buffer overflowed".into());
    }
    Ok(json!({
        "schema": 1, "runId": config().marker.run_id, "profile": config().data,
        "applicationId": app.config().identifier, "eventsPort": crate::events_server::EVENTS_PORT,
        "shortcutBindings": context_shortcut_bindings(&app),
        "worker": crate::extension_host::harness_snapshot(FIXTURE_ID),
        "agent": crate::agent::harness_snapshot(&app),
        "tokenCount": crate::events_server::token_count(),
        "fixtureEnabled": app.state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>().record(FIXTURE_ID).is_some_and(|record| record.enabled),
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
    Load,
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
        FixtureOperation::Load => {
            let root = config()
                .root
                .join("fixture")
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
}

#[tauri::command]
pub async fn agent_harness_submit(
    app: AppHandle,
    window: WebviewWindow,
    instruction: Instruction,
) -> Result<(), String> {
    guard(&app, &window)?;
    let text = match instruction {
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
    };
    crate::agent::harness_submit_instruction(&app, text.into());
    Ok(())
}

#[tauri::command]
pub fn agent_harness_shutdown(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    guard(&app, &window)?;
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
    }
}
