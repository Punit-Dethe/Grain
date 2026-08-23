//! Grain-rendered standard UI for Extension Mode.
//!
//! The selected extension supplies only a validated [`grain_sdk::ExtensionView`]
//! tree. Grain owns the webview, DOM, components, styling, focus, trusted action
//! bar, and lifecycle; no extension HTML, CSS, script, token, or Tauri authority
//! enters this window.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use grain_sdk::{ExtensionView, ExtensionViewEvent};
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

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct ActiveView {
    session_id: u64,
    extension_id: String,
    extension_name: String,
    request: String,
    content: ExtensionViewContent,
    busy: bool,
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

#[derive(Clone, Debug, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionViewContent {
    View {
        view: ExtensionView,
    },
    Result {
        message: String,
        tone: ResultTone,
        can_insert: bool,
        can_replace: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ResultTone {
    Success,
    Warning,
    Danger,
}

fn result_content(message: String, tone: ResultTone, allow_output: bool) -> ExtensionViewContent {
    let target = *output_target().lock().unwrap();
    ExtensionViewContent::Result {
        message,
        tone,
        can_insert: allow_output && target.is_some_and(target_is_usable),
        can_replace: allow_output && target.is_some_and(|target| target.has_selection),
    }
}

fn target_is_usable(target: OutputTarget) -> bool {
    target.window.is_some()
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionViewInit {
    pub session_id: u64,
    pub extension_id: String,
    pub extension_name: String,
    pub content: ExtensionViewContent,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionViewEventResult {
    /// A `change` handler that returns nothing keeps the user's local field
    /// state instead of rehydrating the author tree's original defaults.
    pub unchanged: bool,
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

/// Pre-create the powerless host renderer hidden behind the active capture. A
/// repeated call is free; a cancelled capture destroys it through [`destroy`].
pub fn warm(app: &AppHandle) -> Result<(), String> {
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
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            let removed = active().lock().unwrap().take();
            if removed.is_some() {
                output_target().lock().unwrap().take();
            }
            if let Some(removed) = removed
                .filter(|removed| matches!(&removed.content, ExtensionViewContent::View { .. }))
            {
                action_log::record(
                    &removed.request,
                    Some(removed.extension_id.clone()),
                    None,
                    None,
                    ActionLogOutcome::Refused {
                        reason: "user cancelled confirmation".into(),
                    },
                );
                crate::extension_host::notify_surface_cancel(
                    &removed.extension_id,
                    removed.session_id,
                );
            }
        }
    });
    log::debug!("[GRAIN] extension view: renderer warmed");
    Ok(())
}

fn store_and_emit(app: &AppHandle, active_view: ActiveView) -> Result<u64, String> {
    let init = ExtensionViewInit::from(&active_view);
    let session_id = active_view.session_id;
    *active().lock().unwrap() = Some(active_view);
    if let Err(error) = warm(app) {
        active().lock().unwrap().take();
        return Err(error);
    }
    if let Err(error) = app.emit_to(LABEL, PRESENT_EVENT, init) {
        destroy(app);
        return Err(error.to_string());
    }
    Ok(session_id)
}

/// Present an extension-authored, Grain-rendered component tree.
pub fn present(
    app: &AppHandle,
    extension_id: &str,
    request: &str,
    view: ExtensionView,
) -> Result<u64, String> {
    view.validate()?;
    let name = extension_name(app, extension_id);
    store_and_emit(
        app,
        ActiveView {
            session_id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            extension_id: extension_id.to_string(),
            extension_name: name,
            request: request.to_string(),
            content: ExtensionViewContent::View { view },
            busy: false,
        },
    )
}

/// Show a finite host-owned outcome. Used when an extension returns a message
/// without first opening an editable view, and after a view action completes.
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
            extension_id: extension_id.to_string(),
            extension_name: name,
            // The hand-off path has already logged the request and outcome.
            request: String::new(),
            content: result_content(message, tone, tone == ResultTone::Success),
            busy: false,
        },
    )
}

/// Show a host-owned success notice that is not extension output and therefore
/// must never expose Insert/Replace (the Auto-send disclosure uses this).
pub fn present_notice(app: &AppHandle, extension_id: &str, message: String) -> Result<u64, String> {
    let message = bounded_message(&message);
    if message.is_empty() {
        return Err("notice message must not be empty".into());
    }
    let name = extension_name(app, extension_id);
    store_and_emit(
        app,
        ActiveView {
            session_id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            extension_id: extension_id.to_string(),
            extension_name: name,
            request: String::new(),
            content: result_content(message, ResultTone::Success, false),
            busy: false,
        },
    )
}

/// Tear down the webview and all retained extension display state. Safe on every
/// capture/cancel path; the hidden warm renderer must never become residency.
pub fn destroy(app: &AppHandle) {
    active().lock().unwrap().take();
    output_target().lock().unwrap().take();
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

/// Keep the owning worker out of the idle reaper while its standard view is
/// visible. Closing the finite view restores the normal idle policy.
pub fn owns_extension(extension_id: &str) -> bool {
    active().lock().unwrap().as_ref().is_some_and(|active| {
        active.extension_id == extension_id
            && matches!(&active.content, ExtensionViewContent::View { .. })
    })
}

/// Drop a standard view whose worker is being disabled, removed, or reloaded.
pub fn destroy_for_extension(app: &AppHandle, extension_id: &str) {
    let belongs_to_extension = active()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|active| active.extension_id == extension_id);
    if belongs_to_extension {
        destroy(app);
    }
}

/// A crashed/reaped worker invalidates an interactive view, but a finite result
/// no longer depends on that worker and must remain copyable until the user
/// closes it.
pub fn fail_interactive_for_extension(app: &AppHandle, extension_id: &str, reason: &str) {
    let init = {
        let mut slot = active().lock().unwrap();
        let Some(active) = slot.as_mut().filter(|active| {
            active.extension_id == extension_id
                && matches!(&active.content, ExtensionViewContent::View { .. })
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
        );
        active.busy = false;
        action_log::record(
            &active.request,
            Some(active.extension_id.clone()),
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
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn extension_view_event(
    app: AppHandle,
    window: WebviewWindow,
    session_id: u64,
    event: ExtensionViewEvent,
) -> Result<ExtensionViewEventResult, String> {
    if !caller_is_view(&window) {
        return Err("extension view command called from the wrong window".into());
    }
    let extension_id = {
        let mut slot = active().lock().unwrap();
        let active = slot
            .as_mut()
            .filter(|active| active.session_id == session_id)
            .ok_or("stale extension view session")?;
        if active.busy {
            return Err("an extension view event is already in flight".into());
        }
        let ExtensionViewContent::View { view } = &active.content else {
            return Err("this extension view has already completed".into());
        };
        view.validate_event(&event)?;
        active.busy = true;
        active.extension_id.clone()
    };

    let outcome =
        crate::extension_host::surface_event(&app, &extension_id, session_id, event).await;
    let mut slot = active().lock().unwrap();
    let active = slot
        .as_mut()
        .filter(|active| active.session_id == session_id)
        .ok_or("stale extension view session")?;
    active.busy = false;
    if matches!(&active.content, ExtensionViewContent::Result { .. }) {
        return Ok(ExtensionViewEventResult {
            unchanged: false,
            content: active.content.clone(),
        });
    }

    let unchanged = matches!(
        &outcome,
        crate::extension_host::SurfaceEventOutcome::Unchanged
    );
    match outcome {
        crate::extension_host::SurfaceEventOutcome::View(next) => {
            next.validate()?;
            active.content = ExtensionViewContent::View { view: next };
        }
        crate::extension_host::SurfaceEventOutcome::Unchanged => {}
        crate::extension_host::SurfaceEventOutcome::Done(message) => {
            let message = message
                .filter(|message| !message.trim().is_empty())
                .map(|message| bounded_message(&message))
                .unwrap_or_else(|| "Done".into());
            active.content = result_content(message, ResultTone::Success, true);
            action_log::record(
                &active.request,
                Some(active.extension_id.clone()),
                None,
                None,
                ActionLogOutcome::Ran { confirmed: true },
            );
            active.request.clear();
        }
        crate::extension_host::SurfaceEventOutcome::Failed(reason) => {
            let reason = bounded_message(&reason);
            active.content = result_content(reason.clone(), ResultTone::Danger, false);
            action_log::record(
                &active.request,
                Some(active.extension_id.clone()),
                None,
                None,
                ActionLogOutcome::Failed { reason },
            );
            active.request.clear();
        }
        crate::extension_host::SurfaceEventOutcome::Unknown => {
            active.content = result_content(
                "Grain stopped waiting, so the extension's final outcome is unknown.".into(),
                ResultTone::Warning,
                false,
            );
            action_log::record(
                &active.request,
                Some(active.extension_id.clone()),
                None,
                None,
                ActionLogOutcome::Unknown,
            );
            active.request.clear();
        }
    }
    Ok(ExtensionViewEventResult {
        unchanged,
        content: active.content.clone(),
    })
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
            ExtensionViewContent::Result { message, .. } => Some(message.clone()),
            ExtensionViewContent::View { .. } => None,
        })
        .ok_or("there is no result to copy")?;
    app.clipboard()
        .write_text(message)
        .map_err(|error| format!("failed to copy result: {error}"))
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
pub fn extension_view_close(window: WebviewWindow, session_id: u64) -> Result<(), String> {
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
    if matches!(&removed.content, ExtensionViewContent::View { .. }) {
        action_log::record(
            &removed.request,
            Some(removed.extension_id.clone()),
            None,
            None,
            ActionLogOutcome::Refused {
                reason: "user cancelled confirmation".into(),
            },
        );
        crate::extension_host::notify_surface_cancel(&removed.extension_id, session_id);
    }
    window.destroy().map_err(|error| error.to_string())?;
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
}
