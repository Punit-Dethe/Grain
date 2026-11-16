//! [GRAIN] Grain Pilot: the desktop half of `grain_core::pilot`
//! (`docs/Pilot/PLAN.md`).
//!
//! The Agent acts in the user's own browser and apps through the Cua Driver
//! sidecar that `grain_pilot_install.rs` installs. The core module decides
//! what a call means, compacts reads and writes replies; this module runs the
//! driver, keeps the window handles the model sees (`w1`, `w2`…) and does the
//! few window moves the driver does not (parking a browser window off-screen,
//! handing it to the user).
//!
//! Lifetime (nothing outlives use):
//! - The driver daemon starts on the first call that needs it, with a
//!   capability manifest listing only the apps Pilot works in. It is stopped
//!   after [`IDLE_STOP`] without calls, on switch-off, on disconnect and at
//!   exit, and lives in a kill-on-close job object so it dies with Grain even
//!   after a crash.
//! - Each driver call is one short `cua-driver call` process. Grain holds no
//!   thread or socket between calls; the idle stop is one sleeping task.
//! - One call at a time: the state lock is the gate.
//!
//! Background by default: Pilot opens a new window of the user's Windows
//! default browser. A Firefox-family window (Firefox, Zen…) is parked just
//! past the right edge of the screen (they take focus for a new window).
//! Chromium windows stay where they open, behind the user's window, because
//! Chromium only takes clicks on controls that are on a screen. `show` brings
//! a window to the user; nothing else does.

use std::collections::{BTreeMap, HashSet};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use grain_core::capability_index::ExtensionDirectoryEntry;
use grain_core::execution::{ActionOutcome, FailureClass, SuccessData};
use grain_core::pilot::compact::{self, Read, View};
use grain_core::pilot::manifest::{self, App};
use grain_core::pilot::numbering::{resolve, Numbering, Resolved};
use grain_core::pilot::policy;
use grain_core::pilot::render::{self, Reply};
use grain_core::pilot::request::{self, BrowserFamily, DriverCall, Target};
use grain_core::pilot::tools::{self, Call, Handle, Step};
use grain_core::pilot::{self as core};
use serde_json::Value;
use tauri::AppHandle;

pub(crate) use grain_core::pilot::EXTENSION_ID;

/// One driver call (a read of a busy page takes ~0.6 s; a launch ~2 s).
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// From daemon start to its first answer (measured ~0.6 s).
const READY_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the daemon may sit unused before Grain stops it.
const IDLE_STOP: Duration = Duration::from_secs(5 * 60);
/// From launch to the new window appearing.
const WINDOW_WAIT: Duration = Duration::from_secs(12);
/// From the new window to its page having something to act on.
const LOAD_WAIT: Duration = Duration::from_secs(10);
/// Stopping the daemon (also at app exit, so it stays short).
const STOP_TIMEOUT: Duration = Duration::from_secs(4);
/// Windows one Agent run may keep open through Pilot.
const MAX_WINDOWS: usize = 8;
/// Most stdout read from one driver call.
const MAX_STDOUT_BYTES: u64 = 16 * 1024 * 1024;

static STATE: Mutex<State> = Mutex::new(State::new());
/// Bumped on every call; the idle stop only fires if no call came since.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static OUTPUT_SEQ: AtomicU64 = AtomicU64::new(0);

struct State {
    daemon: Option<Daemon>,
    windows: BTreeMap<u32, PilotWindow>,
    next_handle: u32,
    last: Option<u32>,
    /// The window the driver read last: a read replaces the session's
    /// snapshot, so an action in another window reads that one first.
    last_read: Option<u32>,
}

impl State {
    const fn new() -> Self {
        Self {
            daemon: None,
            windows: BTreeMap::new(),
            next_handle: 1,
            last: None,
            last_read: None,
        }
    }
}

struct Daemon {
    child: Child,
    pipe: String,
    apps: Vec<App>,
    _job: Option<win::Job>,
}

/// A window the model can name.
struct PilotWindow {
    target: Target,
    family: Option<BrowserFamily>,
    app: App,
    /// Pilot opened it, so `close` may close it.
    opened: bool,
    /// Parked off-screen (Firefox-family windows until `show`).
    parked: bool,
    numbering: Numbering,
    /// Snapshot id of the latest read; `None` until read (or after the
    /// daemon restarted).
    snapshot: Option<String>,
    markdown: String,
    /// The latest read in the controls view: every control's row and
    /// identity, and the "before" of the next action's change report.
    baseline: Read,
}

struct Ctx {
    exe: PathBuf,
    dir: PathBuf,
}

struct Failure {
    class: FailureClass,
    message: String,
    /// The driver's refusal code, when it refused.
    code: Option<String>,
}

impl Failure {
    fn new(class: FailureClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
            code: None,
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new(FailureClass::Internal, message)
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self::new(FailureClass::NotFound, message)
    }
    fn refused(message: impl Into<String>) -> Self {
        Self::new(FailureClass::Cancelled, message)
    }
    fn is_stale(&self) -> bool {
        matches!(
            self.code.as_deref(),
            Some("stale_element_token" | "invalid_element_token")
        )
    }
}

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

// ── Extension surface ──────────────────────────────────────────────────────

pub(crate) fn is_enabled(app: &AppHandle) -> bool {
    crate::settings::get_settings(app).pilot_enabled
}

/// [GRAIN] Turn Grain Pilot on or off. Off removes its tools from the Agent on
/// the next model round and stops the driver.
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

/// Connect: install the driver when it is missing. Returns its version.
pub(crate) async fn prepare(app: &AppHandle) -> Result<String, String> {
    crate::grain_pilot_install::ensure_installed(app).await?;
    Ok(core::DRIVER_VERSION.to_string())
}

/// Stop the driver and forget Pilot's windows, off the caller's thread.
pub(crate) fn stop_in_background() {
    tauri::async_runtime::spawn_blocking(|| stop(&mut lock()));
}

/// Stop the driver at app exit without waiting on a call in flight; the job
/// object ends the daemon when Grain's process goes regardless.
pub(crate) fn stop_at_exit() {
    if let Ok(mut state) = STATE.try_lock() {
        stop(&mut state);
    }
}

/// Disconnect: stop the driver, then delete it and its manifest.
pub(crate) fn disconnect_in_background(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        stop(&mut lock());
        crate::grain_pilot_install::remove(&app);
    });
}

fn stop(state: &mut State) {
    state.windows.clear();
    state.last = None;
    state.last_read = None;
    stop_daemon(state);
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
    let Some(exe) = crate::grain_pilot_install::installed(app) else {
        return failed(
            FailureClass::NotFound,
            "Grain Pilot's driver is not installed. Ask the user to connect Grain Pilot on the Extensions page.",
        );
    };
    let dir = match crate::grain_pilot_install::pilot_dir(app) {
        Ok(dir) => dir,
        Err(message) => return failed(FailureClass::Internal, &message),
    };
    crate::grain_pilot_install::note_driver_state_owner(app);
    let title = tools::tools()
        .into_iter()
        .find(|tool| tool.name == action_id)
        .map(|tool| tool.title.to_string());
    // Arguments can be private text; log the shape only.
    log::info!("[GRAIN] pilot: {}", parsed.describe_for_log());
    let read_only = parsed.read_only();
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let task = parsed.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || run(&mut lock(), &Ctx { exe, dir }, &task))
            .await
            .unwrap_or_else(|error| {
                Err(Failure::internal(format!(
                    "The Pilot call stopped: {error}"
                )))
            });
    schedule_idle_stop(generation);
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

/// Stop the daemon when no call arrives for [`IDLE_STOP`]. One sleeping task,
/// superseded by the next call.
fn schedule_idle_stop(generation: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(IDLE_STOP).await;
        if GENERATION.load(Ordering::SeqCst) == generation {
            let _ = tauri::async_runtime::spawn_blocking(move || {
                let mut state = lock();
                if GENERATION.load(Ordering::SeqCst) == generation && state.daemon.is_some() {
                    log::info!("[GRAIN] pilot: idle, stopping the driver");
                    stop_daemon(&mut state);
                }
            })
            .await;
        }
    });
}

// ── Calls ──────────────────────────────────────────────────────────────────

fn run(state: &mut State, ctx: &Ctx, call: &Call) -> Result<String, Failure> {
    match call {
        Call::Open(url) => open_url(state, ctx, url),
        Call::OpenApp(name) => open_app(state, ctx, name),
        Call::Windows => Ok(list_windows(state)),
        Call::Read { window, view } => {
            let handle = pick(state, *window)?;
            prepare_window(state, ctx, handle)?;
            let read = read_window(state, ctx, handle, view)?;
            Ok(render::read_reply(Handle(handle), &read))
        }
        Call::Click { window, control } => {
            let handle = pick(state, *window)?;
            act(state, ctx, handle, |state, ctx, handle| {
                let (snapshot, row) = row_for(
                    state,
                    handle,
                    *control,
                    Some(&[
                        "button",
                        "link",
                        "checkbox",
                        "radio",
                        "menu item",
                        "tab",
                        "option",
                        "dropdown",
                    ]),
                )?;
                refuse_payment(state, handle, *control)?;
                driver(
                    state,
                    ctx,
                    &request::click(target(state, handle)?, &snapshot, row),
                )?;
                Ok(format!("Clicked [{control}]"))
            })
        }
        Call::Type {
            window,
            control,
            text,
            submit,
        } => {
            let handle = pick(state, *window)?;
            act(state, ctx, handle, |state, ctx, handle| {
                let (snapshot, row) = row_for(
                    state,
                    handle,
                    *control,
                    Some(&["field", "dropdown", "spinner"]),
                )?;
                let snapshot = refuse_password(state, ctx, handle, row, snapshot)?;
                let window = target(state, handle)?;
                driver(
                    state,
                    ctx,
                    &request::type_into(window, &snapshot, row, text),
                )?;
                if *submit {
                    driver(state, ctx, &request::press(window, "return"))?;
                }
                Ok(format!(
                    "Typed into [{control}]{}",
                    if *submit { " and pressed Enter" } else { "" }
                ))
            })
        }
        Call::Choose {
            window,
            control,
            option,
        } => {
            let handle = pick(state, *window)?;
            act(state, ctx, handle, |state, ctx, handle| {
                choose(state, ctx, handle, *control, option)
            })
        }
        Call::Press { window, key } => {
            let handle = pick(state, *window)?;
            act(state, ctx, handle, |state, ctx, handle| {
                driver(state, ctx, &request::press(target(state, handle)?, key))?;
                Ok(format!("Pressed {key}"))
            })
        }
        Call::Scroll {
            window,
            direction,
            amount,
        } => {
            let handle = pick(state, *window)?;
            act(state, ctx, handle, |state, ctx, handle| {
                driver(
                    state,
                    ctx,
                    &request::scroll(target(state, handle)?, direction, *amount),
                )?;
                Ok(format!("Scrolled {direction}"))
            })
        }
        Call::Steps { window, steps } => {
            let handle = pick(state, *window)?;
            act(state, ctx, handle, |state, ctx, handle| {
                run_steps(state, ctx, handle, steps)
            })
        }
        Call::Show {
            window,
            reason,
            message,
        } => {
            let handle = pick(state, *window)?;
            show(state, ctx, handle)?;
            Ok(render::show_reply(
                Handle(handle),
                *reason,
                message.as_deref(),
            ))
        }
        Call::Close { window } => {
            let handle = pick(state, *window)?;
            close(state, handle)
        }
    }
}

/// The window a call means: the one it names, or the last one used.
fn pick(state: &mut State, window: Option<Handle>) -> Result<u32, Failure> {
    let handle = match window {
        Some(Handle(handle)) => handle,
        None => state.last.ok_or_else(|| {
            Failure::not_found("No window yet. Use open with a link, or open_app.")
        })?,
    };
    let Some(entry) = state.windows.get(&handle) else {
        return Err(Failure::not_found(format!(
            "There is no {}. Use windows to list Pilot's windows.",
            Handle(handle)
        )));
    };
    if !win::is_window(entry.target.window_id) {
        state.windows.remove(&handle);
        if state.last == Some(handle) {
            state.last = None;
        }
        return Err(Failure::not_found(format!(
            "{} was closed. Use windows, open or open_app.",
            Handle(handle)
        )));
    }
    state.last = Some(handle);
    Ok(handle)
}

fn target(state: &State, handle: u32) -> Result<Target, Failure> {
    state
        .windows
        .get(&handle)
        .map(|window| window.target)
        .ok_or_else(|| Failure::not_found(format!("There is no {}.", Handle(handle))))
}

fn window_mut(state: &mut State, handle: u32) -> Result<&mut PilotWindow, Failure> {
    state
        .windows
        .get_mut(&handle)
        .ok_or_else(|| Failure::not_found(format!("There is no {}.", Handle(handle))))
}

/// Make sure the daemon runs with this window's app in its manifest.
fn prepare_window(state: &mut State, ctx: &Ctx, handle: u32) -> Result<(), Failure> {
    let app = state
        .windows
        .get(&handle)
        .map(|window| window.app.clone())
        .ok_or_else(|| Failure::not_found(format!("There is no {}.", Handle(handle))))?;
    ensure_daemon(state, ctx, &app)
}

/// An action: read first when the session's snapshot is not this window's,
/// run it (retrying once if the page changed under it), then report what
/// changed once the page settles.
fn act(
    state: &mut State,
    ctx: &Ctx,
    handle: u32,
    action: impl Fn(&mut State, &Ctx, u32) -> Result<String, Failure>,
) -> Result<String, Failure> {
    prepare_window(state, ctx, handle)?;
    let fresh = state.last_read == Some(handle)
        && state
            .windows
            .get(&handle)
            .is_some_and(|window| window.snapshot.is_some());
    if !fresh {
        read_window(state, ctx, handle, &View::Controls)?;
    }
    let before = window_mut(state, handle)?.baseline.clone();
    let done = match action(state, ctx, handle) {
        Err(failure) if failure.is_stale() => {
            read_window(state, ctx, handle, &View::Controls)?;
            action(state, ctx, handle)?
        }
        other => other?,
    };
    let changes = settle(state, ctx, handle, &before)?;
    Ok(render::action_reply(Handle(handle), &done, &changes))
}

/// The driver row of control `number` in the latest read, with the snapshot
/// to address it by. `roles` limits which kinds of control the action takes.
fn row_for(
    state: &State,
    handle: u32,
    number: u32,
    roles: Option<&[&str]>,
) -> Result<(String, u32), Failure> {
    let window = state
        .windows
        .get(&handle)
        .ok_or_else(|| Failure::not_found(format!("There is no {}.", Handle(handle))))?;
    let snapshot = window
        .snapshot
        .clone()
        .ok_or_else(|| Failure::not_found("Read the window first."))?;
    match resolve(&window.numbering, number, &window.baseline.controls) {
        Resolved::Row(row) => {
            if let (Some(roles), Some(key)) = (roles, window.numbering.key(number)) {
                if !roles.contains(&key.role.as_str()) {
                    return Err(Failure::new(
                        FailureClass::InvalidArgument,
                        format!(
                            "[{number}] is a {}, which this action does not take.",
                            key.role
                        ),
                    ));
                }
            }
            Ok((snapshot, row))
        }
        Resolved::Unknown => Err(Failure::not_found(format!(
            "There is no [{number}] in {}. Read the window and use its numbers.",
            Handle(handle)
        ))),
        Resolved::Gone(key) => Err(Failure::not_found(format!(
            "[{number}] {} \"{}\" is no longer on the page. Read the window again.",
            key.role, key.name
        ))),
    }
}

/// Refuse a control that commits money; the user presses it after `show`.
fn refuse_payment(state: &State, handle: u32, number: u32) -> Result<(), Failure> {
    let name = state
        .windows
        .get(&handle)
        .and_then(|window| window.numbering.key(number))
        .map(|key| key.name.clone())
        .unwrap_or_default();
    if policy::is_payment_control(&name) {
        return Err(Failure::refused(render::payment_refusal(&name)));
    }
    Ok(())
}

/// Refuse typing into a password field. Windows UI Automation says which
/// fields are passwords; only when the window has one does Grain read the
/// control frames to compare. Returns the snapshot to type with (the frames
/// read replaces the previous one).
fn refuse_password(
    state: &mut State,
    ctx: &Ctx,
    handle: u32,
    row: u32,
    snapshot: String,
) -> Result<String, Failure> {
    let window_id = target(state, handle)?.window_id;
    let passwords = win::password_fields(window_id);
    if passwords.is_empty() {
        return Ok(snapshot);
    }
    let value = driver(state, ctx, &request::read_elements(target(state, handle)?))?;
    // The frames read replaced the snapshot: actions after it use its id.
    let element = value
        .get("elements")
        .and_then(Value::as_array)
        .and_then(|elements| {
            elements.iter().find(|element| {
                element.get("element_index").and_then(Value::as_u64) == Some(row as u64)
            })
        })
        .cloned()
        .ok_or_else(|| Failure::not_found("The page changed. Read the window again."))?;
    let token = element
        .get("element_token")
        .and_then(Value::as_str)
        .unwrap_or("");
    let new_snapshot = token.split(':').next().unwrap_or("").to_string();
    if !request::valid_snapshot(&new_snapshot) {
        return Err(Failure::internal("The driver gave an unreadable reply."));
    }
    {
        let window = window_mut(state, handle)?;
        window.snapshot = Some(new_snapshot.clone());
    }
    state.last_read = Some(handle);
    let frame = render::element_frames(&serde_json::json!({ "elements": [element] }))
        .first()
        .map(|(_, rect)| *rect);
    match frame {
        Some(frame) if !policy::is_password_target(frame, &passwords) => Ok(new_snapshot),
        _ => Err(Failure::refused(
            "Grain Pilot never types into a password field. Call show with reason sign_in so the user signs in.",
        )),
    }
}

fn choose(
    state: &mut State,
    ctx: &Ctx,
    handle: u32,
    number: u32,
    option: &str,
) -> Result<String, Failure> {
    let (snapshot, row) = row_for(state, handle, number, Some(&["dropdown"]))?;
    let markdown = window_mut(state, handle)?.markdown.clone();
    let (options, current) = compact::dropdown_options(&markdown, row)
        .ok_or_else(|| Failure::not_found("The drop-down is gone. Read the window again."))?;
    if options.is_empty() {
        return Err(Failure::not_found(
            "This drop-down shows no options in the read. Click it, read the window again, then choose.",
        ));
    }
    let wanted = request::choose_target(&options, option)
        .map_err(|message| Failure::new(FailureClass::InvalidArgument, message))?;
    if current == Some(wanted) {
        return Ok(format!(
            "\"{}\" was already chosen in [{number}]",
            options[wanted]
        ));
    }
    let keys = request::choose_keys(current, wanted)
        .map_err(|message| Failure::new(FailureClass::InvalidArgument, message))?;
    let window = target(state, handle)?;
    // Expanding a web drop-down takes keyboard focus into its window; the
    // window is parked again right after.
    let expanded = driver(state, ctx, &request::click(window, &snapshot, row));
    let result = expanded.and_then(|_| {
        std::thread::sleep(Duration::from_millis(300));
        let presses: Vec<DriverCall> = keys.iter().map(|key| request::press(window, key)).collect();
        driver(state, ctx, &request::steps(window, &presses))
    });
    if window_mut(state, handle)?.parked {
        win::park(window.window_id);
    }
    result?;
    Ok(format!("Chose \"{}\" in [{number}]", options[wanted]))
}

fn run_steps(state: &mut State, ctx: &Ctx, handle: u32, steps: &[Step]) -> Result<String, Failure> {
    let window = target(state, handle)?;
    let mut calls = Vec::with_capacity(steps.len());
    for step in steps {
        calls.push(match step {
            Step::Click(number) => {
                let (snapshot, row) = row_for(state, handle, *number, None)?;
                refuse_payment(state, handle, *number)?;
                request::click(window, &snapshot, row)
            }
            Step::Type(number, text) => {
                let (snapshot, row) = row_for(
                    state,
                    handle,
                    *number,
                    Some(&["field", "dropdown", "spinner"]),
                )?;
                let snapshot = refuse_password(state, ctx, handle, row, snapshot)?;
                request::type_into(window, &snapshot, row, text)
            }
            Step::Press(key) => request::press(window, key),
        });
    }
    // A frames read for the password check replaced the snapshot; every
    // token must use the latest one.
    let latest = window_mut(state, handle)?
        .snapshot
        .clone()
        .unwrap_or_default();
    for call in &mut calls {
        if let Some(token) = call.args.get("element_token").and_then(Value::as_str) {
            if let Some((_, row)) = token.split_once(':') {
                call.args["element_token"] = Value::String(format!("{latest}:{row}"));
            }
        }
    }
    driver(state, ctx, &request::steps(window, &calls))?;
    Ok(format!("Ran {} steps", steps.len()))
}

fn show(state: &mut State, ctx: &Ctx, handle: u32) -> Result<(), Failure> {
    prepare_window(state, ctx, handle)?;
    let window = target(state, handle)?;
    win::bring_on_screen(window.window_id);
    if driver(state, ctx, &request::bring_to_front(window)).is_err() {
        win::to_front(window.window_id);
    }
    window_mut(state, handle)?.parked = false;
    Ok(())
}

fn close(state: &mut State, handle: u32) -> Result<String, Failure> {
    let window = state
        .windows
        .get(&handle)
        .ok_or_else(|| Failure::not_found(format!("There is no {}.", Handle(handle))))?;
    if !window.opened {
        return Err(Failure::refused(
            "Grain Pilot only closes windows it opened. Leave this one open.",
        ));
    }
    let window_id = window.target.window_id;
    win::close(window_id);
    let deadline = Instant::now() + Duration::from_secs(3);
    while win::is_window(window_id) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if win::is_window(window_id) {
        return Err(Failure::refused(format!(
            "{} asked to confirm closing (it may hold other tabs). Leave it open, or call show so the user closes it.",
            Handle(handle)
        )));
    }
    state.windows.remove(&handle);
    if state.last == Some(handle) {
        state.last = None;
    }
    Ok(format!("Closed {}.", Handle(handle)))
}

fn list_windows(state: &mut State) -> String {
    state
        .windows
        .retain(|_, window| win::is_window(window.target.window_id));
    let rows: Vec<(Handle, String)> = state
        .windows
        .iter()
        .map(|(handle, window)| (Handle(*handle), win::title(window.target.window_id)))
        .collect();
    render::windows_reply(&rows)
}

// ── Reading ────────────────────────────────────────────────────────────────

/// Read a window and compact it. Updates the window's snapshot, numbering
/// and baseline.
fn read_window(state: &mut State, ctx: &Ctx, handle: u32, view: &View) -> Result<Read, Failure> {
    let family = window_mut(state, handle)?.family;
    let mut attempt = 0;
    loop {
        let window = target(state, handle)?;
        let value = driver(state, ctx, &request::read(window))?;
        let (snapshot, markdown) = render::window_state(&value)
            .ok_or_else(|| Failure::internal("The driver gave an unreadable reply."))?;
        state.last_read = Some(handle);
        let entry = window_mut(state, handle)?;
        // The baseline keeps page text as well as controls, so the change
        // report after an action also shows text that changed (a total, an
        // error, a calculator display).
        let baseline = compact::compact(&markdown, &View::Page, &mut entry.numbering);
        // Chromium switches on page accessibility when first read, so the
        // first read can come back almost empty.
        if family == Some(BrowserFamily::Chromium) && baseline.controls.is_empty() && attempt == 0 {
            attempt += 1;
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        let mut baseline = baseline;
        if baseline.title.trim().is_empty() {
            // An error page or app window can have an untitled document;
            // the window's own title still says where it is.
            baseline.title = win::title(entry.target.window_id);
        }
        let shown = match view {
            View::Page => baseline.clone(),
            other => Read {
                title: baseline.title.clone(),
                ..compact::compact(&markdown, other, &mut entry.numbering)
            },
        };
        entry.snapshot = Some(snapshot);
        entry.markdown = markdown;
        entry.baseline = baseline;
        return Ok(shown);
    }
}

fn differs(a: &Read, b: &Read) -> bool {
    a.lines != b.lines || a.url != b.url || a.title != b.title
}

/// After an action: read until the page stops changing, bounded by
/// [`core::SETTLE_MS`], and report what changed. A click returns before the
/// page reacts, so an unchanged page is read again until the time is up.
fn settle(state: &mut State, ctx: &Ctx, handle: u32, before: &Read) -> Result<String, Failure> {
    let deadline = Instant::now() + Duration::from_millis(core::SETTLE_MS);
    std::thread::sleep(Duration::from_millis(250));
    let mut latest = read_window(state, ctx, handle, &View::Page)?;
    let mut changed = differs(before, &latest);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(if changed { 200 } else { 300 }));
        let next = read_window(state, ctx, handle, &View::Page)?;
        if changed && !differs(&latest, &next) {
            latest = next;
            break;
        }
        changed = changed || differs(before, &next);
        latest = next;
    }
    if latest.url != before.url || latest.title != before.title {
        // A new page: list its controls, as a fresh read would, rather than
        // every line of its text.
        let window = window_mut(state, handle)?;
        let controls = Read {
            title: latest.title.clone(),
            ..compact::compact(&window.markdown, &View::Controls, &mut window.numbering)
        };
        return Ok(compact::changes(before, &controls));
    }
    Ok(compact::changes(before, &latest))
}

// ── Opening ────────────────────────────────────────────────────────────────

fn add_window(state: &mut State, window: PilotWindow) -> Result<u32, Failure> {
    if state.windows.len() >= MAX_WINDOWS {
        return Err(Failure::refused(format!(
            "Grain Pilot already has {MAX_WINDOWS} windows open. Close one first."
        )));
    }
    let handle = state.next_handle;
    state.next_handle += 1;
    state.windows.insert(handle, window);
    state.last = Some(handle);
    Ok(handle)
}

fn new_window(
    target: Target,
    family: Option<BrowserFamily>,
    app: App,
    opened: bool,
    parked: bool,
) -> PilotWindow {
    PilotWindow {
        target,
        family,
        app,
        opened,
        parked,
        numbering: Numbering::new(),
        snapshot: None,
        markdown: String::new(),
        baseline: Read::default(),
    }
}

fn open_url(state: &mut State, ctx: &Ctx, url: &str) -> Result<String, Failure> {
    // One browser window at a time: a new page replaces the browser window
    // Pilot opened last, so a task never piles up windows. Only windows Pilot
    // opened (found by diffing the browser's windows at launch) are closed.
    let previous: Vec<u32> = state
        .windows
        .iter()
        .filter(|(_, window)| window.opened && window.family.is_some())
        .map(|(handle, _)| *handle)
        .collect();
    let mut replaced = None;
    for handle in previous {
        let window_id = state.windows[&handle].target.window_id;
        if !win::is_window(window_id) {
            state.windows.remove(&handle);
            continue;
        }
        close(state, handle).map_err(|_| {
            Failure::refused(format!(
                "{} could not be closed to make room (it may ask to confirm). Keep working in it, or call show so the user closes it.",
                Handle(handle)
            ))
        })?;
        replaced = Some(handle);
    }
    if state.windows.len() >= MAX_WINDOWS {
        return Err(Failure::refused(format!(
            "Grain Pilot already has {MAX_WINDOWS} windows open. Close one first."
        )));
    }
    let browser = win::default_browser()
        .ok_or_else(|| Failure::not_found("Could not find the default browser."))?;
    let browser_text = browser.to_string_lossy().to_string();
    let family = request::browser_family(&browser_text).ok_or_else(|| {
        Failure::not_found(format!(
            "Grain Pilot works with Zen, Firefox, Chrome, Edge, Brave, Vivaldi and Opera. The default browser ({}) is not one of them yet.",
            browser.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default()
        ))
    })?;
    let app = App::Executable(browser_text.clone());
    ensure_daemon(state, ctx, &app)?;
    let before: HashSet<u64> = win::windows_of(&browser)
        .into_iter()
        .map(|(window_id, _)| window_id)
        .collect();
    driver(
        state,
        ctx,
        &request::launch(&browser_text, &request::open_arguments(family, url)),
    )?;
    let (window_id, pid) = wait_for_new_window(&browser, &before, family)?;
    let parked = family != BrowserFamily::Chromium;
    if parked {
        win::park(window_id);
    } else {
        win::send_back(window_id);
    }
    let handle = add_window(
        state,
        new_window(
            Target {
                pid: pid as i64,
                window_id,
            },
            Some(family),
            app,
            true,
            parked,
        ),
    )?;
    let read = read_until_loaded(state, ctx, handle, Some(&compact::host(url)))?;
    let reply = render::opened_reply(Handle(handle), &read);
    Ok(match replaced {
        Some(old) => format!(
            "Closed {} (Pilot keeps one browser window). {reply}",
            Handle(old)
        ),
        None => reply,
    })
}

/// The browser's new window. When the browser was not running, it may
/// restore the user's own windows too; Pilot's is the one that appears last
/// (it is created after the session restore), and Pilot only ever parks and
/// reads that one.
fn wait_for_new_window(
    browser: &Path,
    before: &HashSet<u64>,
    family: BrowserFamily,
) -> Result<(u64, u32), Failure> {
    let deadline = Instant::now() + WINDOW_WAIT;
    let mut found: Option<(u64, u32)> = None;
    let mut seen_at = Instant::now();
    while Instant::now() < deadline {
        let fresh: Vec<(u64, u32)> = win::windows_of(browser)
            .into_iter()
            .filter(|(window_id, _)| !before.contains(window_id))
            .collect();
        if let Some(newest) = fresh.first().copied() {
            if found.map(|(id, _)| id) != Some(newest.0) {
                found = Some(newest);
                seen_at = Instant::now();
            }
            // Firefox-family browsers take focus as soon as the window
            // exists: park quickly.
            let quiet = if family == BrowserFamily::Chromium {
                600
            } else {
                250
            };
            if seen_at.elapsed() >= Duration::from_millis(quiet) {
                return Ok(newest);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    found.ok_or_else(|| {
        Failure::internal("The browser did not open a new window. Try again, or check that the browser starts normally.")
    })
}

/// Read until the page has something to act on (and, for a link, is on the
/// site asked for), bounded by [`LOAD_WAIT`].
fn read_until_loaded(
    state: &mut State,
    ctx: &Ctx,
    handle: u32,
    host: Option<&str>,
) -> Result<Read, Failure> {
    let deadline = Instant::now() + LOAD_WAIT;
    std::thread::sleep(Duration::from_millis(400));
    loop {
        let read = read_window(state, ctx, handle, &View::Controls)?;
        let on_site = host.is_none_or(|host| {
            read.url
                .as_deref()
                .is_some_and(|url| same_site(&compact::host(url), host))
        });
        if (on_site && !read.controls.is_empty()) || Instant::now() >= deadline {
            return Ok(read);
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}

/// `www.example.com` and `example.com` count as one site.
fn same_site(a: &str, b: &str) -> bool {
    let trim = |host: &str| host.trim_start_matches("www.").to_ascii_lowercase();
    let (a, b) = (trim(a), trim(b));
    a == b || a.ends_with(&format!(".{b}")) || b.ends_with(&format!(".{a}"))
}

fn open_app(state: &mut State, ctx: &Ctx, name: &str) -> Result<String, Failure> {
    if state.windows.len() >= MAX_WINDOWS {
        return Err(Failure::refused(format!(
            "Grain Pilot already has {MAX_WINDOWS} windows open. Close one first."
        )));
    }
    let apps = crate::context_detect::app_catalog::installed_apps();
    let found = find_app(&apps, name)?;
    let app = if !found.icon_id.contains('\\') && found.icon_id.contains('!') {
        if policy::is_refused_packaged(&found.icon_id) {
            return Err(Failure::refused(
                "Grain Pilot never drives shells, terminals or Grain itself.",
            ));
        }
        App::Packaged(found.icon_id.clone())
    } else {
        if policy::is_refused_app(&found.icon_id) {
            return Err(Failure::refused(
                "Grain Pilot never drives shells, terminals or Grain itself.",
            ));
        }
        if request::browser_family(&found.icon_id).is_some() {
            return Err(Failure::new(
                FailureClass::InvalidArgument,
                "That is a browser: use open with a link instead.",
            ));
        }
        App::Executable(found.icon_id.clone())
    };
    // The manifest check also refuses relative paths and non-.exe targets.
    manifest::manifest_yaml(std::slice::from_ref(&app)).map_err(Failure::refused)?;
    ensure_daemon(state, ctx, &app)?;
    let before: HashSet<u64> = win::all_windows().into_iter().collect();
    let launch = match &app {
        App::Executable(path) => request::launch(path, &[]),
        App::Packaged(aumid) => request::launch_packaged(aumid),
    };
    let reply = driver(state, ctx, &launch)?;
    let pid = reply.get("pid").and_then(Value::as_i64).unwrap_or(0);
    // The launch reply usually lists the window already.
    let launched = render::windows(&reply);
    let window_id = match launched
        .iter()
        .find(|(id, _, minimized)| !before.contains(id) && !minimized)
        .or_else(|| launched.iter().find(|(_, _, minimized)| !minimized))
    {
        Some((window_id, _, _)) => *window_id,
        None => wait_for_app_window(state, ctx, pid, &app, &before)?,
    };
    let pid = win::window_pid(window_id).map(i64::from).unwrap_or(pid);
    // A window that was already open (an app that reuses its window) is the
    // user's: Pilot works in it but never closes it.
    let opened = !before.contains(&window_id);
    let handle = add_window(
        state,
        new_window(Target { pid, window_id }, None, app, opened, false),
    )?;
    let read = read_until_loaded(state, ctx, handle, None)?;
    let mut reply = render::opened_reply(Handle(handle), &read);
    if !opened {
        reply.push_str(
            "\n(This window was already open: it is the user's, so Pilot will not close it.)",
        );
    }
    Ok(reply)
}

fn find_app<'a>(
    apps: &'a [crate::context_detect::app_catalog::InstalledApp],
    wanted: &str,
) -> Result<&'a crate::context_detect::app_catalog::InstalledApp, Failure> {
    let wanted = wanted.trim().trim_end_matches(".exe").to_lowercase();
    if let Some(app) = apps.iter().find(|app| app.name.to_lowercase() == wanted) {
        return Ok(app);
    }
    let partial: Vec<&crate::context_detect::app_catalog::InstalledApp> = apps
        .iter()
        .filter(|app| app.name.to_lowercase().contains(&wanted))
        .collect();
    match partial.as_slice() {
        [one] => Ok(one),
        [] => Err(Failure::not_found(format!(
            "No installed app is called \"{wanted}\"."
        ))),
        many => Err(Failure::new(
            FailureClass::InvalidArgument,
            format!(
                "Several apps match: {}. Name one exactly.",
                many.iter()
                    .take(8)
                    .map(|app| format!("\"{}\"", app.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// The launched app's window: from the driver's list for its pid, or a new
/// top-level window of its executable (a launcher can hand off and exit).
fn wait_for_app_window(
    state: &mut State,
    ctx: &Ctx,
    pid: i64,
    app: &App,
    before: &HashSet<u64>,
) -> Result<u64, Failure> {
    let deadline = Instant::now() + WINDOW_WAIT;
    while Instant::now() < deadline {
        if pid > 0 {
            if let Ok(value) = driver(state, ctx, &request::list_windows(pid)) {
                let windows = render::windows(&value);
                let pick = windows
                    .iter()
                    .find(|(id, _, minimized)| !before.contains(id) && !minimized)
                    .or_else(|| windows.iter().find(|(_, _, minimized)| !minimized))
                    .or_else(|| windows.first());
                if let Some((window_id, _, _)) = pick {
                    return Ok(*window_id);
                }
            }
        }
        if let App::Executable(path) = app {
            if let Some((window_id, _)) = win::windows_of(Path::new(path))
                .into_iter()
                .find(|(window_id, _)| !before.contains(window_id))
            {
                return Ok(window_id);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(Failure::internal(
        "The app started but no window appeared. Try again in a moment.",
    ))
}

// ── Daemon ─────────────────────────────────────────────────────────────────

/// Start the daemon, or restart it with `app` added to its manifest.
fn ensure_daemon(state: &mut State, ctx: &Ctx, app: &App) -> Result<(), Failure> {
    if let Some(daemon) = &mut state.daemon {
        let alive = matches!(daemon.child.try_wait(), Ok(None));
        if alive && daemon.apps.contains(app) {
            return Ok(());
        }
    }
    let mut apps: Vec<App> = state
        .daemon
        .as_ref()
        .map(|daemon| daemon.apps.clone())
        .unwrap_or_default();
    for window in state.windows.values() {
        if !apps.contains(&window.app) {
            apps.push(window.app.clone());
        }
    }
    if !apps.contains(app) {
        apps.push(app.clone());
    }
    // A Store app's window belongs to the system's frame host, not the app's
    // own process, so the driver can only reach it with the host listed.
    // Pilot still only acts in windows it opened (the model has no other
    // handles).
    if apps.iter().any(|app| matches!(app, App::Packaged(_))) {
        if let Some(host) = win::frame_host() {
            if !apps.contains(&host) {
                apps.push(host);
            }
        }
    }
    stop_daemon(state);
    let daemon = start_daemon(ctx, apps)?;
    state.daemon = Some(daemon);
    // A new daemon has no snapshots: every window reads again first.
    for window in state.windows.values_mut() {
        window.snapshot = None;
    }
    state.last_read = None;
    Ok(())
}

fn start_daemon(ctx: &Ctx, apps: Vec<App>) -> Result<Daemon, Failure> {
    let yaml = manifest::manifest_yaml(&apps).map_err(Failure::internal)?;
    std::fs::create_dir_all(&ctx.dir)
        .map_err(|error| Failure::internal(format!("Could not prepare Grain Pilot: {error}")))?;
    let manifest_path = ctx.dir.join("manifest.yaml");
    // Plain bytes: a byte-order mark fails the driver's parse.
    std::fs::write(&manifest_path, yaml.as_bytes())
        .map_err(|error| Failure::internal(format!("Could not prepare Grain Pilot: {error}")))?;
    let pipe = format!(r"\\.\pipe\grain-pilot-{}", std::process::id());
    let mut command = Command::new(&ctx.exe);
    command
        .arg("serve")
        .arg("--socket")
        .arg(&pipe)
        .args(["--permission-mode", "bounded", "--capability-manifest"])
        .arg(&manifest_path)
        .args(["--approve-capability-manifest", "--no-overlay"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    driver_env(&mut command);
    let child = command.spawn().map_err(|error| {
        Failure::internal(format!("Could not start Grain Pilot's driver: {error}"))
    })?;
    let job = win::Job::holding(&child);
    let mut daemon = Daemon {
        child,
        pipe,
        apps,
        _job: job,
    };
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if let Ok(Some(status)) = daemon.child.try_wait() {
            return Err(Failure::internal(format!(
                "Grain Pilot's driver stopped as it started ({status}). Disconnect and connect Grain Pilot again."
            )));
        }
        // Any answer (even a refusal for Grain's own process) means ready.
        let probe = DriverCall {
            tool: "list_windows",
            args: serde_json::json!({ "pid": std::process::id(), "session": core::SESSION }),
        };
        if let Ok((ok, stdout)) = run_driver(&ctx.exe, &daemon.pipe, &probe, Duration::from_secs(3))
        {
            match render::parse_reply(&stdout, ok) {
                Reply::Done(_) | Reply::Refused { .. } => break,
                Reply::Failed(message) if message.contains("Permission denied") => break,
                Reply::Failed(_) => {}
            }
        }
        if Instant::now() >= deadline {
            kill_daemon(&mut daemon);
            return Err(Failure::internal(
                "Grain Pilot's driver did not start. Try again in a moment.",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    log::info!("[GRAIN] pilot: driver ready ({} apps)", daemon.apps.len());
    Ok(daemon)
}

fn stop_daemon(state: &mut State) {
    let Some(mut daemon) = state.daemon.take() else {
        return;
    };
    if let Some(exe) = daemon_exe(&daemon) {
        let mut command = Command::new(exe);
        command
            .args(["stop", "--socket", &daemon.pipe])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        driver_env(&mut command);
        if let Ok(mut stopper) = command.spawn() {
            wait_or_kill(&mut stopper, STOP_TIMEOUT);
        }
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while matches!(daemon.child.try_wait(), Ok(None)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    kill_daemon(&mut daemon);
    log::info!("[GRAIN] pilot: driver stopped");
}

/// The executable the daemon runs (the `stop` command must be the same build).
fn daemon_exe(daemon: &Daemon) -> Option<PathBuf> {
    win::process_image(daemon.child.id())
}

fn kill_daemon(daemon: &mut Daemon) {
    if matches!(daemon.child.try_wait(), Ok(None)) {
        let _ = daemon.child.kill();
    }
    let _ = daemon.child.wait();
    // Dropping the job ends anything the daemon started.
}

fn wait_or_kill(child: &mut Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(30)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        }
    }
}

fn driver_env(command: &mut Command) {
    for (key, value) in core::DRIVER_ENV {
        command.env(key, value);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

/// One driver call through the running daemon.
fn driver(state: &State, ctx: &Ctx, call: &DriverCall) -> Result<Value, Failure> {
    let daemon = state
        .daemon
        .as_ref()
        .ok_or_else(|| Failure::internal("Grain Pilot's driver is not running."))?;
    let (ok, stdout) =
        run_driver(&ctx.exe, &daemon.pipe, call, CALL_TIMEOUT).map_err(Failure::internal)?;
    match render::parse_reply(&stdout, ok) {
        Reply::Done(value) => Ok(value),
        Reply::Refused { code, message } => Err(Failure {
            class: match code.as_str() {
                "stale_element_token" | "invalid_element_token" => FailureClass::NotFound,
                "bounded_resource_outside_manifest" => FailureClass::Cancelled,
                _ => FailureClass::Internal,
            },
            message: render::explain(&code, &message),
            code: Some(code),
        }),
        Reply::Failed(message) => Err(Failure::internal(format!(
            "Grain Pilot's driver failed: {message}"
        ))),
    }
}

/// Run `cua-driver call <tool> --socket <pipe>` with the arguments on stdin.
/// Returns whether it exited successfully and its stdout.
fn run_driver(
    exe: &Path,
    pipe: &str,
    call: &DriverCall,
    timeout: Duration,
) -> Result<(bool, String), String> {
    let output_path = std::env::temp_dir().join(format!(
        "grain-pilot-{}-{}.json",
        std::process::id(),
        OUTPUT_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let output = OutputFile::create(output_path)?;
    let stdout = output
        .file
        .try_clone()
        .map_err(|error| format!("Could not prepare the driver call: {error}"))?;
    let mut command = Command::new(exe);
    command
        .args(["call", call.tool, "--socket", pipe])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::null());
    driver_env(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not run Grain Pilot's driver: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(call.stdin().as_bytes());
    }
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "The driver did not answer within {} seconds; the action may or may not have happened. Read the window to check.",
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => return Err(format!("Lost track of the driver call: {error}")),
        }
    };
    Ok((status.success(), output.read()?))
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
            .map_err(|error| format!("Could not prepare the driver call: {error}"))?;
        Ok(Self { path, file })
    }

    fn read(&self) -> Result<String, String> {
        let mut bytes = Vec::new();
        std::fs::File::open(&self.path)
            .and_then(|file| file.take(MAX_STDOUT_BYTES).read_to_end(&mut bytes))
            .map_err(|error| format!("Could not read the driver's reply: {error}"))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Drop for OutputFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

// ── Windows ────────────────────────────────────────────────────────────────

#[cfg(windows)]
mod win {
    //! The window moves and checks the driver does not do.

    use std::path::{Path, PathBuf};

    use grain_core::pilot::policy::Rect;
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation8, IUIAutomation2, TreeScope_Descendants, UIA_IsPasswordPropertyId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetSystemMetrics, GetWindow, GetWindowTextLengthW,
        GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostMessageW,
        SetForegroundWindow, SetWindowPos, ShowWindow, SystemParametersInfoW, GW_OWNER,
        HWND_BOTTOM, HWND_TOP, SM_CXVIRTUALSCREEN, SM_XVIRTUALSCREEN, SPI_GETWORKAREA,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_MINIMIZE, SW_RESTORE,
        SW_SHOWNOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_CLOSE,
    };

    fn hwnd(window_id: u64) -> HWND {
        HWND(window_id as usize as *mut core::ffi::c_void)
    }

    /// A kill-on-close job object: the daemon and everything it starts end
    /// when the last handle closes, which includes Grain exiting or crashing.
    pub(crate) struct Job(isize);

    impl Job {
        pub(crate) fn holding(child: &std::process::Child) -> Option<Job> {
            use std::os::windows::io::AsRawHandle as _;
            unsafe {
                let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).ok()?;
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                let process = HANDLE(child.as_raw_handle());
                if set.is_err() || AssignProcessToJobObject(job, process).is_err() {
                    let _ = CloseHandle(job);
                    log::warn!("[GRAIN] pilot: could not tie the driver to Grain's lifetime");
                    return None;
                }
                Some(Job(job.0 as isize))
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(HANDLE(self.0 as *mut core::ffi::c_void));
            }
        }
    }

    pub(crate) fn is_window(window_id: u64) -> bool {
        window_id != 0 && unsafe { IsWindow(Some(hwnd(window_id))).as_bool() }
    }

    pub(crate) fn title(window_id: u64) -> String {
        unsafe {
            let length = GetWindowTextLengthW(hwnd(window_id));
            if length <= 0 {
                return String::new();
            }
            let mut buffer = vec![0u16; length as usize + 1];
            let copied = GetWindowTextW(hwnd(window_id), &mut buffer);
            String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
        }
    }

    pub(crate) fn window_pid(window_id: u64) -> Option<u32> {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd(window_id), Some(&mut pid)) };
        (pid != 0).then_some(pid)
    }

    pub(crate) fn process_image(pid: u32) -> Option<PathBuf> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = vec![0u16; 1024];
            let mut length = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            );
            let _ = CloseHandle(process);
            result.ok()?;
            Some(PathBuf::from(String::from_utf16_lossy(
                &buffer[..length as usize],
            )))
        }
    }

    unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
        let list = &mut *(list.0 as *mut Vec<u64>);
        list.push(window.0 as usize as u64);
        BOOL(1)
    }

    /// Every top-level window that is visible, unowned and titled: the
    /// windows a person would call app windows.
    pub(crate) fn all_windows() -> Vec<u64> {
        let mut list: Vec<u64> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut list as *mut Vec<u64> as isize));
        }
        list.retain(|&window_id| unsafe {
            let window = hwnd(window_id);
            IsWindowVisible(window).as_bool()
                && GetWindow(window, GW_OWNER).map_or(true, |owner| owner.0.is_null())
                && GetWindowTextLengthW(window) > 0
        });
        list
    }

    /// App windows of processes running `executable`, front-most first.
    pub(crate) fn windows_of(executable: &Path) -> Vec<(u64, u32)> {
        let wanted = executable.to_string_lossy().to_lowercase();
        let mut images: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
        all_windows()
            .into_iter()
            .filter_map(|window_id| {
                let pid = window_pid(window_id)?;
                let matches = *images.entry(pid).or_insert_with(|| {
                    process_image(pid)
                        .is_some_and(|image| image.to_string_lossy().to_lowercase() == wanted)
                });
                matches.then_some((window_id, pid))
            })
            .collect()
    }

    fn virtual_right() -> i32 {
        unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN) }
    }

    /// Hand focus back to the user and park the window just past the right
    /// edge of the screen, restored (a minimised window cannot be clicked)
    /// and at the bottom of the window order.
    pub(crate) fn park(window_id: u64) {
        let window = hwnd(window_id);
        unsafe {
            let _ = ShowWindow(window, SW_MINIMIZE);
            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                window,
                Some(HWND_BOTTOM),
                virtual_right() + 50,
                0,
                1366,
                900,
                SWP_NOACTIVATE,
            );
        }
    }

    /// A Chromium window stays on screen (its clicks need that) but behind
    /// the user's window, if it took focus.
    pub(crate) fn send_back(window_id: u64) {
        let window = hwnd(window_id);
        unsafe {
            if GetForegroundWindow() == window {
                let _ = ShowWindow(window, SW_MINIMIZE);
                std::thread::sleep(std::time::Duration::from_millis(150));
                let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            }
            let _ = SetWindowPos(
                window,
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    /// Move a parked window onto the main screen's work area.
    pub(crate) fn bring_on_screen(window_id: u64) {
        let window = hwnd(window_id);
        unsafe {
            let mut area = RECT::default();
            let got = SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut area as *mut RECT as *mut core::ffi::c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
            let _ = ShowWindow(window, SW_RESTORE);
            if got.is_ok() {
                let width = (area.right - area.left - 80).clamp(800, 1366);
                let height = (area.bottom - area.top - 80).clamp(600, 900);
                let _ = SetWindowPos(
                    window,
                    Some(HWND_TOP),
                    area.left + 40,
                    area.top + 40,
                    width,
                    height,
                    SWP_SHOWWINDOW,
                );
            }
        }
    }

    pub(crate) fn to_front(window_id: u64) {
        unsafe {
            let _ = SetForegroundWindow(hwnd(window_id));
        }
    }

    pub(crate) fn close(window_id: u64) {
        unsafe {
            let _ = PostMessageW(Some(hwnd(window_id)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// Screen rectangles of the window's password fields, by Windows UI
    /// Automation's own `IsPassword` (never the site's label or id).
    pub(crate) fn password_fields(window_id: u64) -> Vec<Rect> {
        unsafe {
            let _com = ComGuard(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
            let Ok(automation) =
                CoCreateInstance::<_, IUIAutomation2>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
            else {
                return Vec::new();
            };
            let _ = automation.SetConnectionTimeout(2_000);
            let _ = automation.SetTransactionTimeout(2_000);
            let Ok(root) = automation.ElementFromHandle(hwnd(window_id)) else {
                return Vec::new();
            };
            let Ok(condition) =
                automation.CreatePropertyCondition(UIA_IsPasswordPropertyId, &VARIANT::from(true))
            else {
                return Vec::new();
            };
            let Ok(found) = root.FindAll(TreeScope_Descendants, &condition) else {
                return Vec::new();
            };
            let count = found.Length().unwrap_or(0);
            (0..count)
                .filter_map(|at| found.GetElement(at).ok())
                .filter_map(|element| element.CurrentBoundingRectangle().ok())
                .map(|rect| Rect {
                    x: rect.left as f64,
                    y: rect.top as f64,
                    w: (rect.right - rect.left) as f64,
                    h: (rect.bottom - rect.top) as f64,
                })
                .collect()
        }
    }

    /// `ApplicationFrameHost.exe`, which owns Store apps' windows.
    pub(crate) fn frame_host() -> Option<grain_core::pilot::manifest::App> {
        let root = std::env::var_os("SystemRoot")?;
        let path = PathBuf::from(root)
            .join("System32")
            .join("ApplicationFrameHost.exe");
        path.is_file().then(|| {
            grain_core::pilot::manifest::App::Executable(path.to_string_lossy().to_string())
        })
    }

    /// The default browser's executable, from the https association.
    pub(crate) fn default_browser() -> Option<PathBuf> {
        use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER};
        use winreg::RegKey;
        const ASSOCIATION: &str =
            r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https";
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let prog_id: String = user
            .open_subkey(format!(r"{ASSOCIATION}\UserChoiceLatest\ProgId"))
            .and_then(|key| key.get_value("ProgId"))
            .or_else(|_| {
                user.open_subkey(format!(r"{ASSOCIATION}\UserChoice"))
                    .and_then(|key| key.get_value("ProgId"))
            })
            .ok()?;
        let command: String = RegKey::predef(HKEY_CLASSES_ROOT)
            .open_subkey(format!(r"{prog_id}\shell\open\command"))
            .and_then(|key| key.get_value(""))
            .ok()?;
        let path = super::command_executable(&command)?;
        PathBuf::from(&path).is_file().then(|| PathBuf::from(path))
    }
}

#[cfg(not(windows))]
mod win {
    use std::path::{Path, PathBuf};

    use grain_core::pilot::policy::Rect;

    pub(crate) struct Job;
    impl Job {
        pub(crate) fn holding(_: &std::process::Child) -> Option<Job> {
            None
        }
    }
    pub(crate) fn is_window(_: u64) -> bool {
        false
    }
    pub(crate) fn title(_: u64) -> String {
        String::new()
    }
    pub(crate) fn window_pid(_: u64) -> Option<u32> {
        None
    }
    pub(crate) fn process_image(_: u32) -> Option<PathBuf> {
        None
    }
    pub(crate) fn all_windows() -> Vec<u64> {
        Vec::new()
    }
    pub(crate) fn windows_of(_: &Path) -> Vec<(u64, u32)> {
        Vec::new()
    }
    pub(crate) fn park(_: u64) {}
    pub(crate) fn send_back(_: u64) {}
    pub(crate) fn bring_on_screen(_: u64) {}
    pub(crate) fn to_front(_: u64) {}
    pub(crate) fn close(_: u64) {}
    pub(crate) fn password_fields(_: u64) -> Vec<Rect> {
        Vec::new()
    }
    pub(crate) fn default_browser() -> Option<PathBuf> {
        None
    }
    pub(crate) fn frame_host() -> Option<grain_core::pilot::manifest::App> {
        None
    }
}

/// The executable at the start of a shell `open` command:
/// `"C:\Program Files\Zen Browser\zen.exe" -osint -url "%1"`.
fn command_executable(command: &str) -> Option<String> {
    let command = command.trim();
    let path = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next()?
    } else {
        command.split(' ').next()?
    };
    (!path.is_empty()).then(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_rows_belong_to_pilot_and_match_the_core_digest() {
        let (digest, tools) = catalog();
        assert_eq!(digest, core::catalog_digest());
        assert_eq!(tools.len(), tools::tools().len());
        for tool in &tools {
            assert_eq!(tool.action.extension_id, EXTENSION_ID);
            assert_eq!(tool.parameters["type"], "object");
        }
        let safe: Vec<_> = tools
            .iter()
            .filter(|tool| tool.action.risk.is_safe())
            .map(|tool| tool.action.action_id.as_str())
            .collect();
        assert_eq!(safe, ["windows", "read"]);
    }

    #[test]
    fn the_browser_comes_from_its_open_command() {
        assert_eq!(
            command_executable(r#""C:\Program Files\Zen Browser\zen.exe" -osint -url "%1""#)
                .as_deref(),
            Some(r"C:\Program Files\Zen Browser\zen.exe")
        );
        assert_eq!(
            command_executable(r"C:\Apps\browser.exe %1").as_deref(),
            Some(r"C:\Apps\browser.exe")
        );
        assert_eq!(command_executable("  "), None);
    }

    #[test]
    fn sites_match_with_or_without_www() {
        assert!(same_site("www.bookmyshow.com", "bookmyshow.com"));
        assert!(same_site("in.bookmyshow.com", "bookmyshow.com"));
        assert!(!same_site("example.com", "bookmyshow.com"));
    }

    #[test]
    fn apps_are_found_by_exact_then_unique_name() {
        let app = |name: &str| crate::context_detect::app_catalog::InstalledApp {
            name: name.into(),
            target: name.to_lowercase(),
            icon_id: format!(r"C:\Apps\{name}.exe"),
        };
        let apps = vec![app("Notepad"), app("Notepad++"), app("Paint")];
        assert_eq!(
            find_app(&apps, "notepad").map(|a| a.name.as_str()).ok(),
            Some("Notepad")
        );
        assert_eq!(
            find_app(&apps, "pai").map(|a| a.name.as_str()).ok(),
            Some("Paint")
        );
        assert!(find_app(&apps, "note").is_err_and(|f| f.message.contains("Several")));
        assert!(find_app(&apps, "word").is_err());
    }

    #[test]
    fn a_missing_driver_is_reported_not_panicked() {
        let call = request::list_windows(1);
        let error = run_driver(
            Path::new("Z:/definitely/not/cua-driver.exe"),
            r"\\.\pipe\none",
            &call,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.starts_with("Could not run"), "{error}");
    }

    // ── Live runs (Phase 3) ────────────────────────────────────────────────
    //
    // These download the pinned driver (once, into the temp folder), start it
    // and drive the real default browser or a real app, printing each reply
    // with its time and size. Run one at a time:
    // `cargo test --lib live_pilot_ -- --ignored --nocapture --test-threads 1`

    fn live_ctx() -> Ctx {
        let dir = std::env::temp_dir().join("grain-pilot-live");
        let exe =
            tauri::async_runtime::block_on(crate::grain_pilot_install::ensure_installed_in(&dir))
                .expect("driver installed");
        Ctx { exe, dir }
    }

    fn step(state: &mut State, ctx: &Ctx, action: &str, args: Value) -> Result<String, String> {
        let call = tools::parse(action, &args).expect("valid call");
        let started = Instant::now();
        let result = run(state, ctx, &call).map_err(|failure| failure.message);
        let text = match &result {
            Ok(text) => text.clone(),
            Err(message) => format!("FAILED: {message}"),
        };
        println!(
            "--- {action} {args} ({} ms, ~{} tokens)\n{text}\n",
            started.elapsed().as_millis(),
            text.chars().count() / 4
        );
        result
    }

    /// Pilot's state for one live test. Dropping it (also when the test
    /// panics) closes the windows Pilot opened and stops the driver.
    struct Live(State);

    impl Drop for Live {
        fn drop(&mut self) {
            for window in self.0.windows.values().filter(|window| window.opened) {
                win::close(window.target.window_id);
            }
            stop(&mut self.0);
        }
    }

    /// The number of the first control line that contains every word.
    fn number_of(text: &str, words: &[&str]) -> u32 {
        text.lines()
            .find(|line| words.iter().all(|word| line.contains(word)))
            .and_then(|line| line.split('[').nth(1)?.split(']').next()?.parse().ok())
            .unwrap_or_else(|| panic!("no control with {words:?} in:\n{text}"))
    }

    #[test]
    #[ignore = "downloads the driver and drives the default browser"]
    fn live_pilot_browser_reads_clicks_and_closes() {
        let ctx = live_ctx();
        let mut live = Live(State::new());
        let state = &mut live.0;
        let opened = step(
            state,
            &ctx,
            "open",
            serde_json::json!({"url": "https://www.iana.org/help/example-domains"}),
        )
        .expect("open");
        assert!(opened.starts_with("Opened w1"), "{opened}");
        assert!(opened.contains(render::UNTRUSTED));
        let found = step(
            state,
            &ctx,
            "read",
            serde_json::json!({"find": "reserved domains"}),
        )
        .expect("find");
        let link = number_of(&found, &["link", "Reserved"]);
        let clicked =
            step(state, &ctx, "click", serde_json::json!({"control": link})).expect("click");
        assert!(
            clicked.contains("Reserved") || clicked.contains("reserved"),
            "{clicked}"
        );
        let listed = step(state, &ctx, "windows", serde_json::json!({})).expect("windows");
        assert!(listed.starts_with("w1:"), "{listed}");
        // A number Pilot never gave out is refused, not guessed.
        let unknown = step(state, &ctx, "click", serde_json::json!({"control": 9999}));
        assert!(unknown.unwrap_err().contains("There is no [9999]"));
        let closed = step(state, &ctx, "close", serde_json::json!({})).expect("close");
        assert_eq!(closed, "Closed w1.");
    }

    #[test]
    #[ignore = "downloads the driver and drives BookMyShow in the default browser"]
    fn live_pilot_booking_reaches_the_seat_page() {
        let ctx = live_ctx();
        let mut live = Live(State::new());
        let state = &mut live.0;
        let date = chrono::Local::now().format("%Y%m%d");
        let url = format!(
            "https://in.bookmyshow.com/cinemas/mumbai/cinepolis-lake-shore-thane-ex-viviana-mall/buytickets/CPVM/{date}"
        );
        step(state, &ctx, "open", serde_json::json!({ "url": url })).expect("open");
        let shows =
            step(state, &ctx, "read", serde_json::json!({"find": "PM"})).expect("find shows");
        let show = number_of(&shows, &["button", "Book 10:"]);
        let clicked =
            step(state, &ctx, "click", serde_json::json!({"control": show})).expect("click show");
        println!(
            "seat page reached: {}",
            clicked.contains("Select Seats") || clicked.contains("seat")
        );
        let closed = step(state, &ctx, "close", serde_json::json!({})).expect("close");
        assert_eq!(closed, "Closed w1.");
    }

    /// Calculator holds no user data, unlike Notepad, which restores the
    /// user's unsaved tabs when it starts.
    #[test]
    #[ignore = "downloads the driver and drives Calculator"]
    fn live_pilot_app_clicks_and_reads_back() {
        let ctx = live_ctx();
        let mut live = Live(State::new());
        let state = &mut live.0;
        let opened = step(
            state,
            &ctx,
            "open_app",
            serde_json::json!({"name": "Calculator"}),
        )
        .expect("open_app");
        let one = number_of(&opened, &["button \"One\""]);
        let plus = number_of(&opened, &["button \"Plus\""]);
        let two = number_of(&opened, &["button \"Two\""]);
        let equals = number_of(&opened, &["button \"Equals\""]);
        let clicks: Vec<Value> = [one, plus, two, equals]
            .iter()
            .map(|control| serde_json::json!({"action": "click", "control": control}))
            .collect();
        let ran =
            step(state, &ctx, "steps", serde_json::json!({ "steps": clicks })).expect("steps");
        let display =
            step(state, &ctx, "read", serde_json::json!({"find": "display"})).expect("read");
        let closed = step(state, &ctx, "close", serde_json::json!({})).expect("close");
        assert!(display.contains("Display is 3"), "{display}");
        assert!(
            ran.contains("Display is 3"),
            "the change report shows the result: {ran}"
        );
        assert_eq!(closed, "Closed w1.");
    }
}
