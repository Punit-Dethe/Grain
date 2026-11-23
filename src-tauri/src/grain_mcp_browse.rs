//! Curated 1-click Browser Control setup backed by `anomalyco/browse` (MIT).
//!
//! Downloads the official native Rust `browse` binary from Anomaly's release
//! manifest (verifying exact size and SHA-256), registers it as a normal local
//! stdio MCP connection in Grain's `ConnectionRegistry`, triggers browser
//! extension installation/pairing via `browse install --agent none --no-modify-path`,
//! and cleanly removes both connection and managed binary on uninstall.
//!
//! Nothing runs in the background when idle: the MCP server uses Grain's
//! standard `grain_mcp_local` lazy spawn + 3-minute idle reaper.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use flate2::read::GzDecoder;
use grain_core::mcp_connections::ConnectionDefinition;
use grain_sdk::mcp::McpConnection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use tar::Archive;
use tauri::{AppHandle, Manager, WebviewWindow};

use super::{connections, local};

const MANIFEST_URL: &str = "https://anoma.ly/release/browse/cli/stable.json";
const DOWNLOAD_ORIGIN: &str = "https://anoma.ly";
pub(super) const CHROME_WEB_STORE_URL: &str =
    "https://chromewebstore.google.com/detail/mfnicocicmmlkpjnaffgihfjhdgjkdjg";
const CONNECTION_NAME: &str = "Browser (OpenCode Browse)";
const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_ARCHIVE_BYTES: usize = 32 * 1024 * 1024;
const PAIR_CODE_TIMEOUT: Duration = Duration::from_secs(12);
const PAIR_MAX_LIFETIME: Duration = Duration::from_secs(180);

#[cfg(windows)]
const BROWSE_EXE: &str = "browse.exe";
#[cfg(not(windows))]
const BROWSE_EXE: &str = "browse";

#[cfg(windows)]
const TAR_MEMBER: &str = "package/bin/browse.exe";
#[cfg(not(windows))]
const TAR_MEMBER: &str = "package/bin/browse";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Tracks at most one active `browse install` pairing child so clicking Pair
/// again cancels any stale waiter cleanly.
static ACTIVE_PAIR: OnceLock<Mutex<Option<tokio::sync::oneshot::Sender<()>>>> = OnceLock::new();

fn active_pair_slot() -> &'static Mutex<Option<tokio::sync::oneshot::Sender<()>>> {
    ACTIVE_PAIR.get_or_init(|| Mutex::new(None))
}

#[derive(Clone, Debug, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowseSetupStatus {
    pub installed: bool,
    pub managed_by_grain: bool,
    pub binary_path: Option<String>,
    pub version: Option<String>,
    pub connection_id: Option<String>,
    pub connection_revision: Option<String>,
    pub enabled: bool,
    pub extension_paired: Option<bool>,
    pub extension_connected: Option<bool>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowsePairResult {
    pub paired: bool,
    pub message: String,
    pub code: Option<String>,
}

#[derive(Deserialize)]
struct ReleaseManifest {
    app: String,
    artifact: String,
    channel: String,
    release: ReleaseEntry,
}

#[derive(Deserialize)]
struct ReleaseEntry {
    version: String,
    targets: std::collections::BTreeMap<String, TargetEntry>,
}

#[derive(Deserialize)]
struct TargetEntry {
    url: String,
    size: u64,
    sha256: String,
}

fn managed_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let context = app
        .try_state::<Arc<grain_core::AppContext>>()
        .ok_or("Application context unavailable.")?;
    Ok(context.data_dir.join("tools").join("browse"))
}

fn managed_binary_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(managed_dir(app)?.join(BROWSE_EXE))
}

fn detect_binary(app: &AppHandle) -> Result<Option<(PathBuf, bool)>, String> {
    let managed = managed_binary_path(app)?;
    if managed.is_file() {
        return Ok(Some((managed, true)));
    }
    if let Ok(found) = local::resolve_executable("browse") {
        return Ok(Some((found, false)));
    }
    Ok(None)
}

fn is_browse_connection(def: &McpConnection) -> bool {
    let McpConnection::Stdio { command, args, .. } = def else {
        return false;
    };
    if args.as_slice() != ["mcp"] {
        return false;
    }
    let file_name = Path::new(command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(command)
        .to_ascii_lowercase();
    file_name == "browse" || file_name == "browse.exe"
}

fn current_target_key() -> Result<&'static str, String> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            return Ok("windows-x64");
        }
        return Ok("windows-x64-baseline");
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        return Ok("windows-x64-baseline");
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return Ok("macos-arm64");
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            return Ok("macos-x64");
        }
        return Ok("macos-x64-baseline");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            return Ok("linux-x64");
        }
        return Ok("linux-x64-baseline");
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return Ok("linux-arm64");
    }
    #[allow(unreachable_code)]
    Err("OpenCode Browse has no prebuilt binary for this platform.".into())
}

fn build_download_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .user_agent(format!(
            "Grain/{} (browse-setup)",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|error| format!("Could not initialize HTTPS download client: {error}"))
}

fn extract_binary_from_tgz(archive_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let gz = GzDecoder::new(archive_bytes);
    let mut archive = Archive::new(gz);
    let entries = archive
        .entries()
        .map_err(|error| format!("Could not read Browse release archive: {error}"))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Corrupt Browse release archive entry: {error}"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|_| "Invalid entry path inside Browse archive.".to_string())?;
        let normalized = path.to_string_lossy().replace('\\', "/");
        if normalized == TAR_MEMBER {
            if entry.size() > MAX_ARCHIVE_BYTES as u64 {
                return Err("Extracted Browse binary exceeds size safety limit.".into());
            }
            let mut out = Vec::with_capacity(entry.size() as usize);
            entry
                .take(MAX_ARCHIVE_BYTES as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|error| format!("Failed extracting Browse binary: {error}"))?;
            if out.len() > MAX_ARCHIVE_BYTES || out.is_empty() {
                return Err("Extracted Browse binary has an invalid size.".into());
            }
            return Ok(out);
        }
    }
    Err(format!("Browse archive did not contain `{TAR_MEMBER}`."))
}

fn write_binary_atomically(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = dest
        .parent()
        .ok_or("Invalid Browse installation directory.")?;
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("Could not create Browse directory: {error}"))?;

    // Clean up stale rotated copies from earlier upgrades if unlocked.
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&format!("{BROWSE_EXE}.old-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    let staged = dir.join(format!(
        "{BROWSE_EXE}.new-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&staged, bytes)
        .map_err(|error| format!("Could not write staged Browse binary: {error}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    }

    if dest.exists() {
        let backup = dir.join(format!(
            "{BROWSE_EXE}.old-{}",
            uuid::Uuid::new_v4().simple()
        ));
        if let Err(error) = std::fs::rename(dest, &backup) {
            let _ = std::fs::remove_file(&staged);
            return Err(format!("Could not replace existing Browse binary: {error}"));
        }
        if let Err(error) = std::fs::rename(&staged, dest) {
            let _ = std::fs::rename(&backup, dest);
            let _ = std::fs::remove_file(&staged);
            return Err(format!("Could not finalize Browse binary install: {error}"));
        }
        let _ = std::fs::remove_file(&backup);
    } else if let Err(error) = std::fs::rename(&staged, dest) {
        let _ = std::fs::remove_file(&staged);
        return Err(format!("Could not move Browse binary into place: {error}"));
    }
    Ok(())
}

async fn download_and_install_browse(dest: &Path) -> Result<String, String> {
    let target_key = current_target_key()?;
    let client = build_download_client()?;

    let manifest_resp = client
        .get(MANIFEST_URL)
        .send()
        .await
        .map_err(|error| format!("Could not reach Browse release service: {error}"))?;
    if !manifest_resp.status().is_success() {
        return Err(format!(
            "Browse release service returned HTTP {}.",
            manifest_resp.status()
        ));
    }
    let manifest_bytes = read_bounded_response(manifest_resp, MAX_MANIFEST_BYTES).await?;
    let manifest: ReleaseManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| "Browse release manifest was invalid JSON.".to_string())?;
    if manifest.app != "browse" || manifest.artifact != "cli" || manifest.channel != "stable" {
        return Err("Browse release manifest identity mismatch.".into());
    }
    let target = manifest
        .release
        .targets
        .get(target_key)
        .ok_or_else(|| format!("Browse release has no `{target_key}` build."))?;
    if !target.url.starts_with('/')
        || target.size == 0
        || target.size as usize > MAX_ARCHIVE_BYTES
        || target.sha256.len() != 64
        || !target.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Browse release manifest target metadata is invalid.".into());
    }

    let archive_url = format!("{DOWNLOAD_ORIGIN}{}", target.url);
    let archive_resp = client
        .get(&archive_url)
        .send()
        .await
        .map_err(|error| format!("Could not download Browse binary: {error}"))?;
    if !archive_resp.status().is_success() {
        return Err(format!(
            "Browse binary download returned HTTP {}.",
            archive_resp.status()
        ));
    }
    let archive_bytes = read_bounded_response(archive_resp, MAX_ARCHIVE_BYTES).await?;
    if archive_bytes.len() as u64 != target.size {
        return Err(format!(
            "Browse archive size mismatch (expected {} bytes, got {}).",
            target.size,
            archive_bytes.len()
        ));
    }
    let actual_sha: String = Sha256::digest(&archive_bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !actual_sha.eq_ignore_ascii_case(&target.sha256) {
        return Err("Browse archive SHA-256 verification failed; nothing was installed.".into());
    }

    let dest_owned = dest.to_path_buf();
    let version = manifest.release.version.clone();
    tokio::task::spawn_blocking(move || {
        let bin = extract_binary_from_tgz(&archive_bytes)?;
        write_binary_atomically(&dest_owned, &bin)
    })
    .await
    .map_err(|_| "Browse extraction task failed.".to_string())??;

    Ok(version)
}

async fn read_bounded_response(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|len| len > max_bytes as u64)
    {
        return Err("Download exceeded Grain's size limit.".into());
    }
    let mut out = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Network read error during download: {error}"))?
    {
        if out.len().saturating_add(chunk.len()) > max_bytes {
            return Err("Download exceeded Grain's size limit.".into());
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

fn new_command(binary: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(binary);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd
}

async fn probe_browse_cli(
    binary: &Path,
) -> (Option<String>, Option<bool>, Option<bool>, Option<String>) {
    let mut cmd = new_command(binary);
    cmd.args(["status", "--json"]);
    let output = match tokio::time::timeout(Duration::from_secs(4), cmd.output()).await {
        Ok(Ok(out)) => out,
        _ => return (None, None, None, None),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let Ok(val) = serde_json::from_str::<serde_json::Value>(text.trim()) else {
        return (None, None, None, None);
    };
    parse_status_json(&val)
}

fn parse_status_json(
    val: &serde_json::Value,
) -> (Option<String>, Option<bool>, Option<bool>, Option<String>) {
    let version = val
        .pointer("/setup/version")
        .and_then(|v| v.as_str())
        .or_else(|| val.pointer("/relay/version").and_then(|v| v.as_str()))
        .or_else(|| val.get("version").and_then(|v| v.as_str()))
        .map(str::to_owned);
    let pairing_state = val
        .get("pairingState")
        .and_then(|v| v.as_str())
        .or_else(|| {
            val.pointer("/extension/pairingState")
                .and_then(|v| v.as_str())
        });
    let paired = match pairing_state {
        Some("paired") => Some(true),
        Some("unpaired") => Some(false),
        _ => val.pointer("/extension/paired").and_then(|v| v.as_bool()),
    };
    let connected = val.get("connected").and_then(|v| v.as_bool()).or_else(|| {
        val.pointer("/extension/connected")
            .and_then(|v| v.as_bool())
    });
    let note = val
        .get("nextStep")
        .and_then(|v| v.as_str())
        .or_else(|| val.pointer("/extension/nextStep").and_then(|v| v.as_str()))
        .map(str::to_owned);
    (version, paired, connected, note)
}

fn parse_pairing_code(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("Pairing code:")?.trim();
    let code = rest.split_whitespace().next()?;
    if code.len() >= 4
        && code.len() <= 32
        && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        Some(code.to_owned())
    } else {
        None
    }
}

fn find_browse_record(app: &AppHandle) -> Result<Option<(String, String, bool)>, String> {
    let state = app
        .try_state::<connections::State>()
        .ok_or("MCP connection storage is unavailable.")?;
    let registry = state.registry(app)?;
    let records = registry.list().map_err(|error| error.to_string())?;
    let settings = crate::settings::get_settings(app);
    for record in records {
        if is_browse_connection(record.definition()) {
            let enabled = settings
                .mcp_enabled_providers
                .iter()
                .any(|key| key == record.identity().vault_account().as_ref());
            return Ok(Some((
                record.identity().connection_id().to_owned(),
                record.revision().to_string(),
                enabled,
            )));
        }
    }
    Ok(None)
}

fn ensure_browse_registered(
    app: &AppHandle,
    binary: &Path,
) -> Result<(String, String, bool), String> {
    if let Some(existing) = find_browse_record(app)? {
        return Ok(existing);
    }
    let state = app
        .try_state::<connections::State>()
        .ok_or("MCP connection storage is unavailable.")?;
    let registry = state.registry(app)?;
    let def = ConnectionDefinition::validate(McpConnection::Stdio {
        name: CONNECTION_NAME.into(),
        command: binary.to_string_lossy().into_owned(),
        args: vec!["mcp".into()],
        cwd: None,
        env_keys: Vec::new(),
    })
    .map_err(|error| error.to_string())?;
    let record = registry.insert(def).map_err(|error| error.to_string())?;
    Ok((
        record.identity().connection_id().to_owned(),
        record.revision().to_string(),
        false,
    ))
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_browse_status(
    app: AppHandle,
    window: WebviewWindow,
    probe_extension: bool,
) -> Result<BrowseSetupStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    let detected = detect_binary(&app)?;
    let registered = find_browse_record(&app)?;

    let (installed, managed_by_grain, binary_path) = match detected {
        Some((path, managed)) => (true, managed, Some(path)),
        None => (false, false, None),
    };

    let (version, extension_paired, extension_connected, note) = if probe_extension {
        if let Some(ref path) = binary_path {
            probe_browse_cli(path).await
        } else {
            (None, None, None, None)
        }
    } else {
        (None, None, None, None)
    };

    let (connection_id, connection_revision, enabled) = match registered {
        Some((id, rev, en)) => (Some(id), Some(rev), en),
        None => (None, None, false),
    };

    Ok(BrowseSetupStatus {
        installed,
        managed_by_grain,
        binary_path: binary_path.map(|p| p.to_string_lossy().into_owned()),
        version,
        connection_id,
        connection_revision,
        enabled,
        extension_paired,
        extension_connected,
        note,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_browse_install(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<BrowseSetupStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    let (binary_path, managed_by_grain, version) = match detect_binary(&app)? {
        Some((existing, managed)) => (existing, managed, None),
        None => {
            let dest = managed_binary_path(&app)?;
            let version = download_and_install_browse(&dest).await?;
            (dest, true, Some(version))
        }
    };

    let (connection_id, connection_revision, enabled) =
        ensure_browse_registered(&app, &binary_path)?;

    Ok(BrowseSetupStatus {
        installed: true,
        managed_by_grain,
        binary_path: Some(binary_path.to_string_lossy().into_owned()),
        version,
        connection_id: Some(connection_id),
        connection_revision: Some(connection_revision),
        enabled,
        extension_paired: None,
        extension_connected: None,
        note: None,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_browse_pair(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<BrowsePairResult, String> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    crate::grain_commands::require_main_window(&window)?;
    let Some((binary_path, _)) = detect_binary(&app)? else {
        return Err("Install Browser Control first before pairing the Chrome extension.".into());
    };

    // Cancel any previous in-flight pairing waiter.
    if let Some(prev) = active_pair_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    {
        let _ = prev.send(());
    }

    // Use `browse pair --wait-seconds 180` directly: it starts the local relay
    // if needed, prints a 1-time pairing code (valid 10m) immediately on stdout,
    // and never copies binaries to ~/.local/bin or touches other AI agents.
    let mut cmd = new_command(&binary_path);
    cmd.args(["pair", "--wait-seconds", "180"]);
    let mut child = cmd
        .spawn()
        .map_err(|error| format!("Could not launch Browse pairing: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or("Could not read pairing output from Browse.")?;
    let mut lines = BufReader::new(stdout).lines();

    let found_code = tokio::time::timeout(PAIR_CODE_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(code) = parse_pairing_code(&line) {
                return Some(code);
            }
        }
        None
    })
    .await
    .map_err(|_| "Timed out waiting for Browse to generate a pairing code.".to_string())?;

    let Some(code) = found_code else {
        let _ = child.kill().await;
        return Err("Browse exited before printing a pairing code. Check status or retry.".into());
    };

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    *active_pair_slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(cancel_tx);

    // Keep draining stdout in the background while `browse pair` waits for the
    // browser extension to redeem the code (up to 3 minutes).
    tokio::spawn(async move {
        let drain = async {
            while let Ok(Some(_)) = lines.next_line().await {}
            let _ = child.wait().await;
        };
        tokio::select! {
            _ = cancel_rx => {
                let _ = child.kill().await;
            }
            _ = tokio::time::sleep(PAIR_MAX_LIFETIME) => {
                let _ = child.kill().await;
            }
            _ = drain => {}
        }
    });

    Ok(BrowsePairResult {
        paired: false,
        message: "In Chrome/Edge/Brave: click the browse extension icon → Options → Pair with a code (or click \"Connect to it\" at the bottom of the welcome page), paste this code, and click Connect.".into(),
        code: Some(code),
    })
}

#[tauri::command]
#[specta::specta]
pub fn mcp_browse_open_store(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    crate::grain_commands::require_main_window(&window)?;
    app.opener()
        .open_url(CHROME_WEB_STORE_URL, None::<&str>)
        .map_err(|error| format!("Could not open Chrome Web Store: {error}"))
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_browse_uninstall(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    crate::grain_commands::require_main_window(&window)?;

    if let Some(prev) = active_pair_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    {
        let _ = prev.send(());
    }

    if let Some((id, rev, _)) = find_browse_record(&app)? {
        connections::mcp_connection_remove(app.clone(), window.clone(), id, rev).await?;
    }

    if let Some((binary_path, managed)) = detect_binary(&app)? {
        if managed {
            // Stop browse's local relay before removing the binary.
            let mut stop_cmd = new_command(&binary_path);
            stop_cmd.arg("stop");
            let _ = tokio::time::timeout(Duration::from_secs(3), stop_cmd.output()).await;
            let dir = managed_dir(&app)?;
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};

    #[test]
    fn extract_binary_reads_only_expected_member_and_rejects_missing() {
        let mut tar_buf = Vec::new();
        {
            let gz = GzEncoder::new(&mut tar_buf, Compression::default());
            let mut builder = tar::Builder::new(gz);
            let payload = b"fake-browse-binary";
            let mut header = tar::Header::new_gnu();
            header.set_size(payload.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, TAR_MEMBER, &payload[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        assert_eq!(
            extract_binary_from_tgz(&tar_buf).unwrap(),
            b"fake-browse-binary"
        );

        let mut bad_buf = Vec::new();
        {
            let gz = GzEncoder::new(&mut bad_buf, Compression::default());
            let mut builder = tar::Builder::new(gz);
            let payload = b"wrong-file";
            let mut header = tar::Header::new_gnu();
            header.set_size(payload.len() as u64);
            header.set_cksum();
            builder
                .append_data(&mut header, "package/other.txt", &payload[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        assert!(extract_binary_from_tgz(&bad_buf).is_err());
    }

    #[test]
    fn browse_connection_matcher_identifies_browse_stdio_entries_only() {
        assert!(is_browse_connection(&McpConnection::Stdio {
            name: "Browse".into(),
            command: r"C:\Users\me\AppData\Roaming\com.grain.app\tools\browse\browse.exe".into(),
            args: vec!["mcp".into()],
            cwd: None,
            env_keys: Vec::new(),
        }));
        assert!(is_browse_connection(&McpConnection::Stdio {
            name: "Browse".into(),
            command: "browse".into(),
            args: vec!["mcp".into()],
            cwd: None,
            env_keys: Vec::new(),
        }));
        assert!(!is_browse_connection(&McpConnection::Stdio {
            name: "Playwright".into(),
            command: "npx".into(),
            args: vec!["@playwright/mcp".into()],
            cwd: None,
            env_keys: Vec::new(),
        }));
        assert!(!is_browse_connection(&McpConnection::LocalHttp {
            name: "Browse".into(),
            url: "http://127.0.0.1:19988/mcp".into(),
            headers: Default::default(),
        }));
    }

    #[test]
    fn pairing_code_and_status_json_parsers_match_real_browse_output() {
        assert_eq!(
            parse_pairing_code("Pairing code: EB7R-DY40"),
            Some("EB7R-DY40".into())
        );
        assert_eq!(parse_pairing_code("Other output"), None);

        let status = serde_json::json!({
            "connected": true,
            "pairingState": "paired",
            "nextStep": null,
            "relay": { "version": "0.1.5" },
            "setup": { "version": "0.1.5" }
        });
        let (ver, paired, connected, note) = parse_status_json(&status);
        assert_eq!(ver.as_deref(), Some("0.1.5"));
        assert_eq!(paired, Some(true));
        assert_eq!(connected, Some(true));
        assert_eq!(note, None);
    }
}
