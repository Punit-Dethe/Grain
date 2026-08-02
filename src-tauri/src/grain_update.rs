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
//! Two commands and one event, holding nothing between calls. [`install_update`]
//! re-runs the check rather than caching the [`Update`] handle from
//! [`check_for_update`]: a cached handle would be a live network resource kept
//! alive across an arbitrary user pause, for the sake of one cheap request.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;
use tauri_specta::Event;

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
    if !force && !crate::settings::get_settings(&app).update_checks_enabled {
        return Ok(None);
    }

    let updater = app.updater().map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;

    Ok(found.map(|update| UpdateInfo {
        version: update.version.clone(),
        current_version: current_version(&app),
        notes: update.body.clone(),
        date: update.date.map(|d| d.to_string()),
    }))
}

/// Download and install the pending update, then restart into it.
///
/// `restart()` does not return, so there is deliberately no success path: either
/// this call diverges into the new build or it returns an error.
#[tauri::command]
#[specta::specta]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No update is available".to_string())?;

    let progress_app = app.clone();
    let mut downloaded: u64 = 0;

    update
        .download_and_install(
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

    log::info!("[GRAIN] update installed; restarting");
    app.restart();
}
