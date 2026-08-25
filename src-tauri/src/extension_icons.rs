//! Extension icon materialisation and recommendation delivery.
//!
//! Authors ship one verified 512² PNG master. The host writes that master to
//! the installed asset cache and derives the pill's one fixed, premultiplied
//! 64² RGBA payload. Only the tiny derived file is read during recommendation;
//! decoded masters are never retained in process state.

use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use base64::Engine as _;
use grain_core::AppContext;
use grain_sdk::{GrainPack, ICON_MASTER_DIM, ICON_MAX_BYTES};
use image::GenericImageView as _;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

const MASTER_FILE: &str = "icon.png";
const PILL_FILE: &str = "pill.rgba";
const UI_FILE: &str = "ui.png";
const UI_ICON_DIM: u32 = 128;
const DEV_CACHE_DIRECTORY: &str = ".dev-icons-v1";
static ASSET_LOCK: Mutex<()> = Mutex::new(());

fn assets_root(app: &AppHandle) -> Result<PathBuf, String> {
    let ctx = app
        .try_state::<std::sync::Arc<AppContext>>()
        .ok_or("app context unavailable")?;
    Ok(ctx.data_dir.join("extensions").join(".assets"))
}

fn version_dir(app: &AppHandle, id: &str, version: &str) -> Result<PathBuf, String> {
    grain_sdk::validate_extension_id(id)?;
    grain_sdk::validate_extension_version(version)?;
    Ok(assets_root(app)?.join(id).join(version))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;

    let parent = path.parent().ok_or("icon cache path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temp = parent.join(format!(".icon-{}.tmp", uuid::Uuid::new_v4().simple()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = std::fs::remove_file(&temp);
        return Err(error.to_string());
    }
    drop(file);
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            error.to_string()
        })?;
    }
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        error.to_string()
    })
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("extension icon is larger than {} KB", limit / 1024));
    }
    Ok(bytes)
}

fn decode_master(png: &[u8]) -> Result<image::DynamicImage, String> {
    if png.len() as u64 > ICON_MAX_BYTES {
        return Err(format!(
            "extension icon is larger than {} KB",
            ICON_MAX_BYTES / 1024
        ));
    }
    // Inspect dimensions before decoding pixels. A tiny PNG header can claim a
    // huge canvas, so checking only after `load_from_memory` would let a dev
    // project force an oversized allocation despite the compressed-byte cap.
    let (width, height) =
        image::ImageReader::with_format(std::io::Cursor::new(png), image::ImageFormat::Png)
            .into_dimensions()
            .map_err(|error| format!("inspect extension icon: {error}"))?;
    if width != ICON_MASTER_DIM || height != ICON_MASTER_DIM {
        return Err(format!(
            "extension icon is {width}×{height}; expected {ICON_MASTER_DIM}×{ICON_MASTER_DIM}"
        ));
    }
    image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|error| format!("decode extension icon: {error}"))
}

fn derive_pill_rgba_from(image: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let (width, height) = image.dimensions();
    let rgba = image.to_rgba8();
    crate::pill_icon::to_icon(rgba.as_raw(), width as usize, height as usize)
        .ok_or_else(|| "could not derive extension icon".to_string())
}

#[cfg(test)]
fn derive_pill_rgba(png: &[u8]) -> Result<Vec<u8>, String> {
    derive_pill_rgba_from(&decode_master(png)?)
}

fn derive_ui_png_from(image: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let scaled = image.resize_exact(
        UI_ICON_DIM,
        UI_ICON_DIM,
        image::imageops::FilterType::Lanczos3,
    );
    let mut bytes = std::io::Cursor::new(Vec::new());
    scaled
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| format!("encode extension UI icon: {error}"))?;
    Ok(bytes.into_inner())
}

fn derive_ui_png(png: &[u8]) -> Result<Vec<u8>, String> {
    derive_ui_png_from(&decode_master(png)?)
}

fn is_ui_png(bytes: &[u8]) -> bool {
    image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png)
        .into_dimensions()
        .is_ok_and(|dimensions| dimensions == (UI_ICON_DIM, UI_ICON_DIM))
}

/// Decode and persist a pack's master + fixed pill derivative. Missing artwork
/// is allowed for pre-icon-contract packs; malformed embedded artwork is not.
pub fn materialize_pack(app: &AppHandle, pack: &GrainPack) -> Result<(), String> {
    let _guard = ASSET_LOCK
        .lock()
        .map_err(|_| "extension icon cache lock is unavailable")?;
    let Some(png) = pack.embedded_icon_png()? else {
        // A legacy/iconless update must not leave an older version's artwork
        // looking as though it belonged to the newly installed pack.
        purge_unlocked(app, &pack.manifest.id)?;
        return Ok(());
    };
    let image = decode_master(&png)?;
    let rgba = derive_pill_rgba_from(&image)?;
    let ui_png = derive_ui_png_from(&image)?;
    // One installed version is active at a time. Replacing its tiny asset tree
    // keeps repeated extension updates constant-space on disk.
    purge_unlocked(app, &pack.manifest.id)?;
    let dir = version_dir(app, &pack.manifest.id, &pack.manifest.version)?;
    write_atomic(&dir.join(MASTER_FILE), &png)?;
    write_atomic(&dir.join(PILL_FILE), &rgba)?;
    write_atomic(&dir.join(UI_FILE), &ui_png)
}

fn safe_dev_icon(root: &Path, declared: &str) -> Option<PathBuf> {
    let relative = Path::new(declared);
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let path = root.join(relative).canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    path.starts_with(&root).then_some(path)
}

fn dev_cache_root(app: &AppHandle) -> Result<PathBuf, String> {
    let parent = assets_root(app)?;
    std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let root = parent.join(DEV_CACHE_DIRECTORY);
    if root.exists() {
        let metadata = std::fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("extension dev-icon cache is not a regular directory".into());
        }
    } else {
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    }
    let parent = parent.canonicalize().map_err(|error| error.to_string())?;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    if root.parent() != Some(parent.as_path())
        || root.file_name().and_then(|name| name.to_str()) != Some(DEV_CACHE_DIRECTORY)
    {
        return Err("extension dev-icon cache escaped its fixed asset directory".into());
    }
    Ok(root)
}

fn dev_ui_icon(app: &AppHandle, root: &Path, declared: &str) -> Option<Vec<u8>> {
    let source = safe_dev_icon(root, declared)?;
    // Bounded streaming read prevents a load-unpacked project from making the
    // host allocate an arbitrary-sized file before the validator runs.
    let png = read_bounded(&source, ICON_MAX_BYTES).ok()?;
    let digest = format!("{:x}", Sha256::digest(&png));
    let _guard = ASSET_LOCK.lock().ok()?;
    let cache = dev_cache_root(app).ok()?.join(format!("{digest}.png"));
    if let Ok(bytes) = read_bounded(&cache, ICON_MAX_BYTES) {
        if is_ui_png(&bytes) {
            return Some(bytes);
        }
    }
    let derived = derive_ui_png(&png).ok()?;
    write_atomic(&cache, &derived).ok()?;
    Some(derived)
}

/// Small PNG data URL for installed cards/settings. The 512² master remains on
/// disk; settings receives only a transient 128² derivative.
pub fn ui_icon(app: &AppHandle, id: &str) -> Option<String> {
    grain_sdk::validate_extension_id(id).ok()?;
    let bytes = if let Some(registry) =
        app.try_state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>()
    {
        if let Some(root) = registry.dev_path(id) {
            let pack = crate::extension_host::load_manifest(app, id)?;
            dev_ui_icon(app, &root, &pack.manifest.icon)?
        } else {
            installed_ui_icon(app, id)?
        }
    } else {
        installed_ui_icon(app, id)?
    };
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Dev derivatives live only for the current developer-mode lifetime. They are
/// content-addressed, so fixtures sharing one master produce one cached PNG.
pub fn purge_dev_cache(app: &AppHandle) -> Result<(), String> {
    let parent = assets_root(app)?;
    let root = parent.join(DEV_CACHE_DIRECTORY);
    if !root.exists() {
        return Ok(());
    }
    let _guard = ASSET_LOCK
        .lock()
        .map_err(|_| "extension icon cache lock is unavailable")?;
    let metadata = std::fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("refusing to remove a non-directory extension dev-icon cache".into());
    }
    let parent = parent.canonicalize().map_err(|error| error.to_string())?;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    if root.parent() != Some(parent.as_path())
        || root.file_name().and_then(|name| name.to_str()) != Some(DEV_CACHE_DIRECTORY)
    {
        return Err("refusing to remove an extension dev-icon cache outside assets".into());
    }
    std::fs::remove_dir_all(root).map_err(|error| error.to_string())
}

fn installed_ui_icon(app: &AppHandle, id: &str) -> Option<Vec<u8>> {
    let pack = crate::extension_host::load_manifest(app, id)?;
    let dir = version_dir(app, id, &pack.manifest.version).ok()?;
    let read_valid = || {
        let bytes = read_bounded(&dir.join(UI_FILE), ICON_MAX_BYTES).ok()?;
        is_ui_png(&bytes).then_some(bytes)
    };
    read_valid().or_else(|| {
        materialize_pack(app, &pack).ok()?;
        read_valid()
    })
}

/// Remove derived assets only when the caller explicitly purges an extension.
pub fn purge(app: &AppHandle, id: &str) -> Result<(), String> {
    let _guard = ASSET_LOCK
        .lock()
        .map_err(|_| "extension icon cache lock is unavailable")?;
    purge_unlocked(app, id)
}

fn purge_unlocked(app: &AppHandle, id: &str) -> Result<(), String> {
    grain_sdk::validate_extension_id(id)?;
    let target = assets_root(app)?.join(id);
    if target.exists() {
        std::fs::remove_dir_all(target).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_truncated_png_header_is_not_materialisable() {
        let mut header = vec![0u8; 24];
        header[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        header[12..16].copy_from_slice(b"IHDR");
        header[16..20].copy_from_slice(&ICON_MASTER_DIM.to_be_bytes());
        header[20..24].copy_from_slice(&ICON_MASTER_DIM.to_be_bytes());
        assert!(derive_pill_rgba(&header).is_err());
    }

    #[test]
    fn a_dev_icon_is_bounded_before_it_is_allocated() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("huge.png");
        std::fs::write(&path, vec![0u8; ICON_MAX_BYTES as usize + 1]).unwrap();
        assert!(read_bounded(&path, ICON_MAX_BYTES).is_err());
    }
}
