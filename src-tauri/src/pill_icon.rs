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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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
/// The one exception is [`site_fetch::BUNDLED`], for hosts that refuse to serve
/// their icon to any HTTP client at all. It is an exception, not a second way
/// of doing this — see the reasoning there before adding to it.
///
/// It is also what keeps this safe. Grain never fetches from an arbitrary URL
/// the foreground window happened to be showing — only from a host the USER or
/// the compiled table has named.
///
/// The user's own custom context profiles extend the registry: a site named in
/// one is a supported site, so its favicon resolves exactly like a built-in
/// row's. That keeps one promise the feature would otherwise break — you add
/// figma.com to a profile, and the pill starts showing Figma — without widening
/// the safety property at all. The set of fetchable hosts is still a finite list
/// of names, just one the user can add to; it is never "whatever tab is open".
pub fn site_key(host: &str, settings: &grain_core::AppSettings) -> Option<IconKey> {
    let host = host.trim().trim_start_matches("www.").to_ascii_lowercase();
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    if !crate::context_detect::is_supported_site(&host, settings) {
        return None;
    }
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

/// What one look at the foreground window found.
struct Foreground {
    /// The app itself: its packaged identity when it has one, else its binary.
    app: Option<IconKey>,
    /// The site, when a browser is showing one Grain supports.
    site: Option<IconKey>,
    /// A browser was in front but had not named its address yet — see
    /// [`crate::context_detect::ActiveContext::site_read_may_be_early`].
    early: bool,
}

/// Everything one detection round-trip tells the icon path.
///
/// One round-trip, because the URL read is the expensive part (UI Automation)
/// and `detect_active_context` already performs it for browsers — asking
/// separately would pay for it twice. The app's packaged identity now rides
/// along on that same call for the same reason: it comes off a process handle
/// detection already opens.
///
/// # Two gates, and only one of them is shared
///
/// **Which hosts count is shared, deliberately**: [`site_key`] asks
/// [`crate::context_detect::is_supported_site`], the same function that decides
/// which sites get a post-processing profile. A site can never have one without
/// the other. Neither reads `context_awareness_enabled` — the switch governs
/// whether the PROMPT is shaped, never whether Grain will say where your words
/// are going, so turning it off leaves every icon working.
///
/// **How sure we must be is NOT shared**, and this is the one asymmetry in the
/// feature. Post-processing demands [`crate::context_detect::Confidence::Exact`]
/// before a site may set the category; the icon accepts `Probable` too. That is
/// intentional, because the two are making different claims. "Your text will be
/// treated as email" needs the URL to have come off the focused element's own
/// document — get it wrong and a code snippet is rewritten as prose. "You are
/// looking at Gmail" is a weaker statement that the address bar answers
/// perfectly well, and the fallback when it is wrong is the browser's own icon,
/// which is still true. Requiring `Exact` here would cost Firefox users their
/// site icons almost entirely, since Gecko builds its accessibility tree lazily
/// and the structural rung is the one that misses.
///
/// The consequence to know about: on a `Probable` host the pill can show a site
/// while post-processing treats the window as a plain browser. If that ever
/// needs to be one rule, this is the line to change — and the log line in
/// `detect` already names which rung answered, so the case is diagnosable.
fn foreground(settings: &grain_core::AppSettings) -> Foreground {
    let Some(ctx) = crate::context_detect::detect_active_context(false, false) else {
        return Foreground {
            app: None,
            site: None,
            early: false,
        };
    };
    Foreground {
        // A packaged app first: its foreground `.exe` is often a stub with no
        // usable icon resource, and the resource-level APIs cannot see the real
        // asset at all.
        app: match ctx.aumid.clone() {
            Some(aumid) => Some(IconKey::Id(aumid)),
            None => (!ctx.exe_path.is_empty()).then(|| IconKey::Path(PathBuf::from(&ctx.exe_path))),
        },
        site: ctx
            .url_host
            .as_deref()
            .and_then(|host| site_key(host, settings)),
        early: ctx.site_read_may_be_early(),
    }
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

/// The key whose pixels the pill is currently showing. Lets a re-resolve that
/// lands on the same surface skip the emit entirely, which matters now that
/// something as ordinary as a focus change can trigger one.
static SHOWING: Mutex<Option<IconKey>> = Mutex::new(None);

/// Bumped by every resolve. Async work captures the value it started under and
/// drops its result if it no longer matches.
///
/// Without this, a favicon fetch for a tab you have already left lands seconds
/// later and repaints the pill with the wrong site — the slower the site, the
/// more likely it wins. Comparing keys is not enough: a stale result has a
/// *different* key, which is exactly why it would sail past that check.
static RESOLVE_GEN: AtomicU64 = AtomicU64::new(0);

/// Announce the foreground surface's icon to the pill, at session start.
///
/// Clears the pill when the surface cannot be named, so a previous session's
/// icon never leads into a new one.
pub fn emit_for_session(app: &AppHandle) {
    *SHOWING.lock().unwrap() = None;
    resolve_and_emit(app, true, true);
}

/// How long to wait before asking a browser again for an address it had not
/// produced yet.
///
/// This replaces a much blunter instrument. Following the foreground used to
/// wait five seconds before believing a WINDOW switch, partly so that a browser
/// had time to build its accessibility tree — which meant every app switch,
/// browser or not, paid for it. Retrying only the case that needs it lets the
/// switch itself be as quick as a tab change, and covers tab changes too, which
/// the old delay never did.
const EARLY_RETRY: std::time::Duration = std::time::Duration::from_millis(900);

/// Re-resolve mid-session, after the user settled on a different window or tab.
///
/// Unlike session start this does NOT clear on failure. A UI Automation read can
/// come back empty for a moment — Gecko rebuilding its tree is the common case —
/// and blanking the pill to a bare dot every time that happens reads as the
/// feature being broken, when the previous icon was still perfectly correct.
pub fn refresh(app: &AppHandle) {
    resolve_and_emit(app, false, true);
}

/// Two rungs, best-first: a supported WEBSITE outranks the browser showing it,
/// because "Grain knows you are in Gmail" is a stronger claim than "Grain knows
/// you are in Chrome".
///
/// Nothing here blocks a recording. Whatever is already cached is emitted
/// immediately; anything missing resolves behind the session and is emitted late
/// if it succeeds. That also gives the stale-while-revalidate shape for free: on
/// a site whose icon is not cached yet, the browser's own icon shows at once and
/// is replaced the moment the site's arrives.
/// `may_retry` is spent by the one re-resolve a not-yet-readable browser earns,
/// so a browser that never names an address costs exactly one extra look rather
/// than a loop.
fn resolve_and_emit(app: &AppHandle, clear_when_unknown: bool, may_retry: bool) {
    let settings = crate::settings::get_settings(app);
    if !settings.pill_show_app_icon {
        // Clear any icon a previous session left on the pill.
        *SHOWING.lock().unwrap() = None;
        emit(app, None);
        return;
    }
    let generation = RESOLVE_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    let Foreground {
        app: app_key,
        site,
        early,
    } = foreground(&settings);

    if early && may_retry {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(EARLY_RETRY).await;
            // Only if nothing has moved since: a newer resolve has already
            // answered the question this retry was asked to answer.
            if RESOLVE_GEN.load(Ordering::Relaxed) == generation {
                // Never clears — this is a second opinion, and a second empty
                // read is not evidence that the first icon was wrong.
                resolve_and_emit(&app, false, false);
            }
        });
    }

    // Each key's OWN cache, kept separate.
    //
    // These were once collapsed into a single "best cached thing", with the site
    // fetch gated on it — which meant that once the BROWSER's icon was cached,
    // every supported site looked cached too and its fetch never ran. Only the
    // sites visited before the browser's own icon warmed up ever resolved. Keep
    // them apart: the question "should I fetch this site?" may only ever be
    // answered by that site's own cache.
    let site_hit = site.as_ref().and_then(|k| cache_read(app, k));
    let app_hit = app_key.as_ref().and_then(|k| cache_read(app, k));

    let plan = Plan::decide(
        site.clone(),
        site_hit.is_some(),
        app_key.clone(),
        app_hit.is_some(),
    );

    // Show the best thing already on disk, right now.
    match &plan.show {
        Some(key) => {
            let bytes = if Some(key) == site.as_ref() {
                site_hit.as_deref()
            } else {
                app_hit.as_deref()
            };
            if let Some(bytes) = bytes {
                show(app, generation, key, bytes);
            }
        }
        // Nothing to show. At session start that means "clear"; mid-session it
        // means "leave whatever is there", since the surface may simply have
        // failed to resolve this once.
        None if clear_when_unknown => {
            *SHOWING.lock().unwrap() = None;
            emit(app, None);
        }
        None => {}
    }

    match plan.fetch {
        // A website: network-bound, so the async runtime rather than a thread.
        Some(key @ IconKey::Site(_)) => {
            let IconKey::Site(host) = key.clone() else {
                return;
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(rgba) = site_fetch::resolve(&host).await {
                    // Cached even if the user has moved on — the pixels are
                    // still correct for that site, and the next visit is free.
                    cache_write(&app, &key, &rgba);
                    show(&app, generation, &key, &rgba);
                }
            });
        }
        // An app: its own thread, because the Windows resolver initialises COM.
        Some(key) => {
            let app = app.clone();
            std::thread::spawn(move || {
                if let Some(rgba) = resolve(&key) {
                    cache_write(&app, &key, &rgba);
                    show(&app, generation, &key, &rgba);
                }
            });
        }
        None => {}
    }
}

/// What one resolve decided: which cached icon to show now, and which key still
/// needs resolving behind it.
///
/// Split out as a pure decision because the bug it now pins was invisible inside
/// the effectful version: the site fetch was gated on "is anything cached?",
/// which the BROWSER's icon satisfied — so once that warmed up, no further site
/// ever fetched. Only sites visited before it did worked, which looks random
/// from the outside. Every rule here is one line and one test.
#[derive(Debug, PartialEq)]
struct Plan {
    /// Whose cached pixels to display immediately.
    show: Option<IconKey>,
    /// What to resolve in the background.
    fetch: Option<IconKey>,
}

impl Plan {
    fn decide(
        site: Option<IconKey>,
        site_cached: bool,
        app: Option<IconKey>,
        app_cached: bool,
    ) -> Self {
        Plan {
            show: match (&site, site_cached, &app, app_cached) {
                // The site wins whenever we already have it.
                (Some(s), true, ..) => Some(s.clone()),
                // Otherwise the app, including as a stand-in while a supported
                // site's own icon is still being fetched — the browser's icon
                // beats a bare dot.
                (_, _, Some(a), true) => Some(a.clone()),
                _ => None,
            },
            fetch: match (site, site_cached, app, app_cached) {
                // A site's fetch is gated ONLY on that site's own cache.
                (Some(s), false, ..) => Some(s),
                // The app is resolved only when no site is in play: with one, the
                // app icon is not what the pill ends up showing, and resolving it
                // would just risk landing later and stealing the slot.
                (None, _, Some(a), false) => Some(a),
                _ => None,
            },
        }
    }
}

/// Emit `rgba` as the icon for `key`, unless this result is stale or redundant.
///
/// Two guards, and they catch different things:
/// - `generation` — the surface moved on while this resolve was in flight, so
///   these pixels are for a window or tab the user has already left.
/// - `SHOWING` — the pill is already displaying exactly this, so emitting would
///   make the pill decode a payload to arrive at the picture it is drawing.
fn show(app: &AppHandle, generation: u64, key: &IconKey, rgba: &[u8]) {
    if RESOLVE_GEN.load(Ordering::Relaxed) != generation {
        return;
    }
    let mut showing = SHOWING.lock().unwrap();
    if showing.as_ref() == Some(key) {
        return;
    }
    *showing = Some(key.clone());
    drop(showing);
    emit(app, Some(encode(rgba)));
}

fn encode(rgba: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(rgba)
}

/// [GRAIN] A site's favicon as a PNG data URL, for the settings UI.
///
/// Shares the cache and the fetch ladder with the pill, which is the point: an
/// icon fetched for a settings card is already warm when the pill needs it, and
/// vice versa. A second path would drift and would fetch twice.
///
/// # Why this does NOT apply the registry gate
///
/// [`site_key`] refuses any host outside the registry, because there the host
/// comes from *whatever window happens to be in front* — that gate is what stops
/// the foreground app steering Grain's network requests, and it is unchanged.
///
/// The host here comes from the settings UI: either a row of the registry
/// itself, or a domain the user just typed into the profile editor. Requiring
/// it to be registered first would mean a website you add shows no icon until
/// after you save — and the act of typing it IS the act that registers it. So
/// the rule is the same one stated a level up: Grain fetches icons for hosts the
/// USER or the table has named, never for one merely observed.
///
/// It still refuses anything that is not shaped like a public domain, so a typo
/// cannot become a request to an intranet name or a bare IP.
///
/// Returns `None` when there is no icon to be had; the caller draws its fallback
/// glyph and is not told why, because there is nothing it could do differently.
pub async fn site_icon_data_url(app: &AppHandle, host: &str) -> Option<String> {
    let host = normalise_site_host(host)?;
    let key = IconKey::Site(host.clone());
    let rgba = match cache_read(app, &key) {
        Some(rgba) => rgba,
        None => {
            let rgba = site_fetch::resolve(&host).await?;
            cache_write(app, &key, &rgba);
            rgba
        }
    };
    png_data_url(&rgba)
}

/// [GRAIN] An installed application's icon as a PNG data URL, for the picker.
///
/// `id` is an [`crate::context_detect::app_catalog::InstalledApp::icon_id`]: an executable path,
/// or a packaged app's AppUserModelID. Told apart by shape, because the two go
/// to different Shell namespaces and asking the wrong one yields nothing —
/// an AppUserModelID is a bare identity with no path separator in it.
///
/// Shares the pill's cache, so an app whose icon was drawn in the picker is
/// already warm the first time it is dictated into — and vice versa, since these
/// are the very same keys the pill resolves.
///
/// Blocking (COM + shell + disk), so it is handed to a blocking task rather than
/// awaited on the async runtime.
pub async fn app_icon_data_url(app: &AppHandle, id: String) -> Option<String> {
    let key = catalogue_icon_key(&id)?;
    if let Some(rgba) = cache_read(app, &key) {
        return png_data_url(&rgba);
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rgba = resolve(&key)?;
        cache_write(&app, &key, &rgba);
        png_data_url(&rgba)
    })
    .await
    .ok()
    .flatten()
}

/// A catalogue `icon_id` as an [`IconKey`], or `None` if it is not shaped like
/// one the catalogue could have produced.
///
/// The two forms are told apart by a path separator, because they go to
/// different Shell namespaces and asking the wrong one just yields nothing: an
/// AppUserModelID is a bare identity and never contains one.
///
/// # Why the shape is checked at all
///
/// This is the same reasoning as [`normalise_site_host`], one layer over. The
/// pill only ever resolves keys it built itself from the foreground window; this
/// command takes a string across the process boundary, so "it came from the
/// catalogue" is a claim rather than a fact, and the resolver behind it hands
/// paths to the Shell.
///
/// A **UNC path is the one that actually matters**: `\\host\share\x.exe` would
/// have the Shell open an SMB connection to a machine of the caller's choosing —
/// the classic way a local file API becomes a network request and a credential
/// leak. Refusing anything that is not a local absolute path closes that, and
/// costs nothing, since every real `icon_id` is one.
fn catalogue_icon_key(id: &str) -> Option<IconKey> {
    let id = id.trim();
    // Long enough for the longest real AppUserModelID or path, short enough that
    // nothing pathological reaches the Shell.
    if id.is_empty() || id.len() > 512 {
        return None;
    }
    if !id.contains('\\') && !id.contains('/') {
        return Some(IconKey::Id(id.to_string()));
    }
    // UNC, in both spellings the Shell accepts.
    if id.starts_with("\\\\") || id.starts_with("//") {
        return None;
    }
    let path = PathBuf::from(id);
    // A relative path would resolve against Grain's working directory, which is
    // not a place any application lives.
    path.is_absolute().then_some(IconKey::Path(path))
}

/// Lowercase, `www.`-less, and shaped like a public domain — or `None`.
///
/// The shape check is the safety boundary for [`site_icon_data_url`]: it keeps a
/// mistyped target from becoming a request to `localhost`, an intranet
/// hostname, or an IP literal.
fn normalise_site_host(host: &str) -> Option<String> {
    let host = host
        .trim()
        .trim_start_matches("www.")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let (Some(label), true) = (host.rsplit('.').next(), host.contains('.')) else {
        return None;
    };
    // A TLD is alphabetic, which rules out every IPv4 literal in one test.
    let tld_ok = label.len() >= 2 && label.chars().all(|c| c.is_ascii_alphabetic());
    let chars_ok = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let no_empty_labels = !host.starts_with('.') && !host.contains("..");
    (tld_ok && chars_ok && no_empty_labels && host.len() <= 253).then_some(host)
}

/// `PILL_ICON_PX`² premultiplied RGBA → a `data:image/png;base64,…` URL.
///
/// Un-premultiplies on the way out. The pill blits onto a known background and
/// wants premultiplied; a PNG in a webview is composited by the browser and
/// wants straight alpha, so skipping this would darken every semi-transparent
/// edge pixel — visible as a dirty halo on any rounded favicon.
fn png_data_url(rgba: &[u8]) -> Option<String> {
    if rgba.len() != ICON_BYTES {
        return None;
    }
    let mut straight = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let a = px[3];
        let un = |c: u8| {
            if a == 0 {
                0
            } else {
                ((c as u32 * 255) / a as u32).min(255) as u8
            }
        };
        straight.extend_from_slice(&[un(px[0]), un(px[1]), un(px[2]), a]);
    }
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(PILL_ICON_PX as u32, PILL_ICON_PX as u32, straight)?;
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        encode(&png.into_inner())
    ))
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

/// The pill's website gate and post-processing's must stay one decision.
#[cfg(test)]
mod registry_gate_tests {
    use super::*;
    use grain_core::settings::{ContextProfileTarget, CustomContextProfile};
    use grain_core::AppSettings;

    fn with_site_profile(host: &str) -> AppSettings {
        let mut s = AppSettings::default();
        s.context_custom_profiles.push(CustomContextProfile {
            id: "p".into(),
            title: "Design".into(),
            instruction: "Terse.".into(),
            targets: vec![ContextProfileTarget {
                kind: "website".into(),
                value: host.into(),
            }],
        });
        s
    }

    /// The whole promise of the registry: a site Grain knows gets its own icon,
    /// and anything else leaves the BROWSER's icon standing rather than
    /// producing a fetch for whatever host happened to be in the address bar.
    #[test]
    fn only_a_registered_site_ever_becomes_an_icon_key() {
        let s = AppSettings::default();
        for known in ["mail.google.com", "claude.ai", "github.com"] {
            assert!(site_key(known, &s).is_some(), "{known} is a supported site");
        }
        for unknown in [
            "en.wikipedia.org",
            "amazon.com",
            "bbc.co.uk",
            // A lookalike must not inherit a registered site's icon.
            "notgithub.com",
            "github.com.evil.test",
            "localhost",
            "",
        ] {
            assert_eq!(site_key(unknown, &s), None, "{unknown} must fall back");
        }
    }

    /// A site named in a user's own profile is a supported site — the pill and
    /// post-processing extend together, from the same list.
    #[test]
    fn a_custom_profile_site_is_registered_for_the_pill_too() {
        let s = with_site_profile("figma.com");
        assert!(site_key("figma.com", &s).is_some());
        // …with the registry's own host rule, so a row covers its subdomains
        // and nothing that merely ends the same way.
        assert!(site_key("app.figma.com", &s).is_some());
        assert_eq!(site_key("notfigma.com", &s), None);
    }

    /// **Context awareness being off must not blank the pill.** Recognising the
    /// surface and shaping the text for it are separate promises: the switch
    /// governs whether the prompt is touched, never whether Grain will tell you
    /// where your words are going. Asserted for a built-in row and a user's own,
    /// because the custom path reads settings and could plausibly grow a check.
    #[test]
    fn the_icon_survives_context_awareness_being_switched_off() {
        let mut s = with_site_profile("figma.com");
        s.context_awareness_enabled = false;
        assert!(site_key("mail.google.com", &s).is_some());
        assert!(site_key("figma.com", &s).is_some());
        assert_eq!(site_key("example.test", &s), None, "still gated, just not by the switch");
    }

    /// One decision, not two that agree today. If these ever diverge, a site can
    /// get a post-processing profile with no icon — or an icon fetched for a
    /// host that is not a supported site at all, which is the security property.
    #[test]
    fn the_pill_gate_is_the_same_gate_post_processing_uses() {
        let s = with_site_profile("figma.com");
        for host in [
            "mail.google.com",
            "figma.com",
            "app.figma.com",
            "news.ycombinator.com",
            "notfigma.com",
        ] {
            assert_eq!(
                site_key(host, &s).is_some(),
                crate::context_detect::is_supported_site(host, &s),
                "{host}: the pill and post-processing disagree about the registry"
            );
        }
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;

    fn site() -> IconKey {
        IconKey::Site("claude.ai".into())
    }
    fn browser() -> IconKey {
        IconKey::Id("Chrome".into())
    }

    /// The shipped bug, as a test.
    ///
    /// Reported as "Gmail and GitHub work, but Claude, ChatGPT and Gemini never
    /// do". Those first two were visited while the browser's own icon was still
    /// cold; from then on every supported site looked cached because the BROWSER
    /// was, and none of them ever fetched.
    #[test]
    fn a_cached_browser_icon_does_not_suppress_a_sites_own_fetch() {
        let plan = Plan::decide(Some(site()), false, Some(browser()), true);
        assert_eq!(
            plan.fetch,
            Some(site()),
            "the site must fetch on its own cache miss, whatever the browser's state"
        );
        assert_eq!(
            plan.show,
            Some(browser()),
            "and the browser's icon stands in meanwhile, rather than a bare dot"
        );
    }

    #[test]
    fn a_cached_site_is_shown_and_nothing_is_fetched() {
        let plan = Plan::decide(Some(site()), true, Some(browser()), true);
        assert_eq!(plan.show, Some(site()));
        assert_eq!(plan.fetch, None);
    }

    #[test]
    fn a_site_outranks_the_browser_even_before_the_browser_is_known() {
        let plan = Plan::decide(Some(site()), true, None, false);
        assert_eq!(plan.show, Some(site()));
    }

    /// An unsupported site is not a failure — it is the browser's own identity,
    /// which is the correct answer. Reported as "it doesn't switch back to the
    /// browser icon on an unknown website".
    #[test]
    fn an_unsupported_site_falls_back_to_the_browser() {
        let plan = Plan::decide(None, false, Some(browser()), true);
        assert_eq!(plan.show, Some(browser()));
        assert_eq!(plan.fetch, None);
    }

    #[test]
    fn an_unknown_browser_on_an_unsupported_site_resolves_the_browser() {
        let plan = Plan::decide(None, false, Some(browser()), false);
        assert_eq!(plan.show, None, "nothing cached to show yet");
        assert_eq!(plan.fetch, Some(browser()));
    }

    /// With a site in play the app icon is not the destination, so resolving it
    /// would only race the site's fetch for the same slot.
    #[test]
    fn the_app_is_never_resolved_while_a_site_is_in_play() {
        for app_cached in [true, false] {
            for site_cached in [true, false] {
                let plan = Plan::decide(Some(site()), site_cached, Some(browser()), app_cached);
                assert_ne!(
                    plan.fetch,
                    Some(browser()),
                    "site_cached={site_cached} app_cached={app_cached}"
                );
            }
        }
    }

    #[test]
    fn nothing_known_shows_and_fetches_nothing() {
        assert_eq!(
            Plan::decide(None, false, None, false),
            Plan {
                show: None,
                fetch: None
            }
        );
    }
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
    ///
    /// HTML is TRUNCATED at this, not rejected. Rejecting is what an oversized
    /// icon deserves — half a PNG is garbage — but a real app shell can be far
    /// larger than any reasonable cap and still declare its icons in the first
    /// few KB. gemini.google.com serves ~820 KB of HTML, which failed outright
    /// under a rejecting cap even though its `<head>` was well inside it.
    const HTML_CAP: usize = 192 * 1024;
    const ICON_CAP: usize = 1024 * 1024;
    const TIMEOUT: Duration = Duration::from_secs(6);
    /// Bounded so a site that lists a dozen icons cannot turn one session into a
    /// dozen requests.
    const MAX_TRIES: usize = 4;

    /// Hosts whose icon CANNOT be fetched, with the icon shipped instead.
    ///
    /// A deliberate, narrow exception to "the registry holds identities, never
    /// assets" — and the reason it is narrow is that the rule is still right.
    /// An entry here earns its place only by being unfetchable, not by being
    /// important: every OpenAI origin (`chatgpt.com`, `openai.com`, both `/`
    /// and `/favicon.ico`) answers 403 to any plain HTTP client. Measured with
    /// a full Chrome User-Agent as well as ours, so it is not a UA problem —
    /// it is a TLS-fingerprint and JS challenge, which nothing short of
    /// driving a real browser will pass. Their CDN does answer, but only at
    /// content-hashed paths that change on every deploy, so there is no stable
    /// URL to point at either.
    ///
    /// Checked BEFORE the network, not after. Falling back would mean burning
    /// the whole candidate ladder — up to four requests at a 6 s timeout — on
    /// every session at a host we already know will refuse, to arrive at the
    /// same bytes. A host is listed here precisely because asking is pointless.
    ///
    /// The cost of the exception is staleness: a rebrand needs a new commit.
    /// That is the trade being accepted, and it is why this list must not grow
    /// to hold sites that merely *have* a nice logo.
    ///
    /// The art is the public-domain mark from Wikimedia Commons
    /// (`File:ChatGPT-Logo.svg`), recoloured white and rendered to 128px. White
    /// because the pill surface is `#1E1E20`: the source is a pure black glyph
    /// on transparency and would have been invisible on it — and white-on-dark
    /// is how OpenAI presents the mark themselves. See `assets/site-icons/`.
    /// Both rows are hosts the registry already supports. Bare `openai.com` is
    /// deliberately NOT here: it is not in `SITE_TABLE`, so `site_key` would
    /// never let it reach this code, and an entry for it would be a file that
    /// looks like it does something and does not.
    static BUNDLED: &[(&str, &[u8])] = &[
        (
            "chatgpt.com",
            include_bytes!("../assets/site-icons/chatgpt.png"),
        ),
        (
            "chat.openai.com",
            include_bytes!("../assets/site-icons/chatgpt.png"),
        ),
    ];

    /// Matched with the registry's own host rule, so a row covers its
    /// subdomains exactly as it does for site categories. Two spellings of
    /// "same site" that could drift apart is a bug waiting to be filed.
    fn bundled(host: &str) -> Option<&'static [u8]> {
        BUNDLED
            .iter()
            .find(|(pattern, _)| crate::context_detect::host_matches(host, pattern))
            .map(|(_, bytes)| *bytes)
    }

    pub async fn resolve(host: &str) -> Option<Vec<u8>> {
        if let Some(bytes) = bundled(host) {
            // Through the same decode as a fetched icon: one downscale path,
            // one premultiply, no second way for a site icon to be built.
            return decode(bytes);
        }
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(4))
            // Identify honestly. Some sites serve a different (or no) icon to a
            // client that looks like a scraper.
            .user_agent(concat!("Grain/", env!("CARGO_PKG_VERSION")))
            .build()
            .ok()?;
        let origin = format!("https://{host}");

        for url in candidates(&client, &origin)
            .await
            .into_iter()
            .take(MAX_TRIES)
        {
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
        if let Some(bytes) = get_truncating(client, &format!("{origin}/"), HTML_CAP).await {
            // Lossy: a cut at the cap can land mid-character, and one mangled
            // glyph in the tail is irrelevant to link tags in the head.
            out = declared_icons(&String::from_utf8_lossy(&bytes), origin);
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
            if !rel_val
                .split_whitespace()
                .any(|w| w == "icon" || w.ends_with("-icon"))
            {
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
                .unwrap_or(if rel_val.contains("apple-touch") {
                    180
                } else {
                    32
                });
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

    /// GET with a hard byte ceiling, failing if the body exceeds it.
    ///
    /// For icons: a truncated image is garbage, so an oversized one is dropped.
    /// Streamed and counted rather than trusting `Content-Length`, which a server
    /// is free to understate or omit.
    async fn get(client: &reqwest::Client, url: &str, cap: usize) -> Option<Vec<u8>> {
        read_capped(client, url, cap, false).await
    }

    /// GET that stops reading at the ceiling and keeps what it has. For HTML,
    /// where the part we need is at the top and the rest is not worth refusing
    /// the whole document over.
    async fn get_truncating(client: &reqwest::Client, url: &str, cap: usize) -> Option<Vec<u8>> {
        read_capped(client, url, cap, true).await
    }

    async fn read_capped(
        client: &reqwest::Client,
        url: &str,
        cap: usize,
        truncate: bool,
    ) -> Option<Vec<u8>> {
        let resp = client.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        if !truncate && resp.content_length().is_some_and(|n| n as usize > cap) {
            return None;
        }
        let mut buf = Vec::with_capacity(8 * 1024);
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.ok()?;
            if buf.len() + chunk.len() > cap {
                if !truncate {
                    return None;
                }
                // Take the head and stop pulling: dropping the connection here is
                // the point, so a huge page costs us only the cap.
                buf.extend_from_slice(&chunk[..cap - buf.len()]);
                break;
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

        /// A catalogue id crosses the process boundary, so its shape is the only
        /// thing standing between the picker and the Shell opening an SMB
        /// connection to a host of the caller's choosing.
        #[test]
        fn an_app_icon_id_must_be_a_local_path_or_a_bare_identity() {
            use crate::pill_icon::{catalogue_icon_key as k, IconKey};
            // The two real forms.
            assert!(matches!(
                k("Claude_pzs8sxrjxfjjc!Claude"),
                Some(IconKey::Id(_))
            ));
            assert!(matches!(
                k(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
                Some(IconKey::Path(_))
            ));
            // A UNC path would make an icon lookup into a network request.
            assert_eq!(k(r"\\attacker.test\share\payload.exe"), None);
            assert_eq!(k("//attacker.test/share/payload.exe"), None);
            // …and nothing else that names no installed application.
            for bad in ["", "   ", r"..\..\secret.exe", "relative/path.exe"] {
                assert_eq!(k(bad), None, "{bad:?} should be refused");
            }
        }

        /// The settings-side fetch drops the registry gate, so this shape check
        /// is the only thing standing between a typo and a request to something
        /// that is not a public website.
        #[test]
        fn the_settings_icon_fetch_refuses_anything_that_is_not_a_public_domain() {
            use crate::pill_icon::normalise_site_host as n;
            assert_eq!(n("Figma.com").as_deref(), Some("figma.com"));
            assert_eq!(n("www.Figma.com.").as_deref(), Some("figma.com"));
            assert_eq!(n("app.figma.com").as_deref(), Some("app.figma.com"));
            // Nothing that resolves inside a network, and no IP literals.
            for bad in [
                "localhost",
                "127.0.0.1",
                "192.168.1.1",
                "intranet",
                "",
                "   ",
                "figma..com",
                ".figma.com",
                "figma.c",
                "figma.com:8080",
                "http://figma.com",
                "figma.com/path",
            ] {
                assert_eq!(n(bad), None, "{bad:?} should be refused");
            }
        }

        #[test]
        fn a_bundled_host_decodes_to_a_usable_icon() {
            // The whole point of shipping the bytes: they must survive the same
            // decode a fetched icon goes through, or we have shipped a file that
            // silently produces nothing.
            let bytes = bundled("chatgpt.com").expect("chatgpt.com should be bundled");
            let icon = decode(bytes).expect("the bundled PNG must decode");
            assert_eq!(icon.len(), crate::pill_icon::ICON_BYTES);
        }

        #[test]
        fn the_bundled_chatgpt_mark_is_light_enough_to_read_on_the_dark_pill() {
            // A black-on-transparent favicon is the normal case for this mark,
            // and on a #1E1E20 surface it is invisible. Recolouring it is the
            // reason the file exists, so guard the recolour rather than trust it.
            let icon = decode(bundled("chatgpt.com").unwrap()).unwrap();
            let (mut lum, mut covered) = (0u64, 0u64);
            for px in icon.chunks_exact(4) {
                // Premultiplied, so compare against alpha: an opaque white pixel
                // is (255,255,255,255) and an opaque black one is (0,0,0,255).
                if px[3] > 128 {
                    covered += 1;
                    lum += (px[0] as u64 * 299 + px[1] as u64 * 587 + px[2] as u64 * 114) / 1000;
                }
            }
            assert!(covered > 200, "the mark is nearly empty: {covered} px");
            let mean = lum / covered;
            assert!(
                mean > 180,
                "mean luminance {mean} would disappear on the pill surface"
            );
        }

        #[test]
        fn a_bundled_entry_covers_its_subdomains_the_way_the_registry_does() {
            // A row covers anything under it...
            assert!(bundled("chat.openai.com").is_some());
            assert!(bundled("eu.chatgpt.com").is_some());
            // ...but not a lookalike that merely ends the same way, and not the
            // bare parent of a row (openai.com is not a supported site).
            assert!(bundled("notchatgpt.com").is_none());
            assert!(bundled("openai.com").is_none());
            assert!(bundled("example.com").is_none());
        }

        #[test]
        fn every_bundled_host_is_a_site_the_registry_already_knows() {
            // A bundled icon for a host that is not in SITE_TABLE could never be
            // shown: `site_key` gates on the registry before any of this runs.
            for (host, _) in BUNDLED {
                assert!(
                    crate::context_detect::category_for_site(host).is_some(),
                    "{host} is bundled but not a supported site"
                );
            }
        }

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
            assert_eq!(
                absolutise("/a.png", o).as_deref(),
                Some("https://x.test/a.png")
            );
            assert_eq!(
                absolutise("a.png", o).as_deref(),
                Some("https://x.test/a.png")
            );
            assert_eq!(
                absolutise("//cdn.test/a.png", o).as_deref(),
                Some("https://cdn.test/a.png"),
                "protocol-relative must become https, not be dropped"
            );
            assert_eq!(
                absolutise("https://cdn.test/a.png", o).as_deref(),
                Some("https://cdn.test/a.png")
            );
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
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    };

    /// Ask the Shell for a bigger bitmap than we need and downscale ourselves.
    /// The Shell's own fit is a GDI stretch blit; at pill sizes the difference
    /// between that and a real filter is the whole look. (The docs explicitly
    /// suggest this pairing with SIIGBF_BIGGERSIZEOK.)
    const REQUEST_PX: i32 = 96;

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
    unsafe fn bitmap_to_rgba(
        hbm: windows::Win32::Graphics::Gdi::HBITMAP,
    ) -> Option<(Vec<u8>, usize, usize)> {
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
