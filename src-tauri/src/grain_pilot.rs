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
/// How long a call waits for the extension to connect.
const CONNECT_WAIT: Duration = Duration::from_secs(3);
/// After Grain starts the link, an already-open browser connects on the
/// extension's next retry (every 30 s): calls wait out that first window.
const FIRST_CONNECT_WINDOW: Duration = Duration::from_secs(35);
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

/// Whether a model-facing tool name (as the catalogue names it) is one of
/// Pilot's. Built once; the names only change with a new build.
pub(crate) fn is_pilot_tool(name: &str) -> bool {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    NAMES
        .get_or_init(|| {
            tools::tools()
                .iter()
                .map(|tool| {
                    grain_core::capability_agent::tool_name(&format!(
                        "{EXTENSION_ID}:{}",
                        tool.name
                    ))
                })
                .collect()
        })
        .iter()
        .any(|known| known == name)
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
    let mut deadline = tokio::time::Instant::now() + CONNECT_WAIT;
    if let Some(started) = link::started_at() {
        deadline = deadline.max((started + FIRST_CONNECT_WINDOW).into());
    }
    while link::connected_browser().is_none() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    if link::connected_browser().is_none() {
        return Err("The Grain Pilot browser extension is not connected. Ask the user to open their browser with the Grain Pilot extension turned on (it connects within 30 seconds), then try again.".into());
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

/// Refuse a click that commits money and filling a secret field.
async fn check(state: &mut State, handle: u32, step: &Step) -> Result<(), Failure> {
    match step {
        Step::Click(number) => {
            let control = control(state, handle, *number).await?;
            if policy::is_payment_control(&control.name) {
                return Err(Failure::refused(render::payment_refusal(&control.name)));
            }
        }
        Step::Fill {
            control: number, ..
        } => {
            if control(state, handle, *number).await?.secret {
                return Err(Failure::refused(render::secret_refusal()));
            }
        }
        Step::Choose {
            control: number, ..
        } => {
            control(state, handle, *number).await?;
        }
        Step::Key(_) | Step::Scroll(_) => {}
    }
    Ok(())
}

/// What one `act` step did, and whether it left the page.
struct Performed {
    message: String,
    left: bool,
}

/// Run one `act` step in the page.
async fn perform(state: &mut State, handle: u32, step: &Step) -> Result<Performed, Failure> {
    check(state, handle, step).await?;
    let tab_id = tab_mut(state, handle)?.tab_id;
    let op = match step {
        Step::Click(number) => json!({"op": "click", "tab": tab_id, "index": number}),
        Step::Fill {
            control,
            text,
            enter,
        } => json!({"op": "type", "tab": tab_id, "index": control, "text": text, "submit": enter}),
        Step::Choose { control, option } => {
            json!({"op": "select", "tab": tab_id, "index": control, "option": option})
        }
        Step::Key(key) => json!({"op": "press", "tab": tab_id, "key": key}),
        Step::Scroll(direction) => json!({
            "op": "scroll",
            "tab": tab_id,
            "direction": direction,
            "pixels": core::SCROLL_PIXELS,
        }),
    };
    let reply = ext(op).await?;
    let left = reply.get("left").and_then(Value::as_bool) == Some(true);
    let message = outcome(reply)?;
    Ok(if left {
        Performed {
            message: format!("{} The page changed.", step.done()),
            left,
        }
    } else {
        Performed { message, left }
    })
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

/// A reply whose page still only shows a loading placeholder after the
/// retries says what to do: background tabs run a site's own loaders slowly.
fn still_loading(reply: String, read: &Read) -> String {
    if page::looks_loading(read) {
        format!(
            "{reply}
(Still loading. A background tab can load slowly: read again, or call show to bring it forward.)"
        )
    } else {
        reply
    }
}

/// Read the screen after an action and report what changed.
async fn report(
    state: &mut State,
    handle: u32,
    before: &Read,
    done: &str,
) -> Result<String, Failure> {
    let after = settled_read(state, handle).await?;
    Ok(still_loading(
        render::action_reply(Handle(handle), done, before, &after),
        &after,
    ))
}

/// Drop tabs the user closed (or a browser restart ended) and return the
/// open ones' titles by tab id.
async fn forget_closed_tabs(state: &mut State) -> Result<HashMap<i64, String>, Failure> {
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
    state.tabs.retain(|_, tab| open.contains_key(&tab.tab_id));
    if state
        .last
        .is_some_and(|last| !state.tabs.contains_key(&last))
    {
        state.last = None;
    }
    Ok(open)
}

async fn run(state: &mut State, call: &Call) -> Result<String, Failure> {
    match call {
        Call::Open(url) => {
            if state.tabs.len() >= MAX_TABS {
                // Tabs the user closed no longer count.
                forget_closed_tabs(state).await?;
            }
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
            Ok(still_loading(
                render::opened_reply(Handle(handle), &read),
                &read,
            ))
        }
        Call::Read { tab, view } => {
            let handle = pick(state, *tab)?;
            let read = read(state, handle, view).await?;
            Ok(render::read_reply(Handle(handle), &read, view))
        }
        Call::Act { tab, steps } => {
            let handle = pick(state, *tab)?;
            let before = baseline(state, handle).await?;
            let mut done = Vec::new();
            for (at, step) in steps.iter().enumerate() {
                match perform(state, handle, step).await {
                    Ok(Performed { message, left }) => {
                        done.push(message);
                        // Later numbers belonged to the old page.
                        if left && at + 1 < steps.len() {
                            let rest = if at + 2 == steps.len() {
                                format!("Step {}", at + 2)
                            } else {
                                format!("Steps {}-{}", at + 2, steps.len())
                            };
                            done.push(format!("{rest} did not run; read the new page first."));
                            break;
                        }
                    }
                    Err(failure) => {
                        let so_far = if done.is_empty() {
                            String::new()
                        } else {
                            format!(" Done before it: {}", done.join(" "))
                        };
                        let changes = report(state, handle, &before, "").await.unwrap_or_default();
                        return Err(Failure::new(
                            failure.class,
                            format!(
                                "Step {} failed: {}.{so_far}{changes}",
                                at + 1,
                                failure.message.trim_end_matches('.')
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
        Call::Tabs { close } => {
            let mut closed = String::new();
            if let Some(close) = close {
                let handle = pick(state, Some(*close))?;
                let tab_id = tab_mut(state, handle)?.tab_id;
                ext(json!({"op": "close", "tab": tab_id})).await?;
                state.tabs.remove(&handle);
                if state.last == Some(handle) {
                    state.last = None;
                }
                closed = format!("Closed {close}.\n");
            }
            let open = forget_closed_tabs(state).await?;
            let rows: Vec<(Handle, String)> = state
                .tabs
                .iter()
                .map(|(handle, tab)| (Handle(*handle), open[&tab.tab_id].clone()))
                .collect();
            Ok(format!("{closed}{}", render::tabs_reply(&rows)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pilot_tool_names_are_recognised_exactly() {
        let (_, catalog) = catalog();
        assert_eq!(catalog.len(), tools::tools().len());
        for tool in &catalog {
            let name = grain_core::capability_agent::tool_name(&tool.action.canonical_id);
            assert!(is_pilot_tool(&name), "{name}");
        }
        assert!(!is_pilot_tool("act"));
        assert!(!is_pilot_tool(&grain_core::capability_agent::tool_name(
            "grain.recall:search"
        )));
    }

    // ── Live tests ─────────────────────────────────────────────────────────
    //
    // All run in a SEPARATE browser profile (fresh each run; Edge, or the
    // Chromium browser at `PILOT_BROWSER`) with the unpacked
    // extension. They need `browser-extension/dist` built and
    // `GRAIN_BROWSER_HOST` pointing at a built host, and end only the browser
    // processes started with their own profile folder.

    /// A fresh Edge profile with the unpacked extension. Dropping it ends only
    /// the Edge processes using that profile and unregisters the host.
    struct LiveEdge {
        root: PathBuf,
        profile: PathBuf,
    }

    /// The browser the live tests drive: `PILOT_BROWSER`, or Edge.
    fn browser() -> PathBuf {
        std::env::var_os("PILOT_BROWSER")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")
            })
    }

    impl LiveEdge {
        fn start(name: &str) -> Self {
            let host = link::host_path().expect("host");
            let root = std::env::temp_dir().join(name);
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
            let edge = LiveEdge { root, profile };
            tauri::async_runtime::block_on(async {
                link::ensure_started().expect("start");
            });
            std::process::Command::new(browser())
                .arg(format!("--user-data-dir={}", edge.profile.display()))
                .arg(format!("--load-extension={}", extension.display()))
                .args([
                    "--no-first-run",
                    "--no-default-browser-check",
                    "about:blank",
                ])
                .spawn()
                .expect("launch Edge");
            edge
        }
    }

    impl Drop for LiveEdge {
        fn drop(&mut self) {
            let script = format!(
                "Get-CimInstance Win32_Process -Filter \"Name='{}'\" | Where-Object {{ $_.CommandLine -like '*{}*' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}",
                browser().file_name().and_then(|n| n.to_str()).unwrap_or("msedge.exe"),
                self.profile.display()
            );
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .status();
            link::stop();
            link::unregister(&self.root);
        }
    }

    async fn connected() {
        for _ in 0..60 {
            if link::connected_browser().is_some() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        panic!("the extension never connected");
    }

    fn step(name: &'static str, args: Value) -> (&'static str, Call) {
        let call = tools::parse(name, &args).unwrap_or_else(|e| panic!("{name}: {e}"));
        (name, call)
    }

    /// Run one tool call and print its reply with its size.
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
            reply.chars().take(1500).collect::<String>()
        );
        reply
    }

    /// The control number of the first row containing `needle`.
    fn number_of(reply: &str, needle: &str) -> Option<u32> {
        reply
            .lines()
            .find(|line| line.contains(needle) && line.starts_with('['))
            .and_then(|line| line[1..line.find(']')?].parse().ok())
    }

    fn number_in(reply: &str, needle: &str) -> u32 {
        number_of(reply, needle).unwrap_or_else(|| panic!("no row with {needle}"))
    }

    /// Live: the Agent's Pilot tools end to end on Wikipedia and BookMyShow.
    ///
    /// `cargo test --lib live_pilot_tools -- --ignored --nocapture --test-threads 1`
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn live_pilot_tools_book_up_to_the_seat_page() {
        let _edge = LiveEdge::start("grain-pilot-tools-live");
        tauri::async_runtime::block_on(async {
            connected().await;
            let mut state = State::new();
            let opened = go(
                &mut state,
                step(
                    "open",
                    json!({"url": "https://en.wikipedia.org/wiki/Main_Page"}),
                ),
            )
            .await;
            assert!(opened.starts_with("Opened.\nt1 · "), "{opened}");
            let found = go(&mut state, step("read", json!({"find": "search"}))).await;
            let field = match number_of(&found, "] field") {
                Some(field) => field,
                None => {
                    // A narrow window hides the search box behind a button.
                    let opener = number_in(&found, "Search Wikipedia");
                    go(&mut state, step("act", json!({"do": [{"click": opener}]}))).await;
                    let found = go(&mut state, step("read", json!({"find": "search"}))).await;
                    number_in(&found, "] field")
                }
            };
            let typed = go(
                &mut state,
                step(
                    "act",
                    json!({"do": [{"fill": field, "text": "Pune", "enter": true}]}),
                ),
            )
            .await;
            assert!(typed.contains("Pune"), "{typed}");

            let date = chrono::Local::now().format("%Y%m%d");
            let url = format!("https://in.bookmyshow.com/cinemas/mumbai/cinepolis-lake-shore-thane-ex-viviana-mall/buytickets/CPVM/{date}");
            go(&mut state, step("open", json!({"url": url}))).await;
            let shows = go(&mut state, step("read", json!({"find": "Book"}))).await;
            // The day's last show: earlier ones may have started already.
            let show = shows
                .lines()
                .filter(|row| row.contains("] Book "))
                .last()
                .and_then(|row| row[1..row.find(']')?].parse::<u32>().ok())
                .expect("a Book row");
            let clicked = go(&mut state, step("act", json!({"do": [{"click": show}]}))).await;
            println!(
                "seat page reached: {}",
                clicked.contains("Select Seats") || clicked.contains("seats")
            );
            go(&mut state, step("act", json!({"do": [{"scroll": "down"}]}))).await;
            let tabs = go(&mut state, step("tabs", json!({}))).await;
            assert!(tabs.contains("t1") && tabs.contains("t2"), "{tabs}");
            let closed = go(&mut state, step("tabs", json!({"close": "t2"}))).await;
            assert!(
                closed.starts_with("Closed t2.") && !closed.contains("t2:"),
                "{closed}"
            );
        });
    }

    /// Live: every `act` verb and both refusals on forms, a search box and
    /// a list, on public pages with nothing signed in.
    ///
    /// `cargo test --lib live_pilot_acts -- --ignored --nocapture --test-threads 1`
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn live_pilot_acts_on_many_sites() {
        let _edge = LiveEdge::start("grain-pilot-acts-live");
        tauri::async_runtime::block_on(async {
            connected().await;
            let mut state = State::new();
            let refused = "never types there";

            // A form with every kind of control.
            let url = "https://www.selenium.dev/selenium/web/web-form.html";
            go(&mut state, step("open", json!({ "url": url }))).await;
            let form = go(&mut state, step("read", json!({"page": true}))).await;
            let text = number_in(&form, "] field Text input");
            let password = number_in(&form, "] field Password");
            let dropdown = number_in(&form, "] dropdown Dropdown (select)");
            let checkbox = number_in(&form, "] checkbox Default checkbox");
            let radio = number_in(&form, "] radio Default radio");
            let submit = number_in(&form, "] Submit");
            let secret = go(
                &mut state,
                step("act", json!({"do": [{"fill": password, "text": "x"}]})),
            )
            .await;
            assert!(secret.contains(refused), "{secret}");
            let filled = go(
                &mut state,
                step(
                    "act",
                    json!({"do": [
                        {"fill": text, "text": "hello pilot"},
                        {"choose": dropdown, "option": "Two"},
                        {"click": checkbox},
                        {"click": radio}
                    ]}),
                ),
            )
            .await;
            assert!(!filled.starts_with("FAILED"), "{filled}");
            let after = go(&mut state, step("read", json!({"page": true}))).await;
            assert!(after.contains("hello pilot"), "typed text shows:\n{after}");
            assert!(
                after.contains("Default checkbox checked"),
                "the click checked it:\n{after}"
            );
            assert!(after.contains("value=Two"), "the choice shows:\n{after}");
            // A step after one that leaves the page must not run on the new one.
            let started = std::time::Instant::now();
            let sent = go(
                &mut state,
                step("act", json!({"do": [{"click": submit}, {"click": text}]})),
            )
            .await;
            assert!(
                sent.contains("Form submitted") || sent.contains("Received"),
                "{sent}"
            );
            assert!(
                sent.contains("Clicked [") && sent.contains("did not run"),
                "{sent}"
            );
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "leaving the page is quick"
            );

            // A plain <select>.
            let url = "https://the-internet.herokuapp.com/dropdown";
            let page = go(&mut state, step("open", json!({ "url": url }))).await;
            let select = number_in(&page, "] dropdown");
            let chosen = go(
                &mut state,
                step(
                    "act",
                    json!({"do": [{"choose": select, "option": "Option 2"}]}),
                ),
            )
            .await;
            assert!(!chosen.starts_with("FAILED"), "{chosen}");
            assert!(
                chosen.contains("value=Option 2"),
                "the change shows:\n{chosen}"
            );

            // A search box submitted with Enter.
            let found = go(
                &mut state,
                step("open", json!({"url": "https://duckduckgo.com/"})),
            )
            .await;
            let search =
                number_of(&found, "] field").unwrap_or_else(|| number_in(&found, "Search"));
            let results = go(
                &mut state,
                step(
                    "act",
                    json!({"do": [{"fill": search, "text": "Wikipedia Pune", "enter": true}]}),
                ),
            )
            .await;
            assert!(results.contains("Pune"), "{results}");

            // A sign-in page: the password field is refused.
            let login = go(
                &mut state,
                step("open", json!({"url": "https://github.com/login"})),
            )
            .await;
            let password = number_in(&login, "] field Password");
            let secret = go(
                &mut state,
                step("act", json!({"do": [{"fill": password, "text": "x"}]})),
            )
            .await;
            assert!(secret.contains(refused), "{secret}");

            // A long list searched with find.
            go(
                &mut state,
                step("open", json!({"url": "https://news.ycombinator.com/"})),
            )
            .await;
            let comments = go(&mut state, step("read", json!({"find": "comments"}))).await;
            assert!(
                comments
                    .lines()
                    .filter(|row| row.contains("comments"))
                    .count()
                    >= 5,
                "{comments}"
            );
            let tabs = go(&mut state, step("tabs", json!({}))).await;
            assert!(tabs.contains("t5"), "{tabs}");
        });
    }

    /// Live: a background tab hidden for over 5 minutes (when Chromium's
    /// intensive timer throttling starts) still reads and acts. Set
    /// `PILOT_HIDDEN_SECS` to change the wait (default 360).
    ///
    /// `cargo test --lib live_pilot_long_hidden -- --ignored --nocapture --test-threads 1`
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn live_pilot_long_hidden_tab_still_works() {
        let hidden: u64 = std::env::var("PILOT_HIDDEN_SECS")
            .ok()
            .and_then(|secs| secs.parse().ok())
            .unwrap_or(360);
        let _edge = LiveEdge::start("grain-pilot-hidden-live");
        tauri::async_runtime::block_on(async {
            connected().await;
            let mut state = State::new();
            let date = chrono::Local::now().format("%Y%m%d");
            let url = format!("https://in.bookmyshow.com/cinemas/mumbai/cinepolis-lake-shore-thane-ex-viviana-mall/buytickets/CPVM/{date}");
            go(&mut state, step("open", json!({ "url": url }))).await;
            let form = "https://www.selenium.dev/selenium/web/web-form.html";
            go(&mut state, step("open", json!({ "url": form }))).await;
            println!("hiding both tabs for {hidden} s");
            tokio::time::sleep(Duration::from_secs(hidden)).await;
            println!("{:?}", ext(json!({"op": "tabs"})).await.ok());

            let page = go(&mut state, step("read", json!({"tab": "t2", "page": true}))).await;
            let text = number_in(&page, "] field Text input");
            let typed = go(
                &mut state,
                step(
                    "act",
                    json!({"tab": "t2", "do": [{"fill": text, "text": "late"}]}),
                ),
            )
            .await;
            assert!(typed.contains("value=late"), "{typed}");

            let shows = go(
                &mut state,
                step("read", json!({"tab": "t1", "find": "Book"})),
            )
            .await;
            let show = shows
                .lines()
                .filter(|row| row.contains("] Book "))
                .last()
                .and_then(|row| row[1..row.find(']')?].parse::<u32>().ok())
                .expect("a Book row");
            let clicked = go(
                &mut state,
                step("act", json!({"tab": "t1", "do": [{"click": show}]})),
            )
            .await;
            let mut seats = clicked.contains("How many seats");
            for _ in 0..3 {
                if seats {
                    break;
                }
                let again = go(&mut state, step("read", json!({"tab": "t1"}))).await;
                seats = again.contains("How many seats");
            }
            println!("seat dialog after a long hide: {seats}");
            if !seats {
                assert!(clicked.contains("call show"), "{clicked}");
                go(
                    &mut state,
                    step("show", json!({"tab": "t1", "reason": "view"})),
                )
                .await;
                tokio::time::sleep(Duration::from_secs(3)).await;
                let shown = go(&mut state, step("read", json!({"tab": "t1"}))).await;
                println!(
                    "seat dialog once shown: {}",
                    shown.contains("How many seats")
                );
            }
        });
    }

    /// Public pages of many kinds (search, forms, shops, travel, food,
    /// single-page apps, text-heavy), saved as page-controller reads for the
    /// offline tests in `grain-core`. Fresh profile: nothing signed in.
    const CAPTURE_SITES: &[(&str, &str)] = &[
        ("duckduckgo", "https://duckduckgo.com/"),
        (
            "selenium_form",
            "https://www.selenium.dev/selenium/web/web-form.html",
        ),
        (
            "internet_dropdown",
            "https://the-internet.herokuapp.com/dropdown",
        ),
        (
            "internet_checkboxes",
            "https://the-internet.herokuapp.com/checkboxes",
        ),
        ("github_login", "https://github.com/login"),
        ("hackernews", "https://news.ycombinator.com/"),
        ("amazon_search", "https://www.amazon.in/s?k=usb+cable"),
        (
            "flipkart_search",
            "https://www.flipkart.com/search?q=usb%20cable",
        ),
        ("makemytrip", "https://www.makemytrip.com/"),
        ("swiggy", "https://www.swiggy.com/"),
        ("booking", "https://www.booking.com/"),
        (
            "youtube_search",
            "https://www.youtube.com/results?search_query=lofi",
        ),
        ("reddit", "https://www.reddit.com/r/india/"),
        ("irctc", "https://www.irctc.co.in/nget/train-search"),
        ("bms_home", "https://in.bookmyshow.com/explore/home/mumbai"),
        ("wikipedia_article", "https://en.wikipedia.org/wiki/Pune"),
        ("airbnb", "https://www.airbnb.co.in/"),
        ("google_flights", "https://www.google.com/travel/flights"),
    ];

    /// Live: save [`CAPTURE_SITES`] as fixtures.
    ///
    /// `cargo test --lib live_pilot_capture -- --ignored --nocapture --test-threads 1`
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn live_pilot_capture_site_fixtures() {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("crates")
            .join("grain-core")
            .join("tests")
            .join("fixtures")
            .join("pilot")
            .join("sites");
        std::fs::create_dir_all(&out).expect("fixture folder");
        let _edge = LiveEdge::start("grain-pilot-capture-live");
        tauri::async_runtime::block_on(async {
            connected().await;
            let mut state = State::new();
            for (name, url) in CAPTURE_SITES {
                let opened = run(&mut state, &Call::Open(url.to_string())).await;
                if let Err(failure) = opened {
                    println!("{name}: open failed: {}", failure.message);
                    continue;
                }
                let handle = state.last.expect("opened tab");
                let tab_id = state.tabs[&handle].tab_id;
                // Let late scripts and lazy content settle.
                tokio::time::sleep(Duration::from_secs(3)).await;
                for view in ["full", "visible"] {
                    match ext(json!({"op": "read", "tab": tab_id, "view": view})).await {
                        Ok(reply) => {
                            let text = serde_json::to_string_pretty(&reply).unwrap_or_default();
                            println!("{name}.{view}: {} chars", text.len());
                            if text.len() <= 400_000 {
                                let _ =
                                    std::fs::write(out.join(format!("{name}.{view}.json")), text);
                            }
                        }
                        Err(failure) => println!("{name}.{view}: read failed: {}", failure.message),
                    }
                }
                let _ = run(
                    &mut state,
                    &Call::Tabs {
                        close: Some(Handle(handle)),
                    },
                )
                .await;
            }
        });
    }
}
