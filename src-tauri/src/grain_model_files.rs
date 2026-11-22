//! [GRAIN] Pinned files for Grain's own models: the talk-back voice and Meet's
//! speaker detection. Handy's speech-to-text catalog stays 1:1 with Handy;
//! these models never enter it.
//!
//! A file is pinned to an exact URL revision, size and SHA-256. It is streamed
//! into `<name>.partial`, verified, and only then renamed into place, so a
//! present file is always a complete, verified one.
//!
//! Each model is one folder under the app's `models/` directory. The commands
//! here let the UI see, download (with progress), cancel and delete them; a
//! model in use is never deleted.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

pub(crate) struct PinnedFile {
    /// Path inside the model's folder.
    pub(crate) name: &'static str,
    pub(crate) url: &'static str,
    pub(crate) bytes: u64,
    pub(crate) sha256: &'static str,
}

impl PinnedFile {
    /// Whether the file is installed in `dir`: present with its pinned size.
    /// Its content was verified when it was downloaded.
    pub(crate) fn present_in(&self, dir: &Path) -> bool {
        std::fs::metadata(dir.join(self.name)).is_ok_and(|meta| meta.len() == self.bytes)
    }

    /// Pinned to a revision (not a moving branch) with a full SHA-256.
    #[cfg(test)]
    pub(crate) fn is_pinned(&self) -> bool {
        self.url.contains("/resolve/")
            && self.sha256.len() == 64
            && self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

fn partial_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    path.with_file_name(name)
}

/// Download `file` into `dir`. `progress` is called with the bytes received
/// so far after every chunk; setting `cancel` stops it. A failed, cancelled or
/// unverifiable download leaves nothing behind.
pub(crate) async fn download(
    client: &reqwest::Client,
    file: &PinnedFile,
    dir: &Path,
    mut progress: impl FnMut(u64),
    cancel: &AtomicBool,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let path = dir.join(file.name);
    let folder = path.parent().unwrap_or(dir);
    std::fs::create_dir_all(folder)
        .map_err(|e| format!("could not create {}: {e}", folder.display()))?;
    let partial = partial_path(&path);
    let response = client
        .get(file.url)
        .timeout(Duration::from_secs(30 * 60))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("{}: {e}", file.name))?;
    let mut out = std::fs::File::create(&partial).map_err(|e| format!("{}: {e}", file.name))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    let result: Result<(), String> = async {
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Acquire) {
                return Err(CANCELLED.into());
            }
            let chunk = chunk.map_err(|e| format!("{}: {e}", file.name))?;
            downloaded += chunk.len() as u64;
            if downloaded > file.bytes {
                return Err(format!("{} is larger than expected", file.name));
            }
            hasher.update(&chunk);
            out.write_all(&chunk)
                .map_err(|e| format!("{}: {e}", file.name))?;
            progress(downloaded);
        }
        out.sync_all().map_err(|e| format!("{}: {e}", file.name))?;
        if downloaded != file.bytes || format!("{:x}", hasher.finalize()) != file.sha256 {
            return Err(format!("{} failed verification", file.name));
        }
        Ok(())
    }
    .await;
    drop(out);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    std::fs::rename(&partial, &path).map_err(|e| format!("{}: {e}", file.name))
}

const CANCELLED: &str = "download cancelled";

// ── Grain's extra models ────────────────────────────────────────────────────

/// One of Grain's own models (outside Handy's speech-to-text catalog).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum GrainModel {
    /// Meet's speaker detection (the diarizer).
    SpeakerDetection,
    /// The Agent's talk-back voice.
    TalkBackVoice,
}

/// What a model is on disk: its folder, the pinned files a download installs,
/// and whether it is in use right now (then it is never deleted).
pub(crate) struct ModelFiles {
    pub(crate) folder: &'static [&'static str],
    pub(crate) files: &'static [PinnedFile],
    pub(crate) in_use: fn() -> bool,
}

impl ModelFiles {
    pub(crate) fn dir(&self, app: &AppHandle) -> Result<PathBuf, String> {
        let mut dir = crate::portable::app_data_dir(app)
            .map_err(|e| e.to_string())?
            .join("models");
        dir.extend(self.folder);
        Ok(dir)
    }

    pub(crate) fn installed_in(&self, dir: &Path) -> bool {
        self.files.iter().all(|file| file.present_in(dir))
    }

    fn total_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }
}

impl GrainModel {
    fn files(self) -> &'static ModelFiles {
        match self {
            GrainModel::SpeakerDetection => &crate::grain_meet::commands::SPEAKER_MODEL,
            GrainModel::TalkBackVoice => &crate::grain_tts::MODEL,
        }
    }

    fn job(self) -> &'static Job {
        &JOBS[self as usize]
    }
}

/// A model's download in flight.
struct Job {
    running: AtomicBool,
    cancel: AtomicBool,
    downloaded: AtomicU64,
}

#[allow(clippy::declare_interior_mutable_const)]
const IDLE: Job = Job {
    running: AtomicBool::new(false),
    cancel: AtomicBool::new(false),
    downloaded: AtomicU64::new(0),
};

static JOBS: [Job; 2] = [IDLE, IDLE];

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct GrainModelStatus {
    pub installed: bool,
    /// A download is running; `downloaded` of `total` bytes are in.
    pub downloading: bool,
    pub downloaded: u64,
    /// What a full download fetches.
    pub total: u64,
    /// Space the model's folder takes now (extra voices included).
    pub disk_bytes: u64,
}

/// A model's download moved on, or ended (`done`: finished, failed or
/// cancelled; read its status again).
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct GrainModelProgress {
    pub model: GrainModel,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
}

fn folder_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => folder_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

fn status(app: &AppHandle, model: GrainModel) -> Result<GrainModelStatus, String> {
    let files = model.files();
    let dir = files.dir(app)?;
    let job = model.job();
    Ok(GrainModelStatus {
        installed: files.installed_in(&dir),
        downloading: job.running.load(Ordering::Acquire),
        downloaded: job.downloaded.load(Ordering::Acquire),
        total: files.total_bytes(),
        disk_bytes: folder_bytes(&dir),
    })
}

/// Download a model's missing files, reporting progress. Returns at once when
/// it is installed or already downloading; a cancel is not an error.
pub(crate) async fn install(app: &AppHandle, model: GrainModel) -> Result<(), String> {
    let files = model.files();
    let dir = files.dir(app)?;
    let job = model.job();
    if files.installed_in(&dir) || job.running.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    struct Reset(&'static Job);
    impl Drop for Reset {
        fn drop(&mut self) {
            self.0.running.store(false, Ordering::Release);
        }
    }
    let reset = Reset(job);
    job.cancel.store(false, Ordering::Release);
    let total = files.total_bytes();
    let mut done: u64 = files
        .files
        .iter()
        .filter(|file| file.present_in(&dir))
        .map(|file| file.bytes)
        .sum();
    job.downloaded.store(done, Ordering::Release);
    let client = app.state::<reqwest::Client>().inner().clone();
    let emit = |downloaded: u64, finished: bool| {
        let _ = GrainModelProgress {
            model,
            downloaded,
            total,
            done: finished,
        }
        .emit(app);
    };
    emit(done, false);
    let mut result = Ok(());
    for file in files.files.iter().filter(|file| !file.present_in(&dir)) {
        // One event per whole percent, not one per network chunk.
        let mut shown = done * 100 / total.max(1);
        let base = done;
        result = download(
            &client,
            file,
            &dir,
            |received| {
                let now = base + received;
                job.downloaded.store(now, Ordering::Release);
                let percent = now * 100 / total.max(1);
                if percent != shown {
                    shown = percent;
                    emit(now, false);
                }
            },
            &job.cancel,
        )
        .await;
        if result.is_err() {
            break;
        }
        done += file.bytes;
    }
    drop(reset);
    emit(job.downloaded.load(Ordering::Acquire), true);
    match result {
        Err(error) if error == CANCELLED => Ok(()),
        other => other,
    }
}

/// Status of one of Grain's own models.
#[tauri::command]
#[specta::specta]
pub fn grain_model_status(app: AppHandle, model: GrainModel) -> Result<GrainModelStatus, String> {
    status(&app, model)
}

/// Download a model (verified by size and SHA-256). Resolves when the
/// download ends; progress arrives as [`GrainModelProgress`] events.
#[tauri::command]
#[specta::specta]
pub async fn grain_model_download(
    app: AppHandle,
    model: GrainModel,
) -> Result<GrainModelStatus, String> {
    install(&app, model).await?;
    status(&app, model)
}

/// Stop a model's download. Nothing of it is kept.
#[tauri::command]
#[specta::specta]
pub fn grain_model_cancel(model: GrainModel) {
    let job = model.job();
    if job.running.load(Ordering::Acquire) {
        job.cancel.store(true, Ordering::Release);
    }
}

/// Delete a model's folder. Refused while it downloads or is in use.
#[tauri::command]
#[specta::specta]
pub fn grain_model_delete(app: AppHandle, model: GrainModel) -> Result<GrainModelStatus, String> {
    let files = model.files();
    if model.job().running.load(Ordering::Acquire) {
        return Err("Cancel the download first".into());
    }
    if (files.in_use)() {
        return Err(match model {
            GrainModel::SpeakerDetection => {
                "Recall is using the speaker model; try again when processing ends"
            }
            GrainModel::TalkBackVoice => "The voice is speaking; try again when it stops",
        }
        .into());
    }
    let dir = files.dir(&app)?;
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not delete the model: {error}")),
    }
    // An emptied parent folder (e.g. `grain-tts/`) goes too; a non-empty one stays.
    if files.folder.len() > 1 {
        if let Some(parent) = dir.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
    log::info!("[GRAIN] deleted model {model:?} at {}", dir.display());
    status(&app, model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_sits_beside_the_file() {
        assert_eq!(
            partial_path(Path::new("m/voices/Zoey.safetensors")),
            Path::new("m/voices/Zoey.safetensors.partial")
        );
        assert_eq!(
            partial_path(Path::new("m/model.q8.gguf")),
            Path::new("m/model.q8.gguf.partial")
        );
    }

    #[test]
    fn present_means_the_pinned_size() {
        let dir = std::env::temp_dir().join(format!("grain-model-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = PinnedFile {
            name: "f.bin",
            url: "https://example.com/resolve/abc/f.bin",
            bytes: 3,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };
        assert!(!file.present_in(&dir));
        std::fs::write(dir.join("f.bin"), b"ab").unwrap();
        assert!(!file.present_in(&dir));
        std::fs::write(dir.join("f.bin"), b"abc").unwrap();
        assert!(file.present_in(&dir));
        assert!(file.is_pinned());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_size_counts_nested_files() {
        let dir = std::env::temp_dir().join(format!("grain-model-size-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("voices")).unwrap();
        std::fs::write(dir.join("a.bin"), b"abcd").unwrap();
        std::fs::write(dir.join("voices/b.bin"), b"ef").unwrap();
        assert_eq!(folder_bytes(&dir), 6);
        assert_eq!(folder_bytes(&dir.join("missing")), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_model_has_its_own_job_and_folder() {
        assert!(!std::ptr::eq(
            GrainModel::SpeakerDetection.job(),
            GrainModel::TalkBackVoice.job()
        ));
        assert_ne!(
            GrainModel::SpeakerDetection.files().folder,
            GrainModel::TalkBackVoice.files().folder
        );
        assert_eq!(GrainModelProgress::NAME, "grain-model-progress");
    }
}
