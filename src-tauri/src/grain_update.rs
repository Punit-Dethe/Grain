//! [GRAIN] In-app updates.
//!
//! The release side of this already existed and was never wired to a button:
//! `tauri-plugin-updater` is registered, `tauri.conf.json` points at Grain's own
//! signed `latest.json`, `createUpdaterArtifacts` is on, and
//! `.github/workflows/grain-release.yml` builds, signs and publishes. What was
//! missing is the half the user can see — nothing ever called the plugin, so an
//! install could never learn that a newer one existed.
//!
//! # Why this lives in Rust
//!
//! The updater has a JS API and upstream Handy drives it entirely from the
//! frontend. Grain does not: the frontend talks to the backend through generated
//! commands only, and — more importantly — `update_checks_enabled` is a backend
//! setting. A policy enforced in the renderer is a policy that a second caller
//! can forget, so the gate lives next to the setting and the frontend cannot
//! check for updates behind the user's back.
//!
//! # Shape
//!
//! Launch checks run in Rust so they still happen when the main WebView was
//! never created (start-hidden) or was destroyed on close. Only the small,
//! serializable [`UpdateInfo`] is cached; the updater's live [`Update`] handle is
//! never retained across an arbitrary user pause.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use tauri_specta::Event;

const AUTOMATIC_CHECK_DELAY: Duration = Duration::from_secs(4);
#[cfg(debug_assertions)]
const PREVIEW_CHECK_DELAY: Duration = Duration::from_millis(550);
const SHOW_AFTER_UPDATE_MARKER: &str = ".show-after-update";
#[cfg(debug_assertions)]
const UPDATE_PREVIEW_ENV: &str = "GRAIN_PREVIEW_UPDATE";

/// `None` means no launch check has completed yet; `Some(None)` means the
/// running build is current. The mutex also coalesces the backend launch check
/// and the frontend's delayed check into one network request.
#[derive(Default)]
pub(crate) struct UpdateState {
    checked: tokio::sync::Mutex<Option<Option<UpdateInfo>>>,
}

/// A release newer than the running build.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct UpdateInfo {
    /// Version of the available release, e.g. `0.0.2`.
    pub version: String,
    /// The version running right now, so the UI can say "0.0.1 → 0.0.2".
    pub current_version: String,
    /// Release notes, when the release carried a body.
    pub notes: Option<String>,
    /// Publication date as the feed reported it.
    pub date: Option<String>,
    /// Debug-only, non-installing preview launched by `dev:update-preview`.
    pub preview: bool,
}

/// A newly discovered release. The cache covers WebViews created after this
/// event; the event updates an already-open sidebar (including manual checks).
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct UpdateAvailable {
    pub update: UpdateInfo,
}

/// Download progress for an update, so a 100 MB installer is not a dead button.
#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub struct UpdateDownloadProgress {
    pub downloaded: u64,
    /// `0` when the server sends no content length.
    pub total: u64,
    pub percentage: f64,
}

fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

#[cfg(debug_assertions)]
fn preview_enabled() -> bool {
    std::env::var_os(UPDATE_PREVIEW_ENV).is_some()
}

#[cfg(not(debug_assertions))]
fn preview_enabled() -> bool {
    false
}

#[cfg(debug_assertions)]
fn preview_info(app: &AppHandle) -> Option<UpdateInfo> {
    preview_enabled().then(|| UpdateInfo {
        version: "0.0.3".to_string(),
        current_version: current_version(app),
        notes: Some(
            "- A cleaner update notice in the sidebar.\n- Full release details before installation.\n- Automatic surfacing when an update is found.\n- Reliable checks while Grain lives in the tray.\n- A one-launch visibility override after installation.\n- Clear download and installation progress.\n- Keyboard-friendly update controls.\n- Safer recovery when installation fails.\n\nThis is preview data only. No files will be downloaded or changed."
                .to_string(),
        ),
        date: Some("2026-08-16T00:00:00Z".to_string()),
        preview: true,
    })
}

#[cfg(not(debug_assertions))]
fn preview_info(_app: &AppHandle) -> Option<UpdateInfo> {
    None
}

fn automatic_check_delay() -> Duration {
    #[cfg(debug_assertions)]
    if preview_enabled() {
        return PREVIEW_CHECK_DELAY;
    }

    AUTOMATIC_CHECK_DELAY
}

fn marker_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SHOW_AFTER_UPDATE_MARKER)
}

fn write_show_after_update_marker(app: &AppHandle, version: &str) -> Result<PathBuf, String> {
    let path = marker_path(&crate::portable::app_data_dir(app).map_err(|e| e.to_string())?);
    std::fs::write(&path, version).map_err(|e| {
        format!(
            "Could not prepare the post-update restart marker at {}: {e}",
            path.display()
        )
    })?;
    Ok(path)
}

fn take_marker(path: &Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            // The marker exists (or could not be inspected reliably), so honour
            // the visibility override. Leaving it in place is safer than losing
            // the only route past a persisted/CLI start-hidden preference.
            log::warn!(
                "[GRAIN] could not consume post-update visibility marker {}: {error}",
                path.display()
            );
            true
        }
    }
}

/// Consume the one-launch visibility override written immediately before an
/// update is installed. This intentionally beats both the saved preference and
/// a preserved `--start-hidden` updater argument.
pub(crate) fn take_show_after_update(app: &AppHandle) -> bool {
    match crate::portable::app_data_dir(app) {
        Ok(data_dir) => take_marker(&marker_path(&data_dir)),
        Err(error) => {
            log::warn!("[GRAIN] could not resolve post-update visibility marker: {error}");
            false
        }
    }
}

/// Perform exactly one delayed automatic check in the backend. The task and its
/// `AppHandle` are both released as soon as the launch check completes.
pub(crate) fn spawn_automatic_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(automatic_check_delay()).await;
        if let Err(error) = check_for_update(app, false).await {
            // Automatic checks are intentionally quiet in the UI, but retain a
            // diagnostic for support logs and the manual retry path.
            log::warn!("[GRAIN] automatic update check failed: {error}");
        }
    });
}

/// Return already-discovered update metadata without touching the network.
/// This lets a WebView created by the backend update check render the notice on
/// its first frame instead of waiting through a second launch delay.
#[tauri::command]
#[specta::specta]
pub async fn get_cached_update(app: AppHandle) -> Option<UpdateInfo> {
    app.state::<UpdateState>()
        .checked
        .lock()
        .await
        .as_ref()
        .and_then(Clone::clone)
}

fn surface_update(app: &AppHandle, update: &UpdateInfo) {
    let _ = UpdateAvailable {
        update: update.clone(),
    }
    .emit(app);
    crate::show_main_window(app);
}

/// Is there a newer release?
///
/// `force` is the manual "Check now" button: it bypasses `update_checks_enabled`
/// because the user just asked, in person. The automatic check on launch passes
/// `false` and stays silent when the setting is off.
///
/// Returns `Ok(None)` both when the app is current and when checks are off — to
/// every caller those are the same answer ("nothing to show"), and reporting a
/// disabled setting as an error would surface it as a failure in the UI.
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(app: AppHandle, force: bool) -> Result<Option<UpdateInfo>, String> {
    let preview = preview_info(&app);
    if preview.is_none() && !force && !crate::settings::get_settings(&app).update_checks_enabled {
        return Ok(None);
    }

    let state = app.state::<UpdateState>();
    let mut checked = state.checked.lock().await;

    if !force {
        if let Some(cached) = checked.as_ref() {
            let cached = cached.clone();
            drop(checked);
            return Ok(cached);
        }
    }

    let info = if preview.is_some() {
        preview
    } else {
        let updater = app.updater().map_err(|e| e.to_string())?;
        let found = updater.check().await.map_err(|e| e.to_string())?;

        found.map(|update| UpdateInfo {
            version: update.version.clone(),
            current_version: current_version(&app),
            notes: update.body.clone(),
            date: update.date.map(|d| d.to_string()),
            preview: false,
        })
    };

    *checked = Some(info.clone());
    drop(checked);

    // Update discovery overrides start-hidden and recreates a destroyed WebView
    // so the user always sees the install choice. No frontend must be alive for
    // this path to run.
    if let Some(update) = info.as_ref() {
        surface_update(&app, update);
    }

    Ok(info)
}

/// Download and install the pending update, then restart into it.
///
/// `restart()` does not return, so there is deliberately no success path: either
/// this call diverges into the new build or it returns an error.
#[tauri::command]
#[specta::specta]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if preview_enabled() {
        for percentage in [8_u64, 24, 47, 71, 91, 100] {
            let _ = UpdateDownloadProgress {
                downloaded: percentage,
                total: 100,
                percentage: percentage as f64,
            }
            .emit(&app);
            tokio::time::sleep(Duration::from_millis(170)).await;
        }
        log::info!("[GRAIN] update preview completed; no files changed");
        return Ok(());
    }

    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No update is available".to_string())?;

    let progress_app = app.clone();
    let mut downloaded: u64 = 0;

    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                let total = total.unwrap_or(0);
                let percentage = if total > 0 {
                    (downloaded as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                // A failed progress emit must not abort a download that is
                // otherwise fine — the bar stalls, the install still lands.
                let _ = UpdateDownloadProgress {
                    downloaded,
                    total,
                    percentage,
                }
                .emit(&progress_app);
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;

    // Windows' updater exits the process from inside `install`, so this marker
    // must exist before installation starts. Writing it only after the signed
    // package has downloaded avoids a false override for ordinary download
    // failures. On platforms where `install` returns an error, clean it up.
    let marker = write_show_after_update_marker(&app, &update.version)?;
    if let Err(error) = update.install(bytes) {
        if let Err(cleanup_error) = std::fs::remove_file(&marker) {
            if cleanup_error.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "[GRAIN] could not remove failed-update visibility marker {}: {cleanup_error}",
                    marker.display()
                );
            }
        }
        return Err(error.to_string());
    }

    log::info!("[GRAIN] update installed; restarting");
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::{marker_path, take_marker, SHOW_AFTER_UPDATE_MARKER};
    use std::path::PathBuf;

    #[test]
    fn post_update_marker_is_consumed_once() {
        let dir = tempfile::tempdir().unwrap();
        let marker = marker_path(dir.path());
        std::fs::write(&marker, "1.2.3").unwrap();

        assert!(take_marker(&marker));
        assert!(!take_marker(&marker));
        assert!(!marker.exists());
    }

    #[test]
    fn post_update_marker_stays_inside_app_data() {
        let data_dir = PathBuf::from("app-data");
        assert_eq!(
            marker_path(&data_dir),
            data_dir.join(SHOW_AFTER_UPDATE_MARKER)
        );
    }
}
