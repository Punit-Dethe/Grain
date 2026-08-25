//! Host-owned extension OAuth 2.0 Authorization Code + PKCE.
//!
//! Tokens never cross the extension boundary. They are stored only in the OS
//! credential vault and are attached by `host_api::net.fetch` after exact-host
//! authorization. There is no idle service: the loopback listener exists only
//! while a connect flow is active and is dropped on every exit path.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
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
const CONNECTION_ID: &str = "default";
const VAULT_INDEX_ID: &str = "__auth_index__";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(300);
const CALLBACK_MAX_BYTES: usize = 16 * 1024;
const TOKEN_MAX_BYTES: usize = 64 * 1024;
const EXPIRY_SKEW_SECS: u64 = 60;
struct PendingFlow {
    run_id: uuid::Uuid,
    cancel: tokio::sync::oneshot::Sender<()>,
}

static PENDING: OnceLock<Mutex<HashMap<String, PendingFlow>>> = OnceLock::new();
static REFRESH_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static VAULT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone, Serialize, specta::Type)]
pub struct AuthConnection {
    pub id: String,
    pub provider_name: String,
    pub authorization_host: String,
    pub token_host: String,
    pub scopes: Vec<String>,
    pub api_hosts: Vec<String>,
    pub connection_id: String,
    /// `connected` | `needs_reauthorization` | `expired` | `disconnected` | `unavailable`
    pub state: String,
    pub granted_scopes: Vec<String>,
    /// Unix seconds, sent as a string to avoid JS integer loss.
    pub expires_at: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct TokenSet {
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

fn vault_user(extension_id: &str, auth_id: &str) -> String {
    format!("{extension_id}:{auth_id}:{CONNECTION_ID}")
}

fn vault_entry(extension_id: &str, auth_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(VAULT_SERVICE, &vault_user(extension_id, auth_id))
        .map_err(|error| format!("OS credential vault unavailable: {error}"))
}

fn read_index_unlocked(extension_id: &str) -> Result<Vec<String>, String> {
    let entry = vault_entry(extension_id, VAULT_INDEX_ID)?;
    let mut bytes = match entry.get_secret() {
        Ok(bytes) => bytes,
        Err(keyring::Error::NoEntry) => return Ok(Vec::new()),
        Err(error) => return Err(format!("OS credential vault index read failed: {error}")),
    };
    let result = if bytes.len() > 4096 {
        Err("stored OAuth credential index is too large".into())
    } else {
        serde_json::from_slice::<Vec<String>>(&bytes)
            .map_err(|error| format!("stored OAuth credential index is invalid: {error}"))
            .and_then(|ids| {
                if ids.len() > 8
                    || ids.iter().any(|id| {
                        grain_sdk::authentication_capability_id(&format!("auth:{id}")).is_none()
                    })
                {
                    Err("stored OAuth credential index contains invalid ids".into())
                } else {
                    Ok(ids)
                }
            })
    };
    bytes.zeroize();
    result
}

fn write_index_unlocked(extension_id: &str, ids: &[String]) -> Result<(), String> {
    let entry = vault_entry(extension_id, VAULT_INDEX_ID)?;
    if ids.is_empty() {
        return match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(format!("OS credential vault index delete failed: {error}")),
        };
    }
    let mut bytes = serde_json::to_vec(ids).map_err(|error| error.to_string())?;
    let result = entry
        .set_secret(&bytes)
        .map_err(|error| format!("OS credential vault index write failed: {error}"));
    bytes.zeroize();
    result
}

fn read_token_sync(extension_id: &str, auth_id: &str) -> Result<Option<TokenSet>, String> {
    let _guard = VAULT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "OS credential vault lock is unavailable")?;
    let entry = vault_entry(extension_id, auth_id)?;
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

fn write_token_sync(extension_id: &str, auth_id: &str, token: &TokenSet) -> Result<(), String> {
    let _guard = VAULT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "OS credential vault lock is unavailable")?;
    // Keep a vault-resident inventory so uninstall can remove credentials for
    // declarations that a later manifest version no longer contains.
    let mut ids = read_index_unlocked(extension_id)?;
    if !ids.iter().any(|id| id == auth_id) {
        ids.push(auth_id.to_owned());
        ids.sort();
        write_index_unlocked(extension_id, &ids)?;
    }
    let entry = vault_entry(extension_id, auth_id)?;
    let mut bytes = serde_json::to_vec(token).map_err(|error| error.to_string())?;
    let result = entry
        .set_secret(&bytes)
        .map_err(|error| format!("OS credential vault write failed: {error}"));
    bytes.zeroize();
    result
}

fn delete_token_sync(extension_id: &str, auth_id: &str) -> Result<(), String> {
    let _guard = VAULT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "OS credential vault lock is unavailable")?;
    let entry = vault_entry(extension_id, auth_id)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(error) => return Err(format!("OS credential vault delete failed: {error}")),
    }
    let mut ids = read_index_unlocked(extension_id)?;
    ids.retain(|id| id != auth_id);
    write_index_unlocked(extension_id, &ids)
}

fn purge_extension_sync(extension_id: &str) -> Result<(), String> {
    let _guard = VAULT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "OS credential vault lock is unavailable")?;
    let ids = read_index_unlocked(extension_id)?;
    for auth_id in &ids {
        let entry = vault_entry(extension_id, auth_id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(format!("OS credential vault delete failed: {error}")),
        }
    }
    write_index_unlocked(extension_id, &[])
}

async fn read_token(extension_id: &str, auth_id: &str) -> Result<Option<TokenSet>, String> {
    let extension_id = extension_id.to_owned();
    let auth_id = auth_id.to_owned();
    tokio::task::spawn_blocking(move || read_token_sync(&extension_id, &auth_id))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?
}

async fn write_token(extension_id: &str, auth_id: &str, token: TokenSet) -> Result<(), String> {
    let extension_id = extension_id.to_owned();
    let auth_id = auth_id.to_owned();
    tokio::task::spawn_blocking(move || write_token_sync(&extension_id, &auth_id, &token))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?
}

fn declaration(
    app: &AppHandle,
    extension_id: &str,
    auth_id: &str,
) -> Result<AuthenticationDecl, String> {
    crate::grain_commands::load_pack(app, extension_id)?
        .manifest
        .contributes
        .authentication
        .into_iter()
        .find(|decl| decl.id == auth_id)
        .ok_or_else(|| format!("authentication '{auth_id}' is not declared by '{extension_id}'"))
}

fn approved_declaration(
    app: &AppHandle,
    extension_id: &str,
    auth_id: &str,
) -> Result<AuthenticationDecl, String> {
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
    let capability = format!("auth:{auth_id}");
    if !record.granted.contains(&capability) {
        return Err(format!("capability '{capability}' is not granted"));
    }
    let fingerprint = grain_core::extensions::authentication_fingerprint(
        &pack.manifest.contributes.authentication,
    );
    if record.authentication_approved.as_deref() != Some(fingerprint.as_str()) {
        return Err(format!(
            "authentication declaration for '{extension_id}' changed and requires approval"
        ));
    }
    pack.manifest
        .contributes
        .authentication
        .into_iter()
        .find(|decl| decl.id == auth_id)
        .ok_or_else(|| format!("authentication '{auth_id}' is not declared by '{extension_id}'"))
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

async fn connection_for(extension_id: &str, decl: &AuthenticationDecl) -> AuthConnection {
    let (state, granted_scopes, expires_at) = match read_token(extension_id, &decl.id).await {
        Ok(Some(token)) => {
            let state = if !decl.scopes.iter().all(|scope| token.scopes.contains(scope)) {
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
        id: decl.id.clone(),
        provider_name: decl.provider_name.clone(),
        authorization_host: endpoint_host(&decl.authorization_endpoint),
        token_host: endpoint_host(&decl.token_endpoint),
        scopes: decl.scopes.clone(),
        api_hosts: decl.api_hosts.clone(),
        connection_id: CONNECTION_ID.into(),
        state,
        granted_scopes,
        expires_at,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn extension_auth_connections(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<Vec<AuthConnection>, String> {
    crate::grain_commands::require_main_window(&window)?;
    connections(&app, &id).await
}

pub(crate) async fn connections(app: &AppHandle, id: &str) -> Result<Vec<AuthConnection>, String> {
    let declarations = crate::grain_commands::load_pack(app, id)?
        .manifest
        .contributes
        .authentication;
    let mut out = Vec::with_capacity(declarations.len());
    for decl in &declarations {
        out.push(connection_for(id, decl).await);
    }
    Ok(out)
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

async fn refresh(decl: &AuthenticationDecl, refresh_token: &str) -> Result<TokenSet, String> {
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
            .unwrap_or_else(|| decl.scopes.clone()),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn extension_auth_connect(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    auth_id: String,
) -> Result<AuthConnection, String> {
    crate::grain_commands::require_main_window(&window)?;
    connect(app, id, auth_id).await
}

async fn connect(app: AppHandle, id: String, auth_id: String) -> Result<AuthConnection, String> {
    let decl = approved_declaration(&app, &id, &auth_id)?;
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
    let url = authorize_url(&decl, &redirect, &state, &challenge)?;
    let pending_key = format!("{id}:{auth_id}");
    let run_id = uuid::Uuid::new_v4();
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    if let Some(previous) = PENDING
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "authentication cancellation state is unavailable")?
        .insert(
            pending_key.clone(),
            PendingFlow {
                run_id,
                cancel: cancel_tx,
            },
        )
    {
        let _ = previous.cancel.send(());
    }
    if let Err(error) = app.opener().open_url(url, None::<String>) {
        if let Ok(mut pending) = PENDING.get().unwrap().lock() {
            if pending
                .get(&pending_key)
                .is_some_and(|flow| flow.run_id == run_id)
            {
                pending.remove(&pending_key);
            }
        }
        return Err(format!("could not open the sign-in page: {error}"));
    }
    let outcome = tokio::select! {
        result = tokio::time::timeout(CONNECT_TIMEOUT, callback(listener, path, state, format!("127.0.0.1:{port}"))) => {
            result.map_err(|_| "authentication timed out after 5 minutes".to_string()).and_then(|result| result)
        }
        _ = cancel_rx => Err("authentication was cancelled".into()),
    };
    {
        let mut pending = PENDING
            .get()
            .unwrap()
            .lock()
            .map_err(|_| "authentication cancellation state is unavailable")?;
        if pending
            .get(&pending_key)
            .is_some_and(|flow| flow.run_id == run_id)
        {
            pending.remove(&pending_key);
        }
    }
    let code = outcome?;
    let token = exchange(&decl, &code, &redirect, verifier.as_str()).await?;
    if approved_declaration(&app, &id, &auth_id)? != decl {
        return Err("authentication declaration changed during sign-in".into());
    }
    write_token(&id, &auth_id, token).await?;
    Ok(connection_for(&id, &decl).await)
}

#[tauri::command]
#[specta::specta]
pub async fn extension_auth_disconnect(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    auth_id: String,
) -> Result<(), String> {
    crate::grain_commands::require_main_window(&window)?;
    disconnect(&app, id, auth_id).await
}

async fn disconnect(app: &AppHandle, id: String, auth_id: String) -> Result<(), String> {
    declaration(app, &id, &auth_id)?;
    let id_copy = id.clone();
    tokio::task::spawn_blocking(move || delete_token_sync(&id_copy, &auth_id))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?
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
    auth_id: String,
) -> Result<AuthConnection, String> {
    let decl = approved_declaration(&app, &extension_id, &auth_id)?;
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
    connect(app, extension_id, auth_id).await
}

pub(crate) async fn disconnect_from_extension(
    app: AppHandle,
    extension_id: String,
    auth_id: String,
) -> Result<(), String> {
    let decl = declaration(&app, &extension_id, &auth_id)?;
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
    disconnect(&app, extension_id, auth_id).await
}

pub(crate) async fn access_token(
    app: &AppHandle,
    extension_id: &str,
    auth_id: &str,
    host: &str,
) -> Result<Zeroizing<String>, String> {
    let decl = approved_declaration(app, extension_id, auth_id)?;
    if !decl.api_hosts.iter().any(|allowed| allowed == host) {
        return Err(format!(
            "authentication '{auth_id}' is not allowed for host '{host}'"
        ));
    }
    let mut token = read_token(extension_id, auth_id)
        .await?
        .ok_or_else(|| format!("authentication '{auth_id}' is not connected"))?;
    if !decl.scopes.iter().all(|scope| token.scopes.contains(scope)) {
        return Err(format!(
            "authentication '{auth_id}' needs reauthorization for its declared scopes"
        ));
    }
    if needs_refresh(&token) {
        let _guard = REFRESH_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        token = read_token(extension_id, auth_id)
            .await?
            .ok_or_else(|| format!("authentication '{auth_id}' is not connected"))?;
        if needs_refresh(&token) {
            let refresh_token = token.refresh_token.as_deref().ok_or_else(|| {
                format!(
                    "authentication '{auth_id}' has expired; reconnect it in extension settings"
                )
            })?;
            let refreshed = refresh(&decl, refresh_token).await?;
            if approved_declaration(app, extension_id, auth_id)? != decl {
                return Err("authentication declaration changed during token refresh".into());
            }
            write_token(extension_id, auth_id, refreshed).await?;
            token = read_token(extension_id, auth_id)
                .await?
                .ok_or_else(|| "refreshed credential was not stored".to_string())?;
        }
    }
    if !decl.scopes.iter().all(|scope| token.scopes.contains(scope)) {
        return Err(format!(
            "authentication '{auth_id}' needs reauthorization for its declared scopes"
        ));
    }
    if !token.token_type.eq_ignore_ascii_case("bearer") {
        return Err("only Bearer OAuth tokens are supported".into());
    }
    if approved_declaration(app, extension_id, auth_id)? != decl {
        return Err("authentication declaration changed during token access".into());
    }
    Ok(Zeroizing::new(token.access_token.clone()))
}

pub(crate) fn cancel_extension(extension_id: &str) {
    let Some(pending) = PENDING.get() else { return };
    let Ok(mut pending) = pending.lock() else {
        return;
    };
    let prefix = format!("{extension_id}:");
    let keys = pending
        .keys()
        .filter(|key| key.starts_with(&prefix))
        .cloned()
        .collect::<Vec<_>>();
    for key in keys {
        if let Some(flow) = pending.remove(&key) {
            let _ = flow.cancel.send(());
        }
    }
}

pub(crate) async fn purge_extension(_app: &AppHandle, extension_id: &str) -> Result<(), String> {
    let extension_id = extension_id.to_owned();
    tokio::task::spawn_blocking(move || purge_extension_sync(&extension_id))
        .await
        .map_err(|error| format!("credential task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_url_forces_pkce_and_reserved_fields() {
        let decl = AuthenticationDecl {
            id: "github".into(),
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
