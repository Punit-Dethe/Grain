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

use std::collections::HashSet;

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
    pub exposure: ExtensionExposure,
    pub specs: Vec<ToolSpec>,
    pub directory_context: Option<String>,
}

pub fn open(reserved_names: &[String]) -> Opened {
    let mut directory = crate::extension_host::capability_extension_directory();
    directory.truncate(MAX_DIRECTORY_EXTENSIONS);
    let exposure = ExtensionExposure::new(&directory, reserved_names);
    if !exposure.has_directory_entries() {
        return Opened {
            exposure,
            specs: Vec::new(),
            directory_context: None,
        };
    }
    Opened {
        exposure,
        specs: vec![to_spec(load_extension_tool_def())],
        directory_context: Some(capability_agent::extension_directory_context(&directory)),
    }
}

/// The loader plus every action schema published by successful prior loads.
pub fn specs(exposure: &ExtensionExposure) -> Vec<ToolSpec> {
    let ids = exposure.loaded_canonical_ids();
    let mut specs = Vec::with_capacity(ids.len() + 1);
    if exposure.has_directory_entries() {
        specs.push(to_spec(load_extension_tool_def()));
    }
    specs.extend(
        crate::extension_host::capability_tool_defs(&ids)
            .into_iter()
            .map(to_spec),
    );
    specs
}

/// Dispatch one tool call if it belongs to the capability surface. Returns
/// `Some(result_text)` when handled, `None` when the call is not ours (the caller
/// routes it elsewhere — e.g. the Grain Space note tools).
pub async fn dispatch(
    app: &AppHandle,
    call: &ToolCallOut,
    exposure: &mut ExtensionExposure,
    offered_names: &HashSet<String>,
) -> Option<ToolResult> {
    if call.name == LOAD_EXTENSION {
        if !offered_names.contains(LOAD_EXTENSION) {
            return Some(ToolResult::Text(
                "load_extension was not available in this model round.".to_string(),
            ));
        }
        return Some(ToolResult::Text(handle_load(app, call, exposure).await));
    }
    // An action tool resolves through the session's exposure map — the
    // authoritative reverse of `tool_name`. An undisclosed or invented name
    // resolves to nothing here and falls through to the caller (which will report
    // "no such tool"): discovery is not authorisation.
    let loaded = match resolve_offered_action(exposure, offered_names, &call.name) {
        Ok(Some(loaded)) => loaded,
        Ok(None) => return None,
        Err(message) => return Some(ToolResult::Text(message.to_string())),
    };
    Some(
        execute_action(
            app,
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
    exposure: &mut ExtensionExposure,
) -> String {
    let extension_id = match capability_agent::parse_load_extension_arguments(&call.arguments) {
        Ok(id) => id,
        Err(error) => return format!("Could not load extension: {error}."),
    };
    let action_set = match crate::extension_host::capability_actions_for_extension(app, &extension_id)
    {
        Ok(action_set) => action_set,
        Err(error) => return format!("Could not load extension '{extension_id}': {error}."),
    };

    match crate::grain_auth::connection(app, &extension_id).await {
        Ok(Some(connection))
            if !matches!(connection.state.as_str(), "connected" | "expired") =>
        {
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

    let status = match exposure.load(
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

/// Prepare and route one resolved action through the host executor: a `Safe`
/// action runs in process now; a `Confirm` action is withheld and surfaced for
/// the user's approval (never fed to the model as if it ran).
async fn execute_action(
    app: &AppHandle,
    canonical: &str,
    loaded_manifest_digest: &str,
    arguments_json: &str,
) -> ToolResult {
    let Some(action) = crate::extension_host::capability_action_meta(canonical) else {
        return ToolResult::Text(format!("The action \"{canonical}\" is not available."));
    };
    let arguments = match capability_agent::parse_and_validate_arguments(&action, arguments_json)
    {
        Ok(arguments) => arguments,
        Err(error) => return ToolResult::Text(format!("The action arguments are invalid: {error}.")),
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
        )
    {
        SideEffect::Read
    } else {
        SideEffect::Write
    };
    let digest = if builtin {
        Some("builtin".to_string())
    } else {
        crate::extension_host::approved_action_digest(
            app,
            &action.extension_id,
            &action.action_id,
        )
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
}
