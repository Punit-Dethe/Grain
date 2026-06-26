//! [GRAIN] B1: local WebSocket that streams `DaemonEvent`s to the pill.
//!
//! The core listens on `127.0.0.1:EVENTS_PORT`; each connecting client (the pill)
//! subscribes to the `AppContext` broadcast bus and receives every event as JSON.
//! This is the seed of the future local server (the OpenAI-compatible endpoints
//! grow on the same listener later).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use grain_core::AppContext;
use tokio::net::TcpStream;
use tokio::sync::broadcast::error::RecvError;
use tokio_tungstenite::tungstenite::Message;

/// Fixed loopback port the pill connects to (`ws://127.0.0.1:EVENTS_PORT`).
pub const EVENTS_PORT: u16 = 7124;
static EVENTS_READY: AtomicBool = AtomicBool::new(false);

/// Spawn the event WS server on the Tauri async runtime.
pub fn start(ctx: Arc<AppContext>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("[GRAIN] events WS runtime failed: {e}");
                return;
            }
        };

        rt.block_on(async move {
            let addr = format!("127.0.0.1:{EVENTS_PORT}");
            let listener = match addr.parse::<std::net::SocketAddr>() {
                Ok(socket_addr) => {
                    let socket = tokio::net::TcpSocket::new_v4().expect("Failed to create socket");
                    let _ = socket.set_reuseaddr(true);
                    if let Err(e) = socket.bind(socket_addr) {
                        log::error!("[GRAIN] events WS bind {addr} failed: {e}");
                        return;
                    }
                    match socket.listen(1024) {
                        Ok(l) => l,
                        Err(e) => {
                            log::error!("[GRAIN] events WS listen failed: {e}");
                            return;
                        }
                    }
                }
                Err(e) => {
                    log::error!("[GRAIN] events WS invalid address: {e}");
                    return;
                }
            };
            EVENTS_READY.store(true, Ordering::Release);
            log::info!("[GRAIN] events WS listening on ws://{addr}");
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let ctx = ctx.clone();
                        tokio::spawn(handle(stream, ctx));
                    }
                    Err(e) => log::warn!("[GRAIN] events WS accept error: {e}"),
                }
            }
        });
    });
}

async fn handle(stream: TcpStream, ctx: Arc<AppContext>) {
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            log::warn!("[GRAIN] events WS handshake failed: {e}");
            return;
        }
    };
    let (mut write, mut read) = ws.split();
    let mut rx = ctx.subscribe();
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(ev) => {
                    if let Ok(json) = serde_json::to_string(&ev) {
                        if write.send(Message::Text(json.into())).await.is_err() {
                            break; // client gone
                        }
                    }
                }
                Err(RecvError::Lagged(_)) => continue, // dropped some; keep streaming
                Err(RecvError::Closed) => break,        // bus closed (shutdown)
            },
            // Detect client close / drain pings.
            msg = read.next() => match msg {
                Some(Ok(_)) => {}
                _ => break,
            },
        }
    }
}

/// [GRAIN] B2: launch the pill process and keep it alive (the "always-armed"
/// surface). In a bundled build the pill sits next to the core exe; in dev, run
/// `cargo run -p grain-pill` manually (it connects to the same WS).
///
/// On Windows, the pill is assigned to a **Job Object** with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. This is a kernel-level guarantee that
/// the pill is terminated when the main process exits — regardless of whether
/// exit is clean (tray Quit), forced (Ctrl+C), or a crash.
pub fn spawn_pill_supervisor() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::sync::atomic::Ordering;
    std::thread::spawn(|| {
        let Some(exe) = find_pill() else {
            log::warn!("[GRAIN] grain-pill not found next to the exe — run it manually in dev");
            return;
        };
        log::info!("[GRAIN] pill supervisor: launching {}", exe.display());

        // [GRAIN] Kill any stray pill left by a previous (crashed / force-quit)
        // session BEFORE spawning ours, so multiple overlapping layered windows
        // can never stack up (the cause of the "pill not visible" bug). Only one
        // pill should ever run. This is a safety net for upgrades from versions
        // that didn't use Job Objects.
        kill_stray_pills();

        for _ in 0..50 {
            if EVENTS_READY.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if !EVENTS_READY.load(Ordering::Acquire) {
            log::warn!("[GRAIN] events WS was not ready; skipping pill launch");
            return;
        }

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        #[cfg(windows)]
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        // [GRAIN] Create a Job Object so the OS kernel guarantees the pill is
        // killed when the main process exits (for ANY reason).
        #[cfg(windows)]
        let job = create_job_object();

        loop {
            let mut cmd = std::process::Command::new(&exe);
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);

            match cmd.spawn()
            {
                Ok(mut child) => {
                    // [GRAIN] Assign pill to the Job Object immediately after
                    // spawn, before anything else. If this fails the pill still
                    // works — it just won't be auto-killed on parent exit
                    // (the WS-disconnect fallback handles that case).
                    #[cfg(windows)]
                    if let Some(ref job_handle) = job {
                        assign_child_to_job(job_handle, &child);
                    }

                    // Drain the pill's stdout/stderr into our log so its
                    // diagnostics are visible (it has no console of its own).
                    for pipe in [
                        child
                            .stdout
                            .take()
                            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
                        child
                            .stderr
                            .take()
                            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        std::thread::spawn(move || {
                            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                                log::info!("[pill] {line}");
                            }
                        });
                    }
                    let status = child.wait();
                    log::warn!("[GRAIN] pill exited ({status:?}) — restarting in 1s");
                }
                Err(e) => {
                    log::error!("[GRAIN] failed to spawn pill: {e}");
                    return;
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

// ── Windows Job Object ──────────────────────────────────────────────────────

/// Create an anonymous Job Object configured with `KILL_ON_JOB_CLOSE`.
///
/// When the last handle to this Job Object is closed (which happens
/// automatically when the owning process exits), the OS kernel terminates
/// every process still assigned to the job. This is the production-grade
/// mechanism used by Chrome, VS Code, etc.
#[cfg(windows)]
fn create_job_object() -> Option<windows::Win32::Foundation::HANDLE> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::JobObjects::{
        CreateJobObjectW, SetInformationJobObject, JobObjectExtendedLimitInformation,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    unsafe {
        let job = match CreateJobObjectW(None, None) {
            Ok(h) => h,
            Err(e) => {
                log::error!("[GRAIN] failed to create Job Object: {e}");
                return None;
            }
        };

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if let Err(e) = ok {
            log::error!("[GRAIN] failed to configure Job Object: {e}");
            let _ = CloseHandle(job);
            return None;
        }

        log::info!("[GRAIN] pill Job Object created (KILL_ON_JOB_CLOSE)");
        Some(job)
    }
}

/// Assign a child process to the Job Object so it is automatically killed
/// when the main process exits.
#[cfg(windows)]
fn assign_child_to_job(
    job: &windows::Win32::Foundation::HANDLE,
    child: &std::process::Child,
) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::AssignProcessToJobObject;

    unsafe {
        let child_handle = HANDLE(child.as_raw_handle());
        if let Err(e) = AssignProcessToJobObject(*job, child_handle) {
            log::warn!("[GRAIN] failed to assign pill to Job Object: {e}");
        } else {
            log::info!("[GRAIN] pill assigned to Job Object (pid {})", child.id());
        }
    }
}

/// [GRAIN] Kill any `grain-pill` process left over from a previous crashed /
/// force-quit session so multiple layered windows can never stack up.
fn kill_stray_pills() {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/IM", "grain-pill.exe"])
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("pkill")
            .arg("-f")
            .arg("grain-pill")
            .output();
    }
}

fn find_pill() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "grain-pill.exe"
    } else {
        "grain-pill"
    };

    // [GRAIN] Tauri `externalBin` places the pill next to the core exe but
    // appends the Rust target triple, e.g.:
    //   grain-pill-x86_64-pc-windows-msvc.exe
    // We probe that name first in release so the bundled copy is always preferred.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()));

    // Next to the core exe — plain name (dev layout / future non-triple builds).
    let next_to_exe = exe_dir.as_ref().map(|d| d.join(name));

    // Next to the core exe — Tauri externalBin triple-suffixed name (release layout).
    let triple_name = if cfg!(windows) {
        format!("grain-pill-{}.exe", std::env::consts::ARCH.replace("x86_64", "x86_64-pc-windows-msvc"))
    } else {
        format!("grain-pill-{}", std::env::consts::ARCH)
    };
    // Use the real target triple compiled into the binary for reliability.
    let triple_suffixed = exe_dir.as_ref().map(|d| {
        // Try the exact triple that cargo uses on Windows.
        #[cfg(target_os = "windows")]
        { d.join("grain-pill-x86_64-pc-windows-msvc.exe") }
        #[cfg(target_os = "macos")]
        { d.join(format!("grain-pill-{}-apple-darwin", std::env::consts::ARCH)) }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        { d.join(format!("grain-pill-{}-unknown-linux-gnu", std::env::consts::ARCH)) }
    });

    // Workspace target — where `cargo build -p grain-pill` lands in dev. src-tauri
    // uses a SEPARATE target dir (e.g. C:\gt), so resolve the pill from the
    // build-time manifest dir (…/grain/src-tauri → …/grain/target/{debug,release}).
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().map(|w| w.join("target"));
    let workspace_debug = workspace.as_ref().map(|t| t.join("debug").join(name));
    let workspace_release = workspace.as_ref().map(|t| t.join("release").join(name));

    // [GRAIN] In DEV (debug) prefer the freshly-built workspace binary — a stale
    // copy can linger next to the core exe in the src-tauri target dir and would
    // otherwise shadow every rebuild. In RELEASE prefer the bundled exe.
    let order: Vec<Option<PathBuf>> = if cfg!(debug_assertions) {
        vec![workspace_debug, workspace_release, next_to_exe, triple_suffixed]
    } else {
        vec![triple_suffixed, next_to_exe, workspace_release, workspace_debug]
    };
    // Suppress unused variable warning from the triple_name variable on some platforms.
    let _ = triple_name;
    order.into_iter().flatten().find(|c| c.exists())
}

