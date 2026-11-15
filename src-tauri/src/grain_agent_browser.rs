//! [GRAIN] Agent Browser: the desktop half of `grain_core::agent_browser`
//! (`docs/Agent Browser/PLAN.md`).
//!
//! The Agent's web browser is Vercel Labs' `agent-browser` CLI, installed by
//! the user and maintained upstream. Grain finds the native binary, checks its
//! version once, and runs one short-lived CLI process per Agent call with
//! Grain's own session name. The core module decides what a call means and
//! builds its argv; this module only spawns the process and reads its reply.
//!
//! Lifetime (nothing outlives use):
//! - Grain holds no thread, socket or child between calls. Each call is one
//!   CLI process that exits when the command is done.
//! - `agent-browser` keeps its own daemon and browser between calls. Grain sets
//!   its idle timeout ([`grain_core::agent_browser::IDLE_TIMEOUT_MS`]), and
//!   closes the session when the extension is switched off and when Grain
//!   exits, so the browser never lingers.
//!
//! Process boundary: the binary is run directly (never through a shell or the
//! npm `.cmd` shim), every argument is a separate argv entry, and no console
//! window opens. The first command starts the daemon, which inherits the CLI's
//! stdout; a pipe would then never reach end-of-file, so stdout goes to a
//! per-call file and Grain waits on the CLI's exit, not on the pipe.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use grain_core::agent_browser::{self as core, Call, ReplyError};
use grain_core::capability_index::ExtensionDirectoryEntry;
use grain_core::execution::{ActionOutcome, FailureClass, SuccessData};
use tauri::AppHandle;

pub(crate) use grain_core::agent_browser::EXTENSION_ID;

/// Most stdout Grain reads from one call; the reply is clipped far below this.
const MAX_STDOUT_BYTES: u64 = 8 * 1024 * 1024;
/// `--version` and the close at switch-off.
const SHORT_TIMEOUT: Duration = Duration::from_secs(15);
/// Closing at app exit must not hold up quitting.
const EXIT_TIMEOUT: Duration = Duration::from_secs(4);

/// The verified binary: a path and version, never a handle. Cleared when the
/// file disappears, so an uninstall or upgrade is picked up on the next call.
static INSTALL: Mutex<Option<Install>> = Mutex::new(None);
/// Set once this run has sent the browser a command; tells switch-off and
/// exit whether there is a browser to close.
static USED: AtomicBool = AtomicBool::new(false);
/// One command at a time on Grain's session. An Agent frame already runs its
/// calls in order; this also orders calls from overlapping turns.
static GATE: Mutex<()> = Mutex::new(());
static OUTPUT_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
struct Install {
    path: PathBuf,
    version: (u32, u32, u32),
}

pub(crate) fn is_enabled(app: &AppHandle) -> bool {
    crate::settings::get_settings(app).agent_browser_enabled
}

/// [GRAIN] Turn the Agent Browser on or off. Off removes its tools from the
/// Agent on the next model round and closes its browser.
#[tauri::command]
#[specta::specta]
pub fn change_agent_browser_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.agent_browser_enabled = enabled;
    crate::settings::write_settings(&app, settings);
    if !enabled {
        close_in_background();
    }
    Ok(())
}

/// Close Grain's browser session off the caller's thread (switch-off,
/// disconnect). A no-op when this run never used the browser.
pub(crate) fn close_in_background() {
    if USED.load(Ordering::SeqCst) {
        tauri::async_runtime::spawn_blocking(|| close_session(SHORT_TIMEOUT));
    }
}

/// Close Grain's browser session at app exit, bounded so quitting never waits
/// long. The daemon's idle timeout covers a kill that skips this.
pub(crate) fn close_at_exit() {
    // A call in flight holds the gate; never wait on it while quitting.
    if let Ok(gate) = GATE.try_lock() {
        close_locked(EXIT_TIMEOUT, gate);
    }
}

fn close_session(timeout: Duration) {
    close_locked(timeout, GATE.lock().unwrap_or_else(|e| e.into_inner()));
}

fn close_locked(timeout: Duration, _gate: std::sync::MutexGuard<'_, ()>) {
    if !USED.swap(false, Ordering::SeqCst) {
        return;
    }
    let Some(install) = cached_install() else {
        return;
    };
    let mut args: Vec<&str> = core::global_args().to_vec();
    args.push("close");
    match run_captured(&install.path, &args, timeout) {
        Ok(_) => log::info!("[GRAIN] agent-browser: closed the session"),
        Err(error) => log::warn!("[GRAIN] agent-browser: close failed: {error}"),
    }
}

/// Find and verify `agent-browser`, for the Extensions page's Connect.
/// Returns the version on success, or a message that says how to install or
/// update it.
pub(crate) async fn check_installed() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        resolve().map(|install| core::format_version(install.version))
    })
    .await
    .map_err(|error| format!("Could not check agent-browser: {error}"))?
}

/// The Agent directory entry, when the user has turned it on.
pub(crate) fn directory(app: &AppHandle) -> Option<ExtensionDirectoryEntry> {
    is_enabled(app).then(|| ExtensionDirectoryEntry {
        extension_id: EXTENSION_ID.to_string(),
        name: core::PROVIDER_NAME.to_string(),
        description: core::DESCRIPTION.to_string(),
        action_count: core::tools().len(),
    })
}

/// The compiled tool set and its digest. No I/O.
pub(crate) fn catalog() -> (String, Vec<crate::grain_compiled_tools::CatalogTool>) {
    let tools = core::tools()
        .into_iter()
        .map(|tool| {
            crate::grain_compiled_tools::catalog_tool(
                EXTENSION_ID,
                core::PROVIDER_NAME,
                core::DESCRIPTION,
                crate::grain_compiled_tools::ToolDef {
                    name: tool.name,
                    title: tool.title,
                    description: tool.description,
                    keywords: tool.keywords,
                    read_only: tool.read_only,
                    schema: tool.schema,
                },
            )
        })
        .collect();
    (core::catalog_digest(), tools)
}

/// Run one Agent Browser action. Never panics into the Agent loop: every
/// failure is a typed outcome the model can read and correct.
pub(crate) async fn call(
    app: &AppHandle,
    action_id: &str,
    arguments: &serde_json::Value,
) -> ActionOutcome {
    if !is_enabled(app) {
        return failed(FailureClass::Cancelled, "The Agent Browser is turned off.");
    }
    let parsed = match core::parse(action_id, arguments) {
        Ok(call) => call,
        Err(message) => return failed(FailureClass::InvalidArgument, &message),
    };
    let title = core::tools()
        .into_iter()
        .find(|tool| tool.name == action_id)
        .map(|tool| tool.title.to_string());
    // Arguments can be private text; log the shape only.
    log::info!("[GRAIN] agent-browser: {}", parsed.describe_for_log());
    let read_only = parsed.read_only();
    let task = parsed.clone();
    let result = tauri::async_runtime::spawn_blocking(move || run_call(&task))
        .await
        .unwrap_or_else(|error| {
            Err(Failure::internal(format!(
                "The browser call stopped: {error}"
            )))
        });
    match result {
        Ok(body) => ActionOutcome::Succeeded(SuccessData {
            source: Some(core::PROVIDER_NAME.to_string()),
            title,
            body: Some(body),
            structured_content: None,
            details: Vec::new(),
            receipt: !read_only,
        }),
        Err(failure) => {
            log::info!(
                "[GRAIN] agent-browser: {action_id} failed: {}",
                failure.message
            );
            failed(failure.class, &failure.message)
        }
    }
}

struct Failure {
    class: FailureClass,
    message: String,
}

impl Failure {
    fn new(class: FailureClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new(FailureClass::Internal, message)
    }
}

fn failed(class: FailureClass, message: &str) -> ActionOutcome {
    ActionOutcome::Failed {
        class,
        message: message.to_string(),
    }
}

/// Blocking: one CLI process for one call.
fn run_call(call: &Call) -> Result<String, Failure> {
    let _gate = GATE.lock().unwrap_or_else(|e| e.into_inner());
    let install = resolve().map_err(|message| Failure::new(FailureClass::NotFound, message))?;
    let command = call.argv();
    let mut args: Vec<&str> = core::global_args().to_vec();
    args.extend(command.iter().map(String::as_str));
    // From here a browser may exist, even if this call fails or times out.
    USED.store(true, Ordering::SeqCst);
    let (status, stdout) = run_captured(
        &install.path,
        &args,
        Duration::from_secs(call.timeout_secs()),
    )
    .map_err(|message| Failure::new(FailureClass::Internal, message))?;
    match core::render(call, &stdout) {
        Ok(body) => {
            if matches!(call, Call::Close) {
                USED.store(false, Ordering::SeqCst);
            }
            Ok(body)
        }
        Err(ReplyError::Command(message)) => Err(Failure::new(classify(&message), message)),
        Err(ReplyError::Malformed) => Err(Failure::internal(format!(
            "agent-browser gave no reply Grain could read (exit {}). Run `agent-browser doctor`, and make sure it is version {} or newer.",
            status
                .code()
                .map_or_else(|| "unknown".to_string(), |code| code.to_string()),
            core::format_version(core::MIN_VERSION)
        ))),
    }
}

fn classify(message: &str) -> FailureClass {
    let lower = message.to_ascii_lowercase();
    if lower.contains("ref")
        && (lower.contains("unknown") || lower.contains("not found") || lower.contains("stale"))
    {
        FailureClass::NotFound
    } else if lower.contains("net::") || lower.contains("dns") || lower.contains("connection") {
        FailureClass::Network
    } else {
        FailureClass::Internal
    }
}

// ── Install discovery ──────────────────────────────────────────────────────

fn cached_install() -> Option<Install> {
    let mut cached = INSTALL.lock().unwrap_or_else(|e| e.into_inner());
    if cached
        .as_ref()
        .is_some_and(|install| !install.path.is_file())
    {
        *cached = None;
    }
    cached.clone()
}

/// The verified binary, from the cache or a fresh search.
fn resolve() -> Result<Install, String> {
    if let Some(install) = cached_install() {
        return Ok(install);
    }
    let dirs = search_dirs();
    // What the user's own `agent-browser` command runs comes first.
    let mut candidates = shim_candidates(&dirs);
    candidates.extend(core::binary_candidates(&dirs, cfg!(windows)));
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|path| seen.insert(path.clone()));
    let mut too_old: Option<(u32, u32, u32)> = None;
    let mut broken: Option<PathBuf> = None;
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let Some(version) = probe_version(&candidate) else {
            broken.get_or_insert(candidate);
            continue;
        };
        if !core::version_supported(version) {
            too_old.get_or_insert(version);
            continue;
        }
        log::info!(
            "[GRAIN] agent-browser: using {} ({})",
            core::format_version(version),
            candidate.display()
        );
        let install = Install {
            path: candidate,
            version,
        };
        *INSTALL.lock().unwrap_or_else(|e| e.into_inner()) = Some(install.clone());
        return Ok(install);
    }
    Err(match (too_old, broken) {
        (Some(version), _) => format!(
            "agent-browser {} is installed, but Grain needs {} or newer. Update it with `agent-browser upgrade` or `npm install -g agent-browser@latest`.",
            core::format_version(version),
            core::format_version(core::MIN_VERSION)
        ),
        (None, Some(path)) => format!(
            "agent-browser was found at {} but did not report its version. Reinstall it with `npm install -g agent-browser@latest`, then run `agent-browser install`.",
            path.display()
        ),
        (None, None) => format!("agent-browser is not installed. {}", core::INSTALL_HINT),
    })
}

fn probe_version(path: &Path) -> Option<(u32, u32, u32)> {
    let (_, stdout) = run_captured(path, &["--version"], SHORT_TIMEOUT).ok()?;
    core::parse_version(&stdout)
}

/// Native binaries named by npm `.cmd` shims on the search path (Windows).
fn shim_candidates(dirs: &[PathBuf]) -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    dirs.iter()
        .filter_map(|dir| {
            let shim = dir.join("agent-browser.cmd");
            // A shim is a few hundred bytes; never read more than a little.
            let mut text = String::new();
            std::fs::File::open(&shim)
                .ok()?
                .take(4096)
                .read_to_string(&mut text)
                .ok()?;
            core::shim_target(&text).map(|target| dir.join(target))
        })
        .collect()
}

/// `PATH`, then the install directories a GUI app's `PATH` often lacks.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        // npm's default global prefix.
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    #[cfg(not(windows))]
    {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            dirs.push(home.join(".npm-global").join("bin"));
            dirs.push(home.join(".local").join("bin"));
        }
    }
    dirs.retain(|dir| dir.is_absolute());
    dirs
}

// ── Process ────────────────────────────────────────────────────────────────

/// Run `path args…` and return its exit status and stdout. Waits on the
/// process (bounded by `timeout`, then killed), never on the stdout pipe: the
/// daemon the first command starts inherits stdout and keeps it open.
fn run_captured(
    path: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<(ExitStatus, String), String> {
    let output_path = std::env::temp_dir().join(format!(
        "grain-agent-browser-{}-{}.json",
        std::process::id(),
        OUTPUT_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let output = OutputFile::create(output_path)?;
    let stdout = output
        .file
        .try_clone()
        .map_err(|error| format!("Could not prepare the browser call: {error}"))?;
    let mut command = Command::new(path);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::null())
        .env(
            "AGENT_BROWSER_IDLE_TIMEOUT_MS",
            core::IDLE_TIMEOUT_MS.to_string(),
        )
        // No update check against the npm registry from Grain's calls.
        .env("AGENT_BROWSER_NO_VERSION_CHECK", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start agent-browser: {error}"))?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "agent-browser did not answer within {} seconds; the action may or may not have happened. Take a snapshot to check.",
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(error) => return Err(format!("Lost track of agent-browser: {error}")),
        }
    };
    Ok((status, output.read()?))
}

/// A per-call stdout file, removed when dropped.
struct OutputFile {
    path: PathBuf,
    file: std::fs::File,
}

impl OutputFile {
    fn create(path: PathBuf) -> Result<Self, String> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("Could not prepare the browser call: {error}"))?;
        Ok(Self { path, file })
    }

    fn read(&self) -> Result<String, String> {
        let mut bytes = Vec::new();
        std::fs::File::open(&self.path)
            .and_then(|file| file.take(MAX_STDOUT_BYTES).read_to_end(&mut bytes))
            .map_err(|error| format!("Could not read agent-browser's reply: {error}"))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Drop for OutputFile {
    fn drop(&mut self) {
        // Rust opens files with delete sharing on Windows, so this succeeds
        // even while a freshly started daemon still holds an inherited handle.
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_rows_are_owned_by_the_agent_browser_and_match_the_core_digest() {
        let (digest, tools) = catalog();
        assert_eq!(digest, core::catalog_digest());
        assert_eq!(tools.len(), core::tools().len());
        for tool in &tools {
            assert_eq!(tool.action.extension_id, EXTENSION_ID);
            assert_eq!(
                tool.action.canonical_id,
                format!("{EXTENSION_ID}:{}", tool.action.action_id)
            );
            assert_eq!(tool.parameters["type"], "object");
        }
        let safe: Vec<_> = tools
            .iter()
            .filter(|tool| tool.action.risk.is_safe())
            .map(|tool| tool.action.action_id.as_str())
            .collect();
        assert_eq!(safe, ["snapshot", "read", "wait"]);
    }

    #[test]
    fn failures_are_classified_for_the_model() {
        assert_eq!(
            classify("Unknown ref: e9. The page changed"),
            FailureClass::NotFound
        );
        assert_eq!(
            classify("net::ERR_NAME_NOT_RESOLVED"),
            FailureClass::Network
        );
        assert_eq!(classify("No option matched"), FailureClass::Internal);
    }

    #[test]
    fn output_file_is_removed_when_dropped() {
        let path = std::env::temp_dir().join(format!(
            "grain-agent-browser-test-{}.json",
            std::process::id()
        ));
        let output = OutputFile::create(path.clone()).expect("create");
        std::io::Write::write_all(&mut &output.file, b"{\"success\":true}").expect("write");
        assert_eq!(output.read().expect("read"), "{\"success\":true}");
        drop(output);
        assert!(!path.exists());
    }

    #[test]
    fn a_missing_binary_is_reported_not_panicked() {
        let error = run_captured(
            Path::new("Z:/definitely/not/agent-browser.exe"),
            &["--version"],
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.starts_with("Could not start agent-browser"));
    }

    /// Prints what install discovery finds on this machine. Run with
    /// `cargo test --lib live_install -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads this machine's agent-browser install"]
    fn live_install_discovery_reports_what_it_found() {
        match resolve() {
            Ok(install) => println!(
                "found {} at {}",
                core::format_version(install.version),
                install.path.display()
            ),
            Err(message) => println!("not usable: {message}"),
        }
    }

    /// Drives the real CLI through the real process path. Needs network and an
    /// installed agent-browser >= the minimum version on PATH. Run with
    /// `cargo test --lib live_session -- --ignored`.
    #[test]
    #[ignore = "needs agent-browser installed and network access"]
    fn live_session_opens_reads_and_closes_a_page() {
        let step = |action: &str, args: serde_json::Value| {
            let call = core::parse(action, &args).expect("valid call");
            run_call(&call).unwrap_or_else(|failure| panic!("{action}: {}", failure.message))
        };
        let install = resolve().expect("agent-browser installed");
        println!(
            "using {} {}",
            install.path.display(),
            core::format_version(install.version)
        );
        let opened = step("open", serde_json::json!({"url": "https://example.com"}));
        assert!(opened.contains("Example Domain"), "{opened}");
        let snapshot = step("snapshot", serde_json::json!({}));
        assert!(snapshot.contains("[ref=e"), "{snapshot}");
        let text = step("read", serde_json::json!({}));
        assert!(text.contains("documentation"), "{text}");
        let refused = core::parse(
            "open",
            &serde_json::json!({"url": "file:///C:/Windows/win.ini"}),
        );
        assert!(refused.is_err());
        let stale = run_call(&core::parse("click", &serde_json::json!({"ref": "@e999"})).unwrap())
            .err()
            .expect("unknown ref fails");
        assert!(
            stale.message.contains("take a new snapshot"),
            "{}",
            stale.message
        );
        assert_eq!(step("close", serde_json::json!({})), "Closed the browser.");
        assert!(!USED.load(Ordering::SeqCst));
    }
}
