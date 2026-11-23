//! Bounded custom MCP metadata persistence. No transport, credential, consent,
//! worker or enabled state is created here. Host acquisition must still approve
//! network/account/command access before connecting. One short-lived OS lock per save.

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use grain_sdk::mcp::{McpAuthentication, McpConnection, McpHeaders, MCP_CONNECTION_MAX_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::mcp::{
    account_binding_changed, canonical_endpoint, canonical_loopback_endpoint, display_text,
    is_secret_header_name, validate_env_map, validate_headers, validate_local_auth_header,
    validate_oauth_client, validate_stdio, ConnectionIdentity, ConnectionSource, ContractError,
};

const FILE_NAME: &str = "mcp-custom-connections.json";
const LOCK_NAME: &str = ".mcp-custom-connections.lock";
const DISK_SCHEMA: u8 = 2;
pub const MAX_CONNECTIONS: usize = 32;
pub const MAX_REGISTRY_BYTES: usize = 256 * 1024;
pub const MAX_IMPORT_BUNDLE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionDefinition(McpConnection);

impl ConnectionDefinition {
    pub fn parse(bytes: &[u8]) -> Result<Self, RegistryError> {
        if bytes.len() > MCP_CONNECTION_MAX_BYTES {
            return Err(RegistryError::Limit);
        }
        let input = serde_json::from_slice(bytes)
            .map_err(|_| RegistryError::InvalidInput(ContractError::InvalidJson))?;
        Self::validate(input)
    }

    pub fn validate(mut input: McpConnection) -> Result<Self, RegistryError> {
        match &mut input {
            McpConnection::Remote {
                name,
                url,
                authentication,
                headers,
                oauth_client,
            } => {
                if !display_text(name, 120) {
                    return Err(RegistryError::InvalidInput(ContractError::InvalidMetadata));
                }
                *url = canonical_endpoint(url).map_err(RegistryError::InvalidInput)?;
                validate_headers(headers).map_err(RegistryError::InvalidInput)?;
                validate_oauth_client(*authentication, oauth_client.as_ref())
                    .map_err(RegistryError::InvalidInput)?;
            }
            McpConnection::LocalHttp { name, url, headers } => {
                if !display_text(name, 120) {
                    return Err(RegistryError::InvalidInput(ContractError::InvalidMetadata));
                }
                *url = canonical_loopback_endpoint(url).map_err(RegistryError::InvalidInput)?;
                validate_headers(headers).map_err(RegistryError::InvalidInput)?;
            }
            McpConnection::Stdio {
                name,
                command,
                args,
                cwd,
                env_keys,
            } => {
                if !display_text(name, 120) {
                    return Err(RegistryError::InvalidInput(ContractError::InvalidMetadata));
                }
                validate_stdio(command, args, cwd.as_deref(), env_keys)
                    .map_err(RegistryError::InvalidInput)?;
            }
        }
        if serde_json::to_vec(&input)
            .map_err(|_| RegistryError::InvalidState)?
            .len()
            > MCP_CONNECTION_MAX_BYTES
        {
            return Err(RegistryError::Limit);
        }
        Ok(Self(input))
    }

    pub fn definition(&self) -> &McpConnection {
        &self.0
    }
}

/// One validated connection parsed from user-supplied JSON (either Grain's direct
/// format or a standard Claude Desktop / Cursor `mcpServers` block). Secrets are
/// separated at parse time so they are stored only in the OS credential vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedConnection {
    pub definition: ConnectionDefinition,
    pub env_secrets: BTreeMap<String, String>,
    pub auth_header: Option<(String, String)>,
}

/// Parse either:
/// - A standard `{"mcpServers": { "<name>": { ... } }}` or `{"servers": { ... }}` bundle,
/// - A single Grain `McpConnection` object (optionally carrying transient `env` or `authHeader`), or
/// - A single shorthand remote/local object (`{ name, url, ... }` or `{ name, command, ... }`).
pub fn parse_mcp_import_bundle(bytes: &[u8]) -> Result<Vec<ImportedConnection>, RegistryError> {
    if bytes.len() > MAX_IMPORT_BUNDLE_BYTES {
        return Err(RegistryError::Limit);
    }
    let root: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| RegistryError::InvalidInput(ContractError::InvalidJson))?;
    let Some(obj) = root.as_object() else {
        return Err(RegistryError::InvalidInput(ContractError::InvalidJson));
    };

    if let Some(servers_val) = obj.get("mcpServers").or_else(|| obj.get("servers")) {
        // Reject extraneous unexpected keys besides the known wrapper key.
        let allowed_key = if obj.contains_key("mcpServers") {
            "mcpServers"
        } else {
            "servers"
        };
        if obj.len() != 1 || !obj.contains_key(allowed_key) {
            return Err(RegistryError::InvalidInput(ContractError::InvalidJson));
        }
        let Some(servers) = servers_val.as_object() else {
            return Err(RegistryError::InvalidInput(ContractError::InvalidJson));
        };
        if servers.is_empty() {
            return Err(RegistryError::InvalidInput(ContractError::InvalidJson));
        }
        if servers.len() > MAX_CONNECTIONS {
            return Err(RegistryError::Limit);
        }
        let mut out = Vec::with_capacity(servers.len());
        for (raw_name, entry) in servers {
            let name = raw_name.trim();
            out.push(parse_server_entry(name, entry)?);
        }
        return Ok(out);
    }

    // Single direct object.
    let single = parse_single_object(obj)?;
    Ok(vec![single])
}

fn parse_single_object(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<ImportedConnection, RegistryError> {
    // Check if it carries transient `env` (for stdio) or `authHeader` (for local-http)
    // alongside Grain's direct wire schema.
    if let Some(transport) = obj.get("transport").and_then(|v| v.as_str()) {
        let mut cleaned = obj.clone();
        let mut env_secrets = BTreeMap::new();
        let mut auth_header = None;

        if transport == "stdio" {
            if let Some(env_val) = cleaned.remove("env") {
                env_secrets = parse_env_value(&env_val)?;
                // Merge keys into `envKeys` if not already present.
                let mut keys: Vec<String> = cleaned
                    .get("envKeys")
                    .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                    .unwrap_or_default();
                for k in env_secrets.keys() {
                    if !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
                cleaned.insert("envKeys".into(), serde_json::json!(keys));
            }
        } else if transport == "local-http" {
            if let Some(auth_val) = cleaned.remove("authHeader") {
                auth_header = Some(parse_auth_header_object(&auth_val)?);
            }
        }

        let raw = serde_json::to_vec(&serde_json::Value::Object(cleaned))
            .map_err(|_| RegistryError::InvalidInput(ContractError::InvalidJson))?;
        let definition = ConnectionDefinition::parse(&raw)?;
        return Ok(ImportedConnection {
            definition,
            env_secrets,
            auth_header,
        });
    }

    // Shorthand single object with `name`.
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .ok_or(RegistryError::InvalidInput(ContractError::InvalidJson))?;
    parse_server_entry(name, &serde_json::Value::Object(obj.clone()))
}

fn parse_server_entry(
    name: &str,
    entry: &serde_json::Value,
) -> Result<ImportedConnection, RegistryError> {
    let Some(map) = entry.as_object() else {
        return Err(RegistryError::InvalidInput(ContractError::InvalidJson));
    };
    if !display_text(name, 120) {
        return Err(RegistryError::InvalidInput(ContractError::InvalidMetadata));
    }

    let explicit_type = map.get("type").and_then(|v| v.as_str());
    let has_command = map.contains_key("command");
    let has_url = map.contains_key("url") || map.contains_key("serverUrl");

    if has_command && has_url {
        return Err(RegistryError::InvalidInput(ContractError::InvalidJson));
    }

    if has_command || matches!(explicit_type, Some("stdio")) {
        let command = map
            .get("command")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .ok_or(RegistryError::InvalidInput(ContractError::InvalidStdio))?
            .to_owned();
        let args = match map.get("args") {
            None | Some(serde_json::Value::Null) => Vec::new(),
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or(RegistryError::InvalidInput(ContractError::InvalidStdio))
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(RegistryError::InvalidInput(ContractError::InvalidStdio)),
        };
        let cwd = match map.get("cwd") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(s)) if s.trim().is_empty() => None,
            Some(serde_json::Value::String(s)) => Some(s.trim().to_owned()),
            Some(_) => return Err(RegistryError::InvalidInput(ContractError::InvalidStdio)),
        };
        let env_secrets = match map.get("env") {
            None | Some(serde_json::Value::Null) => BTreeMap::new(),
            Some(v) => parse_env_value(v)?,
        };
        let env_keys: Vec<String> = env_secrets.keys().cloned().collect();
        let definition = ConnectionDefinition::validate(McpConnection::Stdio {
            name: name.to_owned(),
            command,
            args,
            cwd,
            env_keys,
        })?;
        return Ok(ImportedConnection {
            definition,
            env_secrets,
            auth_header: None,
        });
    }

    if has_url {
        let url_str = map
            .get("url")
            .or_else(|| map.get("serverUrl"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .ok_or(RegistryError::InvalidInput(ContractError::InvalidEndpoint))?;

        let raw_headers = match map.get("headers") {
            None | Some(serde_json::Value::Null) => BTreeMap::new(),
            Some(serde_json::Value::Object(hmap)) => {
                let mut parsed = BTreeMap::new();
                for (k, v) in hmap {
                    let Some(val) = v.as_str() else {
                        return Err(RegistryError::InvalidInput(ContractError::InvalidHeaders));
                    };
                    let lower = k.trim().to_ascii_lowercase();
                    if parsed.insert(lower, val.trim().to_owned()).is_some() {
                        return Err(RegistryError::InvalidInput(ContractError::InvalidHeaders));
                    }
                }
                parsed
            }
            Some(_) => return Err(RegistryError::InvalidInput(ContractError::InvalidHeaders)),
        };

        // Route loopback URLs to LocalHttp, public HTTPS URLs to Remote.
        if canonical_loopback_endpoint(url_str).is_ok() {
            let mut public_headers = McpHeaders::new();
            let mut auth_header = None;
            for (k, v) in raw_headers {
                if is_secret_header_name(&k) {
                    if auth_header.is_some() {
                        return Err(RegistryError::InvalidInput(ContractError::InvalidHeaders));
                    }
                    auth_header = Some(
                        validate_local_auth_header(&k, &v).map_err(RegistryError::InvalidInput)?,
                    );
                } else {
                    public_headers.insert(k, v);
                }
            }
            if let Some(auth_val) = map.get("authHeader") {
                if auth_header.is_some() {
                    return Err(RegistryError::InvalidInput(ContractError::InvalidHeaders));
                }
                auth_header = Some(parse_auth_header_object(auth_val)?);
            }
            let definition = ConnectionDefinition::validate(McpConnection::LocalHttp {
                name: name.to_owned(),
                url: url_str.to_owned(),
                headers: public_headers,
            })?;
            return Ok(ImportedConnection {
                definition,
                env_secrets: BTreeMap::new(),
                auth_header,
            });
        }

        // Public remote HTTPS server.
        let authentication = match map.get("authentication") {
            None | Some(serde_json::Value::Null) => McpAuthentication::OAuth {},
            Some(v) => serde_json::from_value::<McpAuthentication>(v.clone())
                .map_err(|_| RegistryError::InvalidInput(ContractError::InvalidJson))?,
        };
        let oauth_client = match map.get("oauthClient") {
            None | Some(serde_json::Value::Null) => None,
            Some(v) => Some(
                serde_json::from_value(v.clone())
                    .map_err(|_| RegistryError::InvalidInput(ContractError::InvalidJson))?,
            ),
        };
        let definition = ConnectionDefinition::validate(McpConnection::Remote {
            name: name.to_owned(),
            url: url_str.to_owned(),
            authentication,
            headers: raw_headers,
            oauth_client,
        })?;
        return Ok(ImportedConnection {
            definition,
            env_secrets: BTreeMap::new(),
            auth_header: None,
        });
    }

    Err(RegistryError::InvalidInput(ContractError::InvalidJson))
}

fn parse_env_value(value: &serde_json::Value) -> Result<BTreeMap<String, String>, RegistryError> {
    let Some(map) = value.as_object() else {
        return Err(RegistryError::InvalidInput(ContractError::InvalidStdio));
    };
    let mut env = BTreeMap::new();
    for (k, v) in map {
        let Some(val) = v.as_str() else {
            return Err(RegistryError::InvalidInput(ContractError::InvalidStdio));
        };
        env.insert(k.clone(), val.to_owned());
    }
    validate_env_map(&env).map_err(RegistryError::InvalidInput)?;
    Ok(env)
}

fn parse_auth_header_object(value: &serde_json::Value) -> Result<(String, String), RegistryError> {
    let Some(map) = value.as_object() else {
        return Err(RegistryError::InvalidInput(ContractError::InvalidHeaders));
    };
    let name = map
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or(RegistryError::InvalidInput(ContractError::InvalidHeaders))?;
    let val = map
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or(RegistryError::InvalidInput(ContractError::InvalidHeaders))?;
    validate_local_auth_header(name, val).map_err(RegistryError::InvalidInput)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryError {
    InvalidInput(ContractError),
    InvalidState,
    Filesystem,
    Conflict,
    NotFound,
    Limit,
    RevisionExhausted,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(error) => error.fmt(out),
            Self::InvalidState => {
                out.write_str("MCP connection registry is invalid; its bytes are preserved.")
            }
            Self::Filesystem => {
                out.write_str("MCP connection registry could not be read or saved.")
            }
            Self::Conflict => {
                out.write_str("MCP connection changed; reload it before trying again.")
            }
            Self::NotFound => out.write_str("MCP connection is no longer available."),
            Self::Limit => out.write_str("MCP connection metadata exceeds its supported limit."),
            Self::RevisionExhausted => out.write_str("MCP connection revision is exhausted."),
        }
    }
}
impl std::error::Error for RegistryError {}

/// Read-only host record. The account is an opaque vault namespace, not a
/// credential or proof of consent. Caller-supplied JSON cannot construct it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionRecord {
    identity: ConnectionIdentity,
    revision: u64,
    definition: McpConnection,
}

impl ConnectionRecord {
    pub fn identity(&self) -> &ConnectionIdentity {
        &self.identity
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn definition(&self) -> &McpConnection {
        &self.definition
    }
}

/// In-process optimistic ownership. Not serializable or accepted from JSON.
/// A registry reload invalidates leases even when disk revisions are unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionLease {
    epoch: Uuid,
    record: ConnectionRecord,
}
impl ConnectionLease {
    pub fn record(&self) -> &ConnectionRecord {
        &self.record
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiskRecord {
    connection_id: String,
    account_id: String,
    revision: u64,
    definition: McpConnection,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskFile {
    schema: u8,
    connections: Vec<DiskRecord>,
}
struct State {
    records: BTreeMap<String, ConnectionRecord>,
    disk_digest: Option<[u8; 32]>,
}

pub struct ConnectionRegistry {
    path: PathBuf,
    epoch: Uuid,
    state: Mutex<State>,
}

impl ConnectionRegistry {
    /// Reads a fixed filename beneath a trusted app/profile data directory.
    /// A missing file is empty; it is not created until a successful mutation.
    pub fn load(data_dir: &Path) -> Result<Self, RegistryError> {
        let path = data_dir.join(FILE_NAME);
        let bytes = read_bounded(&path)?;
        let mut records = BTreeMap::new();
        if let Some(bytes) = &bytes {
            let disk: DiskFile =
                serde_json::from_slice(bytes).map_err(|_| RegistryError::InvalidState)?;
            let valid_empty_v1 = disk.schema == 1 && disk.connections.is_empty();
            if (!valid_empty_v1 && disk.schema != DISK_SCHEMA)
                || disk.connections.len() > MAX_CONNECTIONS
            {
                return Err(RegistryError::InvalidState);
            }
            let mut accounts = HashSet::new();
            for row in disk.connections {
                if !opaque_id(&row.connection_id)
                    || !opaque_id(&row.account_id)
                    || row.revision == 0
                    || !accounts.insert(row.account_id.clone())
                {
                    return Err(RegistryError::InvalidState);
                }
                let identity = ConnectionIdentity::new(
                    &row.connection_id,
                    &row.account_id,
                    ConnectionSource::Configured,
                )
                .map_err(|_| RegistryError::InvalidState)?;
                let definition = ConnectionDefinition::validate(row.definition)
                    .map_err(|_| RegistryError::InvalidState)?;
                if records
                    .insert(
                        row.connection_id,
                        ConnectionRecord {
                            identity,
                            revision: row.revision,
                            definition: definition.0,
                        },
                    )
                    .is_some()
                {
                    return Err(RegistryError::InvalidState);
                }
            }
        }
        Ok(Self {
            path,
            epoch: Uuid::new_v4(),
            state: Mutex::new(State {
                records,
                disk_digest: digest(bytes.as_deref()),
            }),
        })
    }

    pub fn list(&self) -> Result<Vec<ConnectionRecord>, RegistryError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| RegistryError::InvalidState)?
            .records
            .values()
            .cloned()
            .collect())
    }

    pub fn lease(&self, id: &str) -> Result<ConnectionLease, RegistryError> {
        let state = self.state.lock().map_err(|_| RegistryError::InvalidState)?;
        let record = state
            .records
            .get(id)
            .ok_or(RegistryError::NotFound)?
            .clone();
        Ok(ConnectionLease {
            epoch: self.epoch,
            record,
        })
    }

    pub fn is_current(&self, lease: &ConnectionLease) -> bool {
        lease.epoch == self.epoch
            && self.state.lock().is_ok_and(|state| {
                state.records.get(lease.record.identity.connection_id()) == Some(&lease.record)
                    && read_bounded(&self.path)
                        .is_ok_and(|bytes| digest(bytes.as_deref()) == state.disk_digest)
            })
    }

    pub fn insert(
        &self,
        definition: ConnectionDefinition,
    ) -> Result<ConnectionRecord, RegistryError> {
        self.mutate(|records| {
            if records.len() >= MAX_CONNECTIONS {
                return Err(RegistryError::Limit);
            }
            let id = new_id();
            let account = new_id();
            if records.contains_key(&id)
                || records
                    .values()
                    .any(|row| row.identity.account_id() == account)
            {
                return Err(RegistryError::InvalidState);
            }
            let identity = ConnectionIdentity::new(&id, &account, ConnectionSource::Configured)
                .map_err(|_| RegistryError::InvalidState)?;
            let record = ConnectionRecord {
                identity,
                revision: 1,
                definition: definition.0,
            };
            records.insert(id, record.clone());
            Ok(record)
        })
    }

    /// Returns an account-retired record for explicit host vault cleanup when
    /// destination/authentication/command changes. Label edits preserve the binding;
    /// identical saves preserve the complete record.
    pub fn replace(
        &self,
        lease: &ConnectionLease,
        definition: ConnectionDefinition,
    ) -> Result<Option<ConnectionRecord>, RegistryError> {
        self.mutate(|records| {
            self.check_lease(records, lease)?;
            let id = lease.record.identity.connection_id();
            let old = records.get(id).unwrap();
            if old.definition == definition.0 {
                return Ok(None);
            }
            let revision = old
                .revision
                .checked_add(1)
                .ok_or(RegistryError::RevisionExhausted)?;
            let account_changed = account_binding_changed(&old.definition, &definition.0);
            let account = if account_changed {
                new_id()
            } else {
                old.identity.account_id().into()
            };
            if account_changed
                && records
                    .values()
                    .any(|row| row.identity.account_id() == account)
            {
                return Err(RegistryError::InvalidState);
            }
            let identity = ConnectionIdentity::new(id, &account, ConnectionSource::Configured)
                .map_err(|_| RegistryError::InvalidState)?;
            let retired = records
                .insert(
                    id.into(),
                    ConnectionRecord {
                        identity,
                        revision,
                        definition: definition.0,
                    },
                )
                .unwrap();
            Ok(account_changed.then_some(retired))
        })
    }

    pub fn remove(&self, lease: &ConnectionLease) -> Result<ConnectionRecord, RegistryError> {
        self.mutate(|records| {
            self.check_lease(records, lease)?;
            Ok(records
                .remove(lease.record.identity.connection_id())
                .unwrap())
        })
    }

    fn check_lease(
        &self,
        records: &BTreeMap<String, ConnectionRecord>,
        lease: &ConnectionLease,
    ) -> Result<(), RegistryError> {
        if lease.epoch != self.epoch
            || records.get(lease.record.identity.connection_id()) != Some(&lease.record)
        {
            return Err(RegistryError::Conflict);
        }
        Ok(())
    }

    fn mutate<T>(
        &self,
        edit: impl FnOnce(&mut BTreeMap<String, ConnectionRecord>) -> Result<T, RegistryError>,
    ) -> Result<T, RegistryError> {
        let mut state = self.state.lock().map_err(|_| RegistryError::InvalidState)?;
        let mut candidate = state.records.clone();
        let result = edit(&mut candidate)?;
        let disk = DiskFile {
            schema: DISK_SCHEMA,
            connections: candidate
                .values()
                .map(|row| DiskRecord {
                    connection_id: row.identity.connection_id().into(),
                    account_id: row.identity.account_id().into(),
                    revision: row.revision,
                    definition: row.definition.clone(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&disk).map_err(|_| RegistryError::InvalidState)?;
        if bytes.len() > MAX_REGISTRY_BYTES {
            return Err(RegistryError::Limit);
        }
        let lock_path = self.path.with_file_name(LOCK_NAME);
        refuse_non_file(&lock_path)?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|_| RegistryError::Filesystem)?;
        // Nonblocking: no idle lock handle, polling thread or waiting writer.
        lock.try_lock().map_err(|_| RegistryError::Conflict)?;
        if digest(read_bounded(&self.path)?.as_deref()) != state.disk_digest {
            return Err(RegistryError::Conflict);
        }
        if candidate == state.records {
            return Ok(result);
        }
        crate::atomic_file::atomic_write(&self.path, &bytes)
            .map_err(|_| RegistryError::Filesystem)?;
        state.disk_digest = digest(Some(&bytes));
        state.records = candidate;
        Ok(result)
    }
}

fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}
fn opaque_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|ch| ch.is_ascii_digit() || (b'a'..=b'f').contains(&ch))
        && Uuid::parse_str(id).is_ok_and(|id| id.get_version() == Some(uuid::Version::Random))
}
fn digest(bytes: Option<&[u8]>) -> Option<[u8; 32]> {
    bytes.map(|bytes| Sha256::digest(bytes).into())
}
fn refuse_non_file(path: &Path) -> Result<(), RegistryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => Err(RegistryError::InvalidState),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(RegistryError::Filesystem),
    }
}
fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>, RegistryError> {
    refuse_non_file(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(RegistryError::Filesystem),
    };
    if file
        .metadata()
        .map_err(|_| RegistryError::Filesystem)?
        .len()
        > MAX_REGISTRY_BYTES as u64
    {
        return Err(RegistryError::Limit);
    }
    let mut bytes = Vec::new();
    file.take(MAX_REGISTRY_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RegistryError::Filesystem)?;
    if bytes.len() > MAX_REGISTRY_BYTES {
        return Err(RegistryError::Limit);
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_sdk::mcp::McpAuthentication;
    use serde_json::{json, Value};

    fn wire() -> Value {
        json!({"transport":"remote","name":"Test calendar","url":"https://mcp.example.com/mcp",
            "authentication":{"type":"oauth"}})
    }
    fn definition(value: &Value) -> ConnectionDefinition {
        ConnectionDefinition::parse(&serde_json::to_vec(value).unwrap()).unwrap()
    }
    fn bytes(dir: &Path) -> Vec<u8> {
        fs::read(dir.join(FILE_NAME)).unwrap()
    }

    #[test]
    fn direct_json_is_strict_nonsecret_and_not_an_extension_package() {
        let admitted = definition(&wire());
        assert!(matches!(
            admitted.definition(),
            McpConnection::Remote {
                authentication: McpAuthentication::OAuth {},
                ..
            }
        ));
        for field in [
            "connectionId",
            "accountId",
            "source",
            "enabled",
            "headers",
            "clientSecret",
            "token",
            "environment",
            "command",
            "trust",
            "revision",
        ] {
            let mut value = wire();
            value[field] = json!("untrusted");
            assert!(
                ConnectionDefinition::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{field}"
            );
        }
        let mut value = wire();
        value["authentication"]["token"] = json!("secret");
        assert!(ConnectionDefinition::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        assert!(ConnectionDefinition::parse(br#"{"transport":"remote","name":"a","name":"b","url":"https://mcp.example.com","authentication":{"type":"none"}}"#).is_err());
        assert!(ConnectionDefinition::parse(
            br#"{"transport":"stdio","name":"a","name":"b","command":"browse"}"#
        )
        .is_err());
        assert!(ConnectionDefinition::parse(&vec![b' '; MCP_CONNECTION_MAX_BYTES + 1]).is_err());
    }

    #[test]
    fn connection_input_reuses_endpoint_and_display_admission() {
        for url in [
            "http://mcp.example.com",
            "https://localhost",
            "https://127.0.0.1",
            "https://a.local",
            "https://user:password@mcp.example.com",
            "https://mcp.example.com?token=s",
            "https://mcp.example.com#s",
            "https://[::1]",
            "https://mcp.example.com:0",
            "https:///mcp.example.com",
        ] {
            let mut value = wire();
            value["url"] = json!(url);
            assert!(
                ConnectionDefinition::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{url}"
            );
        }
        for name in ["", " name", "name\n", "hidden\u{202e}"] {
            let mut value = wire();
            value["name"] = json!(name);
            assert!(ConnectionDefinition::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        let mut value = wire();
        value["url"] = json!("https://MCP.EXAMPLE.COM:443/");
        value["authentication"] = json!({"type":"none"});
        let McpConnection::Remote { url, .. } = definition(&value).definition().clone() else {
            panic!("expected remote");
        };
        assert_eq!(url, "https://mcp.example.com");
    }

    #[test]
    fn import_bundle_parses_mcp_servers_and_separates_secrets() {
        let bundle = json!({
            "mcpServers": {
                "browse": {
                    "command": "browse",
                    "args": ["mcp"]
                },
                "brave": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-brave-search"],
                    "env": { "BRAVE_API_KEY": "BSA-secret-123" }
                },
                "local-pw": {
                    "url": "http://127.0.0.1:8931/mcp",
                    "headers": {
                        "Authorization": "Bearer local-tok",
                        "X-Custom-Opt": "yes"
                    }
                },
                "remote-docs": {
                    "url": "https://mcp.example.com/mcp"
                }
            }
        });
        let imported = parse_mcp_import_bundle(&serde_json::to_vec(&bundle).unwrap()).unwrap();
        assert_eq!(imported.len(), 4);

        let brave = imported
            .iter()
            .find(|i| i.definition.definition().name() == "brave")
            .unwrap();
        assert_eq!(
            brave.env_secrets.get("BRAVE_API_KEY").map(String::as_str),
            Some("BSA-secret-123")
        );
        let serialized_brave = serde_json::to_string(brave.definition.definition()).unwrap();
        assert!(!serialized_brave.contains("BSA-secret-123"));
        assert!(serialized_brave.contains("BRAVE_API_KEY"));

        let local_pw = imported
            .iter()
            .find(|i| i.definition.definition().name() == "local-pw")
            .unwrap();
        assert_eq!(
            local_pw.auth_header,
            Some(("authorization".into(), "Bearer local-tok".into()))
        );
        let McpConnection::LocalHttp { headers, .. } = local_pw.definition.definition() else {
            panic!("expected local-http");
        };
        assert_eq!(headers.get("x-custom-opt").map(String::as_str), Some("yes"));
        assert!(!headers.contains_key("authorization"));

        // Remote server with Authorization header in JSON must still be rejected.
        let bad_remote = json!({
            "mcpServers": {
                "bad": {
                    "url": "https://mcp.example.com/mcp",
                    "headers": { "Authorization": "Bearer leaked" }
                }
            }
        });
        assert!(parse_mcp_import_bundle(&serde_json::to_vec(&bad_remote).unwrap()).is_err());
    }

    #[test]
    fn stdio_and_local_http_persist_and_rotate_accounts_on_binding_change() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let stdio = definition(&json!({
            "transport": "stdio",
            "name": "Browse",
            "command": "browse",
            "args": ["mcp"]
        }));
        let record = registry.insert(stdio).unwrap();
        let lease = registry.lease(record.identity.connection_id()).unwrap();

        // Rename preserves account binding.
        let renamed = definition(&json!({
            "transport": "stdio",
            "name": "Browser Tool",
            "command": "browse",
            "args": ["mcp"]
        }));
        assert_eq!(registry.replace(&lease, renamed).unwrap(), None);
        let lease = registry.lease(record.identity.connection_id()).unwrap();
        assert_eq!(lease.record.identity, record.identity);

        // Changing args rotates account binding.
        let changed = definition(&json!({
            "transport": "stdio",
            "name": "Browser Tool",
            "command": "browse",
            "args": ["mcp", "--headless"]
        }));
        let retired = registry.replace(&lease, changed).unwrap().unwrap();
        let next = registry.lease(record.identity.connection_id()).unwrap();
        assert_ne!(
            retired.identity.account_id(),
            next.record.identity.account_id()
        );

        // Switching transport rotates account binding.
        let local_http = definition(&json!({
            "transport": "local-http",
            "name": "Browser Tool",
            "url": "http://127.0.0.1:8931/mcp"
        }));
        let retired2 = registry.replace(&next, local_http).unwrap().unwrap();
        let reloaded = ConnectionRegistry::load(dir.path()).unwrap();
        assert_eq!(reloaded.list().unwrap().len(), 1);
        assert_ne!(
            retired2.identity.account_id(),
            reloaded.list().unwrap()[0].identity.account_id()
        );
    }

    #[test]
    fn empty_load_has_no_files_handles_or_implicit_enablement() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        assert!(registry.list().unwrap().is_empty());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn retired_store_file_does_not_block_a_fresh_custom_registry() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("mcp-connections.json");
        let old = br#"{"schema":1,"connections":[{"storeArtifact":{}}]}"#;
        fs::write(&old_path, old).unwrap();

        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        assert!(registry.list().unwrap().is_empty());
        let added = registry.insert(definition(&wire())).unwrap();
        assert_eq!(
            ConnectionRegistry::load(dir.path())
                .unwrap()
                .list()
                .unwrap(),
            vec![added]
        );
        assert_eq!(fs::read(old_path).unwrap(), old);
    }

    #[test]
    fn identical_definitions_have_isolated_accounts_and_restart_invalidates_leases() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let a = registry.insert(definition(&wire())).unwrap();
        let b = registry.insert(definition(&wire())).unwrap();
        assert_ne!(a.identity.connection_id(), b.identity.connection_id());
        assert_ne!(a.identity.vault_account(), b.identity.vault_account());
        assert!(matches!(a.identity.source(), ConnectionSource::Configured));
        assert_ne!(
            a.identity.vault_account(),
            ConnectionIdentity::catalog("linear")
                .unwrap()
                .vault_account()
        );
        let lease = registry.lease(a.identity.connection_id()).unwrap();
        assert!(registry.is_current(&lease));
        let saved = bytes(dir.path());
        let restarted = ConnectionRegistry::load(dir.path()).unwrap();
        assert_eq!(restarted.list().unwrap(), registry.list().unwrap());
        assert!(!restarted.is_current(&lease));
        assert_eq!(restarted.remove(&lease), Err(RegistryError::Conflict));
        assert_eq!(bytes(dir.path()), saved);
        let disk: Value = serde_json::from_slice(&saved).unwrap();
        assert!(disk["connections"][0].get("enabled").is_none());
        assert!(disk["connections"][0].get("source").is_none());
    }

    #[test]
    fn changes_rotate_account_but_identical_saves_preserve_grants_and_revision() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let record = registry.insert(definition(&wire())).unwrap();
        let mut lease = registry.lease(record.identity.connection_id()).unwrap();
        let saved = bytes(dir.path());
        assert_eq!(registry.replace(&lease, definition(&wire())).unwrap(), None);
        assert!(registry.is_current(&lease));
        assert_eq!(bytes(dir.path()), saved);
        for (field, value) in [
            ("url", json!("https://new.example.com/mcp")),
            ("authentication", json!({"type":"none"})),
        ] {
            let mut changed = serde_json::to_value(lease.record.definition()).unwrap();
            changed[field] = value;
            let old = registry
                .replace(&lease, definition(&changed))
                .unwrap()
                .unwrap();
            assert_eq!(old, lease.record);
            let next = registry.lease(record.identity.connection_id()).unwrap();
            assert_eq!(next.record.revision, old.revision + 1);
            assert_ne!(next.record.identity.account_id(), old.identity.account_id());
            assert!(!registry.is_current(&lease));
            let saved = bytes(dir.path());
            assert_eq!(registry.remove(&lease), Err(RegistryError::Conflict));
            assert_eq!(bytes(dir.path()), saved);
            lease = next;
        }
    }

    #[test]
    fn label_edits_invalidate_leases_without_retiring_the_account_binding() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let record = registry.insert(definition(&wire())).unwrap();
        let lease = registry.lease(record.identity.connection_id()).unwrap();
        let mut changed = wire();
        changed["name"] = json!("Renamed calendar");
        assert_eq!(
            registry.replace(&lease, definition(&changed)).unwrap(),
            None
        );
        let current = registry.lease(record.identity.connection_id()).unwrap();
        assert_eq!(current.record.identity, record.identity);
        assert_eq!(current.record.revision, record.revision + 1);
        assert_eq!(current.record.definition.name(), "Renamed calendar");
        assert!(!registry.is_current(&lease));
        assert!(registry.is_current(&current));
    }

    #[test]
    fn custom_option_edits_invalidate_leases_but_keep_the_account() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let record = registry.insert(definition(&wire())).unwrap();
        let lease = registry.lease(record.identity.connection_id()).unwrap();
        let mut changed = wire();
        changed["headers"] = json!({"x-mcp-toolsets":"all"});
        assert_eq!(
            registry.replace(&lease, definition(&changed)).unwrap(),
            None
        );
        let next = registry.lease(record.identity.connection_id()).unwrap();
        assert!(!registry.is_current(&lease));
        assert_eq!(next.record.identity, record.identity);
        assert_eq!(next.record.revision, record.revision + 1);
        let McpConnection::Remote { headers, .. } = next.record.definition() else {
            panic!("expected remote");
        };
        assert_eq!(headers["x-mcp-toolsets"], "all");
        assert_eq!(
            ConnectionRegistry::load(dir.path())
                .unwrap()
                .list()
                .unwrap()[0]
                .definition(),
            next.record.definition()
        );
    }

    #[test]
    fn public_client_changes_retire_accounts_and_persist_only_bound_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let mut input = wire();
        input["authentication"]["type"] = json!("oauth");
        let original = registry.insert(definition(&input)).unwrap();
        let id = original.identity.connection_id();
        for client in [
            json!({"clientId":"one", "issuer":"https://issuer.example"}),
            json!({"clientId":"two", "issuer":"https://issuer.example"}),
            json!({"clientId":"two", "issuer":"https://other.example"}),
            serde_json::Value::Null,
        ] {
            let lease = registry.lease(id).unwrap();
            input["oauthClient"] = client;
            let retired = registry
                .replace(&lease, definition(&input))
                .unwrap()
                .unwrap();
            let next = registry.lease(id).unwrap();
            assert_ne!(
                retired.identity.account_id(),
                next.record.identity.account_id()
            );
            assert!(!registry.is_current(&lease));
            assert_eq!(
                ConnectionRegistry::load(dir.path())
                    .unwrap()
                    .list()
                    .unwrap()[0]
                    .definition(),
                next.record.definition()
            );
        }
    }

    #[test]
    fn removal_and_readding_do_not_resurrect_old_ownership() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let old = registry.insert(definition(&wire())).unwrap();
        let lease = registry.lease(old.identity.connection_id()).unwrap();
        assert_eq!(registry.remove(&lease).unwrap(), old);
        let new = registry.insert(definition(&wire())).unwrap();
        assert_ne!(new.identity.vault_account(), old.identity.vault_account());
        assert!(!registry.is_current(&lease));
        assert_eq!(registry.remove(&lease), Err(RegistryError::Conflict));
        assert_eq!(registry.list().unwrap(), vec![new]);
    }

    #[test]
    fn corrupt_future_duplicate_and_oversized_state_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        registry.insert(definition(&wire())).unwrap();
        let original: Value = serde_json::from_slice(&bytes(dir.path())).unwrap();
        let mut variants = vec![
            json!({"schema":3,"connections":[]}),
            json!({"schema":0,"connections":[]}),
            json!({"schema":2,"connections":[],"secret":"s"}),
        ];
        let mut value = original.clone();
        let row = value["connections"][0].clone();
        value["connections"].as_array_mut().unwrap().push(row);
        variants.push(value);
        for (field, bad) in [
            ("connectionId", json!("linear")),
            ("accountId", json!("not-a-host-id")),
            ("revision", json!(0)),
            ("source", json!({"type":"store"})),
            ("enabled", json!(true)),
        ] {
            let mut value = original.clone();
            value["connections"][0][field] = bad;
            variants.push(value);
        }
        let mut value = original.clone();
        let mut second = value["connections"][0].clone();
        second["connectionId"] = json!(new_id());
        value["connections"].as_array_mut().unwrap().push(second);
        variants.push(value);
        let mut buffers: Vec<Vec<u8>> = variants
            .iter()
            .map(|value| serde_json::to_vec(value).unwrap())
            .collect();
        buffers.push(b"{bad-json".to_vec());
        buffers.push(vec![b' '; MAX_REGISTRY_BYTES + 1]);
        for buffer in buffers {
            fs::write(dir.path().join(FILE_NAME), &buffer).unwrap();
            assert!(ConnectionRegistry::load(dir.path()).is_err());
            assert_eq!(bytes(dir.path()), buffer);
        }
    }

    #[test]
    fn writer_lock_is_nonblocking_and_failure_keeps_memory_and_disk() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let old = registry.insert(definition(&wire())).unwrap();
        let saved = bytes(dir.path());
        let lock = File::options()
            .read(true)
            .write(true)
            .open(dir.path().join(LOCK_NAME))
            .unwrap();
        lock.try_lock().unwrap();
        assert_eq!(
            registry.insert(definition(&wire())),
            Err(RegistryError::Conflict)
        );
        assert_eq!(registry.list().unwrap(), vec![old]);
        assert_eq!(bytes(dir.path()), saved);
        drop(lock);
        registry.insert(definition(&wire())).unwrap(); // No retained failed lock.
    }

    #[test]
    fn stale_disk_snapshot_refuses_overwrite_and_invalidates_freshness() {
        let dir = tempfile::tempdir().unwrap();
        let first = ConnectionRegistry::load(dir.path()).unwrap();
        let old = first.insert(definition(&wire())).unwrap();
        let lease = first.lease(old.identity.connection_id()).unwrap();
        let other = ConnectionRegistry::load(dir.path()).unwrap();
        other.insert(definition(&wire())).unwrap();
        let saved = bytes(dir.path());
        assert!(!first.is_current(&lease));
        assert_eq!(first.remove(&lease), Err(RegistryError::Conflict));
        assert_eq!(first.list().unwrap(), vec![old]);
        assert_eq!(bytes(dir.path()), saved);
    }

    #[test]
    fn concurrent_writers_have_one_winner_without_lost_publication() {
        let dir = tempfile::tempdir().unwrap();
        let a = ConnectionRegistry::load(dir.path()).unwrap();
        let b = ConnectionRegistry::load(dir.path()).unwrap();
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let run = |registry: &ConnectionRegistry| {
                barrier.wait();
                registry.insert(definition(&wire()))
            };
            let a = scope.spawn(move || run(&a));
            let b = scope.spawn(move || run(&b));
            [a.join().unwrap(), b.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == Err(RegistryError::Conflict))
                .count(),
            1
        );
        assert_eq!(
            ConnectionRegistry::load(dir.path())
                .unwrap()
                .list()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn count_limit_and_revision_exhaustion_do_not_publish() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        for _ in 0..MAX_CONNECTIONS {
            registry.insert(definition(&wire())).unwrap();
        }
        let saved = bytes(dir.path());
        assert_eq!(
            registry.insert(definition(&wire())),
            Err(RegistryError::Limit)
        );
        assert_eq!(bytes(dir.path()), saved);
        let mut disk: Value = serde_json::from_slice(&saved).unwrap();
        disk["connections"][0]["revision"] = json!(u64::MAX);
        let saved = serde_json::to_vec(&disk).unwrap();
        fs::write(dir.path().join(FILE_NAME), &saved).unwrap();
        let restarted = ConnectionRegistry::load(dir.path()).unwrap();
        let lease = restarted
            .lease(disk["connections"][0]["connectionId"].as_str().unwrap())
            .unwrap();
        let mut changed = wire();
        changed["name"] = json!("Changed");
        assert_eq!(
            restarted.replace(&lease, definition(&changed)),
            Err(RegistryError::RevisionExhausted)
        );
        assert_eq!(bytes(dir.path()), saved);
        assert!(restarted.is_current(&lease));
    }

    #[cfg(windows)]
    #[test]
    fn atomic_save_refuses_windows_publication_lock_without_partial_state() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let record = registry.insert(definition(&wire())).unwrap();
        let lease = registry.lease(record.identity.connection_id()).unwrap();
        let saved = bytes(dir.path());
        // Allow reads but prohibit replacement/deletion of the committed file.
        let held = File::options()
            .read(true)
            .share_mode(1)
            .open(dir.path().join(FILE_NAME))
            .unwrap();
        assert_eq!(registry.remove(&lease), Err(RegistryError::Filesystem));
        assert_eq!(registry.list().unwrap(), vec![record]);
        assert!(registry.is_current(&lease));
        assert_eq!(bytes(dir.path()), saved);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2); // No pending staging file.
        drop(held);
        registry.remove(&lease).unwrap();
    }
}
