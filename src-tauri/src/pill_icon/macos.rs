use std::path::{Path, PathBuf};

use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
use objc2_foundation::{NSDictionary, NSString};

pub(super) fn foreground_bundle() -> Option<PathBuf> {
    autoreleasepool(|_| {
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        let url = app.bundleURL()?;
        Some(PathBuf::from(url.path()?.to_string()))
    })
}

/// Convert one system app icon on the existing icon worker. Only the 64px
/// result travels to the pill or its cache.
pub(super) fn resolve_bundle_icon(bundle: &Path) -> Option<Vec<u8>> {
    autoreleasepool(|_| {
        let icon =
            NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(bundle.to_str()?));
        let tiff = icon.TIFFRepresentation()?;
        let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
        let (width, height) = (bitmap.pixelsWide(), bitmap.pixelsHigh());
        if !(1..=1024).contains(&width) || !(1..=1024).contains(&height) {
            return None;
        }
        // An empty dictionary has no properties with incompatible types.
        let png = unsafe {
            bitmap.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &NSDictionary::new(),
            )
        }?;
        let rgba = image::load_from_memory_with_format(&png.to_vec(), image::ImageFormat::Png)
            .ok()?
            .into_rgba8();
        super::to_icon(rgba.as_raw(), rgba.width() as usize, rgba.height() as usize)
    })
}
