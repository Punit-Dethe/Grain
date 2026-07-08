//! [GRAIN] Grain Space semantic embedding engine (Phase 4).
//!
//! Opt-in, never shipped: BGE-small-en-v1.5 (f32 `model.safetensors` ≈ 130 MB,
//! MIT) is downloaded into the shared HF cache only after explicit user consent.
//! The engine is one dedicated OS thread that owns the tokenizer + Candle BERT
//! weights behind an mpsc channel — 100% independent from the audio/ASR threads.
//! The weights can be loaded in f16 (half the resident RAM) via the
//! `grain_space_embed_f16` setting; see [`set_use_f16`].
//!
//! Lifecycle (strict directive 7, overrides modelinfo.md's "never unload"):
//! spawned lazily by the FIRST semantic search while the overlay window is
//! open, kept warm while it stays open, dropped the instant the window is
//! destroyed (`window.rs` Destroyed hook → [`shutdown_engine`]). No idle
//! timers, nothing resident otherwise.

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
/// (33.4M params × 4 B ≈ 130 MB on disk); f16 is a load-time cast of this same
/// file (no separate download), see [`set_use_f16`].
const MODEL_FILES: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];
pub const EMBED_DIM: usize = 384;
const MAX_TOKENS: usize = 512;

pub const MODEL_PROGRESS_EVENT: &str = "grain-space://embed-model-progress";
pub const MODEL_COMPLETE_EVENT: &str = "grain-space://embed-model-complete";
pub const MODEL_ERROR_EVENT: &str = "grain-space://embed-model-error";

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

/// Load the model in half precision (f16) when set — ~half the resident RAM,
/// near-identical embeddings. Read at engine spawn; the setting command drops
/// the engine so a live one re-loads at the new precision. Mirrors the
/// `grain_space_embed_f16` setting; kept as an atomic because the engine layer
/// has no `AppHandle`.
static USE_F16: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Session flag: set when an f16 load produced invalid (all-zero/NaN) output
/// (candle 0.9.x f16 CPU matmul is unreliable) so subsequent spawns skip the
/// f16 attempt and its double-load. Cleared only by process restart.
static F16_DISABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Point the (next) engine load at f16 or f32. Call from a place that has the
/// setting; pair with [`shutdown_engine`] to re-load a resident engine.
pub fn set_use_f16(enabled: bool) {
    USE_F16.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

// -- download (consent-gated by the frontend) -----------------------------------

/// Live download cancellation token; `None` when no download is running.
static DOWNLOAD: Mutex<Option<CancellationToken>> = Mutex::new(None);

pub fn is_downloading() -> bool {
    DOWNLOAD.lock().unwrap().is_some()
}

pub fn cancel_download() {
    if let Some(token) = DOWNLOAD.lock().unwrap().take() {
        token.cancel();
    }
}

#[derive(Clone, Serialize)]
struct EmbedModelProgress {
    downloaded: u64,
    total: u64,
    percentage: f64,
}

/// Bridges hf-hub progress to the overlay/settings UI. Only the big
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
            &EmbedModelProgress {
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
/// Emits `MODEL_COMPLETE_EVENT` / `MODEL_ERROR_EVENT`; the semantic toggle
/// must stay OFF until [`model_on_disk`] verifies (edge-case rule).
pub async fn download_model(app: AppHandle) -> Result<(), String> {
    if model_on_disk() {
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

    // Clear the slot on every exit path (a cancel may already have taken it).
    DOWNLOAD.lock().unwrap().take();

    match result {
        Ok(true) => {
            let _ = app.emit(MODEL_COMPLETE_EVENT, ());
            Ok(())
        }
        Ok(false) => Ok(()), // cancelled: partial files stay for resume, no event
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

/// Drop the engine (thread joined, weights freed). Called on feature disable
/// and semantic-toggle off. No-op when not running.
pub fn shutdown_engine() {
    *ENGINE.lock().unwrap() = None;
}

/// Uninstall the model from the shared HF cache (R4 — reclaim the ~130 MB).
/// Drops the engine first: `model.safetensors` is mmap'd, and an mmap'd file
/// can't be deleted on Windows. Removes the whole `models--…` repo dir
/// (snapshots + blobs). A no-op when the files are already gone.
pub fn uninstall_model() -> Result<()> {
    shutdown_engine();
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

/// Drop the engine only if NEITHER surface that may use it is still alive — the
/// overlay browser OR the Recall agent panel (RECALL-PLAN §3.4). Called from
/// both windows' Destroyed hooks. A no-op when the engine isn't resident, so
/// Assist-only agent sessions (which never spawn it) pay nothing.
pub fn shutdown_engine_if_idle(app: &AppHandle) {
    use tauri::Manager;
    let overlay_open = app
        .get_webview_window(super::window::WINDOW_LABEL)
        .is_some();
    let panel_open = app.get_webview_window(crate::agent::PANEL_LABEL).is_some();
    if !overlay_open && !panel_open {
        shutdown_engine();
    }
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
        .name("grain-space-embed".to_string())
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
    let loaded = load_with_probe(&config, &tokenizer, &weights);
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

/// Load the model, then probe with a sentinel text. If f16 was used and the
/// forward produced invalid (all-zero/NaN) output — which candle 0.9.x's f16
/// CPU matmul can do silently — flip [`F16_DISABLED`] and reload in f32 so
/// embeddings (and thus recall) actually work this session.
fn load_with_probe(
    config: &PathBuf,
    tokenizer: &PathBuf,
    weights: &PathBuf,
) -> Result<(
    tokenizers::Tokenizer,
    candle_transformers::models::bert::BertModel,
    candle_core::Device,
)> {
    let tried_f16 = USE_F16.load(std::sync::atomic::Ordering::Relaxed)
        && !F16_DISABLED.load(std::sync::atomic::Ordering::Relaxed);
    let (tok, model, device) = load_model(config, tokenizer, weights)?;
    if tried_f16
        && embed_batch(&tok, &model, &device, &["probe".to_string()]).is_err()
    {
        log::warn!(
            "[GRAIN] embed f16 forward produced invalid output on this CPU; reloading in f32 for this session"
        );
        F16_DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);
        return load_model(config, tokenizer, weights);
    }
    Ok((tok, model, device))
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
    use candle_core::{DType, Device};
    use candle_nn::VarBuilder;
    use candle_transformers::models::bert::{BertModel, Config, DTYPE};

    let start = std::time::Instant::now();
    let device = Device::Cpu;
    // f16 halves the resident weights; the safetensors on disk is f32, so Candle
    // casts on load. Final embeddings are cast back to f32 (the vec index dtype).
    // F16_DISABLED is set once an f16 load produced invalid output (candle 0.9.x
    // f16 CPU matmul can silently yield all-zero/NaN) — skip the broken path.
    let f16 = USE_F16.load(std::sync::atomic::Ordering::Relaxed)
        && !F16_DISABLED.load(std::sync::atomic::Ordering::Relaxed);
    let dtype = if f16 { DType::F16 } else { DTYPE };

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
        VarBuilder::from_mmaped_safetensors(&[weights_path.clone()], dtype, &device)
            .context("map model.safetensors")?
    };
    let model = BertModel::load(vb, &config).context("build BERT graph")?;
    log::info!(
        "[GRAIN] embed model loaded in {} ms ({})",
        start.elapsed().as_millis(),
        if f16 { "f16" } else { "f32" }
    );
    Ok((tokenizer, model, device))
}

/// One text at a time (no padding logic needed; note counts are small and each
/// forward is a few ms on CPU). Mean-pool over the sequence, L2-normalize so
/// L2 distance in the vec index is monotonic with cosine similarity.
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
                                                                  // Pool/normalize in f32 so an f16 model doesn't lose precision here and
                                                                  // the stored vector is always f32 (the vec index dtype).
        let hidden = hidden.to_dtype(candle_core::DType::F32)?;
        let pooled = (hidden.sum(1)? / seq_len as f64)?; // [1, dim]
        let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?;
        let normalized = pooled.broadcast_div(&norm)?;
        let vec = normalized.squeeze(0)?.to_vec1::<f32>()?;
        // Reject poison at the source: a non-finite (NaN/Inf) or all-zero
        // embedding means the forward pass was corrupt (e.g. a half-loaded /
        // mmap-raced model). Returning Err keeps the caller from storing it,
        // which would otherwise make every later KNN return NULL distance and
        // crash recall. The note stays embed_stale=1 and retries next time.
        if !vec.iter().all(|x| x.is_finite()) || vec.iter().all(|&x| x == 0.0) {
            return Err(anyhow!(
                "embed produced non-finite/zero vector (model forward corrupt?)"
            ));
        }
        out.push(vec);
    }
    Ok(out)
}

/// The exact text a note embeds as (blank fields omitted; the tokenizer
/// truncates to `MAX_TOKENS` — title+tldr carry the meaning for long notes).
pub fn note_embed_text(title: &str, tldr: &str, body: &str) -> String {
    let mut parts = Vec::new();
    if !title.trim().is_empty() {
        parts.push(format!("Title: {}", title.trim()));
    }
    if !tldr.trim().is_empty() {
        parts.push(format!("Summary: {}", tldr.trim()));
    }
    if !body.trim().is_empty() {
        parts.push(format!("Body: {}", body.trim()));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn note_embed_text_omits_blank_fields() {
        assert_eq!(
            super::note_embed_text("Shopping", "Groceries.", "Milk and eggs"),
            "Title: Shopping\n\nSummary: Groceries.\n\nBody: Milk and eggs"
        );
        assert_eq!(
            super::note_embed_text("", "  ", "raw capture"),
            "Body: raw capture"
        );
        assert_eq!(super::note_embed_text("", "", ""), "");
    }

    /// Regression: when f16 is enabled but its forward pass produces invalid
    /// (all-zero/NaN) output — as candle 0.9.x's f16 CPU matmul does —
    /// [`load_with_probe`] must flip [`F16_DISABLED`] and reload in f32 so
    /// embeddings (and recall) keep working. Skips itself if the model isn't
    /// on disk.
    #[test]
    fn f16_falls_back_to_f32_when_broken() {
        let (cfg, tok, w) = match (
            super::cached_file("config.json"),
            super::cached_file("tokenizer.json"),
            super::cached_file("model.safetensors"),
        ) {
            (Some(c), Some(t), Some(w)) => (c, t, w),
            _ => {
                println!("model not on disk; skipped");
                return;
            }
        };
        super::USE_F16.store(true, std::sync::atomic::Ordering::Relaxed);
        super::F16_DISABLED.store(false, std::sync::atomic::Ordering::Relaxed);
        let (tokenizer, model, device) =
            super::load_with_probe(&cfg, &tok, &w).expect("load_with_probe");
        let v = super::embed_batch(
            &tokenizer,
            &model,
            &device,
            &["my wi fi password is interstellar".to_string()],
        )
        .expect("embed_batch after fallback");
        assert!(v[0].iter().all(|x| x.is_finite()), "NaN/Inf after fallback");
        assert!(v[0].iter().any(|&x| x != 0.0), "all-zero after fallback");
        assert!(
            super::F16_DISABLED.load(std::sync::atomic::Ordering::Relaxed),
            "F16_DISABLED must be set after a broken f16 load"
        );
    }
}
