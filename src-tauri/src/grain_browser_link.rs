//! [GRAIN] Grain side of the Grain Pilot browser extension
//! (`docs/Pilot/EXTENSION-PLAN.md`).
//!
//! The extension (`browser-extension/`) connects through native messaging:
//! the browser starts `grain-browser-host.exe`, which relays length-prefixed
//! JSON between the extension and this module's named pipe. Nothing listens
//! on the network.
//!
//! - [`register`] / [`unregister`]: the host manifest and its per-user
//!   registry entries for Chrome, Chromium, Edge and Brave.
//! - [`ensure_started`] / [`stop`]: the pipe server. It runs only while Pilot
//!   needs it.
//! - [`request`]: send one operation to the extension and wait for its reply.
//!
//! Only the host executable Grain registered may connect: the pipe rejects
//! remote clients, and every client's process image is checked against
//! [`host_path`] before a byte is read.

// Wired into `grain_pilot.rs` in Phase 2 of the plan; until then only the
// live test below drives it.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

/// Native messaging host name (the extension's `connectNative` argument).
pub(crate) const HOST_NAME: &str = "com.grain.pilot";
/// The extension's id, fixed by the `key` in `browser-extension/manifest.json`.
pub(crate) const EXTENSION_ID: &str = "idmbchpcgdgdghoodfajgibenmfapcnn";
/// Host executable file name.
const HOST_EXE: &str = "grain-browser-host.exe";
/// Largest message accepted from the extension (a page read).
const MAX_MESSAGE: u32 = 8 * 1024 * 1024;

/// Per-user registry keys the browsers read native messaging hosts from.
const REGISTRY_ROOTS: &[&str] = &[
    r"Software\Google\Chrome\NativeMessagingHosts",
    r"Software\Chromium\NativeMessagingHosts",
    r"Software\Microsoft\Edge\NativeMessagingHosts",
    r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
];

/// Grain's pipe for this Windows user; must match the host's `pipe_name`.
fn pipe_name() -> String {
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    format!(r"\\.\pipe\grain-pilot-{user}")
}

/// The host executable: `GRAIN_BROWSER_HOST` when set (development and
/// tests), otherwise next to Grain's own executable.
pub(crate) fn host_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GRAIN_BROWSER_HOST") {
        return Some(PathBuf::from(path));
    }
    Some(std::env::current_exe().ok()?.parent()?.join(HOST_EXE))
}

/// The host manifest the browsers read.
fn host_manifest(host: &Path) -> Value {
    json!({
        "name": HOST_NAME,
        "description": "Grain Pilot",
        "path": host,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    })
}

/// Write the host manifest into `dir` and point every browser's per-user
/// registry entry at it.
#[cfg(windows)]
pub(crate) fn register(host: &Path, dir: &Path) -> Result<(), String> {
    if !host.is_file() {
        return Err(format!("The browser host is missing ({}).", host.display()));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let manifest = dir.join(format!("{HOST_NAME}.json"));
    let text = serde_json::to_string_pretty(&host_manifest(host)).map_err(|e| e.to_string())?;
    std::fs::write(&manifest, text)
        .map_err(|e| format!("Could not write the host manifest: {e}"))?;
    let user = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    for root in REGISTRY_ROOTS {
        let (key, _) = user
            .create_subkey(format!(r"{root}\{HOST_NAME}"))
            .map_err(|e| format!("Could not register the browser host: {e}"))?;
        key.set_value("", &manifest.to_string_lossy().to_string())
            .map_err(|e| format!("Could not register the browser host: {e}"))?;
    }
    Ok(())
}

/// Remove the registry entries and the manifest written by [`register`].
#[cfg(windows)]
pub(crate) fn unregister(dir: &Path) {
    let user = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    for root in REGISTRY_ROOTS {
        let _ = user.delete_subkey_all(format!(r"{root}\{HOST_NAME}"));
    }
    let _ = std::fs::remove_file(dir.join(format!("{HOST_NAME}.json")));
}

#[cfg(not(windows))]
pub(crate) fn register(_: &Path, _: &Path) -> Result<(), String> {
    Err("Grain Pilot's browser link runs on Windows only for now.".into())
}

#[cfg(not(windows))]
pub(crate) fn unregister(_: &Path) {}

// ── The pipe server ─────────────────────────────────────────────────────────

/// The extension connection currently in use.
struct Connection {
    generation: u64,
    browser: String,
    send: mpsc::UnboundedSender<Vec<u8>>,
}

struct Link {
    connection: Mutex<Option<Connection>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
    next_id: AtomicU64,
    next_generation: AtomicU64,
    stop: tokio::sync::Notify,
}

static LINK: Mutex<Option<Arc<Link>>> = Mutex::new(None);

fn link() -> Option<Arc<Link>> {
    LINK.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Start the pipe server if it is not running. Needs a Tokio runtime.
pub(crate) fn ensure_started() -> Result<(), String> {
    let mut slot = LINK.lock().unwrap_or_else(|p| p.into_inner());
    if slot.is_some() {
        return Ok(());
    }
    let host = host_path().ok_or("Could not locate the browser host.")?;
    let link = Arc::new(Link {
        connection: Mutex::new(None),
        pending: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        next_generation: AtomicU64::new(1),
        stop: tokio::sync::Notify::new(),
    });
    #[cfg(windows)]
    {
        let first = tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(pipe_name())
            .map_err(|e| format!("Could not open Grain Pilot's pipe: {e}"))?;
        tauri::async_runtime::spawn(serve(link.clone(), first, host));
    }
    #[cfg(not(windows))]
    let _ = host;
    *slot = Some(link);
    Ok(())
}

/// Stop the pipe server and drop the extension's connection. The extension
/// goes idle and tries again later.
pub(crate) fn stop() {
    if let Some(link) = LINK.lock().unwrap_or_else(|p| p.into_inner()).take() {
        link.stop.notify_waiters();
        link.connection
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        link.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }
}

/// The browser the extension reported, when one is connected.
pub(crate) fn connected_browser() -> Option<String> {
    let link = link()?;
    let connection = link.connection.lock().unwrap_or_else(|p| p.into_inner());
    connection.as_ref().map(|c| c.browser.clone())
}

#[cfg(windows)]
async fn serve(
    link: Arc<Link>,
    mut server: tokio::net::windows::named_pipe::NamedPipeServer,
    host: PathBuf,
) {
    use tokio::net::windows::named_pipe::ServerOptions;
    loop {
        tokio::select! {
            _ = link.stop.notified() => return,
            connected = server.connect() => {
                if connected.is_err() {
                    return;
                }
            }
        }
        let next = match ServerOptions::new()
            .reject_remote_clients(true)
            .create(pipe_name())
        {
            Ok(next) => next,
            Err(error) => {
                log::warn!("Grain Pilot: could not reopen the pipe: {error}");
                return;
            }
        };
        let client = std::mem::replace(&mut server, next);
        if !client_is_host(&client, &host) {
            log::warn!("Grain Pilot: refused a pipe client that is not the browser host");
            continue;
        }
        tauri::async_runtime::spawn(handle(link.clone(), client));
    }
}

/// Whether the pipe's client process is the registered host executable.
#[cfg(windows)]
fn client_is_host(pipe: &tokio::net::windows::named_pipe::NamedPipeServer, host: &Path) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Pipes::GetNamedPipeClientProcessId;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let mut pid = 0u32;
    let image = unsafe {
        if GetNamedPipeClientProcessId(HANDLE(pipe.as_raw_handle()), &mut pid).is_err() {
            return false;
        }
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut buffer = vec![0u16; 1024];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        if result.is_err() {
            return false;
        }
        PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize]))
    };
    same_file(&image, host)
}

fn same_file(a: &Path, b: &Path) -> bool {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canonical(a)
        .to_string_lossy()
        .eq_ignore_ascii_case(&canonical(b).to_string_lossy())
}

#[cfg(windows)]
async fn handle(link: Arc<Link>, pipe: tokio::net::windows::named_pipe::NamedPipeServer) {
    let (mut reader, mut writer) = tokio::io::split(pipe);
    let (send, mut outgoing) = mpsc::unbounded_channel::<Vec<u8>>();
    let generation = link.next_generation.fetch_add(1, Ordering::Relaxed);
    let writing = tauri::async_runtime::spawn(async move {
        while let Some(body) = outgoing.recv().await {
            if writer
                .write_all(&(body.len() as u32).to_ne_bytes())
                .await
                .is_err()
                || writer.write_all(&body).await.is_err()
                || writer.flush().await.is_err()
            {
                break;
            }
        }
    });
    let mut send = Some(send);
    loop {
        let mut length = [0u8; 4];
        if reader.read_exact(&mut length).await.is_err() {
            break;
        }
        let length = u32::from_ne_bytes(length);
        if length > MAX_MESSAGE {
            log::warn!("Grain Pilot: the extension sent an oversized message");
            break;
        }
        let mut body = vec![0u8; length as usize];
        if reader.read_exact(&mut body).await.is_err() {
            break;
        }
        let Ok(message) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
        if message.get("type").and_then(Value::as_str) == Some("hello") {
            let browser = message
                .get("browser")
                .and_then(Value::as_str)
                .unwrap_or("Chromium")
                .chars()
                .take(40)
                .collect::<String>();
            if let Some(send) = send.take() {
                log::info!("Grain Pilot: {browser} extension connected");
                *link.connection.lock().unwrap_or_else(|p| p.into_inner()) = Some(Connection {
                    generation,
                    browser,
                    send,
                });
            }
            continue;
        }
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            if let Some(waiter) = link
                .pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id)
            {
                let _ = waiter.send(message);
            }
        }
    }
    let mut connection = link.connection.lock().unwrap_or_else(|p| p.into_inner());
    if connection.as_ref().map(|c| c.generation) == Some(generation) {
        *connection = None;
    }
    drop(connection);
    writing.abort();
}

/// Send one operation to the extension and wait up to `timeout` for its
/// reply. `op` is a JSON object with an `op` field; the id is added here.
pub(crate) async fn request(mut op: Value, timeout: Duration) -> Result<Value, String> {
    let link = link().ok_or("Grain Pilot's browser link is not running.")?;
    let id = link.next_id.fetch_add(1, Ordering::Relaxed);
    let (reply, wait) = oneshot::channel();
    {
        let connection = link.connection.lock().unwrap_or_else(|p| p.into_inner());
        let connection = connection.as_ref().ok_or(
            "The Grain Pilot browser extension is not connected. Is the browser open with the extension on?",
        )?;
        op["id"] = json!(id);
        let body = serde_json::to_vec(&op).map_err(|e| e.to_string())?;
        link.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, reply);
        connection
            .send
            .send(body)
            .map_err(|_| "The browser extension disconnected.".to_string())?;
    }
    let message = match tokio::time::timeout(timeout, wait).await {
        Ok(Ok(message)) => message,
        Ok(Err(_)) => return Err("The browser extension disconnected.".into()),
        Err(_) => {
            link.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            return Err("The browser extension did not answer in time.".into());
        }
    };
    if message.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(message.get("result").cloned().unwrap_or(Value::Null))
    } else {
        Err(message
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("The browser extension refused.")
            .chars()
            .take(500)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_manifest_allows_only_grains_extension() {
        let manifest = host_manifest(Path::new(r"C:\Grain\grain-browser-host.exe"));
        assert_eq!(manifest["name"], HOST_NAME);
        assert_eq!(manifest["type"], "stdio");
        assert_eq!(
            manifest["allowed_origins"],
            json!([format!("chrome-extension://{EXTENSION_ID}/")])
        );
    }

    #[test]
    fn extension_id_matches_the_manifest_key() {
        use sha2::{Digest, Sha256};
        let manifest: Value =
            serde_json::from_str(include_str!("../../browser-extension/manifest.json")).unwrap();
        let key = manifest["key"].as_str().unwrap();
        let der = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(key)
                .unwrap()
        };
        let digest = Sha256::digest(&der);
        let id: String = digest[..16]
            .iter()
            .flat_map(|byte| [byte >> 4, byte & 0xf])
            .map(|nibble| (b'a' + nibble) as char)
            .collect();
        assert_eq!(id, EXTENSION_ID);
    }

    /// Live: the whole chain (Grain pipe ⇄ host ⇄ extension ⇄ page-controller)
    /// in a SEPARATE Edge profile with the unpacked extension. Needs
    /// `browser-extension/dist` built and `GRAIN_BROWSER_HOST` pointing at a
    /// built host. Only the Edge processes started with this test's own
    /// profile folder are ended afterwards.
    ///
    /// `cargo test --lib live_browser_link -- --ignored --nocapture`
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn live_browser_link_drives_a_separate_edge_profile() {
        let host = host_path().expect("host");
        let root = std::env::temp_dir().join("grain-pilot-link-live");
        let profile = root.join("edge-profile");
        let extension = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("browser-extension")
            .join("dist");
        assert!(
            extension.join("page.js").is_file(),
            "build browser-extension first"
        );
        // A fresh profile each run: Edge keeps an unpacked extension's old
        // service worker in a reused profile.
        let _ = std::fs::remove_dir_all(&profile);
        register(&host, &root).expect("register");

        struct Cleanup(PathBuf, PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                // End only the Edge instance using this test's profile folder.
                let script = format!(
                    "Get-CimInstance Win32_Process -Filter \"Name='msedge.exe'\" | Where-Object {{ $_.CommandLine -like '*{}*' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}",
                    self.1.display()
                );
                let _ = std::process::Command::new("powershell")
                    .args(["-NoProfile", "-Command", &script])
                    .status();
                stop();
                unregister(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone(), profile.clone());

        tauri::async_runtime::block_on(async {
            ensure_started().expect("start");
            std::process::Command::new(
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            )
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--load-extension={}", extension.display()))
            .args([
                "--no-first-run",
                "--no-default-browser-check",
                "about:blank",
            ])
            .spawn()
            .expect("launch Edge");
            let mut waited = 0;
            while connected_browser().is_none() && waited < 60 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                waited += 1;
            }
            println!(
                "connected: {:?} after {} ms",
                connected_browser(),
                waited * 500
            );
            assert!(
                connected_browser().is_some(),
                "the extension never connected"
            );

            let call = |op: Value| async move {
                let started = std::time::Instant::now();
                let what = op["op"].as_str().unwrap_or("").to_string();
                let reply = request(op, Duration::from_secs(40)).await;
                println!("  {what}: {} ms", started.elapsed().as_millis());
                reply
            };
            let index_of = |read: &Value, needle: &str| -> Option<u64> {
                read["content"].as_str()?.lines().find_map(|line| {
                    if !line.contains(needle) {
                        return None;
                    }
                    let start = line.find('[')? + 1;
                    let end = line[start..].find(']')? + start;
                    line[start..end].parse().ok()
                })
            };
            let tokens = |read: &Value| read["content"].as_str().map(|c| c.len() / 4).unwrap_or(0);

            println!("== iana");
            let tab =
                call(json!({"op": "open", "url": "https://www.iana.org/help/example-domains"}))
                    .await
                    .expect("open")["tab"]
                    .clone();
            let read = call(json!({"op": "read", "tab": tab})).await.expect("read");
            let link = index_of(&read, "Reserved Domains").expect("link");
            println!(
                "  click: {:?}",
                call(json!({"op": "click", "tab": tab, "index": link})).await
            );
            let read = call(json!({"op": "read", "tab": tab, "view": "visible"}))
                .await
                .expect("read");
            println!("  RESULT now at {}", read["url"]);

            println!("== wikipedia");
            let tab = call(json!({"op": "open", "url": "https://en.wikipedia.org/wiki/Main_Page"}))
                .await
                .expect("open")["tab"]
                .clone();
            let read = call(json!({"op": "read", "tab": tab, "view": "visible"}))
                .await
                .expect("read");
            println!("  visible read ~{} tokens", tokens(&read));
            let read = call(json!({"op": "read", "tab": tab})).await.expect("read");
            println!("  full read ~{} tokens", tokens(&read));
            let _ = std::fs::write(
                root.join("wiki-full.txt"),
                read["content"].as_str().unwrap_or(""),
            );
            // A narrow window shows a Search button that opens the box.
            let read = match index_of(&read, "type=search") {
                Some(_) => read,
                None => {
                    let opener = index_of(&read, "title=Search Wikipedia").expect("search button");
                    println!(
                        "  open search: {:?}",
                        call(json!({"op": "click", "tab": tab, "index": opener})).await
                    );
                    call(json!({"op": "read", "tab": tab})).await.expect("read")
                }
            };
            let search = index_of(&read, "type=search").expect("search box");
            println!(
                "  type: {:?}",
                call(json!({"op": "type", "tab": tab, "index": search, "text": "Pune", "submit": true})).await
            );
            let read = call(json!({"op": "read", "tab": tab, "view": "visible"}))
                .await
                .expect("read");
            println!("  RESULT title {}", read["title"]);

            println!("== bookmyshow");
            let date = chrono::Local::now().format("%Y%m%d");
            let tab = call(json!({"op": "open", "url": format!("https://in.bookmyshow.com/cinemas/mumbai/cinepolis-lake-shore-thane-ex-viviana-mall/buytickets/CPVM/{date}")}))
                .await
                .expect("open")["tab"]
                .clone();
            tokio::time::sleep(Duration::from_secs(4)).await;
            let read = call(json!({"op": "read", "tab": tab})).await.expect("read");
            println!("  title {} ~{} tokens", read["title"], tokens(&read));
            if let Some(show) = index_of(&read, "aria-label=Book ") {
                println!(
                    "  click show: {:?}",
                    call(json!({"op": "click", "tab": tab, "index": show})).await
                );
                for wait in [4, 8, 12] {
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    let read = call(json!({"op": "read", "tab": tab, "view": "visible"}))
                        .await
                        .expect("read");
                    let content = read["content"].as_str().unwrap_or("");
                    println!(
                        "  after {wait}s: {} ~{} tokens, waiting={}",
                        read["url"],
                        tokens(&read),
                        content.contains("Please wait")
                    );
                    if !content.contains("Please wait") {
                        println!("{}", content.chars().take(1500).collect::<String>());
                        break;
                    }
                }
            } else {
                println!("  no show button found");
            }
        });
    }
}
