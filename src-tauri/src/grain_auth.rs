//! Host-owned extension OAuth 2.0 Authorization Code + PKCE.
//!
//! Tokens never cross the extension boundary. They are stored only in the OS
//! credential vault and are attached by `host_api::net.fetch` after exact-host
//! authorization. There is no idle service: the loopback listener exists only
//! while a connect flow is active and is dropped on every exit path.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use futures_util::StreamExt;
use grain_sdk::AuthenticationDecl;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use zeroize::{Zeroize, Zeroizing};

const VAULT_SERVICE: &str = "com.grain.extension.oauth";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(300);
const CALLBACK_MAX_BYTES: usize = 16 * 1024;
const TOKEN_MAX_BYTES: usize = 64 * 1024;
const EXPIRY_SKEW_SECS: u64 = 60;
const MAX_PENDING_CONNECTS: usize = 8;
struct PendingFlow {
    run_id: uuid::Uuid,
    execution_generation: u64,
    cancel: tokio::sync::oneshot::Sender<()>,
}

type PendingFlows = Arc<Mutex<HashMap<String, PendingFlow>>>;
static PENDING: OnceLock<PendingFlows> = OnceLock::new();
type RefreshLocks = Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>;
static REFRESH_LOCKS: OnceLock<RefreshLocks> = OnceLock::new();
static VAULT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

// Only active refreshes retain a lock. Waiting for one account never blocks
// another account's provider request, and the pool has a hard admission bound.
struct RefreshOwner {
    pool: RefreshLocks,
    key: String,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl RefreshOwner {
    fn acquire(pool: RefreshLocks, key: String) -> Result<Self, String> {
        let lock = {
            let mut locks = pool.lock().map_err(|_| "refresh state is unavailable")?;
            locks.retain(|_, lock| lock.strong_count() != 0);
            if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
                lock
            } else {
                if locks.len() >= MAX_PENDING_CONNECTS {
                    return Err("too many native account refreshes are pending".into());
                }
                let lock = Arc::new(tokio::sync::Mutex::new(()));
                locks.insert(key.clone(), Arc::downgrade(&lock));
                lock
            }
        };
        Ok(Self { pool, key, lock })
    }
}

impl Drop for RefreshOwner {
    fn drop(&mut self) {
        if let Ok(mut locks) = self.pool.lock() {
            if Arc::strong_count(&self.lock) == 1 {
                locks.remove(&self.key);
            }
        }
    }
}

#[derive(Clone)]
struct PendingOwner {
    pending: PendingFlows,
    id: String,
    run_id: uuid::Uuid,
}

impl PendingOwner {
    fn current_guard(&self) -> Option<MutexGuard<'_, HashMap<String, PendingFlow>>> {
        let pending = self.pending.lock().ok()?;
        pending
            .get(&self.id)
            .is_some_and(|flow| flow.run_id == self.run_id)
            .then_some(pending)
    }
    // Serialize cancellation/replacement against the actual vault write. The
    // registry is checked separately: never hold its lock through OS vault I/O.
    fn publish<T>(&self, write: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| "authentication cancellation state is unavailable")?;
        if !pending
            .get(&self.id)
            .is_some_and(|flow| flow.run_id == self.run_id)
        {
            return Err("authentication was cancelled or replaced".into());
        }
        write()
    }
}

struct PendingGuard(PendingOwner);

impl PendingGuard {
    fn begin(
        pending: PendingFlows,
        id: String,
        execution_generation: u64,
    ) -> Result<(Self, tokio::sync::oneshot::Receiver<()>), String> {
        let run_id = uuid::Uuid::new_v4();
        let (cancel, receiver) = tokio::sync::oneshot::channel();
        {
            let mut flows = pending
                .lock()
                .map_err(|_| "authentication cancellation state is unavailable")?;
            if !flows.contains_key(&id) && flows.len() >= MAX_PENDING_CONNECTS {
                return Err("too many extension sign-ins are pending".into());
            }
            if let Some(previous) = flows.insert(
                id.clone(),
                PendingFlow {
                    run_id,
                    execution_generation,
                    cancel,
                },
            ) {
                let _ = previous.cancel.send(());
            }
        }
        Ok((
            Self(PendingOwner {
                pending,
                id,
                run_id,
            }),
            receiver,
        ))
    }
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.0.pending.lock() {
            if pending
                .get(&self.0.id)
                .is_some_and(|flow| flow.run_id == self.0.run_id)
            {
                pending.remove(&self.0.id);
            }
        }
    }
}

#[derive(Clone)]
struct PreparedConnect {
    registry: Arc<grain_core::extensions::ExtensionsRegistry>,
    revision: grain_core::extensions::RecordRevision,
    execution_generation: u64,
    declaration: AuthenticationDecl,
}

impl PreparedConnect {
    fn check_worker(&self, generation: u64) -> Result<(), String> {
        if self.execution_generation != generation {
            return Err("extension account or runtime changed; run a fresh tool call".into());
        }
        Ok(())
    }
    fn check(&self, id: &str) -> Result<(), String> {
        self.registry
            .with_current_record(id, self.revision, |_| ())
            .map_err(|error| error.to_string())
    }
}

fn prepare_connect(app: &AppHandle, id: &str) -> Result<PreparedConnect, String> {
    let registry = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?
        .inner()
        .clone();
    let revision = registry.record_revision(id);
    let declaration = approved_declaration(app, id)?;
    let execution_generation = registry
        .with_current_record(id, revision, |record| {
            record.map(|record| record.execution_generation)
        })
        .map_err(|error| error.to_string())?
        .ok_or("extension is no longer installed")?;
    Ok(PreparedConnect {
        registry,
        revision,
        execution_generation,
        declaration,
    })
}

async fn await_connect<T>(
    cancel: tokio::sync::oneshot::Receiver<()>,
    budget: Duration,
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::select! {
        biased;
        _ = cancel => Err("authentication was cancelled".into()),
        result = tokio::time::timeout(budget, work) => {
            result.map_err(|_| "authentication timed out after 5 minutes".to_string())?
        }
    }
}

#[derive(Clone, Serialize, specta::Type)]
pub struct AuthConnection {
    pub provider_name: String,
    pub authorization_host: String,
    pub token_host: String,
    pub scopes: Vec<String>,
    pub api_hosts: Vec<String>,
    /// `connected` | `needs_reauthorization` | `expired` | `disconnected` | `unavailable`
    pub state: String,
    pub granted_scopes: Vec<String>,
    /// Unix seconds, sent as a string to avoid JS integer loss.
    pub expires_at: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct TokenSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    authentication_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credential_revision: Option<String>,
    access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    #[serde(default = "bearer")]
    token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<u64>,
    #[serde(default)]
    scopes: Vec<String>,
}

fn bearer() -> String {
    "Bearer".into()
}

impl Drop for TokenSet {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(refresh) = &mut self.refresh_token {
            refresh.zeroize();
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default = "bearer")]
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

impl Drop for TokenResponse {
    fn drop(&mut self) {
        if let Some(access_token) = &mut self.access_token {
            access_token.zeroize();
        }
        if let Some(refresh_token) = &mut self.refresh_token {
            refresh_token.zeroize();
        }
    }
}

fn vault_entry(extension_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(VAULT_SERVICE, extension_id)
        .map_err(|error| format!("OS credential vault unavailable: {error}"))
}

fn read_token_sync(extension_id: &str) -> Result<Option<TokenSet>, String> {
    let _guard = VAULT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "OS credential vault lock is unavailable")?;
    read_token_unlocked(extension_id)
}

fn read_token_unlocked(extension_id: &str) -> Result<Option<TokenSet>, String> {
    let entry = vault_entry(extension_id)?;
    let mut bytes = match entry.get_secret() {
        Ok(bytes) => bytes,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(error) => return Err(format!("OS credential vault read failed: {error}")),
    };
    let result = if bytes.len() > TOKEN_MAX_BYTES {
        Err("stored OAuth credential is too large".into())
    } else {
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| format!("stored OAuth credential is invalid: {error}"))
    };
    bytes.zeroize();
    result
}

fn write_token_sync(
    extension_id: &str,
    token: &TokenSet,
    connect_owner: Option<(&PreparedConnect, &PendingOwner)>,
) -> Result<(), String> {
    let _guard = VAULT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "OS credential vault lock is unavailable")?;
    let write = || {
        let entry = vault_entry(extension_id)?;
        let mut bytes = serde_json::to_vec(token).map_err(|error| error.to_string())?;
        let result = entry
            .set_secret(&bytes)
            .map_err(|error| format!("OS credential vault write failed: {error}"));
        bytes.zeroize();
        result
    };
    if let Some((prepared, owner)) = connect_owner {
        // Check after waiting for the vault lock, including blocking tasks that
        // outlive their dropped async caller. No registry lock covers vault I/O.
        publish_connect(prepared, owner, write)
    } else {
        write()
    }
}

fn check_refresh_owner(previous: &TokenSet, current: Option<&TokenSet>) -> Result<(), String> {
    let current = current.ok_or("authentication was disconnected during refresh")?;
    if previous.account_session.is_none()
        || previous.credential_revision.is_none()
        || previous.account_session != current.account_session
        || previous.credential_revision != current.credential_revision
    {
        return Err("authentication was replaced during refresh".into());
    }
    Ok(())
}

fn write_refreshed_sync(
    key: &str,
    id: &str,
    prepared: &PreparedConnect,
    previous: &TokenSet,
    refreshed: &TokenSet,
) -> Result<(), String> {
    let _guard = VAULT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "OS credential vault lock is unavailable")?;
    prepared.check(id)?;
    let current = read_token_unlocked(key)?;
    publish_refresh(prepared, id, previous, current.as_ref(), || {
        let bytes =
            Zeroizing::new(serde_json::to_vec(refreshed).map_err(|error| error.to_string())?);
        vault_entry(key)?
            .set_secret(&bytes)
            .map_err(|error| format!("OS credential vault write failed: {error}"))
    })
}

fn publish_refresh<T>(
    prepared: &PreparedConnect,
    id: &str,
    previous: &TokenSet,
    current: Option<&TokenSet>,
    write: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    check_refresh_owner(previous, current)?;
    // Recheck after the potentially blocking read. The unique session key is
    // never reused by a new login, even if logout starts during set_secret.
    prepared.check(id)?;
    write()
}

fn publish_connect<T>(
    prepared: &PreparedConnect,
    owner: &PendingOwner,
    write: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    prepared.check(&owner.id)?;
    owner.publish(write)
}

fn delete_token_sync(extension_id: &str) -> Result<(), String> {
    let _guard = VAULT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "OS credential vault lock is unavailable")?;
    let entry = vault_entry(extension_id)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(error) => return Err(format!("OS credential vault delete failed: {error}")),
    }
    Ok(())
}

fn cleanup_retired_credential(key: &str) {
    if delete_token_sync(key).is_err() {
        // Never include a vault error's credential contents or key in logs.
        log::warn!("retired native OAuth credential cleanup failed in the OS vault");
    }
}

async fn read_token(extension_id: &str) -> Result<Option<TokenSet>, String> {
    let extension_id = extension_id.to_owned();
    tokio::task::spawn_blocking(move || read_token_sync(&extension_id))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?
}

fn session_key(id: &str, session: &str) -> Result<String, String> {
    if session.len() != 32 || !session.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid native authentication session".into());
    }
    Ok(format!("{id}/{session}"))
}

fn active_session(prepared: &PreparedConnect, id: &str) -> Result<String, String> {
    prepared
        .registry
        .with_current_record(id, prepared.revision, |record| {
            record.and_then(|record| record.authentication_session.clone())
        })
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "authentication is not connected; reconnect legacy credentials".into())
}

fn check_account_token(
    token: &TokenSet,
    session: &str,
    decl: &AuthenticationDecl,
) -> Result<(), String> {
    check_token_binding(token, decl)?;
    if token.account_session.as_deref() != Some(session) || token.credential_revision.is_none() {
        return Err("authentication needs reauthorization for its account session".into());
    }
    if !decl.scopes.iter().all(|scope| token.scopes.contains(scope)) {
        return Err("authentication needs reauthorization for its declared scopes".into());
    }
    Ok(())
}

fn declaration(app: &AppHandle, extension_id: &str) -> Result<AuthenticationDecl, String> {
    crate::grain_commands::load_pack(app, extension_id)?
        .manifest
        .contributes
        .authentication
        .ok_or_else(|| format!("authentication is not declared by '{extension_id}'"))
}

fn approved_declaration(app: &AppHandle, extension_id: &str) -> Result<AuthenticationDecl, String> {
    let pack = crate::grain_commands::load_pack(app, extension_id)?;
    let registry = app
        .try_state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let record = registry
        .record(extension_id)
        .ok_or_else(|| format!("'{extension_id}' is not installed"))?;
    if !record.enabled {
        return Err("extension is disabled".into());
    }
    if !record.granted.iter().any(|capability| capability == "auth") {
        return Err("capability 'auth' is not granted".into());
    }
    let declaration = pack
        .manifest
        .contributes
        .authentication
        .ok_or_else(|| format!("authentication is not declared by '{extension_id}'"))?;
    let fingerprint = grain_core::extensions::authentication_fingerprint(&declaration);
    if record.authentication_approved.as_deref() != Some(fingerprint.as_str()) {
        return Err(format!(
            "authentication declaration for '{extension_id}' changed and requires approval"
        ));
    }
    Ok(declaration)
}

fn endpoint_host(endpoint: &str) -> String {
    Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn needs_refresh(token: &TokenSet) -> bool {
    token
        .expires_at
        .is_some_and(|expiry| expiry <= now().saturating_add(EXPIRY_SKEW_SECS))
}

fn check_token_binding(token: &TokenSet, decl: &AuthenticationDecl) -> Result<(), String> {
    let expected = grain_core::extensions::authentication_fingerprint(decl);
    if token.authentication_fingerprint.as_deref() != Some(expected.as_str()) {
        return Err("authentication needs reauthorization for its provider configuration".into());
    }
    Ok(())
}

async fn parse_token_response(
    response: reqwest::Response,
) -> Result<(reqwest::StatusCode, TokenResponse), String> {
    if response
        .content_length()
        .is_some_and(|length| length > TOKEN_MAX_BYTES as u64)
    {
        return Err("token response exceeded 64 KiB".into());
    }
    let status = response.status();
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "token response could not be read".to_string())?;
        if bytes.len().saturating_add(chunk.len()) > TOKEN_MAX_BYTES {
            bytes.zeroize();
            return Err("token response exceeded 64 KiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let parsed = serde_json::from_slice(&bytes)
        .map_err(|_| "token endpoint returned an invalid response".to_string());
    bytes.zeroize();
    parsed.map(|token| (status, token))
}

fn provider_error(status: reqwest::StatusCode, code: Option<String>) -> String {
    let safe = code.filter(|value| {
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    });
    match safe {
        Some(code) => format!("token endpoint rejected the request ({status}, {code})"),
        None => format!("token endpoint rejected the request ({status})"),
    }
}

async fn connection_for(
    extension_id: &str,
    session: Option<&str>,
    decl: &AuthenticationDecl,
) -> AuthConnection {
    let key = session
        .map(|session| session_key(extension_id, session))
        .transpose();
    let credential = match key {
        Ok(Some(key)) => read_token(&key).await,
        // Legacy credentials remain available for explicit reconnection, but
        // cannot silently become the current account after an upgrade.
        Ok(None) => read_token(extension_id).await,
        Err(error) => Err(error),
    };
    let (state, granted_scopes, expires_at) = match credential {
        Ok(Some(token)) => {
            let state = if session
                .is_none_or(|session| check_account_token(&token, session, decl).is_err())
            {
                "needs_reauthorization"
            } else if needs_refresh(&token) {
                "expired"
            } else {
                "connected"
            };
            (
                state.into(),
                token.scopes.clone(),
                token.expires_at.map(|v| v.to_string()),
            )
        }
        Ok(None) => ("disconnected".into(), Vec::new(), None),
        Err(_) => ("unavailable".into(), Vec::new(), None),
    };
    AuthConnection {
        provider_name: decl.provider_name.clone(),
        authorization_host: endpoint_host(&decl.authorization_endpoint),
        token_host: endpoint_host(&decl.token_endpoint),
        scopes: decl.scopes.clone(),
        api_hosts: decl.api_hosts.clone(),
        state,
        granted_scopes,
        expires_at,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn extension_auth_connection(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<Option<AuthConnection>, String> {
    crate::grain_commands::require_main_window(&window)?;
    connection(&app, &id).await
}

pub(crate) async fn connection(
    app: &AppHandle,
    id: &str,
) -> Result<Option<AuthConnection>, String> {
    let declaration = crate::grain_commands::load_pack(app, id)?
        .manifest
        .contributes
        .authentication;
    match declaration {
        Some(declaration) => {
            let registry = app
                .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
                .ok_or("extensions registry unavailable")?;
            let revision = registry.record_revision(id);
            let session = registry
                .with_current_record(id, revision, |record| {
                    record.and_then(|record| record.authentication_session.clone())
                })
                .map_err(|error| error.to_string())?;
            let connection = connection_for(id, session.as_deref(), &declaration).await;
            registry
                .with_current_record(id, revision, |_| ())
                .map_err(|error| error.to_string())?;
            Ok(Some(connection))
        }
        None => Ok(None),
    }
}

fn authorize_url(
    decl: &AuthenticationDecl,
    redirect: &str,
    state: &str,
    challenge: &str,
) -> Result<String, String> {
    let mut url = Url::parse(&decl.authorization_endpoint).map_err(|error| error.to_string())?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &decl.client_id)
            .append_pair("redirect_uri", redirect)
            .append_pair("scope", &decl.scopes.join(" "))
            .append_pair("state", state)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        for (key, value) in &decl.authorization_parameters {
            query.append_pair(key, value);
        }
    }
    Ok(url.into())
}

async fn read_callback_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut request = Vec::with_capacity(1024);
        loop {
            let mut chunk = [0_u8; 1024];
            let read = stream
                .read(&mut chunk)
                .await
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            if request.len().saturating_add(read) > CALLBACK_MAX_BYTES {
                return Err("OAuth callback headers were too large".into());
            }
            request.extend_from_slice(&chunk[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        Ok(request)
    })
    .await
    .map_err(|_| "OAuth callback connection timed out".to_string())?
}

async fn send_callback_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\n\
         X-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'\r\n\
         Content-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

async fn callback(
    listener: TcpListener,
    expected_path: String,
    expected_state: String,
    expected_host: String,
) -> Result<String, String> {
    loop {
        let (mut stream, peer) = listener.accept().await.map_err(|error| error.to_string())?;
        if !peer.ip().is_loopback() {
            continue;
        }
        let request = match read_callback_request(&mut stream).await {
            Ok(request) => request,
            Err(_) => {
                send_callback_response(
                    &mut stream,
                    "400 Bad Request",
                    "Authentication failed. Return to Grain and try again.",
                )
                .await;
                continue;
            }
        };
        let Ok(request) = std::str::from_utf8(&request) else {
            send_callback_response(
                &mut stream,
                "400 Bad Request",
                "Authentication failed. Return to Grain and try again.",
            )
            .await;
            continue;
        };
        let mut lines = request.lines();
        let first = lines.next().unwrap_or_default();
        let mut first = first.split_whitespace();
        let method = first.next().unwrap_or_default();
        let target = first.next().unwrap_or_default();
        let host = lines.find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("host").then(|| value.trim())
        });
        let url = Url::parse(&format!("http://127.0.0.1{target}"));
        let valid_url = url.as_ref().ok();
        let mut duplicate_reserved = false;
        let mut params = std::collections::BTreeMap::new();
        if let Some(url) = valid_url {
            for (key, value) in url.query_pairs().into_owned() {
                if params.insert(key.clone(), value).is_some()
                    && matches!(key.as_str(), "state" | "code" | "error")
                {
                    duplicate_reserved = true;
                }
            }
        }
        let valid = method == "GET"
            && host == Some(expected_host.as_str())
            && valid_url.is_some_and(|url| url.path() == expected_path)
            && !duplicate_reserved
            && params.get("state") == Some(&expected_state);
        if !valid {
            send_callback_response(
                &mut stream,
                "400 Bad Request",
                "Authentication failed. Return to Grain and try again.",
            )
            .await;
            continue;
        }
        if params.get("error").is_some() {
            send_callback_response(
                &mut stream,
                "400 Bad Request",
                "Authentication was not approved. You can close this tab and return to Grain.",
            )
            .await;
            return Err("provider denied authentication".into());
        }
        let code = params
            .get("code")
            .cloned()
            .filter(|code| !code.is_empty() && code.len() <= 4096)
            .ok_or_else(|| "OAuth callback did not contain a valid code".to_string())?;
        send_callback_response(
            &mut stream,
            "200 OK",
            "Authentication complete. You can close this tab and return to Grain.",
        )
        .await;
        return Ok(code);
    }
}

async fn exchange(
    decl: &AuthenticationDecl,
    code: &str,
    redirect: &str,
    verifier: &str,
) -> Result<TokenSet, String> {
    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?
        .post(&decl.token_endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", decl.client_id.as_str()),
            ("redirect_uri", redirect),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|error| format!("token exchange failed: {error}"))?;
    if response.status().is_redirection() {
        return Err("token endpoint redirects are not allowed".into());
    }
    let (status, mut token) = parse_token_response(response).await?;
    if !status.is_success() || token.error.is_some() {
        return Err(provider_error(status, token.error.take()));
    }
    if !token.token_type.eq_ignore_ascii_case("bearer") {
        return Err("token endpoint returned an unsupported token type".into());
    }
    Ok(TokenSet {
        authentication_fingerprint: Some(grain_core::extensions::authentication_fingerprint(decl)),
        account_session: Some(uuid::Uuid::new_v4().simple().to_string()),
        credential_revision: Some(uuid::Uuid::new_v4().simple().to_string()),
        access_token: token
            .access_token
            .take()
            .filter(|token| !token.is_empty())
            .ok_or_else(|| "token response omitted access_token".to_string())?,
        refresh_token: token.refresh_token.take(),
        token_type: std::mem::take(&mut token.token_type),
        expires_at: token
            .expires_in
            .map(|seconds| now().saturating_add(seconds)),
        scopes: token
            .scope
            .take()
            .map(|scope| scope.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_else(|| decl.scopes.clone()),
    })
}

async fn refresh(decl: &AuthenticationDecl, previous: &TokenSet) -> Result<TokenSet, String> {
    let refresh_token = previous.refresh_token.as_deref().ok_or_else(|| {
        "authentication has expired; reconnect it in extension settings".to_string()
    })?;
    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?
        .post(&decl.token_endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", decl.client_id.as_str()),
        ])
        .send()
        .await
        .map_err(|error| format!("token refresh failed: {error}"))?;
    if response.status().is_redirection() {
        return Err("token endpoint redirects are not allowed".into());
    }
    let (status, mut token) = parse_token_response(response).await?;
    if !status.is_success() || token.error.is_some() {
        return Err(provider_error(status, token.error.take()));
    }
    if !token.token_type.eq_ignore_ascii_case("bearer") {
        return Err("token endpoint returned an unsupported token type".into());
    }
    Ok(TokenSet {
        authentication_fingerprint: Some(grain_core::extensions::authentication_fingerprint(decl)),
        account_session: previous.account_session.clone(),
        credential_revision: Some(uuid::Uuid::new_v4().simple().to_string()),
        access_token: token
            .access_token
            .take()
            .filter(|token| !token.is_empty())
            .ok_or_else(|| "refresh response omitted access_token".to_string())?,
        refresh_token: token
            .refresh_token
            .take()
            .or_else(|| Some(refresh_token.to_owned())),
        token_type: std::mem::take(&mut token.token_type),
        expires_at: token
            .expires_in
            .map(|seconds| now().saturating_add(seconds)),
        scopes: token
            .scope
            .take()
            .map(|scope| scope.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_else(|| previous.scopes.clone()),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn extension_auth_connect(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<AuthConnection, String> {
    crate::grain_commands::require_main_window(&window)?;
    connect(app, id).await
}

async fn connect(app: AppHandle, id: String) -> Result<AuthConnection, String> {
    let prepared = prepare_connect(&app, &id)?;
    connect_reviewed(app, id, prepared).await
}

async fn connect_reviewed(
    app: AppHandle,
    id: String,
    prepared: PreparedConnect,
) -> Result<AuthConnection, String> {
    let pending = PENDING
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone();
    let (guard, cancel) = prepared
        .registry
        .with_current_record(&id, prepared.revision, |_| {
            PendingGuard::begin(pending, id.clone(), prepared.execution_generation)
        })
        .map_err(|error| error.to_string())??;
    await_connect(cancel, CONNECT_TIMEOUT, async {
        connect_attempt(&app, &id, &prepared, &guard.0).await
    })
    .await
}

async fn connect_attempt(
    app: &AppHandle,
    id: &str,
    prepared: &PreparedConnect,
    owner: &PendingOwner,
) -> Result<AuthConnection, String> {
    let decl = &prepared.declaration;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|error| format!("could not start OAuth callback listener: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let path = format!("/grain/oauth/{}", uuid::Uuid::new_v4());
    let redirect = format!("http://127.0.0.1:{port}{path}");
    let state = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let verifier = Zeroizing::new(format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    ));
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let url = authorize_url(decl, &redirect, &state, &challenge)?;
    prepared.check(id)?;
    owner.publish(|| Ok(()))?;
    if let Err(error) = app.opener().open_url(url, None::<String>) {
        return Err(format!("could not open the sign-in page: {error}"));
    }
    let code = callback(listener, path, state, format!("127.0.0.1:{port}")).await?;
    prepared.check(id)?;
    let token = exchange(decl, &code, &redirect, verifier.as_str()).await?;
    prepared.check(id)?;
    if approved_declaration(app, id)? != *decl {
        return Err("authentication declaration changed during sign-in".into());
    }
    let write_owner = owner.clone();
    let write_prepared = prepared.clone();
    let id_copy = id.to_owned();
    let prior_session = prepared
        .registry
        .with_current_record(id, prepared.revision, |record| {
            record.and_then(|record| record.authentication_session.clone())
        })
        .map_err(|error| error.to_string())?;
    let session = token
        .account_session
        .clone()
        .ok_or("token exchange omitted account session")?;
    let write_session = session.clone();
    let generation = tokio::task::spawn_blocking(move || {
        let key = session_key(&id_copy, &write_session)?;
        if let Err(error) = write_token_sync(&key, &token, Some((&write_prepared, &write_owner))) {
            cleanup_retired_credential(&key);
            return Err(error);
        }
        // Publish only after the unique candidate is stored. Registry -> pending
        // lock ordering matches cancellation/launch; no vault I/O under either.
        let mut published = false;
        let publication = write_prepared
            .registry
            .set_authentication_session_if_current(
                &id_copy,
                write_prepared.revision,
                Some(write_session),
                || {
                    let guard = write_owner.current_guard()?;
                    published = true;
                    Some(guard)
                },
            )
            .map_err(|error| error.to_string());
        if publication.is_err() {
            // A persistence failure may have made this the active in-memory
            // account. Preserve that credential for recovery in that case.
            if !published {
                cleanup_retired_credential(&key);
            }
        } else {
            if let Some(prior) =
                prior_session.and_then(|session| session_key(&id_copy, &session).ok())
            {
                cleanup_retired_credential(&prior);
            }
            cleanup_retired_credential(&id_copy);
        }
        publication
    })
    .await
    .map_err(|error| format!("credential task failed: {error}"))??;
    let connection = connection_for(id, Some(&session), decl).await;
    let current = prepared.registry.with_record_locked(id, |record| {
        record.is_some_and(|record| {
            record.execution_generation == generation
                && record.authentication_session.as_deref() == Some(session.as_str())
        })
    });
    if !current {
        return Err("authentication changed after sign-in".into());
    }
    owner.publish(|| Ok(connection))
}

#[tauri::command]
#[specta::specta]
pub async fn extension_auth_disconnect(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<(), String> {
    crate::grain_commands::require_main_window(&window)?;
    disconnect(&app, id).await
}

async fn disconnect(app: &AppHandle, id: String) -> Result<(), String> {
    declaration(app, &id)?;
    let registry = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?
        .inner()
        .clone();
    let revision = registry.record_revision(&id);
    disconnect_reviewed(registry, id, revision).await
}

async fn disconnect_reviewed(
    registry: Arc<grain_core::extensions::ExtensionsRegistry>,
    id: String,
    revision: grain_core::extensions::RecordRevision,
) -> Result<(), String> {
    let session = registry
        .with_current_record(&id, revision, |record| {
            record.and_then(|record| record.authentication_session.clone())
        })
        .map_err(|error| error.to_string())?;
    // Clear the effective account before scheduling deletion. Only cancel the
    // reviewed generation so a newly admitted login is not cancelled afterward.
    let old_generation = registry
        .with_current_record(&id, revision, |record| {
            record.map(|record| record.execution_generation)
        })
        .map_err(|error| error.to_string())?
        .ok_or("extension is not installed")?;
    let mut published = false;
    let publication = registry.set_authentication_session_if_current(&id, revision, None, || {
        published = true;
        Some(())
    });
    if !published {
        return publication.map(|_| ()).map_err(|error| error.to_string());
    }
    cancel_extension_generation(&id, old_generation);
    let key = session
        .map(|session| session_key(&id, &session))
        .transpose()?
        .unwrap_or(id);
    let deletion = tokio::task::spawn_blocking(move || delete_token_sync(&key))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?;
    publication.map_err(|error| error.to_string())?;
    deletion
}

async fn confirm(
    app: &AppHandle,
    title: String,
    message: String,
    accept: &str,
) -> Result<(), String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .message(message)
        .title(title)
        .buttons(MessageDialogButtons::OkCancelCustom(
            accept.to_string(),
            "Cancel".into(),
        ))
        .show(move |accepted| {
            let _ = sender.send(accepted);
        });
    if receiver
        .await
        .map_err(|_| "confirmation dialog closed unexpectedly")?
    {
        Ok(())
    } else {
        Err("authentication was cancelled".into())
    }
}

pub(crate) async fn connect_from_extension(
    app: AppHandle,
    extension_id: String,
    worker_generation: u64,
) -> Result<AuthConnection, String> {
    let prepared = prepare_connect(&app, &extension_id)?;
    prepared.check_worker(worker_generation)?;
    let decl = &prepared.declaration;
    confirm(
        &app,
        format!("Connect {}?", decl.provider_name),
        format!(
            "The extension '{extension_id}' wants to connect {}.\n\nScopes: {}\nAPI hosts: {}\n\nGrain will keep the token in your OS credential vault; the extension cannot read it.",
            decl.provider_name,
            decl.scopes.join(", "),
            decl.api_hosts.join(", ")
        ),
        "Connect",
    )
    .await?;
    connect_reviewed(app, extension_id, prepared).await
}

pub(crate) async fn disconnect_from_extension(
    app: AppHandle,
    extension_id: String,
    worker_generation: u64,
) -> Result<(), String> {
    let registry = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?
        .inner()
        .clone();
    let revision = registry.record_revision(&extension_id);
    let generation = registry
        .with_current_record(&extension_id, revision, |record| {
            record.map(|record| record.execution_generation)
        })
        .map_err(|error| error.to_string())?;
    if generation != Some(worker_generation) {
        return Err("extension account or runtime changed; run a fresh tool call".into());
    }
    let decl = declaration(&app, &extension_id)?;
    confirm(
        &app,
        format!("Disconnect {}?", decl.provider_name),
        format!(
            "The extension '{extension_id}' wants to remove its local {} connection.",
            decl.provider_name
        ),
        "Disconnect",
    )
    .await?;
    disconnect_reviewed(registry, extension_id, revision).await
}

pub(crate) async fn access_token(
    app: &AppHandle,
    extension_id: &str,
    host: &str,
    worker_generation: u64,
) -> Result<Zeroizing<String>, String> {
    let prepared = prepare_connect(app, extension_id)?;
    prepared.check_worker(worker_generation)?;
    let decl = &prepared.declaration;
    let session = active_session(&prepared, extension_id)?;
    let key = session_key(extension_id, &session)?;
    if !decl.api_hosts.iter().any(|allowed| allowed == host) {
        return Err(format!("authentication is not allowed for host '{host}'"));
    }
    let mut token = read_token(&key)
        .await?
        .ok_or_else(|| "authentication is not connected".to_string())?;
    prepared.check(extension_id)?;
    check_account_token(&token, &session, decl)?;
    if needs_refresh(&token) {
        let owner = RefreshOwner::acquire(
            REFRESH_LOCKS
                .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
                .clone(),
            key.clone(),
        )?;
        let _guard = owner.lock.lock().await;
        prepared.check(extension_id)?;
        token = read_token(&key)
            .await?
            .ok_or_else(|| "authentication is not connected".to_string())?;
        prepared.check(extension_id)?;
        check_account_token(&token, &session, decl)?;
        if needs_refresh(&token) {
            let refreshed = refresh(decl, &token).await?;
            prepared.check(extension_id)?;
            if approved_declaration(app, extension_id)? != *decl {
                return Err("authentication declaration changed during token refresh".into());
            }
            let write_key = key.clone();
            let write_id = extension_id.to_owned();
            let write_prepared = prepared.clone();
            tokio::task::spawn_blocking(move || {
                write_refreshed_sync(&write_key, &write_id, &write_prepared, &token, &refreshed)
            })
            .await
            .map_err(|error| format!("credential task failed: {error}"))??;
            token = read_token(&key)
                .await?
                .ok_or_else(|| "refreshed credential was not stored".to_string())?;
        }
    }
    check_account_token(&token, &session, decl)?;
    if !token.token_type.eq_ignore_ascii_case("bearer") {
        return Err("only Bearer OAuth tokens are supported".into());
    }
    prepared.check(extension_id)?;
    if approved_declaration(app, extension_id)? != *decl {
        return Err("authentication declaration changed during token access".into());
    }
    Ok(Zeroizing::new(token.access_token.clone()))
}

pub(crate) fn cancel_extension(extension_id: &str) {
    let Some(pending) = PENDING.get() else { return };
    cancel_flow(pending, extension_id, None);
}

pub(crate) fn cancel_extension_generation(extension_id: &str, generation: u64) {
    let Some(pending) = PENDING.get() else { return };
    cancel_flow(pending, extension_id, Some(generation));
}

fn cancel_flow(pending: &PendingFlows, extension_id: &str, generation: Option<u64>) {
    let Ok(mut pending) = pending.lock() else {
        return;
    };
    if generation.is_some_and(|generation| {
        pending
            .get(extension_id)
            .is_none_or(|flow| flow.execution_generation != generation)
    }) {
        return;
    }
    if let Some(flow) = pending.remove(extension_id) {
        let _ = flow.cancel.send(());
    }
}

pub(crate) async fn purge_extension(
    extension_id: &str,
    session: Option<String>,
) -> Result<(), String> {
    let extension_id = extension_id.to_owned();
    tokio::task::spawn_blocking(move || {
        if let Some(session) = session {
            delete_token_sync(&session_key(&extension_id, &session)?)?;
        }
        // No login writes legacy id-only keys. Removing one cannot delete a
        // concurrently connected replacement or the active development grant.
        delete_token_sync(&extension_id)
    })
    .await
    .map_err(|error| format!("credential task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_declaration() -> AuthenticationDecl {
        serde_json::from_value(serde_json::json!({
            "type": "oauth2-pkce", "providerName": "Fixture", "clientId": "client",
            "authorizationEndpoint": "https://login.example.com/authorize",
            "tokenEndpoint": "https://login.example.com/token", "scopes": ["read"],
            "redirectMethods": ["loopback"], "apiHosts": ["api.example.com"]
        }))
        .unwrap()
    }

    fn prepared_fixture() -> (tempfile::TempDir, PreparedConnect) {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Arc::new(grain_core::extensions::ExtensionsRegistry::load(dir.path(), false).unwrap());
        let record = serde_json::from_value(serde_json::json!({
            "id": "com.example.tools", "enabled": true, "installed_version": "1",
            "granted": ["auth"]
        }))
        .unwrap();
        registry.install(record).unwrap();
        let prepared = PreparedConnect {
            revision: registry.record_revision("com.example.tools"),
            execution_generation: registry
                .record("com.example.tools")
                .unwrap()
                .execution_generation,
            registry,
            declaration: test_declaration(),
        };
        (dir, prepared)
    }

    #[tokio::test]
    async fn replaced_sign_in_survives_old_guard_and_generation_cleanup() {
        let flows = Arc::new(Mutex::new(HashMap::new()));
        let (old, cancelled) = PendingGuard::begin(flows.clone(), "tools".into(), 1).unwrap();
        let owner = old.0.clone();
        let (new, mut receiver) = PendingGuard::begin(flows.clone(), "tools".into(), 2).unwrap();
        assert!(cancelled.await.is_ok());
        assert!(owner.publish(|| Ok(())).is_err());
        drop(old);
        cancel_flow(&flows, "tools", Some(1));
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        assert!(new.0.publish(|| Ok(())).is_ok());
        cancel_flow(&flows, "tools", Some(2));
        assert!(receiver.await.is_ok());
        assert!(new.0.publish(|| Ok(())).is_err());
        drop(new);
        assert!(flows.lock().unwrap().is_empty());
    }

    #[test]
    fn sign_in_capacity_allows_replacement_and_releases_on_every_exit() {
        let flows = Arc::new(Mutex::new(HashMap::new()));
        let mut guards = Vec::new();
        for index in 0..MAX_PENDING_CONNECTS {
            let (guard, _) =
                PendingGuard::begin(flows.clone(), format!("tools-{index}"), 1).unwrap();
            guards.push(guard);
        }
        assert!(PendingGuard::begin(flows.clone(), "overflow".into(), 1).is_err());
        let (replacement, _) = PendingGuard::begin(flows.clone(), "tools-0".into(), 2).unwrap();
        drop(guards.remove(0));
        assert_eq!(flows.lock().unwrap().len(), MAX_PENDING_CONNECTS);
        drop(replacement);
        assert!(PendingGuard::begin(flows.clone(), "available".into(), 1).is_ok());
        drop(guards);
        assert!(flows.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancel_timeout_and_future_drop_release_actual_callback_listener() {
        for mode in ["cancel", "timeout", "drop"] {
            let flows = Arc::new(Mutex::new(HashMap::new()));
            let (guard, receiver) = PendingGuard::begin(flows.clone(), "tools".into(), 1).unwrap();
            let owner = guard.0.clone();
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let address = listener.local_addr().unwrap();
            let (started, ready) = tokio::sync::oneshot::channel();
            let budget = if mode == "timeout" {
                Duration::from_millis(25)
            } else {
                CONNECT_TIMEOUT
            };
            let task = tokio::spawn(async move {
                let _guard = guard;
                let _ = started.send(());
                await_connect(
                    receiver,
                    budget,
                    callback(listener, "/cb".into(), "state".into(), address.to_string()),
                )
                .await
            });
            ready.await.unwrap();
            match mode {
                "cancel" => {
                    cancel_flow(&flows, "tools", None);
                    assert!(task.await.unwrap().is_err());
                }
                "timeout" => assert!(task.await.unwrap().is_err()),
                _ => {
                    task.abort();
                    assert!(task.await.is_err());
                }
            }
            assert!(flows.lock().unwrap().is_empty(), "{mode}");
            assert!(owner.publish(|| Ok(())).is_err());
            assert!(
                TcpStream::connect(address).await.is_err(),
                "listener leaked: {mode}"
            );
        }
    }

    #[tokio::test]
    async fn cancellation_after_callback_stops_actual_token_response_and_never_publishes() {
        let flows = Arc::new(Mutex::new(HashMap::new()));
        let (guard, receiver) = PendingGuard::begin(flows.clone(), "tools".into(), 1).unwrap();
        let owner = guard.0.clone();
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let (received, request) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            while !bytes.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let read = stream.read(&mut chunk).await.unwrap();
                assert!(read > 0);
                bytes.extend_from_slice(&chunk[..read]);
                assert!(bytes.len() <= CALLBACK_MAX_BYTES);
            }
            assert!(bytes.starts_with(b"POST /token HTTP/1.1"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            let _ = received.send(());
            // Drain any remaining request body and observe real transport release.
            tokio::time::timeout(Duration::from_secs(2), async {
                let mut bytes = [0; 1024];
                while stream.read(&mut bytes).await.unwrap() != 0 {}
            })
            .await
            .unwrap();
        });
        let task = tokio::spawn(async move {
            let _guard = guard;
            let mut decl = test_declaration();
            decl.token_endpoint = format!("http://{address}/token");
            // The browser callback has already supplied the code. Exercise the
            // production exchange/stream parser without opening a browser.
            await_connect(
                receiver,
                CONNECT_TIMEOUT,
                exchange(&decl, "code", "http://127.0.0.1/cb", "verifier"),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(2), request)
            .await
            .unwrap()
            .unwrap();
        cancel_flow(&flows, "tools", None);
        assert!(task.await.unwrap().is_err());
        server.await.unwrap();
        let writes = std::cell::Cell::new(0);
        assert!(owner
            .publish(|| {
                writes.set(writes.get() + 1);
                Ok(())
            })
            .is_err());
        assert_eq!(writes.get(), 0);
        assert!(flows.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dropped_caller_refuses_already_queued_blocking_publication() {
        let (_dir, prepared) = prepared_fixture();
        let flows = Arc::new(Mutex::new(HashMap::new()));
        let (guard, _) = PendingGuard::begin(
            flows.clone(),
            "com.example.tools".into(),
            prepared.execution_generation,
        )
        .unwrap();
        let owner = guard.0.clone();
        let (release, wait) = std::sync::mpsc::channel();
        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = writes.clone();
        let task = tokio::task::spawn_blocking(move || {
            wait.recv().unwrap();
            publish_connect(&prepared, &owner, || {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
        });
        drop(guard);
        release.send(()).unwrap();
        assert!(task.await.unwrap().is_err());
        assert_eq!(writes.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(flows.lock().unwrap().is_empty());
    }

    #[test]
    fn changed_registry_owner_refuses_sign_in_publication() {
        for mode in ["disable", "reenable", "replace", "uninstall"] {
            let (_dir, prepared) = prepared_fixture();
            let flows = Arc::new(Mutex::new(HashMap::new()));
            let (guard, _) = PendingGuard::begin(
                flows,
                "com.example.tools".into(),
                prepared.execution_generation,
            )
            .unwrap();
            match mode {
                "disable" => {
                    prepared
                        .registry
                        .set_enabled("com.example.tools", false)
                        .unwrap();
                }
                "reenable" => {
                    prepared
                        .registry
                        .set_enabled("com.example.tools", false)
                        .unwrap();
                    prepared
                        .registry
                        .set_enabled("com.example.tools", true)
                        .unwrap();
                }
                "replace" => {
                    prepared
                        .registry
                        .install(prepared.registry.record("com.example.tools").unwrap())
                        .unwrap();
                }
                _ => {
                    prepared.registry.uninstall("com.example.tools").unwrap();
                }
            }
            let writes = std::cell::Cell::new(0);
            assert!(
                publish_connect(&prepared, &guard.0, || {
                    writes.set(1);
                    Ok(())
                })
                .is_err(),
                "{mode}"
            );
            assert_eq!(writes.get(), 0);
        }
    }

    #[test]
    fn token_binding_rejects_legacy_and_changed_provider_client_scopes_or_hosts() {
        let declaration = test_declaration();
        let mut token: TokenSet = serde_json::from_value(serde_json::json!({
            "access_token": "fixture-secret", "refresh_token": "fixture-refresh", "scopes": ["read"]
        }))
        .unwrap();
        assert!(check_token_binding(&token, &declaration).is_err());
        assert!(token.refresh_token.is_some()); // Preserve, never silently bless/delete.
        token.authentication_fingerprint = Some(
            grain_core::extensions::authentication_fingerprint(&declaration),
        );
        let stored = serde_json::to_vec(&token).unwrap();
        let token: TokenSet = serde_json::from_slice(&stored).unwrap();
        assert!(check_token_binding(&token, &declaration).is_ok());
        for mode in ["issuer", "client", "scope", "host"] {
            let mut changed = declaration.clone();
            match mode {
                "issuer" => changed.token_endpoint = "https://other.example.com/token".into(),
                "client" => changed.client_id = "replacement".into(),
                "scope" => changed.scopes.push("write".into()),
                _ => changed.api_hosts.push("other.example.com".into()),
            }
            assert!(check_token_binding(&token, &changed).is_err(), "{mode}");
        }
    }

    #[tokio::test]
    async fn actual_exchange_and_refresh_store_the_reviewed_declaration_binding() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for index in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_callback_request(&mut stream).await.unwrap();
                assert!(request.starts_with(b"POST /token HTTP/1.1"));
                let body = if index == 0 {
                    r#"{"access_token":"fixture-access","token_type":"Bearer","refresh_token":"fixture-refresh","expires_in":3600,"scope":"read extra"}"#
                } else {
                    r#"{"access_token":"fixture-refreshed","token_type":"Bearer","expires_in":3600}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let mut declaration = test_declaration();
        declaration.token_endpoint = format!("http://{address}/token");
        let token = exchange(&declaration, "code", "http://127.0.0.1/cb", "verifier")
            .await
            .unwrap();
        assert!(check_token_binding(&token, &declaration).is_ok());
        let previous = token;
        let token = refresh(&declaration, &previous).await.unwrap();
        assert_eq!(token.account_session, previous.account_session);
        assert_ne!(token.credential_revision, previous.credential_revision);
        assert_eq!(token.scopes, vec!["read", "extra"]);
        assert!(check_token_binding(&token, &declaration).is_ok());
        assert_eq!(token.refresh_token.as_deref(), Some("fixture-refresh"));
        server.await.unwrap();
    }

    fn account_token() -> TokenSet {
        serde_json::from_value(serde_json::json!({
            "authentication_fingerprint": grain_core::extensions::authentication_fingerprint(&test_declaration()),
            "account_session": "a".repeat(32), "credential_revision": "b".repeat(32),
            "access_token": "fixture-access", "refresh_token": "fixture-refresh", "scopes": ["read"]
        })).unwrap()
    }

    #[test]
    fn refresh_publication_refuses_deleted_or_replaced_credentials() {
        let (_dir, prepared) = prepared_fixture();
        let previous = account_token();
        let mut current = previous.clone();
        let mut writes = 0;
        assert!(
            publish_refresh(&prepared, "com.example.tools", &previous, None, || {
                writes += 1;
                Ok(())
            })
            .is_err()
        );
        current.credential_revision = Some("c".repeat(32));
        assert!(publish_refresh(
            &prepared,
            "com.example.tools",
            &previous,
            Some(&current),
            || {
                writes += 1;
                Ok(())
            }
        )
        .is_err());
        current = previous.clone();
        current.account_session = Some("c".repeat(32));
        assert!(publish_refresh(
            &prepared,
            "com.example.tools",
            &previous,
            Some(&current),
            || {
                writes += 1;
                Ok(())
            }
        )
        .is_err());
        assert_eq!(writes, 0);
        publish_refresh(
            &prepared,
            "com.example.tools",
            &previous,
            Some(&previous),
            || {
                writes += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(writes, 1);
    }

    #[tokio::test]
    async fn actual_refresh_response_cannot_publish_after_logout_or_account_switch() {
        for replacement in [None, Some("c".repeat(32))] {
            let (_dir, mut prepared) = prepared_fixture();
            let id = "com.example.tools";
            prepared
                .registry
                .set_authentication_session_if_current(
                    id,
                    prepared.revision,
                    Some("a".repeat(32)),
                    || Some(()),
                )
                .unwrap();
            prepared.revision = prepared.registry.record_revision(id);
            prepared.execution_generation =
                prepared.registry.record(id).unwrap().execution_generation;
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            prepared.declaration.token_endpoint =
                format!("http://{}/token", listener.local_addr().unwrap());
            let (received_tx, received_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_callback_request(&mut stream).await.unwrap();
                assert!(request.starts_with(b"POST /token HTTP/1.1"));
                received_tx.send(()).unwrap();
                release_rx.await.unwrap();
                let body = r#"{"access_token":"late-response","token_type":"Bearer"}"#;
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
            let previous = account_token();
            let declaration = prepared.declaration.clone();
            let work_token = previous.clone();
            let refresh_task =
                tokio::spawn(async move { refresh(&declaration, &work_token).await });
            received_rx.await.unwrap();
            prepared
                .registry
                .set_authentication_session_if_current(
                    id,
                    prepared.revision,
                    replacement.clone(),
                    || Some(()),
                )
                .unwrap();
            release_tx.send(()).unwrap();
            let refreshed = refresh_task.await.unwrap().unwrap();
            assert_eq!(refreshed.account_session, previous.account_session);
            let mut writes = 0;
            assert!(
                publish_refresh(&prepared, id, &previous, Some(&previous), || {
                    writes += 1;
                    Ok(())
                })
                .is_err()
            );
            assert_eq!(writes, 0);
            assert_eq!(
                prepared.registry.record(id).unwrap().authentication_session,
                replacement
            );
            server.await.unwrap();
        }
    }

    #[test]
    fn cancelled_candidate_never_becomes_the_active_account() {
        let (_dir, prepared) = prepared_fixture();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (guard, _cancel) = PendingGuard::begin(
            pending.clone(),
            "com.example.tools".into(),
            prepared.execution_generation,
        )
        .unwrap();
        let owner = guard.0.clone();
        // Represents a candidate already saved to its own key before the async
        // caller disappears. It is not the active credential until publication.
        let key = session_key(&owner.id, &"a".repeat(32)).unwrap();
        drop(guard);
        assert!(prepared
            .registry
            .set_authentication_session_if_current(
                &owner.id,
                prepared.revision,
                Some("a".repeat(32)),
                || owner.current_guard()
            )
            .is_err());
        assert!(prepared
            .registry
            .record(&owner.id)
            .unwrap()
            .authentication_session
            .is_none());
        assert_ne!(key, session_key(&owner.id, &"c".repeat(32)).unwrap());
    }

    #[test]
    fn account_token_and_worker_must_match_the_prepared_session() {
        let (_dir, prepared) = prepared_fixture();
        assert!(prepared.check_worker(prepared.execution_generation).is_ok());
        assert!(prepared
            .check_worker(prepared.execution_generation + 1)
            .is_err());
        let token = account_token();
        assert!(check_account_token(&token, &"a".repeat(32), &test_declaration()).is_ok());
        assert!(check_account_token(&token, &"c".repeat(32), &test_declaration()).is_err());
        assert!(session_key("com.example.tools", "../token").is_err());
        let mut legacy = token;
        legacy.credential_revision = None;
        assert!(check_account_token(&legacy, &"a".repeat(32), &test_declaration()).is_err());
    }

    #[tokio::test]
    async fn refresh_locks_coalesce_per_session_without_retaining_idle_state() {
        let pool = Arc::new(Mutex::new(HashMap::new()));
        let owner = RefreshOwner::acquire(pool.clone(), "account-a".into()).unwrap();
        let same = RefreshOwner::acquire(pool.clone(), "account-a".into()).unwrap();
        assert!(Arc::ptr_eq(&owner.lock, &same.lock));
        let guard = owner.lock.lock().await;
        assert!(same.lock.try_lock().is_err());
        let independent = RefreshOwner::acquire(pool.clone(), "account-b".into()).unwrap();
        assert!(independent.lock.try_lock().is_ok());
        drop(independent);
        drop(guard);
        drop(owner);
        assert_eq!(pool.lock().unwrap().len(), 1);
        drop(same);
        assert!(pool.lock().unwrap().is_empty());
        for _ in 0..100 {
            drop(RefreshOwner::acquire(pool.clone(), "account-a".into()).unwrap());
            assert!(pool.lock().unwrap().is_empty());
        }
        let mut owners = Vec::new();
        for index in 0..MAX_PENDING_CONNECTS {
            owners.push(RefreshOwner::acquire(pool.clone(), index.to_string()).unwrap());
        }
        assert!(RefreshOwner::acquire(pool.clone(), "overflow".into()).is_err());
        drop(owners);
        assert!(pool.lock().unwrap().is_empty());
    }

    #[test]
    fn authorization_url_forces_pkce_and_reserved_fields() {
        let decl = AuthenticationDecl {
            auth_type: grain_sdk::AuthenticationType::OAuth2Pkce,
            provider_name: "GitHub".into(),
            client_id: "client".into(),
            authorization_endpoint: "https://github.com/login/oauth/authorize".into(),
            token_endpoint: "https://github.com/login/oauth/access_token".into(),
            scopes: vec!["read:user".into()],
            redirect_methods: vec![grain_sdk::RedirectMethod::Loopback],
            api_hosts: vec!["api.github.com".into()],
            authorization_parameters: Default::default(),
        };
        let url = Url::parse(
            &authorize_url(&decl, "http://127.0.0.1:1/cb", "state", "challenge").unwrap(),
        )
        .unwrap();
        let query = url
            .query_pairs()
            .into_owned()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(query.get("state").map(String::as_str), Some("state"));
    }

    #[test]
    fn pkce_s256_matches_rfc_7636_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[tokio::test]
    async fn loopback_callback_rejects_noise_then_accepts_exact_state() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(callback(
            listener,
            "/grain/oauth/random".into(),
            "expected".into(),
            format!("127.0.0.1:{port}"),
        ));

        let mut noise = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        noise
            .write_all(
                format!("GET /wrong?state=no HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        drop(noise);

        let mut duplicate = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        duplicate
            .write_all(
                format!(
                    "GET /grain/oauth/random?code=forged&state=expected&state=expected HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        drop(duplicate);

        let mut valid = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        valid
            .write_all(format!("GET /grain/oauth/random?code=abc&state=expected HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n").as_bytes())
            .await
            .unwrap();
        valid.write_all(b"\r\n").await.unwrap();
        assert_eq!(task.await.unwrap().unwrap(), "abc");
    }
}
