//! Extension icon materialisation and recommendation delivery.
//!
//! Authors ship one verified 512² PNG master. The host writes that master to
//! the installed asset cache and derives the pill's one fixed, premultiplied
//! 64² RGBA payload. Only the tiny derived file is read during recommendation;
//! decoded masters are never retained in process state.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use grain_core::AppContext;
use grain_sdk::{GrainPack, ICON_MASTER_DIM, ICON_MAX_BYTES};
use image::GenericImageView as _;
use tauri::{AppHandle, Manager};

const MASTER_FILE: &str = "icon.png";
const PILL_FILE: &str = "pill.rgba";
const UI_FILE: &str = "ui.png";
const UI_ICON_DIM: u32 = 128;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

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
    let parent = path.parent().ok_or("icon cache path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".icon-{}-{suffix}.tmp", std::process::id()));
    std::fs::write(&temp, bytes).map_err(|error| error.to_string())?;
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

fn decode_master(png: &[u8]) -> Result<image::DynamicImage, String> {
    if png.len() as u64 > ICON_MAX_BYTES {
        return Err(format!(
            "extension icon is larger than {} KB",
            ICON_MAX_BYTES / 1024
        ));
    }
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|error| format!("decode extension icon: {error}"))?;
    let (width, height) = image.dimensions();
    if width != ICON_MASTER_DIM || height != ICON_MASTER_DIM {
        return Err(format!(
            "extension icon is {width}×{height}; expected {ICON_MASTER_DIM}×{ICON_MASTER_DIM}"
        ));
    }
    Ok(image)
}

fn derive_pill_rgba_from(image: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let (width, height) = image.dimensions();
    let rgba = image.to_rgba8();
    crate::pill_icon::to_icon(rgba.as_raw(), width as usize, height as usize)
        .ok_or_else(|| "could not derive extension icon".to_string())
}

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

/// Decode and persist a pack's master + fixed pill derivative. Missing artwork
/// is allowed for pre-icon-contract packs; malformed embedded artwork is not.
pub fn materialize_pack(app: &AppHandle, pack: &GrainPack) -> Result<(), String> {
    let Some(png) = pack.embedded_icon_png()? else {
        // A legacy/iconless update must not leave an older version's artwork
        // looking as though it belonged to the newly installed pack.
        purge(app, &pack.manifest.id)?;
        return Ok(());
    };
    let image = decode_master(&png)?;
    let rgba = derive_pill_rgba_from(&image)?;
    let ui_png = derive_ui_png_from(&image)?;
    // One installed version is active at a time. Replacing its tiny asset tree
    // keeps repeated extension updates constant-space on disk.
    purge(app, &pack.manifest.id)?;
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

fn dev_icon(app: &AppHandle, id: &str, root: &Path) -> Option<Vec<u8>> {
    let pack = crate::extension_host::load_manifest(app, id)?;
    let path = safe_dev_icon(root, &pack.manifest.icon)?;
    let bytes = std::fs::read(path).ok()?;
    derive_pill_rgba(&bytes).ok()
}

/// Base64 fixed-size RGBA for the recommendation wire. Installed extensions
/// hit only the 16 KiB derived cache; developer projects are decoded from their
/// live source so hot reloads never show stale artwork.
pub fn recommendation_icon(app: &AppHandle, id: &str) -> Option<String> {
    grain_sdk::validate_extension_id(id).ok()?;
    if let Some(registry) =
        app.try_state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>()
    {
        if let Some(root) = registry.dev_path(id) {
            return dev_icon(app, id, &root)
                .map(|rgba| base64::engine::general_purpose::STANDARD.encode(rgba));
        }
    }

    let pack = crate::extension_host::load_manifest(app, id)?;
    let dir = version_dir(app, id, &pack.manifest.version).ok()?;
    let rgba = std::fs::read(dir.join(PILL_FILE))
        .ok()
        .filter(|bytes| bytes.len() == crate::pill_icon::ICON_BYTES)
        .or_else(|| {
            materialize_pack(app, &pack).ok()?;
            std::fs::read(dir.join(PILL_FILE))
                .ok()
                .filter(|bytes| bytes.len() == crate::pill_icon::ICON_BYTES)
        })?;
    Some(base64::engine::general_purpose::STANDARD.encode(rgba))
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
            let path = safe_dev_icon(&root, &pack.manifest.icon)?;
            derive_ui_png(&std::fs::read(path).ok()?).ok()?
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

fn installed_ui_icon(app: &AppHandle, id: &str) -> Option<Vec<u8>> {
    let pack = crate::extension_host::load_manifest(app, id)?;
    let dir = version_dir(app, id, &pack.manifest.version).ok()?;
    let read_valid = || {
        let bytes = std::fs::read(dir.join(UI_FILE)).ok()?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?;
        (image.dimensions() == (UI_ICON_DIM, UI_ICON_DIM)).then_some(bytes)
    };
    read_valid().or_else(|| {
        materialize_pack(app, &pack).ok()?;
        read_valid()
    })
}

/// Remove derived assets only when the caller explicitly purges an extension.
pub fn purge(app: &AppHandle, id: &str) -> Result<(), String> {
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
}
