//! Bounded custom MCP metadata persistence. No transport, credential, consent,
//! worker or enabled state is created here. Host acquisition must still approve
//! network/account access before connecting. One short-lived OS lock per save.

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use grain_sdk::mcp::{RemoteMcpConnection, MCP_CONNECTION_MAX_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::mcp::{
    canonical_endpoint, display_text, ConnectionIdentity, ConnectionSource, ContractError,
    StoreDescriptor, ValidatedDescriptor,
};

const FILE_NAME: &str = "mcp-connections.json";
const LOCK_NAME: &str = ".mcp-connections.lock";
pub const MAX_CONNECTIONS: usize = 32;
pub const MAX_REGISTRY_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionDefinition(RemoteMcpConnection);

impl ConnectionDefinition {
    pub fn parse(bytes: &[u8]) -> Result<Self, RegistryError> {
        if bytes.len() > MCP_CONNECTION_MAX_BYTES {
            return Err(RegistryError::Limit);
        }
        let input = serde_json::from_slice(bytes)
            .map_err(|_| RegistryError::InvalidInput(ContractError::InvalidJson))?;
        Self::validate(input)
    }

    pub fn validate(mut input: RemoteMcpConnection) -> Result<Self, RegistryError> {
        if !display_text(&input.name, 120) {
            return Err(RegistryError::InvalidInput(ContractError::InvalidMetadata));
        }
        input.url = canonical_endpoint(&input.url).map_err(RegistryError::InvalidInput)?;
        Ok(Self(input))
    }

    pub fn definition(&self) -> &RemoteMcpConnection {
        &self.0
    }
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
    definition: RemoteMcpConnection,
    #[serde(skip_serializing_if = "Option::is_none")]
    store_artifact: Option<StoreArtifact>,
}

/// Preserved original bytes bind persisted provenance to the downloaded hash.
/// Not publisher input to the direct-connection parser.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreArtifact {
    artifact: String,
    sha256: String,
}

impl ConnectionRecord {
    pub fn identity(&self) -> &ConnectionIdentity {
        &self.identity
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn definition(&self) -> &RemoteMcpConnection {
        &self.definition
    }

    pub fn revocation_state(
        &self,
        revocations: &grain_sdk::Revocations,
    ) -> Option<grain_sdk::RevocationState> {
        match self.identity.source() {
            ConnectionSource::Store {
                extension_id,
                version,
                ..
            } => revocations.state_for(extension_id, version),
            _ => None,
        }
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
    definition: RemoteMcpConnection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    store_artifact: Option<StoreArtifact>,
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
            if disk.schema != 1 || disk.connections.len() > MAX_CONNECTIONS {
                return Err(RegistryError::InvalidState);
            }
            let mut accounts = HashSet::new();
            let mut store_ids = HashSet::new();
            for row in disk.connections {
                if !opaque_id(&row.connection_id)
                    || !opaque_id(&row.account_id)
                    || row.revision == 0
                    || !accounts.insert(row.account_id.clone())
                {
                    return Err(RegistryError::InvalidState);
                }
                let source = match &row.store_artifact {
                    None => ConnectionSource::Configured,
                    Some(store) => {
                        let descriptor = ValidatedDescriptor::parse(store.artifact.as_bytes())
                            .map_err(|_| RegistryError::InvalidState)?;
                        crate::trust::verify_artifact(store.artifact.as_bytes(), &store.sha256)
                            .map_err(|_| RegistryError::InvalidState)?;
                        let metadata = descriptor.descriptor();
                        if !store_ids.insert(metadata.id.clone()) {
                            return Err(RegistryError::InvalidState);
                        }
                        if row.definition.name != metadata.name
                            || row.definition.url != descriptor.endpoint()
                            || row.definition.authentication != metadata.authentication
                        {
                            return Err(RegistryError::InvalidState);
                        }
                        ConnectionSource::Store {
                            extension_id: metadata.id.clone(),
                            version: metadata.version.clone(),
                            artifact_sha256: store.sha256.clone(),
                        }
                    }
                };
                let identity = ConnectionIdentity::new(&row.connection_id, &row.account_id, source)
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
                            store_artifact: row.store_artifact,
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
                store_artifact: None,
            };
            records.insert(id, record.clone());
            Ok(record)
        })
    }

    /// Returns an account-retired record for explicit host vault cleanup when
    /// destination/authentication changes. Label edits preserve the binding;
    /// identical saves preserve the complete record.
    pub fn replace(
        &self,
        lease: &ConnectionLease,
        definition: ConnectionDefinition,
    ) -> Result<Option<ConnectionRecord>, RegistryError> {
        self.mutate(|records| {
            self.check_lease(records, lease)?;
            if lease.record.store_artifact.is_some() {
                return Err(RegistryError::Conflict);
            }
            let id = lease.record.identity.connection_id();
            let old = records.get(id).unwrap();
            if old.definition == definition.0 {
                return Ok(None);
            }
            let revision = old
                .revision
                .checked_add(1)
                .ok_or(RegistryError::RevisionExhausted)?;
            let account_changed = old.definition.url != definition.0.url
                || old.definition.authentication != definition.0.authentication;
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
                        store_artifact: None,
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

    /// Fresh, inactive store acquisition. Updates require a distinct host-owned
    /// retirement transaction; direct edits cannot relabel publisher metadata.
    pub fn insert_store(
        &self,
        admitted: StoreDescriptor,
    ) -> Result<ConnectionRecord, RegistryError> {
        self.mutate(|records| {
            if records.len() >= MAX_CONNECTIONS {
                return Err(RegistryError::Limit);
            }
            let metadata = admitted.descriptor().descriptor();
            if records.values().any(|record| {
                matches!(record.identity.source(),
                ConnectionSource::Store { extension_id, .. } if extension_id == &metadata.id)
            }) {
                return Err(RegistryError::Conflict);
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
            let identity = ConnectionIdentity::new(
                &id,
                &account,
                ConnectionSource::Store {
                    extension_id: metadata.id.clone(),
                    version: metadata.version.clone(),
                    artifact_sha256: admitted.sha256().into(),
                },
            )
            .map_err(|_| RegistryError::InvalidState)?;
            let record = ConnectionRecord {
                identity,
                revision: 1,
                definition: RemoteMcpConnection {
                    name: metadata.name.clone(),
                    url: admitted.descriptor().endpoint().into(),
                    authentication: metadata.authentication,
                },
                store_artifact: Some(StoreArtifact {
                    artifact: admitted.artifact().into(),
                    sha256: admitted.sha256().into(),
                }),
            };
            records.insert(id, record.clone());
            Ok(record)
        })
    }

    /// Publish only against the exact acquired owner. Account identity survives
    /// metadata-only changes; endpoint/auth changes allocate a fresh namespace.
    /// The host must cancel work/disable and retire the old grant before calling
    /// this when the destination changes. No credential I/O occurs here.
    pub fn replace_store(
        &self,
        lease: &ConnectionLease,
        admitted: StoreDescriptor,
    ) -> Result<Option<ConnectionRecord>, RegistryError> {
        self.mutate(|records| {
            self.check_lease(records, lease)?;
            let old = &lease.record;
            let ConnectionSource::Store { extension_id, .. } = old.identity.source() else {
                return Err(RegistryError::Conflict);
            };
            let metadata = admitted.descriptor().descriptor();
            if extension_id != &metadata.id {
                return Err(RegistryError::Conflict);
            }
            let artifact = StoreArtifact {
                artifact: admitted.artifact().into(),
                sha256: admitted.sha256().into(),
            };
            if old.store_artifact.as_ref() == Some(&artifact) {
                return Ok(None);
            }
            let revision = old
                .revision
                .checked_add(1)
                .ok_or(RegistryError::RevisionExhausted)?;
            let definition = RemoteMcpConnection {
                name: metadata.name.clone(),
                url: admitted.descriptor().endpoint().into(),
                authentication: metadata.authentication,
            };
            let changed_account = old.definition.url != definition.url
                || old.definition.authentication != definition.authentication;
            let account = if changed_account {
                new_id()
            } else {
                old.identity.account_id().into()
            };
            if changed_account
                && records
                    .values()
                    .any(|row| row.identity.account_id() == account)
            {
                return Err(RegistryError::InvalidState);
            }
            let identity = ConnectionIdentity::new(
                old.identity.connection_id(),
                &account,
                ConnectionSource::Store {
                    extension_id: metadata.id.clone(),
                    version: metadata.version.clone(),
                    artifact_sha256: admitted.sha256().into(),
                },
            )
            .map_err(|_| RegistryError::InvalidState)?;
            records.insert(
                old.identity.connection_id().into(),
                ConnectionRecord {
                    identity,
                    revision,
                    definition,
                    store_artifact: Some(artifact),
                },
            );
            Ok(changed_account.then(|| old.clone()))
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
            schema: 1,
            connections: candidate
                .values()
                .map(|row| DiskRecord {
                    connection_id: row.identity.connection_id().into(),
                    account_id: row.identity.account_id().into(),
                    revision: row.revision,
                    definition: row.definition.clone(),
                    store_artifact: row.store_artifact.clone(),
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
        crate::extensions::atomic_write(&self.path, &bytes)
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

    fn store_entry() -> (grain_sdk::IndexEntry, &'static [u8]) {
        let roots = grain_sdk::Roots {
            spec: 1,
            version: 1,
            publishing_key: "RWRncmFpbi1oMeKKiXB1MzK9cv70E+awsu8bSq3aeqLBQfIzcSpodrNR".into(),
            base_urls: vec![],
            mirrors: vec![],
            expires: None,
        };
        let index = include_bytes!("../tests/fixtures/mcp-store/index.json");
        let signature = include_str!("../tests/fixtures/mcp-store/index.json.minisig");
        let (verified, status) =
            crate::trust::verify_index(&roots, index, signature, Some(99), 1_800_000_000, false)
                .unwrap();
        assert_eq!(status, crate::trust::IndexStatus::Fresh);
        let mut tampered = index.to_vec();
        tampered[0] = b'!';
        assert!(crate::trust::verify_index(
            &roots,
            &tampered,
            signature,
            None,
            1_800_000_000,
            false
        )
        .is_err());
        assert!(crate::trust::verify_index(
            &roots,
            index,
            signature,
            Some(101),
            1_800_000_000,
            false
        )
        .is_err());
        (
            verified.entries[0].clone(),
            include_bytes!("../tests/fixtures/mcp-store/descriptor.json"),
        )
    }

    fn store_change(mode: &str) -> StoreDescriptor {
        let (mut entry, bytes) = store_entry();
        let mut descriptor: Value = serde_json::from_slice(bytes).unwrap();
        descriptor["version"] = json!("2.0.0");
        entry.version = "2.0.0".into();
        match mode {
            "metadata" => {
                descriptor["description"] = json!("Updated listing.");
                entry.description = "Updated listing.".into();
            }
            "endpoint" => {
                descriptor["transport"]["url"] = json!("https://other.example.com/mcp");
            }
            "authentication" => {
                descriptor["authentication"]["type"] = json!("none");
            }
            "identity" => {
                descriptor["id"] = json!("com.example.other");
                entry.id = "com.example.other".into();
            }
            _ => panic!("unknown case"),
        }
        let bytes = serde_json::to_vec(&descriptor).unwrap();
        entry.size = bytes.len() as u64;
        entry.sha256 = crate::trust::sha256_hex(&bytes);
        StoreDescriptor::admit(&entry, &bytes).unwrap()
    }

    #[test]
    fn verified_store_updates_preserve_or_rotate_accounts_and_invalidate_old_leases() {
        for mode in ["metadata", "endpoint", "authentication"] {
            let dir = tempfile::tempdir().unwrap();
            let registry = ConnectionRegistry::load(dir.path()).unwrap();
            let (entry, bytes) = store_entry();
            let original = registry
                .insert_store(StoreDescriptor::admit(&entry, bytes).unwrap())
                .unwrap();
            let lease = registry.lease(original.identity().connection_id()).unwrap();
            let disk = super::tests::bytes(dir.path());
            assert_eq!(
                registry
                    .replace_store(&lease, StoreDescriptor::admit(&entry, bytes).unwrap())
                    .unwrap(),
                None
            );
            assert_eq!(super::tests::bytes(dir.path()), disk);
            assert!(registry.is_current(&lease));
            let retired = registry.replace_store(&lease, store_change(mode)).unwrap();
            let next = registry.lease(original.identity().connection_id()).unwrap();
            assert_eq!(next.record().revision(), 2);
            assert!(!registry.is_current(&lease));
            if mode == "metadata" {
                assert!(retired.is_none());
                assert_eq!(
                    next.record().identity().vault_account(),
                    original.identity().vault_account()
                );
            } else {
                assert_eq!(retired.unwrap(), original);
                assert_ne!(
                    next.record().identity().vault_account(),
                    original.identity().vault_account()
                );
            }
            assert_eq!(
                registry.replace_store(&lease, store_change(mode)),
                Err(RegistryError::Conflict)
            );
            let reloaded = ConnectionRegistry::load(dir.path()).unwrap();
            assert_eq!(reloaded.list().unwrap(), registry.list().unwrap());
        }
    }

    #[test]
    fn failed_or_wrong_owner_store_updates_preserve_prior_state() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let (entry, artifact) = store_entry();
        let store = registry
            .insert_store(StoreDescriptor::admit(&entry, artifact).unwrap())
            .unwrap();
        let direct = registry.insert(definition(&wire())).unwrap();
        let lease = registry.lease(store.identity().connection_id()).unwrap();
        let disk = bytes(dir.path());
        let prior = registry.list().unwrap();
        assert_eq!(
            registry.replace_store(&lease, store_change("identity")),
            Err(RegistryError::Conflict)
        );
        assert_eq!(
            registry.replace_store(
                &registry.lease(direct.identity().connection_id()).unwrap(),
                store_change("metadata")
            ),
            Err(RegistryError::Conflict)
        );
        let lock = File::options()
            .read(true)
            .write(true)
            .open(dir.path().join(LOCK_NAME))
            .unwrap();
        lock.try_lock().unwrap();
        assert_eq!(
            registry.replace_store(&lease, store_change("endpoint")),
            Err(RegistryError::Conflict)
        );
        assert_eq!(registry.list().unwrap(), prior);
        assert_eq!(bytes(dir.path()), disk);
        drop(lock);
        registry.remove(&lease).unwrap();
        registry
            .insert_store(StoreDescriptor::admit(&entry, artifact).unwrap())
            .unwrap();
        assert_eq!(
            registry.replace_store(&lease, store_change("metadata")),
            Err(RegistryError::Conflict)
        );
    }

    #[test]
    fn revocations_match_store_source_and_version_only() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let (entry, artifact) = store_entry();
        let record = registry
            .insert_store(StoreDescriptor::admit(&entry, artifact).unwrap())
            .unwrap();
        let direct = registry.insert(definition(&wire())).unwrap();
        for (id, version, state, expected) in [
            (
                "com.example.calendar",
                None,
                "revoked",
                Some(grain_sdk::RevocationState::Revoked),
            ),
            (
                "com.example.calendar",
                Some("1.0.0"),
                "revoked",
                Some(grain_sdk::RevocationState::Revoked),
            ),
            ("com.example.calendar", Some("2.0.0"), "revoked", None),
            ("com.example.other", None, "revoked", None),
            (
                "com.example.calendar",
                None,
                "deprecated",
                Some(grain_sdk::RevocationState::Deprecated),
            ),
        ] {
            let revocations = serde_json::from_value(
                json!({"spec":1,"version":2,"expires":"2099-01-01T00:00:00Z",
                "entries":[{"id":id,"version":version,"state":state,"reason":"test"}]}),
            )
            .unwrap();
            assert_eq!(record.revocation_state(&revocations), expected);
            assert_eq!(direct.revocation_state(&revocations), None);
        }
    }

    #[test]
    fn signed_mcp_artifact_admission_refuses_substitution_and_native_authority() {
        let (entry, artifact) = store_entry();
        StoreDescriptor::admit(&entry, artifact).unwrap();
        assert!(entry.validate_installable().is_err());
        for field in [
            "id",
            "version",
            "name",
            "description",
            "size",
            "sha256",
            "capabilities",
            "extends",
            "categories",
            "trust",
            "artifact_kind",
            "min_grain_api",
        ] {
            let mut value = serde_json::to_value(&entry).unwrap();
            value[field] = match field {
                "size" => json!(artifact.len() + 1),
                "sha256" => json!("a".repeat(64)),
                "capabilities" => json!(["auth"]),
                "extends" => json!(["pill"]),
                "categories" => json!(["prompts"]),
                "trust" => json!("dev"),
                "artifact_kind" => json!("native"),
                "min_grain_api" => json!("2.0"),
                _ => json!("substitution"),
            };
            let changed = serde_json::from_value(value).unwrap();
            assert!(
                StoreDescriptor::admit(&changed, artifact).is_err(),
                "{field}"
            );
        }
        let mut altered = artifact.to_vec();
        altered[0] = b'!';
        assert!(StoreDescriptor::admit(&entry, &altered).is_err());
        let mut wire: Value = serde_json::from_slice(artifact).unwrap();
        for field in ["token", "enabled", "trust", "accountId", "command"] {
            wire[field] = json!("forged");
            let body = serde_json::to_vec(&wire).unwrap();
            let mut changed = entry.clone();
            changed.size = body.len() as u64;
            changed.sha256 = crate::trust::sha256_hex(&body);
            assert!(StoreDescriptor::admit(&changed, &body).is_err(), "{field}");
            wire.as_object_mut().unwrap().remove(field);
        }
    }

    #[test]
    fn store_acquisition_has_separate_persistent_ownership_and_no_direct_edit() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        let (entry, artifact) = store_entry();
        let direct = registry.insert(definition(&wire())).unwrap();
        let store = registry
            .insert_store(StoreDescriptor::admit(&entry, artifact).unwrap())
            .unwrap();
        let lease = registry.lease(store.identity().connection_id()).unwrap();
        assert!(
            matches!(store.identity().source(), ConnectionSource::Store { extension_id, .. } if extension_id == &entry.id)
        );
        assert!(store
            .identity()
            .vault_account()
            .starts_with("mcp:v1:store:com.example.calendar:"));
        assert_ne!(
            store.identity().vault_account(),
            direct.identity().vault_account()
        );
        let original = bytes(dir.path());
        assert_eq!(
            registry.replace(&lease, definition(&wire())),
            Err(RegistryError::Conflict)
        );
        assert_eq!(
            registry.insert_store(StoreDescriptor::admit(&entry, artifact).unwrap()),
            Err(RegistryError::Conflict)
        );
        assert_eq!(bytes(dir.path()), original);
        let reloaded = ConnectionRegistry::load(dir.path()).unwrap();
        assert_eq!(
            reloaded
                .lease(store.identity().connection_id())
                .unwrap()
                .record(),
            &store
        );
        assert!(!reloaded.is_current(&lease));
        let current = registry.lease(store.identity().connection_id()).unwrap();
        registry.remove(&current).unwrap();
        assert_eq!(registry.list().unwrap(), vec![direct]);
    }

    #[test]
    fn stored_artifact_drift_is_preserved_and_refused_at_restart() {
        for field in ["artifact", "sha256", "definition", "duplicate"] {
            let dir = tempfile::tempdir().unwrap();
            let registry = ConnectionRegistry::load(dir.path()).unwrap();
            let (entry, artifact) = store_entry();
            registry
                .insert_store(StoreDescriptor::admit(&entry, artifact).unwrap())
                .unwrap();
            let mut disk: Value = serde_json::from_slice(&bytes(dir.path())).unwrap();
            if field == "duplicate" {
                let mut duplicate = disk["connections"][0].clone();
                duplicate["connectionId"] = json!(new_id());
                duplicate["accountId"] = json!(new_id());
                disk["connections"].as_array_mut().unwrap().push(duplicate);
            } else if field == "definition" {
                disk["connections"][0]["definition"]["url"] =
                    json!("https://changed.example.com/mcp");
            } else {
                disk["connections"][0]["storeArtifact"][field] = json!("changed");
            }
            let changed = serde_json::to_vec(&disk).unwrap();
            fs::write(dir.path().join(FILE_NAME), &changed).unwrap();
            assert!(
                matches!(
                    ConnectionRegistry::load(dir.path()),
                    Err(RegistryError::InvalidState)
                ),
                "{field}"
            );
            assert_eq!(bytes(dir.path()), changed);
        }
    }

    #[test]
    fn failed_store_publication_preserves_registry_memory_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ConnectionRegistry::load(dir.path()).unwrap();
        registry.insert(definition(&wire())).unwrap();
        let previous = registry.list().unwrap();
        let original = bytes(dir.path());
        let lock = File::options()
            .read(true)
            .write(true)
            .open(dir.path().join(LOCK_NAME))
            .unwrap();
        lock.try_lock().unwrap();
        let (entry, artifact) = store_entry();
        assert_eq!(
            registry.insert_store(StoreDescriptor::admit(&entry, artifact).unwrap()),
            Err(RegistryError::Conflict)
        );
        assert_eq!(registry.list().unwrap(), previous);
        assert_eq!(bytes(dir.path()), original);
    }

    fn wire() -> Value {
        json!({"name":"Test calendar","url":"https://mcp.example.com/mcp",
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
        assert_eq!(
            admitted.definition().authentication,
            McpAuthentication::OAuth {}
        );
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
        assert!(ConnectionDefinition::parse(br#"{"name":"a","name":"b","url":"https://mcp.example.com","authentication":{"type":"none"}}"#).is_err());
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
        assert_eq!(
            definition(&value).definition().url,
            "https://mcp.example.com"
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
        assert_eq!(current.record.definition.name, "Renamed calendar");
        assert!(!registry.is_current(&lease));
        assert!(registry.is_current(&current));
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
            json!({"schema":2,"connections":[]}),
            json!({"schema":1,"connections":[],"secret":"s"}),
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
