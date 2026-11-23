//! Host configuration and current-revision ownership for custom MCP connections
//! (remote HTTPS, local loopback HTTP, and local stdio commands).
//! Import stays inactive. Anonymous and local activation reuse the shared MCP
//! runtime; OAuth and local env/auth secrets use revision-owned OS vault entries.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use grain_core::mcp_connections::{
    parse_mcp_import_bundle, ConnectionDefinition, ConnectionLease, ConnectionRecord,
    ConnectionRegistry, RegistryError,
};
use grain_sdk::mcp::{McpAuthentication, McpConnection, McpHeaders, McpPublicOAuthClient};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, WebviewWindow};

use super::local;

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
    endpoint_label: String,
}

fn empty_headers() -> &'static McpHeaders {
    static EMPTY: std::sync::OnceLock<McpHeaders> = std::sync::OnceLock::new();
    EMPTY.get_or_init(McpHeaders::new)
}

fn endpoint_label_for(def: &McpConnection) -> String {
    match def {
        McpConnection::Remote { url, .. } | McpConnection::LocalHttp { url, .. } => url.clone(),
        McpConnection::Stdio { command, .. } => format!("stdio:{command}"),
    }
}

impl RuntimeOwner {
    pub(super) fn definition(&self) -> &McpConnection {
        self.lease.record().definition()
    }

    pub(super) fn headers(&self) -> &McpHeaders {
        match self.definition() {
            McpConnection::Remote { headers, .. } | McpConnection::LocalHttp { headers, .. } => {
                headers
            }
            McpConnection::Stdio { .. } => empty_headers(),
        }
    }

    pub(super) fn oauth_client(&self) -> Option<&McpPublicOAuthClient> {
        match self.definition() {
            McpConnection::Remote { oauth_client, .. } => oauth_client.as_ref(),
            _ => None,
        }
    }

    pub(super) fn spec(&self) -> super::Provider<'_> {
        let definition = self.definition();
        let registration = match definition {
            McpConnection::Remote {
                authentication: McpAuthentication::None {},
                ..
            } => super::Registration::Anonymous,
            McpConnection::Remote {
                authentication: McpAuthentication::OAuth {},
                ..
            } => super::Registration::Dynamic,
            McpConnection::LocalHttp { .. } => super::Registration::LocalHttp,
            McpConnection::Stdio { .. } => super::Registration::Stdio,
        };
        let description = match definition {
            McpConnection::Remote { .. } => "Tools from this MCP server.",
            McpConnection::LocalHttp { .. } => "Tools from this local HTTP MCP server.",
            McpConnection::Stdio { .. } => "Tools from this local command MCP server.",
        };
        super::Provider {
            id: &self.provider_id,
            name: definition.name(),
            description,
            endpoint: &self.endpoint_label,
            registration,
            setup_url: &self.endpoint_label,
            bundled_preset: false,
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
    }

    pub(super) fn invalidate(&self, app: &AppHandle) -> Result<super::session::Ticket, String> {
        let state = app.try_state::<State>().ok_or("MCP storage unavailable")?;
        let _operation = state
            .operation
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        if !self.current() {
            return Err(RegistryError::Conflict.to_string());
        }
        Ok(self.control.invalidate())
    }

    pub(super) fn enabled(&self, app: &AppHandle) -> bool {
        let settings = crate::settings::get_settings(app);
        settings.mcp_enabled_providers.contains(&self.enable_key()) && self.current()
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
        let endpoint_label = endpoint_label_for(lease.record().definition());
        let mut controls = self
            .controls
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        if let Some(owner) = controls.get_mut(&id) {
            if owner.lease != lease {
                owner.control.invalidate();
                owner.lease = lease;
                owner.endpoint_label = endpoint_label;
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
            endpoint_label,
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
}

/// Host UI projection, deliberately omitting vault/account IDs, secret values,
/// and numeric u64 revisions (JavaScript numbers cannot represent all revisions).
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    id: String,
    revision: String,
    transport: String,
    name: String,
    url: String,
    authentication: String,
    headers: McpHeaders,
    oauth_client: Option<McpPublicOAuthClient>,
    command: Option<String>,
    args: Vec<String>,
    cwd: Option<String>,
    env_keys: Vec<String>,
    has_auth_header: bool,
    runtime: local::LocalMcpRuntimeStatus,
    state: String,
}

impl From<ConnectionRecord> for ConnectionView {
    fn from(record: ConnectionRecord) -> Self {
        let id = record.identity().connection_id().to_string();
        let provider_id = format!("configured-{id}");
        let runtime = local::supervisor().runtime_status(&provider_id);
        let account = record.identity().vault_account();
        match record.definition() {
            McpConnection::Remote {
                name,
                url,
                authentication,
                headers,
                oauth_client,
            } => Self {
                id,
                revision: record.revision().to_string(),
                transport: "remote".into(),
                name: name.clone(),
                url: url.clone(),
                authentication: match authentication {
                    McpAuthentication::OAuth {} => "oauth",
                    McpAuthentication::None {} => "none",
                }
                .into(),
                headers: headers.clone(),
                oauth_client: oauth_client.clone(),
                command: None,
                args: Vec::new(),
                cwd: None,
                env_keys: Vec::new(),
                has_auth_header: false,
                runtime,
                state: "inactive".into(),
            },
            McpConnection::LocalHttp { name, url, headers } => Self {
                id,
                revision: record.revision().to_string(),
                transport: "local-http".into(),
                name: name.clone(),
                url: url.clone(),
                authentication: "none".into(),
                headers: headers.clone(),
                oauth_client: None,
                command: None,
                args: Vec::new(),
                cwd: None,
                env_keys: Vec::new(),
                has_auth_header: local::has_local_auth_sync(&account),
                runtime,
                state: "inactive".into(),
            },
            McpConnection::Stdio {
                name,
                command,
                args,
                cwd,
                env_keys,
            } => Self {
                id,
                revision: record.revision().to_string(),
                transport: "stdio".into(),
                name: name.clone(),
                url: String::new(),
                authentication: "none".into(),
                headers: McpHeaders::new(),
                oauth_client: None,
                command: Some(command.clone()),
                args: args.clone(),
                cwd: cwd.clone(),
                env_keys: env_keys.clone(),
                has_auth_header: false,
                runtime,
                state: "inactive".into(),
            },
        }
    }
}

fn view(
    app: &AppHandle,
    registry: &ConnectionRegistry,
    record: ConnectionRecord,
) -> ConnectionView {
    let settings = crate::settings::get_settings(app);
    let enabled = settings
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
    match record.definition() {
        McpConnection::Remote {
            authentication: McpAuthentication::None {},
            ..
        } => Ok(()),
        McpConnection::Remote {
            authentication: McpAuthentication::OAuth {},
            ..
        } => retire_with(record, |service, account| {
            super::delete_vault_entry(service, account).map_err(|_| ())
        }),
        McpConnection::LocalHttp { .. } => {
            let account = record.identity().vault_account();
            local::delete_local_auth_sync(&account).map_err(|_| {
                "Could not clear local MCP auth header from vault. Reload and retry cleanup.".into()
            })
        }
        McpConnection::Stdio { .. } => {
            let account = record.identity().vault_account();
            local::delete_local_env_sync(&account).map_err(|_| {
                "Could not clear local MCP environment secrets from vault. Reload and retry cleanup."
                    .into()
            })
        }
    }
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
    guard(&window)?;
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
    guard(&window)?;
    let view_app = app.clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        if !matches!(
            lease.record().definition(),
            McpConnection::Remote {
                authentication: McpAuthentication::OAuth {},
                ..
            }
        ) {
            return Err("This server has no OAuth client to configure.".into());
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
    if !matches!(
        record.definition(),
        McpConnection::Remote {
            authentication: McpAuthentication::OAuth {},
            ..
        }
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
    guard(&window)?;
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
    guard(&window)?;
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
    let runtime = if item.registration == super::Registration::Stdio {
        Some(local::supervisor().runtime_status(item.id))
    } else {
        None
    };
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
        let state = if let Some(rt) = &runtime {
            if rt.running {
                "running"
            } else if rt.last_error.is_some() {
                "error"
            } else {
                "idle"
            }
        } else if item.registration == super::Registration::LocalHttp {
            "local"
        } else if !required {
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
            requires_client_credentials: super::preregistered(&app, &item)
                && super::package_client(&app, &item).is_none(),
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
    guard(&window)?;
    let (owner, _) = expected_owner(&app, &id, &expected_revision)?;
    if !super::requires_account(&owner.spec()) {
        return Err("This server has no OAuth account to disconnect.".into());
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
                .map_err(|error| error.to_string())?;
            super::forget_tool_cache(&app, &owner.spec()).await;
            Ok(())
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

fn guard(window: &WebviewWindow) -> Result<(), String> {
    crate::grain_commands::require_main_window(window)
}

async fn storage<T: Send + 'static>(
    app: AppHandle,
    operation: impl FnOnce(&State, &Arc<ConnectionRegistry>) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(move || {
        let state = app
            .try_state::<State>()
            .ok_or("MCP connection storage is unavailable.")?;
        let _operation = state
            .operation
            .lock()
            .map_err(|_| RegistryError::InvalidState.to_string())?;
        let registry = state.registry(&app)?;
        operation(&state, &registry)
    })
    .await
    .map_err(|_| "MCP connection storage operation failed.".to_string())?
}

fn store_imported_secrets(
    record: &ConnectionRecord,
    env_secrets: &BTreeMap<String, String>,
    auth_header: Option<(String, String)>,
) -> Result<(), String> {
    let account = record.identity().vault_account();
    match record.definition() {
        McpConnection::Stdio { .. } if !env_secrets.is_empty() => {
            local::write_local_env_sync(&account, env_secrets)?;
        }
        McpConnection::LocalHttp { .. } if auth_header.is_some() => {
            local::write_local_auth_sync(&account, auth_header)?;
        }
        _ => {}
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connections_list(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Vec<ConnectionView>, String> {
    guard(&window)?;
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
    guard(&window)?;
    let mut bundle =
        parse_mcp_import_bundle(definition_json.as_bytes()).map_err(|error| error.to_string())?;
    if bundle.len() != 1 {
        return Err(
            "Provide a single MCP server definition, or use bundle import for multiple servers."
                .into(),
        );
    }
    let imported = bundle.remove(0);
    storage(app, move |_, registry| {
        let record = registry
            .insert(imported.definition)
            .map_err(|error| error.to_string())?;
        if let Err(err) =
            store_imported_secrets(&record, &imported.env_secrets, imported.auth_header)
        {
            // Roll back metadata if vault write fails so we never leave half-configured secrets.
            if let Ok(lease) = registry.lease(record.identity().connection_id()) {
                let _ = registry.remove(&lease);
            }
            return Err(err);
        }
        Ok(ConnectionView::from(record))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_import_bundle(
    app: AppHandle,
    window: WebviewWindow,
    definition_json: String,
) -> Result<Vec<ConnectionView>, String> {
    guard(&window)?;
    let bundle =
        parse_mcp_import_bundle(definition_json.as_bytes()).map_err(|error| error.to_string())?;
    storage(app, move |_, registry| {
        let existing_count = registry.list().map_err(|e| e.to_string())?.len();
        if existing_count + bundle.len() > grain_core::mcp_connections::MAX_CONNECTIONS {
            return Err(RegistryError::Limit.to_string());
        }
        let mut added = Vec::with_capacity(bundle.len());
        for imported in bundle {
            let record = registry
                .insert(imported.definition)
                .map_err(|error| error.to_string())?;
            if let Err(err) =
                store_imported_secrets(&record, &imported.env_secrets, imported.auth_header)
            {
                if let Ok(lease) = registry.lease(record.identity().connection_id()) {
                    let _ = registry.remove(&lease);
                }
                return Err(err);
            }
            added.push(ConnectionView::from(record));
        }
        Ok(added)
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
    guard(&window)?;
    let mut bundle =
        parse_mcp_import_bundle(definition_json.as_bytes()).map_err(|error| error.to_string())?;
    if bundle.len() != 1 {
        return Err("Replacement must contain exactly one MCP server definition.".into());
    }
    let imported = bundle.remove(0);
    let definition = imported.definition;
    let view_app = app.clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        if lease.record().definition() != definition.definition()
            && lease.record().revision() == u64::MAX
        {
            return Err(RegistryError::RevisionExhausted.to_string());
        }
        if lease.record().definition() != definition.definition()
            || !imported.env_secrets.is_empty()
            || imported.auth_header.is_some()
        {
            state.invalidate(&id)?;
            local::supervisor().evict_in_background(format!("configured-{id}"));
        }
        let account_changed = grain_core::mcp::account_binding_changed(
            lease.record().definition(),
            definition.definition(),
        );
        // Preserve existing vault env secrets for keys that were not re-supplied
        // when editing a stdio connection (e.g. changing args without retyping API keys).
        let carried_env = if matches!(
            (lease.record().definition(), definition.definition()),
            (McpConnection::Stdio { .. }, McpConnection::Stdio { .. })
        ) {
            let mut prev =
                local::read_local_env_sync(lease.record().identity().vault_account().as_ref())
                    .unwrap_or_default();
            for (k, v) in &imported.env_secrets {
                prev.insert(k.clone(), v.clone());
            }
            if let McpConnection::Stdio { env_keys, .. } = definition.definition() {
                prev.retain(|k, _| env_keys.iter().any(|allowed| allowed == k));
            }
            prev
        } else {
            imported.env_secrets.clone()
        };
        let carried_auth = if imported.auth_header.is_some() {
            imported.auth_header.clone()
        } else if matches!(
            (lease.record().definition(), definition.definition()),
            (
                McpConnection::LocalHttp { .. },
                McpConnection::LocalHttp { .. }
            )
        ) && !account_changed
        {
            local::read_local_auth_sync(lease.record().identity().vault_account().as_ref())
                .ok()
                .flatten()
        } else {
            None
        };

        if account_changed {
            if !registry.is_current(&lease) {
                return Err(RegistryError::Conflict.to_string());
            }
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
        if matches!(record.definition(), McpConnection::Stdio { .. }) {
            local::write_local_env_sync(record.identity().vault_account().as_ref(), &carried_env)?;
        } else if matches!(record.definition(), McpConnection::LocalHttp { .. })
            && (imported.auth_header.is_some() || account_changed)
        {
            local::write_local_auth_sync(record.identity().vault_account().as_ref(), carried_auth)?;
        }
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
pub async fn mcp_connection_set_local_env(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
    env: BTreeMap<String, String>,
) -> Result<ConnectionView, String> {
    guard(&window)?;
    grain_core::mcp::validate_env_map(&env).map_err(|error| error.to_string())?;
    let view_app = app.clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        let McpConnection::Stdio {
            name,
            command,
            args,
            cwd,
            ..
        } = lease.record().definition().clone()
        else {
            return Err(
                "Only local command (stdio) MCP connections support environment variables.".into(),
            );
        };
        if !registry.is_current(&lease) {
            return Err(RegistryError::Conflict.to_string());
        }
        state.invalidate(&id)?;
        local::supervisor().evict_in_background(format!("configured-{id}"));
        let env_keys: Vec<String> = env.keys().cloned().collect();
        let next_def = ConnectionDefinition::validate(McpConnection::Stdio {
            name,
            command,
            args,
            cwd,
            env_keys,
        })
        .map_err(|error| error.to_string())?;
        let account_changed = grain_core::mcp::account_binding_changed(
            lease.record().definition(),
            next_def.definition(),
        );
        if account_changed {
            prune_enabled(&view_app, &id, None)?;
            retire_account(lease.record())?;
        }
        let _ = registry
            .replace(&lease, next_def)
            .map_err(|error| error.to_string())?;
        let record = registry
            .lease(&id)
            .map_err(|error| error.to_string())?
            .record()
            .clone();
        local::write_local_env_sync(record.identity().vault_account().as_ref(), &env)?;
        Ok(view(&view_app, registry, record))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_set_local_auth(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
    header_name: String,
    header_value: String,
) -> Result<ConnectionView, String> {
    guard(&window)?;
    let auth = if header_name.trim().is_empty() && header_value.trim().is_empty() {
        None
    } else {
        Some(
            grain_core::mcp::validate_local_auth_header(&header_name, &header_value)
                .map_err(|error| error.to_string())?,
        )
    };
    let view_app = app.clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        if !matches!(lease.record().definition(), McpConnection::LocalHttp { .. }) {
            return Err("Only local HTTP MCP connections support a local auth header.".into());
        }
        if !registry.is_current(&lease) {
            return Err(RegistryError::Conflict.to_string());
        }
        state.invalidate(&id)?;
        local::write_local_auth_sync(lease.record().identity().vault_account().as_ref(), auth)?;
        Ok(view(&view_app, registry, lease.record().clone()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_stop(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
) -> Result<(), String> {
    guard(&window)?;
    let (owner, _) = expected_owner(&app, &id, &expected_revision)?;
    let provider_id = owner.provider_id.clone();
    let _ = owner.invalidate(&app);
    local::supervisor().evict(&provider_id).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_logs(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
) -> Result<local::LocalMcpLogs, String> {
    guard(&window)?;
    let (owner, _) = expected_owner(&app, &id, &expected_revision)?;
    Ok(local::supervisor().logs(&owner.provider_id))
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_test(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
) -> Result<super::McpDiscoveryResult, String> {
    guard(&window)?;
    let (owner, _) = expected_owner(&app, &id, &expected_revision)?;
    let tool_set = super::list_tools_for_test(&app, &owner.provider_id).await?;
    Ok(super::McpDiscoveryResult {
        provider_id: tool_set.provider_id,
        provider_name: tool_set.provider_name,
        tool_count: tool_set.tools.len() as u32,
        tools: tool_set
            .tools
            .iter()
            .map(|tool| tool.name.to_string())
            .collect(),
        tool_set_digest: tool_set.digest,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_connection_remove(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: String,
) -> Result<(), String> {
    guard(&window)?;
    let view_app = app.clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        if !registry.is_current(&lease) {
            return Err(RegistryError::Conflict.to_string());
        }
        state.invalidate(&id)?;
        let provider_id = format!("configured-{id}");
        local::supervisor().evict_in_background(provider_id.clone());
        local::supervisor().clear_diagnostics(&provider_id);
        prune_enabled(&view_app, &id, None)?;
        super::tool_cache::clear_blocking(
            &view_app,
            lease.record().identity().vault_account().as_ref(),
        );
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
    guard(&window)?;
    let context = app
        .try_state::<Arc<grain_core::AppContext>>()
        .ok_or("Application context is unavailable.")?
        .inner()
        .clone();
    storage(app, move |state, registry| {
        let lease = expected_lease(registry, &id, &expected_revision)?;
        let owner = state.owner(registry.clone(), lease)?;
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
        if !enabled {
            local::supervisor().evict_in_background(owner.provider_id.clone());
        }
        context
            .update_settings(|settings| {
                let key = owner.enable_key();
                settings.mcp_enabled_providers.retain(|value| value != &key);
                if enabled {
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
    records
        .into_iter()
        .filter(|record| {
            settings
                .mcp_enabled_providers
                .iter()
                .any(|key| key == record.identity().vault_account().as_ref())
        })
        .map(|record| {
            let description = match record.definition() {
                McpConnection::Remote { .. } => "Tools from this MCP server.",
                McpConnection::LocalHttp { .. } => "Tools from this local HTTP MCP server.",
                McpConnection::Stdio { .. } => "Tools from this local command MCP server.",
            };
            grain_core::capability_index::ExtensionDirectoryEntry {
                extension_id: format!("mcp.configured-{}", record.identity().connection_id()),
                name: record.definition().name().to_owned(),
                description: description.into(),
                action_count: 1,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_metadata_change_denies_vault_io_without_a_generation_notification() {
        let data = tempfile::tempdir().unwrap();
        let registry = Arc::new(ConnectionRegistry::load(data.path()).unwrap());
        let input = ConnectionDefinition::parse(br#"{"transport":"remote","name":"Account","url":"https://server.example.com/mcp","authentication":{"type":"oauth"}}"#).unwrap();
        let record = registry.insert(input).unwrap();
        let lease = registry.lease(record.identity().connection_id()).unwrap();
        let state = State::default();
        let owner = state.owner(registry.clone(), lease.clone()).unwrap();
        let ticket = owner.ticket();
        let store = super::super::VaultCredentialStore {
            account: owner.account(),
            ticket: ticket.clone(),
            _operation: None,
            current: Some(owner),
        };
        let changed = ConnectionDefinition::parse(br#"{"transport":"remote","name":"Changed","url":"https://other.example.com/mcp","authentication":{"type":"oauth"}}"#).unwrap();
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
        let record = registry.insert(ConnectionDefinition::parse(br#"{"transport":"remote","name":"Account","url":"https://server.example.com/mcp","authentication":{"type":"oauth"}}"#).unwrap()).unwrap();
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
        let anonymous = registry.insert(ConnectionDefinition::parse(br#"{"transport":"remote","name":"Anonymous","url":"https://server.example.com/mcp","authentication":{"type":"none"}}"#).unwrap()).unwrap();
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
        let definition = ConnectionDefinition::parse(br#"{"transport":"remote","name":"Server","url":"https://server.example.com/mcp","authentication":{"type":"none"}}"#).unwrap();
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
        let renamed = ConnectionDefinition::parse(br#"{"transport":"remote","name":"Renamed","url":"https://server.example.com/mcp","authentication":{"type":"none"}}"#).unwrap();
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
        let definition = ConnectionDefinition::parse(br#"{"transport":"remote","name":"Server","url":"https://server.example.com/mcp","authentication":{"type":"none"}}"#).unwrap();
        let record = registry.insert(definition).unwrap();
        let state = State::default();
        let lease = registry.lease(record.identity().connection_id()).unwrap();
        let old = state.owner(registry.clone(), lease.clone()).unwrap();
        let replacement = ConnectionDefinition::parse(br#"{"transport":"remote","name":"Server","url":"https://other.example.com/mcp","authentication":{"type":"oauth"}}"#).unwrap();
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
