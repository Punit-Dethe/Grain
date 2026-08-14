//! [GRAIN] Pill identity — the icon of whatever the user is dictating into.
//!
//! See `docs/Pill Identity/PLAN.md` for the full design and the sources behind
//! the Windows API choices. The short version:
//!
//! - **The core resolves, the pill blits.** The pill gets `PILL_ICON_PX`²
//!   premultiplied RGBA and never learns about COM, the shell, or icon themes.
//!   That seam is what makes macOS and Linux one impl each rather than a
//!   redesign.
//! - **Resolution never blocks a recording.** Session start does a cache lookup
//!   only; a miss shows the plain dot and resolves on a worker, so a cold app
//!   costs exactly one dot-only session.
//! - **A wrong icon is worse than no icon.** Every rung falls through to `None`
//!   rather than guessing, because the icon is a *claim* about what Grain
//!   understands.
//!
//! Websites (favicons) are deliberately NOT here yet — that rung is the first
//! one that touches the network and is being specced separately.

use std::path::PathBuf;
use std::sync::Arc;

use grain_core::AppContext;
use grain_sdk::{DaemonEvent, PILL_ICON_PX};
use tauri::{AppHandle, Manager};

/// Bytes in one icon: fixed, so a cache file needs no header and the wire
/// payload needs no dimensions.
pub const ICON_BYTES: usize = PILL_ICON_PX * PILL_ICON_PX * 4;

/// Bumped whenever the stored format or the resolution ladder changes in a way
/// that would make previously cached pixels wrong. Folded into the cache key, so
/// a bump invalidates everything at once without a migration.
const CACHE_SCHEMA: u32 = 1;

/// What we want a picture of. Deliberately NOT an exe path: on macOS `Id` is a
/// bundle identifier and on Linux a desktop-file id, so the platform-specific
/// mess of turning a window into an identity stays inside the platform impl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconKey {
    /// A launchable path: Windows `.exe`, Linux binary, macOS `.app` bundle.
    Path(PathBuf),
    /// A platform-native application identity (Windows: AUMID).
    Id(String),
}

impl IconKey {
    /// Cache identity. Folds in the schema version and, for a path, the file's
    /// size + mtime — so updating an app refreshes its icon without anyone
    /// having to remember to invalidate.
    fn cache_name(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        CACHE_SCHEMA.hash(&mut h);
        match self {
            IconKey::Path(p) => {
                0u8.hash(&mut h);
                p.hash(&mut h);
                if let Ok(md) = std::fs::metadata(p) {
                    md.len().hash(&mut h);
                    if let Ok(t) = md.modified() {
                        if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                            d.as_secs().hash(&mut h);
                        }
                    }
                }
            }
            IconKey::Id(id) => {
                1u8.hash(&mut h);
                id.hash(&mut h);
            }
        }
        format!("{:016x}", h.finish())
    }
}

fn cache_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = crate::portable::app_data_dir(app).ok()?.join("app-icons");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn cache_read(app: &AppHandle, key: &IconKey) -> Option<Vec<u8>> {
    let path = cache_dir(app)?.join(key.cache_name());
    let bytes = std::fs::read(path).ok()?;
    // A truncated file (killed mid-write) is a miss, never a garbled icon.
    (bytes.len() == ICON_BYTES).then_some(bytes)
}

fn cache_write(app: &AppHandle, key: &IconKey, rgba: &[u8]) {
    if rgba.len() != ICON_BYTES {
        return;
    }
    let Some(dir) = cache_dir(app) else { return };
    // Write-then-rename so a reader never sees a half-written icon.
    let final_path = dir.join(key.cache_name());
    let tmp = dir.join(format!("{}.tmp", key.cache_name()));
    if std::fs::write(&tmp, rgba).is_ok() && std::fs::rename(&tmp, &final_path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// The key for the app currently in the foreground, or `None` when we cannot
/// name it. Websites are not handled yet — a browser resolves to the browser's
/// own icon, which is the correct fallback in the final design too.
pub fn foreground_key() -> Option<IconKey> {
    #[cfg(windows)]
    {
        if let Some(aumid) = windows_impl::foreground_aumid() {
            return Some(IconKey::Id(aumid));
        }
    }
    let ctx = crate::context_detect::detect_active_context(false, false)?;
    if ctx.exe_path.is_empty() {
        return None;
    }
    Some(IconKey::Path(PathBuf::from(ctx.exe_path)))
}

/// Resolve to `PILL_ICON_PX`² premultiplied RGBA. Slow (COM + shell + disk) —
/// callers must be on a worker, never on a path a user is waiting on.
pub fn resolve(key: &IconKey) -> Option<Vec<u8>> {
    #[cfg(windows)]
    {
        return windows_impl::resolve(key);
    }
    // macOS: NSWorkspace icon(forFile:) / urlForApplication(withBundleIdentifier:).
    // Linux: desktop-file id → Icon= → freedesktop icon-theme lookup.
    // Both slot in here without touching anything above this function.
    #[cfg(not(windows))]
    {
        let _ = key;
        None
    }
}

/// Announce the foreground app's icon to the pill.
///
/// Cache hit → emitted inline, in the same breath as the rest of session start.
/// Miss → emits nothing now (the pill keeps its dot) and resolves on a worker,
/// emitting late if it succeeds. Recording never waits for pixels.
pub fn emit_for_session(app: &AppHandle) {
    if !crate::settings::get_settings(app).pill_show_app_icon {
        // Clear any icon a previous session left on the pill.
        emit(app, None);
        return;
    }
    let Some(key) = foreground_key() else {
        emit(app, None);
        return;
    };
    if let Some(rgba) = cache_read(app, &key) {
        emit(app, Some(encode(&rgba)));
        return;
    }
    // Cold. Show the dot now, resolve behind the session.
    emit(app, None);
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(rgba) = resolve(&key) {
            cache_write(&app, &key, &rgba);
            emit(&app, Some(encode(&rgba)));
        }
    });
}

fn encode(rgba: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(rgba)
}

fn emit(app: &AppHandle, rgba: Option<String>) {
    if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
        ctx.emit(DaemonEvent::PillIcon { rgba });
    }
}

// ── Windows ─────────────────────────────────────────────────────────────────

#[cfg(windows)]
mod windows_impl {
    use super::{IconKey, ICON_BYTES};
    use grain_sdk::PILL_ICON_PX;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    /// Ask the Shell for a bigger bitmap than we need and downscale ourselves.
    /// The Shell's own fit is a GDI stretch blit; at pill sizes the difference
    /// between that and a real filter is the whole look. (The docs explicitly
    /// suggest this pairing with SIIGBF_BIGGERSIZEOK.)
    const REQUEST_PX: i32 = 96;

    /// The foreground process's AppUserModelID, when it is a packaged (MSIX /
    /// Store) app. `None` for classic Win32 — which is the common case and not
    /// an error.
    ///
    /// This rung matters more than it looks: a packaged app's foreground `.exe`
    /// is often a stub with no usable icon resource, and the resource-level APIs
    /// cannot see the real asset at all.
    pub fn foreground_aumid() -> Option<String> {
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            // AUMIDs are bounded well below this; the call reports the needed
            // length, but one generous buffer avoids the two-call dance.
            let mut len = 512u32;
            let mut buf = vec![0u16; len as usize];
            let rc = GetApplicationUserModelId(
                handle,
                &mut len,
                Some(windows::core::PWSTR(buf.as_mut_ptr())),
            );
            let _ = CloseHandle(handle);
            if rc.is_err() {
                return None; // APPMODEL_ERROR_NO_APPLICATION → classic Win32
            }
            let len = (len as usize).min(buf.len()).saturating_sub(1);
            let s = String::from_utf16_lossy(&buf[..len]);
            (!s.is_empty()).then_some(s)
        }
    }

    pub fn resolve(key: &IconKey) -> Option<Vec<u8>> {
        // The parsing name is the ONLY difference between the two app kinds:
        // packaged apps live in the shell's AppsFolder namespace, which is
        // exactly what the icon-resource APIs cannot reach.
        let parsing_name = match key {
            IconKey::Id(aumid) => format!("shell:AppsFolder\\{aumid}"),
            IconKey::Path(p) => p.to_string_lossy().into_owned(),
        };
        if parsing_name.is_empty() {
            return None;
        }

        unsafe {
            // Our own thread, so our own apartment. The UIA init in
            // `context_detect` is on a different thread and does not cover this.
            let co = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let bgra = shell_bitmap(&parsing_name);
            if co.is_ok() {
                CoUninitialize();
            }
            bgra
        }
    }

    /// `IShellItemImageFactory::GetImage` → premultiplied RGBA at `PILL_ICON_PX`.
    unsafe fn shell_bitmap(parsing_name: &str) -> Option<Vec<u8>> {
        let wide = HSTRING::from(parsing_name);
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;

        let size = windows::Win32::Foundation::SIZE {
            cx: REQUEST_PX,
            cy: REQUEST_PX,
        };
        // ICONONLY: we want the app's identity, never a document thumbnail —
        // without it a text editor would show a picture of the open file.
        let hbm = factory
            .GetImage(size, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
            .ok()?;

        let out = bitmap_to_rgba(hbm);
        // The caller owns the bitmap (documented) — free it on every path.
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        let (src, w, h) = out?;
        Some(downscale(&src, w, h))
    }

    /// HBITMAP → straight-alpha RGBA plus its dimensions.
    unsafe fn bitmap_to_rgba(hbm: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<(Vec<u8>, usize, usize)> {
        let mut bm = BITMAP::default();
        let n = GetObjectW(
            HGDIOBJ(hbm.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut _ as *mut _),
        );
        if n == 0 || bm.bmWidth <= 0 || bm.bmHeight == 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth as usize, bm.bmHeight.unsigned_abs() as usize);

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bm.bmWidth,
                // Negative = top-down, so row 0 is the top and we never flip.
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut buf = vec![0u8; w * h * 4];
        let screen = GetDC(None);
        let rows = GetDIBits(
            screen,
            hbm,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, screen);
        if rows == 0 {
            return None;
        }

        // GDI hands back BGRA. Whether the shell's alpha is premultiplied is not
        // documented either way, so rather than trust one reading we detect it:
        // premultiplied data can never have a channel above its own alpha.
        let premultiplied = buf
            .chunks_exact(4)
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
        // A fully opaque icon (no alpha channel at all) reads as "premultiplied"
        // above but is really just opaque — treat a wholly-zero alpha plane as
        // opaque, which is what 24-bit icon art produces.
        let any_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);

        let mut rgba = vec![0u8; w * h * 4];
        for (dst, src) in rgba.chunks_exact_mut(4).zip(buf.chunks_exact(4)) {
            let a = if any_alpha { src[3] } else { 255 };
            if premultiplied && any_alpha {
                // Un-premultiply to a straight-alpha working buffer; `downscale`
                // re-premultiplies once, at the end.
                let un = |c: u8| {
                    if a == 0 {
                        0
                    } else {
                        ((c as u32 * 255) / a as u32).min(255) as u8
                    }
                };
                dst[0] = un(src[2]);
                dst[1] = un(src[1]);
                dst[2] = un(src[0]);
            } else {
                dst[0] = src[2];
                dst[1] = src[1];
                dst[2] = src[0];
            }
            dst[3] = a;
        }
        Some((rgba, w, h))
    }

    /// Box-filter down to `PILL_ICON_PX` and premultiply.
    ///
    /// A box filter (not the 2×2 tap a bilinear blit uses) because this is a
    /// ~3× reduction — bilinear would drop most of the source pixels and the
    /// result would shimmer. It runs once per app, not per frame, so the cost is
    /// irrelevant and the quality is not.
    fn downscale(src: &[u8], w: usize, h: usize) -> Vec<u8> {
        let n = PILL_ICON_PX;
        let mut out = vec![0u8; ICON_BYTES];
        for oy in 0..n {
            let y0 = oy * h / n;
            let y1 = (((oy + 1) * h).div_ceil(n)).max(y0 + 1).min(h);
            for ox in 0..n {
                let x0 = ox * w / n;
                let x1 = (((ox + 1) * w).div_ceil(n)).max(x0 + 1).min(w);
                let (mut r, mut g, mut b, mut a, mut count) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let p = &src[(y * w + x) * 4..][..4];
                        // Weight colour by alpha so transparent pixels do not
                        // drag the average toward black at the icon's edges.
                        let pa = p[3] as u32;
                        r += p[0] as u32 * pa;
                        g += p[1] as u32 * pa;
                        b += p[2] as u32 * pa;
                        a += pa;
                        count += 1;
                    }
                }
                let d = &mut out[(oy * n + ox) * 4..][..4];
                if count == 0 || a == 0 {
                    continue; // fully transparent
                }
                let mean_a = a / count;
                // Straight-alpha mean, then premultiply — which is exactly
                // (sum of colour·alpha) / (count·255).
                d[0] = (r / count / 255) as u8;
                d[1] = (g / count / 255) as u8;
                d[2] = (b / count / 255) as u8;
                d[3] = mean_a as u8;
            }
        }
        out
    }

}
