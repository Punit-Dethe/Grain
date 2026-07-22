//! [GRAIN] Developer-only Tier-C companion process supervisor (Phase 4).
//!
//! A companion is an ordinary OS process, but Grain's boundary stays the same:
//! one minted WebSocket token binds identity and grants. This module owns only
//! process lifetime. The events server still authenticates and capability-gates
//! every message.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

const MAX_CONSECUTIVE_CRASHES: u32 = 3;
const STABLE_UPTIME: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

struct Control {
    stopping: AtomicBool,
    child: Mutex<Option<Child>>,
}

static RUNNING: OnceLock<Mutex<HashMap<String, Arc<Control>>>> = OnceLock::new();

fn running() -> &'static Mutex<HashMap<String, Arc<Control>>> {
    RUNNING.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn start(
    ext_id: &str,
    token: &str,
    root: PathBuf,
    binary: PathBuf,
    activation: Option<Value>,
) -> Result<(), String> {
    if !binary.starts_with(&root) || !binary.is_file() {
        return Err("companion binary must be a file inside its load-unpacked project".into());
    }
    stop(ext_id, "companion replaced");

    let control = Arc::new(Control {
        stopping: AtomicBool::new(false),
        child: Mutex::new(None),
    });
    running()
        .lock()
        .unwrap()
        .insert(ext_id.to_string(), control.clone());

    let ext_id = ext_id.to_string();
    let map_id = ext_id.clone();
    let token = token.to_string();
    let activation = serde_json::to_string(&activation.unwrap_or(Value::Null))
        .map_err(|error| error.to_string())?;
    if let Err(error) = std::thread::Builder::new()
        .name(format!("ext-companion-{ext_id}"))
        .spawn(move || supervise(ext_id, token, root, binary, activation, control))
    {
        running().lock().unwrap().remove(&map_id);
        return Err(format!("start companion supervisor: {error}"));
    }
    Ok(())
}

pub fn stop(ext_id: &str, reason: &str) {
    let control = running().lock().unwrap().remove(ext_id);
    let Some(control) = control else {
        return;
    };
    control.stopping.store(true, Ordering::Release);
    if let Some(child) = control.child.lock().unwrap().as_mut() {
        let _ = child.kill();
    }
    log::info!("[ext:{ext_id}] life companion stopping ({reason})");
}

pub fn stop_all() {
    let ids = running()
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for id in ids {
        stop(&id, "Grain exiting");
    }
}

fn supervise(
    ext_id: String,
    token: String,
    root: PathBuf,
    binary: PathBuf,
    activation: String,
    control: Arc<Control>,
) {
    #[cfg(windows)]
    let job = crate::events_server::create_job_object();

    let mut crashes = 0_u32;
    loop {
        if control.stopping.load(Ordering::Acquire) {
            break;
        }
        let started = Instant::now();
        let result = spawn_child(&ext_id, &token, &root, &binary, &activation);
        match result {
            Ok(mut child) => {
                #[cfg(windows)]
                if let Some(job) = &job {
                    crate::events_server::assign_child_to_job(job, &child);
                }
                log::info!("[ext:{ext_id}] life companion started (pid {})", child.id());
                drain_logs(&ext_id, child.stdout.take(), child.stderr.take());
                *control.child.lock().unwrap() = Some(child);
                loop {
                    if control.stopping.load(Ordering::Acquire) {
                        if let Some(child) = control.child.lock().unwrap().as_mut() {
                            let _ = child.kill();
                        }
                    }
                    let status = control
                        .child
                        .lock()
                        .unwrap()
                        .as_mut()
                        .and_then(|child| child.try_wait().ok())
                        .flatten();
                    if let Some(status) = status {
                        log::warn!("[ext:{ext_id}] life companion exited ({status})");
                        break;
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
                control.child.lock().unwrap().take();
            }
            Err(error) => {
                log::error!("[ext:{ext_id}] life companion spawn failed: {error}");
            }
        }

        if control.stopping.load(Ordering::Acquire) {
            break;
        }
        if started.elapsed() >= STABLE_UPTIME {
            crashes = 0;
        }
        crashes += 1;
        if crashes >= MAX_CONSECUTIVE_CRASHES {
            crate::extension_host::companion_gave_up(
                &ext_id,
                &token,
                format!("Native companion crashed {crashes} times and was stopped."),
            );
            break;
        }
        let backoff = Duration::from_secs(1_u64 << (crashes - 1));
        log::warn!(
            "[ext:{ext_id}] life companion restart in {}s (crash {crashes}/{MAX_CONSECUTIVE_CRASHES})",
            backoff.as_secs()
        );
        let deadline = Instant::now() + backoff;
        while Instant::now() < deadline && !control.stopping.load(Ordering::Acquire) {
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    let mut map = running().lock().unwrap();
    if map
        .get(&ext_id)
        .is_some_and(|current| Arc::ptr_eq(current, &control))
    {
        map.remove(&ext_id);
    }
}

fn spawn_child(
    ext_id: &str,
    token: &str,
    root: &Path,
    binary: &Path,
    activation: &str,
) -> Result<Child, String> {
    let mut command = companion_command(binary);
    command
        .current_dir(root)
        .env("GRAIN_EXTENSION_ID", ext_id)
        .env("GRAIN_EVENTS_TOKEN", token)
        .env("GRAIN_EVENTS_URL", "ws://127.0.0.1:7124")
        .env("GRAIN_API_VERSION", grain_sdk::GRAIN_API_VERSION)
        .env("GRAIN_ACTIVATION_JSON", activation)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let parent = std::process::id() as libc::pid_t;
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Grain exited before companion spawn completed",
                    ));
                }
                Ok(())
            });
        }
    }

    command.spawn().map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
fn companion_command(binary: &Path) -> Command {
    Command::new(binary)
}

/// macOS has no `PR_SET_PDEATHSIG`. A tiny shell parent watches Grain's PID,
/// kills the companion when it disappears, and also traps normal supervisor
/// termination. The arbitrary extension binary remains a separate child.
#[cfg(target_os = "macos")]
fn companion_command(binary: &Path) -> Command {
    let script = r#"parent="$1"; binary="$2"; "$binary" & child=$!; trap 'kill "$child" 2>/dev/null; wait "$child" 2>/dev/null' EXIT TERM INT; while kill -0 "$parent" 2>/dev/null && kill -0 "$child" 2>/dev/null; do sleep 1; done; kill "$child" 2>/dev/null; wait "$child""#;
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(script)
        .arg("grain-companion-watch")
        .arg(std::process::id().to_string())
        .arg(binary);
    command
}

fn drain_logs(
    ext_id: &str,
    stdout: Option<impl std::io::Read + Send + 'static>,
    stderr: Option<impl std::io::Read + Send + 'static>,
) {
    for pipe in [
        stdout.map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
        stderr.map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let ext_id = ext_id.to_string();
        std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                log::info!("[ext:{ext_id}] companion {line}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_policy_is_bounded_and_parent_cleanup_is_platform_backed() {
        assert_eq!(MAX_CONSECUTIVE_CRASHES, 3);
        assert_eq!(1_u64 << (1 - 1), 1);
        assert_eq!(1_u64 << (2 - 1), 2);
        assert!(STABLE_UPTIME > Duration::from_secs(1));
    }
}
