//! [GRAIN] Grain Pilot: the Agent acting in the user's own browser, signed in
//! as them, through the Grain Pilot browser extension
//! (`docs/Pilot/EXTENSION-PLAN.md`).
//!
//! The pure half (tools, page reads, policy, replies) lives in
//! `grain_core::pilot`. This file holds Pilot's tabs, runs each tool as one
//! or more extension operations over `grain_browser_link`, and enforces the
//! payment and secret-field rules before anything reaches the page.
//!
//! - Control numbers are stable per tab (the page script numbers elements and
//!   never reuses a number), so a number from any earlier read is either the
//!   same control or refused as gone.
//! - After an action Pilot reads what is on screen again and reports only
//!   what changed.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::Duration;

use grain_core::capability_index::ExtensionDirectoryEntry;
use grain_core::execution::{ActionOutcome, FailureClass, SuccessData};
use grain_core::pilot::page::{self, Control, Read, View};
use grain_core::pilot::render;
use grain_core::pilot::tools::{self, Call, Handle, Step};
use grain_core::pilot::{self as core, policy};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::grain_browser_link as link;

pub(crate) const EXTENSION_ID: &str = core::EXTENSION_ID;

/// One extension operation: a page load or an action and its settle wait.
const OP_TIMEOUT: Duration = Duration::from_secs(40);
/// How long a call waits for the extension to connect after the link starts
/// (the extension retries every minute, and at browser start-up).
const CONNECT_WAIT: Duration = Duration::from_secs(3);
/// Tabs one Agent run may keep open through Pilot.
const MAX_TABS: usize = 8;
/// Extra reads while a page only shows a loading placeholder.
const LOADING_RETRIES: usize = 4;
/// Pause between those reads.
const LOADING_PAUSE: Duration = Duration::from_millis(1500);

static STATE: tokio::sync::Mutex<State> = tokio::sync::Mutex::const_new(State::new());

struct State {
    tabs: BTreeMap<u32, PilotTab>,
    next_handle: u32,
    last: Option<u32>,
}

impl State {
    const fn new() -> Self {
        Self {
            tabs: BTreeMap::new(),
            next_handle: 1,
            last: None,
        }
    }
}

/// A tab the model can name.
struct PilotTab {
    /// The browser's tab id.
    tab_id: i64,
    /// Every control seen in this tab, by its stable number.
    known: HashMap<u32, Control>,
    /// The latest on-screen read: the "before" of the next change report.
    screen: Option<Read>,
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
    fn not_found(message: impl Into<String>) -> Self {
        Self::new(FailureClass::NotFound, message)
    }
    fn refused(message: impl Into<String>) -> Self {
        Self::new(FailureClass::Cancelled, message)
    }
}

// ── Extension surface ──────────────────────────────────────────────────────

pub(crate) fn is_enabled(app: &AppHandle) -> bool {
    crate::settings::get_settings(app).pilot_enabled
}

/// [GRAIN] Turn Grain Pilot on or off. Off removes its tools from the Agent on
/// the next model round and closes the browser link.
#[tauri::command]
#[specta::specta]
pub fn change_pilot_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.pilot_enabled = enabled;
    crate::settings::write_settings(&app, settings);
    if !enabled {
        stop_in_background();
    }
    Ok(())
}

fn host_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("pilot"))
        .map_err(|error| format!("Could not find Grain's data folder: {error}"))
}

/// Connect: register the native messaging host with the browsers and open
/// the link. Returns the connected browser, or a note that none is yet.
pub(crate) async fn prepare(app: &AppHandle) -> Result<String, String> {
    let host = link::host_path().ok_or("Could not locate Grain's browser host.")?;
    link::register(&host, &host_dir(app)?)?;
    link::ensure_started()?;
    Ok(link::connected_browser().unwrap_or_else(|| "waiting for the browser extension".into()))
}

/// Close the link and forget Pilot's tabs, off the caller's thread.
pub(crate) fn stop_in_background() {
    link::stop();
    tauri::async_runtime::spawn(async {
        *STATE.lock().await = State::new();
    });
}

/// Close the link at app exit.
pub(crate) fn stop_at_exit() {
    link::stop();
}

/// Disconnect: close the link and remove the host registration.
pub(crate) fn disconnect_in_background(app: &AppHandle) {
    stop_in_background();
    if let Ok(dir) = host_dir(app) {
        link::unregister(&dir);
    }
}

/// The Agent directory entry, when the user has turned Pilot on.
pub(crate) fn directory(app: &AppHandle) -> Option<ExtensionDirectoryEntry> {
    is_enabled(app).then(|| ExtensionDirectoryEntry {
        extension_id: EXTENSION_ID.to_string(),
        name: core::PROVIDER_NAME.to_string(),
        description: core::DESCRIPTION.to_string(),
        action_count: tools::tools().len(),
    })
}

/// The compiled tool set and its digest. No I/O.
pub(crate) fn catalog() -> (String, Vec<crate::grain_compiled_tools::CatalogTool>) {
    let tools = tools::tools()
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

/// Run one Pilot action. Never panics into the Agent loop: every failure is
/// a typed outcome the model can read and act on.
pub(crate) async fn call(
    app: &AppHandle,
    action_id: &str,
    arguments: &serde_json::Value,
) -> ActionOutcome {
    if !is_enabled(app) {
        return failed(FailureClass::Cancelled, "Grain Pilot is turned off.");
    }
    if !cfg!(windows) {
        return failed(
            FailureClass::NotFound,
            "Grain Pilot is not available on this OS yet.",
        );
    }
    let parsed = match tools::parse(action_id, arguments) {
        Ok(call) => call,
        Err(message) => return failed(FailureClass::InvalidArgument, &message),
    };
    if let Err(message) = ensure_link(app).await {
        return failed(FailureClass::NotFound, &message);
    }
    let title = tools::tools()
        .into_iter()
        .find(|tool| tool.name == action_id)
        .map(|tool| tool.title.to_string());
    // Arguments can be private text; log the shape only.
    log::info!("[GRAIN] pilot: {}", parsed.describe_for_log());
    let read_only = parsed.read_only();
    let result = run(&mut *STATE.lock().await, &parsed).await;
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
            log::info!("[GRAIN] pilot: {action_id} failed: {}", failure.message);
            failed(failure.class, &failure.message)
        }
    }
}

fn failed(class: FailureClass, message: &str) -> ActionOutcome {
    ActionOutcome::Failed {
        class,
        message: message.to_string(),
    }
}

/// Make sure the link runs and the extension is connected, waiting briefly
/// for it after a fresh start.
async fn ensure_link(app: &AppHandle) -> Result<(), String> {
    if link::connected_browser().is_some() {
        return Ok(());
    }
    prepare(app).await?;
    let deadline = tokio::time::Instant::now() + CONNECT_WAIT;
    while link::connected_browser().is_none() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    if link::connected_browser().is_none() {
        return Err("The Grain Pilot browser extension is not connected. Ask the user to open their browser with the Grain Pilot extension turned on (it connects within a minute), then try again.".into());
    }
    Ok(())
}

// ── Running a call ─────────────────────────────────────────────────────────

/// One extension operation.
async fn ext(op: Value) -> Result<Value, Failure> {
    link::request(op, OP_TIMEOUT)
        .await
        .map_err(|message| Failure::new(FailureClass::Internal, message))
}

/// An action's `{ok, message}` reply as a result.
fn outcome(reply: Value) -> Result<String, Failure> {
    let message = reply
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Done.")
        .chars()
        .take(300)
        .collect::<String>();
    if reply.get("ok").and_then(Value::as_bool) == Some(false) {
        Err(Failure::refused(message))
    } else {
        Ok(message)
    }
}

fn pick(state: &State, tab: Option<Handle>) -> Result<u32, Failure> {
    let handle = match tab {
        Some(handle) => handle.0,
        None => state
            .last
            .ok_or_else(|| Failure::not_found("No tab yet. Use open first."))?,
    };
    if state.tabs.contains_key(&handle) {
        Ok(handle)
    } else {
        Err(Failure::not_found(format!(
            "There is no {}. Use tabs to list Pilot's tabs.",
            Handle(handle)
        )))
    }
}

fn tab_mut(state: &mut State, handle: u32) -> Result<&mut PilotTab, Failure> {
    state
        .tabs
        .get_mut(&handle)
        .ok_or_else(|| Failure::not_found(format!("There is no {}.", Handle(handle))))
}

/// Read a tab and remember its controls (and, for the on-screen view, the
/// baseline of the next change report).
async fn read(state: &mut State, handle: u32, view: &View) -> Result<Read, Failure> {
    let tab_id = tab_mut(state, handle)?.tab_id;
    let reply = ext(json!({"op": "read", "tab": tab_id, "view": view.extension_view()})).await?;
    let read = page::parse(&reply).ok_or_else(|| {
        Failure::new(
            FailureClass::Internal,
            "The extension gave an unreadable page.",
        )
    })?;
    let tab = tab_mut(state, handle)?;
    for control in &read.controls {
        tab.known.insert(control.number, control.clone());
    }
    if *view == View::Visible {
        tab.screen = Some(read.clone());
    }
    state.last = Some(handle);
    Ok(read)
}

/// The control behind a number, reading the whole page once if this tab has
/// not shown it yet.
async fn control(state: &mut State, handle: u32, number: u32) -> Result<Control, Failure> {
    if let Some(control) = tab_mut(state, handle)?.known.get(&number) {
        return Ok(control.clone());
    }
    read(state, handle, &View::Page).await?;
    tab_mut(state, handle)?
        .known
        .get(&number)
        .cloned()
        .ok_or_else(|| {
            Failure::not_found(format!(
                "There is no [{number}] in {}. Read it again.",
                Handle(handle)
            ))
        })
}

/// Refuse a click that commits money and typing into a secret field.
async fn check(state: &mut State, handle: u32, step: &Step) -> Result<(), Failure> {
    match step {
        Step::Click(number) => {
            let control = control(state, handle, *number).await?;
            if policy::is_payment_control(&control.name) {
                return Err(Failure::refused(render::payment_refusal(&control.name)));
            }
        }
        Step::Type(number, _) => {
            if control(state, handle, *number).await?.secret {
                return Err(Failure::refused(render::secret_refusal()));
            }
        }
        Step::Press(_) => {}
    }
    Ok(())
}

async fn perform(state: &mut State, handle: u32, step: &Step) -> Result<String, Failure> {
    check(state, handle, step).await?;
    let tab_id = tab_mut(state, handle)?.tab_id;
    let op = match step {
        Step::Click(number) => json!({"op": "click", "tab": tab_id, "index": number}),
        Step::Type(number, text) => {
            json!({"op": "type", "tab": tab_id, "index": number, "text": text, "submit": false})
        }
        Step::Press(key) => json!({"op": "press", "tab": tab_id, "key": key}),
    };
    outcome(ext(op).await?)
}

/// The on-screen read before an action, for the change report.
async fn baseline(state: &mut State, handle: u32) -> Result<Read, Failure> {
    match tab_mut(state, handle)?.screen.clone() {
        Some(read) => Ok(read),
        None => read(state, handle, &View::Visible).await,
    }
}

/// Read the screen, reading again while it only shows a loading placeholder
/// ("Please wait…"), up to [`LOADING_RETRIES`] times.
async fn settled_read(state: &mut State, handle: u32) -> Result<Read, Failure> {
    let mut read_now = read(state, handle, &View::Visible).await?;
    for _ in 0..LOADING_RETRIES {
        if !page::looks_loading(&read_now) {
            break;
        }
        tokio::time::sleep(LOADING_PAUSE).await;
        read_now = read(state, handle, &View::Visible).await?;
    }
    Ok(read_now)
}

/// Read the screen after an action and report what changed.
async fn report(
    state: &mut State,
    handle: u32,
    before: &Read,
    done: &str,
) -> Result<String, Failure> {
    let after = settled_read(state, handle).await?;
    Ok(render::action_reply(
        Handle(handle),
        done,
        &page::changes(before, &after),
    ))
}

async fn run(state: &mut State, call: &Call) -> Result<String, Failure> {
    match call {
        Call::Open(url) => {
            if state.tabs.len() >= MAX_TABS {
                return Err(Failure::refused(format!(
                    "Grain Pilot already has {MAX_TABS} tabs open. Close one first."
                )));
            }
            let reply = ext(json!({"op": "open", "url": url})).await?;
            let tab_id = reply.get("tab").and_then(Value::as_i64).ok_or_else(|| {
                Failure::new(FailureClass::Internal, "The extension opened no tab.")
            })?;
            let handle = state.next_handle;
            state.next_handle += 1;
            state.tabs.insert(
                handle,
                PilotTab {
                    tab_id,
                    known: HashMap::new(),
                    screen: None,
                },
            );
            state.last = Some(handle);
            let read = settled_read(state, handle).await?;
            Ok(render::opened_reply(Handle(handle), &read))
        }
        Call::Tabs => {
            let reply = ext(json!({"op": "tabs"})).await?;
            let open: HashMap<i64, String> = reply
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tab| {
                    Some((
                        tab.get("tab")?.as_i64()?,
                        tab.get("title")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ))
                })
                .collect();
            // Tabs the user closed are forgotten.
            state.tabs.retain(|_, tab| open.contains_key(&tab.tab_id));
            let rows: Vec<(Handle, String)> = state
                .tabs
                .iter()
                .map(|(handle, tab)| (Handle(*handle), open[&tab.tab_id].clone()))
                .collect();
            Ok(render::tabs_reply(&rows))
        }
        Call::Read { tab, view } => {
            let handle = pick(state, *tab)?;
            let read = read(state, handle, view).await?;
            Ok(render::read_reply(Handle(handle), &read, view))
        }
        Call::Click { tab, control } => {
            let handle = pick(state, *tab)?;
            let before = baseline(state, handle).await?;
            let done = perform(state, handle, &Step::Click(*control)).await?;
            report(state, handle, &before, &done).await
        }
        Call::Type {
            tab,
            control,
            text,
            submit,
        } => {
            let handle = pick(state, *tab)?;
            check(state, handle, &Step::Type(*control, text.clone())).await?;
            let before = baseline(state, handle).await?;
            let tab_id = tab_mut(state, handle)?.tab_id;
            let done = outcome(
                ext(json!({"op": "type", "tab": tab_id, "index": control, "text": text, "submit": submit}))
                    .await?,
            )?;
            report(state, handle, &before, &done).await
        }
        Call::Choose {
            tab,
            control,
            option,
        } => {
            let handle = pick(state, *tab)?;
            let before = baseline(state, handle).await?;
            let tab_id = tab_mut(state, handle)?.tab_id;
            let done = outcome(
                ext(json!({"op": "select", "tab": tab_id, "index": control, "option": option}))
                    .await?,
            )?;
            report(state, handle, &before, &done).await
        }
        Call::Press { tab, key } => {
            let handle = pick(state, *tab)?;
            let before = baseline(state, handle).await?;
            let done = perform(state, handle, &Step::Press(key)).await?;
            report(state, handle, &before, &done).await
        }
        Call::Scroll {
            tab,
            direction,
            amount,
        } => {
            let handle = pick(state, *tab)?;
            let tab_id = tab_mut(state, handle)?.tab_id;
            outcome(
                ext(json!({
                    "op": "scroll",
                    "tab": tab_id,
                    "direction": direction,
                    "pixels": amount * core::SCROLL_NOTCH_PIXELS,
                }))
                .await?,
            )?;
            let read = read(state, handle, &View::Visible).await?;
            Ok(render::read_reply(Handle(handle), &read, &View::Visible))
        }
        Call::Steps { tab, steps } => {
            let handle = pick(state, *tab)?;
            let before = baseline(state, handle).await?;
            let mut done = Vec::new();
            for (at, step) in steps.iter().enumerate() {
                match perform(state, handle, step).await {
                    Ok(message) => done.push(message),
                    Err(failure) => {
                        let so_far = if done.is_empty() {
                            String::new()
                        } else {
                            format!(" Done before it: {}", done.join(" "))
                        };
                        let changes = report(state, handle, &before, "Stopped")
                            .await
                            .unwrap_or_default();
                        return Err(Failure::new(
                            failure.class,
                            format!(
                                "Step {} failed: {}.{so_far}\n{changes}",
                                at + 1,
                                failure.message
                            ),
                        ));
                    }
                }
            }
            report(state, handle, &before, &done.join(" ")).await
        }
        Call::Show {
            tab,
            reason,
            message,
        } => {
            let handle = pick(state, *tab)?;
            let tab_id = tab_mut(state, handle)?.tab_id;
            ext(json!({"op": "show", "tab": tab_id})).await?;
            Ok(render::show_reply(
                Handle(handle),
                *reason,
                message.as_deref(),
            ))
        }
        Call::Close { tab } => {
            let handle = pick(state, *tab)?;
            let tab_id = tab_mut(state, handle)?.tab_id;
            ext(json!({"op": "close", "tab": tab_id})).await?;
            state.tabs.remove(&handle);
            if state.last == Some(handle) {
                state.last = None;
            }
            Ok(format!("Closed {}.", Handle(handle)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live: the Agent's Pilot tools end to end through the real extension,
    /// in a SEPARATE Edge profile (fresh each run) with the unpacked
    /// extension. Needs `browser-extension/dist` built and
    /// `GRAIN_BROWSER_HOST` pointing at a built host. Ends only the Edge
    /// processes started with this test's profile folder.
    ///
    /// `cargo test --lib live_pilot_tools -- --ignored --nocapture --test-threads 1`
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn live_pilot_tools_book_up_to_the_seat_page() {
        let host = link::host_path().expect("host");
        let root = std::env::temp_dir().join("grain-pilot-tools-live");
        let profile = root.join("edge-profile");
        let extension = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("browser-extension")
            .join("dist");
        assert!(
            extension.join("page.js").is_file(),
            "build browser-extension first"
        );
        let _ = std::fs::remove_dir_all(&profile);
        link::register(&host, &root).expect("register");

        struct Cleanup(PathBuf, PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let script = format!(
                    "Get-CimInstance Win32_Process -Filter \"Name='msedge.exe'\" | Where-Object {{ $_.CommandLine -like '*{}*' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}",
                    self.1.display()
                );
                let _ = std::process::Command::new("powershell")
                    .args(["-NoProfile", "-Command", &script])
                    .status();
                link::stop();
                link::unregister(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone(), profile.clone());

        tauri::async_runtime::block_on(async {
            link::ensure_started().expect("start");
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
            for _ in 0..60 {
                if link::connected_browser().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            assert!(
                link::connected_browser().is_some(),
                "the extension never connected"
            );

            let mut state = State::new();
            let step = |name: &'static str, args: Value| {
                let call = tools::parse(name, &args).unwrap_or_else(|e| panic!("{name}: {e}"));
                (name, call)
            };
            async fn go(state: &mut State, (name, call): (&'static str, Call)) -> String {
                let started = std::time::Instant::now();
                let reply = match run(state, &call).await {
                    Ok(text) => text,
                    Err(failure) => format!("FAILED: {}", failure.message),
                };
                println!(
                    "--- {name} ({} ms, ~{} tokens)\n{}\n",
                    started.elapsed().as_millis(),
                    reply.len() / 4,
                    reply.chars().take(1200).collect::<String>()
                );
                reply
            }
            let number_in = |reply: &str, needle: &str| -> u32 {
                reply
                    .lines()
                    .find(|line| line.contains(needle) && line.trim_start().starts_with('['))
                    .and_then(|line| {
                        let line = line.trim_start();
                        line[1..line.find(']')?].parse().ok()
                    })
                    .unwrap_or_else(|| panic!("no row with {needle}"))
            };

            let opened = go(
                &mut state,
                step(
                    "open",
                    json!({"url": "https://en.wikipedia.org/wiki/Main_Page"}),
                ),
            )
            .await;
            assert!(opened.starts_with("Opened t1"), "{opened}");
            let found = go(&mut state, step("read", json!({"find": "search"}))).await;
            let field = if found.contains("search field") {
                number_in(&found, "search field")
            } else {
                let opener = number_in(&found, "Search Wikipedia");
                go(&mut state, step("click", json!({"control": opener}))).await;
                let found = go(&mut state, step("read", json!({"find": "search"}))).await;
                if found.contains("search field") {
                    number_in(&found, "search field")
                } else {
                    number_in(&found, "combobox")
                }
            };
            let typed = go(
                &mut state,
                step(
                    "type",
                    json!({"control": field, "text": "Pune", "submit": true}),
                ),
            )
            .await;
            assert!(typed.contains("Pune"), "{typed}");

            let date = chrono::Local::now().format("%Y%m%d");
            let url = format!("https://in.bookmyshow.com/cinemas/mumbai/cinepolis-lake-shore-thane-ex-viviana-mall/buytickets/CPVM/{date}");
            go(&mut state, step("open", json!({"url": url}))).await;
            let shows = go(&mut state, step("read", json!({"find": "Book"}))).await;
            let show = number_in(&shows, "\"Book ");
            let clicked = go(&mut state, step("click", json!({"control": show}))).await;
            println!(
                "seat page reached: {}",
                clicked.contains("Select Seats") || clicked.contains("seats")
            );
            let tabs = go(&mut state, step("tabs", json!({}))).await;
            assert!(tabs.contains("t1") && tabs.contains("t2"), "{tabs}");
            go(&mut state, step("close", json!({"tab": "t2"}))).await;
        });
    }
}
