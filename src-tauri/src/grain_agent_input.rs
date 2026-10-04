//! Focusable Agent input exists only while typing. The recording overlay stays
//! nonactivating; Handy's host lifecycle is unchanged.
use crate::agent::AgentState;
use serde::Serialize;
use specta::Type;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_specta::Event;

pub const INPUT_LABEL: &str = "agent-input";
const WIDTH: f64 = 392.0;
const HEIGHT: f64 = 186.0;

#[derive(Serialize, Type)]
pub struct AgentInputSnapshot {
    pub generation: u64,
    pub selection_chars: usize,
    pub quick: bool,
    pub ready: bool,
    pub pill: crate::grain_overlay::OverlaySnapshot,
}

fn session_matches(active: bool, current: u64, expected: u64) -> bool {
    active && current == expected
}

fn require_session(app: &AppHandle, generation: u64) -> Result<(), String> {
    let state = app.state::<AgentState>();
    if session_matches(
        state.input_active.load(Ordering::SeqCst),
        state.summon_gen.load(Ordering::SeqCst),
        generation,
    ) {
        Ok(())
    } else {
        Err("This Agent input has ended.".into())
    }
}

fn require_typing(window: &WebviewWindow, app: &AppHandle, generation: u64) -> Result<(), String> {
    if window.label() != INPUT_LABEL
        || !app
            .state::<AgentState>()
            .input_typing_active
            .load(Ordering::SeqCst)
    {
        return Err("Agent typing is not active.".into());
    }
    require_session(app, generation)
}

async fn on_main<T: Send + 'static>(
    app: AppHandle,
    action: impl FnOnce(&AppHandle) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = send.send(action(&handle));
    })
    .map_err(|e| e.to_string())?;
    receive.await.map_err(|e| e.to_string())?
}

/// Second summon is the keyboard-accessible alternative to the pill button.
pub fn open(app: &AppHandle) {
    let generation = app.state::<AgentState>().summon_gen.load(Ordering::SeqCst);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = open_owned(handle, generation).await {
            log::warn!("[GRAIN] Agent typing could not open: {error}");
        }
    });
}

async fn on_worker<T: Send + 'static>(
    action: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(action)
        .await
        .map_err(|e| e.to_string())?
}

async fn open_owned(app: AppHandle, generation: u64) -> Result<(), String> {
    let (window, created) = on_main(app.clone(), move |app| open_main(app, generation)).await?;
    if !created {
        return Ok(());
    }
    let handle = app.clone();
    on_worker(move || {
        let _guard = crate::grain_actions::capture_start_guard();
        require_typing(&window, &handle, generation)?;
        crate::agent::input_typing(&handle, true);
        require_session(&handle, generation)?;
        handle
            .state::<AgentState>()
            .input_typing_ready
            .store(true, Ordering::SeqCst);
        let _ = handle.emit_to(INPUT_LABEL, crate::grain_events::AgentInputReady::NAME, ());
        Ok(())
    })
    .await?;
    Ok(())
}

// Anchor the typing card to the actual recording window, preserving its monitor
// and screen edge. Physical coordinates avoid mixed-DPI monitor-origin errors.
fn anchored_origin(
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    new_width: f64,
    new_height: f64,
    bottom: bool,
) -> (f64, f64) {
    (
        left + (width - new_width) / 2.0,
        if bottom {
            top + height - new_height
        } else {
            top
        },
    )
}

fn open_main(app: &AppHandle, generation: u64) -> Result<(WebviewWindow, bool), String> {
    require_session(app, generation)?;
    if let Some(window) = app.get_webview_window(INPUT_LABEL) {
        if app
            .state::<AgentState>()
            .input_typing_ready
            .load(Ordering::SeqCst)
        {
            crate::agent::show_and_focus(&window);
        }
        return Ok((window, false));
    }
    let overlay = app
        .get_webview_window("recording_overlay")
        .ok_or("Recording window is unavailable.")?;
    let monitor = overlay
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("No display available.")?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let width = WIDTH.min(area.size.width as f64 / scale);
    let height = HEIGHT.min(area.size.height as f64 / scale);
    let position = overlay.outer_position().map_err(|e| e.to_string())?;
    let size = overlay.outer_size().map_err(|e| e.to_string())?;
    let (x, y) = anchored_origin(
        position.x as f64,
        position.y as f64,
        size.width as f64,
        size.height as f64,
        width * scale,
        height * scale,
        crate::settings::get_settings(app).overlay_position
            == crate::settings::OverlayPosition::Bottom,
    );
    let x = x.clamp(
        area.position.x as f64,
        (area.position.x as f64 + area.size.width as f64 - width * scale)
            .max(area.position.x as f64),
    );
    let y = y.clamp(
        area.position.y as f64,
        (area.position.y as f64 + area.size.height as f64 - height * scale)
            .max(area.position.y as f64),
    );
    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        INPUT_LABEL,
        tauri::WebviewUrl::App(format!("agent-input.html?generation={generation}").into()),
    )
    .title("Grain Agent")
    .inner_size(width, height)
    .position(x / scale, y / scale)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .focused(false)
    .visible(false);
    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }
    let window = builder.build().map_err(|e| e.to_string())?;
    // Reassert physical coordinates after creation crosses a DPI boundary.
    let placement = window
        .set_size(tauri::PhysicalSize::new(
            (width * scale).round() as u32,
            (height * scale).round() as u32,
        ))
        .and_then(|_| {
            window.set_position(tauri::PhysicalPosition::new(
                x.round() as i32,
                y.round() as i32,
            ))
        });
    if let Err(error) = placement {
        let _ = window.close();
        return Err(error.to_string());
    }
    // A global voice submit/cancel can finish while WebView construction runs.
    // It must not leave a new visible input behind after the session ended.
    if let Err(error) = require_session(app, generation) {
        let _ = window.close();
        return Err(error);
    }
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed)
            && require_session(&handle, generation).is_ok()
            && handle
                .state::<AgentState>()
                .input_typing_active
                .load(Ordering::SeqCst)
        {
            let handle = handle.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _guard = crate::grain_actions::capture_start_guard();
                if require_session(&handle, generation).is_ok()
                    && handle
                        .state::<AgentState>()
                        .input_typing_active
                        .load(Ordering::SeqCst)
                {
                    crate::agent::input_cancel(&handle);
                }
            });
        }
    });
    app.state::<AgentState>()
        .input_typing_active
        .store(true, Ordering::SeqCst);
    app.state::<AgentState>()
        .input_typing_ready
        .store(false, Ordering::SeqCst);
    Ok((window, true))
}

/// Clear ownership before destruction so deliberate handoff does not cancel it.
pub fn close(app: &AppHandle) {
    crate::agent::release_typing_shortcut(app);
    app.state::<AgentState>()
        .input_typing_active
        .store(false, Ordering::SeqCst);
    app.state::<AgentState>()
        .input_typing_ready
        .store(false, Ordering::SeqCst);
    // Capture the actual window before queueing; an old close must not resolve
    // the label again and destroy a replacement created by a newer summon.
    if let Some(window) = app.get_webview_window(INPUT_LABEL) {
        let _ = app.run_on_main_thread(move || {
            let _ = window.close();
        });
    }
}

#[tauri::command]
#[specta::specta]
pub fn agent_input_snapshot(
    app: AppHandle,
    window: WebviewWindow,
    generation: u64,
) -> Result<AgentInputSnapshot, String> {
    require_typing(&window, &app, generation)?;
    let state = app.state::<AgentState>();
    let selection_chars = state
        .context
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .map_or(0, |text| text.chars().count());
    Ok(AgentInputSnapshot {
        generation,
        selection_chars,
        quick: crate::settings::get_settings(&app).agent_quick_enabled,
        ready: state.input_typing_ready.load(Ordering::SeqCst),
        pill: crate::grain_overlay::overlay_snapshot(app.clone()),
    })
}

/// The loaded UI has listeners and a painted compact state before handing off.
#[tauri::command]
#[specta::specta]
pub async fn agent_input_reveal(
    app: AppHandle,
    window: WebviewWindow,
    generation: u64,
) -> Result<(), String> {
    on_main(app, move |app| {
        require_typing(&window, app, generation)?;
        if !app
            .state::<AgentState>()
            .input_typing_ready
            .load(Ordering::SeqCst)
        {
            return Err("Agent input is preparing.".into());
        }
        crate::agent::unregister_transient_shortcuts(app);
        crate::agent::show_and_focus(&window);
        crate::grain_overlay::hide_agent_voice(app);
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn agent_input_submit(
    app: AppHandle,
    window: WebviewWindow,
    generation: u64,
    text: String,
    quick: bool,
) -> Result<(), String> {
    on_worker(move || {
        let _guard = crate::grain_actions::capture_start_guard();
        require_typing(&window, &app, generation)?;
        if text.trim().is_empty() {
            return Err("Enter an instruction first.".into());
        }
        crate::agent::input_submit_text(&app, text, quick);
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn agent_input_speak(
    app: AppHandle,
    window: WebviewWindow,
    generation: u64,
) -> Result<(), String> {
    on_worker(move || {
        let _guard = crate::grain_actions::capture_start_guard();
        require_typing(&window, &app, generation)?;
        close(&app);
        crate::agent::input_typing(&app, false);
        if require_session(&app, generation).is_ok() {
            crate::agent::register_transient_shortcuts(&app);
        }
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn agent_input_cancel(
    app: AppHandle,
    window: WebviewWindow,
    generation: u64,
) -> Result<(), String> {
    on_worker(move || {
        let _guard = crate::grain_actions::capture_start_guard();
        require_typing(&window, &app, generation)?;
        crate::agent::input_cancel(&app);
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_or_completed_typing_cannot_control_another_summon() {
        assert!(session_matches(true, 7, 7));
        assert!(!session_matches(false, 7, 7));
        assert!(!session_matches(true, 8, 7));
    }
    #[test]
    fn tab_is_supported_on_both_backends_and_never_registered_at_idle() {
        assert!("tab"
            .parse::<tauri_plugin_global_shortcut::Shortcut>()
            .is_ok());
        assert!("tab".parse::<handy_keys::Hotkey>().is_ok());
        let settings = crate::settings::get_default_settings();
        assert!(!grain_core::capture::shortcut_holds_hotkey(
            &settings,
            "agent_type"
        ));
    }
    #[test]
    fn typing_preserves_overlay_center_and_edge_in_physical_coordinates() {
        assert_eq!(
            anchored_origin(-1800.0, 900.0, 400.0, 100.0, 588.0, 312.0, true),
            (-1894.0, 688.0)
        );
        assert_eq!(
            anchored_origin(-1800.0, 900.0, 400.0, 100.0, 588.0, 312.0, false),
            (-1894.0, 900.0)
        );
    }
}
