//! [GRAIN] B1: local WebSocket that streams `DaemonEvent`s to the pill.
//!
//! The core listens on `127.0.0.1:EVENTS_PORT`; each connecting client (the pill)
//! subscribes to the `AppContext` broadcast bus and receives every event as JSON.
//! This is the seed of the future local server (the OpenAI-compatible endpoints
//! grow on the same listener later).

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
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                log::error!("[GRAIN] failed to get current executable path: {}", e);
                return;
            }
        };
        log::info!(
            "[GRAIN] pill supervisor: launching {} --pill",
            exe.display()
        );

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
            cmd.arg("--pill");
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);

            #[cfg(target_os = "linux")]
            {
                use std::os::unix::process::CommandExt;
                unsafe {
                    cmd.pre_exec(|| {
                        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                        Ok(())
                    });
                }
            }

            match cmd.spawn() {
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

// ── Windows Job Object (raw FFI — no extra `windows` crate features) ────────

/// Opaque handle wrapper for the Job Object (same layout as HANDLE).
#[cfg(windows)]
#[derive(Copy, Clone)]
struct JobHandle(isize);

#[cfg(windows)]
mod job_ffi {
    //! Minimal raw FFI for Job Objects. We declare only the 3 functions and
    //! 2 structs we actually need so we don't pull in `Win32_Security` /
    //! `Win32_System_JobObjects` from the `windows` crate (which bloats
    //! the binary by several MB and increases resident memory).
    use std::ffi::c_void;

    pub type HANDLE = isize;
    pub type BOOL = i32;

    // --- Job Object info class & limits ---
    pub const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;
    pub const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x00002000;

    #[repr(C)]
    #[derive(Default)]
    pub struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        pub per_process_user_time_limit: i64,
        pub per_job_user_time_limit: i64,
        pub limit_flags: u32,
        pub minimum_working_set_size: usize,
        pub maximum_working_set_size: usize,
        pub active_process_limit: u32,
        pub affinity: usize,
        pub priority_class: u32,
        pub scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct IO_COUNTERS {
        pub read_operation_count: u64,
        pub write_operation_count: u64,
        pub other_operation_count: u64,
        pub read_transfer_count: u64,
        pub write_transfer_count: u64,
        pub other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        pub basic_limit_information: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        pub io_info: IO_COUNTERS,
        pub process_memory_limit: usize,
        pub job_memory_limit: usize,
        pub peak_process_memory_used: usize,
        pub peak_job_memory_used: usize,
    }

    extern "system" {
        pub fn CreateJobObjectW(
            lp_job_attributes: *const c_void, // SECURITY_ATTRIBUTES* — we pass null
            lp_name: *const u16,              // optional name — we pass null
        ) -> HANDLE;

        pub fn SetInformationJobObject(
            h_job: HANDLE,
            job_object_information_class: u32,
            lp_job_object_information: *const c_void,
            cb_job_object_information_length: u32,
        ) -> BOOL;

        pub fn AssignProcessToJobObject(h_job: HANDLE, h_process: HANDLE) -> BOOL;

        pub fn CloseHandle(h_object: HANDLE) -> BOOL;

        pub fn GetLastError() -> u32;
    }
}

/// Create an anonymous Job Object configured with `KILL_ON_JOB_CLOSE`.
///
/// When the last handle to this Job Object is closed (which happens
/// automatically when the owning process exits), the OS kernel terminates
/// every process still assigned to the job. This is the production-grade
/// mechanism used by Chrome, VS Code, etc.
#[cfg(windows)]
fn create_job_object() -> Option<JobHandle> {
    use std::ptr;

    unsafe {
        let job = job_ffi::CreateJobObjectW(ptr::null(), ptr::null());
        if job == 0 {
            log::error!(
                "[GRAIN] failed to create Job Object (win32 error {})",
                job_ffi::GetLastError()
            );
            return None;
        }

        let mut info = job_ffi::JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.basic_limit_information.limit_flags = job_ffi::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let ok = job_ffi::SetInformationJobObject(
            job,
            job_ffi::JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<job_ffi::JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if ok == 0 {
            log::error!(
                "[GRAIN] failed to configure Job Object (win32 error {})",
                job_ffi::GetLastError()
            );
            job_ffi::CloseHandle(job);
            return None;
        }

        log::info!("[GRAIN] pill Job Object created (KILL_ON_JOB_CLOSE)");
        Some(JobHandle(job))
    }
}

/// Assign a child process to the Job Object so it is automatically killed
/// when the main process exits.
#[cfg(windows)]
fn assign_child_to_job(job: &JobHandle, child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;

    unsafe {
        let child_handle = child.as_raw_handle() as job_ffi::HANDLE;
        let ok = job_ffi::AssignProcessToJobObject(job.0, child_handle);
        if ok == 0 {
            log::warn!(
                "[GRAIN] failed to assign pill to Job Object (win32 error {})",
                job_ffi::GetLastError()
            );
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
        // Kill legacy standalone pill
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/IM", "grain-pill.exe"])
            .output();

        // Kill any multicall pills (processes with --pill in command line)
        let exe_name = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "grain.exe".to_string());

        let _ = std::process::Command::new("wmic")
            .args([
                "process",
                "where",
                &format!("name='{}' and commandline like '%--pill%'", exe_name),
                "call",
                "terminate",
            ])
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("pkill")
            .arg("-f")
            .arg("grain-pill")
            .output();

        let _ = std::process::Command::new("pkill")
            .arg("-f")
            .arg("--pill")
            .output();
    }
}
