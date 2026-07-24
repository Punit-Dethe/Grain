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
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_LENGTH, LOCATION};
use reqwest::{Method, StatusCode, Url};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::events_auth::{CapabilitySet, ClientIdentity};

/// Per-extension storage quota (SPEC §3.4). 200 MB, generous for KV + docs.
const STORAGE_QUOTA_BYTES: u64 = 200 * 1024 * 1024;

/// Cap on texts per `embed` call. The engine holds every input's tokens in
/// memory at once, so an unbounded batch is a memory-exhaustion lever; a
/// low-RAM device would rather the extension chunk its work.
const EMBED_MAX_BATCH: usize = 64;

/// Extension egress is intentionally small and bounded. API-shaped responses
/// fit comfortably; bulk transfer belongs in a purpose-built host capability.
const NET_TIMEOUT: Duration = Duration::from_secs(15);
const NET_MAX_REDIRECTS: usize = 5;
const NET_MAX_REQUEST_BYTES: usize = 1024 * 1024;
const NET_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const NET_MAX_HEADERS: usize = 64;

static EXTENSION_HTTP: OnceLock<reqwest::Client> = OnceLock::new();

/// Reserved key inside the storage file for the extension's own settings
/// namespace (`ext.<id>.*`). There is no path to `AppSettings` — this is the
/// entire "settings" surface an extension has (SPEC §4.2).
const SETTINGS_KEY: &str = "__settings";
pub(crate) const SECRET_REDACTED: &str = "[REDACTED]";

pub(crate) fn extension_secret_key(ext_id: &str, key: &str) -> String {
    format!("ext.{ext_id}.{key}")
}

type HostResult<T> = Result<T, HostError>;

#[derive(Debug)]
pub enum ExtStorageError {
    InvalidArgument(String),
    Quota(String),
    Io(String),
    Serialization(String),
}

impl fmt::Display for ExtStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument(message)
            | Self::Quota(message)
            | Self::Io(message)
            | Self::Serialization(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ExtStorageError {}

type StorageResult<T> = Result<T, ExtStorageError>;

/// Does this identity hold `cap`? `All` (the pill) holds everything; a worker
/// holds exactly its granted `Named` set.
pub fn has_capability(identity: &ClientIdentity, cap: &str) -> bool {
    match &identity.caps {
        CapabilitySet::All => true,
        CapabilitySet::Named(set) => set.contains(cap),
    }
}

/// The capability a method requires, or `None` for always-allowed methods
/// (`log.*`). Pure, so the gating table is testable without app state.
pub fn required_capability(method: &str) -> Option<&'static str> {
    match method {
        "log.info" | "log.warn" => None,
        "storage.get" | "storage.set" | "storage.delete" => Some("storage"),
        "doc.get" | "doc.put" | "doc.delete" | "doc.list" => Some("storage"),
        "settings.get" | "settings.set" => Some("settings"),
        "llm.complete" => Some("llm"),
        // The real grant is derived from the parsed URL (`net:<exact-host>`).
        "net.fetch" => Some("__dynamic_net__"),
        "embed" => Some("embed"),
        "session.start" => Some("session:start"),
        "capture.selection" => Some("capture:selection"),
        "workspace.open" | "workspace.close" => Some("surface:workspace"),
        "overlay.show" | "overlay.dismiss" => Some("surface:overlay"),
        "open.url" => Some("open:url"),
        "open.app" | "open.pickApp" => Some("open:app"),
        _ => Some("__unknown__"), // unknown methods map to an ungrantable cap
    }
}

/// A per-extension JSON key/value store: `<data>/extensions/<id>.storage.json`.
/// Loaded and saved whole (fine at this scale). Pure over its path, so quota
/// and round-trip behavior are unit-tested directly.
pub struct ExtStorage {
    path: PathBuf,
    /// `<data>/extensions/<id>.docs/` — the document store's own directory, one
    /// file per document, so a large collection is not one blob rewritten on
    /// every edit (SPEC §3.4: notes are documents, not KV values).
    docs_dir: PathBuf,
}

impl ExtStorage {
    pub fn new(data_dir: &std::path::Path, ext_id: &str) -> Self {
        let ext_dir = data_dir.join("extensions");
        Self {
            path: ext_dir.join(format!("{ext_id}.storage.json")),
            docs_dir: ext_dir.join(format!("{ext_id}.docs")),
        }
    }

    /// Uninstall boundary: remove both KV/settings and document storage. Missing
    /// paths are already clean and therefore succeed.
    pub fn purge(&self) -> StorageResult<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(ExtStorageError::Io(error.to_string())),
        }
        match std::fs::remove_dir_all(&self.docs_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(ExtStorageError::Io(error.to_string())),
        }
        Ok(())
    }

    /// A document key sanitized to a single safe filename. Rejects anything that
    /// could escape the store's directory or collide across keys — the whole
    /// point of a per-document file is undone if a key can be `../../secrets`.
    /// Pure and total, so it is exhaustively unit-tested.
    pub fn safe_doc_name(key: &str) -> StorageResult<String> {
        let key = key.trim();
        if key.is_empty() {
            return Err(ExtStorageError::InvalidArgument(
                "a document key must not be empty".into(),
            ));
        }
        if key.len() > 200 {
            return Err(ExtStorageError::InvalidArgument(
                "a document key is at most 200 characters".into(),
            ));
        }
        // Allowlist, not denylist: only characters that are unambiguously safe
        // in a filename on every platform, and never a path separator or dot-run.
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(ExtStorageError::InvalidArgument(
                "a document key may contain only letters, digits, '-', '_' and '.'".into(),
            ));
        }
        // `.` and `..` (and any all-dots name) are path traversal, not documents.
        if key.chars().all(|c| c == '.') {
            return Err(ExtStorageError::InvalidArgument(
                "a document key must not be all dots".into(),
            ));
        }
        Ok(format!("{key}.json"))
    }

    fn doc_path(&self, key: &str) -> StorageResult<PathBuf> {
        Ok(self.docs_dir.join(Self::safe_doc_name(key)?))
    }

    /// Total bytes the document store currently occupies (for the quota).
    fn docs_bytes(&self) -> StorageResult<u64> {
        let entries = match std::fs::read_dir(&self.docs_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(ExtStorageError::Io(error.to_string())),
        };
        let mut total = 0_u64;
        for entry in entries {
            let entry = entry.map_err(|error| ExtStorageError::Io(error.to_string()))?;
            let metadata = entry
                .metadata()
                .map_err(|error| ExtStorageError::Io(error.to_string()))?;
            total = total.saturating_add(metadata.len());
        }
        Ok(total)
    }

    pub fn doc_get(&self, key: &str) -> StorageResult<Value> {
        let path = self.doc_path(key)?;
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|error| ExtStorageError::Serialization(error.to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
            Err(error) => Err(ExtStorageError::Io(error.to_string())),
        }
    }

    pub fn doc_put(&self, key: &str, value: Value) -> StorageResult<()> {
        let path = self.doc_path(key)?;
        let json = serde_json::to_string(&value)
            .map_err(|error| ExtStorageError::Serialization(error.to_string()))?;
        // Quota covers the doc store as a whole; charge the NEW size of this doc
        // against the total minus whatever this key used to occupy.
        let prev = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let projected = self.docs_bytes()?.saturating_sub(prev) + json.len() as u64;
        if projected > STORAGE_QUOTA_BYTES {
            return Err(ExtStorageError::Quota(format!(
                "document storage quota exceeded ({} MB max)",
                STORAGE_QUOTA_BYTES / (1024 * 1024)
            )));
        }
        std::fs::create_dir_all(&self.docs_dir)
            .map_err(|error| ExtStorageError::Io(error.to_string()))?;
        std::fs::write(&path, json).map_err(|error| ExtStorageError::Io(error.to_string()))
    }

    pub fn doc_delete(&self, key: &str) -> StorageResult<()> {
        let path = self.doc_path(key)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ExtStorageError::Io(error.to_string())),
        }
    }

    /// Every document key currently stored (without the `.json` suffix), sorted.
    pub fn doc_list(&self) -> StorageResult<Vec<String>> {
        let entries = match std::fs::read_dir(&self.docs_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(ExtStorageError::Io(error.to_string())),
        };
        let mut keys = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| ExtStorageError::Io(error.to_string()))?;
            if let Some(key) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
            {
                keys.push(key.to_string());
            }
        }
        keys.sort();
        Ok(keys)
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

    /// The extension's own settings namespace (a nested object under one
    /// reserved key — never `AppSettings`).
    ///
    /// Public because the host's settings controls write here too: one store,
    /// one way in, so the schema cannot be enforced on one path and not the
    /// other.
    pub fn settings_get(&self, key: &str) -> StorageResult<Value> {
        Ok(self
            .get(SETTINGS_KEY)?
            .get(key)
            .cloned()
            .unwrap_or(Value::Null))
    }

    pub fn settings_set(&self, key: &str, value: Value) -> StorageResult<()> {
        let mut ns = match self.get(SETTINGS_KEY)? {
            Value::Object(m) => m,
            _ => serde_json::Map::new(),
        };
        ns.insert(key.to_string(), value);
        self.set(SETTINGS_KEY, Value::Object(ns))
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
        ExtStorageError::InvalidArgument(message) => invalid_argument(message),
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

fn param_strings(params: &Value, key: &str) -> HostResult<Vec<String>> {
    let values = params
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_argument(format!("'{key}' must be an array of strings")))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| invalid_argument(format!("'{key}[{index}]' must be a string")))
        })
        .collect()
}

fn authorize(identity: &ClientIdentity, method: &str, params: &Value) -> HostResult<()> {
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
        "storage.get" | "storage.delete" | "doc.get" | "doc.delete" | "settings.get" => {
            param_nonempty_str(params, "key")?;
        }
        "storage.set" | "doc.put" | "settings.set" => {
            param_nonempty_str(params, "key")?;
            param_value(params, "value")?;
        }
        "llm.complete" => {
            param_nonempty_str(params, "prompt")?;
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
        }
        "embed" => {
            let texts = param_strings(params, "texts")?;
            if texts.len() > EMBED_MAX_BATCH {
                return Err(invalid_argument(format!(
                    "'texts' accepts at most {EMBED_MAX_BATCH} items"
                )));
            }
        }
        "session.start" => {
            param_nonempty_str(params, "mode")?;
        }
        "open.url" => {
            // Full validation (scheme allowlist) happens in dispatch; here we
            // only require a non-empty string so a typed error is returned early.
            param_nonempty_str(params, "url")?;
        }
        "open.app" => {
            param_nonempty_str(params, "path")?;
        }
        "doc.list" | "capture.selection" | "workspace.open" | "workspace.close"
        | "overlay.show" | "overlay.dismiss" | "open.pickApp" => {}
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
) -> HostResult<Value> {
    let mut url = authorize_net_url(identity, &param_nonempty_str(params, "url")?)?;
    let mut method = net_method(params)?;
    let mut headers = net_headers(params)?;
    if let Some((name, value)) = secret_header {
        headers.insert(name, value);
    }
    let mut body = params
        .get("body")
        .and_then(Value::as_str)
        .map(str::to_owned);

    for redirect_count in 0..=NET_MAX_REDIRECTS {
        let mut request = extension_http_client()
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
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
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
        let body = String::from_utf8(bytes)
            .map_err(|_| invalid_argument("network response body is not UTF-8 text"))?;
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
) -> HostResult<Value> {
    preflight(identity, method, &params)?;

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
        // [GRAIN] SPEC §3.4: the document store — one file per key, so a large
        // note collection is not one blob rewritten on every edit. Same
        // `storage` grant, same quota; the key is a path-safe filename.
        "doc.get" => store
            .doc_get(&param_str(&params, "key")?)
            .map_err(storage_error),
        "doc.put" => {
            let key = param_str(&params, "key")?;
            let value = param_value(&params, "value")?;
            store.doc_put(&key, value).map_err(storage_error)?;
            Ok(Value::Null)
        }
        "doc.delete" => {
            store
                .doc_delete(&param_str(&params, "key")?)
                .map_err(storage_error)?;
            Ok(Value::Null)
        }
        "doc.list" => Ok(json!({ "keys": store.doc_list().map_err(storage_error)? })),
        // [GRAIN] SPEC §1.2: the extension asks for ITS OWN workspace and gets
        // nothing else — there is no id parameter to point at another
        // extension's surface, because identity comes from the channel.
        "workspace.open" => {
            crate::surfaces::extension::open(app, &identity.id, params.get("payload").cloned())
                .map_err(|error| {
                    typed_error(
                        HostErrorCode::InvalidManifest,
                        format!("workspace.open failed: {error}"),
                        "Declare a valid workspace surface in manifest.json, then reload the extension.",
                    )
                })?;
            Ok(Value::Null)
        }
        "workspace.close" => {
            crate::surfaces::extension::close(app, &identity.id);
            Ok(Value::Null)
        }
        // [GRAIN] SPEC §1.2: a transient HUD for THIS extension, host-budgeted
        // in size and lifetime — same channel-derived identity as workspace.
        "overlay.show" => {
            crate::surfaces::overlay::show(app, &identity.id, params.get("payload").cloned())
                .map_err(|error| {
                    typed_error(
                        HostErrorCode::InvalidManifest,
                        format!("overlay.show failed: {error}"),
                        "Declare a valid overlay surface in manifest.json, then reload the extension.",
                    )
                })?;
            Ok(Value::Null)
        }
        "overlay.dismiss" => {
            crate::surfaces::overlay::dismiss(app, &identity.id);
            Ok(Value::Null)
        }
        "settings.get" => {
            let key = param_str(&params, "key")?;
            if crate::grain_commands::setting_decl(app, &identity.id, &key)
                .is_some_and(|decl| matches!(decl.kind, grain_sdk::SettingKind::Secret))
            {
                let marker = if ctx
                    .extension_secret(&extension_secret_key(&identity.id, &key))
                    .is_some()
                {
                    SECRET_REDACTED
                } else {
                    ""
                };
                return Ok(Value::String(marker.to_string()));
            }
            let stored = store.settings_get(&key).map_err(storage_error)?;
            // A declared setting reads back through the schema, so a value left
            // behind by an older version resolves to something the extension's
            // own declaration says is legal.
            Ok(
                match crate::grain_commands::setting_decl(app, &identity.id, &key) {
                    Some(decl) => grain_sdk::settings_schema::resolve(&decl, Some(&stored)).value,
                    None => stored,
                },
            )
        }
        "settings.set" => {
            let key = param_str(&params, "key")?;
            let value = param_value(&params, "value")?;
            // [GRAIN] SPEC §4.1: the schema is enforced HERE, not only in the
            // settings form — this is the same namespace the host's own
            // controls write to, and the extension can reach it directly. A
            // schema policed only in React is not policed at all.
            //
            // An *undeclared* key is free-form on purpose: the schema says what
            // the host renders, not everything the extension may remember.
            let value = match crate::grain_commands::setting_decl(app, &identity.id, &key) {
                Some(decl) => {
                    let accepted = grain_sdk::settings_schema::coerce(&decl, &value)
                        .map_err(|error| invalid_argument(format!("'{key}': {error}")))?
                        .value;
                    if matches!(decl.kind, grain_sdk::SettingKind::Secret) {
                        let secret = accepted
                            .as_str()
                            .ok_or_else(|| invalid_argument("secret value must be text"))?;
                        ctx.set_extension_secret(
                            extension_secret_key(&identity.id, &key),
                            secret.to_string(),
                        )
                        .map_err(|error| internal_error(format!("secret write failed: {error}")))?;
                        return Ok(Value::Null);
                    }
                    accepted
                }
                None => value,
            };
            store.settings_set(&key, value).map_err(storage_error)?;
            Ok(Value::Null)
        }
        "llm.complete" => {
            let prompt = param_str(&params, "prompt")?;
            let text = crate::grain_post_process::complete_for_extension(app, &prompt)
                .await
                .map_err(|error| service_error("LLM provider", error))?;
            Ok(json!({ "text": text }))
        }
        "net.fetch" => {
            let secret = resolve_net_secret(app, &ctx, identity, &params)?;
            proxy_fetch(identity, &params, secret).await
        }
        "embed" => {
            // [GRAIN] SPEC §1.3 / Grain Space Test: the same on-device BGE
            // embedder Grain Space uses, offered to extensions. Local, free,
            // private — the reason a Grain-Space-class extension is buildable
            // without shipping its own model. Blocking (model inference), so it
            // runs on the blocking pool; failure to load the model is surfaced
            // verbatim rather than pretended around.
            let texts = param_strings(&params, "texts")?;
            let vectors =
                tokio::task::spawn_blocking(move || crate::grain_space::embed::embed(texts))
                    .await
                    .map_err(|error| internal_error(format!("embed task failed: {error}")))?
                    .map_err(|error| service_error("embedding model", error.to_string()))?;
            Ok(json!({ "vectors": vectors }))
        }
        "capture.selection" => {
            // [GRAIN] Grain Space Test: the selection quick-add path. Simulates
            // a copy in the foreground app, reads the result, and restores the
            // clipboard — the same primitive the Agent and Grain Space capture
            // use. Blocking (it polls the clipboard), so off the async thread;
            // `null` when there was nothing selected.
            let app2 = app.clone();
            let text =
                tokio::task::spawn_blocking(move || crate::agent::capture_selection_result(&app2))
                    .await
                    .map_err(|error| internal_error(format!("capture task failed: {error}")))?
                    .map_err(|error| {
                        unavailable(
                            format!("selection capture failed: {error}"),
                            "Focus an app with selectable text and retry the call.",
                        )
                    })?;
            Ok(json!({ "text": text }))
        }
        "session.start" => {
            let mode = param_str(&params, "mode")?;
            match crate::extension_session::start(app, &identity.id, &mode) {
                Ok(session_id) => Ok(json!({ "sessionId": session_id })),
                Err(crate::extension_session::StartError::Busy) => Err(typed_error(
                    HostErrorCode::SessionBusy,
                    "Another recording session is already running.",
                    "Wait for the current session to finish or cancel it, then retry.",
                )),
                Err(crate::extension_session::StartError::InvalidMode) => Err(invalid_argument(
                    format!("'{mode}' is not this extension's contributed session mode"),
                )),
                Err(crate::extension_session::StartError::Unavailable(error)) => Err(unavailable(
                    format!("session could not start: {error}"),
                    "Check microphone access and that the extension is still enabled.",
                )),
            }
        }
        // [GRAIN] Phase 5C — launch side effects. Security is enforced HERE, in
        // Rust, never trusted to the extension (SPEC §7.2).
        "open.url" => {
            // Scheme allowlist: http/https/mailto/tel ONLY. A decade of Electron
            // `openExternal` RCEs and Tauri's own shell-open advisory
            // (GHSA-c9pr-q8gx-3mgp) all trace to opening a URL whose scheme was
            // not restricted — `file:`, `javascript:`, custom URI handlers, etc.
            let raw = param_nonempty_str(&params, "url")?;
            let url = validate_open_url(&raw)?;
            // The opener plugin hands the URL to the OS default handler; it does
            // not spawn a shell, so there is no argument/command injection path.
            app.opener()
                .open_url(url, None::<String>)
                .map_err(|error| {
                    unavailable(
                        format!("could not open the link: {error}"),
                        "The URL was valid but the OS could not open it.",
                    )
                })?;
            Ok(Value::Null)
        }
        "open.pickApp" => {
            // User-mediated app choice: the ONLY way a path becomes launchable.
            // The extension cannot supply a path here — the user picks it in
            // Grain's native chooser, and we record it as approved for this
            // extension so a later `open.app` can run it.
            let app2 = app.clone();
            let picked = tokio::task::spawn_blocking(move || {
                use tauri_plugin_dialog::DialogExt;
                app2.dialog().file().blocking_pick_file()
            })
            .await
            .map_err(|error| internal_error(format!("picker task failed: {error}")))?;
            let path = picked
                .and_then(|f| f.into_path().ok())
                .map(|p| p.to_string_lossy().to_string());
            if let Some(ref p) = path {
                approve_app(&data_dir, &identity.id, p)
                    .map_err(|e| internal_error(format!("could not record approval: {e}")))?;
            }
            Ok(json!({ "path": path }))
        }
        "open.app" => {
            let path = param_nonempty_str(&params, "path")?;
            // The extension may launch ONLY a path the user picked through
            // `open.pickApp`. An arbitrary path (including the extension's own
            // bundled files) is refused — this is what stops `open:app` from
            // being a sandbox escape / RCE.
            if !is_app_approved(&data_dir, &identity.id, &path) {
                return Err(typed_error(
                    HostErrorCode::CapabilityDenied,
                    "This app was not approved by the user.",
                    "Call open.pickApp() so the user chooses the application; only a user-picked app can be launched.",
                ));
            }
            app.opener()
                .open_path(path.clone(), None::<String>)
                .map_err(|error| {
                    unavailable(
                        format!("could not launch the app: {error}"),
                        "The app path was approved but the OS could not open it (it may have moved).",
                    )
                })?;
            Ok(Value::Null)
        }
        other => Err(unknown_method(other)),
    }
}

/// Validate a URL for `open:url`. Returns the accepted URL string, or a typed
/// error. **Scheme allowlist only** — the single most important control here
/// (Electron `openExternal` / Tauri shell-open RCE history): `file:`,
/// `javascript:`, `data:`, `vbscript:` and any custom URI handler are refused,
/// because the OS routes them to handlers that can execute code or read files.
fn validate_open_url(raw: &str) -> HostResult<String> {
    const ALLOWED: &[&str] = &["http", "https", "mailto", "tel"];
    let url = Url::parse(raw.trim())
        .map_err(|error| invalid_argument(format!("invalid URL: {error}")))?;
    let scheme = url.scheme().to_ascii_lowercase();
    if !ALLOWED.contains(&scheme.as_str()) {
        return Err(typed_error(
            HostErrorCode::InvalidArgument,
            format!("scheme '{scheme}' is not allowed"),
            "open.url accepts only http, https, mailto and tel links.",
        ));
    }
    // Defence in depth: reject embedded credentials (phishing / SSRF-ish).
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_argument("URLs must not contain credentials"));
    }
    Ok(url.to_string())
}

/// Where a per-extension set of user-approved launchable app paths lives. This
/// file is written ONLY by the host (via `open.pickApp`); the extension has no
/// API that can add to it, so it cannot self-approve an app to launch.
fn approved_apps_path(data_dir: &std::path::Path, ext_id: &str) -> std::path::PathBuf {
    data_dir
        .join("extensions")
        .join(format!("{ext_id}.approved-apps.json"))
}

fn read_approved_apps(data_dir: &std::path::Path, ext_id: &str) -> Vec<String> {
    std::fs::read_to_string(approved_apps_path(data_dir, ext_id))
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
}

fn is_app_approved(data_dir: &std::path::Path, ext_id: &str, path: &str) -> bool {
    read_approved_apps(data_dir, ext_id)
        .iter()
        .any(|p| p == path)
}

pub(crate) fn approve_app(data_dir: &std::path::Path, ext_id: &str, path: &str) -> std::io::Result<()> {
    let mut approved = read_approved_apps(data_dir, ext_id);
    if !approved.iter().any(|p| p == path) {
        approved.push(path.to_string());
    }
    let file = approved_apps_path(data_dir, ext_id);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(file, serde_json::to_string(&approved)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn named(caps: &[&str]) -> ClientIdentity {
        ClientIdentity {
            id: "com.example.a".into(),
            role: crate::events_auth::ClientRole::Worker,
            caps: CapabilitySet::Named(caps.iter().map(|s| s.to_string()).collect::<HashSet<_>>()),
        }
    }

    #[test]
    fn open_url_allows_only_safe_schemes() {
        // Accepted: the four safe schemes.
        for ok in [
            "https://example.com/x",
            "http://example.com",
            "mailto:a@example.com",
            "tel:+15551234",
        ] {
            assert!(validate_open_url(ok).is_ok(), "{ok} should be allowed");
        }
        // Rejected: every known `openExternal`/shell-open RCE vector.
        for bad in [
            "file:///etc/passwd",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox(1)",
            "smb://attacker/share",
            "vscode://x",
            "ms-msdt:/id",
            "chrome://settings",
            "\\\\attacker\\share",
            "example.com", // no scheme — must be explicit, never guessed
        ] {
            let err = validate_open_url(bad).unwrap_err();
            assert_eq!(err.code, HostErrorCode::InvalidArgument, "{bad} must be refused");
        }
        // Credentials are refused even on an allowed scheme.
        assert!(validate_open_url("https://user:pw@example.com").is_err());
    }

    #[test]
    fn open_app_only_launches_user_approved_paths() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let id = "com.example.actions";
        let evil = "C:/Windows/System32/cmd.exe";
        // Nothing is approved by default — an arbitrary path is refused.
        assert!(!is_app_approved(data, id, evil));
        // Only a path recorded via the user-mediated picker becomes launchable,
        // and only for the extension that picked it.
        let good = "C:/Program Files/Editor/editor.exe";
        approve_app(data, id, good).unwrap();
        assert!(is_app_approved(data, id, good));
        assert!(!is_app_approved(data, id, evil), "approval is per-path, exact");
        assert!(
            !is_app_approved(data, "com.other.ext", good),
            "approval is per-extension"
        );
    }

    #[test]
    fn capability_gate_is_pure_and_correct() {
        let ext = named(&["storage"]);
        assert!(has_capability(&ext, "storage"));
        assert!(!has_capability(&ext, "llm"));

        // Method → capability mapping.
        assert_eq!(required_capability("storage.set"), Some("storage"));
        assert_eq!(required_capability("llm.complete"), Some("llm"));
        assert_eq!(required_capability("net.fetch"), Some("__dynamic_net__"));
        assert_eq!(required_capability("session.start"), Some("session:start"));
        assert_eq!(
            required_capability("capture.selection"),
            Some("capture:selection")
        );
        assert_eq!(required_capability("embed"), Some("embed"));
        // The document store shares the storage grant.
        assert_eq!(required_capability("doc.put"), Some("storage"));
        assert_eq!(required_capability("doc.list"), Some("storage"));
        assert_eq!(required_capability("log.info"), None);
        // Unknown methods require an ungrantable capability → always denied.
        assert_eq!(required_capability("os.exec"), Some("__unknown__"));
        assert!(!has_capability(
            &ClientIdentity {
                id: "x".into(),
                role: crate::events_auth::ClientRole::Worker,
                caps: CapabilitySet::Named(HashSet::new())
            },
            "__unknown__"
        ));
    }

    fn assert_typed(error: &HostError) {
        assert!(!error.message.trim().is_empty());
        assert!(!error.hint.trim().is_empty());
        assert!(error.docs.starts_with("https://"));
    }

    #[test]
    fn every_preflight_refusal_is_typed_and_never_an_empty_success() {
        let no_caps = named(&[]);
        for (method, capability) in [
            ("storage.get", "storage"),
            ("doc.list", "storage"),
            ("settings.get", "settings"),
            ("llm.complete", "llm"),
            ("embed", "embed"),
            ("session.start", "session:start"),
            ("capture.selection", "capture:selection"),
            ("workspace.open", "surface:workspace"),
            ("overlay.show", "surface:overlay"),
        ] {
            let error = preflight(&no_caps, method, &json!({})).unwrap_err();
            assert_eq!(error.code, HostErrorCode::CapabilityDenied, "{method}");
            assert_eq!(error.capability.as_deref(), Some(capability));
            assert_typed(&error);
        }

        let all_caps = named(&[
            "storage",
            "settings",
            "llm",
            "embed",
            "session:start",
            "capture:selection",
            "surface:workspace",
            "surface:overlay",
            "net:api.example.com",
        ]);
        for (method, params) in [
            ("log.info", json!({})),
            ("storage.get", json!({})),
            ("storage.set", json!({"key": "k"})),
            ("doc.put", json!({"key": "k"})),
            ("settings.set", json!({"key": "k"})),
            ("llm.complete", json!({"prompt": 4})),
            ("embed", json!({"texts": ["ok", 4]})),
            ("session.start", json!({})),
            ("net.fetch", json!({})),
        ] {
            let error = preflight(&all_caps, method, &params).unwrap_err();
            assert_eq!(error.code, HostErrorCode::InvalidArgument, "{method}");
            assert_typed(&error);
        }

        for (method, params) in [
            ("log.info", json!({"msg": "ready"})),
            ("log.warn", json!({"msg": "careful"})),
            ("storage.get", json!({"key": "k"})),
            ("storage.set", json!({"key": "k", "value": null})),
            ("storage.delete", json!({"key": "k"})),
            ("doc.get", json!({"key": "k"})),
            ("doc.put", json!({"key": "k", "value": null})),
            ("doc.delete", json!({"key": "k"})),
            ("doc.list", json!({})),
            ("settings.get", json!({"key": "k"})),
            ("settings.set", json!({"key": "k", "value": null})),
            ("llm.complete", json!({"prompt": "hello"})),
            ("embed", json!({"texts": ["hello"]})),
            ("session.start", json!({"mode": "note"})),
            ("capture.selection", json!({})),
            ("workspace.open", json!({})),
            ("workspace.close", json!({})),
            ("overlay.show", json!({})),
            ("overlay.dismiss", json!({})),
            (
                "net.fetch",
                json!({"url": "https://api.example.com/v1", "method": "GET"}),
            ),
        ] {
            assert!(preflight(&all_caps, method, &params).is_ok(), "{method}");
        }

        let unknown = preflight(&all_caps, "os.exec", &json!({})).unwrap_err();
        assert_eq!(unknown.code, HostErrorCode::UnknownMethod);
        assert_typed(&unknown);
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
        let result = proxy_fetch(&identity, &json!({"url": url}), None)
            .await
            .unwrap();
        assert_eq!(result["status"], 200);
        assert_eq!(result["body"], body);

        let url = serve_once(format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            NET_MAX_RESPONSE_BYTES + 1
        ))
        .await;
        let error = proxy_fetch(&identity, &json!({"url": url}), None)
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
        let error = proxy_fetch(&identity, &json!({"url": url}), None)
            .await
            .unwrap_err();
        assert_eq!(error.code, HostErrorCode::CapabilityDenied);
        assert_eq!(error.capability.as_deref(), Some("net:localhost"));
    }

    #[test]
    fn storage_roundtrip_and_settings_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "com.example.a");
        assert_eq!(s.get("k").unwrap(), Value::Null);
        s.set("k", json!({"n": 1})).unwrap();
        assert_eq!(s.get("k").unwrap(), json!({"n": 1}));
        s.delete("k").unwrap();
        assert_eq!(s.get("k").unwrap(), Value::Null);

        // Settings namespace is isolated under its own reserved key.
        s.settings_set("theme", json!("dark")).unwrap();
        assert_eq!(s.settings_get("theme").unwrap(), json!("dark"));
        assert_eq!(s.get("theme").unwrap(), Value::Null); // not a top-level key
    }

    #[test]
    fn uninstall_purge_removes_kv_settings_and_documents() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExtStorage::new(dir.path(), "com.example.removed");
        store.set("key", json!("value")).unwrap();
        store.settings_set("theme", json!("dark")).unwrap();
        store.doc_put("note", json!({"text": "hello"})).unwrap();
        assert!(store.path.exists());
        assert!(store.docs_dir.exists());

        store.purge().unwrap();
        assert!(!store.path.exists());
        assert!(!store.docs_dir.exists());
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

    #[test]
    fn document_keys_cannot_escape_the_store() {
        // The security-critical part: a key is a filename, never a path.
        for bad in [
            "",
            "  ",
            "../secrets",
            "a/b",
            "a\\b",
            ".",
            "..",
            "...",
            "a b",
            "a:b",
            "note\0",
            &"x".repeat(201),
        ] {
            assert!(
                ExtStorage::safe_doc_name(bad).is_err(),
                "key {bad:?} must be rejected"
            );
        }
        for ok in ["note", "note-1", "note_1", "a.b", "2026-07-22", "Z9"] {
            assert_eq!(ExtStorage::safe_doc_name(ok).unwrap(), format!("{ok}.json"));
        }
    }

    #[test]
    fn document_store_roundtrips_and_lists() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "com.example.docs");
        assert_eq!(s.doc_get("a").unwrap(), Value::Null); // absent → null
        assert!(s.doc_list().unwrap().is_empty());

        s.doc_put("a", json!({"body": "one"})).unwrap();
        s.doc_put("b", json!({"body": "two"})).unwrap();
        assert_eq!(s.doc_get("a").unwrap(), json!({"body": "one"}));
        assert_eq!(
            s.doc_list().unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );

        // A document is its own file — the KV store is untouched by doc writes.
        assert_eq!(s.get("a").unwrap(), Value::Null);

        s.doc_delete("a").unwrap();
        assert_eq!(s.doc_get("a").unwrap(), Value::Null);
        assert_eq!(s.doc_list().unwrap(), vec!["b".to_string()]);
        // Deleting an absent document is not an error.
        assert!(s.doc_delete("gone").is_ok());
    }

    #[test]
    fn document_put_rejects_an_unsafe_key_before_touching_disk() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "com.example.docs");
        assert!(s.doc_put("../escape", json!(1)).is_err());
        assert!(s.doc_get("../escape").is_err());
        assert!(s.doc_delete("../escape").is_err());
    }
}
