//! [GRAIN] Shared on-device embeddings for Agent and extension retrieval.

//! Opt-in BGE-small-en-v1.5 weights live in the shared Hugging Face cache.
//! The tokenizer and Candle BERT model load on demand on a dedicated thread.
//! Agent panels and a bounded retrieval TTL govern residency; idle teardown
//! joins the thread and releases its weights.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use hf_hub::api::tokio::{ApiBuilder, CancellationToken, Progress};
use hf_hub::{Cache, Repo, RepoType};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const MODEL_REPO: &str = "BAAI/bge-small-en-v1.5";
pub const MODEL_REVISION: &str = "main";
/// Everything the engine needs. `model.safetensors` is the f32 export
/// (33.4M params × 4 B ≈ 130 MB on disk). Loaded at full precision, always:
/// half precision halved the resident weights but candle's f16 CPU matmul can
/// silently produce all-zero/NaN embeddings, and an embedding that is quietly
/// wrong is worse than one that costs 60 MB more.
const MODEL_FILES: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];
pub const EMBED_DIM: usize = 384;
const MAX_TOKENS: usize = 512;

pub const MODEL_PROGRESS_EVENT: &str = "grain-embed-model-progress";
pub const MODEL_COMPLETE_EVENT: &str = "grain-embed-model-complete";
pub const MODEL_CANCELLED_EVENT: &str = "grain-embed-model-cancelled";
pub const MODEL_ERROR_EVENT: &str = "grain-embed-model-error";

// -- model files on disk --------------------------------------------------------

/// Resolve one model file in the shared HF cache (never downloads).
fn cached_file(filename: &str) -> Option<PathBuf> {
    Cache::from_env()
        .repo(Repo::with_revision(
            MODEL_REPO.to_string(),
            RepoType::Model,
            MODEL_REVISION.to_string(),
        ))
        .get(filename)
}

/// True when every model file is present in the shared HF cache.
pub fn model_on_disk() -> bool {
    MODEL_FILES.iter().all(|f| cached_file(f).is_some())
}

// -- download (consent-gated by the frontend) -----------------------------------

/// Live download cancellation token; `None` when no download is running.
static DOWNLOAD: Mutex<Option<CancellationToken>> = Mutex::new(None);

pub fn is_downloading() -> bool {
    DOWNLOAD.lock().unwrap().is_some()
}

pub fn cancel_download() {
    if let Some(token) = DOWNLOAD.lock().unwrap().as_ref() {
        token.cancel();
    }
}

#[derive(Clone, Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct GrainEmbedModelProgress {
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

#[derive(Clone, Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct GrainEmbedModelComplete;

#[derive(Clone, Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct GrainEmbedModelCancelled;

#[derive(Clone, Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct GrainEmbedModelError(pub String);

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum EmbedModelStatus {
    Ready,
    Downloading,
    Absent,
}

#[tauri::command]
#[specta::specta]
pub fn grain_embed_model_status() -> EmbedModelStatus {
    if is_downloading() {
        EmbedModelStatus::Downloading
    } else if model_on_disk() {
        EmbedModelStatus::Ready
    } else {
        EmbedModelStatus::Absent
    }
}

#[tauri::command]
#[specta::specta]
pub async fn grain_embed_download_model(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    crate::grain_commands::require_main_window(&window)?;
    download_model(app).await
}

#[tauri::command]
#[specta::specta]
pub fn grain_embed_cancel_download(window: tauri::WebviewWindow) -> Result<(), String> {
    crate::grain_commands::require_main_window(&window)?;
    cancel_download();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn grain_embed_uninstall_model(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    crate::grain_commands::require_main_window(&window)?;
    tauri::async_runtime::spawn_blocking(uninstall_model)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| format!("{error:#}"))?;
    crate::extension_host::refresh_index(&app);
    Ok(())
}

/// Bridges hf-hub progress to the settings UI. Only the big
/// `model.safetensors` transfer reports (config + tokenizer are ~1 KB / ~700 KB
/// — invisible next to the weights).
#[derive(Clone)]
struct EmbedDownloadProgress {
    app: AppHandle,
    state: Arc<Mutex<(u64, u64, std::time::Instant)>>, // (downloaded, total, last_emit)
}

impl EmbedDownloadProgress {
    fn emit(&self, downloaded: u64, total: u64) {
        let percentage = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let _ = self.app.emit(
            MODEL_PROGRESS_EVENT,
            &GrainEmbedModelProgress {
                downloaded,
                total,
                percentage,
            },
        );
    }
}

impl Progress for EmbedDownloadProgress {
    async fn init(&mut self, size: usize, _filename: &str) {
        {
            let mut st = self.state.lock().unwrap();
            *st = (0, size as u64, std::time::Instant::now());
        }
        self.emit(0, size as u64);
    }

    async fn update(&mut self, size: usize) {
        let (downloaded, total, emit) = {
            let mut st = self.state.lock().unwrap();
            st.0 = st.0.saturating_add(size as u64);
            let now = std::time::Instant::now();
            // ~10 updates/sec, plus always the final byte.
            let emit = now.duration_since(st.2) >= std::time::Duration::from_millis(100)
                || (st.1 > 0 && st.0 >= st.1);
            if emit {
                st.2 = now;
            }
            (st.0, st.1, emit)
        };
        if emit {
            self.emit(downloaded, total);
        }
    }

    async fn finish(&mut self) {}
}

/// Download the model files into the shared HF cache with progress + cancel.
/// Resumable: hf-hub keeps `.part` files, and files already cached are skipped.
/// Emits `MODEL_COMPLETE_EVENT` / `MODEL_ERROR_EVENT` after cache verification.
pub async fn download_model(app: AppHandle) -> Result<(), String> {
    if model_on_disk() {
        crate::extension_host::refresh_index(&app);
        let _ = app.emit(MODEL_COMPLETE_EVENT, ());
        return Ok(());
    }

    let token = CancellationToken::new();
    {
        let mut slot = DOWNLOAD.lock().unwrap();
        if slot.is_some() {
            return Err("model download already running".to_string());
        }
        *slot = Some(token.clone());
    }

    let result = download_files(&app, token).await;

    // Release the reservation only after transfer cleanup has finished.
    DOWNLOAD.lock().unwrap().take();

    match result {
        Ok(true) => {
            crate::extension_host::refresh_index(&app);
            let _ = app.emit(MODEL_COMPLETE_EVENT, ());
            Ok(())
        }
        Ok(false) => {
            let _ = app.emit(MODEL_CANCELLED_EVENT, ());
            Ok(())
        }
        Err(e) => {
            let msg = format!("{e:#}");
            let _ = app.emit(MODEL_ERROR_EVENT, &msg);
            Err(msg)
        }
    }
}

/// Returns Ok(false) when the user cancelled.
async fn download_files(app: &AppHandle, token: CancellationToken) -> Result<bool> {
    let api = ApiBuilder::from_env()
        .with_progress(false)
        .with_max_files(4)
        .build()
        .context("init Hugging Face API")?;
    let repo = api.repo(Repo::with_revision(
        MODEL_REPO.to_string(),
        RepoType::Model,
        MODEL_REVISION.to_string(),
    ));

    for filename in MODEL_FILES {
        if cached_file(filename).is_some() {
            continue;
        }
        let progress = EmbedDownloadProgress {
            app: app.clone(),
            state: Arc::new(Mutex::new((0, 0, std::time::Instant::now()))),
        };
        match repo
            .download_with_progress_cancellable(filename, progress, token.clone())
            .await
        {
            Ok(_) => {}
            Err(hf_hub::api::tokio::ApiError::Cancelled) => {
                log::info!("[GRAIN] embed model download cancelled at {filename}");
                return Ok(false);
            }
            Err(e) => return Err(anyhow!("download {filename} failed: {e}")),
        }
    }
    log::info!("[GRAIN] embed model downloaded ({MODEL_REPO})");
    Ok(true)
}

// -- engine ----------------------------------------------------------------------

enum Request {
    Embed {
        texts: Vec<String>,
        reply: mpsc::Sender<Result<Vec<Vec<f32>>>>,
    },
}

/// Handle to the engine thread. Dropping it closes the channel; the worker
/// then falls out of its loop and every weight/tokenizer allocation is freed.
struct Engine {
    tx: Option<mpsc::Sender<Request>>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.tx.take(); // close the channel → worker exits
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        log::info!("[GRAIN] embed engine dropped (weights freed)");
    }
}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

/// Drop the engine (thread joined, weights freed). No-op when not running.
pub fn shutdown_engine() {
    *ENGINE.lock().unwrap() = None;
}

/// Uninstall the model from the shared HF cache (R4 — reclaim the ~130 MB).
/// Drops the engine first: `model.safetensors` is mmap'd, and an mmap'd file
/// can't be deleted on Windows. Removes the whole `models--…` repo dir
/// (snapshots + blobs). A no-op when the files are already gone.
pub fn uninstall_model() -> Result<()> {
    // Serialize cache removal with download admission, including cancellation
    // cleanup, so a second transfer cannot race a removal.
    let downloading = DOWNLOAD.lock().unwrap();
    if downloading.is_some() {
        return Err(anyhow!("A model download is in progress."));
    }
    let mut engine = ENGINE.lock().unwrap();
    *engine = None;
    // Derive the cache repo dir from any present file:
    // `<cache>/models--BAAI--bge-small-en-v1.5/snapshots/<rev>/<file>` → up 3.
    let repo_dir = MODEL_FILES
        .iter()
        .find_map(|f| cached_file(f).and_then(|p| p.ancestors().nth(3).map(PathBuf::from)));
    if let Some(dir) = repo_dir {
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("remove model dir {}", dir.display()))?;
            log::info!("[GRAIN] embed model uninstalled ({})", dir.display());
        }
    }
    Ok(())
}

pub fn shutdown_engine_if_idle(app: &AppHandle) {
    use tauri::Manager;
    let panel_open = app.get_webview_window(crate::agent::PANEL_LABEL).is_some();
    if !panel_open && !extension_mode_warm() {
        shutdown_engine();
    }
}

// ── Extension Mode witness (`docs/Extensions V1/PLAN.md` §6) ─────────────────
//
// This is the whole of §6's "Grain owns the embedder lifecycle": an extension
// may declare `needs: ["semantic"]`, but it can never pin the model, set the
// TTL, or keep it alive by polling. The one knob it gets is the honest
// declaration; the arithmetic lives here.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// How long the embedder is held warm after Extension Mode last touched it.
///
/// Long enough that the recommendation, an immediate accept, and a first
/// `match.semantic` from the chosen extension all reuse one load; short enough
/// that an idle Extension-Mode session does not sit on ~130 MB. Not
/// author-settable (§6).
pub const EXTENSION_MODE_TTL: Duration = Duration::from_secs(30);

/// Unix-millis deadline the Extension-Mode witness holds the engine to; `0` =
/// not warm.
static EXTENSION_MODE_WARM_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
/// One reaper at a time.
static REAPER_RUNNING: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// True while Extension Mode is holding the embedder warm.
pub fn extension_mode_warm() -> bool {
    now_ms() < EXTENSION_MODE_WARM_UNTIL_MS.load(Ordering::Relaxed)
}

/// Extension Mode is using — or about to use — the embedder: hold it warm for
/// [`EXTENSION_MODE_TTL`] and spawn it now if the model is present, so the query
/// does not pay the load. Idempotent and cheap; call it on the Extension-Mode
/// keypress (§6, "warm if any searchable installed") and on every semantic use
/// to refresh the TTL.
///
/// A no-op when the model is absent — name-only mode has nothing to warm, and
/// setting a deadline would only schedule a reaper with nothing to reclaim.
pub fn touch_extension_mode(app: &AppHandle) {
    if !model_on_disk() {
        return;
    }
    EXTENSION_MODE_WARM_UNTIL_MS.store(
        now_ms() + EXTENSION_MODE_TTL.as_millis() as u64,
        Ordering::Relaxed,
    );
    ensure_spawned();
    spawn_reaper(app.clone());
}

/// Spawn the engine now if the model is on disk and it is not already resident,
/// WITHOUT embedding anything — the proactive-warm half of [`touch_extension_mode`].
/// [`embed`] still spawns lazily on its own, so this is an optimisation, never a
/// correctness requirement.
fn ensure_spawned() {
    if !model_on_disk() {
        return;
    }
    let mut slot = ENGINE.lock().unwrap();
    if slot.is_none() {
        match spawn_engine() {
            Ok(engine) => *slot = Some(engine),
            Err(error) => log::warn!("[GRAIN] embed: proactive warm failed: {error:#}"),
        }
    }
}

/// Ensure a single background task is watching the TTL, and reclaims the engine
/// when it lapses. Re-reads the deadline each wake, so a `touch` that extends the
/// warmth mid-sleep is honoured rather than fought. The reclaim goes through
/// [`shutdown_engine_if_idle`], so an Agent panel still using the
/// engine keeps it — Extension Mode only ever withdraws its own witness.
fn spawn_reaper(app: AppHandle) {
    if REAPER_RUNNING.swap(true, Ordering::SeqCst) {
        return; // one reaper already watching
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let remaining = EXTENSION_MODE_WARM_UNTIL_MS
                .load(Ordering::Relaxed)
                .saturating_sub(now_ms());
            if remaining == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(remaining)).await;
        }
        REAPER_RUNNING.store(false, Ordering::SeqCst);
        // A `touch` that raced in after the loop broke but before the flag
        // cleared is self-healing: `embed` re-spawns lazily, and the next touch
        // starts a fresh reaper. So an early reclaim costs one reload at worst.
        shutdown_engine_if_idle(&app);
    });
}

/// BGE v1.5's retrieval instruction. ASYMMETRIC by design: the model card
/// recommends prefixing short QUERIES with this when retrieving passages —
/// documents are embedded bare. Applying it only at query time means stored
/// document vectors never need re-embedding to benefit.
pub const QUERY_INSTRUCTION: &str = "Represent this sentence for searching relevant passages: ";

/// Embed one QUERY (instruction-prefixed; see [`QUERY_INSTRUCTION`]). Every
/// query-side embedding must come through here so query and document vectors stay
/// in the model's intended asymmetric geometry. Blocking, like [`embed`].
pub fn embed_query(text: String) -> Result<Vec<f32>> {
    let mut prefixed = String::with_capacity(QUERY_INSTRUCTION.len() + text.len());
    prefixed.push_str(QUERY_INSTRUCTION);
    prefixed.push_str(&text);
    embed(vec![prefixed])?
        .pop()
        .ok_or_else(|| anyhow!("empty query embedding"))
}

/// Embed a batch of texts (mean-pooled, L2-normalized, `EMBED_DIM` floats
/// each), lazily spawning the engine thread on first use. Blocking — call from
/// `spawn_blocking`. Fails fast when the model files are not on disk.
pub fn embed(texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let reply_rx = {
        let mut slot = ENGINE.lock().unwrap();
        if slot.is_none() {
            *slot = Some(spawn_engine()?);
        }
        let engine = slot.as_ref().unwrap();
        let (reply_tx, reply_rx) = mpsc::channel();
        engine
            .tx
            .as_ref()
            .expect("engine channel alive while handle exists")
            .send(Request::Embed {
                texts,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("embed engine thread is gone"))?;
        reply_rx
        // ENGINE lock released here — the worker replies without holding it.
    };
    reply_rx
        .recv()
        .map_err(|_| anyhow!("embed engine dropped mid-request"))?
}

fn spawn_engine() -> Result<Engine> {
    let config = cached_file("config.json").ok_or_else(|| anyhow!("model not downloaded"))?;
    let tokenizer = cached_file("tokenizer.json").ok_or_else(|| anyhow!("model not downloaded"))?;
    let weights =
        cached_file("model.safetensors").ok_or_else(|| anyhow!("model not downloaded"))?;

    let (tx, rx) = mpsc::channel::<Request>();
    let join = std::thread::Builder::new()
        .name("grain-embed".to_string())
        .spawn(move || worker(config, tokenizer, weights, rx))
        .context("spawn embed engine thread")?;
    log::info!("[GRAIN] embed engine spawned");
    Ok(Engine {
        tx: Some(tx),
        join: Some(join),
    })
}

/// Engine thread body: load once, serve until the channel closes. A load
/// failure answers every queued/future request with the error instead of
/// wedging callers.
fn worker(config: PathBuf, tokenizer: PathBuf, weights: PathBuf, rx: mpsc::Receiver<Request>) {
    let loaded = load_model(&config, &tokenizer, &weights);
    match loaded {
        Ok((tokenizer, model, device)) => {
            for req in rx {
                let Request::Embed { texts, reply } = req;
                let result = embed_batch(&tokenizer, &model, &device, &texts);
                let _ = reply.send(result);
            }
        }
        Err(e) => {
            log::error!("[GRAIN] embed model load failed: {e:#}");
            let msg = format!("{e:#}");
            for req in rx {
                let Request::Embed { reply, .. } = req;
                let _ = reply.send(Err(anyhow!(msg.clone())));
            }
        }
    }
}

fn load_model(
    config_path: &PathBuf,
    tokenizer_path: &PathBuf,
    weights_path: &PathBuf,
) -> Result<(
    tokenizers::Tokenizer,
    candle_transformers::models::bert::BertModel,
    candle_core::Device,
)> {
    use candle_core::Device;
    use candle_nn::VarBuilder;
    use candle_transformers::models::bert::{BertModel, Config, DTYPE};

    let start = std::time::Instant::now();
    let device = Device::Cpu;

    let config: Config = serde_json::from_str(
        &std::fs::read_to_string(config_path).context("read model config.json")?,
    )
    .context("parse model config.json")?;

    let mut tokenizer =
        tokenizers::Tokenizer::from_file(tokenizer_path).map_err(|e| anyhow!("tokenizer: {e}"))?;
    tokenizer
        .with_truncation(Some(tokenizers::TruncationParams {
            max_length: MAX_TOKENS,
            ..Default::default()
        }))
        .map_err(|e| anyhow!("tokenizer truncation: {e}"))?;

    // mmap keeps the resident cost close to the pages actually touched.
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&[weights_path.clone()], DTYPE, &device)
            .context("map model.safetensors")?
    };
    let model = BertModel::load(vb, &config).context("build BERT graph")?;
    log::info!(
        "[GRAIN] embed model loaded in {} ms",
        start.elapsed().as_millis()
    );
    Ok((tokenizer, model, device))
}

/// One text at a time (no padding logic needed; batches are capped and each
/// forward is a few ms on CPU). Mean-pool over the sequence, L2-normalize so
/// L2 distance is monotonic with cosine similarity.
fn embed_batch(
    tokenizer: &tokenizers::Tokenizer,
    model: &candle_transformers::models::bert::BertModel,
    device: &candle_core::Device,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    use candle_core::Tensor;

    let mut out = Vec::with_capacity(texts.len());
    for text in texts {
        let encoding = tokenizer
            .encode(text.as_str(), true)
            .map_err(|e| anyhow!("tokenize: {e}"))?;
        let ids = encoding.get_ids();
        if ids.is_empty() {
            out.push(vec![0.0; EMBED_DIM]);
            continue;
        }
        let seq_len = ids.len();
        let input_ids = Tensor::new(ids, device)?.unsqueeze(0)?;
        let type_ids = Tensor::new(encoding.get_type_ids(), device)?.unsqueeze(0)?;
        // No padding → no mask needed (every position is a real token).
        let hidden = model.forward(&input_ids, &type_ids, None)?; // [1, seq, dim]
                                                                  // Pool/normalize in f32 so precision isn't lost precision here and
                                                                  // the stored vector is always f32 (the vec index dtype).
        let hidden = hidden.to_dtype(candle_core::DType::F32)?;
        let pooled = (hidden.sum(1)? / seq_len as f64)?; // [1, dim]
        let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?;
        let normalized = pooled.broadcast_div(&norm)?;
        let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
        // Reject poison at the source: a non-finite (NaN/Inf) or all-zero
        // embedding means the forward pass was corrupt (e.g. a half-loaded /
        // mmap-raced model). Reject it before it can poison retrieval scores.
        if !vec.iter().all(|x| x.is_finite()) || vec.iter().all(|&x| x == 0.0) {
            return Err(anyhow!(
                "embed produced non-finite/zero vector (model forward corrupt?)"
            ));
        }
        out.push(vec);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    #[test]
    fn cancellation_reserves_download_until_transfer_cleanup() {
        let token = super::CancellationToken::new();
        {
            let mut slot = super::DOWNLOAD.lock().unwrap();
            assert!(slot.is_none());
            *slot = Some(token.clone());
        }
        super::cancel_download();
        assert!(token.is_cancelled());
        assert!(super::is_downloading());
        assert!(super::uninstall_model().is_err());
        super::DOWNLOAD.lock().unwrap().take();
        assert!(!super::is_downloading());
    }

    /// The Extension-Mode witness `shutdown_engine_if_idle` consults (§6): a past
    /// or zero deadline reads as not-warm, a future one as warm. This is the
    /// arithmetic that keeps the shared engine alive for the TTL and no longer.
    #[test]
    fn extension_mode_witness_tracks_its_ttl() {
        use super::{extension_mode_warm, now_ms, EXTENSION_MODE_WARM_UNTIL_MS};
        EXTENSION_MODE_WARM_UNTIL_MS.store(0, Ordering::Relaxed);
        assert!(!extension_mode_warm(), "the default is not-warm");
        EXTENSION_MODE_WARM_UNTIL_MS.store(now_ms() + 10_000, Ordering::Relaxed);
        assert!(extension_mode_warm(), "a future deadline holds the engine");
        EXTENSION_MODE_WARM_UNTIL_MS.store(now_ms().saturating_sub(1), Ordering::Relaxed);
        assert!(!extension_mode_warm(), "a lapsed TTL releases it");
        EXTENSION_MODE_WARM_UNTIL_MS.store(0, Ordering::Relaxed);
    }

    /// Verify asymmetric query embeddings rank relevant documents first. Prints the
    /// prefixed-query cosine against related and unrelated documents so the floor
    /// can be tuned against real model output, and asserts the asymmetric
    /// geometry actually separates them. Skips itself if the model isn't on
    /// disk. Run with `--nocapture` to read the values.
    #[test]
    fn query_prefix_separates_related_from_unrelated() {
        if !super::model_on_disk() {
            println!("model not on disk; skipped");
            return;
        }
        let docs = vec![
            "Wifi password. The home network details. the wifi password is interstellar, router is in the hallway".to_string(),
            "Dentist. Appointment note. dentist appointment moved to the 14th at 3pm".to_string(),
            "Pasta recipe. Dinner idea. carbonara: guanciale, pecorino, eggs, no cream ever".to_string(),
        ];
        let doc_vecs = super::embed(docs).expect("doc embed");
        let q =
            super::embed_query("what was my wifi password again".to_string()).expect("query embed");
        let cos = |a: &[f32], b: &[f32]| -> f64 {
            a.iter()
                .zip(b)
                .map(|(x, y)| (*x as f64) * (*y as f64))
                .sum()
        };
        let related = cos(&q, &doc_vecs[0]);
        let unrelated_1 = cos(&q, &doc_vecs[1]);
        let unrelated_2 = cos(&q, &doc_vecs[2]);
        println!("related={related:.4} unrelated=[{unrelated_1:.4}, {unrelated_2:.4}]");
        assert!(
            related > unrelated_1 && related > unrelated_2,
            "prefixed query must rank the related document first"
        );
    }
}
