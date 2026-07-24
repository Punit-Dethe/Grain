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

/// The resident store state (registered as managed Tauri state). Roots and
/// revocations are small and stay loaded; the parsed index is present only
/// while the slide-over is open.
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
    pub size: String,
    pub author: String,
    pub reviewed_at: String,
    pub reviewed_commit: String,
    /// Revocation state for this exact version, if any: "revoked" | "deprecated".
    pub revocation: Option<String>,
    /// Flagged capability combinations (DISTRIBUTION-PLAN §3.3), plain-language,
    /// so the card tells the user what the reviewer was warned about.
    pub flags: Vec<String>,
}

/// What the store slide-over shows when opened.
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

fn trust_str(t: grain_sdk::Trust) -> &'static str {
    match t {
        grain_sdk::Trust::Dev => "dev",
        grain_sdk::Trust::Experimental => "experimental",
        grain_sdk::Trust::Verified => "verified",
        grain_sdk::Trust::Core => "core",
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

    /// Drop the parsed index (store slide-over closed). Roots + revocations stay.
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
        .map(|e| StoreEntry {
            id: e.id.clone(),
            name: e.name.clone(),
            version: e.version.clone(),
            tier: tier_str(&e.tier).into(),
            trust: trust_str(e.trust).into(),
            capabilities: e.capabilities.clone(),
            size: e.size.to_string(),
            author: e.author.clone(),
            reviewed_at: e.reviewed_at.clone(),
            reviewed_commit: e.reviewed_commit.clone(),
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
    let doc = std::fs::read(dir.join(name)).ok()?;
    let sig = std::fs::read_to_string(dir.join(format!("{name}.minisig"))).ok()?;
    Some((doc, sig))
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

async fn fetch(client: &reqwest::Client, base: &str, name: &str) -> Option<Vec<u8>> {
    let url = format!("{}{}", base.trim_end_matches('/').to_string() + "/", name);
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.bytes().await.ok().map(|b| b.to_vec())
}

async fn fetch_text(client: &reqwest::Client, base: &str, name: &str) -> Option<String> {
    fetch(client, base, name)
        .await
        .and_then(|b| String::from_utf8(b).ok())
}

/// Refresh roots → index → revocations from the first reachable base URL,
/// verifying each, caching on success, and applying revocations. Returns the
/// resulting [`StoreView`]; on total network failure it falls back to cache.
pub async fn refresh(state: &StoreState, client: &reqwest::Client) -> StoreView {
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
            fetch(client, base, "roots.json").await,
            fetch_text(client, base, "roots.json.minisig").await,
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
            fetch(client, base, "index.json").await,
            fetch_text(client, base, "index.json.minisig").await,
        ) else {
            continue;
        };
        let Ok((index, status)) =
            trust::verify_index(&roots, &idoc, &isig, stored, now_unix(), false)
        else {
            continue;
        };

        // revocations.json — verify and apply; missing is not fatal.
        if let (Some(vdoc), Some(vsig)) = (
            fetch(client, base, "revocations.json").await,
            fetch_text(client, base, "revocations.json.minisig").await,
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
        if let Some(b) = fetch(client, base, &blob_name).await {
            bytes = Some(b);
            break;
        }
    }
    let bytes = bytes.ok_or("could not download the artifact from any host")?;

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

/// Close the store slide-over: drop the parsed index so idle footprint returns
/// to just the small roots + revocations.
#[tauri::command]
#[specta::specta]
pub fn store_close(app: AppHandle) -> Result<(), String> {
    store_state(&app)?.close();
    Ok(())
}

/// Install (or update to) a specific verified `(id, version)`. In-app click
/// only — a link may open the store but never trigger this.
#[tauri::command]
#[specta::specta]
pub async fn store_install(app: AppHandle, id: String, version: String) -> Result<(), String> {
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
    // that a verified entry surfaces with its real trust.
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
        assert_eq!(view.entries.len(), 1, "the fixture catalogue has one entry");
        let e = &view.entries[0];
        assert_eq!(e.id, "com.example.hello");
        assert_eq!(e.trust, "verified", "trust comes from the signed index");
        assert_eq!(e.tier, "scripted");
        assert!(e.revocation.is_none());
        let _ = std::fs::remove_dir_all(&data);
    }

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("store")
    }

    /// A dead-simple blocking file server for the fixture `v1/` tree. Serves any
    /// existing file by request path; 404 otherwise. One request per connection.
    fn serve_v1(root: PathBuf) -> Option<std::net::SocketAddr> {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        // The signed fixture roots.json names 127.0.0.1:8787, so bind exactly it.
        let listener = TcpListener::bind("127.0.0.1:8787").ok()?;
        let addr = listener.local_addr().ok()?;
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .trim_start_matches('/')
                    .to_string();
                let file = root.join(&path);
                let body = std::fs::read(&file).ok();
                let resp = match body {
                    Some(bytes) => {
                        let mut head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            bytes.len()
                        )
                        .into_bytes();
                        head.extend_from_slice(&bytes);
                        head
                    }
                    None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                };
                let _ = stream.write_all(&resp);
                let _ = stream.flush();
            }
        });
        Some(addr)
    }

    // FULL client end-to-end over real HTTP against the signed catalogue:
    // serve v1/ on localhost → StoreState refresh (verify roots+index) → install
    // the artifact (fetch blob → verify sha256 → unpack → registry record with
    // trust) → confirm the pack loads from its versioned dir. This is the honest
    // synthetic equivalent of the in-app store click, minus the GUI and worker
    // execution.
    #[test]
    fn http_end_to_end_install_from_signed_catalogue() {
        // Build the served tree = fixture (roots/index/sigs + blob).
        let data = tmp_data("e2e");
        let served = data.join("served_v1");
        std::fs::create_dir_all(served.join("blob")).unwrap();
        let fx = fixture_dir();
        for f in [
            "roots.json",
            "roots.json.minisig",
            "index.json",
            "index.json.minisig",
        ] {
            std::fs::copy(fx.join(f), served.join(f)).unwrap();
        }
        for e in std::fs::read_dir(fx.join("blob")).unwrap().flatten() {
            std::fs::copy(e.path(), served.join("blob").join(e.file_name())).unwrap();
        }

        let Some(_addr) = serve_v1(served) else {
            eprintln!("SKIP: could not bind 127.0.0.1:8787 (port busy)");
            return;
        };

        // Seed the store cache with the fixture roots so init picks up the
        // localhost base URL, then refresh + install over HTTP.
        let store_cache = data.join("store");
        std::fs::create_dir_all(&store_cache).unwrap();
        for f in ["roots.json", "roots.json.minisig"] {
            std::fs::copy(fx.join(f), store_cache.join(f)).unwrap();
        }
        let state = StoreState::init(&data);
        let reg = grain_core::extensions::ExtensionsRegistry::load(&data, false).unwrap();
        let ext_root = data.join("extensions");
        let client = reqwest::Client::new();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let view = rt.block_on(refresh(&state, &client));
        assert_eq!(view.status, "fresh", "index verified + fresh over HTTP");
        assert_eq!(view.entries.len(), 1);
        assert!(view.can_install);

        let dir = rt
            .block_on(install_entry(
                &state,
                &reg,
                &ext_root,
                &client,
                "com.example.hello",
                "1.0.0",
            ))
            .expect("install over HTTP");
        assert!(dir.join("pack.grainpack.json").exists(), "artifact unpacked");

        let rec = reg.record("com.example.hello").expect("registry record");
        assert_eq!(rec.trust, grain_sdk::Trust::Verified, "trust from signed index");
        assert_eq!(rec.installed_version, "1.0.0");
        assert!(!rec.enabled, "fresh install lands disabled (enable is explicit)");

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
