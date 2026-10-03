//! Host configuration and current-revision ownership for direct remote MCP.
//! Import stays inactive. Anonymous activation reuses the shared MCP runtime;
//! OAuth stays inactive until account acquisition/retirement are integrated.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use grain_core::mcp_connections::{
    ConnectionDefinition, ConnectionLease, ConnectionRecord, ConnectionRegistry, RegistryError,
};
use grain_sdk::mcp::McpAuthentication;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, WebviewWindow};

#[derive(Default)]
pub struct State {
    registry: Mutex<Option<Arc<ConnectionRegistry>>>,
    operation: Mutex<()>,
    controls: Mutex<BTreeMap<String, RuntimeOwner>>,
}

#[derive(Clone)]
pub(super) struct RuntimeOwner {
    registry: Arc<ConnectionRegistry>,
    lease: ConnectionLease,
    control: Arc<super::session::Control>,
    provider_id: String,
}

impl RuntimeOwner {
    pub(super) fn spec(&self) -> super::Provider<'_> {
        let definition = self.lease.record().definition();
        super::Provider {
            id: &self.provider_id,
            name: &definition.name,
            description: "Tools from a directly configured MCP server.",
            endpoint: &definition.url,
            registration: match definition.authentication {
                McpAuthentication::None {} => super::Registration::Anonymous,
                McpAuthentication::OAuth {} => super::Registration::Dynamic,
            },
            setup_url: &definition.url,
        }
    }

    pub(super) fn ticket(&self) -> super::session::Ticket {
        self.control.ticket()
    }

    fn enable_key(&self) -> String {
        self.lease.record().identity().vault_account().into_owned()
    }

    pub(super) fn enabled(&self, app: &AppHandle) -> bool {
        let settings = crate::settings::get_settings(app);
        self.spec().registration == super::Registration::Anonymous
            && settings.extension_developer_mode
            && settings.mcp_enabled_providers.contains(&self.enable_key())
            && self.registry.is_current(&self.lease)
    }

    fn stamp(&self) -> String {
        let record = self.lease.record();
        format!(
            "{:x}",
            Sha256::digest(format!(
                "{}:{}",
                record.identity().vault_account(),
                record.revision()
            ))
        )
    }

    pub(super) fn bind(&self, digest: &str) -> String {
        format!("{}:{digest}", self.stamp())
    }

    pub(super) fn unbind<'a>(&self, digest: &'a str) -> Result<&'a str, String> {
        let (stamp, digest) = digest.split_once(':').ok_or(super::session::CANCELLED)?;
        if stamp != self.stamp() {
            return Err(super::session::CANCELLED.into());
        }
        Ok(digest)
    }
}

impl State {
    fn owner(
        &self,
        registry: Arc<ConnectionRegistry>,
        lease: ConnectionLease,
    ) -> Result<RuntimeOwner, String> {
        if !registry.is_current(&lease) {
            return Err(RegistryError::Conflict.to_string());
        }
        let id = lease.record().identity().connection_id().to_string();
        let mut controls = self
            .controls
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        if let Some(owner) = controls.get_mut(&id) {
            if owner.lease != lease {
                owner.control.invalidate();
                owner.lease = lease;
            }
            return Ok(owner.clone());
        }
        if controls.len() >= grain_core::mcp_connections::MAX_CONNECTIONS {
            return Err(RegistryError::Limit.to_string());
        }
        let owner = RuntimeOwner {
            registry,
            lease,
            control: super::session::Control::new(),
            provider_id: format!("configured-{id}"),
        };
        controls.insert(id, owner.clone());
        Ok(owner)
    }

    pub(super) fn resolve(
        &self,
        app: &AppHandle,
        id: &str,
    ) -> Result<(RuntimeOwner, super::session::Ticket), String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        let registry = self.registry(app)?;
        let lease = registry.lease(id).map_err(|error| error.to_string())?;
        let owner = self.owner(registry, lease)?;
        // Metadata and cancellation generation are captured under the same
        // mutation lock. A delayed caller must never borrow a newer generation
        // for an older endpoint/account/revision snapshot.
        let ticket = owner.ticket();
        Ok((owner, ticket))
    }

    fn invalidate(&self, id: &str) -> Result<(), String> {
        if let Some(owner) = self
            .controls
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?
            .get(id)
        {
            owner.control.invalidate();
        }
        Ok(())
    }

    pub(super) fn invalidate_all(&self) {
        if let Ok(controls) = self.controls.lock() {
            for owner in controls.values() {
                owner.control.invalidate();
            }
        }
    }

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

fn view(
    app: &AppHandle,
    registry: &ConnectionRegistry,
    record: ConnectionRecord,
) -> ConnectionView {
    let settings = crate::settings::get_settings(app);
    let enabled = matches!(
        record.definition().authentication,
        McpAuthentication::None {}
    ) && settings.extension_developer_mode
        && settings
            .mcp_enabled_providers
            .iter()
            .any(|key| key == record.identity().vault_account().as_ref())
        && registry
            .lease(record.identity().connection_id())
            .is_ok_and(|lease| registry.is_current(&lease));
    let mut result = ConnectionView::from(record);
    if enabled {
        result.state = "enabled".into();
    }
    result
}

fn prune_enabled(app: &AppHandle, id: &str, keep: Option<&str>) -> Result<(), String> {
    let context = app
        .try_state::<Arc<grain_core::AppContext>>()
        .ok_or("Application context unavailable")?;
    let prefix = format!("mcp:v1:configured:{id}:");
    context
        .update_settings(|settings| {
            settings
                .mcp_enabled_providers
                .retain(|key| !key.starts_with(&prefix) || keep == Some(key.as_str()))
        })
        .map_err(|_| {
            "MCP metadata was saved, but old enablement cleanup failed. Reload before trying again."
                .into()
        })
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
    operation: impl FnOnce(&State, &Arc<ConnectionRegistry>) -> Result<T, String> + Send + 'static,
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
        operation(&state, &registry)
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
    let view_app = app.clone();
    storage(app, move |_, registry| {
        registry
            .list()
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| view(&view_app, registry, record))
                    .collect()
            })
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
    storage(app, move |_, registry| {
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
    let view_app = app.clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        if lease.record().definition() != definition.definition() {
            state.invalidate(&id)?;
        }
        // OAuth stays inactive until explicit retired-account vault disposal is
        // added. Anonymous operations are cancelled before metadata publication.
        let retired = registry
            .replace(&lease, definition)
            .map_err(|error| error.to_string())?;
        let record = registry
            .lease(&id)
            .map_err(|error| error.to_string())?
            .record()
            .clone();
        if retired.is_some() {
            prune_enabled(
                &view_app,
                &id,
                Some(record.identity().vault_account().as_ref()),
            )?;
        }
        Ok(view(&view_app, registry, record))
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
    let view_app = app.clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        state.invalidate(&id)?;
        registry.remove(&lease).map_err(|error| error.to_string())?;
        state
            .controls
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?
            .remove(&id);
        prune_enabled(&view_app, &id, None)?;
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_set_enabled(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
    enabled: bool,
) -> Result<(), String> {
    guard(&app, &window)?;
    let context = app
        .try_state::<Arc<grain_core::AppContext>>()
        .ok_or("Application context is unavailable.")?
        .inner()
        .clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        let owner = state.owner(registry.clone(), lease)?;
        if enabled && owner.spec().registration != super::Registration::Anonymous {
            return Err("Configured OAuth connections require the upcoming account integration; this server remains inactive.".into());
        }
        owner.control.invalidate();
        context.update_settings(|settings| {
            let key = owner.enable_key();
            // Only exact account ownership can enable this destination. Neither
            // raw record IDs nor old account keys can activate an edited URL.
            settings.mcp_enabled_providers.retain(|value| value != &key);
            if enabled && settings.extension_developer_mode {
                settings.mcp_enabled_providers.push(key);
                settings.mcp_enabled_providers.sort();
                settings.mcp_enabled_providers.dedup();
            }
        }).map_err(|error| error.to_string())
    }).await
}

pub(super) fn directory(
    app: &AppHandle,
) -> Vec<grain_core::capability_index::ExtensionDirectoryEntry> {
    let Ok(records) = (|| {
        let state = app.try_state::<State>().ok_or("MCP storage unavailable")?;
        let _operation = state
            .operation
            .lock()
            .map_err(|_| "MCP storage unavailable")?;
        let registry = state.registry(app)?;
        let records = registry.list().map_err(|error| error.to_string())?;
        if let Some(record) = records.first() {
            let lease = registry
                .lease(record.identity().connection_id())
                .map_err(|error| error.to_string())?;
            if !registry.is_current(&lease) {
                return Err(RegistryError::Conflict.to_string());
            }
        }
        Ok::<_, String>(records)
    })() else {
        return Vec::new();
    };
    let settings = crate::settings::get_settings(app);
    if !settings.extension_developer_mode {
        return Vec::new();
    }
    records
        .into_iter()
        .filter(|record| {
            matches!(
                record.definition().authentication,
                McpAuthentication::None {}
            ) && settings
                .mcp_enabled_providers
                .iter()
                .any(|key| key == record.identity().vault_account().as_ref())
        })
        .map(
            |record| grain_core::capability_index::ExtensionDirectoryEntry {
                extension_id: format!("mcp.configured-{}", record.identity().connection_id()),
                name: record.definition().name.clone(),
                description: "Tools from a directly configured MCP server.".into(),
                action_count: 1,
            },
        )
        .collect()
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

    #[test]
    fn owner_stamps_separate_identical_servers_and_follow_revision_changes() {
        let data = tempfile::tempdir().unwrap();
        let registry = Arc::new(ConnectionRegistry::load(data.path()).unwrap());
        let definition = ConnectionDefinition::parse(br#"{"name":"Server","url":"https://server.example.com/mcp","authentication":{"type":"none"}}"#).unwrap();
        let first = registry.insert(definition.clone()).unwrap();
        let second = registry.insert(definition).unwrap();
        let state = State::default();
        let lease = registry.lease(first.identity().connection_id()).unwrap();
        let one = state.owner(registry.clone(), lease.clone()).unwrap();
        let two = state
            .owner(
                registry.clone(),
                registry.lease(second.identity().connection_id()).unwrap(),
            )
            .unwrap();
        let approval = one.bind("same-tools");
        assert!(two.unbind(&approval).is_err());
        assert_eq!(one.unbind(&approval).unwrap(), "same-tools");
        let ticket = one.ticket();
        let old_generation = ticket.bind_digest(&approval);
        let renamed = ConnectionDefinition::parse(br#"{"name":"Renamed","url":"https://server.example.com/mcp","authentication":{"type":"none"}}"#).unwrap();
        registry.replace(&lease, renamed).unwrap();
        let current = state
            .owner(
                registry.clone(),
                registry.lease(first.identity().connection_id()).unwrap(),
            )
            .unwrap();
        assert!(Arc::ptr_eq(&one.control, &current.control));
        assert!(current.unbind(&approval).is_err());
        assert!(current.ticket().unbind_digest(&old_generation).is_err());
        assert!(ticket.commit(|| ()).is_err());
        assert_eq!(current.spec().name, "Renamed");
        assert_eq!(state.controls.lock().unwrap().len(), 2);
    }

    #[test]
    fn copied_spec_and_enable_key_cannot_follow_a_destination_account_rotation() {
        let data = tempfile::tempdir().unwrap();
        let registry = Arc::new(ConnectionRegistry::load(data.path()).unwrap());
        let definition = ConnectionDefinition::parse(br#"{"name":"Server","url":"https://server.example.com/mcp","authentication":{"type":"none"}}"#).unwrap();
        let record = registry.insert(definition).unwrap();
        let state = State::default();
        let lease = registry.lease(record.identity().connection_id()).unwrap();
        let old = state.owner(registry.clone(), lease.clone()).unwrap();
        let replacement = ConnectionDefinition::parse(br#"{"name":"Server","url":"https://other.example.com/mcp","authentication":{"type":"oauth"}}"#).unwrap();
        registry.replace(&lease, replacement).unwrap();
        let next = state
            .owner(
                registry.clone(),
                registry.lease(record.identity().connection_id()).unwrap(),
            )
            .unwrap();
        assert_ne!(old.enable_key(), next.enable_key());
        assert_eq!(old.spec().endpoint, "https://server.example.com/mcp");
        assert_eq!(next.spec().endpoint, "https://other.example.com/mcp");
        assert!(matches!(
            old.spec().registration,
            super::super::Registration::Anonymous
        ));
        assert!(matches!(
            next.spec().registration,
            super::super::Registration::Dynamic
        ));
        assert!(!registry.is_current(&old.lease));
    }
}
