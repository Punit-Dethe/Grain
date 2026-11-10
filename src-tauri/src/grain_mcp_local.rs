//! Local MCP servers: installed once, started on the first tool call, shared by
//! the calls that follow, stopped when idle. See docs/MCP Hosting/PLAN.md.
//!
//! Lifecycle rules:
//! - Nothing runs until a call (or discovery) needs the server.
//! - One process per server, shared by concurrent calls.
//! - Idle servers stop after [`IDLE_STOP`]; at most [`MAX_RUNNING`] run at once.
//! - Servers talk over stdin/stdout only and die with Grain (Windows Job Object,
//!   Linux parent-death signal, stdin EOF elsewhere).
//! - No background task exists while no server runs.
use super::{before_dispatch, client_info, McpService, SdkService, ServiceRef, CLEANUP_TIMEOUT};
use grain_core::execution::{ExecutionFailure, FailureClass};
use rmcp::model::ProtocolVersion;
use rmcp::{ClientLifecycleMode, ClientServiceExt};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncBufReadExt;

const IDLE_STOP: Duration = Duration::from_secs(10 * 60);
const REAP_EVERY: Duration = Duration::from_secs(30);
const MAX_RUNNING: usize = 3;
const START_TIMEOUT: Duration = Duration::from_secs(20);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const STOP_GRACE: Duration = Duration::from_secs(2);
const STDERR_LINE_MAX: usize = 300;
const STDERR_LINES_PER_MINUTE: u32 = 30;

/// How a local server is packaged. Versions are exact pins, never ranges.
pub(super) enum Package {
    /// A PyPI package installed with `uv` into its own virtual environment.
    Pypi {
        name: &'static str,
        version: &'static str,
        python: &'static str,
        /// Console script the package installs (launched directly, no `uvx`).
        command: &'static str,
    },
}

pub(super) struct Spec {
    pub id: &'static str,
    pub package: Package,
    /// Fixed by Grain. Never taken from the model, the web or a page.
    pub args: &'static [&'static str],
    /// Signs in with Grain's Google flow; gets a session file while running.
    pub google_account: bool,
    /// Python run before the package's entry point (`python -I -c`), for
    /// switching off behaviour Grain must own. `None` runs the console script.
    pub python_prelude: Option<&'static str>,
}

/// workspace-mcp opens a browser for its own sign-in when a grant fails.
/// Grain owns sign-in, so the server may never open one; it reports the
/// failure instead and the user reconnects in Grain.
const NO_BROWSER_WORKSPACE_MCP: &str = "import sys, webbrowser
webbrowser.open = webbrowser.open_new = webbrowser.open_new_tab = lambda *a, **k: False
from main import main
sys.exit(main())";

pub(super) const SPECS: &[Spec] = &[
    Spec {
        id: "dev-local-time",
        package: Package::Pypi {
            name: "mcp-server-time",
            version: "2026.10.19",
            python: "3.12",
            command: "mcp-server-time",
        },
        args: &[],
        google_account: false,
        python_prelude: None,
    },
    Spec {
        id: super::google::PROVIDER_ID,
        package: Package::Pypi {
            name: "workspace-mcp",
            version: "2.2.1",
            python: "3.12",
            command: "workspace-mcp",
        },
        // The services here must match the scopes in grain_mcp_google.rs.
        args: &[
            "--transport",
            "stdio",
            "--single-user",
            "--tool-tier",
            "core",
            "--tools",
            "gmail",
            "calendar",
            "drive",
            "docs",
            "sheets",
        ],
        google_account: true,
        python_prelude: Some(NO_BROWSER_WORKSPACE_MCP),
    },
];

pub(super) fn spec(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.id == id)
}

// ---------------------------------------------------------------------------
// Paths and runtimes

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.try_state::<Arc<grain_core::AppContext>>()
        .map(|context| context.data_dir.clone())
        .ok_or_else(|| "application context unavailable".into())
}

fn runtime_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("mcp-runtimes"))
}

fn server_root(app: &AppHandle, spec: &Spec) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("mcp-servers").join(spec.id))
}

/// Per-run files (the Google credential file). Removed when the server stops.
fn session_dir(app: &AppHandle, spec: &Spec) -> Result<PathBuf, String> {
    Ok(server_root(app, spec)?.join("session"))
}

/// Delete a server's per-run files (sign-out, removal).
pub(super) async fn clear_session(app: &AppHandle, spec: &Spec) {
    if let Ok(dir) = session_dir(app, spec) {
        let _ = tokio::fs::remove_dir_all(dir).await;
    }
}

fn version(spec: &Spec) -> &'static str {
    match spec.package {
        Package::Pypi { version, .. } => version,
    }
}

fn venv_bin(venv: &Path, name: &str) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join(format!("{name}.exe"))
    } else {
        venv.join("bin").join(name)
    }
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// `uv` shipped in Grain's runtime folder first, then the user's install.
fn find_uv(app: &AppHandle) -> Option<PathBuf> {
    let name = exe_name("uv");
    let mut candidates = Vec::new();
    if let Ok(runtime) = runtime_dir(app) {
        candidates.push(runtime.join("uv").join(&name));
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(&name)));
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if let Some(home) = home.map(PathBuf::from) {
        candidates.push(home.join(".local").join("bin").join(&name));
        candidates.push(home.join(".cargo").join("bin").join(&name));
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// A command with a minimal environment: never every variable Grain holds.
fn command(program: &Path) -> tokio::process::Command {
    const PASS: &[&str] = &[
        "PATH",
        "SYSTEMROOT",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USERPROFILE",
        "HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "LANG",
        "LC_ALL",
        "TZ",
        // Installs and servers that reach the network must honour the
        // machine's proxy and certificate settings.
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "no_proxy",
        "all_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ];
    let mut command = tokio::process::Command::new(program);
    command.env_clear();
    for name in PASS {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    command
}

// ---------------------------------------------------------------------------
// Install

/// Per-server lock shared by install and start, so neither races the other.
fn slot_lock(id: &'static str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: Mutex<BTreeMap<&'static str, Arc<tokio::sync::Mutex<()>>>> =
        Mutex::new(BTreeMap::new());
    LOCKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(id)
        .or_default()
        .clone()
}

/// Install the pinned package if needed. Runs at enable time, never mid-call
/// unless the install is missing.
pub(super) async fn ensure_installed(app: &AppHandle, spec: &'static Spec) -> Result<(), String> {
    let lock = slot_lock(spec.id);
    let _guard = lock.lock().await;
    tokio::time::timeout(INSTALL_TIMEOUT, install_locked(app, spec))
        .await
        .map_err(|_| "Installing this extension took too long. Try again.".to_string())?
        .map(|_| ())
}

async fn install_locked(app: &AppHandle, spec: &'static Spec) -> Result<PathBuf, String> {
    let root = server_root(app, spec)?;
    let dir = root.join(version(spec));
    let marker = dir.join("installed");
    let Package::Pypi {
        name,
        version,
        python,
        command: entry,
    } = spec.package;
    let venv = dir.join("venv");
    let exe = venv_bin(&venv, entry);
    if marker.is_file() && exe.is_file() {
        return Ok(exe);
    }
    let uv = find_uv(app).ok_or(
        "This extension needs uv. Install it from https://docs.astral.sh/uv/ and try again.",
    )?;
    let started = Instant::now();
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| format!("Could not create the install folder: {error}"))?;
    let runtime = runtime_dir(app)?;
    let run = |args: Vec<std::ffi::OsString>| {
        let mut command = command(&uv);
        command
            .args(args)
            .env("UV_PYTHON_INSTALL_DIR", runtime.join("python"))
            .env("UV_CACHE_DIR", runtime.join("uv-cache"))
            .env("UV_NO_PROGRESS", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        async move {
            let output = command
                .output()
                .await
                .map_err(|error| format!("Could not run uv: {error}"))?;
            if output.status.success() {
                return Ok(());
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: String = stderr
                .chars()
                .rev()
                .take(600)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            log::warn!("[GRAIN] mcp-host install uv failed: {tail}");
            Err("Installing this extension failed. See the Grain log for details.".to_string())
        }
    };
    run(vec![
        "venv".into(),
        venv.clone().into(),
        "--python".into(),
        python.into(),
        "--no-config".into(),
    ])
    .await?;
    let venv_python = venv_bin(&venv, "python");
    run(vec![
        "pip".into(),
        "install".into(),
        "--python".into(),
        venv_python.into(),
        "--compile-bytecode".into(),
        "--no-config".into(),
        format!("{name}=={version}").into(),
    ])
    .await?;
    if !exe.is_file() {
        return Err("The installed extension is missing its program.".into());
    }
    tokio::fs::write(&marker, version)
        .await
        .map_err(|error| format!("Could not finish the install: {error}"))?;
    remove_other_versions(&root, version).await;
    log::info!(
        "[GRAIN] mcp-host {} installed {name}=={version} in {}ms",
        spec.id,
        started.elapsed().as_millis()
    );
    Ok(exe)
}

async fn remove_other_versions(root: &Path, keep: &str) {
    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry.file_name() != keep && entry.file_name() != "session" {
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Running servers

struct Running {
    service: Arc<SdkService>,
    child: tokio::process::Child,
    last_used: Instant,
    in_use: usize,
    session: Option<PathBuf>,
}

struct Host {
    running: BTreeMap<&'static str, Running>,
    reaper: bool,
}

fn host() -> std::sync::MutexGuard<'static, Host> {
    static HOST: Mutex<Host> = Mutex::new(Host {
        running: BTreeMap::new(),
        reaper: false,
    });
    HOST.lock().unwrap_or_else(|e| e.into_inner())
}

/// Borrow a running server, starting it if needed. The returned service is
/// released (not closed) when the caller drops it.
pub(super) async fn acquire(
    app: &AppHandle,
    spec: &'static Spec,
    deadline: tokio::time::Instant,
) -> Result<McpService, ExecutionFailure> {
    if let Some(service) = reuse(spec.id) {
        return Ok(shared(service, spec.id));
    }
    let lock = slot_lock(spec.id);
    let _guard = tokio::time::timeout_at(deadline, lock.lock())
        .await
        .map_err(|_| before_dispatch(FailureClass::Network, "Local MCP server is busy."))?;
    if let Some(service) = reuse(spec.id) {
        return Ok(shared(service, spec.id));
    }
    let installed = tokio::time::timeout_at(deadline, install_locked(app, spec))
        .await
        .map_err(|_| {
            before_dispatch(
                FailureClass::Network,
                "This extension is still installing. Try again in a moment.",
            )
        })?
        .map_err(|message| before_dispatch(FailureClass::Internal, message))?;
    make_room().await;
    let (session, env) = if spec.google_account {
        let dir = session_dir(app, spec)
            .map_err(|message| before_dispatch(FailureClass::Internal, message))?;
        let env = super::google::write_session(&dir)
            .await
            .map_err(|message| before_dispatch(FailureClass::Auth, message))?;
        (Some(dir), env)
    } else {
        (None, Vec::new())
    };
    let started = Instant::now();
    let spawned = spawn(spec, &installed, &env);
    let mut child = match spawned {
        Ok(child) => child,
        Err(message) => {
            remove_session(session.as_deref()).await;
            return Err(before_dispatch(FailureClass::Internal, message));
        }
    };
    let spawn_ms = started.elapsed().as_millis();
    let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take()) else {
        let _ = child.kill().await;
        return Err(before_dispatch(
            FailureClass::Internal,
            "Local MCP server pipes are unavailable.",
        ));
    };
    if let Some(stderr) = child.stderr.take() {
        forward_stderr(spec.id, stderr);
    }
    let start_deadline = deadline.min(tokio::time::Instant::now() + START_TIMEOUT);
    let info = client_info().with_protocol_version(ProtocolVersion::V_2025_11_25);
    let served = tokio::time::timeout_at(
        start_deadline,
        info.serve_with_lifecycle((stdout, stdin), ClientLifecycleMode::Initialize),
    )
    .await;
    let service = match served {
        Ok(Ok(service)) => Arc::new(service),
        _ => {
            let _ = child.kill().await;
            remove_session(session.as_deref()).await;
            log::warn!("[GRAIN] mcp-host {} failed to start", spec.id);
            return Err(before_dispatch(
                FailureClass::Network,
                "The local MCP server did not start. See the Grain log for details.",
            ));
        }
    };
    log::info!(
        "[GRAIN] mcp-host {} start spawn_ms={spawn_ms} init_ms={}",
        spec.id,
        started.elapsed().as_millis() - spawn_ms
    );
    let mut host = host();
    host.running.insert(
        spec.id,
        Running {
            service: service.clone(),
            child,
            last_used: Instant::now(),
            in_use: 1,
            session,
        },
    );
    if !host.reaper {
        host.reaper = true;
        tauri::async_runtime::spawn(reap());
    }
    drop(host);
    Ok(shared(service, spec.id))
}

fn shared(service: Arc<SdkService>, id: &'static str) -> McpService {
    McpService {
        inner: ServiceRef::Shared { service, id },
        cancel: tokio::sync::watch::channel(false).0,
        dispatched: std::sync::atomic::AtomicBool::new(false),
    }
}

/// A live process for `id`, marked in use. A process that exited is dropped.
fn reuse(id: &'static str) -> Option<Arc<SdkService>> {
    let mut host = host();
    let running = host.running.get_mut(id)?;
    let exited = !matches!(running.child.try_wait(), Ok(None));
    if exited || running.service.is_transport_closed() {
        log::warn!("[GRAIN] mcp-host {id} exited unexpectedly; restarting on demand");
        host.running.remove(id);
        return None;
    }
    running.in_use += 1;
    running.last_used = Instant::now();
    Some(running.service.clone())
}

/// Called when a borrowed service is dropped.
pub(super) fn release(id: &'static str) {
    if let Some(running) = host().running.get_mut(id) {
        running.in_use = running.in_use.saturating_sub(1);
        running.last_used = Instant::now();
    }
}

/// Stop a server now (disable, disconnect, removal).
pub(super) async fn stop(id: &str) {
    let running = host().running.remove(id);
    if let Some(running) = running {
        shutdown(id, running, "stopped").await;
    }
}

/// Keep at most MAX_RUNNING - 1 before a start: stop the least recently used
/// idle server. Busy servers are never stopped to make room.
async fn make_room() {
    let victim = {
        let mut host = host();
        if host.running.len() < MAX_RUNNING {
            return;
        }
        let id = host
            .running
            .iter()
            .filter(|(_, running)| running.in_use == 0)
            .min_by_key(|(_, running)| running.last_used)
            .map(|(id, _)| *id);
        id.and_then(|id| host.running.remove(id).map(|running| (id, running)))
    };
    if let Some((id, running)) = victim {
        shutdown(id, running, "room").await;
    }
}

async fn reap() {
    loop {
        tokio::time::sleep(REAP_EVERY).await;
        let (idle, finished): (Vec<_>, bool) = {
            let mut host = host();
            let ids: Vec<_> = host
                .running
                .iter()
                .filter(|(_, running)| {
                    running.in_use == 0 && running.last_used.elapsed() >= IDLE_STOP
                })
                .map(|(id, _)| *id)
                .collect();
            let idle = ids
                .into_iter()
                .filter_map(|id| host.running.remove(id).map(|running| (id, running)))
                .collect();
            // The reaper exits with the last server; a later start spawns a new one.
            let finished = host.running.is_empty();
            if finished {
                host.reaper = false;
            }
            (idle, finished)
        };
        for (id, running) in idle {
            shutdown(id, running, "idle").await;
        }
        if finished {
            return;
        }
    }
}

/// MCP spec order: close stdin (by closing the session), wait, then kill.
async fn shutdown(id: &str, mut running: Running, reason: &str) {
    let uptime = running.last_used.elapsed().as_secs();
    if let Ok(mut service) = Arc::try_unwrap(running.service) {
        let _ = service.close_with_timeout(CLEANUP_TIMEOUT).await;
    }
    if tokio::time::timeout(STOP_GRACE, running.child.wait())
        .await
        .is_err()
    {
        let _ = running.child.kill().await;
    }
    remove_session(running.session.as_deref()).await;
    log::info!("[GRAIN] mcp-host {id} stop reason={reason} idle_s={uptime}");
}

async fn remove_session(dir: Option<&Path>) {
    if let Some(dir) = dir {
        let _ = tokio::fs::remove_dir_all(dir).await;
    }
}

fn spawn(
    spec: &Spec,
    program: &Path,
    env: &[(&'static str, String)],
) -> Result<tokio::process::Child, String> {
    let mut command = match spec.python_prelude {
        Some(prelude) => {
            // The console script and the venv's python share a folder.
            let python = program.with_file_name(exe_name("python"));
            let mut command = command(&python);
            command.args(["-I", "-c", prelude]);
            command
        }
        None => command(program),
    };
    command
        .args(spec.args)
        .envs(env.iter().map(|(name, value)| (*name, value)))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(target_os = "linux")]
    unsafe {
        // The server dies if Grain dies, even without a clean shutdown.
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    // Start suspended so the process joins Grain's Job Object before it can
    // start children of its own (venv launchers start python.exe at once).
    #[cfg(windows)]
    command.creation_flags(0x0800_0000 | 0x0000_0004); // CREATE_NO_WINDOW | CREATE_SUSPENDED
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start the local MCP server: {error}"))?;
    #[cfg(windows)]
    {
        kill_with_grain(&child);
        if !resume(&child) {
            let _ = child.start_kill();
            return Err("Could not start the local MCP server.".into());
        }
    }
    Ok(child)
}

/// Resume every thread of a process started with CREATE_SUSPENDED.
#[cfg(windows)]
fn resume(child: &tokio::process::Child) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};
    let Some(pid) = child.id() else {
        return false;
    };
    let mut resumed = false;
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else {
            return false;
        };
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut more = Thread32First(snapshot, &mut entry).is_ok();
        while more {
            if entry.th32OwnerProcessID == pid {
                if let Ok(thread) = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) {
                    resumed |= ResumeThread(thread) != u32::MAX;
                    let _ = CloseHandle(thread);
                }
            }
            more = Thread32Next(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    resumed
}

/// Every local server joins one Job Object that kills its members when Grain's
/// handle closes, including when Grain crashes. Children a server starts after
/// joining are included automatically.
#[cfg(windows)]
fn kill_with_grain(child: &tokio::process::Child) {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    static JOB: OnceLock<Option<usize>> = OnceLock::new();
    let job = JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).ok()?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&info).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .ok()?;
        // Intentionally never closed: the handle must live as long as Grain.
        Some(job.0 as usize)
    });
    if let (Some(job), Some(process)) = (job, child.raw_handle()) {
        unsafe {
            if AssignProcessToJobObject(HANDLE(*job as *mut _), HANDLE(process)).is_err() {
                log::warn!("[GRAIN] mcp-host could not tie a server to Grain's lifetime");
            }
        }
    }
}

/// Server logs go to Grain's log, truncated and rate limited.
fn forward_stderr(id: &'static str, stderr: tokio::process::ChildStderr) {
    tauri::async_runtime::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        let mut window = Instant::now();
        let mut count = 0u32;
        while let Ok(Some(line)) = lines.next_line().await {
            if window.elapsed() >= Duration::from_secs(60) {
                window = Instant::now();
                count = 0;
            }
            count += 1;
            if count <= STDERR_LINES_PER_MINUTE {
                let line: String = line.chars().take(STDERR_LINE_MAX).collect();
                log::info!("[GRAIN] mcp-host {id} stderr: {line}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_are_unique_and_exactly_pinned() {
        let mut ids = std::collections::HashSet::new();
        for spec in SPECS {
            assert!(ids.insert(spec.id));
            let Package::Pypi {
                version, python, ..
            } = spec.package;
            assert!(!version.is_empty());
            assert!(version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.'));
            assert!(!python.is_empty());
            assert!(super::super::provider(spec.id).is_ok());
        }
    }

    #[test]
    fn venv_programs_live_inside_the_venv() {
        let venv = Path::new("venv");
        let program = venv_bin(venv, "tool");
        assert!(program.starts_with(venv));
        assert_eq!(program.file_name().unwrap(), exe_name("tool").as_str());
    }
}
