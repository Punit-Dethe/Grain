//! Host configuration and current-revision ownership for direct remote MCP.
//! Import stays inactive. Anonymous activation reuses the shared MCP runtime;
//! OAuth uses the shared SDK with revision-owned vault publication and cleanup.

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
    store: Option<Arc<crate::grain_store::StoreState>>,
}

impl RuntimeOwner {
    pub(super) fn spec(&self) -> super::Provider<'_> {
        let definition = self.lease.record().definition();
        super::Provider {
            id: &self.provider_id,
            name: &definition.name,
            description: "Tools from this MCP server.",
            endpoint: &definition.url,
            registration: match definition.authentication {
                McpAuthentication::None {} => super::Registration::Anonymous,
                McpAuthentication::OAuth {} => super::Registration::Dynamic,
            },
            setup_url: &definition.url,
            ownership: Some(self),
        }
    }

    pub(super) fn ticket(&self) -> super::session::Ticket {
        self.control.ticket()
    }

    fn enable_key(&self) -> String {
        self.lease.record().identity().vault_account().into_owned()
    }

    pub(super) fn account(&self) -> String {
        self.enable_key()
    }

    pub(super) fn current(&self) -> bool {
        self.registry.is_current(&self.lease)
            && match self.lease.record().identity().source() {
                grain_core::mcp::ConnectionSource::Store { .. } => self
                    .store
                    .as_ref()
                    .is_some_and(|store| store.mcp_record_allowed(self.lease.record())),
                _ => true,
            }
    }

    pub(super) fn invalidate(&self, app: &AppHandle) -> Result<super::session::Ticket, String> {
        let state = app.try_state::<State>().ok_or("MCP storage unavailable")?;
        let _operation = state
            .operation
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        super::require_developer_mode(app)?;
        if !self.current() {
            return Err(RegistryError::Conflict.to_string());
        }
        Ok(self.control.invalidate())
    }

    pub(super) fn enabled(&self, app: &AppHandle) -> bool {
        let settings = crate::settings::get_settings(app);
        settings.extension_developer_mode
            && settings.mcp_enabled_providers.contains(&self.enable_key())
            && self.current()
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
        store: Option<Arc<crate::grain_store::StoreState>>,
    ) -> Result<RuntimeOwner, String> {
        if !registry.is_current(&lease) {
            return Err(RegistryError::Conflict.to_string());
        }
        let store = if matches!(
            lease.record().identity().source(),
            grain_core::mcp::ConnectionSource::Store { .. }
        ) {
            store
        } else {
            None
        };
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
            owner.store = store;
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
            store,
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
        let store = app
            .try_state::<Arc<crate::grain_store::StoreState>>()
            .map(|store| store.inner().clone());
        let owner = self.owner(registry, lease, store)?;
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

    pub(crate) fn registry(&self, app: &AppHandle) -> Result<Arc<ConnectionRegistry>, String> {
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

    pub(crate) fn commit_store_update(
        &self,
        app: &AppHandle,
        registry: &ConnectionRegistry,
        lease: &ConnectionLease,
        admitted: grain_core::mcp::StoreDescriptor,
    ) -> Result<ConnectionRecord, String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        super::require_developer_mode(app)?;
        let grain_core::mcp::ConnectionSource::Store {
            extension_id,
            artifact_sha256,
            ..
        } = lease.record().identity().source()
        else {
            return Err(RegistryError::Conflict.to_string());
        };
        if !registry.is_current(lease) || extension_id != &admitted.descriptor().descriptor().id {
            return Err(RegistryError::Conflict.to_string());
        }
        if artifact_sha256 == admitted.sha256() {
            return Ok(lease.record().clone());
        }
        if lease.record().revision() == u64::MAX {
            return Err(RegistryError::RevisionExhausted.to_string());
        }
        let id = lease.record().identity().connection_id();
        self.invalidate(id)?;
        prune_enabled(app, id, None)?;
        let changed_account = lease.record().definition().url != admitted.descriptor().endpoint()
            || lease.record().definition().authentication
                != admitted.descriptor().descriptor().authentication;
        if changed_account {
            retire_account(lease.record())?;
            prune_client_ids(app, id)?;
        }
        registry
            .replace_store(lease, admitted)
            .map_err(|error| error.to_string())?;
        Ok(registry
            .lease(id)
            .map_err(|error| error.to_string())?
            .record()
            .clone())
    }

    /// Cancel first, then persist disablement. Grants/data remain for explicit
    /// removal; runtime/vault freshness consults the signed state independently
    /// so a settings-save failure cannot authorize a revoked owner.
    pub(crate) fn enforce_store_revocations(
        &self,
        app: &AppHandle,
        store: &crate::grain_store::StoreState,
    ) -> Result<(), String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        let registry = self.registry(app)?;
        let mut failure = None;
        for record in registry.list().map_err(|error| error.to_string())? {
            if matches!(
                record.identity().source(),
                grain_core::mcp::ConnectionSource::Store { .. }
            ) && !store.mcp_record_allowed(&record)
            {
                let id = record.identity().connection_id();
                // One failed settings save must not prevent cancellation of
                // another revoked owner. Runtime freshness also fails closed.
                if let Err(error) = self
                    .invalidate(id)
                    .and_then(|()| prune_enabled(app, id, None))
                {
                    failure.get_or_insert(error);
                }
            }
        }
        failure.map_or(Ok(()), Err)
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
    let enabled = settings.extension_developer_mode
        && record_allowed(app, &record)
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

fn record_allowed(app: &AppHandle, record: &ConnectionRecord) -> bool {
    match record.identity().source() {
        grain_core::mcp::ConnectionSource::Store { .. } => app
            .try_state::<Arc<crate::grain_store::StoreState>>()
            .is_some_and(|store| store.mcp_record_allowed(record)),
        _ => true,
    }
}

fn prune_enabled(app: &AppHandle, id: &str, keep: Option<&str>) -> Result<(), String> {
    let context = app
        .try_state::<Arc<grain_core::AppContext>>()
        .ok_or("Application context unavailable")?;
    let prefix = account_prefix(app, id)?;
    context
        .update_settings(|settings| {
            settings
                .mcp_enabled_providers
                .retain(|key| !key.starts_with(&prefix) || keep == Some(key.as_str()));
        })
        .map_err(|_| {
            "Could not update configured MCP enablement. Reload before trying again.".into()
        })
}

fn retire_account(record: &ConnectionRecord) -> Result<(), String> {
    if matches!(
        record.definition().authentication,
        McpAuthentication::None {}
    ) {
        return Ok(());
    }
    retire_with(record, |service, account| {
        super::delete_vault_entry(service, account).map_err(|_| ())
    })
}

fn prune_client_ids(app: &AppHandle, id: &str) -> Result<(), String> {
    let context = app
        .try_state::<Arc<grain_core::AppContext>>()
        .ok_or("Application context unavailable")?;
    let prefix = account_prefix(app, id)?;
    context
        .update_settings(|settings| {
            settings
                .mcp_oauth_client_ids
                .retain(|key, _| !key.starts_with(&prefix));
        })
        .map_err(|_| "Could not remove configured MCP client metadata. Reload and retry.".into())
}

fn account_prefix(app: &AppHandle, id: &str) -> Result<String, String> {
    let state = app.try_state::<State>().ok_or("MCP storage unavailable")?;
    let lease = state
        .registry(app)?
        .lease(id)
        .map_err(|error| error.to_string())?;
    let account = lease.record().identity().vault_account();
    let (prefix, _) = account
        .rsplit_once(':')
        .ok_or("Invalid MCP account ownership")?;
    Ok(format!("{prefix}:"))
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_set_client_credentials(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
    client_id: String,
    client_secret: String,
) -> Result<(), String> {
    guard(&app, &window)?;
    let (owner, ticket) = expected_owner(&app, &id, &expected_revision)?;
    super::set_client_credentials_flow(
        &app,
        super::RuntimeProvider::Configured(Box::new(owner), ticket),
        client_id,
        client_secret,
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_clear_client_credentials(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
) -> Result<(), String> {
    guard(&app, &window)?;
    let view_app = app.clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        if matches!(
            lease.record().definition().authentication,
            McpAuthentication::None {}
        ) {
            return Err("This anonymous server has no OAuth client to configure.".into());
        }
        if !registry.is_current(&lease) {
            return Err(RegistryError::Conflict.to_string());
        }
        state.invalidate(&id)?;
        prune_enabled(&view_app, &id, None)?;
        retire_account(lease.record())?;
        prune_client_ids(&view_app, &id)
    })
    .await
}

fn retire_with(
    record: &ConnectionRecord,
    mut delete: impl FnMut(&str, &str) -> Result<(), ()>,
) -> Result<(), String> {
    if matches!(
        record.definition().authentication,
        McpAuthentication::None {}
    ) {
        return Ok(());
    }
    let account = record.identity().vault_account();
    let mut failed = false;
    for service in [
        super::VAULT_SERVICE,
        super::CLIENT_SECRET_SERVICE,
        super::CLIENT_REGISTRATION_SERVICE,
    ] {
        failed |= delete(service, &account).is_err();
    }
    if failed {
        Err("Could not retire the configured MCP account. Metadata was preserved; reload and retry cleanup.".into())
    } else {
        Ok(())
    }
}

fn expected_owner(
    app: &AppHandle,
    id: &str,
    expected: &str,
) -> Result<(RuntimeOwner, super::session::Ticket), String> {
    let state = app.try_state::<State>().ok_or("MCP storage unavailable")?;
    let resolved = state.resolve(app, id)?;
    if resolved.0.lease.record().revision() != revision(expected)? {
        return Err(RegistryError::Conflict.to_string());
    }
    Ok(resolved)
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_connect(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
) -> Result<(), String> {
    guard(&app, &window)?;
    let (owner, ticket) = expected_owner(&app, &id, &expected_revision)?;
    if !owner.current() {
        return Err("This MCP connection is revoked or no longer available.".into());
    }
    super::connect_flow(
        &app,
        &window,
        super::RuntimeProvider::Configured(Box::new(owner), ticket),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_status(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
) -> Result<super::McpProviderStatus, String> {
    guard(&app, &window)?;
    let (owner, ticket) = expected_owner(&app, &id, &expected_revision)?;
    let item = owner.spec();
    let required = super::requires_account(&item);
    let stored = if required {
        use rmcp::transport::auth::CredentialStore;
        let store = super::VaultCredentialStore {
            account: owner.account(),
            ticket: ticket.clone(),
            _operation: None,
            current: Some(owner.clone()),
        };
        Some(store.load().await)
    } else {
        None
    };
    let observation = ticket.recovery();
    ticket.commit(|| {
        if !owner.current() {
            return Err(RegistryError::Conflict.to_string());
        }
        let connected = stored.as_ref().is_some_and(|result| {
            result.as_ref().is_ok_and(|value| {
                value
                    .as_ref()
                    .is_some_and(|value| value.token_response.is_some())
            })
        });
        let state = if !required {
            "anonymous"
        } else if stored.as_ref().is_some_and(Result::is_err) {
            "unavailable"
        } else if let Some(recovery) = observation {
            recovery.state()
        } else if connected {
            "stored"
        } else {
            "disconnected"
        };
        Ok(super::McpProviderStatus {
            id: item.id.into(),
            name: item.name.into(),
            description: item.description.into(),
            endpoint: item.endpoint.into(),
            setup_url: item.setup_url.into(),
            requires_client_credentials: super::preregistered(&app, &item),
            client_id_configured: super::provider_client_id(&app, &item).is_some(),
            connected,
            enabled: (!required || connected) && owner.enabled(&app),
            state: state.into(),
        })
    })?
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_disconnect(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
) -> Result<(), String> {
    use rmcp::transport::auth::CredentialStore;
    guard(&app, &window)?;
    let (owner, _) = expected_owner(&app, &id, &expected_revision)?;
    if !super::requires_account(&owner.spec()) {
        return Err("This anonymous server has no account to disconnect.".into());
    }
    let ticket = owner.invalidate(&app)?;
    let result = tokio::time::timeout(
        super::OPERATION_TIMEOUT,
        ticket.run(async {
            let operation = ticket.acquire().await?;
            ticket.commit(|| {
                if !owner.current() {
                    return Err(RegistryError::Conflict.to_string());
                }
                prune_enabled(&app, &id, None)
            })??;
            super::VaultCredentialStore::for_provider(&owner.spec(), &ticket, &operation)
                .clear()
                .await
                .map_err(|error| error.to_string())
        }),
    )
    .await;
    if !matches!(&result, Ok(Ok(Ok(())))) {
        ticket.invalidate_if_current();
    }
    result.map_err(|_| "Configured MCP disconnect timed out".to_string())??
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

pub(crate) fn expected_lease(
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
        if matches!(
            lease.record().identity().source(),
            grain_core::mcp::ConnectionSource::Store { .. }
        ) {
            return Err(
                "Store MCP metadata must be updated through the verified catalogue.".into(),
            );
        }
        if lease.record().definition() != definition.definition()
            && lease.record().revision() == u64::MAX
        {
            return Err(RegistryError::RevisionExhausted.to_string());
        }
        if lease.record().definition() != definition.definition() {
            state.invalidate(&id)?;
        }
        let account_changed = lease.record().definition().url != definition.definition().url
            || lease.record().definition().authentication != definition.definition().authentication;
        if account_changed {
            if !registry.is_current(&lease) {
                return Err(RegistryError::Conflict.to_string());
            }
            // Disable and clean the old account BEFORE publishing a new identity.
            // A failed cleanup preserves metadata/identity for a safe retry.
            prune_enabled(&view_app, &id, None)?;
            retire_account(lease.record())?;
            prune_client_ids(&view_app, &id)?;
        }
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
        if !registry.is_current(&lease) {
            return Err(RegistryError::Conflict.to_string());
        }
        state.invalidate(&id)?;
        prune_enabled(&view_app, &id, None)?;
        retire_account(lease.record())?;
        prune_client_ids(&view_app, &id)?;
        registry.remove(&lease).map_err(|error| error.to_string())?;
        state
            .controls
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?
            .remove(&id);
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
    let view_app = app.clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        let store = view_app
            .try_state::<Arc<crate::grain_store::StoreState>>()
            .map(|store| store.inner().clone());
        let owner = state.owner(registry.clone(), lease, store)?;
        if enabled && !owner.current() {
            return Err("This MCP connection is revoked or no longer available.".into());
        }
        if enabled
            && super::requires_account(&owner.spec())
            && super::read_credentials_sync(&owner.account())
                .map_err(|error| error.to_string())?
                .is_none_or(|stored| stored.token_response.is_none())
        {
            return Err("Connect this configured MCP account before enabling it.".into());
        }
        owner.control.invalidate();
        context
            .update_settings(|settings| {
                let key = owner.enable_key();
                // Only exact account ownership can enable this destination. Neither
                // raw record IDs nor old account keys can activate an edited URL.
                settings.mcp_enabled_providers.retain(|value| value != &key);
                if enabled && settings.extension_developer_mode {
                    settings.mcp_enabled_providers.push(key);
                    settings.mcp_enabled_providers.sort();
                    settings.mcp_enabled_providers.dedup();
                }
            })
            .map_err(|error| error.to_string())
    })
    .await
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
            record_allowed(app, record)
                && settings
                    .mcp_enabled_providers
                    .iter()
                    .any(|key| key == record.identity().vault_account().as_ref())
        })
        .map(
            |record| grain_core::capability_index::ExtensionDirectoryEntry {
                extension_id: format!("mcp.configured-{}", record.identity().connection_id()),
                name: record.definition().name.clone(),
                description: "Tools from this MCP server.".into(),
                action_count: 1,
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_descriptor() -> grain_core::mcp::StoreDescriptor {
        let entry = serde_json::from_slice::<grain_sdk::Index>(include_bytes!(
            "../../crates/grain-core/tests/fixtures/mcp-store/index.json"
        ))
        .unwrap()
        .entries
        .remove(0);
        grain_core::mcp::StoreDescriptor::admit(
            &entry,
            include_bytes!("../../crates/grain-core/tests/fixtures/mcp-store/descriptor.json"),
        )
        .unwrap()
    }

    #[test]
    fn store_revocation_denies_vault_io_before_generation_or_settings_cleanup() {
        for (id, version, status, allowed) in [
            ("com.example.calendar", Some("1.0.0"), "revoked", false),
            ("com.example.calendar", None, "revoked", false),
            ("com.example.other", None, "revoked", true),
            ("com.example.calendar", Some("2.0.0"), "revoked", true),
            ("com.example.calendar", None, "deprecated", true),
        ] {
            let data = tempfile::tempdir().unwrap();
            let registry = Arc::new(ConnectionRegistry::load(data.path()).unwrap());
            let record = registry.insert_store(store_descriptor()).unwrap();
            let lease = registry.lease(record.identity().connection_id()).unwrap();
            let state = State::default();
            assert!(
                !state
                    .owner(registry.clone(), lease.clone(), None)
                    .unwrap()
                    .current(),
                "Store ownership without a revocation policy must fail closed"
            );
            let policy = Arc::new(crate::grain_store::StoreState::init(data.path()));
            let owner = state
                .owner(registry.clone(), lease, Some(policy.clone()))
                .unwrap();
            assert!(owner.current());
            let ticket = owner.ticket();
            let vault = super::super::VaultCredentialStore {
                account: owner.account(),
                ticket: ticket.clone(),
                _operation: None,
                current: Some(owner.clone()),
            };
            policy.set_test_revocations(serde_json::from_value(serde_json::json!({
                "spec":1,"version":100,"expires":"2099-01-01T00:00:00Z",
                "entries":[{"id":id,"version":version,"state":status,"reason":"component fixture"}]
            })).unwrap());
            assert_eq!(owner.current(), allowed);
            assert!(
                ticket.commit(|| ()).is_ok(),
                "No generation cancellation has occurred"
            );
            let mut executed = false;
            assert_eq!(
                vault
                    .commit(|| {
                        executed = true;
                        Ok(())
                    })
                    .is_ok(),
                allowed
            );
            assert_eq!(executed, allowed, "Revoked account performed vault IO");
            state.invalidate(record.identity().connection_id()).unwrap();
            assert!(ticket.commit(|| ()).is_err());
            policy.close();
        }
    }

    #[test]
    fn store_updates_invalidate_copied_approvals_and_use_only_owned_retirement_keys() {
        let data = tempfile::tempdir().unwrap();
        let registry = Arc::new(ConnectionRegistry::load(data.path()).unwrap());
        let record = registry.insert_store(store_descriptor()).unwrap();
        let lease = registry.lease(record.identity().connection_id()).unwrap();
        let state = State::default();
        let policy = Arc::new(crate::grain_store::StoreState::init(data.path()));
        let owner = state
            .owner(registry.clone(), lease.clone(), Some(policy.clone()))
            .unwrap();
        let approval = owner.bind("tools");
        let ticket = owner.ticket();
        let mut artifact: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../crates/grain-core/tests/fixtures/mcp-store/descriptor.json"
        ))
        .unwrap();
        artifact["version"] = "2.0.0".into();
        artifact["transport"]["url"] = "https://other.example.com/mcp".into();
        let bytes = serde_json::to_vec(&artifact).unwrap();
        let mut entry = serde_json::from_slice::<grain_sdk::Index>(include_bytes!(
            "../../crates/grain-core/tests/fixtures/mcp-store/index.json"
        ))
        .unwrap()
        .entries
        .remove(0);
        entry.version = "2.0.0".into();
        entry.size = bytes.len() as u64;
        entry.sha256 = grain_core::trust::sha256_hex(&bytes);
        let update = grain_core::mcp::StoreDescriptor::admit(&entry, &bytes).unwrap();
        let mut deleted = 0;
        retire_with(&record, |_, account| {
            assert_eq!(account, owner.account());
            assert!(account.starts_with("mcp:v1:store:com.example.calendar:"));
            deleted += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(deleted, 3);
        registry.replace_store(&lease, update).unwrap();
        assert!(!owner.current());
        let next = state
            .owner(
                registry.clone(),
                registry.lease(record.identity().connection_id()).unwrap(),
                Some(policy.clone()),
            )
            .unwrap();
        assert_ne!(owner.account(), next.account());
        assert!(next.unbind(&approval).is_err());
        assert!(ticket.commit(|| ()).is_err());
        assert_eq!(next.spec().endpoint, "https://other.example.com/mcp");
        policy.close();
    }

    #[test]
    fn external_metadata_change_denies_vault_io_without_a_generation_notification() {
        let data = tempfile::tempdir().unwrap();
        let registry = Arc::new(ConnectionRegistry::load(data.path()).unwrap());
        let input = ConnectionDefinition::parse(br#"{"name":"Account","url":"https://server.example.com/mcp","authentication":{"type":"oauth"}}"#).unwrap();
        let record = registry.insert(input).unwrap();
        let lease = registry.lease(record.identity().connection_id()).unwrap();
        let state = State::default();
        let owner = state.owner(registry.clone(), lease.clone(), None).unwrap();
        let ticket = owner.ticket();
        let store = super::super::VaultCredentialStore {
            account: owner.account(),
            ticket: ticket.clone(),
            _operation: None,
            current: Some(owner),
        };
        let changed = ConnectionDefinition::parse(br#"{"name":"Changed","url":"https://other.example.com/mcp","authentication":{"type":"oauth"}}"#).unwrap();
        registry.replace(&lease, changed).unwrap();
        assert!(
            ticket.commit(|| ()).is_ok(),
            "No in-process invalidation occurred"
        );
        let mut executed = false;
        assert!(store
            .commit(|| {
                executed = true;
                Ok(())
            })
            .is_err());
        assert!(!executed, "Stale account performed vault IO");
    }

    #[test]
    fn retirement_checks_all_three_exact_owned_keys_and_reports_partial_failure() {
        let data = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(data.path()).unwrap();
        let record = registry.insert(ConnectionDefinition::parse(br#"{"name":"Account","url":"https://server.example.com/mcp","authentication":{"type":"oauth"}}"#).unwrap()).unwrap();
        let expected = record.identity().vault_account();
        let mut services = Vec::new();
        assert!(retire_with(&record, |service, account| {
            assert_eq!(account, expected);
            services.push(service.to_string());
            if services.len() == 1 {
                Err(())
            } else {
                Ok(())
            }
        })
        .is_err());
        assert_eq!(services.len(), 3);
        assert!(services.contains(&super::super::VAULT_SERVICE.to_string()));
        let anonymous = registry.insert(ConnectionDefinition::parse(br#"{"name":"Anonymous","url":"https://server.example.com/mcp","authentication":{"type":"none"}}"#).unwrap()).unwrap();
        assert!(retire_with(&anonymous, |_, _| panic!(
            "Anonymous metadata touched the vault"
        ))
        .is_ok());
    }

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
        let one = state.owner(registry.clone(), lease.clone(), None).unwrap();
        let two = state
            .owner(
                registry.clone(),
                registry.lease(second.identity().connection_id()).unwrap(),
                None,
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
                None,
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
        let old = state.owner(registry.clone(), lease.clone(), None).unwrap();
        let replacement = ConnectionDefinition::parse(br#"{"name":"Server","url":"https://other.example.com/mcp","authentication":{"type":"oauth"}}"#).unwrap();
        registry.replace(&lease, replacement).unwrap();
        let next = state
            .owner(
                registry.clone(),
                registry.lease(record.identity().connection_id()).unwrap(),
                None,
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
