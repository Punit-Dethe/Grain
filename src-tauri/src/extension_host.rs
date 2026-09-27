//! [GRAIN] The extension host (SPEC §3.1, §7.1) — Phase 2, the scripted runtime.
//!
//! Owns the lifecycle of tier-B (scripted) extension workers. One hidden
//! supervisor webview runs Grain's own code and spawns one Web Worker per
//! extension; each worker opens its **own** WebSocket with its **own** token
//! (SPEC §7.1 — never a shared realm, which would make identity forgeable).
//!
//! Explicit tool calls wake workers; no event, transcript or session activation
//! is installed. Rust owns tokens, bounded calls, idle cleanup and generation-
//! scoped supervisor windows. The host API/WS boundary enforces tool-only grants.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use grain_core::{AppContext, DaemonEvent};
use grain_sdk::{GrainPack, HostCall, HostFrame};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

/// The hidden supervisor webview: Grain's code, one per app run, created on the
/// first worker need and torn down when the last worker dies.
const SUPERVISOR_LABEL_PREFIX: &str = "extension-host-";
/// A dedicated frontend route (Step 5) — NOT the SPA root, so no extension code
/// ever shares Grain's main global.
const SUPERVISOR_URL: &str = "extension-host.html";

/// Reaper policy (SPEC §3: workers are ephemeral).
const IDLE_REAP_SECS: u64 = 120;
const REAP_INTERVAL_SECS: u64 = 30;

/// Consecutive over-budget heap samples before resource-policy disable.
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
    pending: PendingCalls,
    next_call_id: Arc<AtomicU64>,
}

type PendingCalls = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

struct PendingCall {
    calls: PendingCalls,
    id: u64,
}

impl Drop for PendingCall {
    fn drop(&mut self) {
        self.calls.lock().unwrap().remove(&self.id);
    }
}

struct QueuedCall {
    pending: PendingCall,
    reply: oneshot::Receiver<Result<Value, String>>,
}

impl QueuedCall {
    async fn wait(self, deadline: Duration) -> Result<Value, String> {
        let result = match tokio::time::timeout(deadline, self.reply).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("worker dropped the call".into()),
            Err(_) => Err("deadline exceeded".into()),
        };
        drop(self.pending);
        result
    }
}

/// Own the exact native worker while startup or a queued call is unfinished.
/// Dropping the request retires that generation; successful replies disarm it.
struct NativeCallOwner<F: FnOnce()> {
    retire: Option<F>,
}

impl<F: FnOnce()> NativeCallOwner<F> {
    fn new(retire: F) -> Self {
        Self {
            retire: Some(retire),
        }
    }

    fn completed(&mut self) {
        self.retire = None;
    }
}

impl<F: FnOnce()> Drop for NativeCallOwner<F> {
    fn drop(&mut self) {
        if let Some(retire) = self.retire.take() {
            retire();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeKind {
    Scripted,
    Companion,
}

struct Worker {
    /// The token minted for this worker; revoked at reap (SPEC §7.1).
    token: String,
    /// Manifest/source and registry identity actually used by this worker.
    call_digest: Option<String>,
    /// `onStartup` workers are never reaped for idleness.
    resident: bool,
    /// Consecutive over-budget heap samples for this worker generation.
    memory_strikes: u32,
    kind: RuntimeKind,
    /// Epoch seconds of the last frame from this worker; the reaper's clock.
    /// `Arc` so the connection can bump it directly (no lock per frame).
    last_activity: Arc<AtomicU64>,
    conn: Option<WorkerConn>,
    /// Paths only; the source map is loaded and parsed solely on failure.
    dev_source: Option<DevSource>,
}

impl Worker {
    fn close_pending(&self) {
        if let Some(conn) = &self.conn {
            let _ = conn.out_tx.send(Message::Close(None));
            for (_, sender) in conn.pending.lock().unwrap().drain() {
                let _ = sender.send(Err("worker terminated".into()));
            }
        }
    }
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

    fn owns_token(&self, ext_id: &str, token: &str) -> bool {
        self.map
            .lock()
            .unwrap()
            .get(ext_id)
            .is_some_and(|worker| worker.token == token)
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

    fn token(&self, ext_id: &str) -> Option<String> {
        self.map
            .lock()
            .unwrap()
            .get(ext_id)
            .map(|worker| worker.token.clone())
    }

    /// Wait only for the generation that this call woke. Removal/replacement
    /// fails promptly instead of migrating the request to a different worker.
    async fn wait_connected(&self, ext_id: &str, token: &str, deadline: Duration) -> bool {
        const POLL: Duration = Duration::from_millis(20);
        let started = Instant::now();
        loop {
            {
                let map = self.map.lock().unwrap();
                let Some(worker) = map.get(ext_id).filter(|worker| worker.token == token) else {
                    return false;
                };
                if worker.conn.is_some() {
                    return true;
                }
            }
            if started.elapsed() >= deadline {
                return false;
            }
            tokio::time::sleep(POLL).await;
        }
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
        self.call_owned(ext_id, None, method, params, deadline)
            .await
    }

    async fn call_owned(
        &self,
        ext_id: &str,
        token: Option<&str>,
        method: &str,
        params: Value,
        deadline: Duration,
    ) -> Result<Value, String> {
        self.call_tracked(
            ext_id,
            token,
            method,
            params,
            deadline,
            &AtomicBool::new(false),
        )
        .await
    }

    /// A successful queue send is the conservative dispatch boundary. Closing
    /// the worker afterward cannot prove that its handler never ran.
    async fn call_tracked(
        &self,
        ext_id: &str,
        token: Option<&str>,
        method: &str,
        params: Value,
        deadline: Duration,
        dispatched: &AtomicBool,
    ) -> Result<Value, String> {
        self.enqueue(ext_id, token, method, params, dispatched, None)?
            .wait(deadline)
            .await
    }

    /// Synchronous queue boundary, usable inside a registry admission lock.
    fn enqueue(
        &self,
        ext_id: &str,
        token: Option<&str>,
        method: &str,
        params: Value,
        dispatched: &AtomicBool,
        expected_digest: Option<&str>,
    ) -> Result<QueuedCall, String> {
        let queued = {
            let map = self.map.lock().unwrap();
            let worker = map.get(ext_id).ok_or("worker not connected")?;
            if token.is_some_and(|token| worker.token != token) {
                return Err("worker generation changed".into());
            }
            if expected_digest
                .is_some_and(|expected| worker.call_digest.as_deref() != Some(expected))
            {
                return Err("worker tool identity changed".into());
            }
            let conn = worker.conn.as_ref().ok_or("worker not connected")?;
            let call_id = conn.next_call_id.fetch_add(1, Ordering::Relaxed);
            let frame = HostFrame::Call(HostCall {
                call_id,
                method: method.to_string(),
                params,
            });
            let json = serde_json::to_string(&frame).map_err(|error| error.to_string())?;
            let (tx, rx) = oneshot::channel();
            conn.pending.lock().unwrap().insert(call_id, tx);
            let guard = PendingCall {
                calls: conn.pending.clone(),
                id: call_id,
            };
            // Queue under the registry lock so removal cannot overtake dispatch
            // or drain before the pending entry is installed.
            if conn.out_tx.send(Message::Text(json.into())).is_err() {
                return Err("worker channel closed".into());
            }
            dispatched.store(true, Ordering::Release);
            QueuedCall {
                pending: guard,
                reply: rx,
            }
        };
        Ok(queued)
    }

    /// Route a `HostCallResult` back to the awaiter of its `call_id` (no-op if
    /// the worker/call is unknown or already timed out).
    fn resolve(&self, ext_id: &str, token: &str, call_id: u64, result: Result<Value, String>) {
        let pending = self
            .map
            .lock()
            .unwrap()
            .get(ext_id)
            .filter(|worker| worker.token == token)
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
    fn idle_victims(&self, now: u64, idle_secs: u64) -> Vec<(String, String)> {
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
                (idle > idle_secs && !busy).then(|| (id.clone(), w.token.clone()))
            })
            .collect()
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

    fn scripted_tokens(&self) -> Vec<(String, String)> {
        self.map
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, worker)| worker.kind == RuntimeKind::Scripted)
            .map(|(id, worker)| (id.clone(), worker.token.clone()))
            .collect()
    }

    fn scripted_connected_ids(&self) -> Vec<String> {
        self.map
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, worker)| {
                (worker.kind == RuntimeKind::Scripted && worker.conn.is_some()).then(|| id.clone())
            })
            .collect()
    }

    fn is_companion(&self, ext_id: &str) -> bool {
        self.map
            .lock()
            .unwrap()
            .get(ext_id)
            .is_some_and(|worker| worker.kind == RuntimeKind::Companion)
    }

    fn detach_companion(&self, ext_id: &str, token: &str) -> bool {
        let mut map = self.map.lock().unwrap();
        let Some(worker) = map.get_mut(ext_id) else {
            return false;
        };
        if worker.kind != RuntimeKind::Companion || worker.token != token {
            return false;
        }
        if let Some(conn) = worker.conn.take() {
            for (_, sender) in conn.pending.lock().unwrap().drain() {
                let _ = sender.send(Err("companion connection closed".into()));
            }
        }
        true
    }

    /// Stop waiting for every host-initiated call to this worker. Session
    /// cancellation uses this to return immediately; late replies are ignored.
    #[cfg(test)]
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
#[derive(Default)]
struct Supervisor {
    generation: u64,
    exists: bool,
    ready: bool,
    queue: Vec<SpawnPayload>,
}

impl Supervisor {
    fn begin(&mut self) -> Option<u64> {
        if self.exists {
            return None;
        }
        self.generation = self
            .generation
            .checked_add(1)
            .expect("supervisor generation exhausted");
        self.exists = true;
        self.ready = false;
        Some(self.generation)
    }

    fn owns(&self, generation: u64) -> bool {
        self.exists && self.generation == generation
    }

    fn accept_ready(&mut self, generation: u64) -> bool {
        if !self.owns(generation) || self.ready {
            return false;
        }
        self.ready = true;
        true
    }

    fn retire(&mut self) -> Option<u64> {
        if !self.exists {
            return None;
        }
        self.exists = false;
        self.ready = false;
        self.queue.clear();
        Some(self.generation)
    }
}

fn supervisor_label(generation: u64) -> String {
    format!("{SUPERVISOR_LABEL_PREFIX}{generation}")
}

#[derive(Deserialize)]
struct SupervisorStatus {
    generation: u64,
    #[serde(default)]
    reason: String,
}

/// The hot-path index. **An installed-but-idle extension must cost the native
/// pipeline nothing**, so the paste path and the event bus consult this — never
/// the registry (which clones every record) and never the disk (which is where
/// manifests live). Rebuilt only when the extension set changes.
#[derive(Default)]
struct Index {
    /// Declared actions, compiled (`docs/Extensions V1/PLAN.md`). Same reasoning
    /// as the layers above: built here so nothing on a felt path reads a
    /// manifest, and gated by the approval digest on every rebuild rather than
    /// once at import.
    actions: grain_core::action_router::ActionIndex,
    /// [GRAIN] The Extension Mode pool (`docs/Extensions V1/PLAN.md` §3.1): the
    /// searchable, recommendation-approved extensions and the aliases the lexical
    /// leg matches. The examples that feed the semantic leg are embedded off this
    /// path — see [`RECOMMEND_VECTORS`] — so what lives here is only what ranking
    /// needs synchronously.
    recommendations: Vec<grain_core::recommend::IndexedRecommendation>,
    /// [GRAIN] Pooled extensions whose author marked them Auto-send eligible
    /// (`autoSend.eligible`, §5). Only the *author* half — the global toggle and
    /// the user's per-extension deny-list are applied at decision time, because
    /// those change without an index rebuild.
    auto_send_eligible: std::collections::HashSet<String>,
    /// [GRAIN] Capability Index V2 (`docs/Extensions 2.0/PLAN.md` §6): the
    /// schema-projection action retriever, built from the same installed
    /// manifests as `actions` above and on the same rebuild trigger — never on a
    /// hot path (§11.2). Pure metadata memory; no worker. Consumed by the Agent
    /// tool loop in a later Phase 2B pass.
    capability: grain_core::capability_index::CapabilityIndex,
}

/// Guards so the common case — no scripted extension enabled — costs exactly
/// one relaxed atomic load on the paste path and per broadcast event. These are
/// free-standing (not behind `HOST`) so the check needs no lock and no
/// `OnceLock` deref.
static HAS_ACTIVATIONS: AtomicBool = AtomicBool::new(false);
static HAS_TRANSFORMS: AtomicBool = AtomicBool::new(false);
/// Same guard for prompt layers: with none installed, a dictation pays one
/// relaxed atomic load and never takes the index lock.
static HAS_PROMPT_LAYERS: AtomicBool = AtomicBool::new(false);
/// And for the `prompt.context` slot. A missing/invalid matching replacement
/// falls back to Grain, but slot state still belongs in the prebuilt index path.
static HAS_CONTEXT_SLOT: AtomicBool = AtomicBool::new(false);
static HAS_MAIN_SLOT: AtomicBool = AtomicBool::new(false);
/// And for declared actions, so a user with none pays one relaxed load.
static HAS_ACTIONS: AtomicBool = AtomicBool::new(false);
/// And for the Extension Mode pool: with nothing searchable installed, the
/// recommendation path is one relaxed load and never touches the index.
static HAS_RECOMMENDATIONS: AtomicBool = AtomicBool::new(false);
/// [GRAIN] Cached example embeddings for the semantic leg of recommendation
/// (`docs/Extensions V1/PLAN.md` §3.1). Held outside [`Index`] and behind its
/// own lock for the same reason the retired calibration was: embedding a pool of
/// examples is seconds of model work, and `refresh_index` runs when the user
/// flips a switch. So the index rebuilds synchronously, this stays empty, and a
/// background task fills it once the vectors are ready — until then
/// recommendation runs in name-only mode, which is honest rather than blocked.
///
/// Keyed by extension id → one vector per declared example. The query is scored
/// against the best of an extension's example vectors at request time.
static RECOMMEND_VECTORS: OnceLock<RwLock<RecommendVectors>> = OnceLock::new();

/// Generation guard so a slow embed for an old pool cannot land on top of a
/// newer one — the same guard the retired calibration used, and for the same
/// race.
static RECOMMEND_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct RecommendVectors {
    generation: u64,
    vectors: HashMap<String, Vec<Vec<f32>>>,
}

fn recommend_vectors() -> &'static RwLock<RecommendVectors> {
    RECOMMEND_VECTORS.get_or_init(|| RwLock::new(RecommendVectors::default()))
}

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
    let Some(host) = HOST.get() else {
        return;
    };
    let mut actions = Vec::new();
    let mut capability_inputs = Vec::new();
    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        for rec in reg.records().into_iter().filter(|record| record.enabled) {
            match load_manifest_result(app, &rec.id) {
                Ok(pack) => {
                    collect_actions(&rec, &pack, &mut actions);
                    collect_capability_actions(&rec, &pack, &mut capability_inputs);
                }
                Err(reason) => {
                    if let Err(error) = reg.quarantine(&rec.id, &reason) {
                        log::error!("[ext:{}] could not persist quarantine: {error}", rec.id);
                    }
                    stop_extension(&rec.id, "incompatible tool-only extension");
                    crate::grain_auth::cancel_extension(&rec.id);
                }
            }
        }
    }
    HAS_ACTIONS.store(!actions.is_empty(), Ordering::Relaxed);
    HAS_RECOMMENDATIONS.store(false, Ordering::Relaxed);
    HAS_ACTIVATIONS.store(false, Ordering::Relaxed);
    HAS_TRANSFORMS.store(false, Ordering::Relaxed);
    HAS_PROMPT_LAYERS.store(false, Ordering::Relaxed);
    HAS_CONTEXT_SLOT.store(false, Ordering::Relaxed);
    HAS_MAIN_SLOT.store(false, Ordering::Relaxed);
    *host.index.write().unwrap() = Index {
        actions: grain_core::action_router::ActionIndex::build(actions),
        capability: grain_core::capability_index::CapabilityIndex::build(capability_inputs),
        ..Index::default()
    };
    // Reconcile old registrations to an empty set; enabling tools creates no
    // event subscription, resident worker, prompt layer or shortcut.
    crate::extension_shortcuts::sync(app);
}

/// Compile one enabled extension's declared prompt layers into the index.
///
/// Two gates, both of which must hold every rebuild rather than once at import:
///
/// 1. **The approved text is the text.** A layer whose wording no longer matches
///    what the user approved is skipped entirely. This is the rug pull —
///    CVE-2025-54136 is exactly "approval of a definition did not survive a
///    later change to it", and the VS Code marketplace shipped the same shape
///    repeatedly through routine version bumps. Grain already holds an extension
///    for a widened *capability*; a prompt layer changes what the model does to
///    the user's words, so it belongs to the same permission surface.
/// 2. **The screen runs again here.** It ran at import too, but import and
///    dictation are different moments and a pack file can be edited in between.
///
/// Both failures are logged and skipped, never repaired: text that has drifted
/// from what the user agreed to is not something to sanitise and use anyway.

/// Compile one enabled extension's declared actions into the index.
///
/// Three gates, and each closes a different hole:
///
/// 1. **Classification** (`docs/Extensions V1/PLAN.md` §2). Only a `searchable`
///    extension may be handed what the user said. Most of the platform is
///    `extending` — prompt layers, settings, App Modes — and without this
///    every one of them competes for "next song". Declared, never inferred: an
///    extension can legitimately be searchable *and* own a shortcut, so
///    "does it declare commands?" is the wrong question.
/// 2. **The recommendation digest.** What it gates is *disclosure*: being
///    pickable means receiving the whole request, so an extension whose
///    recommendation was rewritten since the user read it drops out until they
///    read the new one.
/// 3. **The actions digest**, as before — what an update can quietly change
///    here is not wording but what happens.
fn collect_actions(
    rec: &grain_core::extensions::ExtensionRecord,
    pack: &GrainPack,
    out: &mut Vec<grain_core::action_router::IndexedAction>,
) {
    if !pack.manifest.kind.is_searchable() {
        return;
    }
    let recommend = grain_core::extensions::recommendation_fingerprint(&pack.manifest);
    if rec.recommend_approved.as_deref() != Some(recommend.as_str()) {
        log::warn!(
            "[ext:{}] not searchable — what it is ranked by differs from what was approved; \
             the user must review it again",
            rec.id
        );
        return;
    }
    let declared = &pack.manifest.contributes.actions;
    if declared.is_empty() {
        return;
    }
    let approved = grain_core::extensions::actions_fingerprint(declared);
    if rec.actions_approved.as_deref() != Some(approved.as_str()) {
        log::warn!(
            "[ext:{}] actions not usable — the declaration differs from what was approved; \
             the user must review it again",
            rec.id
        );
        return;
    }
    for decl in declared {
        out.push(grain_core::action_router::IndexedAction::from_decl(
            &rec.id, decl,
        ));
    }
}

/// Project actions into Capability Index V2 only while the exact declaration the
/// user approved is still installed. Unlike V1 routing this intentionally does
/// not require `kind: searchable`: the effective V2 plan makes declared actions
/// Agent contributions for standalone and extending packs too.
fn collect_capability_actions(
    rec: &grain_core::extensions::ExtensionRecord,
    pack: &GrainPack,
    out: &mut Vec<grain_core::capability_index::ActionInput>,
) {
    let declared = &pack.manifest.contributes.actions;
    if declared.is_empty() {
        return;
    }
    let approved = grain_core::extensions::actions_fingerprint(declared);
    if rec.actions_approved.as_deref() != Some(approved.as_str()) {
        log::warn!(
            "[ext:{}] Agent actions withheld because the declaration differs from approval",
            rec.id
        );
        return;
    }
    out.extend(grain_core::capability_index::actions_from_manifest(
        &pack.manifest,
        true,
    ));
}

/// Add one enabled extension to the Extension Mode pool, if it belongs there.
///
/// Two gates, the first two from [`collect_actions`] and for the same reasons —
/// classification (§2) and the recommendation digest (disclosure). It does
/// **not** gate on declaring an action: a translator is a legitimate searchable
/// extension with no command catalogue at all (§3.1), and gating it out here
/// would make the one example the plan uses for "recommend exists even with zero
/// commands" unrankable.
#[cfg(test)]
fn collect_recommendation(
    rec: &grain_core::extensions::ExtensionRecord,
    pack: &GrainPack,
    recommendations: &mut Vec<grain_core::recommend::IndexedRecommendation>,
    examples: &mut Vec<(String, Vec<String>)>,
    auto_send_eligible: &mut std::collections::HashSet<String>,
) {
    if !pack.manifest.kind.is_searchable() {
        return;
    }
    let fingerprint = grain_core::extensions::recommendation_fingerprint(&pack.manifest);
    if rec.recommend_approved.as_deref() != Some(fingerprint.as_str()) {
        // collect_actions already logged the mismatch for a pack that also
        // declares actions; a recommend-only pack would otherwise be silent.
        return;
    }
    let Some(decl) = &pack.manifest.recommend else {
        return;
    };
    recommendations.push(grain_core::recommend::IndexedRecommendation::with_name(
        &rec.id,
        &pack.manifest.name,
        &decl.aliases,
    ));
    examples.push((rec.id.clone(), decl.examples.clone()));
    // Author-declared Auto-send eligibility (§5). The user's deny-list and the
    // global toggle are applied at decision time, not here.
    if pack.manifest.auto_send.as_ref().is_some_and(|a| a.eligible) {
        auto_send_eligible.insert(rec.id.clone());
    }
}

/// The pooled extensions whose author marked them Auto-send eligible (§5). The
/// caller intersects this with the user's deny-list and the global toggle.
pub fn auto_send_eligible() -> std::collections::HashSet<String> {
    if !HAS_RECOMMENDATIONS.load(Ordering::Relaxed) {
        return std::collections::HashSet::new();
    }
    HOST.get()
        .map(|host| host.index.read().unwrap().auto_send_eligible.clone())
        .unwrap_or_default()
}

/// Embed the pool's example phrases and cache the vectors, off the rebuild path.
///
/// Fire-and-forget and generation-guarded, the shape the retired calibration
/// used. Skips entirely when the model is not on disk — that is **name-only
/// mode** (§5), not a failure: the pool still ranks on names, and first use of
/// Extension Mode is where the download is offered. An extension whose examples
/// fail to embed simply has no topical vectors and is reachable by name only.

/// Rank the installed searchable extensions for one spoken request
/// (`docs/Extensions V1/PLAN.md` §3.1). The whole recommendation, minus the
/// hand-off: which extensions, in what order, by name or by topic. With nothing
/// searchable installed this is one relaxed atomic load.
///
/// `excluded` is the decline-and-reopen set (G2): extensions the user already
/// turned down for this request, dropped from the ballot so the reopened chooser
/// cannot offer the same wrong pick again.
///
/// Runs the semantic leg only when the model is on disk *and* the cached vectors
/// belong to the current pool; otherwise it hands `None` to the ranker, which is
/// name-only mode. Blocking on the query embed, so it must not be called from a
/// felt path — the Extension Mode session calls it off the microphone thread.
pub fn recommend(
    spoken: &str,
    excluded: &[String],
) -> (Vec<grain_core::recommend::Recommendation>, bool) {
    if !HAS_RECOMMENDATIONS.load(Ordering::Relaxed) {
        return (Vec::new(), false);
    }
    let Some(host) = HOST.get() else {
        return (Vec::new(), false);
    };
    let index = host.index.read().unwrap();
    let semantic = semantic_scores(spoken);
    let semantic_available = semantic.is_some();
    (
        grain_core::recommend::rank(&index.recommendations, spoken, semantic.as_ref(), excluded),
        semantic_available,
    )
}

/// [GRAIN] Level-1 Agent extension directory (Extensions 2.0 Amendment D).
/// The projection is in-memory and cannot wake a worker or
/// resolve credentials.
pub fn capability_extension_directory() -> Vec<grain_core::capability_index::ExtensionDirectoryEntry>
{
    let Some(host) = HOST.get() else {
        return Vec::new();
    };
    let index = host.index.read().unwrap();
    index.capability.extension_directory().into_iter().collect()
}

pub struct ExtensionActionSet {
    pub actions: Vec<grain_core::capability_index::ActionInput>,
    pub manifest_digest: String,
}

/// [GRAIN] Resolve all currently eligible action records for one exact extension
/// id. Called only after `load_extension`; discovery itself stays code-free.
pub fn capability_actions_for_extension(
    app: &AppHandle,
    extension_id: &str,
) -> Result<ExtensionActionSet, String> {
    let registry = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or_else(|| "extension registry is unavailable".to_string())?;
    let record = registry
        .record(extension_id)
        .filter(|record| record.enabled)
        .ok_or_else(|| "that extension is disabled or no longer installed".to_string())?;
    let pack = load_manifest(app, extension_id)
        .ok_or_else(|| "that extension's manifest is unavailable".to_string())?;
    if !pack.has_runtime() {
        return Err("that extension has no executable runtime".to_string());
    }
    let declared = &pack.manifest.contributes.actions;
    let digest = grain_core::extensions::actions_fingerprint(declared);
    if declared.is_empty() || record.actions_approved.as_deref() != Some(digest.as_str()) {
        return Err("that extension's Agent actions are not currently approved".to_string());
    }

    let Some(host) = HOST.get() else {
        return Err("extension host is unavailable".to_string());
    };
    let index = host.index.read().unwrap();
    let actions: Vec<_> = index
        .capability
        .actions_for_extension(extension_id)
        .into_iter()
        .cloned()
        .collect();
    let expected_ids: std::collections::BTreeSet<String> = declared
        .iter()
        .map(|action| format!("{}:{}", pack.manifest.id, action.id.trim()))
        .collect();
    let indexed_ids: std::collections::BTreeSet<String> = actions
        .iter()
        .map(|action| action.canonical_id.clone())
        .collect();
    if indexed_ids != expected_ids {
        return Err("that extension changed while its Agent tools were loading".to_string());
    }
    Ok(ExtensionActionSet {
        actions,
        manifest_digest: grain_core::extensions::native_call_fingerprint(&record, &pack.manifest)
            .map_err(|_| {
            "that extension's call identity could not be verified".to_string()
        })?,
    })
}

/// [GRAIN] The execution-relevant metadata for one action: its extension id,
/// action id, title, and declared risk. The host executor reads this to classify
/// and prepare a call without re-opening a manifest. `None` for an unknown id.
pub fn capability_action_meta(
    canonical_id: &str,
) -> Option<grain_core::capability_index::ActionInput> {
    let host = HOST.get()?;
    let index = host.index.read().unwrap();
    index.capability.describe(canonical_id).cloned()
}

/// Re-read the installed declaration at execution time and return its approved
/// action digest. This deliberately runs off the retrieval hot path and closes
/// the on-disk edit/update race before a prepared call can execute.
pub fn approved_action_digest(
    app: &AppHandle,
    extension_id: &str,
    action_id: &str,
) -> Option<String> {
    let registry = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()?;
    let record = registry.record(extension_id)?;
    if !record.enabled {
        return None;
    }
    let pack = load_manifest(app, extension_id)?;
    let declared = &pack.manifest.contributes.actions;
    if !declared.iter().any(|action| action.id == action_id) {
        return None;
    }
    let digest = grain_core::extensions::actions_fingerprint(declared);
    if record.actions_approved.as_deref() != Some(digest.as_str()) {
        return None;
    }
    grain_core::extensions::native_call_fingerprint(&record, &pack.manifest).ok()
}

/// The display name and one-line purpose for a pooled extension, for the
/// recommendation event. Reads the manifest off disk, so it is called from the
/// session's off-thread `deliver`, never a felt path.
pub fn recommendation_display(app: &AppHandle, id: &str) -> Option<(String, String)> {
    let pack = load_manifest(app, id)?;
    let purpose = pack
        .manifest
        .recommend
        .as_ref()
        .map(|r| r.purpose.clone())
        .unwrap_or_default();
    Some((pack.manifest.name, purpose))
}

/// How many searchable, approved extensions are in the Extension Mode pool.
///
/// The one fact a surface needs to decide whether Extension Mode is worth
/// offering at all, and whether the embedding-model download is worth its ~130 MB
/// — a pool of zero has nothing to rank, semantic or not. One relaxed atomic
/// load when nothing searchable is installed.
pub fn searchable_count() -> usize {
    if !HAS_RECOMMENDATIONS.load(Ordering::Relaxed) {
        return 0;
    }
    HOST.get()
        .map(|host| host.index.read().unwrap().recommendations.len())
        .unwrap_or(0)
}

/// Every searchable extension id in the pool, in index order.
///
/// The chooser surface lists **all** installed searchable extensions, not only
/// the ranked picks: the recommendations sit highlighted at the top, and the
/// rest are there so the user can always find and choose the right one by hand
/// (or by typing to filter) when ranking withheld it. One relaxed atomic load
/// when nothing searchable is installed.
pub fn searchable_ids() -> Vec<String> {
    if !HAS_RECOMMENDATIONS.load(Ordering::Relaxed) {
        return Vec::new();
    }
    HOST.get()
        .map(|host| {
            host.index
                .read()
                .unwrap()
                .recommendations
                .iter()
                .map(|r| r.extension_id.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The semantic leg: embed the query and score it against each pooled
/// extension's cached example vectors, best example wins.
///
/// `None` — name-only mode — when the model is absent, the cache belongs to an
/// older pool, or the query cannot be embedded. Every one of those degrades to
/// names rather than to a wrong topical guess, which is the safe direction: a
/// missing topical score withholds a recommendation, it never invents one.
///
/// Scores are read straight off the cache rather than off the index, because the
/// generation guard already guarantees the cache belongs to the current pool —
/// an extension that has embedded is in the map, one that has not simply has no
/// topical score, and the ranker treats that as "reachable by name only".
fn semantic_scores(spoken: &str) -> Option<HashMap<String, f32>> {
    if !crate::grain_embed::model_on_disk() {
        return None;
    }
    let cache = recommend_vectors().read().unwrap();
    if cache.generation != RECOMMEND_GENERATION.load(Ordering::SeqCst) || cache.vectors.is_empty() {
        return None;
    }
    let query = crate::grain_embed::embed_query(spoken.to_string()).ok()?;
    let mut scores = HashMap::new();
    for (id, vectors) in &cache.vectors {
        if let Some(best) = vectors.iter().map(|v| cosine(&query, v)).reduce(f32::max) {
            scores.insert(id.clone(), best);
        }
    }
    (!scores.is_empty()).then_some(scores)
}

/// Cosine similarity of two vectors the embedder already L2-normalised, so this
/// is a dot product. Length-guarded rather than trusting both came from the same
/// model — a mismatch scores 0 instead of panicking on a bad index.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Every literal word the installed actions declare, for ASR biasing.
///
/// Deduplicated and unordered — the bias set has its own budget and ordering
/// rules. Placeholders contribute nothing: `{artist}` is the part Grain cannot
/// predict, and that is exactly the part the extension resolves.
pub fn action_vocabulary() -> Vec<String> {
    if !HAS_ACTIONS.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let Some(host) = HOST.get() else {
        return Vec::new();
    };
    let mut terms: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for action in host.index.read().unwrap().actions.actions() {
        for template in &action.templates {
            for part in template {
                if let grain_sdk::manifest::UtterancePart::Literal(literal) = part {
                    for word in literal.split_whitespace() {
                        if word.len() > 2 && seen.insert(word.to_lowercase()) {
                            terms.push(word.to_string());
                        }
                    }
                }
            }
        }
    }
    terms
}

/// What installed extensions bring to this dictation: additive layers in toggle
/// order plus the approved replacements supplied by current slot occupants.
///
/// Called once per finalized transcript, off the paste path. With nothing
/// installed — the overwhelmingly common case — this is one relaxed atomic load
/// and an empty struct: no lock, no disk, no clone of the registry.
/// Migration compatibility shim. Context belongs to the agent/core, and
/// extension metadata can never replace or augment a Grain prompt.
pub fn prompt_contributions(
    _app: &AppHandle,
    _ctx: Option<&crate::context_detect::ActiveContext>,
) -> crate::context_detect::prompt_stack::Contributions {
    crate::context_detect::prompt_stack::Contributions::default()
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
}

#[derive(Clone, Serialize)]
struct KillPayload {
    ext_id: String,
    token: String,
}

#[derive(Deserialize)]
struct DiedPayload {
    ext_id: String,
    token: String,
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

/// Start explicit tool execution support and idle cleanup. No activation or
/// daemon event subscriber is installed. Idempotent.
pub fn start(app: AppHandle, _ctx: Arc<AppContext>) {
    if HOST
        .set(HostState {
            app: app.clone(),
            workers: Workers::new(),
            supervisor: Mutex::new(Supervisor::default()),
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
            if !HOST
                .get()
                .is_some_and(|host| host.workers.owns_token(&p.ext_id, &p.token))
            {
                return;
            }
            let detail = HOST
                .get()
                .and_then(|host| host.workers.dev_source(&p.ext_id))
                .and_then(|source| map_worker_error(&source, &p))
                .or_else(|| p.stack.clone())
                .unwrap_or_else(|| p.reason.clone());
            log::error!("[ext:{}] error {detail}", p.ext_id);
            kill_worker_inner(&p.ext_id, "worker reported death", Some(&p.token), false);
        }
    });

    // Supervisor → host: the page loaded and its listeners are live → flush any
    // spawns queued while it was starting.
    app.listen("ext-host://ready", move |event| {
        if let Ok(status) = serde_json::from_str::<SupervisorStatus>(event.payload()) {
            on_supervisor_ready(status.generation);
        }
    });
    app.listen("ext-host://failed", move |event| {
        if let Ok(status) = serde_json::from_str::<SupervisorStatus>(event.payload()) {
            fail_supervisor(status.generation, &status.reason);
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
#[cfg(test)]
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

#[cfg(test)]
fn declares_transform(activation: &[String]) -> bool {
    activation.iter().any(|a| a == "onTransform")
}

#[cfg(test)]
fn declares_startup(activation: &[String]) -> bool {
    activation.iter().any(|a| a == "onStartup")
}

#[cfg(test)]
fn has_resident_grant(activation: &[String], granted: &[String]) -> bool {
    declares_startup(activation) && has_grant(granted, "resident")
}

#[cfg(test)]
fn has_grant(granted: &[String], capability: &str) -> bool {
    granted.iter().any(|grant| grant == capability)
}

// ── Worker lifecycle ────────────────────────────────────────────────────────

fn is_running(ext_id: &str) -> bool {
    HOST.get()
        .is_some_and(|host| host.workers.is_running(ext_id))
}

fn spawn_worker(
    app: &AppHandle,
    ext_id: &str,
    pack: &GrainPack,
    caps: Vec<String>,
    activation: Option<Value>,
) -> Option<String> {
    if pack.validate_tool_only().is_err() || activation.is_some() {
        log::warn!("[ext:{ext_id}] refused retired extension runtime or activation");
        return None;
    }
    let host = match HOST.get() {
        Some(h) => h,
        None => return None,
    };
    let dev_project = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .and_then(|registry| registry.dev_path(ext_id))
        .and_then(|root| crate::dev_extensions::load_project(&root).ok());
    let record = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()?
        .record(ext_id)
        .filter(|record| record.enabled)?;
    if record.granted != caps {
        return None;
    }
    let call_digest =
        grain_core::extensions::native_call_fingerprint(&record, &pack.manifest).ok()?;
    let companion_launch = if pack.manifest.tier == grain_sdk::Tier::Native {
        if !crate::settings::get_settings(app).extension_developer_mode {
            log::error!("[ext:{ext_id}] deny native companion outside developer mode");
            return None;
        }
        let Some(project) = dev_project.as_ref() else {
            log::error!("[ext:{ext_id}] deny native companion outside load-unpacked project");
            return None;
        };
        let Some(binary) = project.companion_path.clone() else {
            log::error!("[ext:{ext_id}] native project has no current-platform companion");
            return None;
        };
        Some((project.root.clone(), binary))
    } else {
        None
    };
    // Mint a per-worker token bound to exactly the granted caps (SPEC §7.1): the
    // same server-side filter that gates the pill now gates this worker.
    let token = crate::events_server::mint_worker_token(ext_id, caps.iter().cloned().collect());
    // The declaration alone is not authority. Every spawn path (including a
    // shortcut/event that happens to wake this worker) re-checks the grant so a
    // stale/tampered registry cannot turn a denied worker into an immortal one.
    let resident = false;
    let dev_source = dev_project.and_then(|project| {
        project.entry_path.map(|entry| DevSource {
            root: project.root,
            entry,
        })
    });
    let mut sup = host.supervisor.lock().unwrap();
    // A second cold request must not replace a live worker or leak its token.
    if let Some(current) = host.workers.token(ext_id) {
        crate::events_server::revoke_token(&token);
        return Some(current);
    }
    host.workers.insert(
        ext_id,
        Worker {
            token: token.clone(),
            call_digest: Some(call_digest),
            resident,
            memory_strikes: 0,
            kind: if pack.manifest.tier == grain_sdk::Tier::Native {
                RuntimeKind::Companion
            } else {
                RuntimeKind::Scripted
            },
            last_activity: Arc::new(AtomicU64::new(now_secs())),
            conn: None,
            dev_source,
        },
    );
    if let Some((root, binary)) = companion_launch {
        drop(sup);
        log::info!("[ext:{ext_id}] life native companion activation");
        if let Err(error) =
            crate::extension_companion::start(ext_id, &token, root, binary, activation)
        {
            companion_gave_up(
                ext_id,
                &token,
                format!("Native companion could not start: {error}"),
            );
        }
        return Some(token);
    }
    let payload = SpawnPayload {
        ext_id: ext_id.to_string(),
        token: token.clone(),
        entry_source: pack.manifest.entry_source.clone(),
        caps,
    };
    log::info!("[ext:{ext_id}] life worker spawned (resident={resident})");
    let creation = sup.begin();
    if sup.ready {
        let _ = app.emit_to(
            supervisor_label(sup.generation),
            "ext-host://spawn",
            payload,
        );
    } else {
        sup.queue.push(payload);
    }
    drop(sup);
    if let Some(generation) = creation {
        ensure_supervisor(app, generation);
    }
    Some(token)
}

/// Main-thread closures and readiness are scoped to one supervisor generation.
/// Each generation has its own label, so delayed close cannot close a replacement.
fn ensure_supervisor(app: &AppHandle, generation: u64) {
    let app2 = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        let Some(host) = HOST.get() else { return };
        let sup = host.supervisor.lock().unwrap();
        if !sup.owns(generation) {
            return;
        }
        let label = supervisor_label(generation);
        let mut builder =
            WebviewWindowBuilder::new(&app2, &label, WebviewUrl::App(SUPERVISOR_URL.into()))
                .initialization_script(format!(
                    "window.__GRAIN_SUPERVISOR_GENERATION__ = {generation};"
                ))
                .title("Grain Extension Host")
                .inner_size(1.0, 1.0)
                .visible(false)
                .skip_taskbar(true);
        if let Some(data_dir) = crate::portable::data_dir() {
            builder = builder.data_directory(data_dir.join("webview"));
        }
        let result = builder.build();
        if let Ok(window) = &result {
            window.on_window_event(move |event| {
                if matches!(event, tauri::WindowEvent::Destroyed) {
                    fail_supervisor(generation, "supervisor window destroyed");
                }
            });
        }
        drop(sup);
        if let Err(error) = result {
            fail_supervisor(generation, &format!("supervisor creation failed: {error}"));
        }
    }) {
        fail_supervisor(
            generation,
            &format!("supervisor scheduling failed: {error}"),
        );
    }
}

fn on_supervisor_ready(generation: u64) {
    let Some(host) = HOST.get() else { return };
    let mut sup = host.supervisor.lock().unwrap();
    if !sup.accept_ready(generation) {
        return;
    }
    // Keep the gate locked through emission: stop cannot overtake a queued spawn.
    for payload in std::mem::take(&mut sup.queue) {
        if host.workers.owns_token(&payload.ext_id, &payload.token) {
            let _ = host
                .app
                .emit_to(supervisor_label(generation), "ext-host://spawn", payload);
        }
    }
}

fn fail_supervisor(generation: u64, reason: &str) {
    let Some(host) = HOST.get() else { return };
    let victims = {
        let mut sup = host.supervisor.lock().unwrap();
        if !sup.owns(generation) {
            return;
        }
        let victims = host.workers.scripted_tokens();
        sup.retire();
        victims
    };
    log::error!("[GRAIN] ext-host generation {generation}: {reason}");
    for (id, token) in victims {
        kill_worker_inner(&id, "supervisor unavailable", Some(&token), true);
    }
    close_supervisor(generation);
}

/// Terminate a worker: drop its registry entry, revoke its token, tell the
/// supervisor to kill the Web Worker, and fail any in-flight host calls so their
/// awaiters don't hang. Tears down the supervisor when the last worker dies.
fn kill_worker_inner(ext_id: &str, reason: &str, token: Option<&str>, preserve_supervisor: bool) {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    let mut sup = host.supervisor.lock().unwrap();
    let worker = match token {
        Some(token) => host.workers.remove_if_token(ext_id, token),
        None => host.workers.remove(ext_id),
    };
    let worker = match worker {
        Some(w) => w,
        None => return,
    };
    sup.queue
        .retain(|payload| payload.ext_id != ext_id || payload.token != worker.token);
    log::info!("[ext:{ext_id}] life worker reaped ({reason})");
    if sup.exists {
        let _ = host.app.emit_to(
            supervisor_label(sup.generation),
            "ext-host://kill",
            KillPayload {
                ext_id: ext_id.to_string(),
                token: worker.token.clone(),
            },
        );
    }
    let closing = if host.workers.is_empty() && !preserve_supervisor {
        sup.retire()
    } else {
        None
    };
    drop(sup);
    if worker.kind == RuntimeKind::Companion {
        crate::extension_companion::stop(ext_id, reason);
    }
    crate::extension_view::fail_interactive_for_extension(&host.app, ext_id, reason);
    crate::events_server::revoke_token(&worker.token);
    worker.close_pending();
    if let Some(generation) = closing {
        close_supervisor(generation);
    }
}

fn kill_worker(ext_id: &str, reason: &str) {
    kill_worker_inner(ext_id, reason, None, false);
}

/// Public lifecycle hook for registry operations such as unloading a dev
/// override. It is a no-op when the extension has no live worker.
pub fn stop_extension(ext_id: &str, reason: &str) {
    if let Some(host) = HOST.get() {
        crate::extension_view::destroy_for_extension(&host.app, ext_id);
    }
    kill_worker(ext_id, reason);
}

fn close_supervisor(generation: u64) {
    let Some(host) = HOST.get() else { return };
    let app = host.app.clone();
    let label = supervisor_label(generation);
    if let Err(error) = app.clone().run_on_main_thread(move || {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.close();
        }
    }) {
        log::error!("[GRAIN] ext-host generation {generation}: close scheduling failed: {error}");
    }
}

fn reap_idle() {
    let host = match HOST.get() {
        Some(h) => h,
        None => return,
    };
    for (id, token) in host.workers.idle_victims(now_secs(), IDLE_REAP_SECS) {
        if crate::extension_session::is_owned_by(&id) {
            continue;
        }
        kill_worker_inner(&id, "idle timeout", Some(&token), false);
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
    let ids = host.workers.scripted_connected_ids();
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
    if HOST
        .get()
        .is_some_and(|host| host.workers.is_companion(ext_id))
    {
        if let Some(host) = HOST.get() {
            if host.workers.detach_companion(ext_id, token) {
                log::warn!("[ext:{ext_id}] life companion connection closed");
            }
        }
        return;
    }
    kill_worker_inner(ext_id, "connection closed", Some(token), false);
}

/// Route a `HostCallResult` back to its awaiter (the transform/session caller).
pub fn resolve_call_result(ext_id: &str, token: &str, call_id: u64, result: Result<Value, String>) {
    if let Some(host) = HOST.get() {
        host.workers.resolve(ext_id, token, call_id, result);
    }
}

// ── Host-initiated calls + the transform pipeline ────────────────────────────

/// The transform pipeline (SPEC §3.1, §3.3). Runs every enabled `onTransform`
/// extension in **toggle order**, each under a hard 150 ms deadline. A worker
/// that is cold, slow, or errors leaves the text unchanged and takes a strike
/// (3 → auto-disable). An empty-string reply suppresses the paste (the
/// documented output-suppression behavior). Never blocks the paste path on a
/// cold spawn.
pub async fn run_transforms(_app: &AppHandle, text: String) -> String {
    text
}

#[allow(dead_code)] // Retired command compatibility until surface cleanup.
#[derive(Debug, PartialEq, Eq)]
pub enum SessionStageOutput {
    Text(String),
    Handled,
}

#[cfg(test)]
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
pub fn wake_for_session(_app: &AppHandle, _ext_id: &str, _mode: &str) {}

/// Run the owner-controlled slow stage. Failure is deliberately returned to
/// the caller, which falls back to the exact input text.
pub async fn run_session_stage(
    _ext_id: &str,
    _mode: &str,
    _text: &str,
) -> Result<SessionStageOutput, String> {
    Err("Extension recording sessions are retired.".into())
}

// ── Hand-off (`docs/Extensions V1/PLAN.md` §3) ──────────────────────────────
//
// [GRAIN] V1-P0 kept this machinery `allow(dead_code)` through the recommendation
// work; V1-P2 wires it back on. The pivot changed only *who decides* which
// extension is called (the user accepting a recommendation, not a router), and
// *what* it is handed (the full transcript, not extracted spans). Waking a cold
// worker, the deadline pair, and the call/outcome shape are unchanged.

/// Native tool reply deadline; expiry retires the exact worker generation.
const HANDOFF_DEADLINE: Duration = Duration::from_secs(20);

/// How long to wait for a cold worker to connect before giving up.
///
/// The plan budgets ~300 ms for a cold wake; this is deliberately several times
/// that, because the cost of being wrong is a failed request the user has to
/// repeat, while the cost of waiting is a pill that says "working" for another
/// moment.
const HANDOFF_WAKE_DEADLINE: Duration = Duration::from_secs(3);
/// An interactive request should return its finite text outcome quickly; unlike
/// the initial request it has no network-sized interpretation phase.

/// What a handed-off request produced.
#[derive(Debug, PartialEq)]
#[allow(dead_code)] // Legacy wire shape; whole-request execution is retired.
pub enum HandOffOutcome {
    /// The extension handled it. The optional line is a short result to show.
    Done(Option<String>),
    /// The extension is healthy but is not the right owner for this request.
    /// Grain must reopen the chooser with this extension removed rather than
    /// treating a routing correction as an execution failure.
    Declined(String),
    /// It could not, with a reason worth showing.
    Failed(String),
    /// The deadline passed after the call was already in flight.
    ///
    /// **Distinct from `Failed` on purpose.** For anything that leaves the
    /// machine, a timeout does not mean it did not happen — the request may well
    /// have been carried out. Reporting that as failure is a lie, and the one a
    /// user is least able to recover from.
    Unknown,
}

#[cfg(test)]
fn validate_request_reply_shape(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or("extension request replies must be objects")?;
    const ALLOWED: [&str; 3] = ["message", "decline", "error"];
    if let Some(key) = object.keys().find(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(format!("unsupported extension request reply field '{key}'"));
    }
    if object.len() > 1 {
        return Err("extension request reply contains multiple outcomes".into());
    }
    for key in ["message", "decline", "error"] {
        if object.get(key).is_some_and(|value| !value.is_string()) {
            return Err(format!("extension request reply '{key}' must be a string"));
        }
    }
    Ok(())
}

#[cfg(test)]
fn parse_handoff_outcome(value: Value) -> HandOffOutcome {
    if let Err(reason) = validate_request_reply_shape(&value) {
        return HandOffOutcome::Failed(reason);
    }
    if let Some(reason) = value.get("decline").and_then(Value::as_str) {
        return HandOffOutcome::Declined(reason.to_string());
    }
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return HandOffOutcome::Failed(error.to_string());
    }
    HandOffOutcome::Done(
        value
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string),
    )
}

/// Wake the extension the request was handed to, if it is cold.
///
/// Only that one. Warming every searchable extension when the key goes down is
/// the tempting alternative and it violates destroy-if-not-in-use at exactly the
/// scale this feature is built for — twenty installed extensions would mean
/// twenty worker spawns per press. Spawned with no activation payload: the
/// request arrives as an explicit host call, not as a wake event.
fn wake_for_request(app: &AppHandle, ext_id: &str) -> Option<String> {
    if let Some(token) = HOST.get().and_then(|host| host.workers.token(ext_id)) {
        return Some(token);
    }
    let pack = load_manifest(app, ext_id)?;
    let granted = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .and_then(|registry| registry.record(ext_id))
        .map(|record| record.granted)
        .unwrap_or_default();
    spawn_worker(app, ext_id, &pack, granted, None)
}

/// Hand the full request to the extension the user accepted (§3).
///
/// The extension receives the whole transcript, verbatim, and owns what happens
/// next — interpretation, any `match.*` ranking, clarification, the result. Grain
/// does not resolve anything first; that was the old routing model. What it does
/// guarantee is that the extension is still enabled and awake.
pub async fn hand_off(_app: &AppHandle, _ext_id: &str, _request: &str) -> HandOffOutcome {
    HandOffOutcome::Failed(
        "Whole-request extension hand-off is retired. Use a declared tool call.".into(),
    )
}

/// Availability failures precede dispatch; transport failures carry certainty.
#[derive(Debug)]
pub enum ActionCallError {
    Execution(grain_core::execution::ExecutionFailure),
    /// The worker could not be reached or the extension is gone.
    Unavailable(String),
}

fn native_arguments_for_snapshot(
    manifest: &grain_sdk::ExtensionManifest,
    record: &grain_core::extensions::ExtensionRecord,
    action_id: &str,
    arguments: &Value,
    expected_digest: &str,
) -> Result<Value, ActionCallError> {
    if !record.enabled || record.id != manifest.id {
        return Err(ActionCallError::Unavailable(
            "that extension is no longer enabled".into(),
        ));
    }
    let action = manifest
        .contributes
        .actions
        .iter()
        .find(|action| action.id.trim() == action_id)
        .ok_or_else(|| ActionCallError::Unavailable("that tool is no longer declared".into()))?;
    let approved = grain_core::extensions::actions_fingerprint(&manifest.contributes.actions);
    if record.actions_approved.as_deref() != Some(approved.as_str()) {
        return Err(ActionCallError::Unavailable(
            "that tool is no longer approved".into(),
        ));
    }
    let current =
        grain_core::extensions::native_call_fingerprint(record, manifest).map_err(|_| {
            ActionCallError::Unavailable("that tool identity could not be verified".into())
        })?;
    if current != expected_digest {
        return Err(ActionCallError::Unavailable(
            "that tool changed before dispatch".into(),
        ));
    }
    grain_core::capability_agent::validate_native_arguments(action, arguments).map_err(|message| {
        ActionCallError::Execution(grain_core::execution::ExecutionFailure::new(
            grain_core::execution::DispatchPhase::NotDispatched,
            grain_core::execution::FailureClass::InvalidArgument,
            message,
        ))
    })
}

fn approved_native_arguments(
    app: &AppHandle,
    ext_id: &str,
    action_id: &str,
    arguments: &Value,
    expected_digest: &str,
) -> Result<Value, ActionCallError> {
    let pack = load_manifest_result(app, ext_id).map_err(ActionCallError::Unavailable)?;
    pack.validate_tool_only()
        .map_err(ActionCallError::Unavailable)?;
    let record = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .and_then(|registry| registry.record(ext_id))
        .ok_or_else(|| {
            ActionCallError::Unavailable("that extension is no longer installed".into())
        })?;
    native_arguments_for_snapshot(
        &pack.manifest,
        &record,
        action_id,
        arguments,
        expected_digest,
    )
}

struct NativeAdmission<'a> {
    extension_id: &'a str,
    token: &'a str,
    action_id: &'a str,
    arguments: &'a Value,
    expected_digest: &'a str,
    idempotency_key: Option<&'a str>,
}

fn enqueue_native_call(
    registry: &grain_core::extensions::ExtensionsRegistry,
    workers: &Workers,
    manifest: &grain_sdk::ExtensionManifest,
    admission: &NativeAdmission<'_>,
    dispatched: &AtomicBool,
) -> Result<QueuedCall, ActionCallError> {
    registry.with_record_locked(admission.extension_id, |record| {
        let record = record.ok_or_else(|| {
            ActionCallError::Unavailable("that extension is no longer installed".into())
        })?;
        let arguments = native_arguments_for_snapshot(
            manifest,
            record,
            admission.action_id,
            admission.arguments,
            admission.expected_digest,
        )?;
        workers
            .enqueue(
                admission.extension_id,
                Some(admission.token),
                "action",
                json!({"action": admission.action_id, "arguments": arguments,
                "idempotencyKey": admission.idempotency_key}),
                dispatched,
                Some(admission.expected_digest),
            )
            .map_err(|error| ActionCallError::Execution(native_call_failure(&error, false)))
    })
}

fn native_reply_if_current(
    value: Value,
    current: Result<Value, ActionCallError>,
) -> Result<Value, ActionCallError> {
    current.map(|_| value).map_err(|_| {
        ActionCallError::Execution(grain_core::execution::ExecutionFailure::new(
            grain_core::execution::DispatchPhase::ResponseReceived,
            grain_core::execution::FailureClass::Cancelled,
            "The native result belongs to a tool that changed or was disabled. Effects may have occurred.",
        ))
    })
}

/// [GRAIN] Invoke one exact declared action on an extension worker (Extensions 2.0
/// Phase 3b). Unlike [`hand_off`], which gives the worker the whole transcript,
/// this sends the *chosen action id and validated arguments* — the V2 model. The
/// worker's `"action"` handler runs the declared entrypoint and returns a
/// structured result; the executor maps it to an `ActionOutcome`.
///
/// Enablement is re-checked here (time-of-use, §12.7), the cold worker is woken,
/// and queued calls retain dispatch certainty even when their reply is lost.
pub async fn run_action(
    app: &AppHandle,
    ext_id: &str,
    action_id: &str,
    arguments: &Value,
    idempotency_key: Option<&str>,
    expected_digest: &str,
) -> Result<Value, ActionCallError> {
    let arguments = approved_native_arguments(app, ext_id, action_id, arguments, expected_digest)?;
    let Some(host) = HOST.get() else {
        return Err(ActionCallError::Unavailable(
            "extension host unavailable".into(),
        ));
    };
    let token = wake_for_request(app, ext_id)
        .ok_or_else(|| ActionCallError::Unavailable("extension worker unavailable".into()))?;
    let mut owner = NativeCallOwner::new(|| {
        kill_worker_inner(
            ext_id,
            "native request abandoned or failed",
            Some(&token),
            false,
        );
    });
    if !host
        .workers
        .wait_connected(ext_id, &token, HANDOFF_WAKE_DEADLINE)
        .await
    {
        log::warn!("[ext:{ext_id}] action — worker did not start in time");
        kill_worker_inner(ext_id, "worker startup failed", Some(&token), false);
        return Err(ActionCallError::Unavailable(
            "that extension did not start in time".into(),
        ));
    }
    // File validation happens outside the lock. The exact snapshot's identity,
    // approval, arguments and queue admission are then checked under one read.
    let pack = load_manifest_result(app, ext_id).map_err(ActionCallError::Unavailable)?;
    pack.validate_tool_only()
        .map_err(ActionCallError::Unavailable)?;
    let registry = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or_else(|| ActionCallError::Unavailable("extension registry unavailable".into()))?;
    let dispatched = AtomicBool::new(false);
    let queued = enqueue_native_call(
        &registry,
        &host.workers,
        &pack.manifest,
        &NativeAdmission {
            extension_id: ext_id,
            token: &token,
            action_id,
            arguments: &arguments,
            expected_digest,
            idempotency_key,
        },
        &dispatched,
    )?;
    match queued.wait(HANDOFF_DEADLINE).await {
        Ok(value) => {
            let value = native_reply_if_current(
                value,
                approved_native_arguments(app, ext_id, action_id, &arguments, expected_digest),
            )?;
            owner.completed();
            Ok(value)
        }
        Err(error) => Err(ActionCallError::Execution(native_call_failure(
            &error,
            dispatched.load(Ordering::Acquire),
        ))),
    }
}

fn native_call_failure(error: &str, dispatched: bool) -> grain_core::execution::ExecutionFailure {
    use grain_core::execution::{DispatchPhase, ExecutionFailure, FailureClass};
    ExecutionFailure::new(
        if dispatched {
            DispatchPhase::Dispatched
        } else {
            DispatchPhase::NotDispatched
        },
        if error == "deadline exceeded" {
            FailureClass::Network
        } else {
            FailureClass::Internal
        },
        "The native tool did not produce a usable result.",
    )
}

/// User cancellation is immediate: notify the handler's AbortSignal and drop
/// the Rust waiter. The worker may finish later; its response has no recipient.
pub fn cancel_session_stage(_ext_id: &str, _reason: &str) {}

/// How long the host waits for a worker to *acknowledge* a shortcut. The
/// runtime acknowledges on receipt and runs the handler detached, so this
/// covers delivery only — a shortcut that opens an LLM call is not "slow".

/// A contributed shortcut fired (SPEC §3.3). Wakes the extension if it is cold,
/// otherwise hands the press to the running worker.
///
/// Everything happens on the async runtime: the caller is the global-shortcut
/// dispatch path, where blocking hangs every hotkey in the app.
pub fn wake_for_shortcut(_app: &AppHandle, _ext_id: &str, _shortcut_id: &str) {}

/// Whether `id` is an explicitly loaded-unpacked project. Diagnostic call
/// logging uses this cheap registry lookup so installed extensions add no log
/// traffic or formatting work to the host-API path.
pub fn is_dev_extension(app: &AppHandle, id: &str) -> bool {
    app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .and_then(|registry| registry.dev_path(id))
        .is_some()
}

/// Resource-policy disable. Ordinary tool errors/cancellation never use this.
fn auto_disable(app: &AppHandle, ext_id: &str, reason: String) {
    log::warn!("[ext:{ext_id}] life auto-disabled: {reason}");
    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        let _ = reg.set_enabled(ext_id, false);
    }
    refresh_index(app); // drop it from the hot-path index immediately
    crate::extension_view::destroy_for_extension(app, ext_id);
    kill_worker(ext_id, "auto-disabled after repeated resource violations");
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        ctx.emit(DaemonEvent::ExtensionDisabled {
            id: ext_id.to_string(),
            reason,
        });
    }
}

/// The native supervisor exhausted its restart budget. The token guard keeps a
/// stale supervisor from disabling a freshly reloaded replacement generation.
pub fn companion_gave_up(ext_id: &str, token: &str, reason: String) {
    let Some(host) = HOST.get() else {
        return;
    };
    let Some(worker) = host.workers.remove_if_token(ext_id, token) else {
        return;
    };
    crate::events_server::revoke_token(&worker.token);
    if let Some(conn) = worker.conn {
        let _ = conn.out_tx.send(Message::Close(None));
        for (_, sender) in conn.pending.lock().unwrap().drain() {
            let _ = sender.send(Err(reason.clone()));
        }
    }
    log::error!("[ext:{ext_id}] life companion disabled: {reason}");
    if let Some(registry) = host
        .app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
    {
        let _ = registry.set_enabled(ext_id, false);
    }
    crate::extension_view::destroy_for_extension(&host.app, ext_id);
    refresh_index(&host.app);
    if let Some(ctx) = host.app.try_state::<Arc<AppContext>>() {
        ctx.emit(DaemonEvent::ExtensionDisabled {
            id: ext_id.to_string(),
            reason,
        });
    }
}

/// Load the effective extension source. A dev project is re-read from its
/// canonical folder; otherwise this reads the installed `.grainpack.json`.
fn read_pack_file(
    path: &std::path::Path,
    trusted: bool,
    expected_sha256: Option<&str>,
    expected_id: &str,
    expected_version: Option<&str>,
) -> Result<GrainPack, String> {
    use std::io::Read;

    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(grain_sdk::PACK_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > grain_sdk::PACK_MAX_BYTES {
        return Err(format!(
            "extension pack exceeds the {} MiB limit",
            grain_sdk::PACK_MAX_BYTES / (1024 * 1024)
        ));
    }
    if let Some(expected) = expected_sha256 {
        grain_core::trust::verify_artifact(&bytes, expected).map_err(|error| {
            format!("installed extension artifact failed verification: {error}")
        })?;
    }
    let pack: GrainPack = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid extension pack: {error}"))?;
    if pack.manifest.id != expected_id {
        return Err(format!(
            "extension artifact id '{}' does not match installed id '{expected_id}'",
            pack.manifest.id
        ));
    }
    if let Some(version) = expected_version {
        if pack.manifest.version != version {
            return Err(format!(
                "extension artifact version '{}' does not match installed version '{version}'",
                pack.manifest.version
            ));
        }
    }
    if trusted {
        pack.validate_trusted()?;
    } else {
        pack.validate()?;
    }
    Ok(pack)
}

pub fn load_manifest_result(app: &AppHandle, id: &str) -> Result<GrainPack, String> {
    grain_sdk::validate_extension_id(id)?;
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
    let ext_dir = ctx.data_dir.join("extensions");
    let installed_record = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .and_then(|registry| registry.record(id));
    // Legacy single-file location (manual import, seeded built-ins).
    let legacy = ext_dir.join(format!("{id}.grainpack.json"));
    if legacy.exists()
        && installed_record
            .as_ref()
            .is_none_or(|record| record.trust == grain_sdk::Trust::Dev)
    {
        let expected = installed_record
            .as_ref()
            .and_then(|record| record.artifact_sha256.as_deref());
        if installed_record.is_some() && expected.is_none() {
            return Err(format!(
                "extension '{id}' has no artifact hash; reimport it before use"
            ));
        }
        return read_pack_file(
            &legacy,
            false,
            expected,
            id,
            installed_record
                .as_ref()
                .map(|record| record.installed_version.as_str()),
        );
    }
    // [GRAIN] Phase 5B: a store-installed pack lives in its versioned directory
    // `<id>/<version>/pack.grainpack.json` (SPEC §5.2 — atomic, previous-version
    // retained). Resolve it from the record's installed version.
    if let Some(rec) = installed_record {
        if rec.trust == grain_sdk::Trust::Dev {
            return Err(format!("no manually imported pack file for '{id}'"));
        }
        grain_sdk::validate_extension_version(&rec.installed_version)?;
        let versioned = grain_core::install::version_dir(&ext_dir, id, &rec.installed_version)
            .join("pack.grainpack.json");
        if versioned.exists() {
            let expected = rec.artifact_sha256.as_deref();
            if expected.is_none() && rec.trust != grain_sdk::Trust::Dev {
                return Err(format!(
                    "verified extension '{id}' has no artifact hash; reinstall it before use"
                ));
            }
            return read_pack_file(
                &versioned,
                true,
                expected,
                id,
                Some(rec.installed_version.as_str()),
            );
        }
    }
    Err(format!("no pack file for '{id}'"))
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
    reg.install(grain_core::extensions::ExtensionRecord {
        id: id.to_string(),
        enabled,
        toggle_seq: prior.toggle_seq,
        installed_version: loaded.pack.manifest.version.clone(),
        artifact_sha256: None,
        granted: granted.clone(),
        slots: loaded.pack.manifest.slots.clone(),
        // A hot reload re-approves, for the same reason the initial dev load
        // does: this is the author's own project on their own disk, and the
        // prompt text is the thing they are iterating on.
        prompt_layers_approved: (!loaded.pack.manifest.contributes.prompt_layers.is_empty()).then(
            || {
                grain_core::extensions::prompt_layers_fingerprint(
                    &loaded.pack.manifest.contributes.prompt_layers,
                )
            },
        ),
        // Same reasoning for actions, and the same limit: this shortcut exists
        // only for a load-unpacked project on the author's own disk.
        actions_approved: (!loaded.pack.manifest.contributes.actions.is_empty()).then(|| {
            grain_core::extensions::actions_fingerprint(&loaded.pack.manifest.contributes.actions)
        }),
        authentication_approved: loaded
            .pack
            .manifest
            .contributes
            .authentication
            .as_ref()
            .map(grain_core::extensions::authentication_fingerprint),
        // And for what the extension is ranked by. Same shortcut, same limit.
        recommend_approved: loaded
            .pack
            .manifest
            .kind
            .is_searchable()
            .then(|| grain_core::extensions::recommendation_fingerprint(&loaded.pack.manifest)),
        dev: prior.dev,
        // A dev hot-reload preserves the record's rung (a load-unpacked project
        // is `dev`); trust is never changed by a reload.
        trust: prior.trust,
    })
    .map_err(|error| error.to_string())?;

    let had_worker = is_running(id);
    log::info!("[ext:{id}] life developer reload");
    if had_worker {
        kill_worker_inner(id, "developer hot reload", None, true);
    }
    refresh_index(app);
    if had_worker && enabled && !is_running(id) {
        let _ = spawn_worker(app, id, &loaded.pack, granted, None);
    }
    let worker_count = HOST.get().map(|host| host.workers.len()).unwrap_or(0);
    Ok(grain_sdk::DevReloadResult {
        restarted_worker: had_worker && enabled,
        enabled,
        worker_count,
        token_count: crate::events_server::token_count(),
    })
}

// ── Retired built-ins ───────────────────────────────────────────────────────

/// Packs Grain used to seed into every install and no longer ships.
///
/// `grain.auto-categorize` was a dogfood sample proving the scripted runtime end
/// to end — worker spawn on an event, `llm` + `storage` host calls, capability
/// enforcement, the idle reaper. It did its job, but it was seeded on every
/// launch, so it sat under "Installed · not active" in everyone's list forever,
/// as a demo nobody asked for.
const RETIRED_BUILTINS: &[&str] = &["grain.auto-categorize", "grain.agent-center-layout"];

/// Disable legacy bundled integrations while preserving artifacts and user data.
/// A fresh install never receives these packages.
fn retire_builtin_packs(app: &AppHandle) -> Result<(), String> {
    let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() else {
        return Ok(());
    };
    for id in RETIRED_BUILTINS {
        if reg.dev_path(id).is_none() && reg.is_installed(id) {
            reg.quarantine(id, "Retired bundled extension; tools only are supported.")
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Bring the installed set in line with what this build actually ships. Called
/// once after `AppContext` + `ExtensionsRegistry` are managed.
///
/// Grain used to SEED scripted packs from inside the binary here. It no longer
/// does: first-party extensions are real packs in the catalogue, installed the
/// same way anyone else's are, so this only has to clean up after the ones that
/// were seeded into existing installs.
pub fn reconcile_builtin_packs(app: &AppHandle) -> Result<(), String> {
    // Runs before HOST starts. Validate disabled records too so legacy artifacts
    // retain a persistent refusal reason and old grants cannot survive restart.
    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        for record in reg.records() {
            if let Err(reason) = load_manifest_result(app, &record.id) {
                reg.quarantine(&record.id, &reason)
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        grain_core::extensions::archive_retired_prompts(&ctx.data_dir, &ctx.settings())
            .map_err(|error| format!("preserve retired extension prompts: {error}"))?;
        grain_core::extensions::archive_retired_bindings(&ctx.data_dir, &ctx.settings())
            .map_err(|error| format!("preserve retired extension bindings: {error}"))?;
        ctx.update_settings(|settings| {
            grain_core::extensions::retire_extension_prompts(settings);
            grain_core::extensions::retire_extension_bindings(settings);
        })
        .map_err(|error| error.to_string())?;
    }
    retire_builtin_packs(app)?;
    if let Some(reg) = app.try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>() {
        reg.finish_tool_only_migration()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
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
        let startup = vec!["onStartup".to_string()];
        assert!(!has_resident_grant(&startup, &granted));
        assert!(has_resident_grant(&startup, &["resident".to_string()]));
    }

    /// The whole point of the index: with nothing enabled, the paste path and
    /// the event bus must be a single atomic load — no registry, no disk.
    #[test]
    fn hot_paths_are_guarded_off_by_default() {
        assert!(!HAS_TRANSFORMS.load(Ordering::Relaxed));
        assert!(!HAS_ACTIVATIONS.load(Ordering::Relaxed));
    }

    #[test]
    fn installed_pack_hash_is_rechecked_before_runtime_load() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pack.grainpack.json");
        let bytes = br#"{"manifest":{"id":"com.x.hash","name":"Hash","version":"1","tier":"scripted","entry_source":"x"}}"#;
        std::fs::write(&path, bytes).unwrap();
        let hash = grain_core::trust::sha256_hex(bytes);
        assert!(read_pack_file(&path, false, Some(&hash), "com.x.hash", Some("1")).is_ok());

        let mut tampered = bytes.to_vec();
        tampered.push(b' ');
        std::fs::write(&path, tampered).unwrap();
        assert!(read_pack_file(&path, false, Some(&hash), "com.x.hash", Some("1")).is_err());
    }

    // ── The Extension Mode pool (`docs/Extensions V1/PLAN.md` §2) ───────────

    fn pack_of(extra: &str) -> GrainPack {
        let json = format!(
            r#"{{"manifest":{{"id":"com.x.p","name":"P","version":"1.0",
                 "tier":"scripted","entry_source":"export default {{}}",
                 "contributes":{{"actions":[{{"id":"next","title":"Skip",
                   "risk":"safe","utterances":["next song"]}}]}}{extra}}}}}"#
        );
        serde_json::from_str(&json).expect("fixture parses")
    }

    fn record_for(pack: &GrainPack) -> grain_core::extensions::ExtensionRecord {
        grain_core::extensions::ExtensionRecord {
            id: "com.x.p".into(),
            enabled: true,
            toggle_seq: 1,
            installed_version: "1.0".into(),
            artifact_sha256: None,
            granted: vec![],
            prompt_layers_approved: None,
            actions_approved: Some(grain_core::extensions::actions_fingerprint(
                &pack.manifest.contributes.actions,
            )),
            authentication_approved: None,
            recommend_approved: pack
                .manifest
                .kind
                .is_searchable()
                .then(|| grain_core::extensions::recommendation_fingerprint(&pack.manifest)),
            slots: vec![],
            dev: None,
            trust: grain_sdk::Trust::UNTRUSTED_DEFAULT,
        }
    }

    const SEARCHABLE: &str = r#","kind":"searchable","recommend":{"purpose":"Play music",
        "examples":["next song","pause the music"]}"#;

    /// Most of the platform is `extending` — prompt layers, settings, App
    /// Modes. Without the classification gate every one of them would compete
    /// for "next song" against the extension that actually plays music.
    #[test]
    fn an_extending_extension_never_enters_the_pool() {
        let pack = pack_of("");
        assert!(!pack.manifest.kind.is_searchable(), "extending by default");
        let mut out = Vec::new();
        collect_actions(&record_for(&pack), &pack, &mut out);
        assert!(out.is_empty(), "commands are its own business, not Grain's");
    }

    #[test]
    fn a_searchable_extension_enters_the_pool() {
        let pack = pack_of(SEARCHABLE);
        let mut out = Vec::new();
        collect_actions(&record_for(&pack), &pack, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].extension_id, "com.x.p");
    }

    /// The rug pull, at the level that matters most: being pickable means
    /// receiving everything the user said. An extension that rewrote what it
    /// asks to be offered for drops out until the user reads the new version.
    #[test]
    fn a_rewritten_recommendation_drops_out_of_the_pool() {
        let pack = pack_of(SEARCHABLE);
        let mut stale = record_for(&pack);
        stale.recommend_approved = Some("the-fingerprint-of-a-version-the-user-read".into());
        let mut out = Vec::new();
        collect_actions(&stale, &pack, &mut out);
        assert!(out.is_empty());
    }

    fn pool_of(pack: &GrainPack, rec: &grain_core::extensions::ExtensionRecord) -> Vec<String> {
        let mut recommendations = Vec::new();
        let mut examples = Vec::new();
        let mut eligible = std::collections::HashSet::new();
        collect_recommendation(
            rec,
            pack,
            &mut recommendations,
            &mut examples,
            &mut eligible,
        );
        recommendations
            .into_iter()
            .map(|r| r.extension_id)
            .collect()
    }

    #[test]
    fn an_extending_extension_is_not_in_the_recommendation_pool() {
        let pack = pack_of("");
        assert!(pool_of(&pack, &record_for(&pack)).is_empty());
    }

    #[test]
    fn a_searchable_extension_is_in_the_recommendation_pool() {
        let pack = pack_of(SEARCHABLE);
        assert_eq!(pool_of(&pack, &record_for(&pack)), vec!["com.x.p"]);
    }

    /// The translator case (§3.1): a searchable extension with no command
    /// catalogue at all is still rankable — `recommend` exists even with zero
    /// actions, and gating the pool on actions would make it unreachable.
    #[test]
    fn a_searchable_extension_with_no_actions_is_still_pooled() {
        let json = r#"{"manifest":{"id":"com.x.t","name":"T","version":"1.0",
            "tier":"scripted","entry_source":"export default {}",
            "kind":"searchable","recommend":{"purpose":"Translate text",
              "examples":["translate this to french","how do you say hello in spanish"]}}}"#;
        let pack: GrainPack = serde_json::from_str(json).expect("fixture parses");
        assert!(pack.manifest.contributes.actions.is_empty());
        let mut rec = record_for(&pack);
        rec.id = "com.x.t".into();
        let mut recommendations = Vec::new();
        let mut examples = Vec::new();
        let mut eligible = std::collections::HashSet::new();
        collect_recommendation(
            &rec,
            &pack,
            &mut recommendations,
            &mut examples,
            &mut eligible,
        );
        assert_eq!(
            recommendations.len(),
            1,
            "a translator is a real searchable extension"
        );
        assert_eq!(
            examples[0].1.len(),
            2,
            "its examples are carried for embedding"
        );
    }

    #[test]
    fn a_rewritten_recommendation_drops_out_of_the_recommendation_pool() {
        let pack = pack_of(SEARCHABLE);
        let mut stale = record_for(&pack);
        stale.recommend_approved = Some("a-version-the-user-never-read".into());
        assert!(pool_of(&pack, &stale).is_empty());
    }

    #[test]
    fn a_handoff_reply_distinguishes_done_decline_error_and_bare() {
        // An extension's onRequest reply: `{message}` and a bare `{}` are both
        // "handled"; only an explicit `{error}` is a failure. A timeout is
        // Unknown and is decided by the caller, never parsed from a reply.
        assert_eq!(
            parse_handoff_outcome(json!({ "message": "opened your dashboard" })),
            HandOffOutcome::Done(Some("opened your dashboard".into()))
        );
        assert_eq!(parse_handoff_outcome(json!({})), HandOffOutcome::Done(None));
        assert_eq!(
            parse_handoff_outcome(json!({ "decline": "not a music request" })),
            HandOffOutcome::Declined("not a music request".into())
        );
        assert_eq!(
            parse_handoff_outcome(json!({ "error": "no such site" })),
            HandOffOutcome::Failed("no such site".into())
        );
    }

    #[test]
    fn visual_replies_are_rejected_even_when_the_tree_looks_valid() {
        let reply = json!({
            "view": {
                "version": 1,
                "title": "Review issue",
                "root": { "type": "text", "text": "Ready" },
                "actions": []
            }
        });
        assert!(matches!(
            parse_handoff_outcome(reply),
            HandOffOutcome::Failed(_)
        ));
        assert!(matches!(
            parse_handoff_outcome(json!({
                "view": {
                    "version": 1,
                    "title": "Forged",
                    "root": { "type": "html", "markup": "<script />" }
                }
            })),
            HandOffOutcome::Failed(_)
        ));
        assert!(matches!(
            parse_handoff_outcome(json!({ "message": "done", "error": "also failed" })),
            HandOffOutcome::Failed(_)
        ));
        assert!(matches!(
            parse_handoff_outcome(json!({ "html": "<button>forged</button>" })),
            HandOffOutcome::Failed(_)
        ));
    }

    fn worker(last_activity: u64, resident: bool) -> Worker {
        Worker {
            token: "tok".into(),
            call_digest: None,
            resident,
            memory_strikes: 0,
            kind: RuntimeKind::Scripted,
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
                workers.resolve("com.x.a", "tok", call_id, Ok(json!({ "text": "HI" })));
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
    fn native_queue_failures_preserve_dispatch_certainty_without_replay() {
        use grain_core::execution::{ActionOutcome, RiskClass, SideEffect};
        rt().block_on(async {
            for mode in [
                "absent", "stale", "closed", "cancel", "error", "timeout", "invalid", "ok",
            ] {
                let workers = Workers::new();
                let (out_tx, mut out_rx) = mpsc::unbounded_channel();
                if mode != "absent" {
                    workers.insert("tools", worker(now_secs(), false));
                    workers.attach("tools", "tok", out_tx).unwrap();
                }
                if mode == "closed" {
                    out_rx.close();
                }
                let prepared = crate::action_exec::prepare(
                    "tools:write",
                    "tools",
                    "write",
                    "Tools",
                    json!({}),
                    RiskClass::Confirm,
                    SideEffect::Write,
                    "current",
                );
                let dispatched = AtomicBool::new(false);
                let token = if mode == "stale" { "old" } else { "tok" };
                let call = workers.call_tracked(
                    "tools",
                    Some(token),
                    "action",
                    json!({}),
                    Duration::from_millis(20),
                    &dispatched,
                );
                let before = matches!(mode, "absent" | "stale" | "closed");
                let mut count = 0;
                let result = if before {
                    call.await
                } else {
                    let handler = async {
                        let Message::Text(frame) = out_rx.recv().await.expect("one queued call")
                        else {
                            panic!("not a call")
                        };
                        let frame: HostFrame = serde_json::from_str(&frame).unwrap();
                        let HostFrame::Call(call) = frame else {
                            panic!("not a call")
                        };
                        count += 1; // Simulated effect, before teardown or a lost reply.
                        match mode {
                            "cancel" => workers.cancel_pending("tools", "disabled after queue"),
                            "error" => workers.resolve(
                                "tools",
                                "tok",
                                call.call_id,
                                Err("token=secret-provider-payload".into()),
                            ),
                            "invalid" => workers.resolve(
                                "tools",
                                "tok",
                                call.call_id,
                                Ok(json!(["invalid"])),
                            ),
                            "ok" => workers.resolve(
                                "tools",
                                "tok",
                                call.call_id,
                                Ok(json!({"ok": "recorded"})),
                            ),
                            "timeout" => {}
                            _ => unreachable!(),
                        }
                    };
                    tokio::join!(call, handler).0
                };
                assert_eq!(dispatched.load(Ordering::Acquire), !before, "{mode}");
                let outcome = crate::action_exec::native_outcome(
                    result.map_err(|error| {
                        ActionCallError::Execution(native_call_failure(
                            &error,
                            dispatched.load(Ordering::Acquire),
                        ))
                    }),
                    &prepared,
                );
                match mode {
                    "absent" | "stale" | "closed" => {
                        assert!(matches!(outcome, ActionOutcome::Failed { .. }))
                    }
                    "cancel" | "error" | "timeout" => {
                        assert!(matches!(outcome, ActionOutcome::UnknownOutcome { .. }))
                    }
                    "invalid" => {
                        assert!(matches!(outcome, ActionOutcome::ResultUnavailable { .. }))
                    }
                    "ok" => assert!(matches!(outcome, ActionOutcome::Succeeded(_))),
                    _ => unreachable!(),
                }
                assert_eq!(count, if before { 0 } else { 1 });
                assert!(out_rx.try_recv().is_err(), "no replay");
                assert!(!outcome.model_summary().contains("secret-provider"));
                if let Some(worker) = workers.map.lock().unwrap().get("tools") {
                    assert!(worker
                        .conn
                        .as_ref()
                        .unwrap()
                        .pending
                        .lock()
                        .unwrap()
                        .is_empty());
                };
            }
        });
    }

    #[test]
    fn native_snapshot_rejects_invalid_arguments_and_changed_approval_before_queue() {
        rt().block_on(async {
            let mut pack = pack_of("");
            pack.manifest.contributes.actions[0].params = serde_json::from_value(json!([
                {"name": "title", "kind": "text"},
                {"name": "priority", "kind": "number", "required": false}
            ]))
            .unwrap();
            let record = record_for(&pack);
            let digest =
                grain_core::extensions::native_call_fingerprint(&record, &pack.manifest).unwrap();
            let workers = Workers::new();
            workers.insert(&record.id, worker(now_secs(), false));
            let (out_tx, mut out_rx) = mpsc::unbounded_channel();
            workers.attach(&record.id, "tok", out_tx).unwrap();
            for arguments in [
                json!({}),
                json!([]),
                json!({"title": 7}),
                json!({"title": " "}),
                json!({"title": "Hi", "priority": "3"}),
                json!({"title": "Hi", "fixture-private-key": true}),
                json!({"title": "x".repeat(64 * 1024)}),
            ] {
                let error = native_arguments_for_snapshot(
                    &pack.manifest,
                    &record,
                    "next",
                    &arguments,
                    &digest,
                )
                .unwrap_err();
                let ActionCallError::Execution(failure) = error else {
                    panic!("invalid input expected")
                };
                assert_eq!(
                    failure.phase,
                    grain_core::execution::DispatchPhase::NotDispatched
                );
                assert_eq!(
                    failure.class,
                    grain_core::execution::FailureClass::InvalidArgument
                );
                assert!(!failure.message.contains("fixture-private-key"));
                assert!(out_rx.try_recv().is_err());
            }
            let arguments = json!({"title": "Hello", "priority": null});
            let normalized =
                native_arguments_for_snapshot(&pack.manifest, &record, "next", &arguments, &digest)
                    .unwrap();
            assert_eq!(normalized, json!({"title": "Hello"}));
            for mode in [
                "disabled",
                "reenabled",
                "unapproved",
                "source",
                "params",
                "missing",
            ] {
                let mut record = record.clone();
                let mut changed = pack.manifest.clone();
                match mode {
                    "disabled" => record.enabled = false,
                    "reenabled" => record.toggle_seq += 1,
                    "unapproved" => record.actions_approved = None,
                    "source" => changed.entry_source.push_str("// changed"),
                    "params" => changed.contributes.actions[0].params.clear(),
                    "missing" => changed.contributes.actions.clear(),
                    _ => unreachable!(),
                }
                assert!(
                    native_arguments_for_snapshot(&changed, &record, "next", &normalized, &digest)
                        .is_err(),
                    "{mode}"
                );
                assert!(out_rx.try_recv().is_err());
            }
            let dispatched = AtomicBool::new(false);
            let call = workers.call_tracked(
                &record.id,
                Some("tok"),
                "action",
                normalized.clone(),
                Duration::from_secs(1),
                &dispatched,
            );
            let handler = async {
                let Message::Text(frame) = out_rx.recv().await.unwrap() else {
                    panic!("call expected")
                };
                let HostFrame::Call(call) = serde_json::from_str(&frame).unwrap() else {
                    panic!("call expected")
                };
                assert_eq!(call.params, normalized);
                workers.resolve(&record.id, "tok", call.call_id, Ok(json!({"ok": "hello"})));
            };
            assert!(tokio::join!(call, handler).0.is_ok());
            assert!(dispatched.load(Ordering::Acquire));
            assert!(out_rx.try_recv().is_err());
        });
    }

    #[test]
    fn native_admission_orders_registry_disable_and_worker_replacement_with_queue() {
        rt().block_on(async {
            for mode in [
                "disabled",
                "reenabled",
                "replaced",
                "source_worker",
                "missing",
                "invalid",
                "after_queue",
                "after_reply",
                "valid",
            ] {
                let directory = tempfile::tempdir().unwrap();
                let registry =
                    grain_core::extensions::ExtensionsRegistry::load(directory.path(), false)
                        .unwrap();
                let pack = pack_of("");
                let mut record = record_for(&pack);
                record.enabled = false;
                registry.install(record).unwrap();
                registry.set_enabled(&pack.manifest.id, true).unwrap();
                let digest = grain_core::extensions::native_call_fingerprint(
                    &registry.record(&pack.manifest.id).unwrap(),
                    &pack.manifest,
                )
                .unwrap();
                let workers = Workers::new();
                let mut spawned = worker(now_secs(), false);
                spawned.call_digest = Some(if mode == "source_worker" {
                    "old-source".into()
                } else {
                    digest.clone()
                });
                workers.insert(&pack.manifest.id, spawned);
                let (out_tx, mut out_rx) = mpsc::unbounded_channel();
                workers.attach(&pack.manifest.id, "tok", out_tx).unwrap();
                match mode {
                    "disabled" => {
                        registry.set_enabled(&pack.manifest.id, false).unwrap();
                    }
                    "reenabled" => {
                        registry.set_enabled(&pack.manifest.id, false).unwrap();
                        registry.set_enabled(&pack.manifest.id, true).unwrap();
                    }
                    "replaced" => {
                        let mut replacement = worker(now_secs(), false);
                        replacement.token = "replacement".into();
                        workers.insert(&pack.manifest.id, replacement);
                    }
                    "missing" => {
                        registry.uninstall(&pack.manifest.id).unwrap();
                    }
                    _ => {}
                }
                let arguments = if mode == "invalid" {
                    json!({"undeclared": true})
                } else {
                    json!({})
                };
                let admission = NativeAdmission {
                    extension_id: &pack.manifest.id,
                    token: "tok",
                    action_id: "next",
                    arguments: &arguments,
                    expected_digest: &digest,
                    idempotency_key: None,
                };
                let dispatched = AtomicBool::new(false);
                let queued = enqueue_native_call(
                    &registry,
                    &workers,
                    &pack.manifest,
                    &admission,
                    &dispatched,
                );
                if !matches!(mode, "after_queue" | "after_reply" | "valid") {
                    assert!(queued.is_err(), "{mode}");
                    assert!(!dispatched.load(Ordering::Acquire));
                    assert!(out_rx.try_recv().is_err(), "zero frames: {mode}");
                    continue;
                }
                let queued = queued.unwrap();
                assert!(dispatched.load(Ordering::Acquire));
                let Message::Text(frame) = out_rx.try_recv().unwrap() else {
                    panic!("one call")
                };
                let HostFrame::Call(call) = serde_json::from_str(&frame).unwrap() else {
                    panic!("call")
                };
                assert_eq!(call.params["action"], "next");
                if mode == "after_queue" {
                    // Admission has released its lock; disable completes while
                    // the reply waiter remains alive, then exact-token teardown.
                    registry.set_enabled(&pack.manifest.id, false).unwrap();
                    workers
                        .remove_if_token(&pack.manifest.id, "tok")
                        .unwrap()
                        .close_pending();
                    let error = queued.wait(Duration::from_secs(1)).await.unwrap_err();
                    let failure = native_call_failure(&error, dispatched.load(Ordering::Acquire));
                    assert!(matches!(
                        failure.into_outcome(),
                        grain_core::execution::ActionOutcome::UnknownOutcome { .. }
                    ));
                    assert!(matches!(out_rx.try_recv(), Ok(Message::Close(_))));
                } else {
                    workers.resolve(
                        &pack.manifest.id,
                        "tok",
                        call.call_id,
                        Ok(json!({"ok": "hello"})),
                    );
                    let reply = queued.wait(Duration::from_secs(1)).await.unwrap();
                    if mode == "after_reply" {
                        registry.set_enabled(&pack.manifest.id, false).unwrap();
                        let current = registry.with_record_locked(&pack.manifest.id, |record| {
                            native_arguments_for_snapshot(
                                &pack.manifest,
                                record.unwrap(),
                                "next",
                                &arguments,
                                &digest,
                            )
                        });
                        let failure = native_reply_if_current(reply, current).unwrap_err();
                        let prepared = crate::action_exec::prepare(
                            "com.x.p:next",
                            "com.x.p",
                            "next",
                            "P",
                            json!({}),
                            grain_core::execution::RiskClass::Confirm,
                            grain_core::execution::SideEffect::Write,
                            &digest,
                        );
                        assert!(matches!(
                            crate::action_exec::native_outcome(Err(failure), &prepared),
                            grain_core::execution::ActionOutcome::ResultUnavailable { .. }
                        ));
                    } else {
                        assert!(native_reply_if_current(reply, Ok(json!({}))).is_ok());
                    }
                }
                assert!(out_rx.try_recv().is_err(), "no replay");
            }
        });
    }

    #[test]
    fn native_request_owner_drop_retires_only_its_generation_and_releases_pending_calls() {
        rt().block_on(async {
            for mode in [
                "startup",
                "cancel",
                "timeout",
                "transport_error",
                "replacement",
                "success",
            ] {
                let workers = Workers::new();
                workers.insert("tools", worker(now_secs(), false));
                let (out_tx, mut out_rx) = mpsc::unbounded_channel();
                workers.attach("tools", "tok", out_tx).unwrap();
                let mut owner = NativeCallOwner::new(|| {
                    if let Some(worker) = workers.remove_if_token("tools", "tok") {
                        worker.close_pending();
                    }
                });
                let dispatched = AtomicBool::new(false);
                if mode == "startup" {
                    drop(owner);
                    assert_eq!(workers.len(), 0);
                    assert!(matches!(out_rx.try_recv(), Ok(Message::Close(_))));
                    continue;
                }
                let queued = workers
                    .enqueue("tools", Some("tok"), "action", json!({}), &dispatched, None)
                    .unwrap();
                let pending = queued.pending.calls.clone();
                let Message::Text(frame) = out_rx.try_recv().unwrap() else {
                    panic!("one call")
                };
                let HostFrame::Call(call) = serde_json::from_str(&frame).unwrap() else {
                    panic!("call")
                };
                match mode {
                    "cancel" | "replacement" => {
                        if mode == "replacement" {
                            let mut replacement = worker(now_secs(), false);
                            replacement.token = "new".into();
                            workers.insert("tools", replacement);
                        }
                        drop(queued);
                    }
                    "timeout" | "transport_error" => {
                        if mode == "transport_error" {
                            workers.resolve(
                                "tools",
                                "tok",
                                call.call_id,
                                Err("private error".into()),
                            );
                        }
                        let error = queued.wait(Duration::from_millis(10)).await.unwrap_err();
                        assert!(matches!(
                            native_call_failure(&error, true).into_outcome(),
                            grain_core::execution::ActionOutcome::UnknownOutcome { .. }
                        ));
                    }
                    "success" => {
                        workers.resolve("tools", "tok", call.call_id, Ok(json!({"ok": "hello"})));
                        assert!(queued.wait(Duration::from_secs(1)).await.is_ok());
                        owner.completed();
                    }
                    _ => unreachable!(),
                }
                drop(owner);
                assert!(pending.lock().unwrap().is_empty());
                if mode == "replacement" {
                    assert!(workers.owns_token("tools", "new"));
                } else if mode == "success" {
                    assert!(workers.owns_token("tools", "tok"));
                } else {
                    assert_eq!(workers.len(), 0);
                    assert!(matches!(out_rx.try_recv(), Ok(Message::Close(_))));
                }
                assert!(out_rx.try_recv().is_err(), "no replay");
            }
        });
    }

    #[test]
    fn aborting_native_waiter_runs_cleanup_without_retiring_a_replacement() {
        rt().block_on(async {
            for replace in [false, true] {
                let workers = Arc::new(Workers::new());
                workers.insert("tools", worker(now_secs(), false));
                let (out_tx, mut out_rx) = mpsc::unbounded_channel();
                workers.attach("tools", "tok", out_tx).unwrap();
                let owned = workers.clone();
                let task = tokio::spawn(async move {
                    let mut owner = NativeCallOwner::new(|| {
                        if let Some(worker) = owned.remove_if_token("tools", "tok") {
                            worker.close_pending();
                        }
                    });
                    let dispatched = AtomicBool::new(false);
                    let queued = owned
                        .enqueue("tools", Some("tok"), "action", json!({}), &dispatched, None)
                        .unwrap();
                    let result = queued.wait(Duration::from_secs(20)).await;
                    if result.is_ok() {
                        owner.completed();
                    }
                    result
                });
                assert!(matches!(out_rx.recv().await, Some(Message::Text(_))));
                let pending = workers
                    .map
                    .lock()
                    .unwrap()
                    .get("tools")
                    .unwrap()
                    .conn
                    .as_ref()
                    .unwrap()
                    .pending
                    .clone();
                assert_eq!(pending.lock().unwrap().len(), 1);
                if replace {
                    let mut replacement = worker(now_secs(), false);
                    replacement.token = "replacement".into();
                    workers.insert("tools", replacement);
                }
                task.abort();
                assert!(task.await.unwrap_err().is_cancelled());
                assert!(pending.lock().unwrap().is_empty());
                if replace {
                    assert!(workers.owns_token("tools", "replacement"));
                } else {
                    assert_eq!(workers.len(), 0);
                    assert!(matches!(out_rx.try_recv(), Ok(Message::Close(_))));
                }
                assert!(out_rx.try_recv().is_err(), "no replay");
            }
        });
    }

    #[test]
    fn resolve_unknown_call_is_a_noop() {
        let workers = Workers::new();
        workers.resolve("nobody", "tok", 7, Ok(Value::Null)); // must not panic
    }

    #[test]
    fn reaper_picks_only_stale_free_nonresident_workers() {
        let workers = Workers::new();
        workers.insert("stale", worker(0, false)); // ancient
        workers.insert("fresh", worker(now_secs(), false)); // just active
        workers.insert("resident", worker(0, true)); // never reaped
        let victims = workers.idle_victims(now_secs(), IDLE_REAP_SECS);
        assert_eq!(victims, vec![("stale".to_string(), "tok".to_string())]);
    }

    #[test]
    fn heap_samples_are_typed_and_memory_strikes_reset() {
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
        workers.clear_memory_strikes("leak");
        assert_eq!(workers.record_memory_strike("leak"), Some(1));
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
    fn companion_disconnect_keeps_identity_for_supervised_restart() {
        let workers = Workers::new();
        let mut companion = worker(now_secs(), false);
        companion.kind = RuntimeKind::Companion;
        workers.insert("native", companion);
        let (out_tx, _out_rx) = mpsc::unbounded_channel::<Message>();
        workers.attach("native", "tok", out_tx).unwrap();
        assert!(workers.scripted_connected_ids().is_empty());
        assert!(workers.detach_companion("native", "tok"));
        assert_eq!(workers.len(), 1);

        let (replacement_tx, _replacement_rx) = mpsc::unbounded_channel::<Message>();
        assert!(workers.attach("native", "tok", replacement_tx).is_some());
    }

    #[test]
    fn stale_failure_and_queued_spawn_cannot_own_a_replacement_worker() {
        let workers = Workers::new();
        let mut old = worker(0, false);
        old.token = "old".into();
        workers.insert("tools", old);
        assert!(workers.owns_token("tools", "old"));
        let mut replacement = worker(1, false);
        replacement.token = "new".into();
        workers.insert("tools", replacement);
        assert!(!workers.owns_token("tools", "old"));
        assert!(workers.owns_token("tools", "new"));
        assert!(workers.remove_if_token("tools", "old").is_none());
        assert_eq!(workers.len(), 1);
    }

    #[test]
    fn replaced_worker_cannot_receive_a_waiting_or_prepared_call() {
        rt().block_on(async {
            let workers = Workers::new();
            let mut replacement = worker(now_secs(), false);
            replacement.token = "new".into();
            workers.insert("tools", replacement);
            let (out_tx, mut out_rx) = mpsc::unbounded_channel();
            workers.attach("tools", "new", out_tx).unwrap();
            assert!(
                !workers
                    .wait_connected("tools", "old", Duration::from_secs(5))
                    .await
            );
            assert_eq!(
                workers
                    .call_owned(
                        "tools",
                        Some("old"),
                        "action",
                        json!({}),
                        Duration::from_secs(5)
                    )
                    .await
                    .unwrap_err(),
                "worker generation changed"
            );
            assert!(out_rx.try_recv().is_err());
            assert!(workers.map.lock().unwrap()["tools"]
                .conn
                .as_ref()
                .unwrap()
                .pending
                .lock()
                .unwrap()
                .is_empty());
        });
    }

    #[test]
    fn stale_socket_reply_cannot_resolve_same_call_id_in_replacement() {
        rt().block_on(async {
            let workers = Workers::new();
            let mut replacement = worker(now_secs(), false);
            replacement.token = "new".into();
            workers.insert("tools", replacement);
            let (out_tx, mut out_rx) = mpsc::unbounded_channel();
            workers.attach("tools", "new", out_tx).unwrap();
            let responder = async {
                let Message::Text(frame) = out_rx.recv().await.unwrap() else {
                    panic!("call frame")
                };
                let HostFrame::Call(call) = serde_json::from_str(&frame).unwrap() else {
                    panic!("call frame")
                };
                workers.resolve("tools", "old", call.call_id, Ok(json!("forged")));
                assert_eq!(
                    workers.map.lock().unwrap()["tools"]
                        .conn
                        .as_ref()
                        .unwrap()
                        .pending
                        .lock()
                        .unwrap()
                        .len(),
                    1
                );
                workers.resolve("tools", "new", call.call_id, Ok(json!("correct")));
            };
            let (result, _) = tokio::join!(
                workers.call_owned(
                    "tools",
                    Some("new"),
                    "action",
                    json!({}),
                    Duration::from_secs(2)
                ),
                responder
            );
            assert_eq!(result.unwrap(), json!("correct"));
        });
    }

    #[test]
    fn dropping_call_future_releases_pending_entry_without_waiting_for_deadline() {
        rt().block_on(async {
            let workers = Workers::new();
            workers.insert("tools", worker(now_secs(), false));
            let (out_tx, mut out_rx) = mpsc::unbounded_channel();
            workers.attach("tools", "tok", out_tx).unwrap();
            let mut call = Box::pin(workers.call_owned(
                "tools",
                Some("tok"),
                "action",
                json!({}),
                Duration::from_secs(60),
            ));
            tokio::select! {
                frame = out_rx.recv() => { assert!(frame.is_some()); }
                result = &mut call => { panic!("call returned prematurely: {result:?}"); }
            }
            let pending = workers.map.lock().unwrap()["tools"]
                .conn
                .as_ref()
                .unwrap()
                .pending
                .clone();
            assert_eq!(pending.lock().unwrap().len(), 1);
            drop(call);
            assert!(pending.lock().unwrap().is_empty());
        });
    }

    #[test]
    fn closed_worker_channel_releases_pending_entry() {
        rt().block_on(async {
            let workers = Workers::new();
            workers.insert("tools", worker(now_secs(), false));
            let (out_tx, out_rx) = mpsc::unbounded_channel();
            workers.attach("tools", "tok", out_tx).unwrap();
            drop(out_rx);
            assert_eq!(
                workers
                    .call_owned(
                        "tools",
                        Some("tok"),
                        "action",
                        json!({}),
                        Duration::from_secs(60)
                    )
                    .await
                    .unwrap_err(),
                "worker channel closed"
            );
            assert!(workers.map.lock().unwrap()["tools"]
                .conn
                .as_ref()
                .unwrap()
                .pending
                .lock()
                .unwrap()
                .is_empty());
        });
    }

    #[test]
    fn supervisor_restart_refuses_late_ready_and_keeps_new_queue() {
        let mut supervisor = Supervisor::default();
        assert!(!supervisor.accept_ready(0));
        let old = supervisor.begin().unwrap();
        assert!(supervisor.begin().is_none());
        supervisor.queue.push(SpawnPayload {
            ext_id: "tools".into(),
            token: "old".into(),
            entry_source: "old source".into(),
            caps: vec![],
        });
        assert_eq!(supervisor.retire(), Some(old));
        assert!(supervisor.queue.is_empty());
        let new = supervisor.begin().unwrap();
        supervisor.queue.push(SpawnPayload {
            ext_id: "tools".into(),
            token: "new".into(),
            entry_source: "new source".into(),
            caps: vec![],
        });
        assert_ne!(supervisor_label(old), supervisor_label(new));
        assert!(!supervisor.owns(old)); // delayed creation/failure cannot own new window
        assert!(!supervisor.accept_ready(old));
        assert!(!supervisor.ready);
        assert_eq!(supervisor.queue[0].token, "new");
        assert!(supervisor.accept_ready(new));
        assert!(!supervisor.accept_ready(new)); // duplicate ready cannot reflush
        assert_eq!(supervisor.retire(), Some(new));
        assert!(!supervisor.accept_ready(new));
        assert!(supervisor.retire().is_none());
    }

    #[test]
    fn supervisor_failure_snapshot_excludes_companions_and_cannot_remove_replacement() {
        let workers = Workers::new();
        let mut scripted = worker(now_secs(), false);
        scripted.token = "old".into();
        workers.insert("tools", scripted);
        let mut companion = worker(now_secs(), false);
        companion.kind = RuntimeKind::Companion;
        workers.insert("native", companion);
        let victims = workers.scripted_tokens();
        assert_eq!(victims, vec![("tools".into(), "old".into())]);
        workers.remove("tools");
        let mut replacement = worker(now_secs(), false);
        replacement.token = "new".into();
        workers.insert("tools", replacement);
        for (id, token) in victims {
            assert!(workers.remove_if_token(&id, &token).is_none());
        }
        assert!(workers.owns_token("tools", "new"));
        assert!(workers.is_running("native"));
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
                token: "fixture".into(),
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
