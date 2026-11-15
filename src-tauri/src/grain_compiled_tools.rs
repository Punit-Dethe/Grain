//! [GRAIN] Agent tool providers compiled into Grain, beside MCP connections:
//! the Agent Browser and Reminders.
//!
//! One place answers, for every compiled provider, the questions the Agent
//! loop asks of a tool provider: is it on, what is its directory entry, its
//! catalogue and digest, and how does one call run. `capability.rs` and
//! `action_exec.rs` ask here instead of naming each provider. Adding a compiled
//! provider means one arm in each function below.

use grain_core::capability_index::{ActionInput, ExtensionDirectoryEntry};
use grain_core::execution::ActionOutcome;
use grain_sdk::action::ActionRisk;
use tauri::AppHandle;

use crate::grain_agent_browser as browser;
use crate::grain_reminders as reminders;

/// One catalogue row: the retrieval record plus the model-facing definition.
pub(crate) struct CatalogTool {
    pub action: ActionInput,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A compiled provider's view of one of its tools.
pub(crate) struct ToolDef<'a> {
    pub name: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub keywords: &'a [&'a str],
    pub read_only: bool,
    pub schema: serde_json::Value,
}

/// Build the catalogue row for a compiled tool. No I/O.
pub(crate) fn catalog_tool(
    extension_id: &str,
    provider_name: &str,
    provider_description: &str,
    tool: ToolDef<'_>,
) -> CatalogTool {
    CatalogTool {
        action: ActionInput {
            extension_id: extension_id.to_string(),
            action_id: tool.name.to_string(),
            canonical_id: format!("{extension_id}:{}", tool.name),
            provider_name: provider_name.to_string(),
            title: tool.title.to_string(),
            aliases: vec![provider_name.to_string(), tool.name.to_string()],
            namespaces: Vec::new(),
            tags: tool.keywords.iter().map(|k| k.to_string()).collect(),
            examples: Vec::new(),
            phrases: grain_core::capability_agent::tool_search_phrases(tool.name),
            params: Vec::new(),
            when_to_use: String::new(),
            when_not_to_use: String::new(),
            description: format!(
                "{} {}",
                tool.description,
                grain_core::capability_agent::schema_search_text(&tool.schema)
            ),
            provider_context: vec![provider_description.to_string()],
            risk: if tool.read_only {
                ActionRisk::Safe
            } else {
                ActionRisk::Confirm
            },
            enabled: true,
            platform_ok: true,
            quarantined: false,
        },
        description: tool.description.to_string(),
        parameters: tool.schema,
    }
}

/// Whether `extension_id` names a compiled provider (rather than `mcp.*`).
pub(crate) fn is_compiled(extension_id: &str) -> bool {
    extension_id == browser::EXTENSION_ID || extension_id == reminders::EXTENSION_ID
}

/// Whether a compiled provider is on. `None` for an id that is not compiled.
pub(crate) fn enabled(app: &AppHandle, extension_id: &str) -> Option<bool> {
    if extension_id == browser::EXTENSION_ID {
        Some(browser::is_enabled(app))
    } else if extension_id == reminders::EXTENSION_ID {
        // Always on: it only writes Grain's own reminder list.
        Some(true)
    } else {
        None
    }
}

/// Directory entries of the compiled providers that are on.
pub(crate) fn directory(app: &AppHandle) -> Vec<ExtensionDirectoryEntry> {
    let mut entries = Vec::with_capacity(2);
    entries.extend(browser::directory(app));
    entries.push(reminders::directory());
    entries
}

/// A compiled provider's tool set and digest. `None` for an id that is not
/// compiled. No I/O.
pub(crate) fn catalog(extension_id: &str) -> Option<(String, Vec<CatalogTool>)> {
    if extension_id == browser::EXTENSION_ID {
        Some(browser::catalog())
    } else if extension_id == reminders::EXTENSION_ID {
        Some(reminders::catalog())
    } else {
        None
    }
}

/// The digest a prepared call is revalidated against at time of use, or
/// `None` when the provider is off (or the id is not compiled). Compiled
/// digests only change with a new build.
pub(crate) fn current_digest(app: &AppHandle, extension_id: &str) -> Option<String> {
    if extension_id == browser::EXTENSION_ID {
        browser::is_enabled(app).then(grain_core::agent_browser::catalog_digest)
    } else if extension_id == reminders::EXTENSION_ID {
        Some(grain_core::reminders::catalog_digest())
    } else {
        None
    }
}

/// Run one compiled call. Never panics into the Agent loop.
pub(crate) async fn call(
    app: &AppHandle,
    extension_id: &str,
    action_id: &str,
    arguments: &serde_json::Value,
) -> ActionOutcome {
    if extension_id == browser::EXTENSION_ID {
        browser::call(app, action_id, arguments).await
    } else if extension_id == reminders::EXTENSION_ID {
        reminders::call(app, action_id, arguments).await
    } else {
        ActionOutcome::Failed {
            class: grain_core::execution::FailureClass::NotFound,
            message: "That tool provider is not part of Grain.".to_string(),
        }
    }
}
