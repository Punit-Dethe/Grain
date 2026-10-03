//! Host-only custom MCP metadata management. Saving a definition never connects,
//! enables a provider, starts a worker or reads/writes credentials. Runtime
//! acquisition and retired-account disposal must be integrated before activation.

use std::sync::{Arc, Mutex};

use grain_core::mcp_connections::{
    ConnectionDefinition, ConnectionLease, ConnectionRecord, ConnectionRegistry, RegistryError,
};
use grain_sdk::mcp::McpAuthentication;
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};

#[derive(Default)]
pub struct State {
    registry: Mutex<Option<Arc<ConnectionRegistry>>>,
    operation: Mutex<()>,
}

impl State {
    fn registry(&self, app: &AppHandle) -> Result<Arc<ConnectionRegistry>, String> {
        let mut slot = self
            .registry
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        if let Some(registry) = &*slot {
            return Ok(registry.clone());
        }
        let context = app
            .try_state::<Arc<grain_core::AppContext>>()
            .ok_or("MCP connection storage is unavailable.")?;
        let registry = Arc::new(
            ConnectionRegistry::load(&context.data_dir).map_err(|error| error.to_string())?,
        );
        *slot = Some(registry.clone());
        Ok(registry)
    }
}

/// Host UI projection, deliberately omitting vault/account IDs and numeric u64
/// revisions (JavaScript numbers cannot represent all supported revisions).
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    id: String,
    revision: String,
    name: String,
    url: String,
    authentication: String,
    state: String,
}

impl From<ConnectionRecord> for ConnectionView {
    fn from(record: ConnectionRecord) -> Self {
        let definition = record.definition();
        Self {
            id: record.identity().connection_id().into(),
            revision: record.revision().to_string(),
            name: definition.name.clone(),
            url: definition.url.clone(),
            authentication: match definition.authentication {
                McpAuthentication::OAuth {} => "oauth",
                McpAuthentication::None {} => "none",
            }
            .into(),
            state: "inactive".into(),
        }
    }
}

fn revision(value: &str) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| RegistryError::Conflict.to_string())?;
    if parsed == 0 || parsed.to_string() != value {
        return Err(RegistryError::Conflict.to_string());
    }
    Ok(parsed)
}

fn expected_lease(
    registry: &ConnectionRegistry,
    id: &str,
    expected: &str,
) -> Result<ConnectionLease, String> {
    let expected = revision(expected)?;
    let lease = registry.lease(id).map_err(|error| error.to_string())?;
    if lease.record().revision() != expected {
        return Err(RegistryError::Conflict.to_string());
    }
    Ok(lease)
}

fn guard(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    crate::grain_commands::require_main_window(window)?;
    super::require_developer_mode(app)
}

async fn storage<T: Send + 'static>(
    app: AppHandle,
    operation: impl FnOnce(&ConnectionRegistry) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    // Registry disk I/O and its bounded publication retry never block the UI.
    tokio::task::spawn_blocking(move || {
        // Recheck after scheduling; a queued command cannot use old dev consent.
        super::require_developer_mode(&app)?;
        let state = app
            .try_state::<State>()
            .ok_or("MCP connection storage is unavailable.")?;
        // Serialize the command's mutation and returned view as one host unit.
        let _operation = state
            .operation
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        super::require_developer_mode(&app)?;
        let registry = state.registry(&app)?;
        operation(&registry)
    })
    .await
    .map_err(|_| "MCP connection storage operation failed.".to_string())?
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connections_list(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Vec<ConnectionView>, String> {
    guard(&app, &window)?;
    storage(app, |registry| {
        registry
            .list()
            .map(|records| records.into_iter().map(Into::into).collect())
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_import(
    app: AppHandle,
    window: WebviewWindow,
    definition_json: String,
) -> Result<ConnectionView, String> {
    guard(&app, &window)?;
    let definition = ConnectionDefinition::parse(definition_json.as_bytes())
        .map_err(|error| error.to_string())?;
    storage(app, move |registry| {
        registry
            .insert(definition)
            .map(Into::into)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_replace(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
    definition_json: String,
) -> Result<ConnectionView, String> {
    guard(&app, &window)?;
    let definition = ConnectionDefinition::parse(definition_json.as_bytes())
        .map_err(|error| error.to_string())?;
    storage(app, move |registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        // No configured runtime/credential writer exists yet. Before adding one,
        // invalidate its operations and dispose the returned retired account.
        registry
            .replace(&lease, definition)
            .map_err(|error| error.to_string())?;
        Ok(registry
            .lease(&id)
            .map_err(|error| error.to_string())?
            .record()
            .clone()
            .into())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_remove(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
) -> Result<(), String> {
    guard(&app, &window)?;
    storage(app, move |registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        registry.remove(&lease).map_err(|error| error.to_string())?;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_round_trip_beyond_javascript_precision_without_aliases() {
        for value in [1, 9_007_199_254_740_993, u64::MAX] {
            assert_eq!(revision(&value.to_string()).unwrap(), value);
        }
        for value in ["", "0", "01", "+1", " 1", "1.0", "18446744073709551616"] {
            assert!(revision(value).is_err(), "{value}");
        }
    }
}
