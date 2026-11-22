//! Composio extensions: the user's own Composio project signs in to apps
//! (Composio-managed OAuth) and runs their tools over Composio's REST API.
//!
//! The Agent sees each connected toolkit as one more MCP-shaped provider
//! (`composio-<slug>`), so ranking, confirmation and execution reuse the MCP
//! path unchanged. Nothing stays alive between calls: no client, no task. The
//! project API key lives only in the OS credential vault; the frontend never
//! reads it back.
use super::{tool_cache, validate_tool_name, McpCallOutput, McpToolSet, DEFINITIONS_CHANGED};
use grain_core::execution::{DispatchPhase, ExecutionFailure, FailureClass};
use grain_core::settings::ComposioToolkit;
use rmcp::model::{Tool, ToolAnnotations};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

const API: &str = "https://backend.composio.dev/api/v3.1";
const VAULT_SERVICE: &str = "com.grain.composio";
const VAULT_ACCOUNT: &str = "project-api-key";
pub(super) const PROVIDER_PREFIX: &str = "composio-";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const EXECUTE_TIMEOUT: Duration = Duration::from_secs(90);
/// A Composio sign-in link stays valid for ten minutes.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(600);
const SIGN_IN_POLL: Duration = Duration::from_secs(2);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESULT_BYTES: usize = 16 * 1024;
const MAX_ERROR_CHARS: usize = 400;
const MAX_TOOLS: usize = 200;
const MAX_TOOL_PAGES: usize = 4;
/// Saved tool lists older than this are rediscovered, so new Composio tools appear.
const TOOL_CACHE_MAX_AGE: Duration = Duration::from_secs(3 * 24 * 60 * 60);
/// The saved app catalog is shown at once; older than this, it is refreshed
/// in the background for the next visit.
const CATALOG_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const CATALOG_FILE: &str = "composio-catalog.json";
const CATALOG_SCHEMA: u32 = 1;
/// Larger saved catalogs are ignored and fetched again.
const CATALOG_MAX_BYTES: u64 = 4 * 1024 * 1024;
const SIGN_IN_CANCELLED: &str = "Sign-in cancelled.";
const NOT_SET_UP: &str = "Add your Composio API key on the Extensions page first.";

// ---------------------------------------------------------------------------
// Key storage

fn vault_entry() -> Result<keyring::Entry, String> {
    #[cfg(feature = "agent-harness")]
    let service = crate::grain_agent_harness::vault_service(VAULT_SERVICE);
    #[cfg(not(feature = "agent-harness"))]
    let service = VAULT_SERVICE;
    keyring::Entry::new(&service, VAULT_ACCOUNT)
        .map_err(|error| format!("OS credential vault open failed: {error}"))
}

async fn read_key() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| match vault_entry()?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("OS credential vault read failed: {error}")),
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn api_key() -> Result<String, String> {
    read_key().await?.ok_or_else(|| NOT_SET_UP.to_string())
}

async fn write_key(key: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let entry = vault_entry()?;
        match key {
            Some(key) => entry
                .set_password(&key)
                .map_err(|error| format!("OS credential vault write failed: {error}")),
            None => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(format!("OS credential vault delete failed: {error}")),
            },
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Composio reads each kind of key from its own header: project keys
/// (`ak_…`) from `x-api-key`, user keys (`uak_…`) from `x-user-api-key`.
fn key_header(key: &str) -> &'static str {
    if key.starts_with("uak_") {
        "x-user-api-key"
    } else {
        "x-api-key"
    }
}

/// Why a pasted key failed, naming the key Grain needs instead.
fn rejected_key_message(key: &str, error: ApiError) -> String {
    const WHERE: &str = "Grain needs a project API key: in the Composio dashboard open \
        Settings → Project Settings → API Keys and copy the key that starts with ak_.";
    match error {
        ApiError::Status {
            status: 401 | 403,
            message,
        } => {
            if key.starts_with("ck_") {
                format!(
                    "That's a key from Composio's \"For You\" app, which Grain can't use. \
                     Click Switch (top-left in Composio) and pick the developer platform. {WHERE}"
                )
            } else {
                format!("Composio didn't accept that key ({message}). {WHERE}")
            }
        }
        other => other.message(),
    }
}

/// A project key is short printable ASCII; anything else is a paste mistake.
fn normalize_key(raw: &str) -> Result<String, String> {
    let key = raw.trim();
    if key.is_empty() {
        return Err("Paste your Composio API key.".into());
    }
    if key.len() > 256 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("That doesn't look like a Composio API key.".into());
    }
    Ok(key.to_string())
}

// ---------------------------------------------------------------------------
// HTTP

#[derive(Debug)]
enum ApiError {
    /// The request never left this machine (DNS, connect, TLS).
    NotSent(String),
    /// Sent, but no complete answer arrived (timeout, dropped connection).
    NoAnswer(String),
    /// Composio answered with an error status.
    Status { status: u16, message: String },
}

impl ApiError {
    fn message(&self) -> String {
        match self {
            Self::NotSent(message) | Self::NoAnswer(message) => message.clone(),
            Self::Status {
                status: 401 | 403, ..
            } => "Composio didn't accept the API key. Check it on the Extensions page.".into(),
            Self::Status { message, .. } => message.clone(),
        }
    }
}

fn http_client(timeout: Duration) -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(15))
        .https_only(true)
        .build()
        .map_err(|error| ApiError::NotSent(format!("Composio client unavailable: {error}")))
}

fn clip(text: &str, max_chars: usize) -> String {
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push('…');
    }
    out
}

fn error_message(status: u16, body: &[u8]) -> String {
    let detail = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value["error"]["message"]
                .as_str()
                .or_else(|| value["message"].as_str())
                .map(str::to_owned)
        });
    match detail {
        Some(detail) => format!("Composio: {}", clip(&detail, MAX_ERROR_CHARS)),
        None => format!("Composio returned HTTP {status}."),
    }
}

async fn send(
    key: &str,
    method: reqwest::Method,
    path: &str,
    query: &[(&str, &str)],
    body: Option<&Value>,
    timeout: Duration,
) -> Result<Value, ApiError> {
    let client = http_client(timeout)?;
    let mut request = client
        .request(method, format!("{API}{path}"))
        .header(key_header(key), key)
        .header("accept", "application/json")
        .query(query);
    if let Some(body) = body {
        request = request.json(body);
    }
    let mut response = request.send().await.map_err(|error| {
        if error.is_connect() || error.is_builder() || error.is_request() && !error.is_timeout() {
            ApiError::NotSent("Couldn't reach Composio. Check your connection.".into())
        } else {
            ApiError::NoAnswer("Composio didn't answer in time.".into())
        }
    })?;
    let status = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|length| length as usize > MAX_RESPONSE_BYTES)
    {
        return Err(ApiError::NoAnswer(
            "Composio's answer was too large.".into(),
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ApiError::NoAnswer("Composio's answer was cut off.".into()))?
    {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(ApiError::NoAnswer(
                "Composio's answer was too large.".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    if !(200..300).contains(&status) {
        return Err(ApiError::Status {
            status,
            message: error_message(status, &bytes),
        });
    }
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::NoAnswer("Composio sent an unreadable answer.".into()))
}

async fn get(key: &str, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
    send(
        key,
        reqwest::Method::GET,
        path,
        query,
        None,
        REQUEST_TIMEOUT,
    )
    .await
    .map_err(|error| error.message())
}

async fn post(key: &str, path: &str, body: &Value) -> Result<Value, String> {
    send(
        key,
        reqwest::Method::POST,
        path,
        &[],
        Some(body),
        REQUEST_TIMEOUT,
    )
    .await
    .map_err(|error| error.message())
}

// ---------------------------------------------------------------------------
// Settings

fn context(app: &AppHandle) -> Result<Arc<grain_core::AppContext>, String> {
    app.try_state::<Arc<grain_core::AppContext>>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| "application context unavailable".to_string())
}

fn update(
    app: &AppHandle,
    apply: impl FnOnce(&mut crate::settings::AppSettings),
) -> Result<(), String> {
    context(app)?
        .update_settings(apply)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn toolkits(app: &AppHandle) -> Vec<ComposioToolkit> {
    crate::settings::get_settings(app).composio_toolkits
}

fn find(app: &AppHandle, slug: &str) -> Option<ComposioToolkit> {
    toolkits(app)
        .into_iter()
        .find(|toolkit| toolkit.slug == slug)
}

fn user_id(app: &AppHandle) -> Result<String, String> {
    let current = crate::settings::get_settings(app).composio_user_id;
    if !current.is_empty() {
        return Ok(current);
    }
    let fresh = format!("grain-{}", uuid::Uuid::new_v4().simple());
    let mut chosen = fresh.clone();
    update(app, |settings| {
        if settings.composio_user_id.is_empty() {
            settings.composio_user_id = fresh;
        }
        chosen = settings.composio_user_id.clone();
    })?;
    Ok(chosen)
}

fn valid_slug(slug: &str) -> Result<String, String> {
    let slug = slug.trim().to_ascii_lowercase();
    if slug.is_empty()
        || slug.len() > 64
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("Unknown Composio app.".into());
    }
    Ok(slug)
}

fn provider_id(slug: &str) -> String {
    format!("{PROVIDER_PREFIX}{slug}")
}

fn slug_of(provider_id: &str) -> Option<&str> {
    provider_id.strip_prefix(PROVIDER_PREFIX)
}

// ---------------------------------------------------------------------------
// Frontend commands

#[derive(Serialize, Type, Clone, Debug)]
pub struct ComposioStatus {
    /// A project API key is saved in the OS vault.
    pub configured: bool,
    pub toolkits: Vec<ComposioToolkit>,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug)]
pub struct ComposioCatalogEntry {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub logo: String,
    /// Usable without signing in to an account.
    pub no_auth: bool,
}

async fn status(app: &AppHandle) -> Result<ComposioStatus, String> {
    Ok(ComposioStatus {
        configured: read_key().await?.is_some(),
        toolkits: toolkits(app),
    })
}

/// Saved state; with `refresh`, first drops toolkits whose Composio account
/// was removed or expired elsewhere (best effort, offline keeps the list).
#[tauri::command]
#[specta::specta]
pub async fn composio_status(
    app: AppHandle,
    window: WebviewWindow,
    refresh: bool,
) -> Result<ComposioStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    if refresh {
        if let Some(key) = read_key().await? {
            let _ = prune_lost_accounts(&app, &key).await;
        }
    }
    status(&app).await
}

async fn prune_lost_accounts(app: &AppHandle, key: &str) -> Result<(), String> {
    let accounts: Vec<String> = toolkits(app)
        .into_iter()
        .filter_map(|toolkit| toolkit.connected_account_id)
        .collect();
    if accounts.is_empty() {
        return Ok(());
    }
    let ids = accounts.join(",");
    let page = get(
        key,
        "/connected_accounts",
        &[("connected_account_ids", &ids), ("limit", "200")],
    )
    .await?;
    let active: Vec<String> = page["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["status"].as_str() == Some("ACTIVE"))
        .filter_map(|item| item["id"].as_str().map(str::to_owned))
        .collect();
    let lost: Vec<ComposioToolkit> = toolkits(app)
        .into_iter()
        .filter(|toolkit| {
            toolkit
                .connected_account_id
                .as_ref()
                .is_some_and(|id| !active.contains(id))
        })
        .collect();
    if lost.is_empty() {
        return Ok(());
    }
    update(app, |settings| {
        settings
            .composio_toolkits
            .retain(|toolkit| !lost.contains(toolkit));
    })?;
    for toolkit in &lost {
        tool_cache::clear(app, &cache_account(toolkit)).await;
    }
    Ok(())
}

/// Check the key with Composio, then save it. A different key means a
/// different Composio project, so connections from the old one are forgotten.
#[tauri::command]
#[specta::specta]
pub async fn composio_set_api_key(
    app: AppHandle,
    window: WebviewWindow,
    key: String,
) -> Result<ComposioStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    let key = normalize_key(&key)?;
    send(
        &key,
        reqwest::Method::GET,
        "/toolkits",
        &[("limit", "1")],
        None,
        REQUEST_TIMEOUT,
    )
    .await
    .map_err(|error| rejected_key_message(&key, error))?;
    if read_key().await?.as_deref() != Some(key.as_str()) {
        forget_all(&app).await?;
    }
    write_key(Some(key)).await?;
    user_id(&app)?;
    status(&app).await
}

/// Forget the key and every Composio connection on this install. Accounts
/// stay in the user's Composio project, where they can remove them.
#[tauri::command]
#[specta::specta]
pub async fn composio_remove_api_key(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<ComposioStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    forget_all(&app).await?;
    write_key(None).await?;
    status(&app).await
}

async fn forget_all(app: &AppHandle) -> Result<(), String> {
    let previous = toolkits(app);
    update(app, |settings| settings.composio_toolkits.clear())?;
    for toolkit in &previous {
        tool_cache::clear(app, &cache_account(toolkit)).await;
    }
    if let Some(path) = catalog_path(app) {
        let _ = tokio::fs::remove_file(path).await;
    }
    Ok(())
}

fn catalog_entry(item: &Value) -> Option<ComposioCatalogEntry> {
    let slug = valid_slug(item["slug"].as_str()?).ok()?;
    let no_auth = item["no_auth"].as_bool().unwrap_or(false);
    let managed = item["composio_managed_auth_schemes"]
        .as_array()
        .is_some_and(|schemes| !schemes.is_empty());
    // Only apps that connect in one click: Composio's own sign-in, or none.
    if !no_auth && !managed {
        return None;
    }
    Some(ComposioCatalogEntry {
        name: clip(item["name"].as_str().unwrap_or(&slug), 80),
        description: clip(
            item["meta"]["description"].as_str().unwrap_or_default(),
            240,
        ),
        logo: item["meta"]["logo"]
            .as_str()
            .filter(|url| url.starts_with("https://"))
            .unwrap_or_default()
            .to_string(),
        slug,
        no_auth,
    })
}

/// Apps the user can connect, most used first. Served from the on-disk copy
/// when there is one, so the page opens at once; a copy older than
/// [`CATALOG_MAX_AGE`] is refreshed in the background for the next visit.
/// Nothing is held in memory between calls.
#[tauri::command]
#[specta::specta]
pub async fn composio_catalog(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Vec<ComposioCatalogEntry>, String> {
    crate::grain_commands::require_main_window(&window)?;
    let key = api_key().await?;
    if let Some((entries, age)) = load_catalog(&app, &key).await {
        if age > CATALOG_MAX_AGE && !CATALOG_REFRESHING.swap(true, Ordering::AcqRel) {
            tauri::async_runtime::spawn(async move {
                if let Ok(fresh) = fetch_catalog(&key).await {
                    // The key may have been removed or replaced meanwhile;
                    // never write a catalog back for a key that is gone.
                    if read_key().await.ok().flatten().as_deref() == Some(key.as_str()) {
                        store_catalog(&app, &key, &fresh).await;
                    }
                }
                CATALOG_REFRESHING.store(false, Ordering::Release);
            });
        }
        return Ok(entries);
    }
    let entries = fetch_catalog(&key).await?;
    store_catalog(&app, &key, &entries).await;
    Ok(entries)
}

/// One background catalog refresh at a time.
static CATALOG_REFRESHING: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Deserialize)]
struct SavedCatalog {
    schema: u32,
    /// SHA-256 of the API key it was fetched with; another project's key
    /// never sees this copy. The key itself is never written here.
    key: String,
    entries: Vec<ComposioCatalogEntry>,
}

fn catalog_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    let context = app.try_state::<Arc<grain_core::AppContext>>()?;
    Some(context.data_dir.join(CATALOG_FILE))
}

fn key_fingerprint(key: &str) -> String {
    format!("{:x}", Sha256::digest(key.as_bytes()))
}

/// The saved catalog and its age, when it was fetched with `key`.
async fn load_catalog(app: &AppHandle, key: &str) -> Option<(Vec<ComposioCatalogEntry>, Duration)> {
    let path = catalog_path(app)?;
    let metadata = tokio::fs::metadata(&path).await.ok()?;
    if !metadata.is_file() || metadata.len() > CATALOG_MAX_BYTES {
        return None;
    }
    let age = metadata.modified().ok()?.elapsed().unwrap_or_default();
    let saved: SavedCatalog = serde_json::from_slice(&tokio::fs::read(&path).await.ok()?).ok()?;
    (saved.schema == CATALOG_SCHEMA && saved.key == key_fingerprint(key))
        .then_some((saved.entries, age))
}

/// Best effort: a failed write only costs a live fetch next time.
async fn store_catalog(app: &AppHandle, key: &str, entries: &[ComposioCatalogEntry]) {
    let Some(path) = catalog_path(app) else {
        return;
    };
    let saved = SavedCatalog {
        schema: CATALOG_SCHEMA,
        key: key_fingerprint(key),
        entries: entries.to_vec(),
    };
    let Ok(bytes) = serde_json::to_vec(&saved) else {
        return;
    };
    if bytes.len() as u64 > CATALOG_MAX_BYTES {
        return;
    }
    let tmp = path.with_extension("json.tmp");
    if tokio::fs::write(&tmp, &bytes).await.is_ok() && tokio::fs::rename(&tmp, &path).await.is_err()
    {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
}

async fn fetch_catalog(key: &str) -> Result<Vec<ComposioCatalogEntry>, String> {
    let mut entries = Vec::new();
    let mut cursor = String::new();
    for _ in 0..4 {
        let mut query = vec![("limit", "1000"), ("sort_by", "usage")];
        if !cursor.is_empty() {
            query.push(("cursor", cursor.as_str()));
        }
        let page = get(key, "/toolkits", &query).await?;
        entries.extend(
            page["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(catalog_entry),
        );
        match page["next_cursor"].as_str() {
            Some(next) if !next.is_empty() && next != cursor => cursor = next.to_string(),
            _ => break,
        }
    }
    let mut seen = std::collections::HashSet::new();
    entries.retain(|entry| seen.insert(entry.slug.clone()));
    Ok(entries)
}

static SIGNING_IN: Mutex<BTreeMap<String, oneshot::Sender<()>>> = Mutex::new(BTreeMap::new());

struct SignInGuard(String);

impl Drop for SignInGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = SIGNING_IN.lock() {
            map.remove(&self.0);
        }
    }
}

/// The Composio-managed auth config for a toolkit, created on first use.
async fn managed_auth_config(key: &str, slug: &str) -> Result<String, String> {
    let existing = get(
        key,
        "/auth_configs",
        &[
            ("toolkit_slug", slug),
            ("is_composio_managed", "true"),
            ("limit", "50"),
        ],
    )
    .await?;
    let reusable = existing["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| {
            item["is_composio_managed"].as_bool() == Some(true)
                && item["status"].as_str() != Some("DISABLED")
                && item["toolkit"]["slug"]
                    .as_str()
                    .is_some_and(|value| value.eq_ignore_ascii_case(slug))
        });
    if let Some(id) = reusable.and_then(|item| item["id"].as_str()) {
        return Ok(id.to_string());
    }
    let created = post(
        key,
        "/auth_configs",
        &json!({
            "toolkit": { "slug": slug },
            "auth_config": { "type": "use_composio_managed_auth" },
        }),
    )
    .await?;
    created["auth_config"]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Composio didn't return a sign-in setup for this app.".into())
}

/// Wait until the account Composio created for this sign-in is usable.
async fn await_active(
    key: &str,
    account_id: &str,
    mut cancel: oneshot::Receiver<()>,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + SIGN_IN_TIMEOUT;
    loop {
        tokio::select! {
            _ = &mut cancel => return Err(SIGN_IN_CANCELLED.into()),
            _ = tokio::time::sleep(SIGN_IN_POLL) => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Sign-in timed out. Try Connect again.".into());
        }
        let account = match get(key, &format!("/connected_accounts/{account_id}"), &[]).await {
            Ok(account) => account,
            // A dropped poll is not a failed sign-in; ask again.
            Err(_) => continue,
        };
        match account["status"].as_str().unwrap_or_default() {
            "ACTIVE" => return Ok(()),
            "FAILED" | "EXPIRED" | "INACTIVE" => {
                let reason = account["status_reason"].as_str().unwrap_or_default();
                return Err(if reason.is_empty() {
                    "Sign-in didn't finish. Try Connect again.".into()
                } else {
                    format!("Sign-in failed: {}", clip(reason, MAX_ERROR_CHARS))
                });
            }
            _ => {}
        }
    }
}

async fn delete_account(key: &str, account_id: &str) -> Result<(), String> {
    match send(
        key,
        reqwest::Method::DELETE,
        &format!("/connected_accounts/{account_id}"),
        &[],
        None,
        REQUEST_TIMEOUT,
    )
    .await
    {
        Ok(_) | Err(ApiError::Status { status: 404, .. }) => Ok(()),
        Err(error) => Err(error.message()),
    }
}

/// Connect one app: Composio's sign-in page opens in the browser and Grain
/// waits for the account to become active. Apps without accounts connect at once.
#[tauri::command]
#[specta::specta]
pub async fn composio_connect(
    app: AppHandle,
    window: WebviewWindow,
    slug: String,
) -> Result<ComposioStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    let slug = valid_slug(&slug)?;
    let key = api_key().await?;
    let user = user_id(&app)?;
    let (cancel_tx, cancel_rx) = oneshot::channel();
    {
        let mut map = SIGNING_IN.lock().map_err(|_| "sign-in state unavailable")?;
        if map.contains_key(&slug) {
            return Err("Already signing in to this app.".into());
        }
        map.insert(slug.clone(), cancel_tx);
    }
    let _guard = SignInGuard(slug.clone());
    let info = get(&key, &format!("/toolkits/{slug}"), &[]).await?;
    let entry = catalog_entry(&info)
        .ok_or("This app needs its own sign-in setup, which Grain doesn't support yet.")?;
    let connected_account_id = if entry.no_auth {
        None
    } else {
        let auth_config_id = managed_auth_config(&key, &slug).await?;
        let link = post(
            &key,
            "/connected_accounts/link",
            &json!({ "user_id": user, "auth_config_id": auth_config_id }),
        )
        .await?;
        let account_id = link["connected_account_id"]
            .as_str()
            .ok_or("Composio didn't return a sign-in link.")?
            .to_string();
        let url = link["redirect_url"]
            .as_str()
            .filter(|url| url.starts_with("https://"))
            .ok_or("Composio didn't return a sign-in link.")?;
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|error| format!("Couldn't open the browser: {error}"))?;
        if let Err(error) = await_active(&key, &account_id, cancel_rx).await {
            // Leave nothing half-made in the user's Composio project.
            let _ = delete_account(&key, &account_id).await;
            return Err(error);
        }
        Some(account_id)
    };
    let previous = find(&app, &slug);
    let toolkit = ComposioToolkit {
        slug: slug.clone(),
        name: entry.name,
        description: entry.description,
        logo: entry.logo,
        connected_account_id,
        enabled: true,
    };
    update(&app, |settings| {
        settings.composio_toolkits.retain(|item| item.slug != slug);
        settings.composio_toolkits.push(toolkit.clone());
        settings
            .composio_toolkits
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    })?;
    if let Some(previous) = previous {
        tool_cache::clear(&app, &cache_account(&previous)).await;
        if let Some(old) = previous.connected_account_id {
            if Some(&old) != toolkit.connected_account_id.as_ref() {
                let _ = delete_account(&key, &old).await;
            }
        }
    }
    // Warm the Agent's saved tool list; a failure only means first use discovers it.
    let _ = list_tools(&app, &provider_id(&slug)).await;
    status(&app).await
}

#[tauri::command]
#[specta::specta]
pub fn composio_cancel_connect(window: WebviewWindow, slug: String) -> Result<(), String> {
    crate::grain_commands::require_main_window(&window)?;
    let slug = valid_slug(&slug)?;
    if let Some(sender) = SIGNING_IN
        .lock()
        .map_err(|_| "sign-in state unavailable")?
        .remove(&slug)
    {
        let _ = sender.send(());
    }
    Ok(())
}

/// Remove the app from Grain and its account from the user's Composio project.
#[tauri::command]
#[specta::specta]
pub async fn composio_disconnect(
    app: AppHandle,
    window: WebviewWindow,
    slug: String,
) -> Result<ComposioStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    let slug = valid_slug(&slug)?;
    let toolkit = find(&app, &slug).ok_or("This app isn't connected.")?;
    if let Some(account_id) = &toolkit.connected_account_id {
        if let Some(key) = read_key().await? {
            delete_account(&key, account_id).await?;
        }
    }
    update(&app, |settings| {
        settings.composio_toolkits.retain(|item| item.slug != slug)
    })?;
    tool_cache::clear(&app, &cache_account(&toolkit)).await;
    status(&app).await
}

#[tauri::command]
#[specta::specta]
pub async fn composio_set_enabled(
    app: AppHandle,
    window: WebviewWindow,
    slug: String,
    enabled: bool,
) -> Result<ComposioStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    let slug = valid_slug(&slug)?;
    let mut found = false;
    update(&app, |settings| {
        for toolkit in &mut settings.composio_toolkits {
            if toolkit.slug == slug {
                toolkit.enabled = enabled;
                found = true;
            }
        }
    })?;
    if !found {
        return Err("This app isn't connected.".into());
    }
    status(&app).await
}

// ---------------------------------------------------------------------------
// Agent provider

pub(super) fn directory(
    app: &AppHandle,
) -> Vec<grain_core::capability_index::ExtensionDirectoryEntry> {
    toolkits(app)
        .into_iter()
        .filter(|toolkit| toolkit.enabled)
        .map(
            |toolkit| grain_core::capability_index::ExtensionDirectoryEntry {
                extension_id: format!("mcp.{}", provider_id(&toolkit.slug)),
                name: toolkit.name,
                description: toolkit.description,
                // Tools are discovered lazily, like remote MCP servers.
                action_count: 1,
            },
        )
        .collect()
}

pub(super) fn is_provider(provider_id: &str) -> bool {
    provider_id.starts_with(PROVIDER_PREFIX)
}

fn enabled_toolkit(app: &AppHandle, provider_id: &str) -> Option<ComposioToolkit> {
    let slug = slug_of(provider_id)?;
    find(app, slug).filter(|toolkit| toolkit.enabled)
}

pub(super) fn is_enabled(app: &AppHandle, provider_id: &str) -> bool {
    enabled_toolkit(app, provider_id).is_some()
}

fn cache_account(toolkit: &ComposioToolkit) -> String {
    format!(
        "composio:{}:{}",
        toolkit.slug,
        toolkit.connected_account_id.as_deref().unwrap_or("none")
    )
}

fn digest(slug: &str, tools: &[Tool]) -> Result<String, String> {
    let mut canonical: Vec<_> = tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": tool.description,
                "inputSchema": tool.input_schema,
            })
        })
        .collect();
    canonical.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let bytes = serde_json::to_vec(&(PROVIDER_PREFIX, slug, API, canonical))
        .map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn cache_key<'a>(account: &'a str, binding: &'a str) -> tool_cache::Key<'a> {
    tool_cache::Key {
        account,
        endpoint: API,
        binding,
    }
}

/// One Composio tool as the MCP `Tool` the capability index already ranks.
fn to_tool(item: &Value) -> Option<(bool, Tool)> {
    if item["is_deprecated"].as_bool() == Some(true) {
        return None;
    }
    let name = item["slug"].as_str()?;
    validate_tool_name(name).ok()?;
    let schema = item["input_parameters"].as_object()?.clone();
    grain_core::tool_schema::validate_definition(&Value::Object(schema.clone())).ok()?;
    let tags: Vec<&str> = item["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let has = |tag: &str| tags.iter().any(|value| value.eq_ignore_ascii_case(tag));
    let description = item["description"]
        .as_str()
        .or_else(|| item["human_description"].as_str())
        .unwrap_or(name);
    let mut annotations = ToolAnnotations::new();
    if has("readOnlyHint") {
        annotations = annotations.read_only(true);
    }
    if has("destructiveHint") {
        annotations = annotations.destructive(true);
    }
    if has("idempotentHint") {
        annotations = annotations.idempotent(true);
    }
    let mut tool = Tool::new(name.to_string(), clip(description, 1_200), Arc::new(schema))
        .with_annotations(annotations);
    if let Some(title) = item["name"].as_str() {
        tool = tool.with_title(clip(title, 160));
    }
    Some((has("important"), tool))
}

async fn fetch_tools(key: &str, slug: &str) -> Result<Vec<Tool>, String> {
    let mut found: Vec<(bool, Tool)> = Vec::new();
    let mut cursor = String::new();
    for _ in 0..MAX_TOOL_PAGES {
        let mut query = vec![("toolkit_slug", slug), ("limit", "500")];
        if !cursor.is_empty() {
            query.push(("cursor", cursor.as_str()));
        }
        let page = get(key, "/tools", &query).await?;
        found.extend(
            page["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(to_tool),
        );
        match page["next_cursor"].as_str() {
            Some(next) if !next.is_empty() && next != cursor => cursor = next.to_string(),
            _ => break,
        }
    }
    // Large toolkits (GitHub has hundreds) keep Composio's important tools first.
    found.sort_by_key(|(important, _)| !*important);
    let mut seen = std::collections::HashSet::new();
    let tools: Vec<Tool> = found
        .into_iter()
        .map(|(_, tool)| tool)
        .filter(|tool| seen.insert(tool.name.to_string()))
        .take(MAX_TOOLS)
        .collect();
    if tools.is_empty() {
        return Err("Composio listed no usable tools for this app.".into());
    }
    Ok(tools)
}

fn tool_set(toolkit: &ComposioToolkit, digest: String, tools: Vec<Tool>) -> McpToolSet {
    McpToolSet {
        provider_id: provider_id(&toolkit.slug),
        provider_name: toolkit.name.clone(),
        digest,
        tools,
    }
}

pub(super) async fn cached_tools(app: &AppHandle, provider_id: &str) -> Option<McpToolSet> {
    let toolkit = enabled_toolkit(app, provider_id)?;
    let account = cache_account(&toolkit);
    if tool_cache::age(app, &account)
        .await
        .is_none_or(|age| age > TOOL_CACHE_MAX_AGE)
    {
        return None;
    }
    let (raw_digest, tools) = tool_cache::load(app, &cache_key(&account, &toolkit.slug)).await?;
    if digest(&toolkit.slug, &tools).ok().as_deref() != Some(raw_digest.as_str()) {
        tool_cache::clear(app, &account).await;
        return None;
    }
    Some(tool_set(&toolkit, raw_digest, tools))
}

pub(super) async fn list_tools(app: &AppHandle, provider_id: &str) -> Result<McpToolSet, String> {
    let toolkit =
        enabled_toolkit(app, provider_id).ok_or("This Composio app is off or not connected.")?;
    let key = api_key().await?;
    let tools = fetch_tools(&key, &toolkit.slug).await?;
    let raw_digest = digest(&toolkit.slug, &tools)?;
    let account = cache_account(&toolkit);
    tool_cache::store(
        app,
        &cache_key(&account, &toolkit.slug),
        &raw_digest,
        &tools,
    )
    .await;
    Ok(tool_set(&toolkit, raw_digest, tools))
}

fn before(class: FailureClass, message: impl Into<String>) -> ExecutionFailure {
    ExecutionFailure::new(DispatchPhase::NotDispatched, class, message)
}

fn output_text(data: &Value) -> (String, bool) {
    let text = match data {
        Value::String(text) => text.clone(),
        Value::Null => "Done.".into(),
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    if text.len() <= MAX_RESULT_BYTES {
        return (text, false);
    }
    let mut end = MAX_RESULT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

pub(super) async fn call_tool(
    app: &AppHandle,
    provider_id: &str,
    tool_name: &str,
    arguments: &Value,
    expected_digest: &str,
) -> Result<McpCallOutput, ExecutionFailure> {
    let toolkit = enabled_toolkit(app, provider_id)
        .ok_or_else(|| before(FailureClass::Cancelled, "This Composio app is off."))?;
    if !arguments.is_object() {
        return Err(before(
            FailureClass::InvalidArgument,
            "Tool arguments must be an object.",
        ));
    }
    // The Agent chose from a tool list; refuse if that list has since changed.
    let account = cache_account(&toolkit);
    let current = tool_cache::load(app, &cache_key(&account, &toolkit.slug)).await;
    match current {
        Some((digest, tools))
            if digest == expected_digest
                && tools.iter().any(|tool| tool.name.as_ref() == tool_name) => {}
        _ => {
            tool_cache::clear(app, &account).await;
            return Err(before(FailureClass::Cancelled, DEFINITIONS_CHANGED));
        }
    }
    let key = api_key()
        .await
        .map_err(|message| before(FailureClass::Auth, message))?;
    let user = user_id(app).map_err(|message| before(FailureClass::Internal, message))?;
    let mut body = json!({ "arguments": arguments, "user_id": user });
    if let Some(account_id) = &toolkit.connected_account_id {
        body["connected_account_id"] = json!(account_id);
    }
    let started = std::time::Instant::now();
    let result = send(
        &key,
        reqwest::Method::POST,
        &format!("/tools/execute/{tool_name}"),
        &[],
        Some(&body),
        EXECUTE_TIMEOUT,
    )
    .await;
    log::debug!(
        "[composio] {tool_name} in {} ms (ok={})",
        started.elapsed().as_millis(),
        result.is_ok()
    );
    let reply = result.map_err(|error| match error {
        ApiError::NotSent(message) => before(FailureClass::Network, message),
        ApiError::NoAnswer(message) => {
            ExecutionFailure::new(DispatchPhase::Dispatched, FailureClass::Network, message)
        }
        ApiError::Status { status, message } => {
            let (class, message) = match status {
                401 | 403 => (
                    FailureClass::Auth,
                    "Composio didn't accept the API key. Check it on the Extensions page.".into(),
                ),
                422 => (
                    FailureClass::Auth,
                    format!("Reconnect {} on the Extensions page.", toolkit.name),
                ),
                404 => (FailureClass::NotFound, message),
                400 => (FailureClass::InvalidArgument, message),
                429 => (FailureClass::RateLimited, message),
                _ => (FailureClass::Network, message),
            };
            ExecutionFailure::new(DispatchPhase::ResponseReceived, class, message)
        }
    })?;
    let successful = reply["successful"].as_bool().unwrap_or(false);
    if !successful {
        let error = reply["error"].as_str().unwrap_or("The tool failed.");
        return Ok(McpCallOutput {
            text: clip(error, 2_000),
            is_error: true,
            structured_content: None,
            truncated: false,
            unsupported_content: Vec::new(),
        });
    }
    let (text, truncated) = output_text(&reply["data"]);
    Ok(McpCallOutput {
        text,
        is_error: false,
        structured_content: None,
        truncated,
        unsupported_content: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_must_be_printable() {
        assert_eq!(normalize_key("  ak_abc123 \n").unwrap(), "ak_abc123");
        assert!(normalize_key("").is_err());
        assert!(normalize_key("ak abc").is_err());
    }

    #[test]
    fn each_key_kind_goes_in_its_own_header_and_wrong_kinds_are_named() {
        assert_eq!(key_header("ak_123"), "x-api-key");
        assert_eq!(key_header("uak_123"), "x-user-api-key");
        let rejected = |key| {
            rejected_key_message(
                key,
                ApiError::Status {
                    status: 401,
                    message: "Composio: Invalid API key".into(),
                },
            )
        };
        assert!(rejected("ck_123").contains("Switch"));
        assert!(rejected("ak_123").contains("Invalid API key"));
        assert!(rejected("ak_123").contains("starts with ak_"));
    }

    #[test]
    fn slug_is_normalized_and_bounded() {
        assert_eq!(valid_slug(" Gmail ").unwrap(), "gmail");
        assert!(valid_slug("../x").is_err());
        assert!(valid_slug("").is_err());
    }

    #[test]
    fn catalog_keeps_one_click_apps_only() {
        let managed = json!({"slug":"gmail","name":"Gmail","composio_managed_auth_schemes":["OAUTH2"],"meta":{"logo":"https://logos.composio.dev/gmail.png","description":"Mail"}});
        let open = json!({"slug":"hackernews","name":"Hacker News","no_auth":true,"meta":{}});
        let byo = json!({"slug":"googlechat","name":"Google Chat","composio_managed_auth_schemes":[],"meta":{}});
        assert_eq!(
            catalog_entry(&managed).unwrap().logo,
            "https://logos.composio.dev/gmail.png"
        );
        assert!(catalog_entry(&open).unwrap().no_auth);
        assert!(catalog_entry(&byo).is_none());
    }

    #[test]
    fn tool_conversion_reads_hints_and_skips_deprecated() {
        let item = json!({
            "slug": "GMAIL_FETCH_EMAILS",
            "name": "Fetch emails",
            "description": "List recent emails",
            "tags": ["readOnlyHint", "important"],
            "input_parameters": {"type": "object", "properties": {"max_results": {"type": "integer"}}},
        });
        let (important, tool) = to_tool(&item).unwrap();
        assert!(important);
        assert_eq!(tool.annotations.unwrap().read_only_hint, Some(true));
        let mut old = item.clone();
        old["is_deprecated"] = json!(true);
        assert!(to_tool(&old).is_none());
    }

    #[test]
    fn digest_ignores_order() {
        let a = to_tool(&json!({"slug":"A_X","input_parameters":{"type":"object"}}))
            .unwrap()
            .1;
        let b = to_tool(&json!({"slug":"A_Y","input_parameters":{"type":"object"}}))
            .unwrap()
            .1;
        assert_eq!(
            digest("a", &[a.clone(), b.clone()]).unwrap(),
            digest("a", &[b, a]).unwrap()
        );
    }

    #[test]
    fn large_results_are_cut_on_a_char_boundary() {
        let (text, truncated) = output_text(&json!("é".repeat(MAX_RESULT_BYTES)));
        assert!(truncated);
        assert!(text.len() <= MAX_RESULT_BYTES);
    }
}
