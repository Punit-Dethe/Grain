//! [GRAIN] Recall for Grain's own Agent: four read-only meeting tools, run
//! in-process.
//!
//! The same queries and tool definitions as the Recall MCP server
//! (`grain_recall::spec`), called directly: no server, process or transport.
//! Off (the default), Recall is not in the Agent's directory. On, it adds one
//! directory line per Agent task; schemas load only when the model searches
//! for them, and each call reads the meeting files on a blocking thread and
//! frees everything when it returns. Nothing stays alive between calls.

use std::path::Path;

use grain_core::capability_index::ExtensionDirectoryEntry;
use grain_core::execution::{ActionOutcome, FailureClass, SuccessData};
use grain_recall::spec;
use tauri::AppHandle;

use crate::grain_compiled_tools::{catalog_tool, CatalogTool, ToolDef};

pub(crate) const EXTENSION_ID: &str = "grain.recall";
pub(crate) const PROVIDER_NAME: &str = "Grain Recall";
const DESCRIPTION: &str = "The user's recorded meetings: search what was said, \
list by date, read a summary or transcript. Speakers are anonymous (Speaker 1, \
Speaker 2); never guess their names. Use only meeting ids the tools return.";
const OFF: &str = "Recall is turned off for the Agent. The user can turn it on in \
Extensions > Grain Recall.";

/// The store switch, read from the in-memory settings.
pub(crate) fn is_enabled(app: &AppHandle) -> bool {
    crate::settings::get_settings(app).recall_agent_enabled
}

/// Listed only while the switch is on. The Agent prompt carries no date, and
/// "yesterday's standup" needs one, so the line does.
pub(crate) fn directory(app: &AppHandle) -> Option<ExtensionDirectoryEntry> {
    is_enabled(app).then(directory_entry)
}

fn directory_entry() -> ExtensionDirectoryEntry {
    ExtensionDirectoryEntry {
        extension_id: EXTENSION_ID.to_string(),
        name: PROVIDER_NAME.to_string(),
        description: format!(
            "{DESCRIPTION} Today is {}.",
            chrono::Local::now().format("%A %Y-%m-%d")
        ),
        action_count: spec::tools().len(),
    }
}

/// The tool set and its digest. No I/O.
pub(crate) fn catalog() -> (String, Vec<CatalogTool>) {
    let tools = spec::tools()
        .into_iter()
        .map(|tool| {
            catalog_tool(
                EXTENSION_ID,
                PROVIDER_NAME,
                DESCRIPTION,
                ToolDef {
                    name: tool.name,
                    title: tool.title,
                    description: tool.description,
                    keywords: tool.keywords,
                    read_only: true,
                    schema: tool.schema,
                },
            )
        })
        .collect();
    (catalog_digest(), tools)
}

/// Changes only with a new build (names, descriptions or schemas).
pub(crate) fn catalog_digest() -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(DESCRIPTION.as_bytes());
    for tool in spec::tools() {
        for part in [tool.name, tool.description, &tool.schema.to_string()] {
            hasher.update(part.as_bytes());
            hasher.update([0]);
        }
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Run one tool over `root` and shape the answer for the model.
fn run(root: &Path, action_id: &str, arguments: serde_json::Value) -> ActionOutcome {
    match spec::call(root, action_id, arguments) {
        None => ActionOutcome::Failed {
            class: FailureClass::NotFound,
            message: format!("Grain Recall has no tool `{action_id}`."),
        },
        Some(Err(message)) => ActionOutcome::Failed {
            class: if message.starts_with("No meeting") {
                FailureClass::NotFound
            } else {
                FailureClass::InvalidArgument
            },
            message,
        },
        Some(Ok(result)) => ActionOutcome::Succeeded(SuccessData {
            source: Some(PROVIDER_NAME.to_string()),
            title: None,
            body: Some(spec::render(action_id, &result)),
            structured_content: None,
            details: Vec::new(),
            receipt: false,
        }),
    }
}

/// Run one Agent call. Re-checks the switch, so a call prepared before the
/// user turned Recall off is refused.
pub(crate) async fn call(
    app: &AppHandle,
    action_id: &str,
    arguments: &serde_json::Value,
) -> ActionOutcome {
    if !is_enabled(app) {
        return ActionOutcome::Failed {
            class: FailureClass::Auth,
            message: OFF.to_string(),
        };
    }
    let root = match crate::portable::app_data_dir(app) {
        Ok(dir) => dir.join("meetings"),
        Err(e) => {
            return ActionOutcome::Failed {
                class: FailureClass::Internal,
                message: format!("Could not find Grain's data folder: {e}"),
            }
        }
    };
    log::info!("[GRAIN] recall: agent {action_id}");
    let action_id = action_id.to_string();
    let arguments = arguments.clone();
    tauri::async_runtime::spawn_blocking(move || run(&root, &action_id, arguments))
        .await
        .unwrap_or_else(|e| ActionOutcome::Failed {
            class: FailureClass::Internal,
            message: format!("Reading the meetings failed: {e}"),
        })
}

#[cfg(test)]
#[path = "grain_recall_agent_tests.rs"]
mod tests;
