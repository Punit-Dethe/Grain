//! [GRAIN] Two-level extension discovery for the Agent tool loop
//! (`docs/Extensions 2.0/PLAN.md` Amendment D).
//!
//! The bridge between the pure directory/exposure layer in `grain-core` and the
//! Agent's live tool loop (`agent::run_with_tools`). It:
//!
//! - injects a compact, inert directory of enabled extensions;
//! - exposes metadata search and selected schema loading, then atomically adds
//!   only the selected schemas under a task byte budget;
//! - dispatches action calls through an authoritative task-local name map and the
//!   existing prepared-call, confirmation, and worker boundary.
//!
use std::collections::{BTreeMap, HashSet};

use crate::llm_client::{ToolCallOut, ToolSpec};
use grain_core::capability_agent::{
    self, load_extension_tool_def, search_tools_tool_def, ExtensionExposure, ExtensionLoadStatus,
    LOAD_EXTENSION, SEARCH_TOOLS, TOOL_NAME_PREFIX,
};
use grain_core::execution::{RiskClass, SideEffect};
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
    },
}

pub fn open(app: &AppHandle, reserved_names: &[String]) -> Opened {
    let mut directory = crate::extension_host::capability_extension_directory();
    directory.extend(crate::grain_mcp::directory(app));
    directory.sort_by(|a, b| a.extension_id.cmp(&b.extension_id));
    directory.dedup_by(|a, b| a.extension_id == b.extension_id);
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
        specs: vec![
            to_spec(search_tools_tool_def()),
            to_spec(load_extension_tool_def()),
        ],
        directory_context: Some(capability_agent::extension_directory_context(&directory)),
    }
}

/// Host discovery affordances and the selected schemas published by prior loads.
pub fn specs(session: &CapabilitySession) -> Vec<ToolSpec> {
    let mut specs = Vec::with_capacity(session.actions.len() + 2);
    if session.exposure.has_directory_entries() {
        specs.push(to_spec(search_tools_tool_def()));
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
/// reports an unavailable tool).
pub async fn dispatch(
    app: &AppHandle,
    call: &ToolCallOut,
    session: &mut CapabilitySession,
    offered_names: &HashSet<String>,
) -> Option<ToolResult> {
    if call.name == LOAD_EXTENSION || call.name == SEARCH_TOOLS {
        if !offered_names.contains(&call.name) {
            return Some(ToolResult::Text(
                "load_extension was not available in this model round.".to_string(),
            ));
        }
        return Some(ToolResult::Text(handle_catalog(app, call, session).await));
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

async fn handle_catalog(
    app: &AppHandle,
    call: &ToolCallOut,
    session: &mut CapabilitySession,
) -> String {
    let (extension_id, selection, query, offset) = if call.name == SEARCH_TOOLS {
        match capability_agent::parse_search_request(&call.arguments) {
            Ok(request) => (request.extension_id, None, request.query, request.offset),
            Err(error) => return format!("Could not search tools: {error}"),
        }
    } else {
        match capability_agent::parse_load_request(&call.arguments) {
            Ok(request) => (request.extension_id, request.tool_ids, String::new(), 0),
            Err(error) => return format!("Could not load tools: {error}"),
        }
    };
    if !session.exposure.contains_extension(&extension_id) {
        return "That extension is not present in this request's enabled directory.".into();
    }
    let (actions, digest, mcp) = if let Some(provider_id) = extension_id.strip_prefix("mcp.") {
        let catalog = match crate::grain_mcp::list_tools(app, provider_id).await {
            Ok(catalog) => catalog,
            Err(error) => return format!("Could not discover MCP tools: {error}"),
        };
        let actions = catalog
            .tools
            .iter()
            .map(|tool| mcp_action_input(&extension_id, &catalog.provider_name, tool))
            .collect::<Vec<_>>();
        (actions, catalog.digest.clone(), Some(catalog))
    } else {
        let catalog =
            match crate::extension_host::capability_actions_for_extension(app, &extension_id) {
                Ok(catalog) => catalog,
                Err(error) => return format!("Could not discover native tools: {error}"),
            };
        match crate::grain_auth::connection(app, &extension_id).await {
            Ok(Some(connection))
                if !matches!(connection.state.as_str(), "connected" | "expired") =>
            {
                return format!(
                    "The native account is {}. Connect it in Grain Settings first.",
                    capability_agent::sanitize(&connection.state, 32)
                );
            }
            Err(_) => return "Could not verify the native account state.".into(),
            _ => {}
        }
        (catalog.actions, catalog.manifest_digest, None)
    };
    let Some(ids) = selection else {
        let mut page = capability_agent::search_metadata(&actions, &query, offset);
        page["extension_id"] = serde_json::json!(extension_id);
        return page.to_string();
    };
    let mut candidates = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(action) = actions.iter().find(|action| action.action_id == id) else {
            return "A selected tool is absent from the current catalog. Search again; no schemas were loaded.".into();
        };
        let candidate = if let Some(catalog) = &mcp {
            let Some(tool) = catalog.tools.iter().find(|tool| tool.name.as_ref() == id) else {
                return "Tool catalog changed.".into();
            };
            SessionAction {
                spec: ToolSpec {
                    name: capability_agent::tool_name(&action.canonical_id),
                    description: capability_agent::sanitize(
                        tool.description.as_deref().unwrap_or(&action.title),
                        1200,
                    ),
                    parameters: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
                },
                origin: ActionOrigin::Mcp {
                    provider_id: catalog.provider_id.clone(),
                    provider_name: catalog.provider_name.clone(),
                    tool_name: tool.name.to_string(),
                    title: action.title.clone(),
                },
            }
        } else {
            SessionAction {
                spec: to_spec(capability_agent::action_tool_def(action)),
                origin: ActionOrigin::Native,
            }
        };
        candidates.push((action.clone(), candidate));
    }
    match publish_selected(session, &extension_id, &digest, candidates) {
        Ok(status) => format!("Selected schemas {status:?}. Tools become callable on the next model round. Search can find additional tools; nothing was executed."),
        Err(error) => format!("Could not load selected schemas: {error}"),
    }
}

fn schema_bytes(spec: &ToolSpec) -> usize {
    #[derive(serde::Serialize)]
    struct Definition<'a> {
        name: &'a str,
        description: &'a str,
        parameters: &'a serde_json::Value,
    }
    serde_json::to_vec(&Definition {
        name: &spec.name,
        description: &spec.description,
        parameters: &spec.parameters,
    })
    .map(|bytes| bytes.len())
    .unwrap_or(usize::MAX)
}

fn publish_selected(
    session: &mut CapabilitySession,
    id: &str,
    digest: &str,
    candidates: Vec<(grain_core::capability_index::ActionInput, SessionAction)>,
) -> Result<ExtensionLoadStatus, String> {
    let mut bytes = schema_bytes(&to_spec(load_extension_tool_def()))
        .saturating_add(schema_bytes(&to_spec(search_tools_tool_def())));
    for (canonical, current) in &session.actions {
        if !candidates
            .iter()
            .any(|(action, _)| &action.canonical_id == canonical)
        {
            bytes = bytes.saturating_add(schema_bytes(&current.spec));
        }
    }
    for (_, candidate) in &candidates {
        bytes = bytes.saturating_add(schema_bytes(&candidate.spec));
    }
    if bytes > capability_agent::MAX_SCHEMA_BYTES {
        return Err("selected schemas exceed the 32 KiB task budget; select fewer tools".into());
    }
    let actions: Vec<_> = candidates
        .iter()
        .map(|(action, _)| action.clone())
        .collect();
    let status = session.exposure.load(id, &actions, digest)?;
    for (action, candidate) in candidates {
        session.actions.insert(action.canonical_id, candidate);
    }
    Ok(status)
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

/// Prepare one resolved action for exact host approval. Both adapters use the
/// conservative Confirm floor; declarations/annotations cannot authorize reads.
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
    } = &session_action.origin
    {
        let arguments = match parse_mcp_arguments(arguments_json, &session_action.spec.parameters) {
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
                ToolResult::Confirm(crate::action_exec::to_agent_confirm(interaction))
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

    // Native declarations cannot grant automatic execution. A host-reviewed
    // read policy remains separate work; the current floor treats all as writes.
    let risk = RiskClass::Confirm;
    let side_effect = SideEffect::Write;
    let digest =
        crate::extension_host::approved_action_digest(app, &action.extension_id, &action.action_id);
    let Some(digest) = digest else {
        return ToolResult::Text(
            "The action is no longer approved or available. Ask again after reviewing the extension."
                .to_string(),
        );
    };
    if !loaded_digest_is_current(loaded_manifest_digest, &digest) {
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
            ToolResult::Confirm(crate::action_exec::to_agent_confirm(interaction))
        }
    }
}

fn parse_mcp_arguments(raw: &str, schema: &serde_json::Value) -> Result<serde_json::Value, String> {
    grain_core::tool_schema::parse_arguments(raw, schema)
}

fn loaded_digest_is_current(loaded: &str, current: &str) -> bool {
    !loaded.is_empty() && loaded == current
}

/// Backend-only catalog fixture: uses the production publication boundary and
/// owns no Tauri windows, transports, accounts or runtimes.
#[cfg(test)]
pub(crate) fn fixture_session() -> CapabilitySession {
    let mut session = tests::session();
    publish_selected(
        &mut session,
        "com.example.github",
        "digest",
        vec![tests::candidate("read", 0), tests::candidate("write", 0)],
    )
    .unwrap();
    session
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

    pub(super) fn session() -> CapabilitySession {
        let entry = ExtensionDirectoryEntry {
            extension_id: "com.example.github".into(),
            name: "GitHub".into(),
            description: "Issues".into(),
            action_count: 200,
        };
        CapabilitySession {
            exposure: ExtensionExposure::new(&[entry], &[]),
            actions: BTreeMap::new(),
        }
    }

    pub(super) fn candidate(id: &str, padding: usize) -> (ActionInput, SessionAction) {
        let mut input = action();
        input.action_id = id.into();
        input.canonical_id = format!("{}:{id}", input.extension_id);
        let mut spec = to_spec(capability_agent::action_tool_def(&input));
        spec.parameters["description"] = serde_json::json!("x".repeat(padding));
        (
            input,
            SessionAction {
                spec,
                origin: ActionOrigin::Native,
            },
        )
    }

    #[test]
    fn selected_publication_is_incremental_and_budget_failure_is_atomic() {
        let mut session = session();
        let id = "com.example.github";
        publish_selected(&mut session, id, "digest", vec![candidate("read", 0)]).unwrap();
        assert_eq!(specs(&session).len(), 3);
        assert!(publish_selected(
            &mut session,
            id,
            "digest",
            vec![
                candidate("write", 0),
                candidate("large", capability_agent::MAX_SCHEMA_BYTES)
            ]
        )
        .is_err());
        assert_eq!(session.actions.len(), 1);
        assert_eq!(session.exposure.loaded_canonical_ids().len(), 1);
        assert!(
            publish_selected(&mut session, id, "changed", vec![candidate("write", 0)]).is_err()
        );
        assert_eq!(session.actions.len(), 1);
        publish_selected(&mut session, id, "digest", vec![candidate("write", 0)]).unwrap();
        assert_eq!(specs(&session).len(), 4);
        assert_eq!(
            publish_selected(&mut session, id, "digest", vec![candidate("write", 0)]).unwrap(),
            ExtensionLoadStatus::AlreadyLoaded
        );
        assert!(
            specs(&session).iter().map(schema_bytes).sum::<usize>()
                <= capability_agent::MAX_SCHEMA_BYTES
        );
    }

    #[test]
    fn selected_mcp_schema_keeps_nested_constraints_without_metadata_schema_leaks() {
        let mut session = session();
        let (input, mut selected) = candidate("nested", 0);
        let schema = serde_json::json!({"type":"object","properties":{"data":{"type":"object","properties":{"mode":{"enum":["read"]}},"required":["mode"],"additionalProperties":false}},"required":["data"],"additionalProperties":false});
        selected.spec.parameters = schema.clone();
        selected.origin = ActionOrigin::Mcp {
            provider_id: "fixture".into(),
            provider_name: "Fixture".into(),
            tool_name: "nested".into(),
            title: "Nested".into(),
        };
        let metadata = capability_agent::search_metadata(std::slice::from_ref(&input), "", 0);
        assert!(!metadata.to_string().contains("properties"));
        publish_selected(
            &mut session,
            "com.example.github",
            "digest",
            vec![(input, selected)],
        )
        .unwrap();
        assert_eq!(specs(&session)[2].parameters, schema);
        assert!(parse_mcp_arguments(r#"{"data":{"mode":"read"}}"#, &schema).is_ok());
        assert!(parse_mcp_arguments(r#"{"data":{"mode":"write"}}"#, &schema).is_err());
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
