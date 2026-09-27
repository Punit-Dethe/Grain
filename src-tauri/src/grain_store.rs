//! [GRAIN] Phase 5A — the in-app store's client (DISTRIBUTION-PLAN §5.2, §5.3).
//!
//! This is the thin Tauri layer over the verified-catalogue machinery in
//! `grain_core::trust` / `grain_core::install`. It owns:
//!
//! - **Step 3 (store data path):** a seed catalogue shipped in the binary, a
//!   refresh that piggybacks the update check, a disk cache used when the
//!   network is down (store renders offline, new installs refused), and the
//!   drop-on-close rule — the parsed index is only resident while the store is
//!   open; roots + revocations (small) stay resident.
//! - **Step 5 (install):** fetch the content-addressed blob, verify its hash
//!   against the verified entry, then run the single trust-setting install path.
//! - **Step 6 (revocation):** enforce the signed kill switch from cache at
//!   enable time and surface banners for revoked installs.
//!
//! Overhead rule: with the store closed only the small roots/revocations stay in
//! memory; the entry list is dropped. Nothing here runs on a timer.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use grain_core::install::{self, InstallError};
use grain_core::pack::ExtractLimits;
use grain_core::trust::{self, IndexStatus, TrustError};
use grain_sdk::distribution::{Index, IndexEntry, RevocationState, Revocations, Roots};
use serde::Serialize;

const STORE_DOCUMENT_MAX_BYTES: u64 = 4 * 1024 * 1024;
const STORE_SIGNATURE_MAX_BYTES: u64 = 64 * 1024;
const STORE_MEDIA_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// The resident store state (registered as managed Tauri state). Roots and
/// revocations are small and stay loaded; the parsed index is present only
/// while the Extensions store UI is active.
pub struct StoreState {
    cache_dir: PathBuf,
    roots: RwLock<Roots>,
    revocations: RwLock<Revocations>,
    index: RwLock<Option<Index>>,
    /// Highest index `version` accepted so far — the rollback floor.
    stored_version: RwLock<Option<u64>>,
}

/// One card's data for the store UI (a specta-friendly projection of
/// [`IndexEntry`]; the index type itself lives in the crypto-free leaf).
#[derive(Clone, Serialize, specta::Type)]
pub struct StoreEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub tier: String,
    pub trust: String,
    pub capabilities: Vec<String>,
    /// One-line summary shown under the name on the card.
    pub description: String,
    /// Source repository (GitHub), for the "view on GitHub" link.
    pub repo: String,
    pub size: String,
    pub author: String,
    pub reviewed_at: String,
    pub reviewed_commit: String,
    /// Repository popularity captured by the signed publish pipeline. The
    /// desktop client never contacts GitHub to populate detail pages.
    pub stars: u64,
    /// Popularity signal shown on the card and detail page. Read straight from
    /// the signed index — the client never counts or queries per card.
    pub installs: u64,
    /// README media hash (empty = none). The detail page fetches it lazily.
    pub readme: String,
    /// Screenshots / GIFs for the detail page, loaded lazily — never in browse.
    pub media: Vec<StoreMedia>,
    /// What kind of thing this is, for the store's filter row.
    pub categories: Vec<String>,
    /// Which parts of Grain this extension changes (slots claimed, settings
    /// anchors, payload surfaces). Read straight from the signed index, so the
    /// store can place a card without downloading its artifact.
    pub extends: Vec<String>,
    /// Revocation state for this exact version, if any: "revoked" | "deprecated".
    pub revocation: Option<String>,
    /// Flagged capability combinations (DISTRIBUTION-PLAN §3.3), plain-language,
    /// so the card tells the user what the reviewer was warned about.
    pub flags: Vec<String>,
}

/// One screenshot/GIF ref crossed to the store UI (mirror of `MediaRef`).
#[derive(Clone, Serialize, specta::Type)]
pub struct StoreMedia {
    pub sha256: String,
    /// `webp` | `gif`.
    pub kind: String,
}

/// Catalogue state projected into the Extensions store and detail UI.
#[derive(Clone, Serialize, specta::Type)]
pub struct StoreView {
    /// "fresh" | "offline" | "needs-newer-client".
    pub status: String,
    /// Whether new installs are allowed (false when offline/expired).
    pub can_install: bool,
    pub entries: Vec<StoreEntry>,
}

/// A banner for an installed extension that has been revoked or deprecated.
#[derive(Clone, Serialize, specta::Type)]
pub struct RevocationBanner {
    pub id: String,
    pub state: String,
    pub reason: String,
}

fn tier_str(t: &grain_sdk::Tier) -> &'static str {
    match t {
        grain_sdk::Tier::Pack => "pack",
        grain_sdk::Tier::Scripted => "scripted",
        grain_sdk::Tier::Native => "native",
    }
}

/// Trust as the STORE says it.
///
/// `Core` is an internal provenance rung — our own packs are built and signed by
/// the same CI job, with no review queue — but to someone deciding whether to
/// install, it promises exactly what `Verified` promises. Two words for one
/// guarantee only raises the question of which is better, and first-party
/// software claiming its own extra tier is not a good answer to that. So the
/// catalogue keeps the distinction and the card does not.
fn trust_str(t: grain_sdk::Trust) -> &'static str {
    match t {
        grain_sdk::Trust::Dev => "dev",
        grain_sdk::Trust::Experimental => "experimental",
        grain_sdk::Trust::Verified | grain_sdk::Trust::Core => "verified",
    }
}

impl StoreState {
    /// Load roots + revocations from the disk cache, falling back to the
    /// embedded seed. The index is intentionally **not** loaded here (only its
    /// version, for the rollback floor) so idle footprint stays minimal.
    pub fn init(data_dir: &Path) -> Self {
        let cache_dir = data_dir.join("store");
        let _ = std::fs::create_dir_all(&cache_dir);

        // Roots: cache first (verified against the pinned keys), else seed.
        let roots = load_cached_roots(&cache_dir)
            .or_else(|| {
                trust::verify_roots(trust::SEED_ROOTS.as_bytes(), trust::SEED_ROOTS_SIG).ok()
            })
            .unwrap_or_else(|| {
                // The seed is embedded and signed at build time; this cannot
                // fail unless the binary is corrupt.
                panic!("embedded seed roots failed to verify — corrupt binary");
            });

        // Revocations: cache first, else seed.
        let revocations = load_cached_revocations(&cache_dir, &roots)
            .or_else(|| {
                trust::verify_revocations(
                    &roots,
                    trust::SEED_REVOCATIONS.as_bytes(),
                    trust::SEED_REVOCATIONS_SIG,
                )
                .ok()
            })
            .unwrap_or_else(|| Revocations {
                spec: 1,
                version: 0,
                expires: String::new(),
                entries: Vec::new(),
            });

        // Rollback floor from any cached index (verify, read version, drop).
        let stored_version = load_cached_index(&cache_dir, &roots, None, now_unix())
            .ok()
            .map(|(idx, _)| idx.version);

        StoreState {
            cache_dir,
            roots: RwLock::new(roots),
            revocations: RwLock::new(revocations),
            index: RwLock::new(None),
            stored_version: RwLock::new(stored_version),
        }
    }

    /// Revocation state for an installed `(id, version)`, read from the resident
    /// revocation list. Enforced at enable time, before any worker spawns.
    pub fn revocation_state(&self, id: &str, version: &str) -> Option<RevocationState> {
        self.revocations.read().unwrap().state_for(id, version)
    }

    /// Drop the parsed index when the Extensions store UI closes. Roots and
    /// revocations stay resident.
    pub fn close(&self) {
        *self.index.write().unwrap() = None;
    }

    /// Ensure the index is resident (from cache or seed), returning the view.
    /// Used when opening the store without a network round-trip.
    fn view_from_resident(&self) -> StoreView {
        let mut guard = self.index.write().unwrap();
        if guard.is_none() {
            let roots = self.roots.read().unwrap();
            let loaded = load_cached_index(&self.cache_dir, &roots, None, now_unix())
                .map(|(idx, status)| (idx, status))
                .or_else(|_| {
                    // Seed is expiry-exempt until the first refresh.
                    trust::verify_index(
                        &roots,
                        trust::SEED_INDEX.as_bytes(),
                        trust::SEED_INDEX_SIG,
                        None,
                        now_unix(),
                        true,
                    )
                });
            match loaded {
                Ok((idx, _)) => *guard = Some(idx),
                Err(_) => *guard = None,
            }
        }
        let revocations = self.revocations.read().unwrap();
        let entries = guard
            .as_ref()
            .map(|idx| project_entries(&idx.entries, &revocations))
            .unwrap_or_default();
        // Resident view has not been network-refreshed this open, so treat as
        // offline for install purposes until a refresh succeeds.
        StoreView {
            status: "offline".into(),
            can_install: false,
            entries,
        }
    }
}

fn project_entries(entries: &[IndexEntry], revocations: &Revocations) -> Vec<StoreEntry> {
    entries
        .iter()
        .filter(|entry| entry.validate_tool_only().is_ok())
        .map(|e| StoreEntry {
            id: e.id.clone(),
            name: e.name.clone(),
            version: e.version.clone(),
            tier: tier_str(&e.tier).into(),
            trust: trust_str(e.trust).into(),
            capabilities: e.capabilities.clone(),
            description: e.description.clone(),
            repo: e.repo.clone(),
            size: e.size.to_string(),
            author: e.author.clone(),
            reviewed_at: e.reviewed_at.clone(),
            reviewed_commit: e.reviewed_commit.clone(),
            stars: e.stars,
            installs: e.installs,
            readme: e.readme.clone(),
            media: e
                .media
                .iter()
                .map(|m| StoreMedia {
                    sha256: m.sha256.clone(),
                    kind: m.kind.clone(),
                })
                .collect(),
            categories: e.categories.clone(),
            extends: e.extends.clone(),
            revocation: revocations.state_for(&e.id, &e.version).map(|s| {
                match s {
                    RevocationState::Revoked => "revoked",
                    RevocationState::Deprecated => "deprecated",
                }
                .to_string()
            }),
            flags: grain_sdk::flagged_combinations(&e.capabilities, e.tier.clone())
                .into_iter()
                .map(|f| f.reason().to_string())
                .collect(),
        })
        .collect()
}

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Cache helpers (all verify before returning) ────────────────────────────

fn read_pair(dir: &Path, name: &str) -> Option<(Vec<u8>, String)> {
    let doc = read_bounded_file(&dir.join(name), STORE_DOCUMENT_MAX_BYTES)?;
    let sig = String::from_utf8(read_bounded_file(
        &dir.join(format!("{name}.minisig")),
        STORE_SIGNATURE_MAX_BYTES,
    )?)
    .ok()?;
    Some((doc, sig))
}

fn read_bounded_file(path: &Path, max: u64) -> Option<Vec<u8>> {
    use std::io::Read;

    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}

fn write_pair(dir: &Path, name: &str, doc: &[u8], sig: &str) {
    let _ = std::fs::write(dir.join(name), doc);
    let _ = std::fs::write(dir.join(format!("{name}.minisig")), sig);
}

fn load_cached_roots(dir: &Path) -> Option<Roots> {
    let (doc, sig) = read_pair(dir, "roots.json")?;
    trust::verify_roots(&doc, &sig).ok()
}

fn load_cached_revocations(dir: &Path, roots: &Roots) -> Option<Revocations> {
    let (doc, sig) = read_pair(dir, "revocations.json")?;
    trust::verify_revocations(roots, &doc, &sig).ok()
}

fn load_cached_index(
    dir: &Path,
    roots: &Roots,
    stored_version: Option<u64>,
    now: i64,
) -> Result<(Index, IndexStatus), TrustError> {
    let (doc, sig) = read_pair(dir, "index.json").ok_or(TrustError::BadSignatureFormat)?;
    trust::verify_index(roots, &doc, &sig, stored_version, now, false)
}

// ── HTTP refresh + install (async, via the shared reqwest client) ──────────

async fn fetch(client: &reqwest::Client, base: &str, name: &str, max: u64) -> Option<Vec<u8>> {
    let url = format!("{}{}", base.trim_end_matches('/').to_string() + "/", name);
    let mut resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    if resp.content_length().is_some_and(|length| length > max) {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await.ok()? {
        if bytes.len().saturating_add(chunk.len()) as u64 > max {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

async fn fetch_text(client: &reqwest::Client, base: &str, name: &str, max: u64) -> Option<String> {
    fetch(client, base, name, max)
        .await
        .and_then(|b| String::from_utf8(b).ok())
}

/// Refresh roots → index → revocations from the first reachable base URL,
/// verifying each, caching on success, and applying revocations. Returns the
/// resulting [`StoreView`]; on total network failure it falls back to cache.
pub async fn refresh(state: &StoreState, client: &reqwest::Client) -> StoreView {
    refresh_at(state, client, now_unix()).await
}

async fn refresh_at(state: &StoreState, client: &reqwest::Client, now: i64) -> StoreView {
    let bases: Vec<String> = {
        let roots = state.roots.read().unwrap();
        roots
            .base_urls
            .iter()
            .chain(roots.mirrors.iter())
            .cloned()
            .collect()
    };

    for base in &bases {
        // roots.json — verified against pinned keys; adopt if newer.
        if let (Some(rdoc), Some(rsig)) = (
            fetch(client, base, "roots.json", STORE_DOCUMENT_MAX_BYTES).await,
            fetch_text(
                client,
                base,
                "roots.json.minisig",
                STORE_SIGNATURE_MAX_BYTES,
            )
            .await,
        ) {
            if let Ok(new_roots) = trust::verify_roots(&rdoc, &rsig) {
                let adopt = new_roots.version >= state.roots.read().unwrap().version;
                if adopt {
                    write_pair(&state.cache_dir, "roots.json", &rdoc, &rsig);
                    *state.roots.write().unwrap() = new_roots;
                }
            }
        }

        let roots = state.roots.read().unwrap().clone();
        let stored = *state.stored_version.read().unwrap();

        let (Some(idoc), Some(isig)) = (
            fetch(client, base, "index.json", STORE_DOCUMENT_MAX_BYTES).await,
            fetch_text(
                client,
                base,
                "index.json.minisig",
                STORE_SIGNATURE_MAX_BYTES,
            )
            .await,
        ) else {
            continue;
        };
        let Ok((index, status)) = trust::verify_index(&roots, &idoc, &isig, stored, now, false)
        else {
            continue;
        };

        // revocations.json — verify and apply; missing is not fatal.
        if let (Some(vdoc), Some(vsig)) = (
            fetch(client, base, "revocations.json", STORE_DOCUMENT_MAX_BYTES).await,
            fetch_text(
                client,
                base,
                "revocations.json.minisig",
                STORE_SIGNATURE_MAX_BYTES,
            )
            .await,
        ) {
            if let Ok(revs) = trust::verify_revocations(&roots, &vdoc, &vsig) {
                write_pair(&state.cache_dir, "revocations.json", &vdoc, &vsig);
                *state.revocations.write().unwrap() = revs;
            }
        }

        match status {
            IndexStatus::NeedsNewerClient => {
                return StoreView {
                    status: "needs-newer-client".into(),
                    can_install: false,
                    entries: Vec::new(),
                };
            }
            IndexStatus::Fresh => {
                write_pair(&state.cache_dir, "index.json", &idoc, &isig);
                *state.stored_version.write().unwrap() = Some(index.version);
                let revocations = state.revocations.read().unwrap();
                let entries = project_entries(&index.entries, &revocations);
                *state.index.write().unwrap() = Some(index);
                return StoreView {
                    status: "fresh".into(),
                    can_install: true,
                    entries,
                };
            }
            IndexStatus::Expired => {
                // Signature good but stale: keep serving, refuse new installs.
                let revocations = state.revocations.read().unwrap();
                let entries = project_entries(&index.entries, &revocations);
                *state.index.write().unwrap() = Some(index);
                return StoreView {
                    status: "offline".into(),
                    can_install: false,
                    entries,
                };
            }
        }
    }

    // No base reachable — render from whatever is resident/cached, offline.
    state.view_from_resident()
}

/// Install a verified entry: fetch its content-addressed blob, verify the hash,
/// then run the single trust-setting install path. Refuses if the store is not
/// fresh (offline installs are not allowed, DISTRIBUTION-PLAN §5.3).
pub async fn install_entry(
    state: &StoreState,
    reg: &grain_core::extensions::ExtensionsRegistry,
    ext_root: &Path,
    client: &reqwest::Client,
    id: &str,
    version: &str,
) -> Result<PathBuf, String> {
    // Revocation gate: never install a revoked (id, version).
    if let Some(RevocationState::Revoked) = state.revocation_state(id, version) {
        return Err(format!("{id} {version} has been revoked"));
    }

    let entry: IndexEntry = {
        let guard = state.index.read().unwrap();
        let idx = guard
            .as_ref()
            .ok_or("store is not open; open it before installing")?;
        idx.entries
            .iter()
            .find(|e| e.id == id && e.version == version)
            .cloned()
            .ok_or_else(|| format!("no entry {id} {version} in the verified index"))?
    };
    entry.validate_tool_only()?;
    if entry.size > grain_sdk::PACK_MAX_BYTES {
        return Err(format!(
            "catalogue artifact exceeds the {} MiB pack limit",
            grain_sdk::PACK_MAX_BYTES / (1024 * 1024)
        ));
    }

    let bases: Vec<String> = {
        let roots = state.roots.read().unwrap();
        roots
            .base_urls
            .iter()
            .chain(roots.mirrors.iter())
            .cloned()
            .collect()
    };
    let blob_name = format!("blob/{}.grainpack", entry.sha256);
    let mut bytes: Option<Vec<u8>> = None;
    for base in &bases {
        if let Some(b) = fetch(client, base, &blob_name, grain_sdk::PACK_MAX_BYTES).await {
            bytes = Some(b);
            break;
        }
    }
    let bytes = bytes.ok_or("could not download the artifact from any host")?;
    if entry.size != 0 && bytes.len() as u64 != entry.size {
        return Err("downloaded artifact size did not match the signed catalogue".into());
    }

    install::install_from_verified_entry(reg, ext_root, &entry, &bytes, ExtractLimits::default())
        .map_err(|e: InstallError| e.to_string())
}

// ── Tauri commands ─────────────────────────────────────────────────────────

use std::sync::Arc;
use tauri::{AppHandle, Manager};

fn store_state(app: &AppHandle) -> Result<Arc<StoreState>, String> {
    app.try_state::<Arc<StoreState>>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| "store unavailable".to_string())
}

fn ext_root(app: &AppHandle) -> Result<PathBuf, String> {
    let ctx = app
        .try_state::<Arc<grain_core::AppContext>>()
        .ok_or("app context unavailable")?;
    Ok(ctx.data_dir.join("extensions"))
}

/// Open the store: refresh from the network (piggybacking the update check),
/// verify, and return the catalogue. Falls back to the offline cache/seed.
#[tauri::command]
#[specta::specta]
pub async fn store_browse(app: AppHandle) -> Result<StoreView, String> {
    let state = store_state(&app)?;
    let client = app
        .try_state::<reqwest::Client>()
        .map(|c| c.inner().clone())
        .ok_or("http client unavailable")?;
    Ok(refresh(&state, &client).await)
}

/// Close the Extensions store UI: drop the parsed index so idle footprint
/// returns to just the small roots and revocations.
#[tauri::command]
#[specta::specta]
pub fn store_close(app: AppHandle) -> Result<(), String> {
    store_state(&app)?.close();
    Ok(())
}

/// Dependency-free base64 (standard alphabet, padded) — for building `data:`
/// URLs from small media blobs without pulling a crate.
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Fetch a content-addressed media blob (`media/<hash>.<ext>`) — cache-first,
/// then the base URLs. Content-addressed blobs are immutable, so the on-disk
/// cache never goes stale and a hit avoids the network entirely (low-RAM: no
/// resident media, and the webview drops the bytes when the detail closes).
async fn fetch_media(app: &AppHandle, sha256: &str, ext: &str) -> Result<Vec<u8>, String> {
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid media hash".into());
    }
    let state = store_state(app)?;
    let name = format!("media/{sha256}.{ext}");
    let cache_path = state.cache_dir.join(format!("{sha256}.{ext}"));
    if let Some(bytes) = read_bounded_file(&cache_path, STORE_MEDIA_MAX_BYTES) {
        if grain_core::trust::sha256_hex(&bytes) == sha256 {
            return Ok(bytes);
        }
    }
    let client = app
        .try_state::<reqwest::Client>()
        .map(|c| c.inner().clone())
        .ok_or("http client unavailable")?;
    let bases: Vec<String> = {
        let roots = state.roots.read().unwrap();
        roots
            .base_urls
            .iter()
            .chain(roots.mirrors.iter())
            .cloned()
            .collect()
    };
    for base in &bases {
        if let Some(bytes) = fetch(&client, base, &name, STORE_MEDIA_MAX_BYTES).await {
            // Content integrity: the bytes MUST hash to the requested id.
            if grain_core::trust::sha256_hex(&bytes) != sha256 {
                continue;
            }
            let _ = std::fs::create_dir_all(&state.cache_dir);
            let _ = std::fs::write(&cache_path, &bytes);
            return Ok(bytes);
        }
    }
    Err("media not reachable".into())
}

/// One extension's catalogue metadata (description, installs, README + media
/// refs) from the CACHED signed index — no network, and the parsed index is
/// dropped immediately, so an installed extension's detail page can show the
/// same header as the store without keeping the catalogue resident.
/// `None` when the id is not in the catalogue (e.g. a locally imported pack).
#[tauri::command]
#[specta::specta]
pub fn store_entry(app: AppHandle, id: String) -> Result<Option<StoreEntry>, String> {
    let state = store_state(&app)?;
    let roots = state.roots.read().unwrap().clone();
    let loaded = load_cached_index(&state.cache_dir, &roots, None, now_unix()).or_else(|_| {
        trust::verify_index(
            &roots,
            trust::SEED_INDEX.as_bytes(),
            trust::SEED_INDEX_SIG,
            None,
            now_unix(),
            true,
        )
    });
    let Ok((index, _)) = loaded else {
        return Ok(None);
    };
    let revocations = state.revocations.read().unwrap();
    // Newest entry wins when several versions are published for this id.
    let matches: Vec<IndexEntry> = index
        .entries
        .iter()
        .filter(|e| e.id == id)
        .cloned()
        .collect();
    let Some(entry) = matches.last() else {
        return Ok(None);
    };
    Ok(project_entries(std::slice::from_ref(entry), &revocations)
        .into_iter()
        .next())
}

/// One installed extension's cover reference.
#[derive(Clone, Serialize, specta::Type)]
pub struct StoreCover {
    pub id: String,
    pub sha256: String,
    pub kind: String,
}

/// Cover references for a set of ids, from the CACHED index in ONE parse.
///
/// The installed list shows each extension's picture, and asking `store_entry`
/// per row would re-read and re-verify the whole catalogue once per extension.
/// This reads it once, keeps only `(id, cover)`, and drops the rest — the list
/// gets its images without the catalogue ever staying resident. Ids that are not
/// in the catalogue, or have no media, are simply absent from the result.
#[tauri::command]
#[specta::specta]
pub fn store_covers(app: AppHandle, ids: Vec<String>) -> Result<Vec<StoreCover>, String> {
    let state = store_state(&app)?;
    let roots = state.roots.read().unwrap().clone();
    let loaded = load_cached_index(&state.cache_dir, &roots, None, now_unix()).or_else(|_| {
        trust::verify_index(
            &roots,
            trust::SEED_INDEX.as_bytes(),
            trust::SEED_INDEX_SIG,
            None,
            now_unix(),
            true,
        )
    });
    let Ok((index, _)) = loaded else {
        return Ok(Vec::new());
    };
    let mut covers: Vec<StoreCover> = Vec::new();
    for id in ids {
        // Newest published entry wins, matching `store_entry`.
        if let Some(entry) = index.entries.iter().filter(|e| e.id == id).next_back() {
            if entry.validate_tool_only().is_err() {
                continue;
            }
            if let Some(m) = entry.media.first() {
                covers.push(StoreCover {
                    id,
                    sha256: m.sha256.clone(),
                    kind: m.kind.clone(),
                });
            }
        }
    }
    Ok(covers)
}

/// A screenshot/GIF for the detail page, as a `data:` URL. Lazy — called only
/// when a detail opens — and integrity-checked against its hash. WEBP or GIF.
#[tauri::command]
#[specta::specta]
pub async fn store_media(app: AppHandle, sha256: String, kind: String) -> Result<String, String> {
    let (ext, mime) = match kind.as_str() {
        "webp" => ("webp", "image/webp"),
        "gif" => ("gif", "image/gif"),
        _ => return Err("unsupported media kind".into()),
    };
    let bytes = fetch_media(&app, &sha256, ext).await?;
    Ok(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

/// An extension's full README (markdown text), by its media hash. Lazy, and
/// integrity-checked. Rendered on the detail page.
#[tauri::command]
#[specta::specta]
pub async fn store_readme(app: AppHandle, sha256: String) -> Result<String, String> {
    let bytes = fetch_media(&app, &sha256, "md").await?;
    String::from_utf8(bytes).map_err(|_| "readme is not valid UTF-8".to_string())
}

/// Install (or update to) a specific verified `(id, version)`. In-app click
/// only — a link may open the store but never trigger this.
#[tauri::command]
#[specta::specta]
pub async fn store_install(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    version: String,
) -> Result<(), String> {
    crate::grain_commands::require_main_window(&window)?;
    let state = store_state(&app)?;
    let reg = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .map(|r| r.inner().clone())
        .ok_or("extensions registry unavailable")?;
    let client = app
        .try_state::<reqwest::Client>()
        .map(|c| c.inner().clone())
        .ok_or("http client unavailable")?;
    let root = ext_root(&app)?;
    install_entry(&state, &reg, &root, &client, &id, &version).await?;
    // Store artifacts were registry-built, but decoding still fails closed at
    // the consumer boundary. A bad icon degrades the card rather than undoing a
    // cryptographically valid extension install.
    if let Ok(pack) = crate::extension_host::load_manifest_result(&app, &id) {
        if let Err(error) = crate::extension_icons::materialize_pack(&app, &pack) {
            log::warn!("[GRAIN] store: could not materialize icon for {id}: {error}");
        }
    }
    crate::extension_host::refresh_index(&app);
    Ok(())
}

/// Banners for installed extensions that have been revoked or deprecated —
/// enforced from the resident (cached) revocation list, so it holds offline.
#[tauri::command]
#[specta::specta]
pub fn store_revocation_banners(app: AppHandle) -> Result<Vec<RevocationBanner>, String> {
    let state = store_state(&app)?;
    let reg = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let revocations = state.revocations.read().unwrap();
    let mut out = Vec::new();
    for rec in reg.records() {
        if let Some(s) = revocations.state_for(&rec.id, &rec.installed_version) {
            out.push(RevocationBanner {
                id: rec.id.clone(),
                state: match s {
                    RevocationState::Revoked => "revoked",
                    RevocationState::Deprecated => "deprecated",
                }
                .to_string(),
                reason: revocations
                    .entries
                    .iter()
                    .find(|e| e.id == rec.id)
                    .map(|e| e.reason.clone())
                    .unwrap_or_default(),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_data(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        // Unique per call so parallel tests never share (and wipe) a dir.
        let uniq = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!(
            "grain-store-test-{}-{}-{}",
            std::process::id(),
            label,
            uniq
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // Synthetic RAM proof for the overhead rule (DISTRIBUTION-PLAN §5.3): with
    // the store closed, the parsed index is NOT resident — only the small
    // roots + revocations remain. This is a memory-ownership assertion, not a
    // process-RSS measurement (that needs a live app run).
    #[test]
    fn parsed_index_is_dropped_on_close() {
        let data = tmp_data("close");
        let state = StoreState::init(&data);
        // Opening loads the (seed) index into memory.
        let _ = state.view_from_resident();
        assert!(
            state.index.read().unwrap().is_some(),
            "index should be resident while the store is open"
        );
        // Closing drops it — idle footprint returns to roots + revocations only.
        state.close();
        assert!(
            state.index.read().unwrap().is_none(),
            "parsed index must be dropped on close (overhead rule)"
        );
        let _ = std::fs::remove_dir_all(&data);
    }

    // End-to-end client path against a REAL signed catalogue (the committed
    // fixture produced by `grain-registry publish`): cache load → verify roots
    // against the PINNED keys → verify index against the publishing key →
    // project entries. Proves the producer (5B) and verifier (5A) agree, and
    // that signature verification does not grant retired runtime capabilities.
    #[test]
    fn verified_fixture_catalogue_loads_from_cache() {
        let data = tmp_data("fixture");
        let store = data.join("store");
        std::fs::create_dir_all(&store).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("store");
        for f in [
            "roots.json",
            "roots.json.minisig",
            "index.json",
            "index.json.minisig",
        ] {
            std::fs::copy(fixture.join(f), store.join(f))
                .unwrap_or_else(|e| panic!("copy fixture {f}: {e}"));
        }
        let state = StoreState::init(&data);
        let view = state.view_from_resident();
        assert!(
            view.entries.is_empty(),
            "signed retired entries stay hidden"
        );
        let index = state.index.read().unwrap();
        let entry = &index.as_ref().expect("verified cached index").entries[0];
        assert_eq!(entry.id, "com.example.hello");
        assert_eq!(entry.trust, grain_sdk::Trust::Verified);
        assert_eq!(entry.capabilities, vec!["transform:transcript"]);
        drop(index);
        let _ = std::fs::remove_dir_all(&data);
    }

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("store")
    }

    // Verify the immutable signed legacy catalogue, then use an ephemeral
    // listener as a tripwire. No server thread or fixed-port skip is needed:
    // retirement must refuse the install before even opening a connection.
    #[test]
    fn signed_legacy_install_is_refused_before_download() {
        let data = tmp_data("retired-install");
        let state = StoreState::init(&data);
        let fx = fixture_dir();
        let roots = trust::verify_roots(
            &std::fs::read(fx.join("roots.json")).unwrap(),
            &std::fs::read_to_string(fx.join("roots.json.minisig")).unwrap(),
        )
        .unwrap();
        let (index, status) = trust::verify_index(
            &roots,
            &std::fs::read(fx.join("index.json")).unwrap(),
            &std::fs::read_to_string(fx.join("index.json.minisig")).unwrap(),
            None,
            1_787_270_400,
            false,
        )
        .unwrap();
        assert!(matches!(status, IndexStatus::Fresh));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        // Change only the test transport destination after signature verification.
        // The index, permissions, artifact identity and signatures stay untouched.
        let mut transport_roots = roots;
        transport_roots.base_urls = vec![format!("http://{}/", listener.local_addr().unwrap())];
        transport_roots.mirrors.clear();
        *state.roots.write().unwrap() = transport_roots;
        *state.index.write().unwrap() = Some(index);
        let reg = grain_core::extensions::ExtensionsRegistry::load(&data, false).unwrap();
        let ext_root = data.join("extensions");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(300))
            .build()
            .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let error = rt
            .block_on(install_entry(
                &state,
                &reg,
                &ext_root,
                &client,
                "com.example.hello",
                "1.0.0",
            ))
            .unwrap_err();
        assert!(error.contains("transform:transcript"), "{error}");
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(reg.record("com.example.hello").is_none());
        assert!(!ext_root.join("com.example.hello").exists());
        state.close();
        drop(listener);
        let _ = std::fs::remove_dir_all(&data);
    }

    // Exercise the real download/hash/install path with an embedded tool pack.
    // Metadata is synthetic here; the separate legacy fixture verifies signatures.
    #[test]
    fn tool_download_installs_disabled_and_rejects_wrong_hash() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            for corrupt in [false, true] {
                let data = tmp_data(if corrupt { "hash" } else { "tool" });
                let state = StoreState::init(&data);
                let bytes = serde_json::to_vec(&serde_json::json!({
                    "manifest": {"id": "com.example.tools", "name": "Test tools",
                        "version": "1.0.0", "grainApi": "^1.0", "tier": "scripted",
                        "entry_source": "grain.actions({});", "permissions": []},
                    "payloads": {}
                })).unwrap();
                let hash = trust::sha256_hex(&bytes);
                let entry: IndexEntry = serde_json::from_value(serde_json::json!({
                    "id": "com.example.tools", "name": "Test tools", "version": "1.0.0",
                    "tier": "scripted", "trust": "verified", "sha256": hash,
                    "size": bytes.len(), "categories": ["tools"]
                })).unwrap();
                assert_eq!(project_entries(std::slice::from_ref(&entry),
                    &state.revocations.read().unwrap()).len(), 1);
                *state.index.write().unwrap() = Some(grain_sdk::Index {
                    spec: 1, version: 1, expires: "2099-01-01T00:00:00Z".into(),
                    entries: vec![entry],
                });
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                {
                    let mut roots = state.roots.write().unwrap();
                    roots.base_urls = vec![format!("http://{}/", listener.local_addr().unwrap())];
                    roots.mirrors.clear();
                }
                let server = async {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut request = [0u8; 2048];
                    let mut count = 0;
                    while !request[..count].windows(4).any(|part| part == b"\r\n\r\n") {
                        assert!(count < request.len(), "oversized request header");
                        let read = socket.read(&mut request[count..]).await.unwrap();
                        assert_ne!(read, 0, "truncated request header");
                        count += read;
                    }
                    assert!(String::from_utf8_lossy(&request[..count])
                        .starts_with(&format!("GET /blob/{hash}.grainpack HTTP/1.1")));
                    let mut body = bytes.clone();
                    if corrupt { body[0] = b'!'; }
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
                    socket.write_all(&body).await.unwrap();
                };
                let reg = grain_core::extensions::ExtensionsRegistry::load(&data, false).unwrap();
                let ext_root = data.join("extensions");
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(2)).build().unwrap();
                let install = install_entry(&state, &reg, &ext_root, &client,
                    "com.example.tools", "1.0.0");
                // Both futures and all sockets are owned by this bounded scope.
                let (_, result) = tokio::time::timeout(std::time::Duration::from_secs(3),
                    async { tokio::join!(server, install) }).await.unwrap();
                if corrupt {
                    assert!(result.unwrap_err().contains("artifact verification failed"));
                    assert!(reg.record("com.example.tools").is_none());
                    assert!(!ext_root.join("com.example.tools").exists());
                } else {
                    assert!(result.unwrap().join("pack.grainpack.json").exists());
                    let record = reg.record("com.example.tools").unwrap();
                    assert_eq!(record.trust, grain_sdk::Trust::Verified);
                    assert!(!record.enabled);
                }
                state.close();
                drop(listener);
                let _ = std::fs::remove_dir_all(&data);
            }
        });
    }

    // Read-only live catalogue compatibility check; no legacy artifact install.
    #[test]
    #[ignore = "network: fetches the live GitHub-hosted catalogue"]
    fn live_github_catalogue_exposes_only_tools() {
        let data = tmp_data("live");
        let state = StoreState::init(&data);
        let client = reqwest::Client::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let view = rt.block_on(refresh(&state, &client));
        assert_eq!(view.status, "fresh", "live index verified + fresh");
        let index = state.index.read().unwrap();
        let raw = index.as_ref().expect("verified live index");
        for entry in &view.entries {
            raw.entries
                .iter()
                .find(|candidate| candidate.id == entry.id && candidate.version == entry.version)
                .unwrap()
                .validate_tool_only()
                .unwrap();
        }
        drop(index);
        state.close();
        let _ = std::fs::remove_dir_all(&data);
    }

    // The seed loads and verifies at startup, so a fresh install has a working
    // (if empty) offline store.
    #[test]
    fn seed_boots_a_working_store() {
        let data = tmp_data("seed");
        let state = StoreState::init(&data);
        let view = state.view_from_resident();
        // Empty seed catalogue, but it renders (offline) rather than erroring.
        assert_eq!(view.status, "offline");
        assert!(view.entries.is_empty());
        let _ = std::fs::remove_dir_all(&data);
    }
}
