//! [GRAIN] The host API router (SPEC §1.3) — Phase 2.
//!
//! Every method an extension worker can call, each behind a **capability check
//! first** (return a "not granted" error, never partial data). The check reads
//! the connection's [`ClientIdentity`] — resolved server-side from its token —
//! so a worker can never reach a method it wasn't granted, regardless of what
//! its JS does.
//!
//! Testability: the security-critical pieces are pure and unit-tested here
//! ([`has_capability`], [`required_capability`], [`ExtStorage`]). [`dispatch`]
//! itself is a thin router that needs `AppHandle` state and is exercised at
//! integration level.
//!
//! Rate-limiting is enforced at the per-connection frame-read boundary in
//! `events_server` (where the connection lives), not here — a pure dispatcher
//! has no connection to meter.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::StreamExt;
use grain_sdk::{HostError, HostErrorCode};
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_LENGTH, LOCATION,
};
use reqwest::{Method, StatusCode, Url};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use zeroize::{Zeroize, Zeroizing};

use crate::events_auth::{CapabilitySet, ClientIdentity};

/// Per-extension storage quota (SPEC §3.4).
const STORAGE_QUOTA_BYTES: u64 = 200 * 1024 * 1024;

/// Extension egress is intentionally small and bounded. API-shaped responses
/// fit comfortably; bulk transfer belongs in a purpose-built host capability.
const NET_TIMEOUT: Duration = Duration::from_secs(15);
const NET_MAX_REDIRECTS: usize = 5;
const NET_MAX_REQUEST_BYTES: usize = 1024 * 1024;
const NET_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const NET_MAX_HEADERS: usize = 64;

static EXTENSION_HTTP: OnceLock<reqwest::Client> = OnceLock::new();

pub(crate) const SECRET_REDACTED: &str = "[REDACTED]";

pub(crate) fn extension_secret_key(ext_id: &str, key: &str) -> String {
    format!("ext.{ext_id}.{key}")
}

type HostResult<T> = Result<T, HostError>;

#[derive(Debug)]
pub enum ExtStorageError {
    Quota(String),
    Io(String),
    Serialization(String),
}

impl fmt::Display for ExtStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quota(message) | Self::Io(message) | Self::Serialization(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for ExtStorageError {}

type StorageResult<T> = Result<T, ExtStorageError>;

/// Does this identity hold `cap`? Workers hold exactly their granted named set.
pub fn has_capability(identity: &ClientIdentity, cap: &str) -> bool {
    match &identity.caps {
        CapabilitySet::Named(set) => set.contains(cap),
    }
}

/// The capability a method requires, or `None` for always-allowed methods
/// (`log.*`). Pure, so the gating table is testable without app state.
pub fn required_capability(method: &str) -> Option<&'static str> {
    match method {
        "log.info" | "log.warn" => None,
        "storage.get" | "storage.set" | "storage.delete" => Some("storage"),
        "net.fetch" => Some("__dynamic_net__"),
        "auth.status" | "auth.connect" | "auth.disconnect" => Some("auth"),
        _ => Some("__unknown__"),
    }
}

/// A per-extension JSON key/value store: `<data>/extensions/<id>.storage.json`.
/// Loaded and saved whole (fine at this scale). Pure over its path, so quota
/// and round-trip behavior are unit-tested directly.
pub struct ExtStorage {
    path: PathBuf,
}

impl ExtStorage {
    pub fn new(data_dir: &std::path::Path, ext_id: &str) -> Self {
        Self {
            path: data_dir
                .join("extensions")
                .join(format!("{ext_id}.storage.json")),
        }
    }

    /// Uninstall boundary: remove the storage file. A missing file is already
    /// clean and therefore succeeds.
    pub fn purge(&self) -> StorageResult<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ExtStorageError::Io(error.to_string())),
        }
    }

    fn load(&self) -> StorageResult<BTreeMap<String, Value>> {
        let raw = match std::fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new())
            }
            Err(error) => return Err(ExtStorageError::Io(error.to_string())),
        };
        serde_json::from_str(&raw)
            .map_err(|error| ExtStorageError::Serialization(error.to_string()))
    }

    fn save(&self, map: &BTreeMap<String, Value>) -> StorageResult<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| ExtStorageError::Io(error.to_string()))?;
        }
        let json = serde_json::to_string(map)
            .map_err(|error| ExtStorageError::Serialization(error.to_string()))?;
        if json.len() as u64 > STORAGE_QUOTA_BYTES {
            return Err(ExtStorageError::Quota(format!(
                "storage quota exceeded ({} MB max)",
                STORAGE_QUOTA_BYTES / (1024 * 1024)
            )));
        }
        std::fs::write(&self.path, json).map_err(|error| ExtStorageError::Io(error.to_string()))
    }

    pub fn get(&self, key: &str) -> StorageResult<Value> {
        Ok(self.load()?.get(key).cloned().unwrap_or(Value::Null))
    }

    pub fn set(&self, key: &str, value: Value) -> StorageResult<()> {
        let mut map = self.load()?;
        map.insert(key.to_string(), value);
        self.save(&map)
    }

    pub fn delete(&self, key: &str) -> StorageResult<()> {
        let mut map = self.load()?;
        map.remove(key);
        self.save(&map)
    }
}

fn typed_error(
    code: HostErrorCode,
    message: impl Into<String>,
    hint: impl Into<String>,
) -> HostError {
    HostError::new(code, message, hint)
}

fn invalid_argument(message: impl Into<String>) -> HostError {
    typed_error(
        HostErrorCode::InvalidArgument,
        message,
        "Correct the call arguments and try again.",
    )
}

fn internal_error(message: impl Into<String>) -> HostError {
    typed_error(
        HostErrorCode::Internal,
        message,
        "Retry the call. If it keeps failing, copy the Extensions > Developer log.",
    )
}

fn unavailable(message: impl Into<String>, hint: impl Into<String>) -> HostError {
    typed_error(HostErrorCode::Unavailable, message, hint)
}

fn service_error(service: &str, message: String) -> HostError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        typed_error(
            HostErrorCode::Timeout,
            format!("{service} timed out: {message}"),
            "Retry the call or reduce the request size.",
        )
    } else {
        unavailable(
            format!("{service} is unavailable: {message}"),
            format!("Check the {service} configuration, then retry the call."),
        )
    }
}

fn net_url_and_capability(raw_url: &str) -> HostResult<(Url, String)> {
    let url =
        Url::parse(raw_url).map_err(|error| invalid_argument(format!("invalid URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid_argument("network URLs must use http or https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_argument(
            "network URLs must not contain credentials",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| invalid_argument("network URL must contain a host"))?
        .to_ascii_lowercase();
    Ok((url, format!("net:{host}")))
}

fn authorize_net_url(identity: &ClientIdentity, raw_url: &str) -> HostResult<Url> {
    let (url, capability) = net_url_and_capability(raw_url)?;
    if has_capability(identity, &capability) {
        Ok(url)
    } else {
        Err(HostError::capability_denied(&capability, "net.fetch"))
    }
}

fn storage_error(error: ExtStorageError) -> HostError {
    match error {
        ExtStorageError::Quota(message) => typed_error(
            HostErrorCode::Quota,
            message,
            "Delete extension storage you no longer need, then retry the write.",
        ),
        ExtStorageError::Io(message) | ExtStorageError::Serialization(message) => {
            internal_error(format!("extension storage failed: {message}"))
        }
    }
}

fn param_str(params: &Value, key: &str) -> HostResult<String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| invalid_argument(format!("'{key}' must be a string")))
}

fn param_value(params: &Value, key: &str) -> HostResult<Value> {
    params
        .get(key)
        .cloned()
        .ok_or_else(|| invalid_argument(format!("missing required '{key}' value")))
}

fn param_nonempty_str(params: &Value, key: &str) -> HostResult<String> {
    let value = param_str(params, key)?;
    if value.trim().is_empty() {
        Err(invalid_argument(format!("'{key}' must not be empty")))
    } else {
        Ok(value)
    }
}

fn authorize(identity: &ClientIdentity, method: &str, params: &Value) -> HostResult<()> {
    // The reduced profile applies before capability checks. Even a stale token
    // containing a retired grant (or All) cannot restore Grain internals.
    if !matches!(
        method,
        "log.info"
            | "log.warn"
            | "storage.get"
            | "storage.set"
            | "storage.delete"
            | "net.fetch"
            | "auth.status"
            | "auth.connect"
            | "auth.disconnect"
    ) {
        return Err(unknown_method(method));
    }
    if method == "net.fetch" {
        authorize_net_url(identity, &param_nonempty_str(params, "url")?)?;
        return Ok(());
    }
    match required_capability(method) {
        Some("__unknown__") => Err(unknown_method(method)),
        Some(capability) if !has_capability(identity, capability) => {
            Err(HostError::capability_denied(capability, method))
        }
        _ => Ok(()),
    }
}

fn unknown_method(method: &str) -> HostError {
    typed_error(
        HostErrorCode::UnknownMethod,
        format!("Unknown host method '{method}'."),
        "Use a method declared by the current grain.d.ts SDK.",
    )
}

/// Validate every request field before touching app state. This is the refusal
/// boundary: malformed calls always receive a typed error, never `ok: null`.
fn validate_request(method: &str, params: &Value) -> HostResult<()> {
    match method {
        "log.info" | "log.warn" => {
            param_str(params, "msg")?;
        }
        "storage.get" | "storage.delete" => {
            param_nonempty_str(params, "key")?;
        }
        "storage.set" => {
            param_nonempty_str(params, "key")?;
            param_value(params, "value")?;
        }
        "net.fetch" => {
            param_nonempty_str(params, "url")?;
            if let Some(method) = params.get("method") {
                if !method.is_string() {
                    return Err(invalid_argument("'method' must be a string"));
                }
            }
            if let Some(headers) = params.get("headers") {
                let headers = headers
                    .as_object()
                    .ok_or_else(|| invalid_argument("'headers' must be an object"))?;
                if headers.len() > NET_MAX_HEADERS {
                    return Err(invalid_argument(format!(
                        "'headers' accepts at most {NET_MAX_HEADERS} entries"
                    )));
                }
                if headers.values().any(|value| !value.is_string()) {
                    return Err(invalid_argument("every header value must be a string"));
                }
            }
            if let Some(body) = params.get("body") {
                let body = body
                    .as_str()
                    .ok_or_else(|| invalid_argument("'body' must be a string"))?;
                if body.len() > NET_MAX_REQUEST_BYTES {
                    return Err(invalid_argument(format!(
                        "'body' exceeds the {NET_MAX_REQUEST_BYTES}-byte request limit"
                    )));
                }
            }
            if let Some(secret) = params.get("secret") {
                let secret = secret
                    .as_object()
                    .ok_or_else(|| invalid_argument("'secret' must be an object"))?;
                for key in ["key", "header"] {
                    if !secret.get(key).is_some_and(Value::is_string) {
                        return Err(invalid_argument(format!("'secret.{key}' must be a string")));
                    }
                }
                if secret.get("prefix").is_some_and(|value| !value.is_string()) {
                    return Err(invalid_argument("'secret.prefix' must be a string"));
                }
            }
            if let Some(auth) = params.get("auth") {
                if auth.as_bool() != Some(true) {
                    return Err(invalid_argument("'auth' must be true when provided"));
                }
                if params.get("secret").is_some() {
                    return Err(invalid_argument(
                        "'auth' and 'secret' are mutually exclusive",
                    ));
                }
            }
        }
        "auth.status" | "auth.connect" | "auth.disconnect" => {}
        _ => return Err(unknown_method(method)),
    }
    Ok(())
}

fn preflight(identity: &ClientIdentity, method: &str, params: &Value) -> HostResult<()> {
    authorize(identity, method, params)?;
    validate_request(method, params)?;
    Ok(())
}

fn extension_http_client() -> &'static reqwest::Client {
    EXTENSION_HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(NET_TIMEOUT)
            // Redirects are followed manually so every Location is checked
            // against the same exact-host grant before any bytes are sent.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Grain-Extension-Proxy/1")
            .build()
            .expect("extension HTTP client configuration is valid")
    })
}

fn response_too_large() -> HostError {
    typed_error(
        HostErrorCode::ResponseTooLarge,
        format!("network response exceeds the {NET_MAX_RESPONSE_BYTES}-byte limit"),
        "Request a smaller response or use an endpoint with pagination.",
    )
}

fn net_method(params: &Value) -> HostResult<Method> {
    let raw = params
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    match raw.as_str() {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" => {
            Method::from_bytes(raw.as_bytes())
                .map_err(|error| invalid_argument(format!("invalid HTTP method: {error}")))
        }
        _ => Err(invalid_argument(format!(
            "HTTP method '{raw}' is not supported"
        ))),
    }
}

fn header_is_host_controlled(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "proxy-authorization"
            | "proxy-connection"
            | "upgrade"
    )
}

fn net_headers(params: &Value) -> HostResult<HeaderMap> {
    let Some(values) = params.get("headers") else {
        return Ok(HeaderMap::new());
    };
    let values = values
        .as_object()
        .ok_or_else(|| invalid_argument("'headers' must be an object"))?;
    let mut headers = HeaderMap::with_capacity(values.len());
    for (raw_name, raw_value) in values {
        let name = HeaderName::from_bytes(raw_name.as_bytes())
            .map_err(|error| invalid_argument(format!("invalid header '{raw_name}': {error}")))?;
        if header_is_host_controlled(&name) {
            return Err(invalid_argument(format!(
                "header '{}' is controlled by the host",
                name.as_str()
            )));
        }
        let value =
            HeaderValue::from_str(raw_value.as_str().unwrap_or_default()).map_err(|error| {
                invalid_argument(format!("invalid value for header '{raw_name}': {error}"))
            })?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn resolve_net_secret(
    app: &AppHandle,
    ctx: &grain_core::AppContext,
    identity: &ClientIdentity,
    params: &Value,
) -> HostResult<Option<(HeaderName, HeaderValue)>> {
    let Some(secret) = params.get("secret") else {
        return Ok(None);
    };
    let secret = secret
        .as_object()
        .ok_or_else(|| invalid_argument("'secret' must be an object"))?;
    let key = secret
        .get("key")
        .and_then(Value::as_str)
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| invalid_argument("'secret.key' must not be empty"))?;
    let declaration = crate::grain_commands::setting_decl(app, &identity.id, key)
        .filter(|decl| matches!(decl.kind, grain_sdk::SettingKind::Secret))
        .ok_or_else(|| invalid_argument(format!("'{key}' is not a declared secret setting")))?;
    drop(declaration);
    let value = ctx
        .extension_secret(&extension_secret_key(&identity.id, key))
        .ok_or_else(|| {
            unavailable(
                format!("secret setting '{key}' has no value"),
                "Set the credential in the extension's settings, then retry the call.",
            )
        })?;
    let raw_header = secret
        .get("header")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let header = HeaderName::from_bytes(raw_header.as_bytes())
        .map_err(|error| invalid_argument(format!("invalid secret header: {error}")))?;
    if header_is_host_controlled(&header) {
        return Err(invalid_argument(format!(
            "header '{}' is controlled by the host",
            header.as_str()
        )));
    }
    let prefix = secret.get("prefix").and_then(Value::as_str).unwrap_or("");
    let header_value = HeaderValue::from_str(&format!("{prefix}{value}"))
        .map_err(|_| invalid_argument("secret prefix produced an invalid header value"))?;
    Ok(Some((header, header_value)))
}

async fn proxy_fetch(
    identity: &ClientIdentity,
    params: &Value,
    secret_header: Option<(HeaderName, HeaderValue)>,
    auth_hosts: Option<&[String]>,
    redact_response: Option<Zeroizing<String>>,
) -> HostResult<Value> {
    let mut url = authorize_net_url(identity, &param_nonempty_str(params, "url")?)?;
    let mut method = net_method(params)?;
    let mut headers = net_headers(params)?;
    if let Some((name, value)) = secret_header {
        if headers.contains_key(&name) {
            return Err(invalid_argument(format!(
                "header '{}' is controlled by the selected credential",
                name.as_str()
            )));
        }
        headers.insert(name, value);
    }
    let mut body = params
        .get("body")
        .and_then(Value::as_str)
        .map(str::to_owned);

    for redirect_count in 0..=NET_MAX_REDIRECTS {
        #[cfg(feature = "agent-harness")]
        let scoped_client = if [
            crate::grain_agent_harness_auth::FIXTURE_ID,
            crate::grain_agent_harness_auth::PEER_ID,
        ]
        .contains(&identity.id.as_str())
        {
            Some(
                crate::grain_agent_harness_auth::api_client(&identity.id, url.as_str())
                    .map_err(internal_error)?,
            )
        } else {
            None
        };
        #[cfg(feature = "agent-harness")]
        let client = scoped_client
            .as_ref()
            .unwrap_or_else(|| extension_http_client());
        #[cfg(not(feature = "agent-harness"))]
        let client = extension_http_client();
        let mut request = client
            .request(method.clone(), url.clone())
            .headers(headers.clone());
        if let Some(body) = &body {
            request = request.body(body.clone());
        }
        let response = request
            .send()
            .await
            .map_err(|error| service_error("network request", error.to_string()))?;
        let status = response.status();

        if status.is_redirection() {
            let location = response.headers().get(LOCATION).ok_or_else(|| {
                unavailable(
                    "network redirect omitted its Location header",
                    "Retry the call or contact the endpoint owner.",
                )
            })?;
            if redirect_count == NET_MAX_REDIRECTS {
                return Err(typed_error(
                    HostErrorCode::Timeout,
                    "network request exceeded the redirect limit",
                    "Use an endpoint with a stable URL.",
                ));
            }
            let location = location
                .to_str()
                .map_err(|_| invalid_argument("redirect Location is not valid text"))?;
            let next = url
                .join(location)
                .map_err(|error| invalid_argument(format!("invalid redirect URL: {error}")))?;
            url = authorize_net_url(identity, next.as_str())?;
            if let Some(allowed) = auth_hosts {
                let next_host = url.host_str().unwrap_or_default().to_ascii_lowercase();
                if !allowed.iter().any(|host| host == &next_host) {
                    return Err(invalid_argument(format!(
                        "authenticated redirect to '{next_host}' is outside the declared apiHosts"
                    )));
                }
            }

            // Match browser fetch semantics for the common method-changing
            // redirects. 307/308 retain the original method and body.
            if status == StatusCode::SEE_OTHER
                || ((status == StatusCode::MOVED_PERMANENTLY || status == StatusCode::FOUND)
                    && method == Method::POST)
            {
                method = Method::GET;
                body = None;
                headers.remove(CONTENT_LENGTH);
                headers.remove(reqwest::header::CONTENT_TYPE);
            }
            continue;
        }

        if response
            .content_length()
            .is_some_and(|length| length > NET_MAX_RESPONSE_BYTES as u64)
        {
            return Err(response_too_large());
        }
        let response_headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value.to_str().ok().map(|value| {
                    let value = match redact_response.as_deref() {
                        Some(secret) if !secret.is_empty() && value.contains(secret) => {
                            SECRET_REDACTED.to_string()
                        }
                        _ => value.to_string(),
                    };
                    (name.as_str().to_string(), value)
                })
            })
            .collect::<BTreeMap<_, _>>();
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|error| service_error("network response", error.to_string()))?;
            if bytes.len().saturating_add(chunk.len()) > NET_MAX_RESPONSE_BYTES {
                return Err(response_too_large());
            }
            bytes.extend_from_slice(&chunk);
        }
        let mut body = String::from_utf8(bytes)
            .map_err(|_| invalid_argument("network response body is not UTF-8 text"))?;
        if let Some(secret) = redact_response.as_deref() {
            if !secret.is_empty() && body.contains(secret) {
                let redacted = body.replace(secret, SECRET_REDACTED);
                body.zeroize();
                body = redacted;
            }
        }
        return Ok(json!({
            "status": status.as_u16(),
            "ok": status.is_success(),
            "headers": response_headers,
            "body": body,
            "url": url.as_str(),
        }));
    }
    unreachable!("redirect loop always returns or continues within its bound")
}

/// Route one worker API call. Capability check FIRST; on failure return the
/// error and touch nothing.
pub async fn dispatch(
    app: &AppHandle,
    identity: &ClientIdentity,
    method: &str,
    params: Value,
    worker_generation: Option<u64>,
) -> HostResult<Value> {
    preflight(identity, method, &params)?;
    let worker_generation = worker_generation
        .ok_or_else(|| unavailable("extension worker was replaced", "Run a fresh tool call."))?;
    let registry = app
        .try_state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or_else(|| internal_error("extensions registry unavailable"))?;
    let current = registry.with_record_locked(&identity.id, |record| {
        record.is_some_and(|record| {
            record.enabled && record.execution_generation == worker_generation
        })
    });
    if !current {
        return Err(unavailable(
            "extension account or runtime changed",
            "Run a fresh tool call.",
        ));
    }

    let ctx = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .ok_or_else(|| internal_error("app context unavailable"))?;
    let data_dir = ctx.data_dir.clone();
    let store = ExtStorage::new(&data_dir, &identity.id);

    match method {
        "log.info" => {
            log::info!("[ext:{}] {}", identity.id, param_str(&params, "msg")?);
            Ok(Value::Null)
        }
        "log.warn" => {
            log::warn!("[ext:{}] {}", identity.id, param_str(&params, "msg")?);
            Ok(Value::Null)
        }
        "storage.get" => store
            .get(&param_str(&params, "key")?)
            .map_err(storage_error),
        "storage.set" => {
            let key = param_str(&params, "key")?;
            let value = param_value(&params, "value")?;
            store.set(&key, value).map_err(storage_error)?;
            Ok(Value::Null)
        }
        "storage.delete" => {
            store
                .delete(&param_str(&params, "key")?)
                .map_err(storage_error)?;
            Ok(Value::Null)
        }
        "net.fetch" => {
            if params.get("auth").and_then(Value::as_bool) == Some(true) {
                let raw_url = param_nonempty_str(&params, "url")?;
                let (url, _) = net_url_and_capability(&raw_url)?;
                let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
                let decl = crate::grain_commands::load_pack(app, &identity.id)
                    .map_err(internal_error)?
                    .manifest
                    .contributes
                    .authentication
                    .ok_or_else(|| invalid_argument("authentication is not declared"))?;
                let token =
                    crate::grain_auth::access_token(app, &identity.id, &host, worker_generation)
                        .await
                        .map_err(|error| {
                            unavailable(
                                error,
                                "Connect the account in the extension's settings, then retry.",
                            )
                        })?;
                let value =
                    HeaderValue::from_str(&format!("Bearer {}", token.as_str())).map_err(|_| {
                        internal_error("stored OAuth token is not a valid header value")
                    })?;
                proxy_fetch(
                    identity,
                    &params,
                    Some((AUTHORIZATION, value)),
                    Some(&decl.api_hosts),
                    Some(token),
                )
                .await
            } else {
                let secret = resolve_net_secret(app, &ctx, identity, &params)?;
                proxy_fetch(identity, &params, secret, None, None).await
            }
        }
        "auth.status" => {
            let connection = crate::grain_auth::connection(app, &identity.id)
                .await
                .map_err(internal_error)?;
            serde_json::to_value(connection).map_err(|error| internal_error(error.to_string()))
        }
        "auth.connect" => {
            let row = crate::grain_auth::connect_from_extension(
                app.clone(),
                identity.id.clone(),
                worker_generation,
            )
            .await
            .map_err(|error| unavailable(error, "Retry from the extension settings page."))?;
            serde_json::to_value(row).map_err(|error| internal_error(error.to_string()))
        }
        "auth.disconnect" => {
            crate::grain_auth::disconnect_from_extension(
                app.clone(),
                identity.id.clone(),
                worker_generation,
            )
            .await
            .map_err(internal_error)?;
            Ok(Value::Null)
        }
        other => Err(unknown_method(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_host_methods_are_unreachable_even_with_old_or_all_grants() {
        let identities = [
            named(grain_sdk::KNOWN_CAPABILITIES),
            ClientIdentity {
                id: "com.example.old".into(),
                role: crate::events_auth::ClientRole::Worker,
                caps: CapabilitySet::Named(
                    grain_sdk::KNOWN_CAPABILITIES
                        .iter()
                        .map(|cap| (*cap).to_owned())
                        .collect(),
                ),
            },
        ];
        for method in [
            "doc.get",
            "doc.put",
            "doc.delete",
            "doc.list",
            "settings.get",
            "settings.set",
            "llm.complete",
            "embed",
            "match.lexical",
            "match.semantic",
            "match.decide",
            "session.start",
            "capture.selection",
            "capture.app",
            "capture.screenText",
            "capture.screenImage",
            "open.url",
            "open.app",
            "open.pickApp",
            "os.exec",
            "space.get",
            "workspace.open",
        ] {
            for identity in &identities {
                let error = preflight(identity, method, &json!({})).unwrap_err();
                assert_eq!(error.code, HostErrorCode::UnknownMethod, "{method}");
                assert_typed(&error);
            }
        }
    }

    #[test]
    fn tool_support_methods_still_enforce_grants_and_validate_inputs() {
        let identity = named(&["storage", "auth", "net:api.example.com"]);
        for (method, params) in [
            ("log.info", json!({"msg":"ready"})),
            ("storage.get", json!({"key":"k"})),
            ("storage.set", json!({"key":"k","value":null})),
            ("storage.delete", json!({"key":"k"})),
            ("auth.status", json!({})),
            ("net.fetch", json!({"url":"https://api.example.com/v1"})),
        ] {
            assert!(preflight(&identity, method, &params).is_ok(), "{method}");
        }
        let denied = preflight(&named(&[]), "storage.get", &json!({"key":"k"})).unwrap_err();
        assert_eq!(denied.code, HostErrorCode::CapabilityDenied);
        let invalid = preflight(&identity, "storage.set", &json!({"key":"k"})).unwrap_err();
        assert_eq!(invalid.code, HostErrorCode::InvalidArgument);
    }
    use std::collections::HashSet;

    fn named(caps: &[&str]) -> ClientIdentity {
        ClientIdentity {
            id: "com.example.a".into(),
            role: crate::events_auth::ClientRole::Worker,
            caps: CapabilitySet::Named(caps.iter().map(|s| s.to_string()).collect::<HashSet<_>>()),
        }
    }

    #[test]
    fn auth_gate_is_extension_scoped_and_fetch_credentials_do_not_mix() {
        let github = named(&["auth", "net:api.github.com"]);
        assert!(preflight(&github, "auth.status", &json!({})).is_ok());
        let denied = preflight(&named(&[]), "auth.connect", &json!({})).unwrap_err();
        assert_eq!(denied.code, HostErrorCode::CapabilityDenied);

        let mixed = preflight(
            &github,
            "net.fetch",
            &json!({
                "url": "https://api.github.com/user",
                "auth": true,
                "secret": { "key": "token", "header": "authorization" }
            }),
        )
        .unwrap_err();
        assert_eq!(mixed.code, HostErrorCode::InvalidArgument);
    }

    fn assert_typed(error: &HostError) {
        assert!(!error.message.trim().is_empty());
        assert!(!error.hint.trim().is_empty());
        assert!(error.docs.starts_with("https://"));
    }

    #[test]
    fn network_grants_match_the_parsed_host_exactly() {
        let identity = named(&["net:api.example.com"]);
        assert!(authorize_net_url(&identity, "https://api.example.com/v1").is_ok());
        for url in [
            "https://api.example.com.evil.test/v1",
            "https://evil.test/api.example.com",
            "https://user:pass@api.example.com/v1",
        ] {
            let error = authorize_net_url(&identity, url).unwrap_err();
            assert!(matches!(
                error.code,
                HostErrorCode::CapabilityDenied | HostErrorCode::InvalidArgument
            ));
            assert_typed(&error);
        }
    }

    async fn serve_once(response: String) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 2048];
            let _ = socket.read(&mut request).await;
            socket.write_all(response.as_bytes()).await.unwrap();
            let _ = socket.shutdown().await;
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn network_proxy_allows_granted_host_and_refuses_large_responses() {
        let identity = named(&["net:127.0.0.1"]);
        let body = "weather-ready";
        let url = serve_once(format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ))
        .await;
        let result = proxy_fetch(&identity, &json!({"url": url}), None, None, None)
            .await
            .unwrap();
        assert_eq!(result["status"], 200);
        assert_eq!(result["body"], body);

        let url = serve_once(format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            NET_MAX_RESPONSE_BYTES + 1
        ))
        .await;
        let error = proxy_fetch(&identity, &json!({"url": url}), None, None, None)
            .await
            .unwrap_err();
        assert_eq!(error.code, HostErrorCode::ResponseTooLarge);
        assert_typed(&error);
    }

    #[tokio::test]
    async fn network_proxy_revalidates_redirect_hosts() {
        let identity = named(&["net:127.0.0.1"]);
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = target.local_addr().unwrap().port();
        drop(target);
        let url = serve_once(format!(
            "HTTP/1.1 302 Found\r\nLocation: http://localhost:{port}/off-grant\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ))
        .await;
        let error = proxy_fetch(&identity, &json!({"url": url}), None, None, None)
            .await
            .unwrap_err();
        assert_eq!(error.code, HostErrorCode::CapabilityDenied);
        assert_eq!(error.capability.as_deref(), Some("net:localhost"));
    }

    #[tokio::test]
    async fn authenticated_proxy_redacts_an_echoed_token() {
        let identity = named(&["net:127.0.0.1"]);
        let token = "provider-secret-token";
        let body = format!("echo={token}");
        let url = serve_once(format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nX-Echo: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
            token
        ))
        .await;
        let result = proxy_fetch(
            &identity,
            &json!({"url": url}),
            Some((
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
            )),
            Some(&["127.0.0.1".to_string()]),
            Some(Zeroizing::new(token.to_string())),
        )
        .await
        .unwrap();
        assert_eq!(result["body"], format!("echo={SECRET_REDACTED}"));
        assert_eq!(result["headers"]["x-echo"], SECRET_REDACTED);
        assert!(!result.to_string().contains(token));
    }

    #[test]
    fn storage_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "com.example.a");
        assert_eq!(s.get("k").unwrap(), Value::Null);
        s.set("k", json!({"n": 1})).unwrap();
        assert_eq!(s.get("k").unwrap(), json!({"n": 1}));
        s.delete("k").unwrap();
        assert_eq!(s.get("k").unwrap(), Value::Null);
    }

    #[test]
    fn uninstall_purge_removes_storage() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExtStorage::new(dir.path(), "com.example.removed");
        store.set("key", json!("value")).unwrap();
        assert!(store.path.exists());

        store.purge().unwrap();
        assert!(!store.path.exists());
        // Idempotent cleanup keeps uninstall retries safe.
        store.purge().unwrap();
    }

    #[test]
    fn quota_is_enforced_on_set() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "big");
        let huge = "x".repeat((STORAGE_QUOTA_BYTES + 1) as usize);
        let error = storage_error(s.set("k", json!(huge)).unwrap_err());
        assert_eq!(error.code, HostErrorCode::Quota);
        assert_typed(&error);
        // A normal value still writes.
        assert!(s.set("k", json!("small")).is_ok());
    }

    #[test]
    fn corrupt_storage_is_an_error_not_a_fake_missing_value() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "corrupt");
        std::fs::create_dir_all(s.path.parent().unwrap()).unwrap();
        std::fs::write(&s.path, "not json").unwrap();
        let error = storage_error(s.get("k").unwrap_err());
        assert_eq!(error.code, HostErrorCode::Internal);
        assert_typed(&error);
    }
}
