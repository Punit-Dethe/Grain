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
//! - **Websites resolve against a REGISTRY, not the open web.** Grain shows a
//!   site's icon only for hosts in [`crate::context_detect`]'s site table — the
//!   same table that decides which sites get their own post-processing profile.
//!   That registry stores site *identities*, never site *assets*: adding a site
//!   is one row, and its logo arrives (and re-arrives after a rebrand) by
//!   itself, instead of a checked-in PNG that silently goes stale.
//!
//!   It is also the security story. Grain never fetches an icon for whatever URL
//!   the foreground window happened to be showing — only for a host that matched
//!   a table compiled into the binary — so the general-purpose favicon resolver's
//!   SSRF surface simply does not exist here.

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
const CACHE_SCHEMA: u32 = 2;

/// What we want a picture of. Deliberately NOT an exe path: on macOS `Id` is a
/// bundle identifier and on Linux a desktop-file id, so the platform-specific
/// mess of turning a window into an identity stays inside the platform impl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconKey {
    /// A launchable path: Windows `.exe`, Linux binary, macOS `.app` bundle.
    Path(PathBuf),
    /// A platform-native application identity (Windows: AUMID).
    Id(String),
    /// A **supported** website, keyed by host. Every page on a host collapses to
    /// one key — `claude.ai/new`, `/chat/123` and `/settings` are all `claude.ai`
    /// — so a site is resolved once and never again, and the cache does not grow
    /// with the number of pages visited.
    Site(String),
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
            IconKey::Site(host) => {
                2u8.hash(&mut h);
                host.hash(&mut h);
            }
        }
        format!("{:016x}", h.finish())
    }
}

/// [GRAIN] The icon key for a browser address-bar host, or `None` when the site
/// is not one Grain supports.
///
/// The gate is [`crate::context_detect::category_for_site`] — the SAME registry
/// that decides which sites get their own post-processing profile. That is the
/// whole design: the registry stores site *identities*, never site *assets*.
/// Adding Perplexity is one row there, and its logo arrives by itself; nobody
/// checks in a `perplexity.png` that goes stale at the next rebrand.
///
/// It is also what keeps this safe. Grain never fetches from an arbitrary URL
/// the foreground window happened to be showing — only from a host that matched
/// a table compiled into the binary.
pub fn site_key(host: &str) -> Option<IconKey> {
    let host = host.trim().trim_start_matches("www.").to_ascii_lowercase();
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    crate::context_detect::category_for_site(&host)?;
    Some(IconKey::Site(host))
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

/// Both identities the foreground window can carry: the app, and — when it is a
/// browser sitting on a supported site — that site.
///
/// One detection round-trip serves both. The URL read is the expensive part (UI
/// Automation), and `detect_active_context` already performs it for browsers, so
/// asking separately would pay for it twice.
fn foreground_keys() -> (Option<IconKey>, Option<IconKey>) {
    let ctx = crate::context_detect::detect_active_context(false, false);
    let site = ctx
        .as_ref()
        .and_then(|c| c.url_host.as_deref())
        .and_then(site_key);

    #[cfg(windows)]
    if let Some(aumid) = windows_impl::foreground_aumid() {
        return (Some(IconKey::Id(aumid)), site);
    }
    let app = ctx
        .as_ref()
        .filter(|c| !c.exe_path.is_empty())
        .map(|c| IconKey::Path(PathBuf::from(&c.exe_path)));
    (app, site)
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

/// Announce the foreground surface's icon to the pill.
///
/// Two rungs, best-first: a supported WEBSITE outranks the browser showing it,
/// because "Grain knows you are in Gmail" is a stronger claim than "Grain knows
/// you are in Chrome".
///
/// Nothing here blocks a recording. Whatever is already cached is emitted
/// immediately; anything missing resolves behind the session and is emitted late
/// if it succeeds. That also gives the stale-while-revalidate shape for free: on
/// a site whose icon is not cached yet, the browser's own icon shows at once and
/// is replaced the moment the site's arrives.
pub fn emit_for_session(app: &AppHandle) {
    if !crate::settings::get_settings(app).pill_show_app_icon {
        // Clear any icon a previous session left on the pill.
        emit(app, None);
        return;
    }
    let (app_key, site) = foreground_keys();

    // Show the best thing already on disk, right now.
    let cached = site
        .as_ref()
        .and_then(|k| cache_read(app, k))
        .or_else(|| app_key.as_ref().and_then(|k| cache_read(app, k)));
    let site_cached = cached.is_some() && site.is_some();
    emit(app, cached.as_deref().map(encode));

    // The app icon, when it is what the pill will end up showing. Skipped when a
    // site is in play — that icon wins, and resolving this one would only risk
    // landing after it and stealing the slot.
    if site.is_none() {
        if let Some(key) = app_key {
            if cached.is_none() {
                let app = app.clone();
                // Its own thread: the Windows resolver initialises COM.
                std::thread::spawn(move || {
                    if let Some(rgba) = resolve(&key) {
                        cache_write(&app, &key, &rgba);
                        emit(&app, Some(encode(&rgba)));
                    }
                });
            }
        }
        return;
    }

    // A supported site with no cached icon: fetch it, then upgrade the pill.
    if let (Some(IconKey::Site(host)), false) = (site.clone(), site_cached) {
        let key = site.unwrap();
        let app = app.clone();
        // The async runtime rather than a thread — this rung is network-bound,
        // and the app already runs a reactor.
        tauri::async_runtime::spawn(async move {
            if let Some(rgba) = site_fetch::resolve(&host).await {
                cache_write(&app, &key, &rgba);
                emit(&app, Some(encode(&rgba)));
            }
        });
    }
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

/// Box-filter straight-alpha RGBA (`w`×`h`) into the `PILL_ICON_PX` square of
/// PREMULTIPLIED RGBA the pill expects.
///
/// A box filter, not the 2×2 tap a bilinear blit uses, because this is a large
/// reduction (a 180px touch icon down to 64) — bilinear would drop most of the
/// source pixels and the result would shimmer. It runs once per app or site,
/// never per frame, so the cost is irrelevant and the quality is not.
///
/// Non-square art is letterboxed, never stretched: a squashed logo reads as
/// broken in a way a slightly smaller one never does.
fn to_icon(src: &[u8], w: usize, h: usize) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return None;
    }
    let n = PILL_ICON_PX;
    let (bw, bh) = if w >= h {
        (n, (h * n / w).max(1))
    } else {
        ((w * n / h).max(1), n)
    };
    let (ox, oy) = ((n - bw) / 2, (n - bh) / 2);

    let mut out = vec![0u8; ICON_BYTES];
    for by in 0..bh {
        let y0 = by * h / bh;
        let y1 = (((by + 1) * h).div_ceil(bh)).max(y0 + 1).min(h);
        for bx in 0..bw {
            let x0 = bx * w / bw;
            let x1 = (((bx + 1) * w).div_ceil(bw)).max(x0 + 1).min(w);
            let (mut r, mut g, mut b, mut a, mut count) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = &src[(y * w + x) * 4..][..4];
                    // Weight colour by alpha so transparent pixels do not drag
                    // the average toward black at the icon's edges.
                    let pa = p[3] as u32;
                    r += p[0] as u32 * pa;
                    g += p[1] as u32 * pa;
                    b += p[2] as u32 * pa;
                    a += pa;
                    count += 1;
                }
            }
            if count == 0 || a == 0 {
                continue; // fully transparent
            }
            let d = &mut out[((oy + by) * n + (ox + bx)) * 4..][..4];
            // Straight-alpha mean, then premultiply — which is exactly
            // (sum of colour·alpha) / (count·255).
            d[0] = (r / count / 255) as u8;
            d[1] = (g / count / 255) as u8;
            d[2] = (b / count / 255) as u8;
            d[3] = (a / count) as u8;
        }
    }
    Some(out)
}

// ── Websites ────────────────────────────────────────────────────────────────

/// [GRAIN] Fetch a supported site's own icon from the site itself.
///
/// The registry (see [`site_key`]) has already decided this host is one Grain
/// supports, so this module never sees an arbitrary URL. That is what keeps it
/// small: no SSRF apparatus, no redirect-host allowlist, no manifest parsing —
/// just HTTPS, a cap, a timeout, and no credentials.
///
/// The ladder is deliberately short, and ordered by RESOLUTION rather than by
/// the spec's preference order. We render at 22 px from a 64 px source, so the
/// 180 px `apple-touch-icon` is worth more than a 16 px `/favicon.ico` — and
/// starting the ladder at `/favicon.ico`, as the classic advice says, is exactly
/// how you end up with the pixelation this feature was already fixed for once.
/// SVG is skipped: rasterising it would mean a vector stack for a 22 px glyph.
mod site_fetch {
    use futures_util::StreamExt;
    use std::time::Duration;

    /// Enough for a `<head>`; icon links live near the top of the document.
    const HTML_CAP: usize = 192 * 1024;
    const ICON_CAP: usize = 1024 * 1024;
    const TIMEOUT: Duration = Duration::from_secs(6);
    /// Bounded so a site that lists a dozen icons cannot turn one session into a
    /// dozen requests.
    const MAX_TRIES: usize = 4;

    pub async fn resolve(host: &str) -> Option<Vec<u8>> {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(4))
            // Identify honestly. Some sites serve a different (or no) icon to a
            // client that looks like a scraper.
            .user_agent(concat!("Grain/", env!("CARGO_PKG_VERSION")))
            .build()
            .ok()?;
        let origin = format!("https://{host}");

        for url in candidates(&client, &origin).await.into_iter().take(MAX_TRIES) {
            let Some(bytes) = get(&client, &url, ICON_CAP).await else {
                continue;
            };
            if let Some(icon) = decode(&bytes) {
                return Some(icon);
            }
        }
        None
    }

    /// Icon URLs to try, best first: whatever the page declares (largest first),
    /// then the well-known path as a last resort.
    async fn candidates(client: &reqwest::Client, origin: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(html) = get(client, &format!("{origin}/"), HTML_CAP)
            .await
            .and_then(|b| String::from_utf8(b).ok())
        {
            out = declared_icons(&html, origin);
        }
        out.push(format!("{origin}/favicon.ico"));
        out
    }

    /// Scan `<link rel="...icon...">` tags, newest-spec attributes included, and
    /// return their hrefs ordered largest-declared-size first.
    ///
    /// A hand-rolled scan rather than an HTML parser: this reads four attributes
    /// out of one tag name, and pulling in a full parser (and its allocator
    /// behaviour) for that would cost more than it explains.
    fn declared_icons(html: &str, origin: &str) -> Vec<String> {
        // `to_ascii_lowercase` is length-preserving, so offsets found in the
        // lowered copy index the original correctly — which matters because URLs
        // are case-sensitive and must be taken from the original.
        let hay = html.to_ascii_lowercase();
        let mut found: Vec<(u32, String)> = Vec::new();

        let mut at = 0usize;
        while let Some(rel) = hay[at..].find("<link").map(|i| at + i) {
            let end = hay[rel..].find('>').map(|i| rel + i).unwrap_or(hay.len());
            let tag_lo = &hay[rel..end];
            let tag_hi = &html[rel..end];
            at = end.max(rel + 5);

            let Some(rel_val) = attr(tag_lo, tag_hi, "rel") else {
                continue;
            };
            let rel_val = rel_val.to_ascii_lowercase();
            if !rel_val.split_whitespace().any(|w| w == "icon" || w.ends_with("-icon")) {
                continue;
            }
            let Some(href) = attr(tag_lo, tag_hi, "href") else {
                continue;
            };
            // A vector icon would need a rasteriser we deliberately do not carry.
            let ty = attr(tag_lo, tag_hi, "type").unwrap_or_default();
            if href.trim_end().to_ascii_lowercase().ends_with(".svg")
                || ty.to_ascii_lowercase().contains("svg")
            {
                continue;
            }
            let Some(url) = absolutise(href.trim(), origin) else {
                continue;
            };

            // Rank: the declared pixel size, or a floor per rel kind. An
            // apple-touch-icon is 180 px by convention even when unlabelled,
            // which is why it outranks an unlabelled `icon`.
            let px = attr(tag_lo, tag_hi, "sizes")
                .and_then(|s| {
                    s.to_ascii_lowercase()
                        .split(['x', ' '])
                        .filter_map(|p| p.trim().parse::<u32>().ok())
                        .max()
                })
                .unwrap_or(if rel_val.contains("apple-touch") { 180 } else { 32 });
            found.push((px, url));
        }

        found.sort_by(|a, b| b.0.cmp(&a.0));
        found.dedup_by(|a, b| a.1 == b.1);
        found.into_iter().map(|(_, u)| u).collect()
    }

    /// Read `name="…"` (or `name='…'`) out of one tag. `lo` is the lowercased
    /// tag for matching, `hi` the original for the value.
    fn attr(lo: &str, hi: &str, name: &str) -> Option<String> {
        let mut from = 0usize;
        loop {
            let i = lo[from..].find(name).map(|i| from + i)?;
            from = i + name.len();
            // Must be a whole attribute name, not a substring of another.
            let before_ok = lo[..i]
                .chars()
                .next_back()
                .is_none_or(|c| c.is_whitespace() || c == '<');
            let rest = lo[from..].trim_start();
            if !before_ok || !rest.starts_with('=') {
                continue;
            }
            let eq = from + lo[from..].find('=')?;
            let val = hi[eq + 1..].trim_start();
            let quote = val.chars().next()?;
            return if quote == '"' || quote == '\'' {
                val[1..].find(quote).map(|e| val[1..1 + e].to_string())
            } else {
                Some(
                    val.split([' ', '\t', '\r', '\n', '>'])
                        .next()
                        .unwrap_or("")
                        .to_string(),
                )
            };
        }
    }

    /// Resolve an href against the site's origin. HTTPS only — an icon is not
    /// worth a cleartext request, and `http://` here would be a downgrade on a
    /// page we reached over TLS.
    fn absolutise(href: &str, origin: &str) -> Option<String> {
        if href.is_empty() || href.starts_with("data:") {
            return None;
        }
        if let Some(rest) = href.strip_prefix("//") {
            return Some(format!("https://{rest}"));
        }
        if href.starts_with("https://") {
            return Some(href.to_string());
        }
        if href.starts_with("http://") {
            return None;
        }
        // Anything else carrying a scheme — `javascript:`, `mailto:`, `tel:` —
        // is refused outright. Testing for "://" is not enough: `javascript:x`
        // has no slashes and would otherwise be pasted onto the origin as though
        // it were a path.
        if href.split('/').next().is_some_and(|seg| seg.contains(':')) {
            return None;
        }
        Some(if let Some(path) = href.strip_prefix('/') {
            format!("{origin}/{path}")
        } else {
            format!("{origin}/{href}")
        })
    }

    /// GET with a hard byte ceiling. Streamed and counted rather than trusting
    /// `Content-Length`, which a server is free to understate or omit.
    async fn get(client: &reqwest::Client, url: &str, cap: usize) -> Option<Vec<u8>> {
        let resp = client.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        if resp.content_length().is_some_and(|n| n as usize > cap) {
            return None;
        }
        let mut buf = Vec::with_capacity(8 * 1024);
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.ok()?;
            if buf.len() + chunk.len() > cap {
                return None; // oversized: drop it rather than truncate to garbage
            }
            buf.extend_from_slice(&chunk);
        }
        (!buf.is_empty()).then_some(buf)
    }

    /// Decode PNG/ICO to the pill's icon format. Anything else — including a
    /// 404 page served with a 200 — fails here rather than becoming a smear.
    fn decode(bytes: &[u8]) -> Option<Vec<u8>> {
        let img = image::load_from_memory(bytes).ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        // A 1×1 tracking pixel or similar is not an icon.
        if w < 8 || h < 8 {
            return None;
        }
        super::to_icon(rgba.as_raw(), w as usize, h as usize)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn declared_icons_are_ranked_by_size_and_svg_is_skipped() {
            let html = r#"
                <html><head>
                <link rel="icon" href="/small.png" sizes="16x16">
                <link rel="icon" type="image/svg+xml" href="/vector.svg">
                <link rel="apple-touch-icon" href="/Touch.png">
                <link rel="shortcut icon" href="/mid.png" sizes="48x48">
                </head></html>"#;
            let got = declared_icons(html, "https://x.test");
            assert_eq!(
                got,
                vec![
                    // apple-touch (180 by convention) > 48 > 16; svg dropped.
                    "https://x.test/Touch.png".to_string(),
                    "https://x.test/mid.png".to_string(),
                    "https://x.test/small.png".to_string(),
                ],
                "ranking or filtering is wrong: {got:?}"
            );
        }

        #[test]
        fn hrefs_resolve_against_the_origin_and_refuse_downgrades() {
            let o = "https://x.test";
            assert_eq!(absolutise("/a.png", o).as_deref(), Some("https://x.test/a.png"));
            assert_eq!(absolutise("a.png", o).as_deref(), Some("https://x.test/a.png"));
            assert_eq!(
                absolutise("//cdn.test/a.png", o).as_deref(),
                Some("https://cdn.test/a.png"),
                "protocol-relative must become https, not be dropped"
            );
            assert_eq!(absolutise("https://cdn.test/a.png", o).as_deref(), Some("https://cdn.test/a.png"));
            // Refused: cleartext, data URIs, and anything exotic.
            assert_eq!(absolutise("http://x.test/a.png", o), None);
            assert_eq!(absolutise("data:image/png;base64,AAAA", o), None);
            assert_eq!(absolutise("javascript:alert(1)", o), None);
            assert_eq!(absolutise("", o), None);
        }

        #[test]
        fn an_attribute_is_matched_whole_not_as_a_substring() {
            // `data-href` must not satisfy a search for `href`.
            let tag = r#"<link rel="icon" data-href="/decoy.png" href="/real.png">"#;
            assert_eq!(attr(tag, tag, "href").as_deref(), Some("/real.png"));
        }

        #[test]
        fn a_page_declaring_nothing_yields_nothing() {
            assert!(declared_icons("<html><head></head></html>", "https://x.test").is_empty());
            // …and the caller still appends /favicon.ico, which is the point of
            // keeping that rung outside this function.
        }
    }
}

// ── Windows ─────────────────────────────────────────────────────────────────

#[cfg(windows)]
mod windows_impl {
    use super::IconKey;
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
            // A website has no shell item; it is fetched, not asked for.
            IconKey::Site(_) => return None,
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
        super::to_icon(&src, w, h)
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

}
