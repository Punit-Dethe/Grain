//! [GRAIN] Lexical first-round tools and combined discovery recovery
//! (`docs/Extensions 2.0/PLAN.md` Amendment D).
//!
//! The bridge between the pure directory/exposure layer in `grain-core` and the
//! Agent's live tool loop (`agent::run_with_tools`). It:
//!
//! - injects a compact, inert directory of enabled extensions;
//! - prepares task-local native/MCP catalogues and a lexical first-round shortlist;
//! - combines recovery search and schema publication under the task byte budget;
//! - dispatches action calls through an authoritative task-local name map and the
//!   existing prepared-call and worker boundary.
//!
use futures_util::{stream, StreamExt};
use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

use crate::llm_client::{ToolCallOut, ToolSpec};
#[cfg(test)]
use grain_core::capability_agent::LOAD_EXTENSION;
use grain_core::capability_agent::{
    self, search_tools_tool_def, ExtensionExposure, ExtensionLoadStatus, SEARCH_TOOLS,
    TOOL_NAME_PREFIX, TOOL_PAGE_SIZE,
};
use grain_core::capability_index::{
    ActionInput, CapabilityIndex, QuerySource, RetrievalContext, RetrievalParams,
};
use grain_core::execution::{RiskClass, SideEffect};
use tauri::AppHandle;

/// Discovery feedback, a typed execution outcome, or a legacy held confirmation.
/// Current Agent execution returns Completed without a per-call dialog.
pub enum ToolResult {
    Text(String),
    Completed(grain_core::execution::ActionOutcome),
    #[allow(dead_code)]
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
    directory_ids: Vec<String>,
    catalogs: BTreeMap<String, Catalog>,
    index: Option<CapabilityIndex>,
}

#[derive(Clone)]
struct SessionAction {
    spec: ToolSpec,
    origin: ActionOrigin,
}

#[derive(Clone)]
enum ActionOrigin {
    Native,
    Mcp {
        provider_id: String,
        provider_name: String,
        tool_name: String,
    },
}

struct Catalog {
    digest: String,
    entries: Vec<(ActionInput, SessionAction)>,
}

const SHORTLIST_SIZE: usize = 6;
const EAGER_TOOL_COUNT: usize = 8;
const PREFETCH_BUDGET: Duration = Duration::from_secs(3);
const MAX_TASK_CATALOG_BYTES: usize = 8 * 1024 * 1024;

fn empty_session(
    directory: &[grain_core::capability_index::ExtensionDirectoryEntry],
    reserved_names: &[String],
) -> CapabilitySession {
    CapabilitySession {
        exposure: ExtensionExposure::new(directory, reserved_names),
        actions: BTreeMap::new(),
        directory_ids: directory
            .iter()
            .take(capability_agent::MAX_DIRECTORY_EXTENSIONS)
            .map(|entry| entry.extension_id.clone())
            .collect(),
        catalogs: BTreeMap::new(),
        index: None,
    }
}

pub fn open(app: &AppHandle, reserved_names: &[String]) -> Opened {
    let mut directory = crate::extension_host::capability_extension_directory();
    directory.extend(crate::grain_mcp::directory(app));
    directory.sort_by(|a, b| a.extension_id.cmp(&b.extension_id));
    directory.dedup_by(|a, b| a.extension_id == b.extension_id);
    let session = empty_session(&directory, reserved_names);
    if !session.exposure.has_directory_entries() {
        return Opened {
            session,
            specs: Vec::new(),
            directory_context: None,
        };
    }
    Opened {
        session,
        specs: vec![to_spec(search_tools_tool_def())],
        directory_context: Some(capability_agent::extension_directory_context(&directory)),
    }
}

/// Host discovery affordances and the selected schemas published by prior loads.
pub fn specs(session: &CapabilitySession) -> Vec<ToolSpec> {
    let mut specs = Vec::with_capacity(session.actions.len() + 1);
    if session.exposure.has_directory_entries() {
        specs.push(to_spec(search_tools_tool_def()));
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
    if call.name == SEARCH_TOOLS {
        if !offered_names.contains(&call.name) {
            return Some(ToolResult::Text(
                "That discovery tool was not available in this model round.".to_string(),
            ));
        }
        let result = handle_catalog(app, call, session).await;
        // Public tool identities only: never arguments, results or account credentials.
        log::info!(
            "[GRAIN] agent-tool-trace {}",
            serde_json::json!({
                "phase": "loaded",
                "tools": session.actions.iter().map(|(id, action)|
                    (action.spec.name.clone(), id.clone())).collect::<BTreeMap<_, _>>()
            })
        );
        return Some(ToolResult::Text(result));
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
            "That extension action was not offered in this model round. Search for it, then call the action on the next round.",
        );
    }
    Ok(exposure.resolve(provider_name).cloned())
}

async fn fetch_catalog(
    app: &AppHandle,
    extension_id: &str,
    deadline: Option<tokio::time::Instant>,
) -> Result<Catalog, String> {
    if deadline.is_some_and(|deadline| tokio::time::Instant::now() >= deadline) {
        return Err("Catalogue preparation deadline elapsed; use recovery search.".into());
    }
    if let Some(provider_id) = extension_id.strip_prefix("mcp.") {
        let catalog = match deadline {
            Some(deadline) => {
                crate::grain_mcp::list_tools_until(app, provider_id, deadline).await?
            }
            None => crate::grain_mcp::list_tools(app, provider_id).await?,
        };
        let entries = catalog
            .tools
            .iter()
            .map(|tool| {
                let action = mcp_action_input(extension_id, &catalog.provider_name, tool);
                let selected = SessionAction {
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
                    },
                };
                (action, selected)
            })
            .collect();
        return Ok(Catalog {
            digest: catalog.digest,
            entries,
        });
    }
    let catalog = crate::extension_host::capability_actions_for_extension(app, extension_id)?;
    let account = match deadline {
        Some(deadline) => {
            tokio::time::timeout_at(deadline, crate::grain_auth::connection(app, extension_id))
                .await
                .map_err(|_| "Native account preparation timed out.".to_string())?
        }
        None => crate::grain_auth::connection(app, extension_id).await,
    };
    match account {
        Ok(Some(connection)) if !matches!(connection.state.as_str(), "connected" | "expired") => {
            return Err("Connect the native account in Grain Settings first.".into());
        }
        Err(_) => return Err("Could not verify the native account state.".into()),
        _ => {}
    };
    Ok(Catalog {
        digest: catalog.manifest_digest,
        entries: catalog
            .actions
            .into_iter()
            .map(|action| {
                let selected = SessionAction {
                    spec: to_spec(capability_agent::action_tool_def(&action)),
                    origin: ActionOrigin::Native,
                };
                (action, selected)
            })
            .collect(),
    })
}

fn remember_catalog(
    session: &mut CapabilitySession,
    id: String,
    catalog: Catalog,
) -> Result<(), String> {
    let bytes = session
        .catalogs
        .values()
        .chain(std::iter::once(&catalog))
        .flat_map(|catalog| &catalog.entries)
        .map(|(_, action)| schema_bytes(&action.spec))
        .fold(0usize, usize::saturating_add);
    if bytes > MAX_TASK_CATALOG_BYTES {
        return Err("This request's catalogue definition budget is full.".into());
    }
    session.catalogs.insert(id, catalog);
    session.index = None;
    Ok(())
}

/// No idle cache or retained MCP client. Independent providers are prepared in
/// parallel, sharing a short deadline; a miss remains available through recovery.
pub async fn prepare(
    app: &AppHandle,
    session: &mut CapabilitySession,
    queries: &[String],
    trace_id: &str,
) {
    let started = Instant::now();
    let deadline = tokio::time::Instant::now() + PREFETCH_BUDGET;
    let ids = session.directory_ids.clone();
    let mut pending = stream::iter(ids.into_iter().map(|id| async move {
        let result = fetch_catalog(app, &id, Some(deadline)).await;
        (id, result)
    }))
    .buffered(4);
    let mut unavailable = 0;
    while let Some((id, result)) = pending.next().await {
        match result.and_then(|catalog| remember_catalog(session, id, catalog)) {
            Ok(()) => {}
            Err(_) => unavailable += 1,
        }
    }
    let catalog_ms = started.elapsed().as_millis();
    let eager = session
        .catalogs
        .values()
        .map(|catalog| catalog.entries.len())
        .sum::<usize>()
        <= EAGER_TOOL_COUNT
        && session
            .catalogs
            .values()
            .flat_map(|catalog| &catalog.entries)
            .map(|(_, action)| schema_bytes(&action.spec))
            .fold(
                schema_bytes(&to_spec(search_tools_tool_def())),
                usize::saturating_add,
            )
            <= capability_agent::MAX_SCHEMA_BYTES;
    let ids = ranked_ids(session, queries, None, eager);
    let report = publish_ranked(
        session,
        ids,
        if eager {
            EAGER_TOOL_COUNT
        } else {
            SHORTLIST_SIZE
        },
    );
    log::info!(
        "[GRAIN] agent-tool-trace {}",
        serde_json::json!({
            "phase": "prepared", "task": trace_id, "catalog_ms": catalog_ms,
            "elapsed_ms": started.elapsed().as_millis(), "catalogs": session.catalogs.len(),
            "unavailable_catalogs": unavailable, "eager": eager, "selection": report,
            "offered_count": specs(session).len(),
            "schema_bytes": specs(session).iter().map(schema_bytes).sum::<usize>()
        })
    );
}

fn ranked_ids(
    session: &mut CapabilitySession,
    queries: &[String],
    scope: Option<&str>,
    eager: bool,
) -> Vec<String> {
    if eager || queries.is_empty() {
        return session
            .catalogs
            .iter()
            .filter(|(id, _)| scope.is_none_or(|scope| scope == id.as_str()))
            .flat_map(|(_, catalog)| &catalog.entries)
            .filter(|(action, _)| !session.actions.contains_key(&action.canonical_id))
            .map(|(action, _)| action.canonical_id.clone())
            .collect();
    }
    if session.index.is_none() {
        session.index = Some(CapabilityIndex::build(
            session
                .catalogs
                .values()
                .flat_map(|catalog| &catalog.entries)
                .map(|(action, _)| action.clone())
                .collect(),
        ));
    }
    let excluded: HashSet<_> = session
        .catalogs
        .iter()
        .filter(|(id, _)| scope.is_some_and(|scope| scope != id.as_str()))
        .flat_map(|(_, catalog)| &catalog.entries)
        .map(|(action, _)| action.canonical_id.clone())
        .chain(session.actions.keys().cloned())
        .collect();
    let auth_missing = HashSet::new();
    let context = RetrievalContext {
        dense: None,
        context_ineligible: &excluded,
        auth_missing: &auth_missing,
    };
    let rankings: Vec<_> = queries
        .iter()
        .map(|query| {
            session
                .index
                .as_ref()
                .unwrap()
                .retrieve(
                    query,
                    &context,
                    RetrievalParams {
                        source: QuerySource::Transcript,
                        k: capability_agent::MAX_LOADED_ACTIONS,
                    },
                )
                .entries
        })
        .collect();
    // Query batches preserve several intents instead of allowing one operation
    // to occupy every shortlist slot. IDs, not scores, are merged.
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for rank in 0..capability_agent::MAX_LOADED_ACTIONS {
        for ranking in &rankings {
            if let Some(entry) = ranking.get(rank) {
                if seen.insert(entry.canonical_id.clone()) {
                    ids.push(entry.canonical_id.clone());
                }
            }
        }
    }
    ids
}

fn publish_ranked(
    session: &mut CapabilitySession,
    ids: Vec<String>,
    limit: usize,
) -> serde_json::Value {
    let mut loaded = Vec::new();
    let mut skipped = Vec::new();
    for id in ids {
        if loaded.len() == limit {
            break;
        }
        let selected = session.catalogs.iter().find_map(|(extension_id, catalog)| {
            catalog
                .entries
                .iter()
                .find(|(action, _)| action.canonical_id == id)
                .map(|candidate| {
                    (
                        extension_id.clone(),
                        catalog.digest.clone(),
                        candidate.clone(),
                    )
                })
        });
        let Some((extension_id, digest, candidate)) = selected else {
            continue;
        };
        match publish_selected(session, &extension_id, &digest, vec![candidate]) {
            Ok(_) => loaded.push(id),
            Err(error) => skipped.push(serde_json::json!({"tool_id": id, "reason": capability_agent::sanitize(&error, 160)})),
        }
    }
    serde_json::json!({
        "loaded": loaded, "skipped": skipped,
        "instruction": "Loaded actions are callable on the next model round. Use already-offered tools directly. Nothing was executed.",
        "coverage": "Lexical candidates under the task schema budget; a miss does not prove absence."
    })
}

async fn handle_catalog(
    app: &AppHandle,
    call: &ToolCallOut,
    session: &mut CapabilitySession,
) -> String {
    let request = match capability_agent::parse_search_request(&call.arguments) {
        Ok(request) => request,
        Err(error) => return format!("Could not search tools: {error}"),
    };
    if request
        .extension_id
        .as_ref()
        .is_some_and(|id| !session.exposure.contains_extension(id))
    {
        return "That extension is not present in this request's enabled directory.".into();
    }
    let missing: Vec<_> = session
        .directory_ids
        .iter()
        .filter(|id| {
            request
                .extension_id
                .as_ref()
                .is_none_or(|scope| scope == *id)
                && !session.catalogs.contains_key(*id)
        })
        .cloned()
        .collect();
    let mut errors = Vec::new();
    let mut pending = stream::iter(missing.into_iter().map(|id| async move {
        let result = fetch_catalog(app, &id, None).await;
        (id, result)
    }))
    .buffered(4);
    while let Some((id, result)) = pending.next().await {
        if let Err(error) =
            result.and_then(|catalog| remember_catalog(session, id.clone(), catalog))
        {
            errors.push(serde_json::json!({"extension_id": id, "reason": capability_agent::sanitize(&error, 240)}));
        }
    }
    let ids = ranked_ids(
        session,
        &request.queries,
        request.extension_id.as_deref(),
        false,
    );
    let mut report = publish_ranked(session, ids, TOOL_PAGE_SIZE);
    report["unavailable"] = serde_json::json!(errors);
    report.to_string()
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
    let mut bytes = schema_bytes(&to_spec(search_tools_tool_def()));
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
        description: format!(
            "{} {}",
            capability_agent::sanitize(description, 1_200),
            capability_agent::schema_search_text(&serde_json::Value::Object(
                tool.input_schema.as_ref().clone()
            ))
        ),
        provider_context: Vec::new(),
        // MCP annotations are untrusted hints. Keep conservative classification
        // independently of the host's current automatic-approval policy.
        risk: ActionRisk::Confirm,
        enabled: true,
        platform_ok: true,
        quarantined: false,
    }
}

/// Prepare and automatically execute one resolved action under the current
/// Agent policy. Conservative classification and time-of-use checks remain.
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
        return ToolResult::Completed(
            crate::action_exec::run_agent_call(Some(app), prepared).await,
        );
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

    // Automatic approval is a host policy, not a claim that these are reads.
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

    ToolResult::Completed(crate::action_exec::run_agent_call(Some(app), prepared).await)
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
        empty_session(&[entry], &[])
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
    fn first_frame_exposes_actions_without_a_loader_and_packing_skips_large_tools() {
        let mut session = session();
        remember_catalog(
            &mut session,
            "com.example.github".into(),
            Catalog {
                digest: "digest".into(),
                entries: vec![
                    candidate("large", capability_agent::MAX_SCHEMA_BYTES),
                    candidate("read", 0),
                    candidate("write", 0),
                ],
            },
        )
        .unwrap();
        let ids = ranked_ids(&mut session, &[], None, true);
        let report = publish_ranked(&mut session, ids, EAGER_TOOL_COUNT);
        assert_eq!(report["loaded"].as_array().unwrap().len(), 2);
        assert_eq!(report["skipped"].as_array().unwrap().len(), 1);
        let definitions = specs(&session);
        assert_eq!(definitions.len(), 3);
        assert!(!definitions.iter().any(|spec| spec.name == LOAD_EXTENSION));
        let offered = definitions.iter().map(|spec| spec.name.clone()).collect();
        let name = capability_agent::tool_name("com.example.github:read");
        assert!(resolve_offered_action(&session.exposure, &offered, &name)
            .unwrap()
            .is_some());
        assert!(
            definitions.iter().map(schema_bytes).sum::<usize>()
                <= capability_agent::MAX_SCHEMA_BYTES
        );
    }

    #[test]
    fn query_batch_keeps_all_required_steps_and_cross_provider_identities() {
        use grain_core::capability_eval::required_set_coverage;
        let entries = [
            ExtensionDirectoryEntry {
                extension_id: "com.example.github".into(),
                name: "GitHub".into(),
                description: "Issues".into(),
                action_count: 2,
            },
            ExtensionDirectoryEntry {
                extension_id: "mcp.notion".into(),
                name: "Notion".into(),
                description: "Pages".into(),
                action_count: 2,
            },
        ];
        let mut session = empty_session(&entries, &[]);
        for (extension, provider, actions) in [
            (
                "com.example.github",
                "GitHub",
                [
                    ("find_issue", "Find issue"),
                    ("update_issue", "Update issue"),
                ],
            ),
            (
                "mcp.notion",
                "Notion",
                [("find_page", "Find page"), ("update_page", "Update page")],
            ),
        ] {
            let candidates = actions
                .into_iter()
                .map(|(id, title)| {
                    let (mut input, mut selected) = candidate(id, 0);
                    input.extension_id = extension.into();
                    input.canonical_id = format!("{extension}:{id}");
                    input.provider_name = provider.into();
                    input.title = title.into();
                    selected.spec.name = capability_agent::tool_name(&input.canonical_id);
                    (input, selected)
                })
                .collect();
            remember_catalog(
                &mut session,
                extension.into(),
                Catalog {
                    digest: extension.into(),
                    entries: candidates,
                },
            )
            .unwrap();
        }
        let ids = ranked_ids(
            &mut session,
            &["find Notion page".into(), "update GitHub issue".into()],
            None,
            false,
        );
        let report = publish_ranked(&mut session, ids, 2);
        let offered: Vec<String> = serde_json::from_value(report["loaded"].clone()).unwrap();
        assert!(
            required_set_coverage(
                &offered,
                &[
                    vec!["mcp.notion:find_page".into()],
                    vec!["com.example.github:update_issue".into()]
                ]
            )
            .unwrap()
            .complete
        );
        let ids = ranked_ids(
            &mut session,
            &["update page".into()],
            Some("mcp.notion"),
            false,
        );
        assert_eq!(ids, ["mcp.notion:update_page"]);
        publish_ranked(&mut session, ids, 1);
        assert_eq!(session.actions.len(), 3);
        assert_eq!(session.exposure.loaded_canonical_ids().len(), 3);
        assert_eq!(session.catalogs.len(), 2); // No rediscovery to publish recovery.
    }

    #[test]
    fn explicit_service_does_not_sort_a_large_catalogue_alphabetically() {
        let mut session = session();
        let mut entries: Vec<_> = (0..99)
            .map(|n| {
                let (mut action, selected) = candidate(&format!("noise_{n}"), 0);
                action.title = "Close repository archive".into();
                (action, selected)
            })
            .collect();
        entries.push(candidate("create_issue", 0));
        remember_catalog(
            &mut session,
            "com.example.github".into(),
            Catalog {
                digest: "digest".into(),
                entries,
            },
        )
        .unwrap();
        let ids = ranked_ids(&mut session, &["Create a GitHub issue".into()], None, false);
        assert_eq!(
            ids.first().map(String::as_str),
            Some("com.example.github:create_issue")
        );
    }

    #[test]
    fn selected_publication_is_incremental_and_budget_failure_is_atomic() {
        let mut session = session();
        let id = "com.example.github";
        publish_selected(&mut session, id, "digest", vec![candidate("read", 0)]).unwrap();
        assert_eq!(specs(&session).len(), 2);
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
        assert_eq!(specs(&session).len(), 3);
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
        assert_eq!(specs(&session)[1].parameters, schema);
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
