//! Grain-rendered session UI for Extension Mode.
//!
//! The same prewarmed Tauri window owns routing, choice, execution progress,
//! and finite results. Grain owns every element, style, transition, focus rule,
//! and lifecycle; extensions can return data and text, never UI declarations.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::grain_actions::action_log::{self, ActionLogOutcome};

pub const LABEL: &str = "extension-view";
const URL: &str = "extension-view.html";
const PRESENT_EVENT: &str = "extension-view://present";
const WINDOW_SIZE: (f64, f64) = (640.0, 720.0);
const WINDOW_MIN: (f64, f64) = (440.0, 360.0);
const REQUEST_PREVIEW_MAX_CHARS: usize = 2_048;
const COMPLETION_DISMISS_MS: u32 = 1_600;
const UNAVAILABLE_DISMISS_MS: u32 = 10_000;

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct ActiveView {
    session_id: u64,
    request_id: Option<u64>,
    extension_id: Option<String>,
    extension_name: Option<String>,
    request: String,
    content: ExtensionViewContent,
    busy: bool,
    /// The native capture pill stays visible until this webview has painted and
    /// shown. Reusing the value through every post-capture state makes the
    /// native-to-webview hand-off atomic instead of timing it with a delay.
    pill_session_id: Option<u64>,
}

#[derive(Clone, Copy, Default)]
struct OutputTarget {
    window: Option<crate::agent::CapturedWindowTarget>,
    has_selection: bool,
}

fn output_target() -> &'static Mutex<Option<OutputTarget>> {
    static TARGET: OnceLock<Mutex<Option<OutputTarget>>> = OnceLock::new();
    TARGET.get_or_init(|| Mutex::new(None))
}

/// Snapshot the destination before the recommendation UI takes focus. Windows
/// can inspect selection length without reading it. Other platforms fail
/// closed for Insert/Replace until Grain can bind and revalidate an original
/// native destination; Copy remains available everywhere.
pub fn capture_output_target(_app: &AppHandle) {
    #[cfg(windows)]
    let has_selection = crate::context_detect::focused_has_selection();
    #[cfg(not(windows))]
    let has_selection = false;

    *output_target().lock().unwrap() = Some(OutputTarget {
        window: crate::agent::foreground_window_target(),
        has_selection,
    });
}

fn active() -> &'static Mutex<Option<ActiveView>> {
    static ACTIVE: OnceLock<Mutex<Option<ActiveView>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(None))
}

fn bounded_message(message: &str) -> String {
    message.trim().chars().take(8192).collect()
}

fn bounded_request_preview(request: &str) -> String {
    request
        .trim()
        .chars()
        .take(REQUEST_PREVIEW_MAX_CHARS)
        .collect()
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionChoiceCandidate {
    pub extension_id: String,
    pub name: String,
    pub purpose: String,
    pub signal: String,
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionViewContent {
    Routing {
        request_preview: Option<String>,
    },
    Choose {
        presentation_id: u64,
        request_preview: String,
        candidates: Vec<ExtensionChoiceCandidate>,
        name_only: bool,
    },
    Running {
        automatic: bool,
    },
    Result {
        message: String,
        tone: ResultTone,
        can_copy: bool,
        can_insert: bool,
        can_replace: bool,
        can_open_extensions: bool,
        dismiss_after_ms: Option<u32>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ResultTone {
    Success,
    Warning,
    Danger,
}

fn result_content(
    message: String,
    tone: ResultTone,
    allow_output: bool,
    dismiss_after_ms: Option<u32>,
) -> ExtensionViewContent {
    let target = *output_target().lock().unwrap();
    ExtensionViewContent::Result {
        message,
        tone,
        can_copy: dismiss_after_ms.is_none(),
        can_insert: allow_output && target.is_some_and(target_is_usable),
        can_replace: allow_output && target.is_some_and(|target| target.has_selection),
        can_open_extensions: false,
        dismiss_after_ms,
    }
}

fn target_is_usable(target: OutputTarget) -> bool {
    target.window.is_some()
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionViewInit {
    pub session_id: u64,
    pub extension_id: Option<String>,
    pub extension_name: Option<String>,
    pub content: ExtensionViewContent,
}

impl From<&ActiveView> for ExtensionViewInit {
    fn from(active: &ActiveView) -> Self {
        Self {
            session_id: active.session_id,
            extension_id: active.extension_id.clone(),
            extension_name: active.extension_name.clone(),
            content: active.content.clone(),
        }
    }
}

fn extension_name(app: &AppHandle, extension_id: &str) -> String {
    crate::extension_host::load_manifest(app, extension_id)
        .map(|pack| pack.manifest.name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| extension_id.to_string())
}

fn notify_closed(app: &AppHandle, removed: &ActiveView) {
    match &removed.content {
        ExtensionViewContent::Routing { .. } => {
            if let Some(request_id) = removed.request_id {
                crate::grain_actions::action_session::dismiss_request_from_view(request_id);
            } else if let Some(pill_session_id) = removed.pill_session_id {
                crate::grain_actions::action_session::cancel_if_pill_session(app, pill_session_id);
            } else {
                crate::grain_actions::action_session::cancel(app);
            }
        }
        ExtensionViewContent::Choose {
            presentation_id, ..
        } => {
            crate::grain_actions::action_session::dismiss_from_view(*presentation_id);
        }
        ExtensionViewContent::Running { .. } => {
            if let Some(request_id) = removed.request_id {
                crate::grain_actions::action_session::dismiss_request_from_view(request_id);
            }
        }
        ExtensionViewContent::Result { .. } => {}
    }
}

/// Create the renderer on Tauri's UI thread. Window construction must never run
/// inline in a global-shortcut callback: on Windows that re-enters the event
/// loop which is waiting for the callback to return, freezing audio, Escape,
/// and every later shortcut event.
fn build_renderer_on_main(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(URL.into()))
        .title("Grain Extension")
        .inner_size(WINDOW_SIZE.0, WINDOW_SIZE.1)
        .min_inner_size(WINDOW_MIN.0, WINDOW_MIN.1)
        .resizable(true)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(false)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(false)
        .visible(false)
        .center();

    let window = builder.build().map_err(|error| error.to_string())?;
    let app_for_close = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            let removed = active().lock().unwrap().take();
            if removed.is_some() {
                output_target().lock().unwrap().take();
            }
            if let Some(removed) = removed {
                notify_closed(&app_for_close, &removed);
            }
        }
    });
    log::debug!("[GRAIN] extension view: renderer warmed");
    Ok(())
}

fn current_session_is(session_id: u64) -> bool {
    active()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|active| active.session_id == session_id)
}

fn take_session(session_id: u64) -> Option<ActiveView> {
    let mut slot = active().lock().unwrap();
    if slot
        .as_ref()
        .is_some_and(|active| active.session_id == session_id)
    {
        slot.take()
    } else {
        None
    }
}

fn destroy_window_on_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

fn schedule_window_destroy(app: &AppHandle) {
    let app_for_ui = app.clone();
    if let Err(error) = app.run_on_main_thread(move || destroy_window_on_main(&app_for_ui)) {
        log::warn!("[GRAIN] extension view: could not schedule renderer teardown: {error}");
    }
}

/// Fail closed when the host renderer cannot be created or reached. The
/// recorder/session cleanup is as important as the window cleanup: otherwise a
/// failed webview leaves the microphone and dynamic Escape binding owned until
/// process exit.
fn fail_renderer(
    app: &AppHandle,
    session_id: Option<u64>,
    pill_session_id: Option<u64>,
    error: impl std::fmt::Display,
    already_on_main: bool,
) {
    let removed = session_id.and_then(take_session);
    if session_id.is_some() && removed.is_none() {
        // A newer presentation superseded this queued job. It owns the renderer
        // now, so an old failure must not tear it down or surface an error.
        return;
    }
    if removed.is_some() {
        output_target().lock().unwrap().take();
    }
    if already_on_main {
        destroy_window_on_main(app);
    } else {
        schedule_window_destroy(app);
    }
    if let Some(removed) = removed.as_ref() {
        notify_closed(app, removed);
    } else if let Some(pill_session_id) = pill_session_id {
        crate::grain_actions::action_session::cancel_if_pill_session(app, pill_session_id);
    }
    let message = format!("Extension Mode could not open its interaction window: {error}");
    log::error!("[GRAIN] extension view: {message}");
}

/// Queue creation of the powerless host renderer hidden behind active capture.
/// A repeated call is free; a cancelled capture destroys it through [`destroy`].
pub fn warm(app: &AppHandle, pill_session_id: u64) -> Result<(), String> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    let app_for_ui = app.clone();
    app.run_on_main_thread(move || {
        // Capture may have been cancelled before this queued job reached the UI
        // thread. Do not create a resident hidden webview for a dead session.
        if !crate::grain_actions::action_session::owns_pill_session(pill_session_id) {
            return;
        }
        if let Err(error) = build_renderer_on_main(&app_for_ui) {
            fail_renderer(&app_for_ui, None, Some(pill_session_id), error, true);
        }
    })
    .map_err(|error| error.to_string())
}

fn store_and_emit(
    app: &AppHandle,
    mut next: ActiveView,
    reuse_session: bool,
) -> Result<u64, String> {
    let init = {
        let mut slot = active().lock().unwrap();
        if reuse_session {
            if let Some(current) = slot.as_ref() {
                next.session_id = current.session_id;
                next.pill_session_id = current.pill_session_id;
                if next.request_id.is_none() {
                    next.request_id = current.request_id;
                }
            }
        }
        let init = ExtensionViewInit::from(&next);
        *slot = Some(next);
        init
    };
    let session_id = init.session_id;
    let app_for_ui = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        if !current_session_is(session_id) {
            return;
        }
        if let Err(error) = build_renderer_on_main(&app_for_ui) {
            fail_renderer(&app_for_ui, Some(session_id), None, error, true);
            return;
        }
        // Cancellation/supersession can run while WebView2 is being created.
        // Revalidate before publishing into the newly created renderer.
        if !current_session_is(session_id) {
            destroy_window_on_main(&app_for_ui);
            return;
        }
        if let Err(error) = app_for_ui.emit_to(LABEL, PRESENT_EVENT, init) {
            fail_renderer(&app_for_ui, Some(session_id), None, error, true);
        }
    }) {
        fail_renderer(app, Some(session_id), None, error, false);
        return Err("could not schedule the Extension Mode interaction window".into());
    }
    Ok(session_id)
}

/// Begin the post-capture interaction while ASR is still finalising. The
/// renderer was prewarmed during recording; it becomes visible only after its
/// first frame calls [`extension_view_ready`].
pub fn present_routing(app: &AppHandle, pill_session_id: u64) -> Result<u64, String> {
    store_and_emit(
        app,
        ActiveView {
            session_id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            request_id: None,
            extension_id: None,
            extension_name: None,
            request: String::new(),
            content: ExtensionViewContent::Routing {
                request_preview: None,
            },
            busy: false,
            pill_session_id: Some(pill_session_id),
        },
        false,
    )
}

/// Keep the routing surface informative while the blocking recommendation pass
/// runs. Only a bounded preview enters the renderer; the verbatim request stays
/// in the host-owned pending session for the eventual hand-off.
pub fn present_ranking(app: &AppHandle, request_id: u64, request: &str) -> Result<u64, String> {
    store_and_emit(
        app,
        ActiveView {
            session_id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            request_id: Some(request_id),
            extension_id: None,
            extension_name: None,
            request: request.to_string(),
            content: ExtensionViewContent::Routing {
                request_preview: Some(bounded_request_preview(request)),
            },
            busy: false,
            pill_session_id: None,
        },
        true,
    )
}

pub fn present_choice(
    app: &AppHandle,
    request_id: u64,
    presentation_id: u64,
    request: &str,
    candidates: Vec<ExtensionChoiceCandidate>,
    name_only: bool,
) -> Result<u64, String> {
    store_and_emit(
        app,
        ActiveView {
            session_id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            request_id: Some(request_id),
            extension_id: None,
            extension_name: None,
            request: request.to_string(),
            content: ExtensionViewContent::Choose {
                presentation_id,
                request_preview: bounded_request_preview(request),
                candidates,
                name_only,
            },
            busy: false,
            pill_session_id: None,
        },
        true,
    )
}

pub fn present_running(
    app: &AppHandle,
    request_id: u64,
    extension_id: &str,
    request: &str,
    automatic: bool,
) -> Result<u64, String> {
    let name = extension_name(app, extension_id);
    store_and_emit(
        app,
        ActiveView {
            session_id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            request_id: Some(request_id),
            extension_id: Some(extension_id.to_string()),
            extension_name: Some(name),
            request: request.to_string(),
            content: ExtensionViewContent::Running { automatic },
            busy: true,
            pill_session_id: None,
        },
        true,
    )
}

/// Show a finite host-owned outcome. Used when an extension returns a message
/// after it completes a request.
pub fn present_result(
    app: &AppHandle,
    extension_id: &str,
    message: String,
    tone: ResultTone,
) -> Result<u64, String> {
    let message = bounded_message(&message);
    if message.is_empty() {
        return Err("result message must not be empty".into());
    }
    let name = extension_name(app, extension_id);
    store_and_emit(
        app,
        ActiveView {
            session_id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            request_id: None,
            extension_id: Some(extension_id.to_string()),
            extension_name: Some(name),
            // The hand-off path has already logged the request and outcome.
            request: String::new(),
            content: result_content(message, tone, tone == ResultTone::Success, None),
            busy: false,
            pill_session_id: None,
        },
        true,
    )
}

/// Show a brief completion state for an extension that returned no copyable
/// result. This keeps the direct-send transition legible without turning a
/// no-output task into a modal the user must dismiss.
pub fn present_completion(app: &AppHandle, extension_id: &str) -> Result<u64, String> {
    let name = extension_name(app, extension_id);
    store_and_emit(
        app,
        ActiveView {
            session_id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            request_id: None,
            extension_id: Some(extension_id.to_string()),
            extension_name: Some(name.clone()),
            request: String::new(),
            content: result_content(
                format!("Completed in {name}."),
                ResultTone::Success,
                false,
                Some(COMPLETION_DISMISS_MS),
            ),
            busy: false,
            pill_session_id: None,
        },
        true,
    )
}

fn unavailable_content(message: &str) -> Result<ExtensionViewContent, String> {
    let message = bounded_message(message);
    if message.is_empty() {
        return Err("unavailable message must not be empty".into());
    }
    Ok(ExtensionViewContent::Result {
        message,
        tone: ResultTone::Warning,
        can_copy: false,
        can_insert: false,
        can_replace: false,
        can_open_extensions: true,
        dismiss_after_ms: Some(UNAVAILABLE_DISMISS_MS),
    })
}

/// Explain a pre-capture refusal in the same bounded, keyboard-dismissible host
/// window instead of putting the native recording pill into its generic,
/// persistent error fallback. This is also the actionable first-run path when
/// the catalogue has not supplied any searchable extension yet.
pub fn present_unavailable(app: &AppHandle, message: &str) -> Result<u64, String> {
    let content = unavailable_content(message)?;
    store_and_emit(
        app,
        ActiveView {
            session_id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            request_id: None,
            extension_id: None,
            extension_name: None,
            request: String::new(),
            content,
            busy: false,
            pill_session_id: None,
        },
        false,
    )
}

/// Tear down the webview and all retained extension display state. Safe on every
/// capture/cancel path; the hidden warm renderer must never become residency.
pub fn destroy(app: &AppHandle) {
    active().lock().unwrap().take();
    output_target().lock().unwrap().take();
    schedule_window_destroy(app);
}

/// Whether a painted-but-not-yet-ready renderer is responsible for retiring
/// this native pill. Used by the recorder tail so a fast ranking pass cannot
/// hide the pill before a cold webview becomes visible.
pub fn owns_pill_handoff(pill_session_id: u64) -> bool {
    active()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|active| active.pill_session_id == Some(pill_session_id))
}

/// Drop a standard view whose worker is being disabled, removed, or reloaded.
pub fn destroy_for_extension(app: &AppHandle, extension_id: &str) {
    let belongs_to_extension = active()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|active| active.extension_id.as_deref() == Some(extension_id));
    if belongs_to_extension {
        destroy(app);
    }
}

/// A crashed/reaped worker invalidates an in-progress request, but a finite
/// result no longer depends on that worker and remains copyable until closed.
pub fn fail_interactive_for_extension(app: &AppHandle, extension_id: &str, reason: &str) {
    let init = {
        let mut slot = active().lock().unwrap();
        let Some(active) = slot.as_mut().filter(|active| {
            active.extension_id.as_deref() == Some(extension_id)
                && matches!(&active.content, ExtensionViewContent::Running { .. })
        }) else {
            return;
        };
        let bounded_reason = bounded_message(reason);
        active.content = result_content(
            bounded_message(&format!(
                "The extension stopped before it finished: {bounded_reason}"
            )),
            ResultTone::Danger,
            false,
            None,
        );
        active.busy = false;
        action_log::record(
            &active.request,
            active.extension_id.clone(),
            None,
            None,
            ActionLogOutcome::Failed {
                reason: bounded_reason,
            },
        );
        active.request.clear();
        ExtensionViewInit::from(&*active)
    };
    let _ = app.emit_to(LABEL, PRESENT_EVENT, init);
}

fn caller_is_view(window: &WebviewWindow) -> bool {
    window.label() == LABEL
}

#[tauri::command]
#[specta::specta]
pub fn extension_view_init(window: WebviewWindow) -> Option<ExtensionViewInit> {
    if !caller_is_view(&window) {
        return None;
    }
    active()
        .lock()
        .unwrap()
        .as_ref()
        .map(ExtensionViewInit::from)
}

#[tauri::command]
#[specta::specta]
pub fn extension_view_ready(window: WebviewWindow, session_id: u64) -> Result<(), String> {
    if !caller_is_view(&window) {
        return Err("extension view command called from the wrong window".into());
    }
    if !active()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|active| active.session_id == session_id)
    {
        return Err("stale extension view session".into());
    }
    let _ = window.center();
    window.show().map_err(|error| error.to_string())?;
    {
        // Claim the hand-off while holding the renderer slot. `complete` checks
        // this same slot before it retires the native pill, so keeping the lock
        // through `surface_ready` closes both duplicate-ready and pill-gap races.
        let mut slot = active().lock().unwrap();
        let current = slot
            .as_mut()
            .filter(|active| active.session_id == session_id)
            .ok_or("stale extension view session")?;
        if let Some(pill_session_id) = current.pill_session_id.take() {
            crate::grain_actions::action_session::surface_ready(
                &window.app_handle(),
                pill_session_id,
            );
        }
    }
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn extension_view_choose(
    app: AppHandle,
    window: WebviewWindow,
    session_id: u64,
    presentation_id: u64,
    extension_id: String,
) -> Result<(), String> {
    if !caller_is_view(&window) {
        return Err("extension view command called from the wrong window".into());
    }
    let allowed = active().lock().unwrap().as_ref().is_some_and(|active| {
        active.session_id == session_id
            && matches!(
                &active.content,
                ExtensionViewContent::Choose {
                    presentation_id: current,
                    candidates,
                    ..
                } if *current == presentation_id
                    && candidates.iter().any(|candidate| candidate.extension_id == extension_id)
            )
    });
    if !allowed {
        return Err("stale or invalid extension choice".into());
    }
    crate::grain_actions::action_session::accept(&app, presentation_id, &extension_id)
}

#[tauri::command]
#[specta::specta]
pub async fn extension_view_download_model(
    app: AppHandle,
    window: WebviewWindow,
    session_id: u64,
) -> Result<(), String> {
    if !caller_is_view(&window) {
        return Err("extension view command called from the wrong window".into());
    }
    let request_id = {
        let mut slot = active().lock().unwrap();
        let current = slot
            .as_mut()
            .filter(|active| active.session_id == session_id)
            .ok_or("stale extension view session")?;
        if current.busy {
            return Err("a language model download is already in progress".into());
        }
        let request_id = match &current.content {
            ExtensionViewContent::Choose {
                name_only: true, ..
            } => current.request_id,
            _ => None,
        }
        .ok_or("this chooser is not waiting for the language model")?;
        current.busy = true;
        request_id
    };
    if let Err(error) = crate::grain_space::embed::download_model(app.clone()).await {
        if let Some(current) = active().lock().unwrap().as_mut().filter(|active| {
            active.session_id == session_id
                && matches!(&active.content, ExtensionViewContent::Choose { .. })
        }) {
            current.busy = false;
        }
        return Err(error);
    }
    crate::grain_actions::action_session::rerank(&app, request_id).await;
    if let Some(current) = active().lock().unwrap().as_mut().filter(|active| {
        active.session_id == session_id
            && matches!(&active.content, ExtensionViewContent::Choose { .. })
    }) {
        current.busy = false;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn extension_view_copy(
    app: AppHandle,
    window: WebviewWindow,
    session_id: u64,
) -> Result<(), String> {
    if !caller_is_view(&window) {
        return Err("extension view command called from the wrong window".into());
    }
    let message = active()
        .lock()
        .unwrap()
        .as_ref()
        .filter(|active| active.session_id == session_id)
        .and_then(|active| match &active.content {
            ExtensionViewContent::Result {
                message,
                can_copy: true,
                ..
            } => Some(message.clone()),
            _ => None,
        })
        .ok_or("there is no result to copy")?;
    app.clipboard()
        .write_text(message)
        .map_err(|error| format!("failed to copy result: {error}"))
}

#[tauri::command]
#[specta::specta]
pub fn extension_view_open_extensions(
    app: AppHandle,
    window: WebviewWindow,
    session_id: u64,
) -> Result<(), String> {
    if !caller_is_view(&window) {
        return Err("extension view command called from the wrong window".into());
    }
    let allowed = active().lock().unwrap().as_ref().is_some_and(|active| {
        active.session_id == session_id
            && matches!(
                &active.content,
                ExtensionViewContent::Result {
                    can_open_extensions: true,
                    ..
                }
            )
    });
    if !allowed {
        return Err("this session cannot open Extension settings".into());
    }

    extension_view_close(app.clone(), window, session_id)?;
    crate::show_main_window(&app);
    let main = app
        .get_webview_window("main")
        .ok_or("Grain's main window is unavailable")?;
    main.eval("window.location.hash = '#/extensions/store';")
        .map_err(|error| format!("could not open the Extensions store: {error}"))
}

/// Insert a successful finite result back into the app Extension Mode was
/// started from. Replace is exposed only when Grain observed a non-empty text
/// selection at capture; Insert collapses that selection to its trailing edge.
#[tauri::command]
#[specta::specta]
pub fn extension_view_output(
    app: AppHandle,
    window: WebviewWindow,
    session_id: u64,
    action: String,
) -> Result<(), String> {
    if !caller_is_view(&window) {
        return Err("extension view command called from the wrong window".into());
    }
    let replace = match action.as_str() {
        "insert" => false,
        "replace" => true,
        _ => return Err("unknown result output action".into()),
    };
    #[cfg(not(windows))]
    return Err("Insert and Replace require a securely captured native output target".into());

    #[cfg(windows)]
    let (message, target, removed) = {
        let mut slot = active().lock().unwrap();
        let current = slot
            .as_ref()
            .filter(|active| active.session_id == session_id)
            .ok_or("stale extension view session")?;
        let ExtensionViewContent::Result {
            message,
            tone: ResultTone::Success,
            can_insert: true,
            can_replace,
            ..
        } = &current.content
        else {
            return Err("this result cannot be inserted".into());
        };
        if replace && !*can_replace {
            return Err("there is no captured selection to replace".into());
        }
        let target = output_target()
            .lock()
            .unwrap()
            .take()
            .ok_or("the original output target is no longer available")?;
        let message = message.clone();
        let removed = slot.take().ok_or("stale extension view session")?;
        (message, target, removed)
    };
    #[cfg(windows)]
    if let Err(error) = window.destroy() {
        *active().lock().unwrap() = Some(removed);
        *output_target().lock().unwrap() = Some(target);
        return Err(error.to_string());
    }

    #[cfg(windows)]
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(120));
        #[cfg(windows)]
        if !target.window.is_some_and(crate::agent::refocus_window) {
            crate::bridge::emit(
                &app,
                grain_core::DaemonEvent::PasteError {
                    error: "The app selected for this result is no longer open.".into(),
                },
            );
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(120));

        #[cfg(windows)]
        if !target
            .window
            .is_some_and(crate::agent::captured_window_is_foreground)
        {
            crate::bridge::emit(
                &app,
                grain_core::DaemonEvent::PasteError {
                    error: "The original app did not keep focus, so Grain cancelled the output."
                        .into(),
                },
            );
            return;
        }

        if !replace && target.has_selection {
            use enigo::{Enigo, Key, Keyboard, Settings};
            let collapsed = match Enigo::new(&Settings::default()) {
                Ok(mut enigo) => {
                    crate::input::release_modifiers(&mut enigo);
                    enigo
                        .key(Key::RightArrow, enigo::Direction::Click)
                        .map_err(|error| error.to_string())
                }
                Err(error) => Err(error.to_string()),
            };
            if let Err(error) = collapsed {
                crate::bridge::emit(
                    &app,
                    grain_core::DaemonEvent::PasteError {
                        error: format!(
                            "Could not preserve the selected text before insert: {error}"
                        ),
                    },
                );
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(35));
            #[cfg(windows)]
            if !target
                .window
                .is_some_and(crate::agent::captured_window_is_foreground)
            {
                crate::bridge::emit(
                    &app,
                    grain_core::DaemonEvent::PasteError {
                        error: "The original app lost focus before Grain could insert the result."
                            .into(),
                    },
                );
                return;
            }
        }
        if let Err(error) = crate::clipboard::paste(message, app.clone()) {
            crate::bridge::emit(&app, grain_core::DaemonEvent::PasteError { error });
        }
    });
    #[cfg(windows)]
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn extension_view_close(
    app: AppHandle,
    window: WebviewWindow,
    session_id: u64,
) -> Result<(), String> {
    if !caller_is_view(&window) {
        return Err("extension view command called from the wrong window".into());
    }
    let removed = {
        let mut slot = active().lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|active| active.session_id == session_id)
        {
            slot.take()
        } else {
            None
        }
    };
    let Some(removed) = removed else {
        return Err("stale extension view session".into());
    };
    if let Err(error) = window.destroy() {
        // A failed destroy leaves the live renderer able to retry. Do not turn
        // it into an orphaned, unauthenticated window by dropping its session.
        *active().lock().unwrap() = Some(removed);
        return Err(error.to_string());
    }
    output_target().lock().unwrap().take();
    notify_closed(&app, &removed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_messages_are_bounded_by_unicode_scalar_count() {
        let long = "é".repeat(9000);
        let bounded = bounded_message(&long);
        assert_eq!(bounded.chars().count(), 8192);
        assert!(bounded.is_char_boundary(bounded.len()));
    }

    #[test]
    fn request_previews_are_trimmed_and_bounded() {
        let request = format!("  {}  ", "é".repeat(REQUEST_PREVIEW_MAX_CHARS + 100));
        let preview = bounded_request_preview(&request);
        assert_eq!(preview.chars().count(), REQUEST_PREVIEW_MAX_CHARS);
        assert!(!preview.starts_with(char::is_whitespace));
        assert!(!preview.ends_with(char::is_whitespace));
    }

    #[test]
    fn unavailable_state_is_actionable_bounded_and_self_releasing() {
        let content = unavailable_content(&"x".repeat(9000)).unwrap();
        let ExtensionViewContent::Result {
            message,
            tone,
            can_copy,
            can_insert,
            can_replace,
            can_open_extensions,
            dismiss_after_ms,
        } = content
        else {
            panic!("unavailable guidance must be a finite result")
        };
        assert_eq!(message.chars().count(), 8192);
        assert_eq!(tone, ResultTone::Warning);
        assert!(!can_copy && !can_insert && !can_replace);
        assert!(can_open_extensions);
        assert_eq!(dismiss_after_ms, Some(UNAVAILABLE_DISMISS_MS));
    }
}
