//! Authenticated loopback requests/results for native workers and developer control.
//! Recording and Agent UI events stay on the internal WebView bridge; public
//! socket roles never subscribe to the core event bus.

use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(test)]
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tauri::AppHandle;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    accept_hdr_async_with_config,
    tungstenite::{
        handshake::server::{ErrorResponse, Request, Response},
        http::{header::ORIGIN, StatusCode, Uri},
        protocol::WebSocketConfig,
        Message,
    },
};

/// Fixed loopback port for extension worker/developer-control traffic.
#[cfg(not(feature = "agent-harness"))]
pub const EVENTS_PORT: u16 = 7124;
#[cfg(feature = "agent-harness")]
pub const EVENTS_PORT: u16 = 17124;
static UNAUTHENTICATED_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
const MAX_UNAUTHENTICATED_CONNECTIONS: usize = 64;

// Applies before authentication to every client on this shared listener.
// Bound fragmented messages as well as individual frames before JSON decoding.
const MAX_INBOUND_WS_BYTES: usize = 512 * 1024;
const MAX_WORKER_RPC_TASKS: usize = 4;
const SOCKET_WRITE_DEADLINE: Duration = Duration::from_secs(2);

// A slow peer must not trap the socket owner inside the writer arm. Cancelling
// a partially written frame closes this connection; it is never replayed.
async fn socket_write<F, E>(
    send: F,
    closed: Option<&mut tokio::sync::watch::Receiver<bool>>,
    deadline: Duration,
) -> bool
where
    F: std::future::Future<Output = Result<(), E>>,
{
    tokio::select! {
        biased;
        _ = async {
            match closed {
                Some(closed) => { let _ = closed.wait_for(|value| *value).await; }
                None => std::future::pending::<()>().await,
            }
        } => false,
        result = tokio::time::timeout(deadline, send) => matches!(result, Ok(Ok(()))),
    }
}

fn spawn_worker_rpc<F>(requests: &mut tokio::task::JoinSet<()>, request: F) -> bool
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    while requests.try_join_next().is_some() {}
    if requests.len() >= MAX_WORKER_RPC_TASKS {
        return false;
    }
    requests.spawn(request);
    true
}

// Stop serialization before retaining more than one permitted wire frame.
// Parsed host-API results have separate per-API limits; this bounds encoding.
fn worker_response_json(response: &grain_sdk::HostFrame) -> Result<String, serde_json::Error> {
    struct Limited(Vec<u8>);
    impl std::io::Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_INBOUND_WS_BYTES.saturating_sub(self.0.len()) {
                return Err(std::io::Error::other("worker response exceeds wire budget"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Limited(Vec::new());
    serde_json::to_writer(&mut buffer, response)?;
    Ok(String::from_utf8(buffer.0).expect("JSON serialization is UTF-8"))
}

fn events_websocket_config() -> WebSocketConfig {
    WebSocketConfig {
        max_message_size: Some(MAX_INBOUND_WS_BYTES),
        max_frame_size: Some(MAX_INBOUND_WS_BYTES),
        ..WebSocketConfig::default()
    }
}

/// [GRAIN] SPEC §7.1: the server-side token → identity table. Minted per app
/// run; each worker's token is injected into its environment at spawn. A
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
/// must reject browser handshakes from arbitrary sites. Native worker clients
/// omit Origin and remain valid.
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
    mut caps: std::collections::HashSet<String>,
    role: crate::events_auth::ClientRole,
) -> String {
    caps.retain(|cap| grain_sdk::manifest::tool_permission_allowed(cap));
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
        url: &format!("ws://127.0.0.1:{EVENTS_PORT}"),
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

/// Spawn the authenticated worker/developer request server.
pub fn start(app: AppHandle) {
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
            // worker/developer transport for the new app run. Retry at low
            // frequency until the old listener releases the port.
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
                        let app = app.clone();
                        tokio::spawn(handle(stream, app));
                    }
                    Err(e) => log::warn!("[GRAIN] events WS accept error: {e}"),
                }
            }
        });
    });
}

async fn handle(stream: TcpStream, app: AppHandle) {
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

    let ws = match accept_hdr_async_with_config(
        stream,
        |request: &Request, response: Response| {
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
        },
        Some(events_websocket_config()),
    )
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
        if !socket_write(
            write.send(Message::Text(json.into())),
            None,
            SOCKET_WRITE_DEADLINE,
        )
        .await
        {
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
            if !socket_write(
                write.send(Message::Text(json.into())),
                None,
                SOCKET_WRITE_DEADLINE,
            )
            .await
            {
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
    let (out_tx, mut out_rx) =
        tokio::sync::mpsc::channel::<Message>(crate::extension_host::WORKER_OUTBOUND_CAPACITY);

    // The authenticated role, not its capability set, decides which protocol
    // this socket speaks and whether the worker host tracks it for reaping.
    let is_worker = identity.role == crate::events_auth::ClientRole::Worker;
    let mut worker_closed = None;
    if is_worker {
        worker_closed =
            crate::extension_host::attach_connection(&identity.id, &session.token, out_tx.clone());
        if worker_closed.is_none() {
            return;
        }
    }

    // Workers and developer controls have only explicit request/result traffic.
    // The recording WebView is internal and no longer subscribes through this socket.
    // Own and bound host-API work to this socket. Drop aborts unfinished tasks.
    let mut requests = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = async { let _ = worker_closed.as_mut().expect("worker close signal").wait_for(|closed| *closed).await; }, if is_worker => break,
            _ = requests.join_next(), if !requests.is_empty() => {},
            // The single writer: everything bound for this socket passes here.
            outgoing = out_rx.recv() => match outgoing {
                Some(m) => {
                    if !socket_write(write.send(m), worker_closed.as_mut(), SOCKET_WRITE_DEADLINE).await {
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
                    if is_worker {
                        match serde_json::from_str::<grain_sdk::HostFrame>(&txt) {
                            Ok(grain_sdk::HostFrame::Request(req)) => {
                                // Capability-checked host API. Dispatch off the
                                // read loop; the reply returns via the writer arm.
                                let app = app.clone();
                                let identity = identity.clone();
                                let out_tx = out_tx.clone();
                                let worker_token = session.token.clone();
                                if !spawn_worker_rpc(&mut requests, async move {
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
                                        crate::extension_host::rpc_execution_generation(&identity.id, &worker_token),
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
                                    let sent = worker_response_json(&grain_sdk::HostFrame::Response(resp))
                                        .is_ok_and(|json| out_tx.try_send(Message::Text(json.into())).is_ok());
                                    if !sent {
                                        crate::extension_host::detach_connection(&identity.id, &worker_token);
                                    }
                                }) { break; }
                            }
                            Ok(grain_sdk::HostFrame::CallResult(r)) => {
                                let result = match r.err {
                                    Some(e) => Err(e),
                                    None => Ok(r.ok.unwrap_or(serde_json::Value::Null)),
                                };
                                crate::extension_host::resolve_call_result(
                                    &identity.id, &session.token, r.call_id, result,
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
    requests.abort_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn socket_write_deadline_and_close_signal_drop_stalled_writer() {
        struct Dropped(Arc<AtomicUsize>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        for close in [false, true] {
            let dropped = Arc::new(AtomicUsize::new(0));
            let guard = Dropped(dropped.clone());
            let (tx, mut rx) = tokio::sync::watch::channel(false);
            let write = async move {
                let _guard = guard;
                std::future::pending::<Result<(), ()>>().await
            };
            let shutdown = async {
                if close {
                    tokio::task::yield_now().await;
                    tx.send_replace(true);
                }
            };
            let (sent, _) = tokio::join!(
                socket_write(write, Some(&mut rx), Duration::from_millis(20)),
                shutdown
            );
            assert!(!sent);
            assert_eq!(dropped.load(Ordering::SeqCst), 1);
        }
        assert!(socket_write(async { Ok::<_, ()>(()) }, None, Duration::from_secs(1)).await);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn worker_rpc_tasks_are_bounded_and_socket_drop_releases_owners() {
        struct Owner(Arc<AtomicUsize>);
        impl Drop for Owner {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let started = Arc::new(AtomicUsize::new(0));
        let released = Arc::new(AtomicUsize::new(0));
        let mut requests = tokio::task::JoinSet::new();
        for index in 0..=MAX_WORKER_RPC_TASKS {
            let started = started.clone();
            let released = released.clone();
            assert_eq!(
                spawn_worker_rpc(&mut requests, async move {
                    started.fetch_add(1, Ordering::SeqCst);
                    let _owner = Owner(released);
                    std::future::pending::<()>().await;
                }),
                index < MAX_WORKER_RPC_TASKS
            );
        }
        tokio::task::yield_now().await;
        assert_eq!(started.load(Ordering::SeqCst), MAX_WORKER_RPC_TASKS);
        drop(requests);
        tokio::time::timeout(Duration::from_secs(1), async {
            while released.load(Ordering::SeqCst) != MAX_WORKER_RPC_TASKS {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn worker_rpc_encoding_stops_at_wire_budget_and_preserves_small_replies() {
        for size in [100, MAX_INBOUND_WS_BYTES, MAX_INBOUND_WS_BYTES * 2] {
            let frame = grain_sdk::HostFrame::Response(grain_sdk::ServerResponse {
                id: 1,
                ok: Some(serde_json::json!({"body": "x".repeat(size)})),
                err: None,
            });
            let result = worker_response_json(&frame);
            if size == 100 {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&result.unwrap()).unwrap(),
                    serde_json::to_value(&frame).unwrap()
                );
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn socket_limits_reject_large_frames_and_fragmented_messages() {
        use tokio_tungstenite::tungstenite::protocol::frame::{
            coding::{Data, OpCode},
            Frame,
        };
        for fragmented in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async_with_config(
                    socket,
                    Some(events_websocket_config()),
                )
                .await
                .unwrap();
                assert_eq!(
                    ws.next().await.unwrap().unwrap(),
                    Message::Text("normal".into())
                );
                let result = tokio::time::timeout(Duration::from_secs(2), ws.next())
                    .await
                    .unwrap()
                    .unwrap();
                assert!(matches!(
                    result,
                    Err(tokio_tungstenite::tungstenite::Error::Capacity(_))
                ));
            });
            let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
                .await
                .unwrap();
            client.send(Message::Text("normal".into())).await.unwrap();
            if fragmented {
                let half = MAX_INBOUND_WS_BYTES / 2 + 1;
                client
                    .send(Message::Frame(Frame::message(
                        vec![b'x'; half],
                        OpCode::Data(Data::Text),
                        false,
                    )))
                    .await
                    .unwrap();
                // Each fragment fits; their aggregate must still be rejected.
                let _ = client
                    .send(Message::Frame(Frame::message(
                        vec![b'x'; half],
                        OpCode::Data(Data::Continue),
                        true,
                    )))
                    .await;
            } else {
                let _ = client
                    .send(Message::Text("x".repeat(MAX_INBOUND_WS_BYTES + 1)))
                    .await;
            }
            server.await.unwrap();
        }
    }

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
