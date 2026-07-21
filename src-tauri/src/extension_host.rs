//! [GRAIN] The extension host (SPEC §3.1, §7.1) — Phase 2, the scripted runtime.
//!
//! Owns the lifecycle of tier-B (scripted) extension workers. One hidden
//! supervisor webview runs Grain's own code and spawns one Web Worker per
//! extension; each worker opens its **own** WebSocket with its **own** token
//! (SPEC §7.1 — never a shared realm, which would make identity forgeable).
//!
//! Responsibilities:
//! - **Activation dispatch**: subscribe to the event bus; wake a worker when a
//!   broadcast matches its manifest `activation` (`onEvent:<Variant>`,
//!   `onTransform` → warm on `RecordingStarted`), carrying the triggering event
//!   as the injected `activation` payload (the broadcast is already past when
//!   the worker connects, so the wake reason must travel with the spawn).
//! - **Worker registry**: per-extension token, strikes, last-activity, and the
//!   connection channel (set when its WS attaches in `events_server`).
//! - **Host calls**: [`call_worker`] issues a `HostCall` and awaits the worker's
//!   `HostCallResult` under a deadline; [`run_transforms`] is the transform
//!   pipeline built on it (150 ms hard deadline, 3-strike auto-disable).
//! - **Reaper**: idle (> 120 s, no pending calls, not resident) workers are
//!   killed and their tokens revoked — "destroy if not in use".
//!
//! The security wall is the Rust WS boundary ([`crate::events_auth`] +
//! [`crate::host_api`]); this module is the *lifecycle*, not the enforcement.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use grain_core::{AppContext, DaemonEvent};
use grain_sdk::{GrainPack, HostCall, HostFrame};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

/// The hidden supervisor webview: Grain's code, one per app run, created on the
/// first worker need and torn down when the last worker dies.
const SUPERVISOR_LABEL: &str = "extension-host";
/// A dedicated frontend route (Step 5) — NOT the SPA root, so no extension code
/// ever shares Grain's main global.
const SUPERVISOR_URL: &str = "extension-host.html";

/// Reaper policy (SPEC §3: workers are ephemeral).
const IDLE_REAP_SECS: u64 = 120;
const REAP_INTERVAL_SECS: u64 = 30;

/// Transform budget (SPEC §3.1): a cold worker cannot fit this, which is why
/// `onTransform` extensions warm on `RecordingStarted`.
const TRANSFORM_DEADLINE: Duration = Duration::from_millis(150);
/// Consecutive transform failures before auto-disable (SPEC §3.3).
const MAX_STRIKES: u32 = 3;

/// One extension worker's live connection channel. Populated by
/// [`attach_connection`] when the worker's WS authenticates in `events_server`.
struct WorkerConn {
    /// The single-writer funnel to this worker's socket (owned by its `handle`
    /// task; every frame the host sends goes through here).
    out_tx: mpsc::UnboundedSender<Message>,
    /// In-flight host calls: `call_id` → the awaiter's oneshot. `Arc` so
    /// [`call_worker`]/[`resolve_call_result`] can operate on it without holding
    /// the `workers` lock across an await.
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>,
    next_call_id: Arc<AtomicU64>,
}

struct Worker {
    /// The token minted for this worker; revoked at reap (SPEC §7.1).
    token: String,
    /// `onStartup` workers are never reaped for idleness.
    resident: bool,
    /// Consecutive transform-deadline failures (SPEC §3.3).
    strikes: u32,
    /// Epoch seconds of the last frame from this worker; the reaper's clock.
    /// `Arc` so the connection can bump it directly (no lock per frame).
    last_activity: Arc<AtomicU64>,
    conn: Option<WorkerConn>,
}

/// The worker registry: the map plus every operation over it. Deliberately free
/// of any `AppHandle`/Tauri coupling, so the async call/resolve correlation, the
/// strike accounting, and the reaper's victim selection are unit-tested directly
/// (the Tauri-side spawn/kill/emit stay as free functions above it).
struct Workers {
    map: Mutex<HashMap<String, Worker>>,
}

impl Workers {
    fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    fn is_running(&self, ext_id: &str) -> bool {
        self.map.lock().unwrap().contains_key(ext_id)
    }

    fn is_empty(&self) -> bool {
        self.map.lock().unwrap().is_empty()
    }

    fn insert(&self, ext_id: &str, worker: Worker) {
        self.map.lock().unwrap().insert(ext_id.to_string(), worker);
    }

    fn remove(&self, ext_id: &str) -> Option<Worker> {
        self.map.lock().unwrap().remove(ext_id)
    }

    /// Register (or refresh) a worker's connection and return the shared
    /// last-activity clock for the caller to bump on each inbound frame.
    fn attach(&self, ext_id: &str, out_tx: mpsc::UnboundedSender<Message>) -> Arc<AtomicU64> {
        let mut map = self.map.lock().unwrap();
        let w = map.entry(ext_id.to_string()).or_insert_with(|| Worker {
            token: String::new(),
            resident: false,
            strikes: 0,
            last_activity: Arc::new(AtomicU64::new(now_secs())),
            conn: None,
        });
        w.last_activity.store(now_secs(), Ordering::Relaxed);
        w.conn = Some(WorkerConn {
            out_tx,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_call_id: Arc::new(AtomicU64::new(1)),
        });
        w.last_activity.clone()
    }

    /// Issue a `HostCall` to a connected worker and await its answer under
    /// `deadline`. Never holds the registry lock across the await.
    async fn call(
        &self,
        ext_id: &str,
        method: &str,
        params: Value,
        deadline: Duration,
    ) -> Result<Value, String> {
        let (out_tx, pending, next_call_id) = {
            let map = self.map.lock().unwrap();
            let conn = map
                .get(ext_id)
                .and_then(|w| w.conn.as_ref())
                .ok_or("worker not connected")?;
            (
                conn.out_tx.clone(),
                conn.pending.clone(),
                conn.next_call_id.clone(),
            )
        };
        let call_id = next_call_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        pending.lock().unwrap().insert(call_id, tx);
        let frame = HostFrame::Call(HostCall {
            call_id,
            method: method.to_string(),
            params,
        });
        let json = serde_json::to_string(&frame).map_err(|e| e.to_string())?;
        if out_tx.send(Message::Text(json.into())).is_err() {
            pending.lock().unwrap().remove(&call_id);
            return Err("worker channel closed".into());
        }
        match tokio::time::timeout(deadline, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err("worker dropped the call".into()),
            Err(_) => {
                pending.lock().unwrap().remove(&call_id);
                Err("deadline exceeded".into())
            }
        }
    }

    /// Route a `HostCallResult` back to the awaiter of its `call_id` (no-op if
    /// the worker/call is unknown or already timed out).
    fn resolve(&self, ext_id: &str, call_id: u64, result: Result<Value, String>) {
        let pending = self
            .map
            .lock()
            .unwrap()
            .get(ext_id)
            .and_then(|w| w.conn.as_ref())
            .map(|c| c.pending.clone());
        if let Some(pending) = pending {
            if let Some(tx) = pending.lock().unwrap().remove(&call_id) {
                let _ = tx.send(result);
            }
        }
    }

    /// Ids of workers idle longer than `idle_secs` with no pending calls and not
    /// resident — the reaper's kill list.
    fn idle_victims(&self, now: u64, idle_secs: u64) -> Vec<String> {
        self.map
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, w)| {
                if w.resident {
                    return None;
                }
                let idle = now.saturating_sub(w.last_activity.load(Ordering::Relaxed));
                let busy = w
                    .conn
                    .as_ref()
                    .map(|c| !c.pending.lock().unwrap().is_empty())
                    .unwrap_or(false);
                (idle > idle_secs && !busy).then(|| id.clone())
            })
            .collect()
    }

    fn clear_strikes(&self, ext_id: &str) {
        if let Some(w) = self.map.lock().unwrap().get_mut(ext_id) {
            w.strikes = 0;
        }
    }

    /// Record a transform failure; returns true once the worker hits the strike
    /// limit (caller then auto-disables).
    fn record_strike(&self, ext_id: &str, limit: u32) -> bool {
        match self.map.lock().unwrap().get_mut(ext_id) {
            Some(w) => {
                w.strikes += 1;
                w.strikes >= limit
            }
            None => false,
        }
    }
}

/// Supervisor readiness gate: Tauri events are not buffered, so a `spawn` emit
/// before the page's `listen` is registered would be lost. Spawns issued before
/// the page reports `ext-host://ready` are queued and flushed on ready.
struct Supervisor {
    exists: bool,
    ready: bool,
    queue: Vec<SpawnPayload>,
}

/// The hot-path index. **An installed-but-idle extension must cost the native
/// pipeline nothing**, so the paste path and the event bus consult this — never
/// the registry (which clones every record) and never the disk (which is where
/// manifests live). Rebuilt only when the extension set changes.
#[derive(Default)]
struct Index {
    /// Event variant name → extension ids that wake on it.
    by_event: HashMap<String, Vec<String>>,
    /// `onTransform` extensions, already sorted into toggle order.
    transforms: Vec<String>,
}

/// Guards so the common case — no scripted extension enabled — costs exactly
/// one relaxed atomic load on the paste path and per broadcast event. These are
/// free-standing (not behind `HOST`) so the check needs no lock and no
/// `OnceLock` deref.
static HAS_ACTIVATIONS: AtomicBool = AtomicBool::new(false);
static HAS_TRANSFORMS: AtomicBool = AtomicBool::new(false);

struct HostState {
    app: AppHandle,
    workers: Workers,
    supervisor: Mutex<Supervisor>,
    index: RwLock<Index>,
}

static HOST: OnceLock<HostState> = OnceLock::new();

/// Rebuild the hot-path index from the registry + on-disk manifests. Called on
/// startup and whenever the extension set changes (enable/disable/grant/import/
/// uninstall/auto-disable) — **never** from a hot path.
pub fn refresh_index(app: &AppHandle) {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    let mut by_event: HashMap<String, Vec<String>> = HashMap::new();
    let mut transforms: Vec<(String, u64)> = Vec::new();

    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        for rec in reg.records() {
            if !rec.enabled {
                continue;
            }
            let pack = match load_manifest(app, &rec.id) {
                Some(p) if p.is_scripted() => p,
                _ => continue,
            };
            for variant in activation_variants(&pack.manifest.activation) {
                by_event.entry(variant).or_default().push(rec.id.clone());
            }
            if declares_transform(&pack.manifest.activation) {
                transforms.push((rec.id.clone(), rec.toggle_seq));
            }
        }
    }
    transforms.sort_by_key(|(_, seq)| *seq);
    let transforms: Vec<String> = transforms.into_iter().map(|(id, _)| id).collect();

    HAS_ACTIVATIONS.store(!by_event.is_empty(), Ordering::Relaxed);
    HAS_TRANSFORMS.store(!transforms.is_empty(), Ordering::Relaxed);
    log::debug!(
        "[GRAIN] ext-host: index rebuilt — {} activation variant(s), {} transform(s)",
        by_event.len(),
        transforms.len()
    );
    *host.index.write().unwrap() = Index {
        by_event,
        transforms,
    };
}

/// Supervisor → worker: create a Web Worker for this extension.
#[derive(Clone, Serialize)]
struct SpawnPayload {
    ext_id: String,
    /// The worker's own WS token (SPEC §7.1) — its sole credential.
    token: String,
    /// The extension's JS, embedded in its pack (guide Step 4).
    entry_source: String,
    /// The granted capability names, injected so the shim can expose only the
    /// matching `grain.*` surface (the wall is still Rust-side).
    caps: Vec<String>,
    /// The event that woke this worker (`{"Variant": {...}}`), or absent for a
    /// non-event spawn.
    #[serde(skip_serializing_if = "Option::is_none")]
    activation: Option<Value>,
}

#[derive(Clone, Serialize)]
struct KillPayload {
    ext_id: String,
}

#[derive(Deserialize)]
struct DiedPayload {
    ext_id: String,
    #[serde(default)]
    reason: String,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Start the host: register the supervisor↔host event bridge, the activation
/// dispatcher, and the reaper. Idempotent (a second call is a no-op).
pub fn start(app: AppHandle, ctx: Arc<AppContext>) {
    if HOST
        .set(HostState {
            app: app.clone(),
            workers: Workers::new(),
            supervisor: Mutex::new(Supervisor {
                exists: false,
                ready: false,
                queue: Vec::new(),
            }),
            index: RwLock::new(Index::default()),
        })
        .is_err()
    {
        return; // already started
    }

    // Build the hot-path index once up front; the guards stay false (and both
    // hot paths stay free) until something is actually enabled.
    refresh_index(&app);

    // Supervisor → host: a worker crashed or reported a fatal error.
    app.listen("ext-host://died", move |ev| {
        if let Ok(p) = serde_json::from_str::<DiedPayload>(ev.payload()) {
            log::warn!("[GRAIN] ext-host: worker '{}' died: {}", p.ext_id, p.reason);
            kill_worker(&p.ext_id, "worker reported death");
        }
    });

    // Supervisor → host: the page loaded and its listeners are live → flush any
    // spawns queued while it was starting.
    app.listen("ext-host://ready", move |_| on_supervisor_ready());

    // Activation dispatch: wake workers on their declared events. A dedicated
    // subscriber so the wake path is independent of any connected worker.
    let app_act = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut rx = ctx.subscribe();
        loop {
            match rx.recv().await {
                Ok(ev) => on_event(&app_act, &ev),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Reaper: return RAM when a worker goes idle.
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(REAP_INTERVAL_SECS));
        loop {
            tick.tick().await;
            reap_idle();
        }
    });
}

// ── Activation ──────────────────────────────────────────────────────────────

/// The event variants an activation list wakes on. `onTransform` warms at
/// session start (SPEC §3.1: a ~300 ms cold wake cannot fit the 150 ms
/// transform budget, so the worker must already be up when the transform runs).
fn activation_variants(activation: &[String]) -> Vec<String> {
    let mut out: Vec<String> = activation
        .iter()
        .filter_map(|a| {
            if let Some(v) = a.strip_prefix("onEvent:") {
                Some(v.to_string())
            } else if a == "onTransform" {
                Some("RecordingStarted".to_string())
            } else {
                None // onStartup / onShortcut are not event-driven
            }
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn declares_transform(activation: &[String]) -> bool {
    activation.iter().any(|a| a == "onTransform")
}

fn on_event(app: &AppHandle, ev: &DaemonEvent) {
    // Zero-cost when no extension declares an event activation: one relaxed
    // load. This runs for EVERY broadcast, including AudioLevel while
    // recording, so nothing above this line may allocate or touch the disk.
    if !HAS_ACTIVATIONS.load(Ordering::Relaxed) {
        return;
    }
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    // Allocation-free tag read, then an index lookup — no registry, no disk.
    let waking: Vec<String> = {
        let index = host.index.read().unwrap();
        match index.by_event.get(ev.variant_name()) {
            Some(ids) => ids.clone(),
            None => return,
        }
    };
    let cold: Vec<String> = waking.into_iter().filter(|id| !is_running(id)).collect();
    if cold.is_empty() {
        return; // already awake — it gets this event over its own connection
    }
    // Only now, on the rare spawn path, is it worth serializing the event to
    // carry as the activation payload.
    let activation = serde_json::to_value(ev).ok();
    let reg = match app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        Some(r) => r,
        None => return,
    };
    for id in cold {
        let pack = match load_manifest(app, &id) {
            Some(p) => p,
            None => continue,
        };
        let granted = reg.record(&id).map(|r| r.granted).unwrap_or_default();
        spawn_worker(app, &id, &pack, granted, activation.clone());
    }
}

// ── Worker lifecycle ────────────────────────────────────────────────────────

fn is_running(ext_id: &str) -> bool {
    HOST.get()
        .map(|h| h.workers.is_running(ext_id))
        .unwrap_or(false)
}

fn spawn_worker(
    app: &AppHandle,
    ext_id: &str,
    pack: &GrainPack,
    caps: Vec<String>,
    activation: Option<Value>,
) {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    // Mint a per-worker token bound to exactly the granted caps (SPEC §7.1): the
    // same server-side filter that gates the pill now gates this worker.
    let token = crate::events_server::mint_extension_token(ext_id, caps.iter().cloned().collect());
    let resident = pack.manifest.activation.iter().any(|a| a == "onStartup");
    host.workers.insert(
        ext_id,
        Worker {
            token: token.clone(),
            resident,
            strikes: 0,
            last_activity: Arc::new(AtomicU64::new(now_secs())),
            conn: None,
        },
    );
    let payload = SpawnPayload {
        ext_id: ext_id.to_string(),
        token,
        entry_source: pack.manifest.entry_source.clone(),
        caps,
        activation,
    };
    log::info!("[GRAIN] ext-host: spawning '{ext_id}' (resident={resident})");
    ensure_supervisor(app);
    let mut sup = host.supervisor.lock().unwrap();
    if sup.ready {
        let _ = app.emit_to(SUPERVISOR_LABEL, "ext-host://spawn", payload);
    } else {
        sup.queue.push(payload); // flushed by on_supervisor_ready
    }
}

/// Create the supervisor webview if it isn't up yet. Window creation is posted
/// to the main thread (tauri#3990: never build a window on a shortcut/event
/// thread) and returns immediately.
fn ensure_supervisor(app: &AppHandle) {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    {
        let mut sup = host.supervisor.lock().unwrap();
        if sup.exists {
            return;
        }
        sup.exists = true;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if app2.get_webview_window(SUPERVISOR_LABEL).is_some() {
            return;
        }
        let mut builder = WebviewWindowBuilder::new(
            &app2,
            SUPERVISOR_LABEL,
            WebviewUrl::App(SUPERVISOR_URL.into()),
        )
        .title("Grain Extension Host")
        .inner_size(1.0, 1.0)
        .visible(false)
        .skip_taskbar(true);
        if let Some(data_dir) = crate::portable::data_dir() {
            builder = builder.data_directory(data_dir.join("webview"));
        }
        if let Err(e) = builder.build() {
            log::error!("[GRAIN] ext-host: supervisor window failed: {e}");
            if let Some(h) = HOST.get() {
                h.supervisor.lock().unwrap().exists = false; // allow a retry
            }
        }
    });
}

fn on_supervisor_ready() {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    let (app, queued) = {
        let mut sup = host.supervisor.lock().unwrap();
        sup.ready = true;
        (host.app.clone(), std::mem::take(&mut sup.queue))
    };
    for payload in queued {
        let _ = app.emit_to(SUPERVISOR_LABEL, "ext-host://spawn", payload);
    }
}

/// Terminate a worker: drop its registry entry, revoke its token, tell the
/// supervisor to kill the Web Worker, and fail any in-flight host calls so their
/// awaiters don't hang. Tears down the supervisor when the last worker dies.
fn kill_worker(ext_id: &str, reason: &str) {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    let worker = match host.workers.remove(ext_id) {
        Some(w) => w,
        None => return,
    };
    let empty = host.workers.is_empty();
    log::info!("[GRAIN] ext-host: reaping '{ext_id}' ({reason})");
    crate::events_server::revoke_token(&worker.token);
    let _ = host.app.emit_to(
        SUPERVISOR_LABEL,
        "ext-host://kill",
        KillPayload {
            ext_id: ext_id.to_string(),
        },
    );
    if let Some(conn) = &worker.conn {
        for (_, tx) in conn.pending.lock().unwrap().drain() {
            let _ = tx.send(Err("worker terminated".into()));
        }
    }
    if empty {
        teardown_supervisor();
    }
}

fn teardown_supervisor() {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    {
        let mut sup = host.supervisor.lock().unwrap();
        if !sup.exists {
            return;
        }
        sup.exists = false;
        sup.ready = false;
        sup.queue.clear();
    }
    let app = host.app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(w) = app.get_webview_window(SUPERVISOR_LABEL) {
            let _ = w.close();
        }
    });
}

fn reap_idle() {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    for id in host.workers.idle_victims(now_secs(), IDLE_REAP_SECS) {
        kill_worker(&id, "idle timeout");
    }
}

// ── Connection surface (called from events_server on the worker's WS) ────────

/// A worker's WS authenticated: register its outbound channel and return the
/// shared last-activity clock for the connection to bump on each frame. Upserts,
/// so a worker that connects without a prior spawn record still gets tracked.
pub fn attach_connection(ext_id: &str, out_tx: mpsc::UnboundedSender<Message>) -> Arc<AtomicU64> {
    match HOST.get() {
        Some(h) => h.workers.attach(ext_id, out_tx),
        None => Arc::new(AtomicU64::new(now_secs())),
    }
}

/// The worker's WS closed → the worker process is gone; reap it.
pub fn detach_connection(ext_id: &str) {
    kill_worker(ext_id, "connection closed");
}

/// Route a `HostCallResult` back to its awaiter (the transform/session caller).
pub fn resolve_call_result(ext_id: &str, call_id: u64, result: Result<Value, String>) {
    if let Some(host) = HOST.get() {
        host.workers.resolve(ext_id, call_id, result);
    }
}

// ── Host-initiated calls + the transform pipeline ────────────────────────────

/// The transform pipeline (SPEC §3.1, §3.3). Runs every enabled `onTransform`
/// extension in **toggle order**, each under a hard 150 ms deadline. A worker
/// that is cold, slow, or errors leaves the text unchanged and takes a strike
/// (3 → auto-disable). An empty-string reply suppresses the paste (the
/// documented output-suppression behavior). Never blocks the paste path on a
/// cold spawn.
pub async fn run_transforms(app: &AppHandle, text: String) -> String {
    // THE paste path. With no transform extension enabled this is one relaxed
    // atomic load and a return — no registry clone, no disk, no allocation.
    // Dictation must be exactly as fast as it was before the platform existed.
    if !HAS_TRANSFORMS.load(Ordering::Relaxed) {
        return text;
    }
    let host = match HOST.get() {
        Some(h) => h,
        None => return text,
    };
    // Pre-sorted into toggle order at index-build time.
    let transforms: Vec<String> = host.index.read().unwrap().transforms.clone();
    if transforms.is_empty() {
        return text;
    }

    let mut current = text;
    for id in transforms {
        // A cold worker cannot fit the budget; skip rather than block the paste.
        if !is_running(&id) {
            continue;
        }
        match host
            .workers
            .call(&id, "transform", json!({ "text": current }), TRANSFORM_DEADLINE)
            .await
        {
            Ok(v) => {
                // Accept either `{ "text": "…" }` or a bare string.
                if let Some(s) = v.get("text").and_then(|t| t.as_str()) {
                    current = s.to_string();
                } else if let Some(s) = v.as_str() {
                    current = s.to_string();
                }
                clear_strikes(&id);
            }
            Err(e) => {
                log::warn!("[GRAIN] transform '{id}' failed ({e}) — text unchanged");
                record_strike(app, &id);
            }
        }
    }
    current
}

fn clear_strikes(ext_id: &str) {
    if let Some(host) = HOST.get() {
        host.workers.clear_strikes(ext_id);
    }
}

fn record_strike(app: &AppHandle, ext_id: &str) {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    if host.workers.record_strike(ext_id, MAX_STRIKES) {
        auto_disable(app, ext_id);
    }
}

/// SPEC §3.3: a persistently failing transform is disabled (not left slowing
/// every paste). The user re-enables explicitly from Overview.
fn auto_disable(app: &AppHandle, ext_id: &str) {
    log::warn!("[GRAIN] ext-host: auto-disabling '{ext_id}' after {MAX_STRIKES} transform strikes");
    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        let _ = reg.set_enabled(ext_id, false);
    }
    refresh_index(app); // drop it from the hot-path index immediately
    kill_worker(ext_id, "auto-disabled (3 transform strikes)");
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        ctx.emit(DaemonEvent::ExtensionDisabled {
            id: ext_id.to_string(),
            reason: "It repeatedly missed the 150 ms transform deadline.".to_string(),
        });
    }
}

/// Load a scripted extension's pack (manifest + embedded `entry_source`) from
/// its on-disk `.grainpack.json` (SPEC §5.1 storage; same path as tier-A packs).
fn load_manifest(app: &AppHandle, id: &str) -> Option<GrainPack> {
    let ctx = app.try_state::<Arc<AppContext>>()?;
    let path = ctx
        .data_dir
        .join("extensions")
        .join(format!("{id}.grainpack.json"));
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

// ── Built-in scripted packs (the dogfood) ────────────────────────────────────

/// The auto-categorization built-in (guide Step 8) — the first scripted
/// built-in, proving the runtime end to end: worker spawn on an event, the
/// injected activation payload, `llm` + `storage` host calls, capability
/// enforcement, and the idle reaper.
const AUTO_CATEGORIZE_ID: &str = "grain.auto-categorize";

/// Its worker source. On each finalized transcript it asks the user's active LLM
/// for a one-word category and appends the note to that category's list in the
/// extension's OWN storage (no path to app data). Authored as a raw string so
/// the JS reads naturally.
const AUTO_CATEGORIZE_ENTRY: &str = r#"grain.onEvent(async function (ev) {
  var payload = ev && ev.TranscriptionComplete;
  if (!payload || !payload.text) return;
  var text = String(payload.text);
  if (text.trim().length < 8) return; // too short to be worth a category
  var prompt =
    "Classify this dictated note under ONE short lowercase category label " +
    "(1-2 words, e.g. work, personal, ideas, todo, journal). " +
    "Reply with ONLY the label, nothing else.\n\nNote:\n" + text;
  try {
    var label = (await grain.llm.complete(prompt) || "").trim().toLowerCase();
    label = label.split("\n")[0].replace(/[^a-z0-9 _-]/g, "").trim();
    if (!label) return;
    var key = "category:" + label;
    var existing = await grain.storage.get(key);
    if (!Array.isArray(existing)) existing = [];
    existing.push({ at: Date.now(), text: text.slice(0, 280) });
    if (existing.length > 200) existing = existing.slice(-200);
    await grain.storage.set(key, existing);
    await grain.log.info("auto-categorized into '" + label + "'");
  } catch (e) {
    await grain.log.warn("auto-categorize failed: " + ((e && e.message) || e));
  }
});
"#;

/// Seed Grain's built-in scripted packs at startup. Idempotent: the pack FILE is
/// (re)written so shipped code upgrades reach existing users, while the registry
/// RECORD is installed only when absent so the user's enabled/toggle state
/// survives (SPEC §6, §10.1). Built-ins default **off** and, being first-party,
/// arrive pre-granted their declared capabilities (no permission sheet).
///
/// Call once after `AppContext` + `ExtensionsRegistry` are managed.
pub fn seed_builtin_packs(app: &AppHandle) {
    seed_pack(
        app,
        AUTO_CATEGORIZE_ID,
        "Auto-Categorize",
        "Files each dictation under an AI-chosen category, kept in the extension's own storage.",
        &["storage", "llm", "events:transcripts"],
        &["onEvent:TranscriptionComplete"],
        AUTO_CATEGORIZE_ENTRY,
    );
}

fn seed_pack(
    app: &AppHandle,
    id: &str,
    name: &str,
    description: &str,
    permissions: &[&str],
    activation: &[&str],
    entry_source: &str,
) {
    let ctx = match app.try_state::<Arc<AppContext>>() {
        Some(c) => c,
        None => return,
    };
    let dir = ctx.data_dir.join("extensions");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let pack = json!({
        "manifest": {
            "id": id,
            "name": name,
            "version": env!("CARGO_PKG_VERSION"),
            "grain_api": grain_sdk::GRAIN_API_VERSION,
            "tier": "scripted",
            "description": description,
            "permissions": permissions,
            "activation": activation,
            "entry_source": entry_source,
        },
        "payloads": {}
    });
    if let Ok(new_content) = serde_json::to_string_pretty(&pack) {
        let path = dir.join(format!("{id}.grainpack.json"));
        // Write only when changed — ship upgrades without a redundant write.
        let changed = std::fs::read_to_string(&path)
            .map(|old| old != new_content)
            .unwrap_or(true);
        if changed {
            let _ = std::fs::write(&path, new_content);
        }
    }
    // Register DISABLED if absent (SPEC §8: built-in scripted packs default off).
    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        if !reg.is_installed(id) {
            let _ = reg.install(grain_core::extensions::ExtensionRecord {
                id: id.to_string(),
                enabled: false,
                toggle_seq: 0,
                installed_version: env!("CARGO_PKG_VERSION").to_string(),
                // First-party: pre-grant its declared caps so enabling just works.
                granted: permissions.iter().map(|s| s.to_string()).collect(),
                slots: Vec::new(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_core::event::SessionMode;

    /// The index is keyed by `DaemonEvent::variant_name`, so the variants an
    /// activation expands to must be spelled exactly the way events report
    /// themselves — otherwise an activation silently never fires.
    #[test]
    fn activation_variants_match_real_event_names() {
        let started = DaemonEvent::RecordingStarted {
            session_id: 1,
            mode: SessionMode::Dictation,
        };
        let done = DaemonEvent::TranscriptionComplete {
            session_id: 1,
            text: "hi".into(),
        };

        let on_event = vec!["onEvent:TranscriptionComplete".to_string()];
        assert_eq!(activation_variants(&on_event), vec![done.variant_name()]);
        assert!(!declares_transform(&on_event));

        // onTransform warms at session start (SPEC §3.1), nothing else.
        let transform = vec!["onTransform".to_string()];
        assert_eq!(
            activation_variants(&transform),
            vec![started.variant_name()]
        );
        assert!(declares_transform(&transform));

        // Non-event clauses contribute nothing to the event index.
        assert!(activation_variants(&["onStartup".to_string()]).is_empty());
        assert!(activation_variants(&[]).is_empty());

        // Duplicates collapse (onTransform + an explicit RecordingStarted).
        let both = vec![
            "onTransform".to_string(),
            "onEvent:RecordingStarted".to_string(),
        ];
        assert_eq!(activation_variants(&both).len(), 1);
    }

    /// The whole point of the index: with nothing enabled, the paste path and
    /// the event bus must be a single atomic load — no registry, no disk.
    #[test]
    fn hot_paths_are_guarded_off_by_default() {
        assert!(!HAS_TRANSFORMS.load(Ordering::Relaxed));
        assert!(!HAS_ACTIVATIONS.load(Ordering::Relaxed));
    }

    fn worker(last_activity: u64, resident: bool) -> Worker {
        Worker {
            token: "tok".into(),
            resident,
            strikes: 0,
            last_activity: Arc::new(AtomicU64::new(last_activity)),
            conn: None,
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// The host-call round-trip: `call` sends a `{"call":…}` frame down the
    /// worker's channel and blocks until the matching `resolve(call_id, …)`
    /// answers it — proving call-id correlation over the mpsc/oneshot pair (the
    /// Rust-level fake worker the DoD asks for).
    #[test]
    fn call_roundtrips_through_a_fake_worker() {
        rt().block_on(async {
            let workers = Workers::new();
            let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
            workers.attach("com.x.a", out_tx);

            // The fake worker: read the outbound HostCall, answer via resolve().
            let responder = async {
                let msg = out_rx.recv().await.expect("host emits a call frame");
                let txt = match msg {
                    Message::Text(t) => t.to_string(),
                    _ => panic!("expected a text frame"),
                };
                let call_id = match serde_json::from_str::<HostFrame>(&txt).unwrap() {
                    HostFrame::Call(c) => {
                        assert_eq!(c.method, "transform");
                        assert_eq!(c.params, json!({ "text": "hi" }));
                        c.call_id
                    }
                    _ => panic!("expected a Call frame"),
                };
                workers.resolve("com.x.a", call_id, Ok(json!({ "text": "HI" })));
            };

            let (res, _) = tokio::join!(
                workers.call(
                    "com.x.a",
                    "transform",
                    json!({ "text": "hi" }),
                    Duration::from_secs(2),
                ),
                responder,
            );
            assert_eq!(res.unwrap(), json!({ "text": "HI" }));
        });
    }

    #[test]
    fn call_errors_when_silent_or_unconnected() {
        rt().block_on(async {
            let workers = Workers::new();
            // Not connected → immediate error, no hang.
            assert_eq!(
                workers
                    .call("ghost", "transform", json!({}), Duration::from_millis(20))
                    .await
                    .unwrap_err(),
                "worker not connected"
            );
            // Connected but silent → the deadline fires (never blocks the paste).
            let (out_tx, _keep) = mpsc::unbounded_channel::<Message>();
            workers.attach("com.x.a", out_tx);
            assert_eq!(
                workers
                    .call("com.x.a", "transform", json!({}), Duration::from_millis(20))
                    .await
                    .unwrap_err(),
                "deadline exceeded"
            );
        });
    }

    #[test]
    fn resolve_unknown_call_is_a_noop() {
        let workers = Workers::new();
        workers.resolve("nobody", 7, Ok(Value::Null)); // must not panic
    }

    #[test]
    fn reaper_picks_only_stale_free_nonresident_workers() {
        let workers = Workers::new();
        workers.insert("stale", worker(0, false)); // ancient
        workers.insert("fresh", worker(now_secs(), false)); // just active
        workers.insert("resident", worker(0, true)); // never reaped
        let victims = workers.idle_victims(now_secs(), IDLE_REAP_SECS);
        assert_eq!(victims, vec!["stale".to_string()]);
    }

    #[test]
    fn strikes_trip_at_the_limit_and_reset() {
        let workers = Workers::new();
        workers.insert("a", worker(0, false));
        assert!(!workers.record_strike("a", MAX_STRIKES));
        assert!(!workers.record_strike("a", MAX_STRIKES));
        assert!(workers.record_strike("a", MAX_STRIKES)); // 3rd strike → trip
        workers.clear_strikes("a");
        assert!(!workers.record_strike("a", MAX_STRIKES)); // counter reset
        assert!(!workers.record_strike("missing", MAX_STRIKES)); // unknown never trips
    }
}
