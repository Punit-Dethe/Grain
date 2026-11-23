//! Local MCP runtime: stdio child-process supervisor with lazy spawn, warm
//! session reuse, idle reaping, Windows JobObject / CREATE_NO_WINDOW isolation,
//! bounded stderr capture, and OS credential vault storage for local env/auth.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{CommandWrap, KillOnDrop};
#[cfg(windows)]
use process_wrap::tokio::{CommandWrapper, JobObject};
use rmcp::model::{ClientCapabilities, ClientConfig, Implementation, ProtocolVersion};
use rmcp::service::{Peer, RoleClient};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde::Serialize;
use specta::Type;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::watch;

use super::{vault_parts, SdkService, CLEANUP_TIMEOUT};

pub(super) const LOCAL_ENV_SERVICE: &str = "com.grain.mcp.local-env";
pub(super) const LOCAL_AUTH_SERVICE: &str = "com.grain.mcp.local-auth";
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_STDERR_LINES: usize = 64;
const MAX_STDERR_BYTES: usize = 16 * 1024;
const MAX_STDERR_LINE_BYTES: usize = 512;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_SUSPENDED: u32 = 0x0000_0004;

/// Sets `CREATE_NO_WINDOW` while preserving `CREATE_SUSPENDED` required by
/// `JobObject` when registered after `JobObject` in `CommandWrap`.
#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct NoWindowWithJob;

#[cfg(windows)]
impl CommandWrapper for NoWindowWithJob {
    fn pre_spawn(
        &mut self,
        command: &mut tokio::process::Command,
        _core: &CommandWrap,
    ) -> std::io::Result<()> {
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Vault helpers (local env vars & local-http auth header)

struct KeyringIo<'a> {
    service: &'a str,
}

impl vault_parts::Io for KeyringIo<'_> {
    fn get(&mut self, account: &str) -> Result<Option<Vec<u8>>, String> {
        let entry = super::credential_entry(self.service, account)
            .map_err(|error| format!("OS credential vault open failed: {error}"))?;
        match entry.get_secret() {
            Ok(bytes) => Ok(Some(bytes)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(format!("OS credential vault read failed: {error}")),
        }
    }

    fn set(&mut self, account: &str, value: &[u8]) -> Result<(), String> {
        let entry = super::credential_entry(self.service, account)
            .map_err(|error| format!("OS credential vault open failed: {error}"))?;
        entry
            .set_secret(value)
            .map_err(|error| format!("OS credential vault write failed: {error}"))
    }

    fn delete(&mut self, account: &str) -> Result<(), String> {
        let entry = super::credential_entry(self.service, account)
            .map_err(|error| format!("OS credential vault open failed: {error}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(format!("OS credential vault delete failed: {error}")),
        }
    }
}

fn fresh_generation() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub(super) fn read_local_env_sync(account: &str) -> Result<BTreeMap<String, String>, String> {
    let mut io = KeyringIo {
        service: LOCAL_ENV_SERVICE,
    };
    let Some(bytes) = vault_parts::read(&mut io, account)? else {
        return Ok(BTreeMap::new());
    };
    serde_json::from_slice(&bytes)
        .map_err(|_| "Stored local MCP environment secrets are invalid.".to_string())
}

pub(super) fn write_local_env_sync(
    account: &str,
    env: &BTreeMap<String, String>,
) -> Result<(), String> {
    grain_core::mcp::validate_env_map(env).map_err(|error| error.to_string())?;
    let mut io = KeyringIo {
        service: LOCAL_ENV_SERVICE,
    };
    if env.is_empty() {
        return vault_parts::delete(&mut io, account);
    }
    let bytes = serde_json::to_vec(env)
        .map_err(|_| "Could not encode local MCP environment secrets.".to_string())?;
    vault_parts::write(&mut io, account, &bytes, &fresh_generation())
}

pub(super) fn delete_local_env_sync(account: &str) -> Result<(), String> {
    let mut io = KeyringIo {
        service: LOCAL_ENV_SERVICE,
    };
    vault_parts::delete(&mut io, account)
}

pub(super) fn read_local_auth_sync(account: &str) -> Result<Option<(String, String)>, String> {
    let mut io = KeyringIo {
        service: LOCAL_AUTH_SERVICE,
    };
    let Some(bytes) = vault_parts::read(&mut io, account)? else {
        return Ok(None);
    };
    let (name, value): (String, String) = serde_json::from_slice(&bytes)
        .map_err(|_| "Stored local MCP auth header is invalid.".to_string())?;
    let valid =
        grain_core::mcp::validate_local_auth_header(&name, &value).map_err(|e| e.to_string())?;
    Ok(Some(valid))
}

pub(super) fn write_local_auth_sync(
    account: &str,
    auth: Option<(String, String)>,
) -> Result<(), String> {
    let mut io = KeyringIo {
        service: LOCAL_AUTH_SERVICE,
    };
    let Some((name, value)) = auth else {
        return vault_parts::delete(&mut io, account);
    };
    let valid =
        grain_core::mcp::validate_local_auth_header(&name, &value).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(&valid)
        .map_err(|_| "Could not encode local MCP auth header.".to_string())?;
    vault_parts::write(&mut io, account, &bytes, &fresh_generation())
}

pub(super) fn delete_local_auth_sync(account: &str) -> Result<(), String> {
    let mut io = KeyringIo {
        service: LOCAL_AUTH_SERVICE,
    };
    vault_parts::delete(&mut io, account)
}

pub(super) fn has_local_auth_sync(account: &str) -> bool {
    read_local_auth_sync(account).ok().flatten().is_some()
}

pub(super) async fn read_local_env(account: String) -> Result<BTreeMap<String, String>, String> {
    tokio::task::spawn_blocking(move || read_local_env_sync(&account))
        .await
        .map_err(|_| "Local MCP environment read failed.".to_string())?
}

pub(super) async fn read_local_auth(account: String) -> Result<Option<(String, String)>, String> {
    tokio::task::spawn_blocking(move || read_local_auth_sync(&account))
        .await
        .map_err(|_| "Local MCP auth header read failed.".to_string())?
}

// ---------------------------------------------------------------------------
// Bounded stderr ring buffer & diagnostics

#[derive(Default)]
struct StderrRing {
    lines: Mutex<VecDeque<String>>,
    total_bytes: Mutex<usize>,
}

impl StderrRing {
    fn push(&self, raw: &str) {
        let trimmed = raw.trim_end_matches(&['\r', '\n'][..]);
        if trimmed.is_empty() {
            return;
        }
        let clipped = if trimmed.len() > MAX_STDERR_LINE_BYTES {
            let mut end = MAX_STDERR_LINE_BYTES;
            while !trimmed.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            format!("{}…", &trimmed[..end])
        } else {
            trimmed.to_owned()
        };
        let mut lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        let mut total = self.total_bytes.lock().unwrap_or_else(|e| e.into_inner());
        *total += clipped.len();
        lines.push_back(clipped);
        while lines.len() > MAX_STDERR_LINES || *total > MAX_STDERR_BYTES {
            if let Some(dropped) = lines.pop_front() {
                *total = total.saturating_sub(dropped.len());
            } else {
                break;
            }
        }
    }

    fn snapshot(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalMcpRuntimeStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalMcpLogs {
    pub running: bool,
    pub pid: Option<u32>,
    pub last_error: Option<String>,
    pub stderr: Vec<String>,
}

#[derive(Clone)]
pub(super) struct StdioLaunchSpec {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
}

struct WarmEntry {
    id: u64,
    binding: String,
    service: SdkService,
    pid: Option<u32>,
    activity_tx: watch::Sender<u64>,
    stop_tx: watch::Sender<bool>,
}

#[derive(Default)]
struct SavedDiagnostics {
    stderr: Option<Arc<StderrRing>>,
    last_error: Option<String>,
}

pub(super) struct Supervisor {
    idle_timeout: Duration,
    entries: Mutex<HashMap<String, WarmEntry>>,
    diagnostics: Mutex<HashMap<String, SavedDiagnostics>>,
    next_id: std::sync::atomic::AtomicU64,
}

static SUPERVISOR: OnceLock<Arc<Supervisor>> = OnceLock::new();

pub(super) fn supervisor() -> &'static Arc<Supervisor> {
    SUPERVISOR.get_or_init(|| Arc::new(Supervisor::new(DEFAULT_IDLE_TIMEOUT)))
}

impl Supervisor {
    pub(super) fn new(idle_timeout: Duration) -> Self {
        Self {
            idle_timeout,
            entries: Mutex::new(HashMap::new()),
            diagnostics: Mutex::new(HashMap::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Returns a warm `Peer` if already running for `(provider_id, binding)` and
    /// open; otherwise spawns the child process, runs MCP initialize, registers
    /// the idle reaper, and returns its `Peer`.
    pub(super) async fn acquire(
        self: &Arc<Self>,
        provider_id: &str,
        binding: &str,
        spec: StdioLaunchSpec,
        deadline: tokio::time::Instant,
    ) -> Result<Peer<RoleClient>, String> {
        let stale: Option<WarmEntry> = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = entries.get(provider_id) {
                if entry.binding == binding && !entry.service.is_closed() {
                    let next = entry.activity_tx.borrow().wrapping_add(1);
                    let _ = entry.activity_tx.send(next);
                    return Ok(entry.service.peer().clone());
                }
            }
            entries.remove(provider_id)
        };
        if let Some(old) = stale {
            shutdown_entry(old).await;
        }

        let resolved = match resolve_executable(&spec.command) {
            Ok(path) => path,
            Err(message) => {
                self.record_error(provider_id, &message, None);
                return Err(message);
            }
        };

        if let Some(ref cwd) = spec.cwd {
            if !Path::new(cwd).is_dir() {
                let message = format!("Working directory does not exist: {cwd}");
                self.record_error(provider_id, &message, None);
                return Err(message);
            }
        }

        let mut wrap = CommandWrap::with_new(&resolved, |cmd| {
            cmd.args(&spec.args);
            if let Some(ref cwd) = spec.cwd {
                cmd.current_dir(cwd);
            }
            for (k, v) in &spec.env {
                cmd.env(k, v);
            }
        });
        #[cfg(windows)]
        {
            wrap.wrap(JobObject);
            wrap.wrap(NoWindowWithJob);
            wrap.wrap(KillOnDrop);
        }
        #[cfg(unix)]
        {
            wrap.wrap(ProcessGroup::leader());
            wrap.wrap(KillOnDrop);
        }

        let (child, stderr_opt) = match TokioChildProcess::builder(wrap)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(parts) => parts,
            Err(error) => {
                let message = format!("Could not start `{}`: {error}", spec.command);
                self.record_error(provider_id, &message, None);
                return Err(message);
            }
        };

        let pid = child.id();
        let stderr = Arc::new(StderrRing::default());
        if let Some(child_stderr) = stderr_opt {
            let sink = stderr.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(child_stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    sink.push(&line);
                }
            });
        }

        let config = local_client_config();
        let service = match tokio::time::timeout_at(deadline, config.serve(child)).await {
            Ok(Ok(service)) => service,
            Ok(Err(error)) => {
                let tail = stderr.snapshot();
                let hint = tail
                    .last()
                    .map(|line| format!(" ({line})"))
                    .unwrap_or_default();
                let message = format!(
                    "Local MCP `{}` failed during handshake: {error}{hint}",
                    spec.command
                );
                self.record_error(provider_id, &message, Some(stderr));
                return Err(message);
            }
            Err(_) => {
                let message = format!("Local MCP `{}` timed out while starting.", spec.command);
                self.record_error(provider_id, &message, Some(stderr));
                return Err(message);
            }
        };

        let peer = service.peer().clone();
        let entry_id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (activity_tx, mut activity_rx) = watch::channel(0u64);
        let (stop_tx, mut stop_rx) = watch::channel(false);

        let replaced = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            let prev = entries.insert(
                provider_id.to_owned(),
                WarmEntry {
                    id: entry_id,
                    binding: binding.to_owned(),
                    service,
                    pid,
                    activity_tx,
                    stop_tx,
                },
            );
            let mut diags = self.diagnostics.lock().unwrap_or_else(|e| e.into_inner());
            diags.insert(
                provider_id.to_owned(),
                SavedDiagnostics {
                    stderr: Some(stderr),
                    last_error: None,
                },
            );
            prev
        };
        if let Some(old) = replaced {
            shutdown_entry(old).await;
        }

        // Spawn one lightweight idle reaper strictly for this live child.
        let sup = self.clone();
        let key = provider_id.to_owned();
        let idle = self.idle_timeout;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    changed = stop_rx.changed() => {
                        if changed.is_err() || *stop_rx.borrow() {
                            return;
                        }
                    }
                    changed = activity_rx.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        // Activity observed; restart idle timer.
                    }
                    _ = tokio::time::sleep(idle) => {
                        let evicted = {
                            let mut entries = sup.entries.lock().unwrap_or_else(|e| e.into_inner());
                            if entries.get(&key).is_some_and(|e| e.id == entry_id) {
                                entries.remove(&key)
                            } else {
                                None
                            }
                        };
                        if let Some(entry) = evicted {
                            log::info!("[GRAIN] Stopping idle local MCP `{key}`");
                            shutdown_entry(entry).await;
                        }
                        return;
                    }
                }
            }
        });

        Ok(peer)
    }

    pub(super) fn touch(&self, provider_id: &str) {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = entries.get(provider_id) {
            let next = entry.activity_tx.borrow().wrapping_add(1);
            let _ = entry.activity_tx.send(next);
        }
    }

    pub(super) async fn evict(&self, provider_id: &str) {
        let removed = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.remove(provider_id)
        };
        if let Some(entry) = removed {
            shutdown_entry(entry).await;
        }
    }

    pub(super) fn evict_in_background(self: &Arc<Self>, provider_id: String) {
        let removed = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.remove(&provider_id)
        };
        if let Some(entry) = removed {
            let _ = entry.stop_tx.send(true);
            entry.service.cancellation_token().cancel();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    shutdown_entry(entry).await;
                });
            } else {
                drop(entry);
            }
        }
    }

    pub(super) fn clear_diagnostics(&self, provider_id: &str) {
        self.diagnostics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(provider_id);
    }

    fn record_error(&self, provider_id: &str, message: &str, stderr: Option<Arc<StderrRing>>) {
        let mut diags = self.diagnostics.lock().unwrap_or_else(|e| e.into_inner());
        let entry = diags.entry(provider_id.to_owned()).or_default();
        entry.last_error = Some(message.to_owned());
        if let Some(ring) = stderr {
            entry.stderr = Some(ring);
        }
    }

    pub(super) fn runtime_status(&self, provider_id: &str) -> LocalMcpRuntimeStatus {
        let (running, pid) = {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            match entries.get(provider_id) {
                Some(entry) if !entry.service.is_closed() => (true, entry.pid),
                _ => (false, None),
            }
        };
        let last_error = self
            .diagnostics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(provider_id)
            .and_then(|d| d.last_error.clone());
        LocalMcpRuntimeStatus {
            running,
            pid,
            last_error,
        }
    }

    pub(super) fn logs(&self, provider_id: &str) -> LocalMcpLogs {
        let status = self.runtime_status(provider_id);
        let stderr = self
            .diagnostics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(provider_id)
            .and_then(|d| d.stderr.as_ref())
            .map(|ring| ring.snapshot())
            .unwrap_or_default();
        LocalMcpLogs {
            running: status.running,
            pid: status.pid,
            last_error: status.last_error,
            stderr,
        }
    }

    /// Synchronously cancel and drop every live child at application exit.
    /// Windows JobObject + KillOnDrop ensure immediate process-tree teardown.
    pub(super) fn stop_all_at_exit(&self) {
        let drained: Vec<WarmEntry> = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.drain().map(|(_, v)| v).collect()
        };
        for entry in drained {
            let _ = entry.stop_tx.send(true);
            entry.service.cancellation_token().cancel();
            drop(entry);
        }
    }
}

async fn shutdown_entry(mut entry: WarmEntry) {
    let _ = entry.stop_tx.send(true);
    let _ = entry.service.close_with_timeout(CLEANUP_TIMEOUT).await;
}

pub(crate) fn stop_at_exit() {
    if let Some(sup) = SUPERVISOR.get() {
        sup.stop_all_at_exit();
    }
}

fn local_client_config() -> ClientConfig {
    let mut config = ClientConfig::default();
    config.protocol_version = ProtocolVersion::V_2025_03_26;
    config.capabilities = ClientCapabilities::default();
    config.client_info = Implementation::new("Grain", env!("CARGO_PKG_VERSION"));
    config
}

/// Safely resolves a command name to an executable path without invoking a shell.
/// On Windows, checks `PATHEXT` (`.EXE`, `.CMD`, `.BAT`, `.COM`) so tools like
/// `npx` resolve to `npx.cmd`.
pub(super) fn resolve_executable(command: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(command);
    let has_separator = command.contains('/') || command.contains('\\');
    if has_separator || candidate.is_absolute() {
        if let Some(found) = check_file_with_pathext(candidate) {
            return Ok(found);
        }
        return Err(format!(
            "Executable `{command}` was not found. Check the path in Extensions settings."
        ));
    }

    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if let Some(found) = check_file_with_pathext(&dir.join(command)) {
                return Ok(found);
            }
        }
    }

    let lower = command.to_ascii_lowercase();
    let base = lower.strip_suffix(".exe").unwrap_or(&lower);
    let hint = match base {
        "npx" | "node" | "npm" => {
            "Node.js (`npx`/`node`) was not found on PATH. Install Node.js (nodejs.org) or use a standalone binary MCP."
        }
        "uvx" | "uv" => {
            "uv (`uvx`) was not found on PATH. Install uv (astral.sh/uv) or use a standalone binary MCP."
        }
        "python" | "python3" => "Python was not found on PATH.",
        _ => "",
    };
    if !hint.is_empty() {
        Err(hint.to_owned())
    } else {
        Err(format!(
            "Executable `{command}` was not found on PATH. Check the command in Extensions settings."
        ))
    }
}

fn check_file_with_pathext(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    #[cfg(windows)]
    {
        if path.extension().is_none() {
            let exts =
                std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned());
            for ext in exts.split(';').filter(|e| !e.is_empty()) {
                let mut os = path.as_os_str().to_os_string();
                os.push(ext);
                let with_ext = PathBuf::from(os);
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_ring_caps_lines_and_bytes_and_clips_long_lines() {
        let ring = StderrRing::default();
        for i in 0..100 {
            ring.push(&format!("line-{i}\r\n"));
        }
        let lines = ring.snapshot();
        assert_eq!(lines.len(), MAX_STDERR_LINES);
        assert_eq!(lines.first().map(String::as_str), Some("line-36"));
        assert_eq!(lines.last().map(String::as_str), Some("line-99"));

        let huge_ring = StderrRing::default();
        for _ in 0..50 {
            huge_ring.push(&"x".repeat(800));
        }
        let total: usize = huge_ring.snapshot().iter().map(String::len).sum();
        assert!(total <= MAX_STDERR_BYTES);
        assert!(huge_ring
            .snapshot()
            .iter()
            .all(|line| line.len() <= MAX_STDERR_LINE_BYTES + 4));
    }

    #[test]
    fn resolve_executable_gives_helpful_hints_for_missing_binaries() {
        let missing = resolve_executable("grain_definitely_missing_bin_12345").unwrap_err();
        assert!(missing.contains("was not found on PATH"));

        #[cfg(windows)]
        {
            let found = resolve_executable("cmd").unwrap();
            assert!(found.is_file());
        }
    }

    #[tokio::test]
    async fn supervisor_records_error_and_stays_clean_when_binary_missing() {
        let sup = Arc::new(Supervisor::new(Duration::from_millis(200)));
        let spec = StdioLaunchSpec {
            command: "grain_definitely_missing_bin_12345".into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        assert!(sup
            .acquire("configured-missing", "b1", spec, deadline)
            .await
            .is_err());
        let status = sup.runtime_status("configured-missing");
        assert!(!status.running);
        assert!(status.pid.is_none());
        assert!(status.last_error.is_some());
    }

    #[tokio::test]
    async fn supervisor_spawns_reuses_warm_stdio_server_and_reaps_after_idle() {
        // Use the workspace's own compiled `grain-recall-mcp` binary when present.
        let exe_name = if cfg!(windows) {
            "grain-recall-mcp.exe"
        } else {
            "grain-recall-mcp"
        };
        let target_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent()?.parent().map(Path::to_path_buf));
        let Some(server_exe) = target_dir.map(|d| d.join(exe_name)).filter(|p| p.is_file()) else {
            // Skip gracefully if grain-recall-mcp hasn't been built yet in this profile.
            return;
        };

        let dir = tempfile::tempdir().unwrap();
        // Enable the Recall MCP gate file inside our disposable tempdir.
        std::fs::write(dir.path().join("recall-mcp-enabled"), b"1").unwrap();

        let sup = Arc::new(Supervisor::new(Duration::from_millis(300)));
        let spec = StdioLaunchSpec {
            command: server_exe.to_string_lossy().into_owned(),
            args: vec![
                "--data-dir".into(),
                dir.path().to_string_lossy().into_owned(),
            ],
            cwd: None,
            env: BTreeMap::new(),
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

        // 1. Cold spawn + MCP initialize + live tool discovery.
        let peer1 = sup
            .acquire("configured-recall-test", "bind-1", spec.clone(), deadline)
            .await
            .expect("spawn stdio MCP");
        let status1 = sup.runtime_status("configured-recall-test");
        assert!(status1.running);
        let pid1 = status1.pid.expect("child pid");
        let tools = peer1.list_tools(None).await.expect("list_tools over stdio");
        assert!(!tools.tools.is_empty());

        // 2. Second acquire reuses the exact warm process (same PID).
        let _peer2 = sup
            .acquire("configured-recall-test", "bind-1", spec.clone(), deadline)
            .await
            .expect("reuse warm stdio MCP");
        assert_eq!(sup.runtime_status("configured-recall-test").pid, Some(pid1));

        // 3. Idle reaper shuts down the child cleanly after 300ms idle.
        tokio::time::sleep(Duration::from_millis(700)).await;
        let after_idle = sup.runtime_status("configured-recall-test");
        assert!(
            !after_idle.running,
            "idle reaper should have stopped the child"
        );
        assert!(after_idle.pid.is_none());

        // 4. Re-acquire spawns a fresh child, and explicit evict kills it immediately.
        let _peer3 = sup
            .acquire("configured-recall-test", "bind-1", spec, deadline)
            .await
            .expect("respawn after idle");
        assert!(sup.runtime_status("configured-recall-test").running);
        sup.evict("configured-recall-test").await;
        assert!(!sup.runtime_status("configured-recall-test").running);
    }
}
