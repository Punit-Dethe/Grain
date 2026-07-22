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
//! [`crate::host_api`]). This lifecycle index also requires the corresponding
//! grant before a declared activation can wake a worker, so the carried wake
//! payload cannot bypass the connection's live-event filter.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use grain_core::{AppContext, DaemonEvent};
use grain_sdk::{daemon_event_capability, GrainPack, HostCall, HostFrame};
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
/// A session mode owns the deliberately slow stage, but never indefinitely.
/// Thirty seconds is generous for a routed model call and still guarantees a
/// hung worker cannot eat the user's transcript.
const SESSION_STAGE_DEADLINE: Duration = Duration::from_secs(30);
/// Consecutive transform failures before auto-disable (SPEC §3.3).
const MAX_STRIKES: u32 = 3;
/// Generous pathology guard, not accounting: Chromium's reported JS heap is
/// not the worker process footprint. It is still the right signal for a runaway
/// extension allocation, which is the failure this ceiling is meant to stop.
const WORKER_HEAP_LIMIT_BYTES: u64 = 128 * 1024 * 1024;
const MEMORY_SAMPLE_DEADLINE: Duration = Duration::from_secs(2);
/// Source maps are read only after a dev worker fails. Bound the exceptional
/// allocation independently from the 5 MB generated-entry limit.
const MAX_SOURCE_MAP_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone)]
struct DevSource {
    root: PathBuf,
    entry: PathBuf,
}

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
    /// Consecutive over-budget samples. Separate from transform strikes so a
    /// healthy heap sample cannot forgive a slow transform (or vice versa).
    memory_strikes: u32,
    /// Epoch seconds of the last frame from this worker; the reaper's clock.
    /// `Arc` so the connection can bump it directly (no lock per frame).
    last_activity: Arc<AtomicU64>,
    conn: Option<WorkerConn>,
    /// Paths only; the source map is loaded and parsed solely on failure.
    dev_source: Option<DevSource>,
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

    fn remove_if_token(&self, ext_id: &str, token: &str) -> Option<Worker> {
        let mut map = self.map.lock().unwrap();
        if !map.get(ext_id).is_some_and(|worker| worker.token == token) {
            return None;
        }
        map.remove(ext_id)
    }

    fn len(&self) -> usize {
        self.map.lock().unwrap().len()
    }

    fn dev_source(&self, ext_id: &str) -> Option<DevSource> {
        self.map
            .lock()
            .unwrap()
            .get(ext_id)
            .and_then(|worker| worker.dev_source.clone())
    }

    /// Register a spawned worker's matching connection and return its shared
    /// last-activity clock. A stale or unspawned token is rejected.
    fn attach(
        &self,
        ext_id: &str,
        token: &str,
        out_tx: mpsc::UnboundedSender<Message>,
    ) -> Option<Arc<AtomicU64>> {
        let mut map = self.map.lock().unwrap();
        let w = map.get_mut(ext_id)?;
        if w.token != token {
            return None;
        }
        w.last_activity.store(now_secs(), Ordering::Relaxed);
        w.conn = Some(WorkerConn {
            out_tx,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_call_id: Arc::new(AtomicU64::new(1)),
        });
        Some(w.last_activity.clone())
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

    /// Record a transform failure and return its new consecutive-strike count.
    fn record_strike(&self, ext_id: &str) -> Option<u32> {
        match self.map.lock().unwrap().get_mut(ext_id) {
            Some(w) => {
                w.strikes += 1;
                Some(w.strikes)
            }
            None => None,
        }
    }

    fn clear_memory_strikes(&self, ext_id: &str) {
        if let Some(worker) = self.map.lock().unwrap().get_mut(ext_id) {
            worker.memory_strikes = 0;
        }
    }

    fn record_memory_strike(&self, ext_id: &str) -> Option<u32> {
        self.map.lock().unwrap().get_mut(ext_id).map(|worker| {
            worker.memory_strikes += 1;
            worker.memory_strikes
        })
    }

    fn connected_ids(&self) -> Vec<String> {
        self.map
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, worker)| worker.conn.as_ref().map(|_| id.clone()))
            .collect()
    }

    /// Send a host notification that intentionally has no response waiter.
    /// Call id zero is reserved for these one-way lifecycle messages.
    fn notify(&self, ext_id: &str, method: &str, params: Value) -> Result<(), String> {
        let out_tx = self
            .map
            .lock()
            .unwrap()
            .get(ext_id)
            .and_then(|worker| worker.conn.as_ref())
            .map(|conn| conn.out_tx.clone())
            .ok_or("worker not connected")?;
        let frame = HostFrame::Call(HostCall {
            call_id: 0,
            method: method.to_string(),
            params,
        });
        let json = serde_json::to_string(&frame).map_err(|error| error.to_string())?;
        out_tx
            .send(Message::Text(json.into()))
            .map_err(|_| "worker channel closed".to_string())
    }

    /// Stop waiting for every host-initiated call to this worker. Session
    /// cancellation uses this to return immediately; late replies are ignored.
    fn cancel_pending(&self, ext_id: &str, reason: &str) {
        let pending = self
            .map
            .lock()
            .unwrap()
            .get(ext_id)
            .and_then(|worker| worker.conn.as_ref())
            .map(|conn| conn.pending.clone());
        if let Some(pending) = pending {
            for (_, sender) in pending.lock().unwrap().drain() {
                let _ = sender.send(Err(reason.to_string()));
            }
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
    let mut startup_workers: Vec<(String, GrainPack, Vec<String>)> = Vec::new();

    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        for rec in reg.records() {
            if !rec.enabled {
                continue;
            }
            let pack = match load_manifest(app, &rec.id) {
                Some(p) if p.is_scripted() => p,
                _ => continue,
            };
            let mut granted_variants = Vec::new();
            for variant in declared_event_variants(&pack.manifest.activation) {
                let Some(capability) = daemon_event_capability(&variant) else {
                    continue;
                };
                if has_grant(&rec.granted, capability) {
                    granted_variants.push(variant);
                } else {
                    log::warn!(
                        "[ext:{}] deny activation onEvent:{variant} missing capability {capability}",
                        rec.id
                    );
                }
            }
            if declares_transform(&pack.manifest.activation)
                && has_grant(&rec.granted, "transform:transcript")
            {
                transforms.push((rec.id.clone(), rec.toggle_seq));
                granted_variants.push("RecordingStarted".to_string());
            } else if declares_transform(&pack.manifest.activation) {
                log::warn!(
                    "[ext:{}] deny activation onTransform missing capability transform:transcript",
                    rec.id
                );
            }
            granted_variants.sort();
            granted_variants.dedup();
            for variant in granted_variants {
                by_event.entry(variant).or_default().push(rec.id.clone());
            }
            if declares_startup(&pack.manifest.activation) && !is_running(&rec.id) {
                startup_workers.push((rec.id.clone(), pack, rec.granted.clone()));
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
    // "The extension set changed" is exactly the trigger for reconciling
    // contributed shortcuts, so every caller of `refresh_index` gets it for
    // free rather than having to remember a second call. `sync` defers onto
    // the async runtime, so this stays safe even when the change was caused by
    // a shortcut press.
    crate::extension_shortcuts::sync(app);
    // Same logic for the pill theme (SPEC §9): a change to the `pill.theme` slot
    // occupant only happens through a registry mutation, and this runs on every
    // one. The broadcast reaches a connected pill; a pill that connects later
    // gets the theme in its welcome instead.
    crate::pill_theme::broadcast(app);
    for (id, pack, granted) in startup_workers {
        if !is_running(&id) {
            log::info!("[ext:{id}] life activation startup");
            spawn_worker(app, &id, &pack, granted, None);
        }
    }
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
    #[serde(default)]
    stack: Option<String>,
    #[serde(default)]
    worker_url: Option<String>,
    #[serde(default)]
    entry_line_offset: u32,
    #[serde(default)]
    line: Option<u32>,
    #[serde(default)]
    column: Option<u32>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Resolve the map declared by the exact generated file the worker executed.
/// External maps must remain inside the already-approved project root; inline
/// maps need no filesystem access. Nothing is retained after this call.
fn load_dev_source_map(source: &DevSource) -> Option<(sourcemap::DecodedMap, PathBuf)> {
    let generated = std::fs::read(&source.entry).ok()?;
    let reference = sourcemap::locate_sourcemap_reference_slice(&generated)
        .ok()
        .flatten()?;
    let base = source.entry.parent()?.to_path_buf();
    if let Ok(Some(map)) = reference.get_embedded_sourcemap() {
        return Some((map, base));
    }

    let map_path = reference.resolve_path(&source.entry)?.canonicalize().ok()?;
    if !map_path.starts_with(&source.root) || !map_path.is_file() {
        return None;
    }
    if std::fs::metadata(&map_path).ok()?.len() > MAX_SOURCE_MAP_BYTES {
        log::warn!(
            "[GRAIN] ext-host: refusing source map over {} MB: {}",
            MAX_SOURCE_MAP_BYTES / (1024 * 1024),
            map_path.display()
        );
        return None;
    }
    let bytes = std::fs::read(&map_path).ok()?;
    let map = sourcemap::decode_slice(&bytes).ok()?;
    Some((map, map_path.parent()?.to_path_buf()))
}

fn mapped_location(
    map: &sourcemap::DecodedMap,
    map_base: &std::path::Path,
    project_root: &std::path::Path,
    generated_line: u32,
    generated_column: u32,
) -> Option<String> {
    let token = map.lookup_token(
        generated_line.checked_sub(1)?,
        generated_column.saturating_sub(1),
    )?;
    let raw_source = token.get_source()?;
    let source_path = map_base.join(raw_source);
    let display = source_path
        .canonicalize()
        .ok()
        .filter(|path| path.starts_with(project_root))
        .and_then(|path| path.strip_prefix(project_root).ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(raw_source));
    Some(format!(
        "{}:{}:{}",
        display.to_string_lossy().replace('\\', "/"),
        token.get_src_line() + 1,
        token.get_src_col() + 1
    ))
}

/// Replace every entry-source frame in a worker stack while leaving runtime
/// shim frames untouched. Worker coordinates are one-indexed and include the
/// supervisor prefix; source-map coordinates are zero-indexed and do not.
fn map_worker_error(source: &DevSource, payload: &DiedPayload) -> Option<String> {
    let (map, map_base) = load_dev_source_map(source)?;
    let map_line = |worker_line: u32, column: u32| {
        let generated_line = worker_line.checked_sub(payload.entry_line_offset)?;
        mapped_location(&map, &map_base, &source.root, generated_line, column)
    };

    if let (Some(stack), Some(worker_url)) = (&payload.stack, &payload.worker_url) {
        let pattern = format!(r"{}:(\d+):(\d+)", regex::escape(worker_url));
        let frames = regex::Regex::new(&pattern).ok()?;
        let mapped = frames.replace_all(stack, |captures: &regex::Captures<'_>| {
            let original = captures.get(0).map(|m| m.as_str()).unwrap_or_default();
            let line = captures.get(1).and_then(|m| m.as_str().parse().ok());
            let column = captures.get(2).and_then(|m| m.as_str().parse().ok());
            match (line, column) {
                (Some(line), Some(column)) => {
                    map_line(line, column).unwrap_or_else(|| original.to_string())
                }
                _ => original.to_string(),
            }
        });
        if mapped != stack.as_str() {
            return Some(mapped.into_owned());
        }
    }

    let location = map_line(payload.line?, payload.column.unwrap_or(1))?;
    Some(format!("{}\n    at {location}", payload.reason))
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
            let detail = HOST
                .get()
                .and_then(|host| host.workers.dev_source(&p.ext_id))
                .and_then(|source| map_worker_error(&source, &p))
                .or_else(|| p.stack.clone())
                .unwrap_or_else(|| p.reason.clone());
            log::error!("[ext:{}] error {detail}", p.ext_id);
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
            sample_worker_heaps().await;
            reap_idle();
        }
    });
}

// ── Activation ──────────────────────────────────────────────────────────────

/// Explicit daemon-event variants an activation list wakes on. `onTransform`
/// warming is added separately only after its own capability grant is checked.
fn declared_event_variants(activation: &[String]) -> Vec<String> {
    let mut out: Vec<String> = activation
        .iter()
        .filter_map(|a| {
            if let Some(v) = a.strip_prefix("onEvent:") {
                Some(v.to_string())
            } else {
                None // onStartup / onShortcut / onTransform are handled elsewhere
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

fn declares_startup(activation: &[String]) -> bool {
    activation.iter().any(|a| a == "onStartup")
}

fn has_grant(granted: &[String], capability: &str) -> bool {
    granted.iter().any(|grant| grant == capability)
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
        log::info!("[ext:{id}] life activation {}", ev.variant_name());
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
    let token = crate::events_server::mint_worker_token(ext_id, caps.iter().cloned().collect());
    let resident = pack.manifest.activation.iter().any(|a| a == "onStartup");
    let dev_source = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .and_then(|registry| registry.dev_path(ext_id))
        .and_then(|root| crate::dev_extensions::load_project(&root).ok())
        .map(|project| DevSource {
            root: project.root,
            entry: project.entry_path,
        });
    host.workers.insert(
        ext_id,
        Worker {
            token: token.clone(),
            resident,
            strikes: 0,
            memory_strikes: 0,
            last_activity: Arc::new(AtomicU64::new(now_secs())),
            conn: None,
            dev_source,
        },
    );
    let payload = SpawnPayload {
        ext_id: ext_id.to_string(),
        token,
        entry_source: pack.manifest.entry_source.clone(),
        caps,
        activation,
    };
    log::info!("[ext:{ext_id}] life worker spawned (resident={resident})");
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
fn kill_worker_inner(ext_id: &str, reason: &str, token: Option<&str>, preserve_supervisor: bool) {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    let worker = match token {
        Some(token) => host.workers.remove_if_token(ext_id, token),
        None => host.workers.remove(ext_id),
    };
    let worker = match worker {
        Some(w) => w,
        None => return,
    };
    let empty = host.workers.is_empty();
    log::info!("[ext:{ext_id}] life worker reaped ({reason})");
    // A reload/disable may reap the worker while its session-owned slow stage
    // is awaiting a result. Give the worker's AbortSignal one best-effort turn
    // before closing the socket, then release the host-side waiter below.
    if crate::extension_session::is_owned_by(ext_id) {
        if let Some(conn) = &worker.conn {
            let frame = HostFrame::Call(HostCall {
                call_id: 0,
                method: "session.cancel".to_string(),
                params: json!({ "reason": reason }),
            });
            if let Ok(payload) = serde_json::to_string(&frame) {
                let _ = conn.out_tx.send(Message::Text(payload.into()));
            }
        }
    }
    crate::events_server::revoke_token(&worker.token);
    let _ = host.app.emit_to(
        SUPERVISOR_LABEL,
        "ext-host://kill",
        KillPayload {
            ext_id: ext_id.to_string(),
        },
    );
    if let Some(conn) = &worker.conn {
        let _ = conn.out_tx.send(Message::Close(None));
        for (_, tx) in conn.pending.lock().unwrap().drain() {
            let _ = tx.send(Err("worker terminated".into()));
        }
    }
    if empty && !preserve_supervisor {
        teardown_supervisor();
    }
}

fn kill_worker(ext_id: &str, reason: &str) {
    kill_worker_inner(ext_id, reason, None, false);
}

/// Public lifecycle hook for registry operations such as unloading a dev
/// override. It is a no-op when the extension has no live worker.
pub fn stop_extension(ext_id: &str, reason: &str) {
    kill_worker(ext_id, reason);
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
        if crate::extension_session::is_owned_by(&id) {
            continue;
        }
        kill_worker(&id, "idle timeout");
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HeapSample {
    Unsupported,
    Bytes(u64),
}

fn parse_heap_sample(value: Value) -> Result<HeapSample, String> {
    if value
        .get("supported")
        .and_then(Value::as_bool)
        .is_some_and(|supported| !supported)
    {
        return Ok(HeapSample::Unsupported);
    }
    value
        .get("usedBytes")
        .and_then(Value::as_u64)
        .map(HeapSample::Bytes)
        .ok_or_else(|| "worker returned an invalid heap sample".to_string())
}

/// Sample live realms on the existing reaper tick. `performance.memory` is a
/// Chromium engine estimate, not process RSS; it catches runaway allocations
/// without pretending to be a billing-grade memory accountant.
async fn sample_worker_heaps() {
    let Some(host) = HOST.get() else {
        return;
    };
    let ids = host.workers.connected_ids();
    let samples = futures_util::future::join_all(ids.iter().map(|id| {
        host.workers
            .call(id, "memory.sample", Value::Null, MEMORY_SAMPLE_DEADLINE)
    }))
    .await;

    for (id, sample) in ids.into_iter().zip(samples) {
        let Ok(sample) = sample.and_then(parse_heap_sample) else {
            // Missing engine support or a transient worker failure is not an
            // over-budget reading and therefore never earns a strike.
            continue;
        };
        match sample {
            HeapSample::Unsupported => {}
            HeapSample::Bytes(bytes) if bytes <= WORKER_HEAP_LIMIT_BYTES => {
                host.workers.clear_memory_strikes(&id);
            }
            HeapSample::Bytes(bytes) => {
                let strike = host.workers.record_memory_strike(&id).unwrap_or(0);
                log::warn!(
                    "[ext:{id}] life worker heap {} MiB exceeds {} MiB (strike {strike} of {MAX_STRIKES})",
                    bytes / (1024 * 1024),
                    WORKER_HEAP_LIMIT_BYTES / (1024 * 1024),
                );
                if strike >= MAX_STRIKES {
                    auto_disable(
                        &host.app,
                        &id,
                        format!(
                            "It repeatedly exceeded the {} MiB extension worker heap ceiling.",
                            WORKER_HEAP_LIMIT_BYTES / (1024 * 1024)
                        ),
                    );
                }
            }
        }
    }
}

// ── Connection surface (called from events_server on the worker's WS) ────────

/// A worker's WS authenticated: register its outbound channel and return the
/// shared last-activity clock for the connection to bump on each frame. The
/// token must match the current generation, so a stale socket cannot replace it.
pub fn attach_connection(
    ext_id: &str,
    token: &str,
    out_tx: mpsc::UnboundedSender<Message>,
) -> Option<Arc<AtomicU64>> {
    match HOST.get() {
        Some(h) => h.workers.attach(ext_id, token, out_tx),
        None => None,
    }
}

/// The worker's WS closed → the worker process is gone; reap it.
pub fn detach_connection(ext_id: &str, token: &str) {
    kill_worker_inner(ext_id, "connection closed", Some(token), false);
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
        let started = Instant::now();
        match host
            .workers
            .call(
                &id,
                "transform",
                json!({ "text": current }),
                TRANSFORM_DEADLINE,
            )
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
                let strikes = record_strike(app, &id);
                if e == "deadline exceeded" {
                    log::warn!(
                        "[ext:{id}] slow transform took {} ms (budget {} ms) — strike {strikes} of {MAX_STRIKES}",
                        started.elapsed().as_millis(),
                        TRANSFORM_DEADLINE.as_millis(),
                    );
                }
            }
        }
    }
    current
}

#[derive(Debug, PartialEq, Eq)]
pub enum SessionStageOutput {
    Text(String),
    Handled,
}

fn parse_session_stage_output(value: Value) -> Result<SessionStageOutput, String> {
    if let Some(text) = value.as_str() {
        return Ok(SessionStageOutput::Text(text.to_string()));
    }
    if value
        .get("handled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(SessionStageOutput::Handled);
    }
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        return Ok(SessionStageOutput::Text(text.to_string()));
    }
    Err("session stage returned neither text nor handled=true".into())
}

/// Keep the owning worker warm for the recording. The activation payload is
/// data only; the session's identity still comes from its minted token.
pub fn wake_for_session(app: &AppHandle, ext_id: &str, mode: &str) {
    if is_running(ext_id) {
        return;
    }
    let Some(pack) = load_manifest(app, ext_id) else {
        return;
    };
    let granted = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .and_then(|registry| registry.record(ext_id))
        .map(|record| record.granted)
        .unwrap_or_default();
    spawn_worker(
        app,
        ext_id,
        &pack,
        granted,
        Some(json!({ "Session": { "mode": mode } })),
    );
}

/// Run the owner-controlled slow stage. Failure is deliberately returned to
/// the caller, which falls back to the exact input text.
pub async fn run_session_stage(
    ext_id: &str,
    mode: &str,
    text: &str,
) -> Result<SessionStageOutput, String> {
    let host = HOST.get().ok_or("extension host unavailable")?;
    let result = host
        .workers
        .call(
            ext_id,
            "sessionStage",
            json!({ "mode": mode, "text": text }),
            SESSION_STAGE_DEADLINE,
        )
        .await;
    if result
        .as_ref()
        .is_err_and(|error| error == "deadline exceeded")
    {
        let _ = host
            .workers
            .notify(ext_id, "session.cancel", json!({ "reason": "timeout" }));
    }
    parse_session_stage_output(result?)
}

/// User cancellation is immediate: notify the handler's AbortSignal and drop
/// the Rust waiter. The worker may finish later; its response has no recipient.
pub fn cancel_session_stage(ext_id: &str, reason: &str) {
    let Some(host) = HOST.get() else {
        return;
    };
    let _ = host
        .workers
        .notify(ext_id, "session.cancel", json!({ "reason": reason }));
    host.workers.cancel_pending(ext_id, reason);
}

/// How long the host waits for a worker to *acknowledge* a shortcut. The
/// runtime acknowledges on receipt and runs the handler detached, so this
/// covers delivery only — a shortcut that opens an LLM call is not "slow".
const SHORTCUT_DEADLINE: Duration = Duration::from_secs(2);

/// A contributed shortcut fired (SPEC §3.3). Wakes the extension if it is cold,
/// otherwise hands the press to the running worker.
///
/// Everything happens on the async runtime: the caller is the global-shortcut
/// dispatch path, where blocking hangs every hotkey in the app.
pub fn wake_for_shortcut(app: &AppHandle, ext_id: &str, shortcut_id: &str) {
    let app = app.clone();
    let ext_id = ext_id.to_string();
    let shortcut_id = shortcut_id.to_string();
    tauri::async_runtime::spawn(async move {
        log::info!("[ext:{ext_id}] life activation shortcut:{shortcut_id}");
        if is_running(&ext_id) {
            let host = match HOST.get() {
                Some(h) => h,
                None => return,
            };
            if let Err(e) = host
                .workers
                .call(
                    &ext_id,
                    "shortcut",
                    json!({ "id": shortcut_id }),
                    SHORTCUT_DEADLINE,
                )
                .await
            {
                log::warn!("[GRAIN] shortcut '{shortcut_id}' → '{ext_id}' failed: {e}");
            }
            return;
        }
        // Cold: the press IS the activation, and travels as the payload — the
        // same device `on_event` uses, since there is no ready handshake to
        // wait on and a dropped keypress would be indistinguishable from a bug.
        let Some(pack) = load_manifest(&app, &ext_id) else {
            return;
        };
        let granted = app
            .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
            .and_then(|reg| reg.record(&ext_id))
            .map(|r| r.granted)
            .unwrap_or_default();
        spawn_worker(
            &app,
            &ext_id,
            &pack,
            granted,
            Some(json!({ "Shortcut": { "id": shortcut_id } })),
        );
    });
}

fn clear_strikes(ext_id: &str) {
    if let Some(host) = HOST.get() {
        host.workers.clear_strikes(ext_id);
    }
}

fn record_strike(app: &AppHandle, ext_id: &str) -> u32 {
    let host = match HOST.get() {
        Some(h) => h,
        None => return 0,
    };
    let strikes = host.workers.record_strike(ext_id).unwrap_or(0);
    if strikes >= MAX_STRIKES {
        auto_disable(
            app,
            ext_id,
            "It repeatedly missed the 150 ms transform deadline.".to_string(),
        );
    }
    strikes
}

/// Whether `id` is an explicitly loaded-unpacked project. Diagnostic call
/// logging uses this cheap registry lookup so installed extensions add no log
/// traffic or formatting work to the host-API path.
pub fn is_dev_extension(app: &AppHandle, id: &str) -> bool {
    app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .and_then(|registry| registry.dev_path(id))
        .is_some()
}

/// SPEC §3.3: a persistently failing transform is disabled (not left slowing
/// every paste). The user re-enables explicitly from Overview.
fn auto_disable(app: &AppHandle, ext_id: &str, reason: String) {
    log::warn!("[ext:{ext_id}] life auto-disabled: {reason}");
    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        let _ = reg.set_enabled(ext_id, false);
    }
    refresh_index(app); // drop it from the hot-path index immediately
    kill_worker(ext_id, "auto-disabled after repeated resource violations");
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        ctx.emit(DaemonEvent::ExtensionDisabled {
            id: ext_id.to_string(),
            reason,
        });
    }
}

/// Load the effective extension source. A dev project is re-read from its
/// canonical folder; otherwise this reads the installed `.grainpack.json`.
pub fn load_manifest_result(app: &AppHandle, id: &str) -> Result<GrainPack, String> {
    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        if let Some(path) = reg.dev_path(id) {
            if !crate::settings::get_settings(app).extension_developer_mode {
                return Err("developer mode is disabled".into());
            }
            return crate::dev_extensions::load_project(&path).map(|project| project.pack);
        }
    }
    let ctx = app
        .try_state::<Arc<AppContext>>()
        .ok_or("app context unavailable")?;
    let path = ctx
        .data_dir
        .join("extensions")
        .join(format!("{id}.grainpack.json"));
    let raw = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw).map_err(|error| error.to_string())
}

pub fn load_manifest(app: &AppHandle, id: &str) -> Option<GrainPack> {
    load_manifest_result(app, id).ok()
}

/// Re-read and atomically activate an already-approved load-unpacked project.
/// The developer channel supplies only the id; source and capabilities remain
/// rooted in the canonical folder and registry selected through Grain's UI.
pub fn reload_dev_extension(
    app: &AppHandle,
    id: &str,
) -> Result<grain_sdk::DevReloadResult, String> {
    if !crate::settings::get_settings(app).extension_developer_mode {
        return Err("Developer mode is disabled".into());
    }
    let reg = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let path = reg.dev_path(id).ok_or_else(|| {
        format!("'{id}' is not loaded unpacked; add it from Grain settings first")
    })?;
    let loaded = crate::dev_extensions::load_project(&path)?;
    if loaded.pack.manifest.id != id {
        return Err(format!(
            "manifest id changed from '{id}' to '{}'; unload and add the project again",
            loaded.pack.manifest.id
        ));
    }

    let prior = reg.record(id).ok_or("developer extension record missing")?;
    let slots_changed = prior.slots != loaded.pack.manifest.slots;
    if slots_changed && prior.enabled {
        reg.set_enabled(id, false)
            .map_err(|error| error.to_string())?;
    }
    let enabled = prior.enabled && !slots_changed;
    let requested = &loaded.pack.manifest.permissions;
    let granted = prior
        .granted
        .iter()
        .filter(|permission| requested.contains(permission))
        .cloned()
        .collect::<Vec<_>>();
    let permissions_changed = granted != prior.granted;
    reg.install(grain_core::extensions::ExtensionRecord {
        id: id.to_string(),
        enabled,
        toggle_seq: prior.toggle_seq,
        installed_version: loaded.pack.manifest.version.clone(),
        granted: granted.clone(),
        slots: loaded.pack.manifest.slots.clone(),
        variant_slots: prior.variant_slots,
        dev: prior.dev,
    })
    .map_err(|error| error.to_string())?;

    let had_worker = is_running(id);
    log::info!("[ext:{id}] life developer reload");
    if had_worker {
        kill_worker_inner(id, "developer hot reload", None, true);
    }
    refresh_index(app);
    if had_worker && enabled && !is_running(id) {
        spawn_worker(app, id, &loaded.pack, granted, None);
    }
    let remounted_surfaces = if enabled && !permissions_changed {
        if loaded.pack.manifest.surfaces.workspace.is_none() {
            crate::surfaces::extension::destroy(app, id);
        }
        if loaded.pack.manifest.surfaces.overlay.is_none() {
            crate::surfaces::overlay::dismiss(app, id);
        }
        crate::surfaces::extension::reload(app, id, &loaded.pack)
    } else {
        crate::surfaces::extension::destroy(app, id);
        crate::surfaces::overlay::dismiss(app, id);
        false
    };

    let worker_count = HOST.get().map(|host| host.workers.len()).unwrap_or(0);
    Ok(grain_sdk::DevReloadResult {
        restarted_worker: had_worker && enabled,
        remounted_surfaces,
        enabled,
        worker_count,
        token_count: crate::events_server::token_count(),
    })
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
  // Declared settings (SPEC 4.1): the host renders these, validates them on the
  // way in, and hands back values this code can trust — no defaulting here
  // beyond the case where the read itself fails.
  var minLength = 8;
  var keepPer = 200;
  try {
    var a = await grain.settings.get("min_length");
    var b = await grain.settings.get("max_per_category");
    if (typeof a === "number") minLength = a;
    if (typeof b === "number") keepPer = b;
  } catch (e) {
    // No `settings` grant (an older install): the declared defaults still apply.
  }
  if (text.trim().length < minLength) return;
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
    if (existing.length > keepPer) existing = existing.slice(-keepPer);
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
        &["storage", "settings", "llm", "events:transcripts"],
        &["onEvent:TranscriptionComplete"],
        AUTO_CATEGORIZE_ENTRY,
        // [GRAIN] The first dogfood of the schema settings render (SPEC §4.1).
        // Anchored to the dictation pipeline, so these controls appear beside
        // post-processing — the feature they extend — rather than in a tab of
        // their own. The worker reads these same keys back over `settings.get`.
        json!([
            {
                "key": "min_length",
                "label": "Minimum note length",
                "description": "Shorter dictations are left uncategorized.",
                "kind": "number",
                "min": 1,
                "max": 500,
                "default": 8,
                "anchor": "dictation.pipeline.after",
                "order": 0
            },
            {
                "key": "max_per_category",
                "label": "Notes kept per category",
                "description": "The oldest are dropped once a category is full.",
                "kind": "slider",
                "min": 20.0,
                "max": 1000.0,
                "step": 20.0,
                "default": 200,
                "anchor": "dictation.pipeline.after",
                "order": 1
            }
        ]),
    );
}

#[allow(clippy::too_many_arguments)]
fn seed_pack(
    app: &AppHandle,
    id: &str,
    name: &str,
    description: &str,
    permissions: &[&str],
    activation: &[&str],
    entry_source: &str,
    settings: serde_json::Value,
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
            "contributes": { "settings": settings },
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
        if let Some(mut rec) = reg.record(id) {
            // A shipped built-in that gained a capability in this release: top
            // up its grants so the upgrade doesn't leave it half-working.
            //
            // Safe ONLY because `seed_pack` is called exclusively for
            // first-party packs whose permissions ship inside Grain itself and
            // were pre-granted at install. A third-party pack that widens its
            // permissions goes through the permission diff instead (SPEC §6) —
            // never through here.
            debug_assert!(id.starts_with("grain."));
            let added = permissions
                .iter()
                .filter(|p| !rec.granted.iter().any(|g| g == *p))
                .map(|p| p.to_string())
                .collect::<Vec<_>>();
            if !added.is_empty() && id.starts_with("grain.") {
                log::info!("[GRAIN] built-in '{id}' gained capabilities: {added:?}");
                rec.granted.extend(added);
                let _ = reg.install(rec);
            }
        }
        if !reg.is_installed(id) {
            let _ = reg.install(grain_core::extensions::ExtensionRecord {
                id: id.to_string(),
                enabled: false,
                toggle_seq: 0,
                installed_version: env!("CARGO_PKG_VERSION").to_string(),
                // First-party: pre-grant its declared caps so enabling just works.
                granted: permissions.iter().map(|s| s.to_string()).collect(),
                slots: Vec::new(),
                variant_slots: Vec::new(),
                dev: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The index is keyed by `DaemonEvent::variant_name`, so the variants an
    /// activation expands to must be spelled exactly the way events report
    /// themselves — otherwise an activation silently never fires.
    #[test]
    fn activation_variants_match_real_event_names() {
        let done = DaemonEvent::TranscriptionComplete {
            session_id: 1,
            text: "hi".into(),
        };

        let on_event = vec!["onEvent:TranscriptionComplete".to_string()];
        assert_eq!(
            declared_event_variants(&on_event),
            vec![done.variant_name()]
        );
        assert!(!declares_transform(&on_event));

        // onTransform warming is indexed separately after its grant check.
        let transform = vec!["onTransform".to_string()];
        assert!(declared_event_variants(&transform).is_empty());
        assert!(declares_transform(&transform));
        assert!(!declares_startup(&transform));

        // Non-event clauses contribute nothing to the event index.
        assert!(declared_event_variants(&["onStartup".to_string()]).is_empty());
        assert!(declares_startup(&["onStartup".to_string()]));
        assert!(declared_event_variants(&[]).is_empty());

        // Duplicate explicit declarations collapse before indexing.
        let both = vec![
            "onEvent:RecordingStarted".to_string(),
            "onEvent:RecordingStarted".to_string(),
        ];
        assert_eq!(declared_event_variants(&both).len(), 1);
    }

    #[test]
    fn activation_index_requires_the_matching_user_grant() {
        let granted = vec!["events:sessions".to_string()];
        assert!(has_grant(&granted, "events:sessions"));
        assert!(!has_grant(&granted, "events:transcripts"));
        assert!(!has_grant(&granted, "transform:transcript"));
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
            memory_strikes: 0,
            last_activity: Arc::new(AtomicU64::new(last_activity)),
            conn: None,
            dev_source: None,
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
            workers.insert("com.x.a", worker(now_secs(), false));
            workers.attach("com.x.a", "tok", out_tx).unwrap();

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
            workers.insert("com.x.a", worker(now_secs(), false));
            workers.attach("com.x.a", "tok", out_tx).unwrap();
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
    fn session_stage_parses_replace_suppress_and_rejects_ambiguous_output() {
        assert_eq!(
            parse_session_stage_output(json!("changed")).unwrap(),
            SessionStageOutput::Text("changed".into())
        );
        assert_eq!(
            parse_session_stage_output(json!({ "text": "changed" })).unwrap(),
            SessionStageOutput::Text("changed".into())
        );
        assert_eq!(
            parse_session_stage_output(json!({ "handled": true })).unwrap(),
            SessionStageOutput::Handled
        );
        assert!(parse_session_stage_output(Value::Null).is_err());
    }

    #[test]
    fn cancelling_pending_calls_releases_the_waiter_immediately() {
        rt().block_on(async {
            let workers = Workers::new();
            let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
            workers.insert("com.x.a", worker(now_secs(), false));
            workers.attach("com.x.a", "tok", out_tx).unwrap();

            let cancel = async {
                out_rx.recv().await.expect("stage call emitted");
                workers.cancel_pending("com.x.a", "session cancelled");
            };
            let (result, _) = tokio::join!(
                workers.call(
                    "com.x.a",
                    "sessionStage",
                    json!({ "text": "keep me" }),
                    Duration::from_secs(30),
                ),
                cancel,
            );
            assert_eq!(result.unwrap_err(), "session cancelled");
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
        assert_eq!(workers.record_strike("a"), Some(1));
        assert_eq!(workers.record_strike("a"), Some(2));
        assert_eq!(workers.record_strike("a"), Some(3)); // 3rd strike → trip
        workers.clear_strikes("a");
        assert_eq!(workers.record_strike("a"), Some(1)); // counter reset
        assert_eq!(workers.record_strike("missing"), None); // unknown never trips
    }

    #[test]
    fn heap_samples_are_typed_and_memory_strikes_are_independent() {
        assert_eq!(
            parse_heap_sample(json!({"supported": true, "usedBytes": 42})).unwrap(),
            HeapSample::Bytes(42)
        );
        assert_eq!(
            parse_heap_sample(json!({"supported": false, "usedBytes": null})).unwrap(),
            HeapSample::Unsupported
        );
        assert!(parse_heap_sample(json!({"supported": true})).is_err());

        let workers = Workers::new();
        workers.insert("leak", worker(0, false));
        assert_eq!(workers.record_memory_strike("leak"), Some(1));
        assert_eq!(workers.record_memory_strike("leak"), Some(2));
        assert_eq!(workers.record_strike("leak"), Some(1));
        workers.clear_memory_strikes("leak");
        assert_eq!(workers.record_memory_strike("leak"), Some(1));
        // The transform counter was not forgiven by a healthy heap sample.
        assert_eq!(workers.record_strike("leak"), Some(2));
    }

    #[test]
    fn stale_worker_token_cannot_attach_or_remove_replacement() {
        let workers = Workers::new();
        let mut replacement = worker(now_secs(), false);
        replacement.token = "new-token".into();
        workers.insert("a", replacement);
        let (out_tx, _out_rx) = mpsc::unbounded_channel::<Message>();
        assert!(workers.attach("a", "old-token", out_tx).is_none());
        assert!(workers.remove_if_token("a", "old-token").is_none());
        assert_eq!(workers.len(), 1);
        assert!(workers.remove_if_token("a", "new-token").is_some());
        assert_eq!(workers.len(), 0);
    }

    #[test]
    fn ten_worker_replacements_leave_one_live_worker() {
        let workers = Workers::new();
        for generation in 0..10 {
            if generation > 0 {
                assert!(workers.remove("dev").is_some());
            }
            let mut next = worker(now_secs(), false);
            next.token = format!("token-{generation}");
            workers.insert("dev", next);
            assert_eq!(workers.len(), 1);
        }
    }

    #[test]
    fn dev_worker_stack_maps_to_the_author_file() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::create_dir(root.join("dist")).unwrap();
        std::fs::write(root.join("src/main.ts"), "throw new Error('mapped');\n").unwrap();
        let entry = root.join("dist/main.js");
        std::fs::write(
            &entry,
            "\"use strict\";\n(() => {\n  throw new Error('mapped');\n})();\n//# sourceMappingURL=main.js.map\n",
        )
        .unwrap();
        std::fs::write(
            root.join("dist/main.js.map"),
            r#"{"version":3,"sources":["../src/main.ts"],"sourcesContent":["throw new Error('mapped');\n"],"mappings":";;AAAA,QAAM,IAAI,MAAM,QAAQ;"}"#,
        )
        .unwrap();

        let mapped = map_worker_error(
            &DevSource { root, entry },
            &DiedPayload {
                ext_id: "com.example.dev".into(),
                reason: "Uncaught Error: mapped".into(),
                stack: Some("Error: mapped\n    at blob:grain-worker:23:3".into()),
                worker_url: Some("blob:grain-worker".into()),
                entry_line_offset: 20,
                line: Some(23),
                column: Some(3),
            },
        )
        .unwrap();

        assert!(mapped.contains("src/main.ts:1:"), "{mapped}");
        assert!(!mapped.contains("blob:grain-worker:23:3"), "{mapped}");
    }
}
