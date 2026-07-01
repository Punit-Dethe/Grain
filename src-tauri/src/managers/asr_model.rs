//! [GRAIN] Native ASR (streaming) model manager — GGUF models for transcribe-cpp.
//!
//! Single-file GGUF models fetched directly from Hugging Face (no archive, no
//! extraction), stored under `<app_data>/models/asr-gguf/<id>/<file>.gguf`.
//! Deliberately separate from the Batch/Rolling registry: these are the
//! streaming models surfaced in the "Streaming" section of the model library
//! (every entry here is `supports_streaming`), and they run through the
//! transcribe-cpp engine, not transcribe-rs.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use futures_util::StreamExt;
use log::info;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};

/// One streaming GGUF model in the catalog.
struct GgufModel {
    /// Short stable id used in settings + on disk.
    id: &'static str,
    name: &'static str,
    /// Hugging Face repo (`org/name`) the GGUF lives in.
    hf_repo: &'static str,
    /// The GGUF filename to download (a single quantization).
    filename: &'static str,
    languages: &'static [&'static str],
    /// Download size (MB), for the UI.
    size_mb: u64,
    /// Rough resident footprint (MB).
    memory_mb: u32,
}

/// The built-in streaming catalog (Handy's `handy-computer/*` GGUF repos). Every
/// entry is a true streaming model (`supports_streaming`).
const GGUF_CATALOG: &[GgufModel] = &[
    GgufModel {
        id: "nemotron-3.5-streaming-0.6b",
        name: "NVIDIA Nemotron Streaming 3.5 (best accuracy)",
        hf_repo: "handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf",
        filename: "nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf",
        languages: &["en", "es", "fr", "it", "pt"],
        size_mb: 716,
        memory_mb: 820,
    },
    GgufModel {
        id: "parakeet-unified-en-0.6b",
        name: "Parakeet Unified EN 0.6B",
        hf_repo: "handy-computer/parakeet-unified-en-0.6b-gguf",
        filename: "parakeet-unified-en-0.6b-Q8_0.gguf",
        languages: &["en"],
        size_mb: 697,
        memory_mb: 800,
    },
    GgufModel {
        id: "moonshine-streaming-small",
        name: "Moonshine Streaming Small (English, fast)",
        hf_repo: "handy-computer/moonshine-streaming-small-gguf",
        filename: "moonshine-streaming-small-Q8_0.gguf",
        languages: &["en"],
        size_mb: 189,
        memory_mb: 260,
    },
];

fn catalog_entry(id: &str) -> Option<&'static GgufModel> {
    GGUF_CATALOG.iter().find(|m| m.id == id)
}

fn hf_resolve_url(repo: &str, filename: &str) -> String {
    format!("https://huggingface.co/{repo}/resolve/main/{filename}")
}

/// UI-facing description of a streaming model: catalog metadata + install state.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AsrModelInfo {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub languages: Vec<String>,
    pub sample_rate_hz: u32,
    pub size_mb: u64,
    pub memory_mb: u32,
    pub is_downloaded: bool,
    pub is_downloading: bool,
}

/// Download progress for a streaming model (mirrors the Batch model event shape).
#[derive(Debug, Clone, Serialize, Type)]
pub struct AsrDownloadProgress {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

pub struct AsrModelManager {
    app_handle: AppHandle,
    /// `<app_data>/models/asr-gguf`.
    dir: PathBuf,
    downloading: Mutex<HashSet<String>>,
    cancel_flags: Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>,
}

impl AsrModelManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        let dir = crate::portable::app_data_dir(app_handle)
            .map_err(|e| anyhow::anyhow!("Failed to get app data dir: {}", e))?
            .join("models")
            .join("asr-gguf");
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        Ok(Self {
            app_handle: app_handle.clone(),
            dir,
            downloading: Mutex::new(HashSet::new()),
            cancel_flags: Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Local path a model's GGUF would live at (whether or not it's present).
    fn gguf_path_for(&self, m: &GgufModel) -> PathBuf {
        self.dir.join(m.id).join(m.filename)
    }

    /// The GGUF path for an installed model, or `None` if the id is unknown or
    /// the file isn't downloaded. This is what the streaming worker loads.
    pub fn get_gguf_path(&self, id: &str) -> Option<PathBuf> {
        let m = catalog_entry(id)?;
        let p = self.gguf_path_for(m);
        p.is_file().then_some(p)
    }

    fn is_installed(&self, m: &GgufModel) -> bool {
        self.gguf_path_for(m).is_file()
    }

    /// List all catalog models with their current install/download state.
    pub fn list(&self) -> Vec<AsrModelInfo> {
        let downloading = self.downloading.lock().unwrap();
        GGUF_CATALOG
            .iter()
            .map(|m| AsrModelInfo {
                id: m.id.to_string(),
                name: m.name.to_string(),
                backend: "transcribe-cpp".to_string(),
                languages: m.languages.iter().map(|s| s.to_string()).collect(),
                sample_rate_hz: 16_000,
                size_mb: m.size_mb,
                memory_mb: m.memory_mb,
                is_downloaded: self.is_installed(m),
                is_downloading: downloading.contains(m.id),
            })
            .collect()
    }

    /// Cancel an in-flight download.
    pub fn cancel_download(&self, model_id: &str) {
        if let Some(flag) = self.cancel_flags.lock().unwrap().get(model_id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Delete an installed model's directory.
    pub fn delete(&self, model_id: &str) -> Result<()> {
        let dir = self.dir.join(model_id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
            info!("[GRAIN] deleted streaming model '{}'", model_id);
        }
        Ok(())
    }

    /// Download a model's single GGUF file directly from Hugging Face. Emits
    /// `asr-model-download-progress` (throttled). Idempotent: an installed model
    /// returns immediately.
    pub async fn download(&self, model_id: &str) -> Result<()> {
        let m = catalog_entry(model_id)
            .ok_or_else(|| anyhow::anyhow!("unknown streaming model: {model_id}"))?;
        if self.is_installed(m) {
            return Ok(());
        }

        // Single-flight guard.
        {
            let mut dl = self.downloading.lock().unwrap();
            if !dl.insert(model_id.to_string()) {
                return Ok(()); // already downloading
            }
        }
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancel_flags
            .lock()
            .unwrap()
            .insert(model_id.to_string(), cancel_flag.clone());

        let result = self.download_inner(m, &cancel_flag).await;

        self.downloading.lock().unwrap().remove(model_id);
        self.cancel_flags.lock().unwrap().remove(model_id);
        result
    }

    async fn download_inner(&self, m: &GgufModel, cancel_flag: &Arc<AtomicBool>) -> Result<()> {
        let model_dir = self.dir.join(m.id);
        fs::create_dir_all(&model_dir)?;
        let final_path = model_dir.join(m.filename);
        // Download to a temp file, then rename — a half-written .gguf must never
        // look "installed".
        let tmp_path = model_dir.join(format!("{}.part", m.filename));

        let client = self
            .app_handle
            .try_state::<reqwest::Client>()
            .map(|c| c.inner().clone())
            .unwrap_or_default();

        let url = hf_resolve_url(m.hf_repo, m.filename);
        info!("[GRAIN] downloading streaming model '{}' from {url}", m.id);
        let response = client.get(&url).send().await?.error_for_status()?;
        let total = response.content_length().unwrap_or(0);

        let mut stream = response.bytes_stream();
        let mut file = fs::File::create(&tmp_path)?;
        let mut downloaded: u64 = 0;
        let mut last_emit = Instant::now();
        let throttle = Duration::from_millis(100);
        use std::io::Write;

        while let Some(chunk) = stream.next().await {
            if cancel_flag.load(Ordering::Relaxed) {
                drop(file);
                let _ = fs::remove_file(&tmp_path);
                info!("[GRAIN] streaming model download cancelled: {}", m.id);
                return Ok(());
            }
            let chunk = chunk?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            if last_emit.elapsed() >= throttle {
                self.emit_progress(m.id, downloaded, total);
                last_emit = Instant::now();
            }
        }
        file.flush()?;
        drop(file);
        fs::rename(&tmp_path, &final_path)?;
        self.emit_progress(m.id, downloaded, total.max(downloaded));

        info!("[GRAIN] streaming model '{}' installed", m.id);
        Ok(())
    }

    fn emit_progress(&self, model_id: &str, downloaded: u64, total: u64) {
        let percentage = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let _ = self.app_handle.emit(
            "asr-model-download-progress",
            AsrDownloadProgress {
                model_id: model_id.to_string(),
                downloaded,
                total,
                percentage,
            },
        );
    }
}
