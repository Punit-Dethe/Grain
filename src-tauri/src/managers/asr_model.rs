//! [GRAIN] M4: Native ASR model manager.
//!
//! The IO/host side of the Native ASR registry. The pure catalog, layout, and
//! validation live in `grain_asr_core::registry`; this manager adds the things
//! that touch the disk and network: listing what is installed, downloading +
//! extracting Sherpa `.tar.bz2` bundles, resolving an installed model to an
//! [`AsrModelSpec`], and deleting.
//!
//! Deliberately SEPARATE from [`crate::managers::model::ModelManager`] (the
//! Batch/Rolling registry keyed by `selected_model`): ASR bundles are multi-file
//! transducers with their own directory layout and lifecycle. Installed models
//! live under `<app_data>/models/asr/<id>/`.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use futures_util::StreamExt;
use grain_asr_core::model::AsrModelSpec;
use grain_asr_core::registry::{builtin_catalog, catalog_entry, AsrModelCatalogEntry};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};

/// UI-facing description of a Native ASR model: catalog metadata + install state.
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

/// Download progress for a Native ASR model (mirrors the Batch model event shape).
#[derive(Debug, Clone, Serialize, Type)]
pub struct AsrDownloadProgress {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

pub struct AsrModelManager {
    app_handle: AppHandle,
    /// `<app_data>/models/asr`.
    asr_dir: PathBuf,
    catalog: Vec<AsrModelCatalogEntry>,
    downloading: Mutex<HashSet<String>>,
    cancel_flags: Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>,
}

impl AsrModelManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        let asr_dir = crate::portable::app_data_dir(app_handle)
            .map_err(|e| anyhow::anyhow!("Failed to get app data dir: {}", e))?
            .join("models")
            .join("asr");
        if !asr_dir.exists() {
            fs::create_dir_all(&asr_dir)?;
        }
        Ok(Self {
            app_handle: app_handle.clone(),
            asr_dir,
            catalog: builtin_catalog(),
            downloading: Mutex::new(HashSet::new()),
            cancel_flags: Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Whether every required bundle file for `entry` is present on disk.
    fn is_installed(&self, entry: &AsrModelCatalogEntry) -> bool {
        let dir = entry.bundle_dir(&self.asr_dir);
        let present: Vec<String> = match fs::read_dir(&dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect(),
            Err(_) => return false,
        };
        entry.layout.is_complete(&present)
    }

    /// List all catalog models with their current install/download state.
    pub fn list(&self) -> Vec<AsrModelInfo> {
        let downloading = self.downloading.lock().unwrap();
        self.catalog
            .iter()
            .map(|e| AsrModelInfo {
                id: e.id.clone(),
                name: e.name.clone(),
                backend: format!("{:?}", e.backend),
                languages: e.languages.clone(),
                sample_rate_hz: e.sample_rate_hz,
                size_mb: e.download.size_mb,
                memory_mb: e.memory.approx_mb,
                is_downloaded: self.is_installed(e),
                is_downloading: downloading.contains(&e.id),
            })
            .collect()
    }

    /// Resolve an installed model to its [`AsrModelSpec`] (absolute file paths).
    /// `None` if the model is unknown or not fully downloaded.
    pub fn get_spec(&self, model_id: &str) -> Option<AsrModelSpec> {
        let entry = catalog_entry(model_id)?;
        if !self.is_installed(&entry) {
            return None;
        }
        Some(entry.to_spec(&entry.bundle_dir(&self.asr_dir)))
    }

    /// Cancel an in-flight download.
    pub fn cancel_download(&self, model_id: &str) {
        if let Some(flag) = self.cancel_flags.lock().unwrap().get(model_id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Delete an installed model's directory.
    pub fn delete(&self, model_id: &str) -> Result<()> {
        let dir = self.asr_dir.join(model_id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
            info!("[GRAIN] deleted ASR model '{}'", model_id);
        }
        Ok(())
    }

    /// Download and extract a model's Sherpa `.tar.bz2` bundle. Emits
    /// `asr-model-download-progress` events (throttled). Idempotent: a model
    /// already installed returns immediately.
    pub async fn download(&self, model_id: &str) -> Result<()> {
        let entry =
            catalog_entry(model_id).ok_or_else(|| anyhow::anyhow!("unknown ASR model: {model_id}"))?;
        if self.is_installed(&entry) {
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

        let result = self.download_inner(&entry, &cancel_flag).await;

        self.downloading.lock().unwrap().remove(model_id);
        self.cancel_flags.lock().unwrap().remove(model_id);
        result
    }

    async fn download_inner(
        &self,
        entry: &AsrModelCatalogEntry,
        cancel_flag: &Arc<AtomicBool>,
    ) -> Result<()> {
        let model_dir = self.asr_dir.join(&entry.id);
        fs::create_dir_all(&model_dir)?;
        let archive_path = self.asr_dir.join(format!("{}.tar.bz2", entry.id));

        let client = self
            .app_handle
            .try_state::<reqwest::Client>()
            .map(|c| c.inner().clone())
            .unwrap_or_default();

        info!("[GRAIN] downloading ASR model '{}'", entry.id);
        let response = client
            .get(&entry.download.url)
            .send()
            .await?
            .error_for_status()?;
        let total = response.content_length().unwrap_or(0);

        let mut stream = response.bytes_stream();
        let mut file = fs::File::create(&archive_path)?;
        let mut downloaded: u64 = 0;
        let mut last_emit = Instant::now();
        let throttle = Duration::from_millis(100);
        use std::io::Write;

        while let Some(chunk) = stream.next().await {
            if cancel_flag.load(Ordering::Relaxed) {
                drop(file);
                let _ = fs::remove_file(&archive_path);
                info!("[GRAIN] ASR download cancelled: {}", entry.id);
                return Ok(());
            }
            let chunk = chunk?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            if last_emit.elapsed() >= throttle {
                self.emit_progress(&entry.id, downloaded, total);
                last_emit = Instant::now();
            }
        }
        file.flush()?;
        drop(file);
        self.emit_progress(&entry.id, downloaded, total.max(downloaded));

        // Extract the .tar.bz2 into the model directory (creates the archive's
        // wrapping folder, which `bundle_dir` accounts for).
        let archive = fs::File::open(&archive_path)?;
        let decoder = bzip2::read::BzDecoder::new(archive);
        let mut tar = tar::Archive::new(decoder);
        tar.unpack(&model_dir)
            .map_err(|e| anyhow::anyhow!("failed to extract {}: {e}", entry.id))?;
        let _ = fs::remove_file(&archive_path);

        if !self.is_installed(entry) {
            warn!(
                "[GRAIN] ASR model '{}' extracted but bundle is incomplete",
                entry.id
            );
            return Err(anyhow::anyhow!(
                "extracted bundle for {} is missing required files",
                entry.id
            ));
        }
        info!("[GRAIN] ASR model '{}' installed", entry.id);
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
