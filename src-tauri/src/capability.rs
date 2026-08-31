//! [GRAIN] Two-level extension discovery for the Agent tool loop
//! (`docs/Extensions 2.0/PLAN.md` Amendment D).
//!
//! The bridge between the pure directory/exposure layer in `grain-core` and the
//! Agent's live tool loop (`agent::run_with_note_tools`). It:
//!
//! - injects a compact, inert directory of enabled extensions;
//! - exposes only `load_extension` initially, then atomically adds every approved
//!   action schema for each extension loaded in a prior model round;
//! - dispatches action calls through an authoritative task-local name map and the
//!   existing prepared-call, confirmation, and worker boundary.
//!
//! Grain Space (built-in) actions keep executing through their own tools; a turn
//! on a machine with no installed action extensions exposes nothing here and is
//! byte-for-byte the old behaviour.

use std::collections::{BTreeMap, HashSet};

use crate::llm_client::{ToolCallOut, ToolSpec};
use grain_core::capability_agent::{
    self, load_extension_tool_def, ExtensionExposure, ExtensionLoadStatus, LOAD_EXTENSION,
    MAX_DIRECTORY_EXTENSIONS, TOOL_NAME_PREFIX,
};
use grain_core::execution::{RiskClass, SideEffect};
use grain_core::interaction::Interaction;
use tauri::AppHandle;

/// The result of dispatching one capability tool call: either text to feed back
/// to the model, or a risky action withheld pending the user's approval (surfaced
/// on `AgentReply.confirm_action`, not fed to the model as if it ran).
pub enum ToolResult {
    Text(String),
    Confirm(crate::agent::AgentConfirm),
}

fn to_spec(def: capability_agent::ToolDef) -> ToolSpec {
    ToolSpec {
        name: def.name,
        description: def.description,
        parameters: def.parameters,
    }
}

/// The task-local directory, exposure state, initial schema list, and ephemeral
/// context constructed at the start of one Agent request.
pub struct Opened {
    pub session: CapabilitySession,
    pub specs: Vec<ToolSpec>,
    pub directory_context: Option<String>,
}

pub struct CapabilitySession {
    exposure: ExtensionExposure,
    actions: BTreeMap<String, SessionAction>,
}

struct SessionAction {
    spec: ToolSpec,
    origin: ActionOrigin,
}

enum ActionOrigin {
    Native,
    Mcp {
        provider_id: String,
        provider_name: String,
        tool_name: String,
        title: String,
        input_schema: serde_json::Value,
    },
}

pub fn open(app: &AppHandle, reserved_names: &[String]) -> Opened {
    let mut directory = crate::extension_host::capability_extension_directory();
    directory.extend(crate::grain_mcp::directory(app));
    directory.sort_by(|a, b| a.extension_id.cmp(&b.extension_id));
    directory.dedup_by(|a, b| a.extension_id == b.extension_id);
    directory.truncate(MAX_DIRECTORY_EXTENSIONS);
    let exposure = ExtensionExposure::new(&directory, reserved_names);
    if !exposure.has_directory_entries() {
        return Opened {
            session: CapabilitySession {
                exposure,
                actions: BTreeMap::new(),
            },
            specs: Vec::new(),
            directory_context: None,
        };
    }
    Opened {
        session: CapabilitySession {
            exposure,
            actions: BTreeMap::new(),
        },
        specs: vec![to_spec(load_extension_tool_def())],
        directory_context: Some(capability_agent::extension_directory_context(&directory)),
    }
}

/// The loader plus every action schema published by successful prior loads.
pub fn specs(session: &CapabilitySession) -> Vec<ToolSpec> {
    let mut specs = Vec::with_capacity(session.actions.len() + 1);
    if session.exposure.has_directory_entries() {
        specs.push(to_spec(load_extension_tool_def()));
    }
    specs.extend(session.actions.values().map(|action| ToolSpec {
        name: action.spec.name.clone(),
        description: action.spec.description.clone(),
        parameters: action.spec.parameters.clone(),
    }));
    specs
}

/// Dispatch one tool call if it belongs to the capability surface. Returns
/// `Some(result_text)` when handled, `None` when the call is not ours (the caller
/// routes it elsewhere — e.g. the Grain Space note tools).
pub async fn dispatch(
    app: &AppHandle,
    call: &ToolCallOut,
    session: &mut CapabilitySession,
    offered_names: &HashSet<String>,
) -> Option<ToolResult> {
    if call.name == LOAD_EXTENSION {
        if !offered_names.contains(LOAD_EXTENSION) {
            return Some(ToolResult::Text(
                "load_extension was not available in this model round.".to_string(),
            ));
        }
        return Some(ToolResult::Text(handle_load(app, call, session).await));
    }
    // An action tool resolves through the session's exposure map — the
    // authoritative reverse of `tool_name`. An undisclosed or invented name
    // resolves to nothing here and falls through to the caller (which will report
    // "no such tool"): discovery is not authorisation.
    let loaded = match resolve_offered_action(&session.exposure, offered_names, &call.name) {
        Ok(Some(loaded)) => loaded,
        Ok(None) => return None,
        Err(message) => return Some(ToolResult::Text(message.to_string())),
    };
    Some(
        execute_action(
            app,
            session,
            &loaded.canonical_id,
            &loaded.manifest_digest,
            &call.arguments,
        )
        .await,
    )
}

fn resolve_offered_action(
    exposure: &ExtensionExposure,
    offered_names: &HashSet<String>,
    provider_name: &str,
) -> Result<Option<capability_agent::LoadedAction>, &'static str> {
    if provider_name.starts_with(TOOL_NAME_PREFIX) && !offered_names.contains(provider_name) {
        return Err(
            "That extension action was not offered in this model round. Load its extension, then call the action on the next round.",
        );
    }
    Ok(exposure.resolve(provider_name).cloned())
}

async fn handle_load(
    app: &AppHandle,
    call: &ToolCallOut,
    session: &mut CapabilitySession,
) -> String {
    let extension_id = match capability_agent::parse_load_extension_arguments(&call.arguments) {
        Ok(id) => id,
        Err(error) => return format!("Could not load extension: {error}."),
    };
    if let Some(provider_id) = extension_id.strip_prefix("mcp.") {
        return handle_mcp_load(app, &extension_id, provider_id, session).await;
    }
    let action_set =
        match crate::extension_host::capability_actions_for_extension(app, &extension_id) {
            Ok(action_set) => action_set,
            Err(error) => return format!("Could not load extension '{extension_id}': {error}."),
        };

    match crate::grain_auth::connection(app, &extension_id).await {
        Ok(Some(connection)) if !matches!(connection.state.as_str(), "connected" | "expired") => {
            return format!(
                "Could not load extension '{extension_id}': its {} account is {}. Connect it in Grain Settings first.",
                capability_agent::sanitize(&connection.provider_name, 96),
                capability_agent::sanitize(&connection.state, 32)
            );
        }
        Err(_) => {
            return format!(
                "Could not load extension '{extension_id}': Grain could not verify its account state."
            );
        }
        _ => {}
    }

    let status = match session.exposure.load(
        &extension_id,
        &action_set.actions,
        &action_set.manifest_digest,
    ) {
        Ok(status) => status,
        Err(error) => return format!("Could not load extension '{extension_id}': {error}."),
    };
    if status == ExtensionLoadStatus::AlreadyLoaded {
        return format!(
            "Extension '{extension_id}' is already loaded; its tools remain available."
        );
    }

    for action in &action_set.actions {
        session.actions.insert(
            action.canonical_id.clone(),
            SessionAction {
                spec: to_spec(capability_agent::action_tool_def(action)),
                origin: ActionOrigin::Native,
            },
        );
    }

    let mut lines = Vec::with_capacity(action_set.actions.len());
    for action in &action_set.actions {
        lines.push(format!(
            "- {} — {}",
            capability_agent::tool_name(&action.canonical_id),
            capability_agent::sanitize(&action.title, 160)
        ));
    }
    format!(
        "Loaded extension '{}' for this request. Its tools will be available on the next model round:\n{}",
        capability_agent::sanitize(&extension_id, 255),
        lines.join("\n")
    )
}

async fn handle_mcp_load(
    app: &AppHandle,
    extension_id: &str,
    provider_id: &str,
    session: &mut CapabilitySession,
) -> String {
    let tool_set = match crate::grain_mcp::list_tools(app, provider_id).await {
        Ok(tool_set) => tool_set,
        Err(error) => return format!("Could not load MCP provider '{extension_id}': {error}."),
    };
    let actions: Vec<_> = tool_set
        .tools
        .iter()
        .map(|tool| mcp_action_input(extension_id, &tool_set.provider_name, tool))
        .collect();
    let status = match session
        .exposure
        .load(extension_id, &actions, &tool_set.digest)
    {
        Ok(status) => status,
        Err(error) => return format!("Could not load MCP provider '{extension_id}': {error}."),
    };
    if status == ExtensionLoadStatus::AlreadyLoaded {
        return format!(
            "MCP provider '{extension_id}' is already loaded; its tools remain available."
        );
    }

    let mut lines = Vec::with_capacity(actions.len());
    for (action, tool) in actions.iter().zip(&tool_set.tools) {
        let parameters = serde_json::Value::Object(tool.input_schema.as_ref().clone());
        let spec = ToolSpec {
            name: capability_agent::tool_name(&action.canonical_id),
            description: capability_agent::sanitize(
                tool.description.as_deref().unwrap_or(&action.title),
                1_200,
            ),
            parameters: parameters.clone(),
        };
        session.actions.insert(
            action.canonical_id.clone(),
            SessionAction {
                spec,
                origin: ActionOrigin::Mcp {
                    provider_id: tool_set.provider_id.clone(),
                    provider_name: tool_set.provider_name.clone(),
                    tool_name: tool.name.to_string(),
                    title: action.title.clone(),
                    input_schema: parameters,
                },
            },
        );
        lines.push(format!(
            "- {} — {}",
            capability_agent::tool_name(&action.canonical_id),
            capability_agent::sanitize(&action.title, 160)
        ));
    }
    format!(
        "Loaded MCP provider '{}' for this request. Its tools will be available on the next model round:\n{}",
        capability_agent::sanitize(extension_id, 255),
        lines.join("\n")
    )
}

fn mcp_action_input(
    extension_id: &str,
    provider_name: &str,
    tool: &rmcp::model::Tool,
) -> grain_core::capability_index::ActionInput {
    use grain_sdk::manifest::ActionRisk;
    let tool_name = tool.name.to_string();
    let title = tool
        .title
        .as_deref()
        .or_else(|| {
            tool.annotations
                .as_ref()
                .and_then(|value| value.title.as_deref())
        })
        .unwrap_or(&tool_name);
    let description = tool.description.as_deref().unwrap_or_default();
    grain_core::capability_index::ActionInput {
        extension_id: extension_id.to_string(),
        action_id: tool_name.clone(),
        canonical_id: format!("{extension_id}:{tool_name}"),
        provider_name: provider_name.to_string(),
        title: capability_agent::sanitize(title, 160),
        aliases: vec![provider_name.to_string()],
        namespaces: Vec::new(),
        tags: Vec::new(),
        examples: Vec::new(),
        phrases: Vec::new(),
        params: Vec::new(),
        when_to_use: String::new(),
        when_not_to_use: String::new(),
        description: capability_agent::sanitize(description, 1_200),
        provider_context: Vec::new(),
        // MCP annotations are explicitly untrusted hints. The execution path
        // applies Confirm regardless of this placeholder.
        risk: ActionRisk::Confirm,
        enabled: true,
        platform_ok: true,
        quarantined: false,
    }
}

/// Prepare and route one resolved action through the host executor: a `Safe`
/// action runs in process now; a `Confirm` action is withheld and surfaced for
/// the user's approval (never fed to the model as if it ran).
async fn execute_action(
    app: &AppHandle,
    session: &CapabilitySession,
    canonical: &str,
    loaded_manifest_digest: &str,
    arguments_json: &str,
) -> ToolResult {
    let Some(session_action) = session.actions.get(canonical) else {
        return ToolResult::Text(format!("The action \"{canonical}\" is not available."));
    };
    if let ActionOrigin::Mcp {
        provider_id,
        provider_name,
        tool_name,
        title,
        input_schema,
    } = &session_action.origin
    {
        let arguments = match parse_mcp_arguments(arguments_json, input_schema) {
            Ok(arguments) => arguments,
            Err(error) => {
                return ToolResult::Text(format!("The action arguments are invalid: {error}."))
            }
        };
        let extension_id = format!("mcp.{provider_id}");
        let prepared = crate::action_exec::prepare(
            canonical,
            &extension_id,
            tool_name,
            provider_name,
            arguments,
            RiskClass::Confirm,
            SideEffect::Write,
            loaded_manifest_digest,
        );
        return match crate::action_exec::run_or_confirm(app, prepared, title).await {
            crate::action_exec::Dispatch::Ran(outcome) => ToolResult::Text(outcome.model_summary()),
            crate::action_exec::Dispatch::AwaitConfirm(interaction) => {
                ToolResult::Confirm(to_agent_confirm(interaction))
            }
        };
    }

    let Some(action) = crate::extension_host::capability_action_meta(canonical) else {
        return ToolResult::Text(format!("The action \"{canonical}\" is not available."));
    };
    let arguments = match capability_agent::parse_and_validate_arguments(&action, arguments_json) {
        Ok(arguments) => arguments,
        Err(error) => {
            return ToolResult::Text(format!("The action arguments are invalid: {error}."))
        }
    };

    // The host floor classifies the action; the axis retry/parallelism reads
    // follows it (a Confirm write, a Safe read). User policy could only tighten
    // this — never loosen it — and is applied here once wired.
    let builtin = action.extension_id == crate::action_exec::GRAIN_SPACE_EXT_ID;
    let risk = if builtin {
        RiskClass::floor(action.risk)
    } else {
        RiskClass::Confirm
    };
    let side_effect = if builtin
        && matches!(
            action.action_id.as_str(),
            "search_notes" | "get_note" | "list_collections"
        ) {
        SideEffect::Read
    } else {
        SideEffect::Write
    };
    let digest = if builtin {
        Some("builtin".to_string())
    } else {
        crate::extension_host::approved_action_digest(app, &action.extension_id, &action.action_id)
    };
    let Some(digest) = digest else {
        return ToolResult::Text(
            "The action is no longer approved or available. Ask again after reviewing the extension."
                .to_string(),
        );
    };
    if !builtin && !loaded_digest_is_current(loaded_manifest_digest, &digest) {
        return ToolResult::Text(
            "The extension changed after its tools were loaded. Start the request again so Grain can expose the current schemas."
                .to_string(),
        );
    }
    let prepared = crate::action_exec::prepare(
        canonical,
        &action.extension_id,
        &action.action_id,
        &action.provider_name,
        arguments,
        risk,
        side_effect,
        &digest,
    );

    match crate::action_exec::run_or_confirm(app, prepared, &action.title).await {
        crate::action_exec::Dispatch::Ran(outcome) => ToolResult::Text(outcome.model_summary()),
        // The confirmation is held host-side by token and surfaced on
        // `AgentReply.confirm_action` for the user to approve (§2.5). It is never
        // fed to the model as if it ran.
        crate::action_exec::Dispatch::AwaitConfirm(interaction) => {
            ToolResult::Confirm(to_agent_confirm(interaction))
        }
    }
}

fn parse_mcp_arguments(raw: &str, schema: &serde_json::Value) -> Result<serde_json::Value, String> {
    const MAX_ARGUMENT_BYTES: usize = 64 * 1024;
    if raw.len() > MAX_ARGUMENT_BYTES {
        return Err("arguments exceed the 64 KiB limit".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| "arguments are not valid JSON")?;
    let object = value.as_object().ok_or("arguments must be a JSON object")?;
    let schema_object = schema.as_object().ok_or("the provider schema is invalid")?;
    if let Some(required) = schema_object
        .get("required")
        .and_then(serde_json::Value::as_array)
    {
        for name in required.iter().filter_map(serde_json::Value::as_str) {
            if !object.contains_key(name) {
                return Err(format!("missing required argument '{name}'"));
            }
        }
    }
    if schema_object
        .get("additionalProperties")
        .and_then(serde_json::Value::as_bool)
        == Some(false)
    {
        let properties = schema_object
            .get("properties")
            .and_then(serde_json::Value::as_object);
        if let Some(name) = object
            .keys()
            .find(|name| !properties.is_some_and(|properties| properties.contains_key(*name)))
        {
            return Err(format!("unknown argument '{name}'"));
        }
    }
    Ok(value)
}

fn loaded_digest_is_current(loaded: &str, current: &str) -> bool {
    !loaded.is_empty() && loaded == current
}

/// Project a host `Interaction::Confirm` into the frontend confirmation type,
/// with the interim markdown pre-rendered (renderer #1).
fn to_agent_confirm(interaction: Interaction) -> crate::agent::AgentConfirm {
    let markdown = grain_core::interaction::to_markdown(&interaction);
    match interaction {
        Interaction::Confirm {
            token,
            title,
            summary,
            details,
            side_effect,
            destinations,
        } => crate::agent::AgentConfirm {
            token,
            title,
            summary,
            details: details
                .into_iter()
                .map(|field| crate::agent::AgentConfirmField {
                    label: field.label,
                    value: field.value,
                })
                .collect(),
            side_effect,
            destinations,
            markdown,
        },
        // Only Confirm should reach here; render anything else as a bare prompt.
        _ => crate::agent::AgentConfirm {
            token: String::new(),
            title: "Confirm".to_string(),
            summary: String::new(),
            details: Vec::new(),
            side_effect: String::new(),
            destinations: Vec::new(),
            markdown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_core::capability_index::{ActionInput, ExtensionDirectoryEntry};
    use grain_sdk::manifest::ActionRisk;

    fn action() -> ActionInput {
        ActionInput {
            extension_id: "com.example.github".to_string(),
            action_id: "create_issue".to_string(),
            canonical_id: "com.example.github:create_issue".to_string(),
            provider_name: "GitHub".to_string(),
            title: "Create issue".to_string(),
            aliases: Vec::new(),
            namespaces: Vec::new(),
            tags: Vec::new(),
            examples: Vec::new(),
            phrases: Vec::new(),
            params: Vec::new(),
            when_to_use: String::new(),
            when_not_to_use: String::new(),
            description: String::new(),
            provider_context: Vec::new(),
            risk: ActionRisk::Safe,
            enabled: true,
            platform_ok: true,
            quarantined: false,
        }
    }

    #[test]
    fn same_round_load_cannot_authorize_an_unoffered_action() {
        let entry = ExtensionDirectoryEntry {
            extension_id: "com.example.github".to_string(),
            name: "GitHub".to_string(),
            description: "Issues".to_string(),
            action_count: 1,
        };
        let action = action();
        let mut exposure = ExtensionExposure::new(&[entry], &[]);
        exposure
            .load(
                "com.example.github",
                std::slice::from_ref(&action),
                "approved-digest",
            )
            .unwrap();
        let name = capability_agent::tool_name(&action.canonical_id);

        let offered_before_load = HashSet::from([LOAD_EXTENSION.to_string()]);
        assert!(resolve_offered_action(&exposure, &offered_before_load, &name).is_err());

        let offered_next_round = HashSet::from([LOAD_EXTENSION.to_string(), name.clone()]);
        assert_eq!(
            resolve_offered_action(&exposure, &offered_next_round, &name)
                .unwrap()
                .map(|loaded| (loaded.canonical_id, loaded.manifest_digest)),
            Some((action.canonical_id, "approved-digest".to_string()))
        );
    }

    #[test]
    fn manifest_change_invalidates_a_loaded_action() {
        assert!(loaded_digest_is_current("approved-a", "approved-a"));
        assert!(!loaded_digest_is_current("approved-a", "approved-b"));
        assert!(!loaded_digest_is_current("", "approved-a"));
    }

    #[test]
    fn mcp_arguments_are_bounded_and_honor_required_and_closed_properties() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"],
            "additionalProperties": false
        });
        assert_eq!(
            parse_mcp_arguments(r#"{"query":"grain"}"#, &schema).unwrap(),
            serde_json::json!({ "query": "grain" })
        );
        assert!(parse_mcp_arguments("{}", &schema).is_err());
        assert!(parse_mcp_arguments(r#"{"query":"grain","hidden":true}"#, &schema).is_err());
        assert!(parse_mcp_arguments("[]", &schema).is_err());
    }
}
