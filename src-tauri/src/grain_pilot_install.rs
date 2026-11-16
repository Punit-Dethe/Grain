//! [GRAIN] Grain Pilot: download, verify, unpack and remove the pinned Cua
//! Driver release (`docs/Pilot/PLAN.md`, "Grain-managed install").
//!
//! Connect downloads the release asset for this OS and CPU, checks its SHA-256
//! against the one compiled into Grain, and unpacks only the driver and its UI
//! Automation helper into `<app data>/pilot/driver/<version>/`. Nothing is
//! unpacked before the hash matches. The driver's own updater is never called
//! (and its update check is switched off on every spawn), so the version only
//! changes with a Grain release. Disconnect deletes the whole `pilot` folder.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use futures_util::StreamExt as _;
use grain_core::pilot::{self as core, DriverAsset};
use sha2::{Digest as _, Sha256};
use tauri::{AppHandle, Manager as _};

/// Written after a verified unpack; its absence means "not installed".
const VERIFIED_MARKER: &str = ".verified";
/// Written when the driver's own state folders did not exist before Grain
/// first ran it, so Disconnect may delete them.
const OWNS_DRIVER_STATE: &str = "owns-driver-state";

/// `<app data>/pilot`.
pub(crate) fn pilot_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("pilot"))
        .map_err(|error| format!("Could not find Grain's data folder: {error}"))
}

fn version_dir(pilot: &Path) -> PathBuf {
    pilot.join("driver").join(core::DRIVER_VERSION)
}

fn this_platform() -> Result<DriverAsset, String> {
    core::driver_asset(std::env::consts::OS, std::env::consts::ARCH)
        .ok_or_else(|| "Grain Pilot is not available on this OS yet (Windows only for now).".into())
}

/// The installed driver executable, when a verified install is present.
pub(crate) fn installed(app: &AppHandle) -> Option<PathBuf> {
    installed_in(&pilot_dir(app).ok()?)
}

fn installed_in(pilot: &Path) -> Option<PathBuf> {
    let dir = version_dir(pilot);
    let complete = dir.join(VERIFIED_MARKER).is_file()
        && core::DRIVER_FILES
            .iter()
            .all(|file| dir.join(file).is_file());
    complete.then(|| dir.join(core::DRIVER_FILES[0]))
}

/// One install at a time (two quick Connect clicks share one download).
static INSTALLING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Install the driver if it is not installed yet. Returns the executable.
pub(crate) async fn ensure_installed(app: &AppHandle) -> Result<PathBuf, String> {
    ensure_installed_in(&pilot_dir(app)?).await
}

pub(crate) async fn ensure_installed_in(pilot: &Path) -> Result<PathBuf, String> {
    let _installing = INSTALLING.lock().await;
    if let Some(path) = installed_in(pilot) {
        return Ok(path);
    }
    let asset = this_platform()?;
    let pilot = pilot.to_path_buf();
    let target = version_dir(&pilot);
    std::fs::create_dir_all(&pilot)
        .map_err(|error| format!("Could not create Grain Pilot's folder: {error}"))?;
    let download = pilot.join(format!("{}.download", asset.file));
    log::info!("[GRAIN] pilot: downloading {}", asset.file);
    let result = download_verified(&asset, &download).await;
    let unpacked = match result {
        Ok(()) => {
            let (download, target) = (download.clone(), target.clone());
            tauri::async_runtime::spawn_blocking(move || unpack(&download, &target))
                .await
                .map_err(|error| format!("Unpacking stopped: {error}"))
                .and_then(|result| result)
        }
        Err(error) => Err(error),
    };
    let _ = std::fs::remove_file(&download);
    unpacked?;
    remove_other_versions(&pilot);
    installed_in(&pilot).ok_or_else(|| "The driver was unpacked but is incomplete.".into())
}

/// Download `asset` to `path`, refusing anything over the size cap or with
/// the wrong SHA-256. The file is deleted on any failure.
async fn download_verified(asset: &DriverAsset, path: &Path) -> Result<(), String> {
    let result = download_to(asset, path).await;
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

async fn download_to(asset: &DriverAsset, path: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("Grain")
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|error| format!("Could not prepare the download: {error}"))?;
    let response = client
        .get(asset.url())
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| format!("Could not download Grain Pilot's driver: {error}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > core::MAX_DOWNLOAD_BYTES)
    {
        return Err("The driver download is larger than expected; not installing it.".into());
    }
    let mut file = std::fs::File::create(path)
        .map_err(|error| format!("Could not save the download: {error}"))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("The download was interrupted: {error}"))?;
        received += chunk.len() as u64;
        if received > core::MAX_DOWNLOAD_BYTES {
            return Err("The driver download is larger than expected; not installing it.".into());
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .map_err(|error| format!("Could not save the download: {error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("Could not save the download: {error}"))?;
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != asset.sha256 {
        log::warn!("[GRAIN] pilot: download hash mismatch ({digest})");
        return Err(
            "The downloaded driver does not match the one Grain was built for; not installing it."
                .into(),
        );
    }
    Ok(())
}

/// Unpack the driver files from a verified zip into `target`, through a
/// `.partial` folder so a half-finished unpack is never mistaken for an
/// install.
fn unpack(zip_path: &Path, target: &Path) -> Result<(), String> {
    let partial = target.with_extension("partial");
    let _ = std::fs::remove_dir_all(&partial);
    std::fs::create_dir_all(&partial)
        .map_err(|error| format!("Could not create the driver folder: {error}"))?;
    let result = unpack_into(zip_path, &partial);
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(error);
    }
    let _ = std::fs::remove_dir_all(target);
    std::fs::rename(&partial, target).map_err(|error| {
        let _ = std::fs::remove_dir_all(&partial);
        format!("Could not finish installing the driver: {error}")
    })
}

fn unpack_into(zip_path: &Path, dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|error| format!("Could not open the download: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("The download is not a valid archive: {error}"))?;
    for name in core::DRIVER_FILES {
        let mut entry = archive
            .by_name(name)
            .map_err(|_| format!("The download has no {name}."))?;
        if entry.size() > core::MAX_UNPACKED_FILE_BYTES {
            return Err(format!("{name} in the download is larger than expected."));
        }
        // Fixed names only: nothing from the archive chooses a path.
        let mut out = std::fs::File::create(dir.join(name))
            .map_err(|error| format!("Could not write {name}: {error}"))?;
        let copied = std::io::copy(
            &mut std::io::Read::take(&mut entry, core::MAX_UNPACKED_FILE_BYTES + 1),
            &mut out,
        )
        .map_err(|error| format!("Could not unpack {name}: {error}"))?;
        if copied > core::MAX_UNPACKED_FILE_BYTES {
            return Err(format!("{name} in the download is larger than expected."));
        }
    }
    std::fs::write(dir.join(VERIFIED_MARKER), core::DRIVER_VERSION)
        .map_err(|error| format!("Could not finish installing the driver: {error}"))
}

/// Remove driver versions other than the pinned one.
fn remove_other_versions(pilot: &Path) {
    let Ok(entries) = std::fs::read_dir(pilot.join("driver")) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name() != core::DRIVER_VERSION {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The driver's own state folders outside Grain's data folder.
fn driver_state_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        dirs.push(PathBuf::from(home).join(".cua-driver"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("cua-driver"));
    }
    dirs
}

/// Before the first daemon start: remember whether the driver's state
/// folders are Grain's to delete (they did not exist before).
pub(crate) fn note_driver_state_owner(app: &AppHandle) {
    let Ok(pilot) = pilot_dir(app) else {
        return;
    };
    let marker = pilot.join(OWNS_DRIVER_STATE);
    if marker.exists() {
        return;
    }
    if driver_state_dirs().iter().all(|dir| !dir.exists()) {
        let _ = std::fs::write(marker, "");
    }
}

/// Disconnect: delete the driver, its manifest and Grain's markers, and the
/// driver's own state folders when Grain created them. The daemon must be
/// stopped first.
pub(crate) fn remove(app: &AppHandle) {
    let Ok(pilot) = pilot_dir(app) else {
        return;
    };
    if pilot.join(OWNS_DRIVER_STATE).exists() {
        for dir in driver_state_dirs() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    match std::fs::remove_dir_all(&pilot) {
        Ok(()) => log::info!("[GRAIN] pilot: removed the driver"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => log::warn!("[GRAIN] pilot: could not remove the driver: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("grain-pilot-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).expect("zip file");
        let mut zip = zip::ZipWriter::new(file);
        for (name, bytes) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .expect("entry");
            zip.write_all(bytes).expect("write");
        }
        zip.finish().expect("finish");
    }

    #[test]
    fn unpacks_only_the_driver_files_and_marks_them_verified() {
        let dir = scratch("unpack");
        let zip_path = dir.join("driver.zip");
        write_zip(
            &zip_path,
            &[
                ("cua-driver.exe", b"driver"),
                ("cua-driver-uia.exe", b"uia"),
                ("cua_driver_sdk.dll", b"sdk"),
                ("../escape.exe", b"never"),
            ],
        );
        let target = dir.join("0.52.0");
        unpack(&zip_path, &target).expect("unpack");
        assert_eq!(
            std::fs::read(target.join("cua-driver.exe")).unwrap(),
            b"driver"
        );
        assert!(target.join(VERIFIED_MARKER).is_file());
        assert!(!target.join("cua_driver_sdk.dll").exists());
        assert!(!dir.join("escape.exe").exists());
        assert!(!target.with_extension("partial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_archive_without_the_driver_leaves_nothing_behind() {
        let dir = scratch("missing");
        let zip_path = dir.join("driver.zip");
        write_zip(&zip_path, &[("cua-driver.exe", b"driver")]);
        let target = dir.join("0.52.0");
        let error = unpack(&zip_path, &target).unwrap_err();
        assert!(error.contains("cua-driver-uia.exe"), "{error}");
        assert!(!target.exists());
        assert!(!target.with_extension("partial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
