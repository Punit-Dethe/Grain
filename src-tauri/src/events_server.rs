//! [GRAIN] B1: local WebSocket that streams `DaemonEvent`s to the pill.
//!
//! The core listens on `127.0.0.1:EVENTS_PORT`; each connecting client (the pill)
//! subscribes to the `AppContext` broadcast bus and receives every event as JSON.
//! This is the seed of the future local server (the OpenAI-compatible endpoints
//! grow on the same listener later).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use grain_core::AppContext;
use tauri::AppHandle;
use tokio::net::TcpStream;
use tokio::sync::broadcast::error::RecvError;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{ErrorResponse, Request, Response},
        http::{header::ORIGIN, StatusCode, Uri},
        Message,
    },
};

/// Fixed loopback port the pill connects to (`ws://127.0.0.1:EVENTS_PORT`).
pub const EVENTS_PORT: u16 = 7124;
static UNAUTHENTICATED_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
const MAX_UNAUTHENTICATED_CONNECTIONS: usize = 64;

/// [GRAIN] SPEC §7.1: the server-side token → identity table. Minted per app
/// run; the pill's token is injected into its environment at spawn. A
/// connection that hasn't authenticated with a registered token within
/// [`AUTH_DEADLINE`] receives nothing and is dropped.
static TOKENS: std::sync::OnceLock<crate::events_auth::TokenRegistry> = std::sync::OnceLock::new();
static DEV_TOKEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static DEV_CONNECTIONS: std::sync::Mutex<Vec<tokio::sync::mpsc::UnboundedSender<()>>> =
    std::sync::Mutex::new(Vec::new());
const AUTH_DEADLINE: Duration = Duration::from_secs(3);
pub const DEV_TOKEN_FILE: &str = "extension-dev-token.json";

/// A connection occupies one pre-authentication slot from accept until its
/// first frame proves an identity. The guard makes every early return release
/// the slot, including handshake failures and authentication timeouts.
struct PreAuthGuard<'a> {
    count: &'a AtomicUsize,
    active: bool,
}

impl<'a> PreAuthGuard<'a> {
    fn acquire(count: &'a AtomicUsize, limit: usize) -> Option<Self> {
        count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < limit).then_some(current + 1)
            })
            .ok()
            .map(|_| Self {
                count,
                active: true,
            })
    }

    fn release(&mut self) {
        if self.active {
            self.count.fetch_sub(1, Ordering::AcqRel);
            self.active = false;
        }
    }
}

impl Drop for PreAuthGuard<'_> {
    fn drop(&mut self) {
        self.release();
    }
}

/// Browsers do not apply same-origin policy to WebSockets, so the local server
/// must reject browser handshakes from arbitrary sites. Native clients (the
/// pill and, later, native extensions) omit Origin and remain valid.
fn origin_allowed(origin: Option<&str>) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };

    match uri.scheme_str() {
        Some(scheme) if scheme.eq_ignore_ascii_case("tauri") => uri.authority().is_some(),
        Some(scheme)
            if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") =>
        {
            uri.host().is_some_and(|host| {
                host.eq_ignore_ascii_case("tauri.localhost")
                    || host.eq_ignore_ascii_case("localhost")
                    || host == "127.0.0.1"
                    || host == "::1"
                    || host == "[::1]"
            })
        }
        _ => false,
    }
}

fn registry() -> &'static crate::events_auth::TokenRegistry {
    TOKENS.get_or_init(crate::events_auth::TokenRegistry::new)
}

/// [GRAIN] SPEC §7.1: mint a per-worker token for an extension, bound to its id
/// and the capability set the user granted. Called at **worker spawn** (not at
/// enable) and paired with [`revoke_token`] at reap/disable — tokens are short-
/// lived, never long-lived. The `Named` set is exactly the extension's grants,
/// so the same server-side filter that gates the pill (`events_auth`) gates the
/// worker: no grant → the message never reaches it.
pub fn mint_worker_token(ext_id: &str, caps: std::collections::HashSet<String>) -> String {
    mint_extension_token(ext_id, caps, crate::events_auth::ClientRole::Worker)
}

fn mint_extension_token(
    ext_id: &str,
    caps: std::collections::HashSet<String>,
    role: crate::events_auth::ClientRole,
) -> String {
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    registry().register(
        token.clone(),
        crate::events_auth::ClientIdentity {
            id: ext_id.to_string(),
            role,
            caps: crate::events_auth::CapabilitySet::Named(caps),
        },
    );
    token
}

/// Revoke a token so its connection is rejected on reconnect and no new one can
/// authenticate with it (worker reaped, extension disabled/uninstalled).
pub fn revoke_token(token: &str) {
    registry().revoke(token);
}

pub fn token_count() -> usize {
    registry().len()
}

#[derive(serde::Serialize)]
struct DevTokenFile<'a> {
    url: &'a str,
    token: &'a str,
}

/// Expose the role-bound developer credential only while developer mode is on.
pub fn enable_dev_control(data_dir: &std::path::Path) -> Result<(), String> {
    let mut active = DEV_TOKEN.lock().unwrap();
    if active.is_some() {
        return Ok(());
    }
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    registry().register(
        token.clone(),
        crate::events_auth::ClientIdentity {
            id: "grain-ext".into(),
            role: crate::events_auth::ClientRole::DevControl,
            caps: crate::events_auth::CapabilitySet::Named(Default::default()),
        },
    );
    std::fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
    let path = data_dir.join(DEV_TOKEN_FILE);
    let body = serde_json::to_vec_pretty(&DevTokenFile {
        url: "ws://127.0.0.1:7124",
        token: &token,
    })
    .map_err(|error| error.to_string())?;
    let write_result = write_private_file(&path, &body);
    if let Err(error) = write_result {
        registry().revoke(&token);
        return Err(error);
    }
    *active = Some(token);
    Ok(())
}

#[cfg(unix)]
fn write_private_file(path: &std::path::Path, body: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    file.write_all(body).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn write_private_file(path: &std::path::Path, body: &[u8]) -> Result<(), String> {
    std::fs::write(path, body).map_err(|error| error.to_string())
}

pub fn disable_dev_control(data_dir: &std::path::Path) {
    if let Some(token) = DEV_TOKEN.lock().unwrap().take() {
        registry().revoke(&token);
    }
    for close in DEV_CONNECTIONS.lock().unwrap().drain(..) {
        let _ = close.send(());
    }
    match std::fs::remove_file(data_dir.join(DEV_TOKEN_FILE)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => log::warn!("[GRAIN] could not remove developer token: {error}"),
    }
}

fn bind_events_listener(
    socket_addr: std::net::SocketAddr,
) -> std::io::Result<tokio::net::TcpListener> {
    let socket = tokio::net::TcpSocket::new_v4()?;
    // Windows' SO_REUSEADDR can let two live dev processes bind the same port
    // and route a new pill to the old core. Keep Windows exclusive; Unix still
    // needs reuse for quick rebinding after a closed listener.
    #[cfg(not(windows))]
    socket.set_reuseaddr(true)?;
    socket.bind(socket_addr)?;
    socket.listen(1024)
}

/// Spawn the event WS server on the Tauri async runtime.
///
/// `app` is carried alongside the headless `ctx` because the reverse channel now
/// needs Tauri-managed state: a Prompt Record click must reach the
/// `AudioRecordingManager` to snapshot the audio split mark. Event emission stays
/// headless (`ctx.emit`).
pub fn start(ctx: Arc<AppContext>, app: AppHandle) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("[GRAIN] events WS runtime failed: {e}");
                return;
            }
        };

        rt.block_on(async move {
            let addr = format!("127.0.0.1:{EVENTS_PORT}");
            let socket_addr = match addr.parse::<std::net::SocketAddr>() {
                Ok(socket_addr) => socket_addr,
                Err(e) => {
                    log::error!("[GRAIN] events WS invalid address: {e}");
                    return;
                }
            };
            // Rapid dev restarts can briefly leave the previous process owning
            // the fixed loopback port. Returning here used to permanently skip
            // both the event transport and pill launch for the new app run.
            // Retry at low frequency until the old listener releases the port;
            // only then is it safe to start the pill supervisor.
            let mut bind_attempt = 0u32;
            let listener = loop {
                match bind_events_listener(socket_addr) {
                    Ok(listener) => break listener,
                    Err(error) => {
                        bind_attempt = bind_attempt.saturating_add(1);
                        if bind_attempt == 1 || bind_attempt % 20 == 0 {
                            log::warn!(
                                "[GRAIN] events WS bind {addr} unavailable ({error}); waiting for the previous dev process"
                            );
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                }
            };
            log::info!("[GRAIN] events WS listening on ws://{addr}");

            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let ctx = ctx.clone();
                        let app = app.clone();
                        tokio::spawn(handle(stream, ctx, app));
                    }
                    Err(e) => log::warn!("[GRAIN] events WS accept error: {e}"),
                }
            }
        });
    });
}

async fn handle(stream: TcpStream, ctx: Arc<AppContext>, app: AppHandle) {
    let mut pre_auth = match PreAuthGuard::acquire(
        &UNAUTHENTICATED_CONNECTIONS,
        MAX_UNAUTHENTICATED_CONNECTIONS,
    ) {
        Some(guard) => guard,
        None => {
            log::warn!(
                "[GRAIN] events WS: unauthenticated connection limit reached ({MAX_UNAUTHENTICATED_CONNECTIONS})"
            );
            return;
        }
    };

    let ws = match accept_hdr_async(stream, |request: &Request, response: Response| {
        let origin = request.headers().get(ORIGIN);
        let allowed = match origin {
            None => origin_allowed(None),
            Some(value) => value
                .to_str()
                .is_ok_and(|value| origin_allowed(Some(value))),
        };
        if allowed {
            return Ok(response);
        }

        let offending = origin
            .and_then(|value| value.to_str().ok())
            .unwrap_or("<invalid Origin header>");
        log::warn!("[GRAIN] events WS: rejected Origin '{offending}'");
        let mut error = ErrorResponse::new(Some("WebSocket Origin not allowed".into()));
        *error.status_mut() = StatusCode::FORBIDDEN;
        Err(error)
    })
    .await
    {
        Ok(ws) => ws,
        Err(e) => {
            log::warn!("[GRAIN] events WS handshake failed: {e}");
            return;
        }
    };
    let (mut write, mut read) = ws.split();

    // [GRAIN] SPEC §7.1: the connection is nobody until its FIRST frame
    // authenticates. No events flow before this; a slow, silent, or unknown
    // client is dropped on the deadline. Identity comes from the server-side
    // registry — nothing a later message claims can change it.
    let session = match tokio::time::timeout(AUTH_DEADLINE, read.next()).await {
        Ok(Some(Ok(Message::Text(txt)))) => match registry().authenticate_session(&txt) {
            Some(session) => session,
            None => {
                log::warn!("[GRAIN] events WS: rejected unauthenticated client");
                return;
            }
        },
        Ok(_) => {
            log::warn!("[GRAIN] events WS: client closed before authenticating");
            return;
        }
        Err(_) => {
            log::warn!("[GRAIN] events WS: auth deadline expired — dropping client");
            return;
        }
    };
    let identity = session.identity;
    pre_auth.release();
    log::info!("[GRAIN] events WS: '{}' authenticated", identity.id);

    // Contract handshake: tell the client which grainApi we speak. Clients
    // that predate the handshake ignore this frame (it isn't a DaemonEvent).
    if let Ok(json) = serde_json::to_string(&grain_sdk::ServerWelcome {
        grain_api: grain_sdk::GRAIN_API_VERSION.into(),
    }) {
        if write.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

    if identity.role == crate::events_auth::ClientRole::DevControl {
        let (close_tx, mut close_rx) = tokio::sync::mpsc::unbounded_channel();
        DEV_CONNECTIONS.lock().unwrap().push(close_tx);
        loop {
            let message = tokio::select! {
                _ = close_rx.recv() => break,
                message = read.next() => message,
            };
            let Some(message) = message else { break };
            let Ok(Message::Text(text)) = message else {
                break;
            };
            let Ok(grain_sdk::DevControlFrame::DevReload {
                request_id,
                extension_id,
            }) = serde_json::from_str(&text)
            else {
                continue;
            };
            let frame = match crate::extension_host::reload_dev_extension(&app, &extension_id) {
                Ok(result) => grain_sdk::DevControlFrame::DevResult {
                    request_id,
                    result: Some(result),
                    error: None,
                },
                Err(error) => grain_sdk::DevControlFrame::DevResult {
                    request_id,
                    result: None,
                    error: Some(error),
                },
            };
            let Ok(json) = serde_json::to_string(&frame) else {
                continue;
            };
            if write.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
        drop(close_rx);
        DEV_CONNECTIONS
            .lock()
            .unwrap()
            .retain(|sender| !sender.is_closed());
        return;
    }

    // [GRAIN] SPEC §7.1: one writer per connection. Broadcast events, host-API
    // responses, and host-initiated calls all funnel through this mpsc so `write`
    // is touched from exactly one place (the `outgoing` arm) — no interleaved
    // partial frames, no borrow fight. `write` is only used here from now on.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Message>();

    // The authenticated role, not its capability set, decides which protocol
    // this socket speaks and whether the worker host tracks it for reaping.
    let is_worker = identity.role == crate::events_auth::ClientRole::Worker;
    let last_activity = if is_worker {
        let Some(activity) =
            crate::extension_host::attach_connection(&identity.id, &session.token, out_tx.clone())
        else {
            return;
        };
        Some(activity)
    } else {
        None
    };

    let mut rx = ctx.subscribe();
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(ev) => {
                    // Capability filter: an identity without the grant never
                    // receives the event at all (SPEC §1.3).
                    if !crate::events_auth::allows_event(&identity, &ev) {
                        continue;
                    }
                    if let Ok(json) = serde_json::to_string(&ev) {
                        if out_tx.send(Message::Text(json.into())).is_err() {
                            break; // writer arm gone
                        }
                    }
                }
                Err(RecvError::Lagged(_)) => continue, // dropped some; keep streaming
                Err(RecvError::Closed) => break,        // bus closed (shutdown)
            },
            // The single writer: everything bound for this socket passes here.
            outgoing = out_rx.recv() => match outgoing {
                Some(m) => {
                    if write.send(m).await.is_err() {
                        break; // client gone
                    }
                }
                None => break, // all senders dropped
            },
            // Inbound frames. Extensions speak HostFrame (host-API requests +
            // answers to host calls). Token identity selects worker or developer
            // control; workers cannot issue application control commands.
            msg = read.next() => match msg {
                Some(Ok(Message::Text(txt))) => {
                    if let Some(la) = &last_activity {
                        // Touch: feeds the extension host's idle reaper.
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        la.store(now, Ordering::Relaxed);
                    }
                    if is_worker {
                        match serde_json::from_str::<grain_sdk::HostFrame>(&txt) {
                            Ok(grain_sdk::HostFrame::Request(req)) => {
                                // Capability-checked host API. Dispatch off the
                                // read loop; the reply returns via the writer arm.
                                let app = app.clone();
                                let identity = identity.clone();
                                let out_tx = out_tx.clone();
                                tokio::spawn(async move {
                                    let developer_logging =
                                        crate::extension_host::is_dev_extension(
                                            &app,
                                            &identity.id,
                                        );
                                    let started = developer_logging
                                        .then(std::time::Instant::now);
                                    let method = developer_logging
                                        .then(|| req.method.replace(['\r', '\n'], " "));
                                    let denied_capability = developer_logging
                                        .then(|| crate::host_api::required_capability(&req.method))
                                        .flatten()
                                        .filter(|capability| {
                                            *capability != "__unknown__"
                                                && !crate::host_api::has_capability(
                                                    &identity,
                                                    capability,
                                                )
                                        });
                                    let resp = match crate::host_api::dispatch(
                                        &app, &identity, &req.method, req.params,
                                    )
                                    .await
                                    {
                                        Ok(ok) => {
                                            if developer_logging {
                                                log::debug!(
                                                    "[ext:{}] call {} → ok ({} ms)",
                                                    identity.id,
                                                    method.as_deref().unwrap_or_default(),
                                                    started.map(|time| time.elapsed().as_millis()).unwrap_or(0),
                                                );
                                            }
                                            grain_sdk::ServerResponse {
                                                id: req.id, ok: Some(ok), err: None,
                                            }
                                        }
                                        Err(e) => {
                                            if developer_logging {
                                                if let Some(capability) = denied_capability {
                                                    log::error!(
                                                        "[ext:{}] denied {} — missing capability '{}'; add \"permissions\": [\"{}\"] to manifest.json",
                                                        identity.id,
                                                        method.as_deref().unwrap_or_default(),
                                                        capability,
                                                        capability,
                                                    );
                                                }
                                                log::debug!(
                                                    "[ext:{}] call {} → error ({} ms)",
                                                    identity.id,
                                                    method.as_deref().unwrap_or_default(),
                                                    started.map(|time| time.elapsed().as_millis()).unwrap_or(0),
                                                );
                                            }
                                            grain_sdk::ServerResponse {
                                                id: req.id, ok: None, err: Some(e),
                                            }
                                        }
                                    };
                                    if let Ok(json) =
                                        serde_json::to_string(&grain_sdk::HostFrame::Response(resp))
                                    {
                                        let _ = out_tx.send(Message::Text(json.into()));
                                    }
                                });
                            }
                            Ok(grain_sdk::HostFrame::CallResult(r)) => {
                                let result = match r.err {
                                    Some(e) => Err(e),
                                    None => Ok(r.ok.unwrap_or(serde_json::Value::Null)),
                                };
                                crate::extension_host::resolve_call_result(
                                    &identity.id, r.call_id, result,
                                );
                            }
                            // Response/Call are server→worker; a worker echoing
                            // them (or any non-HostFrame) is ignored.
                            _ => {}
                        }
                    }
                }
                Some(Ok(_)) => {}
                _ => break,
            },
        }
    }
    if is_worker {
        crate::extension_host::detach_connection(&identity.id, &session.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn event_listener_is_exclusive_and_rebinds_after_release() {
        let first = bind_events_listener("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = first.local_addr().unwrap();

        assert!(bind_events_listener(addr).is_err());
        drop(first);
        bind_events_listener(addr).expect("the released port should be reusable immediately");
    }

    #[test]
    fn origin_allowlist_covers_native_and_grain_clients() {
        let allowed = [
            None,
            Some("tauri://localhost"),
            Some("tauri://grain"),
            Some("http://tauri.localhost"),
            Some("https://tauri.localhost"),
            Some("http://localhost:1420"),
            Some("https://127.0.0.1:1420"),
            Some("http://[::1]:1420"),
        ];

        for origin in allowed {
            assert!(origin_allowed(origin), "expected {origin:?} to be allowed");
        }
    }

    #[test]
    fn origin_allowlist_rejects_web_and_malformed_origins() {
        let rejected = [
            "null",
            "https://evil.example",
            "https://localhost.evil.example",
            "ws://localhost:7124",
            "file://localhost",
            "tauri:",
            "not an origin",
        ];

        for origin in rejected {
            assert!(
                !origin_allowed(Some(origin)),
                "expected {origin:?} to be rejected"
            );
        }
    }

    #[test]
    fn pre_auth_guard_caps_and_releases_slots() {
        let count = AtomicUsize::new(0);
        let first = PreAuthGuard::acquire(&count, 2).expect("first slot");
        let mut second = PreAuthGuard::acquire(&count, 2).expect("second slot");
        assert!(PreAuthGuard::acquire(&count, 2).is_none());
        assert_eq!(count.load(Ordering::Acquire), 2);

        drop(first);
        assert_eq!(count.load(Ordering::Acquire), 1);
        let third = PreAuthGuard::acquire(&count, 2).expect("released slot");

        second.release();
        second.release();
        assert_eq!(count.load(Ordering::Acquire), 1);
        drop(third);
        assert_eq!(count.load(Ordering::Acquire), 0);
    }
}
