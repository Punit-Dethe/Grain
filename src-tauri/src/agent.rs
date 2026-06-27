//! [GRAIN] The Agent — a summoned, voice-first AI scratchpad in its own
//! destroyable windows ("if it's not in use, destroy it").
//!
//! Two surfaces (faithful to the reference design):
//!   • PALETTE — a centred summon bar that records by default; type to override,
//!     Enter to submit. It captures the foreground selection (synthesised copy +
//!     clipboard diff) and shows only its char count, never the full text. On
//!     submit it hands the instruction to the panel and closes.
//!   • PANEL — a right-side conversation showing the reply (auto-copied) plus a
//!     follow-up input.
//!
//! The conversation is sent to the SAME AI the post-processing layer uses (single
//! provider, or the smart-rotation pool with failover + daily quota).
//!
//! Everything here is headless-friendly: it reads the owned settings, reuses the
//! STT dispatcher (`stt_router`) and the LLM rotation infra (`post_process_router`
//! + `rotation_state`), and never assumes a UI is alive.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use grain_core::PostProcessProvider;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::input::EnigoState;
use crate::llm_client::LlmError;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::rotation_state::{
    now_secs, record_outcome, select_order, CallOutcome, RotationTrackers,
};
use crate::settings::{get_settings, ShortcutBinding, APPLE_INTELLIGENCE_PROVIDER_ID};

/// Window labels (matched by their capability + the frontend router in
/// `main.tsx`). The Agent is two faithful surfaces:
///   • the PALETTE (`agent`) — a centred summon bar that records by default; type
///     to override, Enter to submit. It vanishes on submit.
///   • the PANEL (`agent-panel`) — a right-side conversation showing the reply +
///     a follow-up input.
pub const PALETTE_LABEL: &str = "agent";
pub const PANEL_LABEL: &str = "agent-panel";

/// Recording binding id used for the palette's dictation (kept distinct from the
/// global transcribe bindings so the two never alias in the recorder).
const AGENT_BINDING: &str = "agent";
const AGENT_SUBMIT_BINDING: &str = "agent_submit";
const AGENT_CLOSE_BINDING: &str = "agent_close";
const AGENT_LLM_TIMEOUT: Duration = Duration::from_secs(120);

/// Palette geometry (logical px): a fixed centred bar near the upper third.
const PALETTE_W: f64 = 620.0;
const PALETTE_H: f64 = 122.0;
/// Panel geometry (logical px): a fixed right-edge sidebar, vertically centred.
const PANEL_W: f64 = 500.0;
const PANEL_MARGIN: f64 = 18.0;

/// The Agent's system instruction. The user's dictated/typed instruction is the
/// task; the selected text (if any) is supplied as context separately.
const AGENT_SYSTEM_PROMPT: &str = "You are Grain's built-in assistant. The user acts on text they have selected and on what they dictate or type. Follow their instruction precisely and reply with ONLY the result they asked for — no preamble, no sign-off, no meta commentary. Do not wrap the answer in markdown code fences unless the user explicitly asks for code. When they ask you to rewrite, summarise, translate, fix, shorten, or reformat the selected text, operate on that text. Keep answers tight and useful.";

/// Cross-window state, set at summon and handed off palette → panel.
#[derive(Default)]
pub struct AgentState {
    /// Selection captured at summon: the palette shows only its char count (never
    /// the full text), the panel uses it as the LLM context. Non-consuming;
    /// overwritten on each summon.
    pub context: Mutex<Option<String>>,
    /// First instruction handed from the palette to the panel on submit.
    pub pending_instruction: Mutex<Option<String>>,
}

/// One conversation turn from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentMessage {
    /// `"user"` or `"assistant"` (anything else is treated as `"user"`).
    pub role: String,
    pub content: String,
}

// ============================================================================
// Summon + windows
// ============================================================================

/// Capture the foreground app's current selection, then open the centred palette.
/// Runs the capture off the hotkey thread (so it never blocks the input listener)
/// and creates the window on the main thread (where Tauri wants window ops).
pub fn summon(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let palette_open = app.get_webview_window(PALETTE_LABEL).is_some();

        // Only capture for a FRESH summon. If the palette is already up the user is
        // interacting with the Agent, not a source app, so a synthetic Ctrl+C would
        // copy nothing useful (and could clobber their selection).
        if !palette_open {
            let c = capture_selection(&app);
            if let Some(state) = app.try_state::<AgentState>() {
                if let Ok(mut g) = state.context.lock() {
                    *g = c;
                }
            }
        }

        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            // A new summon starts a fresh session — close any open conversation.
            if let Some(panel) = app2.get_webview_window(PANEL_LABEL) {
                let _ = panel.close();
            }
            show_palette(&app2);
        });
    });
}

/// Create the palette window if needed, then show + focus it (destroyed on close,
/// so this recreates it — mirrors `show_main_window`).
fn show_palette(app: &AppHandle) {
    register_transient_shortcuts(app);

    let win = match app.get_webview_window(PALETTE_LABEL) {
        Some(w) => w,
        None => match build_window(app, PALETTE_LABEL, PALETTE_W, PALETTE_H) {
            Ok(w) => {
                place_palette(&w); // position before first paint
                w
            }
            Err(e) => {
                log::error!("[GRAIN] failed to build agent palette: {e}");
                return;
            }
        },
    };
    show_and_focus(&win);
}

/// Show + reliably grab keyboard focus. A hotkey-summoned, always-on-top, frameless
/// window is subject to Windows' foreground lock: it appears on top but keyboard
/// focus stays with the previous app, so typing/Enter/Esc go nowhere. We bridge the
/// foreground thread's input queue to ours, force the window foreground, then detach.
fn show_and_focus(win: &tauri::WebviewWindow) {
    let _ = win.show();
    focus_now(win);

    // Hotkey-summoned windows can briefly lose focus again while the previous
    // foreground app processes the key release. Re-focus shortly after first
    // paint. A SINGLE retry task handles every delay (instead of one detached
    // thread per delay), and each step re-resolves the window by label and runs
    // the focus work on the main thread — so a window closed in the meantime
    // (e.g. the palette being handed off to the panel) is a no-op rather than a
    // focus call racing `close()`.
    let app = win.app_handle().clone();
    let label = win.label().to_string();
    std::thread::spawn(move || {
        for delay_ms in [60_u64, 180] {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            let app = app.clone();
            let label = label.clone();
            let _ = app.clone().run_on_main_thread(move || {
                if let Some(w) = app.get_webview_window(&label) {
                    focus_now(&w);
                }
            });
        }
    });
}

/// Pull a window to the foreground and grab keyboard focus right now (on the
/// calling thread). On Windows this also bridges the foreground input queue.
fn focus_now(win: &tauri::WebviewWindow) {
    let _ = win.set_focus();
    #[cfg(windows)]
    force_foreground(win);
}

#[cfg(windows)]
fn force_foreground(win: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
        ShowWindow, SW_SHOW,
    };

    let Ok(raw) = win.hwnd() else { return };
    let hwnd = HWND(raw.0 as _);
    unsafe {
        let fg = GetForegroundWindow();
        let our_tid = GetCurrentThreadId();
        let fg_tid = GetWindowThreadProcessId(fg, None);
        // Attaching a thread to itself is an error; only bridge across processes.
        let attached =
            fg_tid != 0 && fg_tid != our_tid && AttachThreadInput(fg_tid, our_tid, true).as_bool();
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        if attached {
            let _ = AttachThreadInput(fg_tid, our_tid, false);
        }
    }
}

/// Build a frameless, transparent, always-on-top Agent surface (hidden until
/// placed). Shared by the palette and the panel; both are excluded from the main
/// window's aspect-ratio lock and are destroyed on close.
fn build_window(
    app: &AppHandle,
    label: &str,
    w: f64,
    h: f64,
) -> tauri::Result<tauri::WebviewWindow> {
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("/".into()))
            .title("Grain Assist")
            .inner_size(w, h)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(true)
            .shadow(false)
            .visible(false);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    builder.build()
}

/// Monitor metrics in LOGICAL px as `(origin_x, origin_y, screen_w, screen_h)`.
fn monitor_logical(window: &tauri::WebviewWindow) -> Option<(f64, f64, f64, f64)> {
    let monitor = match window.current_monitor() {
        Ok(Some(m)) => Some(m),
        _ => window.primary_monitor().ok().flatten(),
    }?;
    let scale = monitor.scale_factor();
    let s = monitor.size();
    let p = monitor.position();
    Some((
        p.x as f64 / scale,
        p.y as f64 / scale,
        s.width as f64 / scale,
        s.height as f64 / scale,
    ))
}

/// Centre the palette horizontally, near the upper third of the screen.
fn place_palette(window: &tauri::WebviewWindow) {
    if let Some((ox, oy, sw, sh)) = monitor_logical(window) {
        let x = ox + (sw - PALETTE_W) / 2.0;
        let y = oy + sh * 0.32;
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    }
}

/// Anchor the panel to the right edge, vertically centred, a bit shorter than the
/// screen (matches the reference sidebar footprint).
fn place_panel(window: &tauri::WebviewWindow) {
    if let Some((ox, oy, sw, sh)) = monitor_logical(window) {
        let h = (sh - 90.0).clamp(360.0, 880.0);
        let _ = window.set_size(tauri::LogicalSize::new(PANEL_W, h));
        let x = ox + sw - PANEL_W - PANEL_MARGIN;
        let y = oy + (sh - h) / 2.0;
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    }
}

/// Synthesise a platform copy and read the resulting selection off the clipboard,
/// restoring the user's original clipboard afterwards (the capture is invisible).
/// Returns `None` if nothing usable was selected, input simulation is unavailable,
/// or the clipboard didn't change.
fn capture_selection(app: &AppHandle) -> Option<String> {
    let enigo_state = app.try_state::<EnigoState>()?;
    let clipboard = app.clipboard();
    let saved = clipboard.read_text().ok();

    {
        let mut enigo = enigo_state.0.lock().ok()?;
        crate::input::release_modifiers(&mut enigo);
        std::thread::sleep(std::time::Duration::from_millis(40));
        if let Err(e) = crate::input::send_copy_ctrl_c(&mut enigo) {
            warn!("[GRAIN] agent: simulated copy failed: {e}");
            return None;
        }
    } // release the enigo lock before sleeping/polling

    // The target app may write the clipboard asynchronously — poll until it
    // changes (selection differs from the prior clipboard) or we time out.
    let mut captured = None;
    for _ in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(45));
        let now = clipboard.read_text().ok();
        if now.is_some() && now != saved {
            captured = now;
            break;
        }
    }

    // Restore the user's clipboard regardless of outcome.
    if let Some(prev) = saved {
        let _ = clipboard.write_text(prev);
    }

    captured
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ============================================================================
// Commands
// ============================================================================

/// The selection captured at summon (non-consuming): the palette reads its length
/// for the char-count chip, the panel reads the text as LLM context.
#[tauri::command]
#[specta::specta]
pub fn agent_get_context(app: AppHandle) -> Option<String> {
    app.try_state::<AgentState>()
        .and_then(|s| s.context.lock().ok().and_then(|g| g.clone()))
}

/// Palette → panel handoff: store the first instruction the panel will run.
#[tauri::command]
#[specta::specta]
pub fn agent_set_instruction(app: AppHandle, text: String) -> Result<(), String> {
    if let Some(s) = app.try_state::<AgentState>() {
        if let Ok(mut g) = s.pending_instruction.lock() {
            *g = Some(text);
        }
    }
    Ok(())
}

fn set_pending_instruction(app: &AppHandle, text: String) {
    if let Some(s) = app.try_state::<AgentState>() {
        if let Ok(mut g) = s.pending_instruction.lock() {
            *g = Some(text);
        }
    }
}

/// Consume the first instruction (the panel calls this on mount).
#[tauri::command]
#[specta::specta]
pub fn agent_take_instruction(app: AppHandle) -> Option<String> {
    app.try_state::<AgentState>()
        .and_then(|s| s.pending_instruction.lock().ok().and_then(|mut g| g.take()))
}

/// Open (or focus) the conversation panel — called by the palette on submit.
#[tauri::command]
#[specta::specta]
pub fn agent_show_panel(app: AppHandle) -> Result<(), String> {
    show_panel(&app)
}

fn show_panel(app: &AppHandle) -> Result<(), String> {
    register_transient_shortcuts(app);

    info!("[GRAIN] agent: showing panel");
    let win = match app.get_webview_window(PANEL_LABEL) {
        Some(w) => w,
        None => {
            info!("[GRAIN] agent: building panel window");
            let w = build_window(app, PANEL_LABEL, PANEL_W, 600.0)
                .map_err(|e| format!("failed to build agent panel: {e}"))?;
            info!("[GRAIN] agent: panel window built");
            place_panel(&w);
            info!("[GRAIN] agent: panel window placed");
            w
        }
    };
    show_and_focus(&win);
    unregister_submit_shortcut_deferred(app);
    info!("[GRAIN] agent: panel shown");
    Ok(())
}

/// Atomically hand the palette instruction to the panel and move window work to
/// the main thread. The panel is opened from a detached backend task instead of
/// the palette's IPC call stack: on Windows/WebView2, building a second agent
/// webview while the first one is still resolving its submit invoke can wedge at
/// `WebviewWindowBuilder::build()`, leaving the palette looking frozen.
#[tauri::command]
#[specta::specta]
pub fn agent_submit_instruction(app: AppHandle, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("Agent instruction is empty".to_string());
    }

    info!(
        "[GRAIN] agent: submitting instruction ({} chars)",
        text.chars().count()
    );
    set_pending_instruction(&app, text);

    let app_for_task = app.clone();
    std::thread::spawn(move || {
        let app_for_close = app_for_task.clone();
        let close_handle = app_for_close.clone();
        if let Err(e) = app_for_close.run_on_main_thread(move || {
            info!("[GRAIN] agent: closing palette before panel handoff");
            if let Some(palette) = close_handle.get_webview_window(PALETTE_LABEL) {
                let _ = palette.close();
            }
        }) {
            error!("[GRAIN] agent: failed to schedule palette close: {e:?}");
        }

        // Let the palette close event and the submit invoke get off the stack
        // before creating the panel. This keeps the submit call from being the
        // thing that also has to create a new webview.
        std::thread::sleep(Duration::from_millis(90));

        let app_for_panel = app_for_task.clone();
        let panel_handle = app_for_panel.clone();
        if let Err(e) = app_for_panel.run_on_main_thread(move || {
            if let Err(e) = show_panel(&panel_handle) {
                error!("[GRAIN] agent: failed to show panel: {e}");
            }
        }) {
            error!("[GRAIN] agent: failed to schedule agent panel: {e:?}");
        }
    });

    Ok(())
}

fn submit_binding() -> ShortcutBinding {
    ShortcutBinding {
        id: AGENT_SUBMIT_BINDING.to_string(),
        name: "Agent Submit".to_string(),
        description: "Submit the visible Agent palette.".to_string(),
        default_binding: "enter".to_string(),
        current_binding: "enter".to_string(),
    }
}

fn close_binding() -> ShortcutBinding {
    ShortcutBinding {
        id: AGENT_CLOSE_BINDING.to_string(),
        name: "Agent Close".to_string(),
        description: "Close the visible Agent surface.".to_string(),
        default_binding: "escape".to_string(),
        current_binding: "escape".to_string(),
    }
}

fn transient_bindings() -> [ShortcutBinding; 2] {
    [submit_binding(), close_binding()]
}

/// Register temporary global Enter/Escape while the Agent is visible. This
/// mirrors the old QML assist workflow and covers Windows focus loss, where the
/// palette is on screen but ordinary webview keydown events never arrive.
pub fn register_transient_shortcuts(app: &AppHandle) {
    for binding in transient_bindings() {
        let _ = crate::shortcut::unregister_shortcut(app, binding.clone());
        if let Err(e) = crate::shortcut::register_shortcut(app, binding.clone()) {
            warn!(
                "[GRAIN] agent: failed to register transient shortcut '{}': {}",
                binding.current_binding, e
            );
        }
    }
}

pub fn unregister_transient_shortcuts(app: &AppHandle) {
    for binding in transient_bindings() {
        let _ = crate::shortcut::unregister_shortcut(app, binding);
    }
}

pub fn unregister_transient_shortcuts_deferred(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || unregister_transient_shortcuts(&app));
}

fn unregister_submit_shortcut_deferred(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = crate::shortcut::unregister_shortcut(&app, submit_binding());
    });
}

/// Called by the transient global Enter shortcut. The frontend still owns typed
/// text, so this emits into the palette instead of trying to infer state in Rust.
pub fn global_submit(app: &AppHandle) {
    if app.get_webview_window(PALETTE_LABEL).is_some() {
        let _ = app.emit_to(PALETTE_LABEL, "agent-global-enter", ());
    }
}

/// Called by the transient global Escape shortcut. This is backend-owned so a
/// wedged webview can still be dismissed without quitting the whole app.
pub fn global_close(app: &AppHandle) {
    let app_for_main = app.clone();
    let _ = app.run_on_main_thread(move || {
        app_for_main
            .state::<Arc<AudioRecordingManager>>()
            .cancel_recording();

        if let Some(palette) = app_for_main.get_webview_window(PALETTE_LABEL) {
            let _ = palette.close();
        } else if let Some(panel) = app_for_main.get_webview_window(PANEL_LABEL) {
            let _ = panel.close();
        }

        if app_for_main.get_webview_window(PALETTE_LABEL).is_none()
            && app_for_main.get_webview_window(PANEL_LABEL).is_none()
        {
            unregister_transient_shortcuts_deferred(&app_for_main);
        }
    });
}

/// Start recording for in-window dictation, warming the local model/VAD when the
/// STT path is local so the transcript is ready quickly on stop.
#[tauri::command]
#[specta::specta]
pub fn agent_start_dictation(app: AppHandle) -> Result<(), String> {
    let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());

    if !get_settings(&app).stt_smart_rotation {
        let tm = app.state::<Arc<TranscriptionManager>>();
        tm.initiate_model_load();
    }
    {
        let rm = Arc::clone(&rm);
        std::thread::spawn(move || {
            let _ = rm.preload_vad();
        });
    }

    rm.try_start_recording(AGENT_BINDING)
}

/// Stop dictation and return the transcript (routed through the STT dispatcher —
/// local or cloud rotation per settings). Empty string if nothing was recorded.
#[tauri::command]
#[specta::specta]
pub async fn agent_stop_dictation(app: AppHandle) -> Result<String, String> {
    let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    let samples = match rm.stop_recording(AGENT_BINDING) {
        Some(s) if !s.is_empty() => s,
        _ => {
            info!("[GRAIN] agent: stop dictation produced no samples");
            return Ok(String::new());
        }
    };
    info!(
        "[GRAIN] agent: transcribing dictation ({} samples)",
        samples.len()
    );
    let text = crate::stt_router::transcribe(&app, samples).await?;
    info!(
        "[GRAIN] agent: dictation transcript ready ({} chars)",
        text.chars().count()
    );
    Ok(text)
}

/// Cancel an in-progress dictation without transcribing.
#[tauri::command]
#[specta::specta]
pub fn agent_cancel_dictation(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<AudioRecordingManager>>().cancel_recording();
    Ok(())
}

/// Copy text to the clipboard (used for the auto-copy of the first reply and the
/// per-message copy buttons).
#[tauri::command]
#[specta::specta]
pub fn agent_copy(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("Failed to copy to clipboard: {e}"))
}

/// Run the conversation against the configured AI and return the assistant reply.
/// Uses the post-processing provider config: a single provider, or the smart
/// rotation pool (round-robin + daily quota + health-ordered failover).
#[tauri::command]
#[specta::specta]
pub async fn agent_run(
    app: AppHandle,
    messages: Vec<AgentMessage>,
    context: Option<String>,
) -> Result<String, String> {
    info!(
        "[GRAIN] agent: running AI request ({} messages, context: {})",
        messages.len(),
        if context
            .as_ref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false)
        {
            "yes"
        } else {
            "no"
        }
    );
    let settings = get_settings(&app);

    let full = build_messages(&messages, context.as_deref());

    if settings.post_process_smart_rotation {
        return agent_run_rotated(&app, &full).await;
    }

    let provider = settings
        .active_post_process_provider()
        .cloned()
        .ok_or("No AI provider is configured. Choose one in Post-Processing settings.")?;
    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    if model.trim().is_empty() {
        return Err(format!(
            "{} has no model configured. Set one in Post-Processing settings.",
            provider.label
        ));
    }
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    let http_client = app
        .try_state::<reqwest::Client>()
        .map(|s| s.inner().clone())
        .ok_or("Agent: shared HTTP client unavailable")?;

    match run_agent_once(&http_client, &provider, model, api_key, &full).await {
        CallOutcome::Ok { text, .. } => Ok(text),
        CallOutcome::RateLimited { .. } => Err(format!(
            "{} is rate-limited right now — try again shortly.",
            provider.label
        )),
        CallOutcome::Failed => Err(format!("{} could not produce a response.", provider.label)),
    }
}

/// Build the full message list: system prompt + optional selected-text context +
/// the conversation turns (normalising every role to user/assistant).
fn build_messages(messages: &[AgentMessage], context: Option<&str>) -> Vec<(String, String)> {
    let mut full: Vec<(String, String)> = Vec::with_capacity(messages.len() + 2);
    full.push(("system".to_string(), AGENT_SYSTEM_PROMPT.to_string()));

    if let Some(ctx) = context.map(str::trim).filter(|c| !c.is_empty()) {
        full.push((
            "system".to_string(),
            format!(
                "The user has selected the following text. Treat it as the subject of their instruction unless they say otherwise:\n\n{ctx}"
            ),
        ));
    }

    for m in messages {
        let role = if m.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        full.push((role.to_string(), m.content.clone()));
    }
    full
}

/// Smart-rotation path: health-ordered failover across eligible post-process
/// providers (those enabled, under daily quota, and with a model configured),
/// recording quota usage on success — exactly the post-processing strategy.
async fn agent_run_rotated(app: &AppHandle, full: &[(String, String)]) -> Result<String, String> {
    crate::post_process_router::reset_quota_if_new_day(app);
    let settings = get_settings(app); // re-read so quotas reflect any reset

    let eligible: Vec<PostProcessProvider> = crate::post_process_router::rotation_pool(&settings)
        .into_iter()
        .filter(|p| {
            settings
                .post_process_models
                .get(&p.id)
                .map(|m| !m.trim().is_empty())
                .unwrap_or(false)
        })
        .collect();
    if eligible.is_empty() {
        return Err(
            "Smart rotation is on, but no eligible AI providers have a model configured.".into(),
        );
    }

    let trackers = app
        .try_state::<Arc<RotationTrackers>>()
        .ok_or("RotationTrackers unavailable")?;

    let est_text: String = full
        .iter()
        .map(|(_, c)| c.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let est_tokens = provider_router::estimate_tokens(&est_text);
    let candidates: Vec<(String, String)> = eligible
        .iter()
        .map(|p| (p.id.clone(), p.base_url.clone()))
        .collect();
    let order = select_order(&trackers.llm, &candidates, est_tokens, now_secs());

    let Some(http_client) = app.try_state::<reqwest::Client>() else {
        return Err("Agent: shared HTTP client unavailable".into());
    };
    let http_client = http_client.inner().clone();

    let mut last_err = "All AI providers failed.".to_string();
    for id in &order {
        let Some(provider) = eligible.iter().find(|p| &p.id == id) else {
            continue;
        };
        let model = settings
            .post_process_models
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        let api_key = settings
            .post_process_api_keys
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();

        let outcome = run_agent_once(&http_client, provider, model, api_key, full).await;
        record_outcome(&trackers.llm, &provider.id, &outcome, now_secs());
        match outcome {
            CallOutcome::Ok { text, .. } => {
                crate::post_process_router::record_usage(app, &provider.id);
                log::info!("[GRAIN] agent routed to '{}'", provider.id);
                return Ok(text);
            }
            CallOutcome::RateLimited { .. } => last_err = format!("{} rate-limited", provider.id),
            CallOutcome::Failed => last_err = format!("{} failed", provider.id),
        }
    }
    Err(last_err)
}

/// Run ONE provider with already-resolved model/key. HTTP providers go through
/// `llm_client::send_chat`; Apple Intelligence (local, no HTTP) is flattened to a
/// single system+user prompt. Returns a [`CallOutcome`] so the rotation tracker
/// learns from it.
async fn run_agent_once(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    model: String,
    api_key: String,
    messages: &[(String, String)],
) -> CallOutcome {
    // Disable reasoning where it adds latency without helping (mirrors the
    // post-process path): custom servers + OpenRouter.
    let (reasoning_effort, reasoning) = match provider.id.as_str() {
        "custom" => (Some("none".to_string()), None),
        "openrouter" => (
            None,
            Some(crate::llm_client::ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
        ),
        _ => (None, None),
    };

    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            if !crate::apple_intelligence::check_apple_intelligence_availability() {
                return CallOutcome::Failed;
            }
            let (system, user) = flatten_for_single_prompt(messages);
            let token_limit = model.trim().parse::<i32>().unwrap_or(0);
            return match crate::apple_intelligence::process_text_with_system_prompt(
                &system,
                &user,
                token_limit,
            ) {
                Ok(result) if !result.trim().is_empty() => CallOutcome::Ok {
                    text: result,
                    remaining_requests: None,
                    remaining_tokens: None,
                    total_tokens: None,
                },
                _ => CallOutcome::Failed,
            };
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            return CallOutcome::Failed;
        }
    }

    let response = tokio::time::timeout(
        AGENT_LLM_TIMEOUT,
        crate::llm_client::send_chat(
            client,
            provider,
            api_key,
            &model,
            messages.to_vec(),
            reasoning_effort,
            reasoning,
        ),
    )
    .await;

    match response {
        Err(_) => {
            warn!(
                "[GRAIN] agent provider '{}' timed out after {}s",
                provider.id,
                AGENT_LLM_TIMEOUT.as_secs()
            );
            CallOutcome::Failed
        }
        Ok(Ok(success)) => match success.content {
            Some(content) if !content.trim().is_empty() => CallOutcome::Ok {
                text: content,
                remaining_requests: success.remaining_requests,
                remaining_tokens: success.remaining_tokens,
                total_tokens: success.total_tokens,
            },
            _ => CallOutcome::Failed,
        },
        Ok(Err(LlmError::RateLimited { retry_after_s })) => {
            CallOutcome::RateLimited { retry_after_s }
        }
        Ok(Err(LlmError::Other(e))) => {
            warn!("[GRAIN] agent provider '{}' failed: {e}", provider.id);
            CallOutcome::Failed
        }
    }
}

/// Flatten a multi-turn conversation into a single (system, user) pair for local
/// backends that don't take a message array (Apple Intelligence).
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn flatten_for_single_prompt(messages: &[(String, String)]) -> (String, String) {
    let mut system = String::new();
    let mut convo = String::new();
    for (role, content) in messages {
        match role.as_str() {
            "system" => {
                if !system.is_empty() {
                    system.push_str("\n\n");
                }
                system.push_str(content);
            }
            "assistant" => convo.push_str(&format!("Assistant: {content}\n")),
            _ => convo.push_str(&format!("User: {content}\n")),
        }
    }
    (system, convo.trim().to_string())
}
