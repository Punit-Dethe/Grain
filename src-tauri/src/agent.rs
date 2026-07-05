//! [GRAIN] The Agent — a summoned, voice-first AI scratchpad in its own
//! destroyable windows ("if it's not in use, destroy it").
//!
//! Two surfaces (faithful to the reference design):
//!   • PALETTE — a centred summon bar that records by default; type to override,
//!     Enter to submit. It captures the foreground selection (synthesised copy +
//!     clipboard diff). On submit it hands the instruction to the panel and closes.
//!   • PANEL — a bottom-right reply card (COMPACT: pager over retry versions,
//!     the captured text, the reply, copy / retry / Confirm-⏎-paste) that grows
//!     into the EXPANDED conversation when the user asks a follow-up.
//!
//! QUICK AGENT (opt-in): submit runs the AI headlessly and pastes the reply at
//! the cursor; the pill then briefly offers "ask follow-up", which reopens the
//! panel expanded with the conversation restored.
//!
//! The conversation is sent to the SAME AI the post-processing layer uses (single
//! provider, or the smart-rotation pool with failover + daily quota).
//!
//! Everything here is headless-friendly: it reads the owned settings, reuses the
//! STT dispatcher (`stt_router`) and the LLM rotation infra (`post_process_router`
//! + `rotation_state`), and never assumes a UI is alive.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use grain_core::{DaemonEvent, PostProcessProvider};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::input::EnigoState;
use crate::llm_client::LlmError;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::rotation_state::{CallOutcome, RotationTrackers};
use crate::settings::{
    get_settings, AgentAutocopy, AgentContextMode, ShortcutBinding,
    APPLE_INTELLIGENCE_PROVIDER_ID,
};

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
/// The user-configurable "ask follow-up" binding id (seeded in settings). Only
/// ever registered TRANSIENTLY while an Agent surface (panel / pill offer) is
/// live — and in that window it overrides any other Grain binding on the same
/// keys (suppressed at register, restored at teardown).
const AGENT_FOLLOWUP_BINDING: &str = "agent_followup";
const AGENT_LLM_TIMEOUT: Duration = Duration::from_secs(120);
/// How long the Quick-Agent pill offer stays live before it is withdrawn (and
/// the transient follow-up shortcut released) — "destroy if not in use".
const FOLLOWUP_OFFER_TTL: Duration = Duration::from_secs(45);
/// Cap on the FULL-mode field context handed to the LLM (chars).
const FIELD_CONTEXT_MAX_CHARS: usize = 6000;

/// Palette geometry (logical px): a fixed centred bar near the upper third.
const PALETTE_W: f64 = 620.0;
const PALETTE_H: f64 = 122.0;
/// Panel geometry (logical px). The COMPACT reply card sits in the bottom-right
/// corner (the reference design); the EXPANDED conversation keeps the old
/// sidebar footprint but stays anchored bottom-right.
const PANEL_W: f64 = 500.0;
const PANEL_COMPACT_W: f64 = 392.0;
const PANEL_COMPACT_H: f64 = 442.0;
const PANEL_MARGIN: f64 = 18.0;

/// The Agent's system instruction. The user's dictated/typed instruction is the
/// task; the selected text (if any) is supplied as context separately.
const AGENT_SYSTEM_PROMPT: &str = "You are Grain's built-in assistant. The user acts on text they have selected and on what they dictate or type. Follow their instruction precisely and reply with ONLY the result they asked for — no preamble, no sign-off, no meta commentary. Do not wrap the answer in markdown code fences unless the user explicitly asks for code. When they ask you to rewrite, summarise, translate, fix, shorten, or reformat the selected text, operate on that text. Keep answers tight and useful.";

/// [GRAIN] Focused-field context captured at summon (agent context awareness).
/// `full == false` → `text` is a comma-joined list of unique terms; `full ==
/// true` → `text` is the capped raw field content.
#[derive(Debug, Clone)]
pub struct FieldContext {
    pub full: bool,
    pub text: String,
}

/// Cross-window state, set at summon and handed off palette → panel.
#[derive(Default)]
pub struct AgentState {
    /// Selection captured at summon: the palette shows the text (truncated) and
    /// the panel uses it as the LLM context. Non-consuming; overwritten on each
    /// summon.
    pub context: Mutex<Option<String>>,
    /// First instruction handed from the palette to the panel on submit.
    pub pending_instruction: Mutex<Option<String>>,
    /// Foreground window at summon — the paste target for Confirm / Quick Agent.
    /// Raw HWND as isize on Windows; unused elsewhere.
    pub target_hwnd: Mutex<Option<isize>>,
    /// Focused-field context captured at summon (per `agent_context_mode`).
    pub field_context: Mutex<Option<FieldContext>>,
    /// Quick-Agent conversation retained so "ask follow-up" can reopen the panel
    /// with history. Cleared on fresh summon and consumed by the panel on mount.
    pub conversation: Mutex<Vec<AgentMessage>>,
    /// Grain bindings suppressed while the follow-up shortcut overrides them.
    pub suppressed_bindings: Mutex<Vec<ShortcutBinding>>,
    /// True while a Quick-Agent pill offer is live (keeps the transient follow-up
    /// shortcut registered even though no Agent window exists).
    pub followup_offer_active: AtomicBool,
    /// Bumped per offer so a stale TTL expiry never clears a newer offer.
    pub followup_offer_gen: AtomicU64,
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
            // A fresh summon supersedes any lingering Quick-Agent offer.
            clear_followup_offer(&app);

            // Snapshot the paste target BEFORE any Agent window can steal focus.
            let hwnd = foreground_hwnd();
            let c = capture_selection(&app);
            // Field context reads the still-focused field via UI Automation —
            // must run before the palette opens (it grabs focus).
            let mode = get_settings(&app).agent_context_mode;
            let fc = capture_field_context(mode);
            if let Some(state) = app.try_state::<AgentState>() {
                if let Ok(mut g) = state.context.lock() {
                    *g = c;
                }
                if let Ok(mut g) = state.target_hwnd.lock() {
                    *g = hwnd;
                }
                if let Ok(mut g) = state.field_context.lock() {
                    *g = fc;
                }
                if let Ok(mut g) = state.conversation.lock() {
                    g.clear();
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
    let Ok(raw) = win.hwnd() else { return };
    force_foreground_raw(raw.0 as isize);
}

/// The current foreground window, as a raw HWND — the paste target snapshot
/// taken at summon. `None` off Windows (macOS restores focus to the previous
/// app by itself when our window closes).
fn foreground_hwnd() -> Option<isize> {
    #[cfg(windows)]
    unsafe {
        let h = windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        if h.0.is_null() {
            None
        } else {
            Some(h.0 as isize)
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Bring an arbitrary window (by raw HWND) back to the foreground so a
/// synthesised paste lands in it. Same input-queue bridge as `force_foreground`.
#[cfg(windows)]
fn force_foreground_raw(raw: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
        SetForegroundWindow, ShowWindow, SW_SHOW,
    };

    let hwnd = HWND(raw as _);
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return; // target app closed since summon — paste lands wherever focus is.
        }
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

    let window = builder.build()?;

    // [GRAIN] Release the transient global Enter/Escape shortcuts when this
    // surface is destroyed and no other agent window remains. This covers every
    // close path (the in-window × button, the frontend's own Escape handler, or
    // the backend `global_close`) so the shortcuts can never outlive the Agent
    // and keep hijacking Enter/Escape system-wide ("destroy if not in use").
    {
        let app = app.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let palette_gone = app.get_webview_window(PALETTE_LABEL).is_none();
                let panel_gone = app.get_webview_window(PANEL_LABEL).is_none();
                if palette_gone && panel_gone {
                    unregister_transient_shortcuts_deferred(&app);
                }
            }
        });
    }

    Ok(window)
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

/// Monitor WORK AREA (excludes the taskbar/dock) in LOGICAL px, same tuple shape
/// as [`monitor_logical`] — so the bottom-right panel never hides behind the
/// taskbar.
fn monitor_work_logical(window: &tauri::WebviewWindow) -> Option<(f64, f64, f64, f64)> {
    let monitor = match window.current_monitor() {
        Ok(Some(m)) => Some(m),
        _ => window.primary_monitor().ok().flatten(),
    }?;
    let scale = monitor.scale_factor();
    let wa = monitor.work_area();
    Some((
        wa.position.x as f64 / scale,
        wa.position.y as f64 / scale,
        wa.size.width as f64 / scale,
        wa.size.height as f64 / scale,
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

/// Anchor the panel to the BOTTOM-RIGHT corner of the work area (the reference
/// design). Compact = the small reply card; expanded = the old sidebar footprint
/// (same width/height budget), still bottom-right.
fn place_panel(window: &tauri::WebviewWindow, expanded: bool) {
    let metrics = monitor_work_logical(window).or_else(|| monitor_logical(window));
    if let Some((ox, oy, sw, sh)) = metrics {
        let (w, h) = if expanded {
            (PANEL_W, (sh - 90.0).clamp(360.0, 880.0))
        } else {
            (PANEL_COMPACT_W, PANEL_COMPACT_H.min(sh - 2.0 * PANEL_MARGIN))
        };
        let _ = window.set_size(tauri::LogicalSize::new(w, h));
        let x = ox + sw - w - PANEL_MARGIN;
        let y = oy + sh - h - PANEL_MARGIN;
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

    // Restore the user's clipboard regardless of outcome. If there was prior
    // text, put it back. If there was none (empty / non-text clipboard) but our
    // synthetic copy DID land something, clear it so the capture stays invisible
    // and we never leave the selected text sitting on the user's clipboard.
    match saved {
        Some(prev) => {
            let _ = clipboard.write_text(prev);
        }
        None if captured.is_some() => {
            let _ = clipboard.write_text(String::new());
        }
        None => {}
    }

    captured
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// [GRAIN] Agent context awareness: read the still-focused field at summon.
/// `Unique` reuses the nearby-terms extractor (high-signal identifiers/names
/// only); `Full` takes the capped raw text. Best-effort and silent — any failure
/// simply yields `None` (behaves as if the mode were off). Password fields are
/// never read (enforced inside `read_focused_text`).
fn capture_field_context(mode: AgentContextMode) -> Option<FieldContext> {
    match mode {
        AgentContextMode::Off => None,
        AgentContextMode::Unique => {
            let text = crate::context_detect::read_focused_text()?;
            let terms = crate::context_detect::extract_unique_terms(&text);
            if terms.is_empty() {
                None
            } else {
                Some(FieldContext {
                    full: false,
                    text: terms.join(", "),
                })
            }
        }
        AgentContextMode::Full => {
            let text = crate::context_detect::read_focused_text()?;
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            Some(FieldContext {
                full: true,
                text: trimmed.chars().take(FIELD_CONTEXT_MAX_CHARS).collect(),
            })
        }
    }
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
    show_panel(&app, false)
}

/// Resize/reposition the panel between the COMPACT reply card and the EXPANDED
/// conversation, and swap the global Enter accordingly: compact owns a global
/// Enter (= Confirm/paste); expanded owns an in-window input, so a registered
/// global Enter would swallow the user's keystrokes.
#[tauri::command]
#[specta::specta]
pub fn agent_set_panel_mode(app: AppHandle, expanded: bool) -> Result<(), String> {
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window(PANEL_LABEL) {
            place_panel(&w, expanded);
        }
        if expanded {
            let _ = crate::shortcut::unregister_shortcut(&app2, submit_binding());
        } else {
            register_one_transient(&app2, submit_binding());
        }
    })
    .map_err(|e| format!("failed to set agent panel mode: {e:?}"))
}

fn show_panel(app: &AppHandle, expanded: bool) -> Result<(), String> {
    // Escape (close) + the configurable follow-up shortcut are live whenever the
    // panel is up. The global Enter (= Confirm/paste latest reply) is COMPACT
    // only — the expanded panel owns its own input field.
    register_one_transient(app, close_binding());
    register_followup_shortcut(app);
    if expanded {
        let _ = crate::shortcut::unregister_shortcut(app, submit_binding());
    } else {
        register_one_transient(app, submit_binding());
    }

    info!("[GRAIN] agent: showing panel (expanded: {expanded})");
    let win = match app.get_webview_window(PANEL_LABEL) {
        Some(w) => {
            place_panel(&w, expanded);
            w
        }
        None => {
            info!("[GRAIN] agent: building panel window");
            let (w, h) = if expanded {
                (PANEL_W, 600.0)
            } else {
                (PANEL_COMPACT_W, PANEL_COMPACT_H)
            };
            let w = build_window(app, PANEL_LABEL, w, h)
                .map_err(|e| format!("failed to build agent panel: {e}"))?;
            info!("[GRAIN] agent: panel window built");
            place_panel(&w, expanded);
            info!("[GRAIN] agent: panel window placed");
            w
        }
    };
    show_and_focus(&win);
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

    // [GRAIN] Quick Agent: run the AI headlessly and paste the reply straight at
    // the cursor — no panel. The pill then briefly offers "ask follow-up".
    if get_settings(&app).agent_quick_enabled {
        quick_run(app, text);
        return Ok(());
    }

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
            if let Err(e) = show_panel(&panel_handle, false) {
                error!("[GRAIN] agent: failed to show panel: {e}");
                report_submit_failure(&panel_handle, &e);
            }
        }) {
            error!("[GRAIN] agent: failed to schedule agent panel: {e:?}");
            report_submit_failure(&app_for_task, &format!("{e:?}"));
        }
    });

    Ok(())
}

/// The panel handoff failed after the palette was already closed. Bring the
/// palette back and tell it why, so the user can retry instead of being left
/// with no Agent window and a wedged submit guard (the palette's
/// `agent-submit-error` listener resets its state on this event).
fn report_submit_failure(app: &AppHandle, message: &str) {
    let app = app.clone();
    let message = message.to_string();
    let _ = app.clone().run_on_main_thread(move || {
        show_palette(&app);
        // Let the rebuilt palette webview mount its listener before emitting.
        let app_for_emit = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(120));
            let _ = app_for_emit.emit_to(PALETTE_LABEL, "agent-submit-error", message);
        });
    });
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

/// The user-configured follow-up binding (falls back to the seeded default).
fn followup_binding(app: &AppHandle) -> ShortcutBinding {
    get_settings(app)
        .bindings
        .get(AGENT_FOLLOWUP_BINDING)
        .cloned()
        .unwrap_or_else(|| {
            crate::settings::get_default_settings()
                .bindings
                .get(AGENT_FOLLOWUP_BINDING)
                .cloned()
                .expect("agent_followup default binding exists")
        })
}

/// Register the follow-up shortcut for the lifetime of the Agent surface. It
/// OVERRIDES any other Grain binding on the same keys: conflicting bindings are
/// unregistered and remembered, then restored at [`unregister_followup_shortcut`]
/// — so the user can share one accelerator between a global action and the
/// Agent, with the Agent winning while it is open.
fn register_followup_shortcut(app: &AppHandle) {
    let binding = followup_binding(app);
    let accel = binding.current_binding.trim().to_ascii_lowercase();
    if accel.is_empty() {
        return;
    }
    let settings = get_settings(app);
    if let Some(state) = app.try_state::<AgentState>() {
        if let Ok(mut suppressed) = state.suppressed_bindings.lock() {
            for (id, b) in settings.bindings.iter() {
                // Dynamic bindings are never globally registered — nothing to suppress.
                if id == AGENT_FOLLOWUP_BINDING || id == "cancel" || id == "transcribe_send_to_ai"
                {
                    continue;
                }
                if b.current_binding.trim().eq_ignore_ascii_case(&accel)
                    && !suppressed.iter().any(|s| s.id == b.id)
                {
                    let _ = crate::shortcut::unregister_shortcut(app, b.clone());
                    suppressed.push(b.clone());
                }
            }
        }
    }
    register_one_transient(app, binding);
}

/// Release the transient follow-up shortcut and restore any Grain bindings it
/// suppressed while overriding them.
fn unregister_followup_shortcut(app: &AppHandle) {
    let _ = crate::shortcut::unregister_shortcut(app, followup_binding(app));
    if let Some(state) = app.try_state::<AgentState>() {
        if let Ok(mut suppressed) = state.suppressed_bindings.lock() {
            for b in suppressed.drain(..) {
                if let Err(e) = crate::shortcut::register_shortcut(app, b.clone()) {
                    warn!(
                        "[GRAIN] agent: failed to restore suppressed binding '{}': {e}",
                        b.id
                    );
                }
            }
        }
    }
}

/// Register temporary global Enter/Escape while the Agent is visible. This
/// mirrors the old QML assist workflow and covers Windows focus loss, where the
/// palette is on screen but ordinary webview keydown events never arrive.
pub fn register_transient_shortcuts(app: &AppHandle) {
    for binding in transient_bindings() {
        register_one_transient(app, binding);
    }
}

fn register_one_transient(app: &AppHandle, binding: ShortcutBinding) {
    let _ = crate::shortcut::unregister_shortcut(app, binding.clone());
    if let Err(e) = crate::shortcut::register_shortcut(app, binding.clone()) {
        warn!(
            "[GRAIN] agent: failed to register transient shortcut '{}': {}",
            binding.current_binding, e
        );
    }
}

pub fn unregister_transient_shortcuts(app: &AppHandle) {
    for binding in transient_bindings() {
        let _ = crate::shortcut::unregister_shortcut(app, binding);
    }
}

pub fn unregister_transient_shortcuts_deferred(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        unregister_transient_shortcuts(&app);
        // The follow-up shortcut outlives the windows ONLY while a Quick-Agent
        // pill offer is live; otherwise release it (and restore suppressed keys).
        let offer_live = app
            .try_state::<AgentState>()
            .map(|s| s.followup_offer_active.load(Ordering::SeqCst))
            .unwrap_or(false);
        if !offer_live {
            unregister_followup_shortcut(&app);
        }
    });
}

/// Called by the transient global Enter shortcut. The frontend still owns typed
/// text (palette) / the displayed reply version (panel), so this emits into the
/// open surface instead of trying to infer state in Rust. On the compact panel
/// the frontend answers with `agent_confirm_paste`.
pub fn global_submit(app: &AppHandle) {
    if app.get_webview_window(PALETTE_LABEL).is_some() {
        let _ = app.emit_to(PALETTE_LABEL, "agent-global-enter", ());
    } else if app.get_webview_window(PANEL_LABEL).is_some() {
        let _ = app.emit_to(PANEL_LABEL, "agent-global-enter", ());
    }
}

/// Called by the transient follow-up shortcut (and the pill's offer click):
/// expand the open panel, or reopen it expanded with the retained Quick-Agent
/// conversation.
pub fn open_followup(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window(PANEL_LABEL) {
        // The frontend expands itself (and calls `agent_set_panel_mode`).
        let _ = app.emit_to(PANEL_LABEL, "agent-followup", ());
        show_and_focus(&panel);
        return;
    }

    // Windowless (pill offer) path: only meaningful with a retained conversation.
    let has_conversation = app
        .try_state::<AgentState>()
        .map(|s| {
            s.conversation
                .lock()
                .map(|g| !g.is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !has_conversation {
        return;
    }

    clear_followup_offer(app);
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Err(e) = show_panel(&app2, true) {
            error!("[GRAIN] agent: failed to open follow-up panel: {e}");
        }
    });
}

// ============================================================================
// Quick Agent (headless run → paste at cursor → pill follow-up offer)
// ============================================================================

/// [GRAIN] Quick Agent: close the palette, run the conversation headlessly, then
/// refocus the summon target and paste the reply at the cursor. A selection
/// still held in the target app is replaced by the paste — which is exactly the
/// "rewrite the selected chunk" behavior, with no synthetic select-all/erase.
/// Ends by offering "ask follow-up" through the pill.
fn quick_run(app: AppHandle, instruction: String) {
    std::thread::spawn(move || {
        // Seed the retained conversation with the user's turn.
        if let Some(state) = app.try_state::<AgentState>() {
            if let Ok(mut g) = state.conversation.lock() {
                g.clear();
                g.push(AgentMessage {
                    role: "user".to_string(),
                    content: instruction.clone(),
                });
            }
        }

        // Close the palette right away — the instruction was accepted.
        let close_handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(palette) = close_handle.get_webview_window(PALETTE_LABEL) {
                let _ = palette.close();
            }
        });

        let (context, field) = read_summon_context(&app);
        let messages = vec![AgentMessage {
            role: "user".to_string(),
            content: instruction,
        }];

        // Blocking this detached thread on the shared runtime is fine — it is
        // not a runtime worker.
        let result = tauri::async_runtime::block_on(run_conversation(
            &app,
            &messages,
            context.as_deref(),
            field.as_ref(),
        ));

        match result {
            Ok(reply) => {
                if let Some(state) = app.try_state::<AgentState>() {
                    if let Ok(mut g) = state.conversation.lock() {
                        g.push(AgentMessage {
                            role: "assistant".to_string(),
                            content: reply.clone(),
                        });
                    }
                }
                // Auto-copy per policy (the sole reply is also the first reply).
                if get_settings(&app).agent_autocopy != AgentAutocopy::Off {
                    let _ = app.clipboard().write_text(reply.clone());
                }

                // Refocus the summon target, give it a beat, then paste.
                refocus_target(&app);
                std::thread::sleep(Duration::from_millis(160));
                if let Err(e) = crate::clipboard::paste(reply, app.clone()) {
                    error!("[GRAIN] agent: quick paste failed: {e}");
                    crate::bridge::emit(&app, DaemonEvent::PasteError { error: e });
                }
                // Offer the follow-up either way — the panel is the retry path.
                offer_followup(&app);
            }
            Err(e) => {
                warn!("[GRAIN] agent: quick run failed: {e}");
                report_submit_failure(&app, &e);
            }
        }
    });
}

/// Selection + field context captured at summon (cloned out of the state).
fn read_summon_context(app: &AppHandle) -> (Option<String>, Option<FieldContext>) {
    let Some(state) = app.try_state::<AgentState>() else {
        return (None, None);
    };
    let context = state.context.lock().ok().and_then(|g| g.clone());
    let field = state.field_context.lock().ok().and_then(|g| g.clone());
    (context, field)
}

/// Refocus the window that was foreground at summon so a synthesised paste
/// lands where the user was working.
fn refocus_target(app: &AppHandle) {
    #[cfg(windows)]
    {
        let hwnd = app
            .try_state::<AgentState>()
            .and_then(|s| s.target_hwnd.lock().ok().and_then(|g| *g));
        if let Some(raw) = hwnd {
            force_foreground_raw(raw);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

/// Arm the pill's "ask follow-up" offer: keep the follow-up shortcut registered,
/// tell the pill to show the affordance, and withdraw it after the TTL.
fn offer_followup(app: &AppHandle) {
    let Some(state) = app.try_state::<AgentState>() else {
        return;
    };
    register_followup_shortcut(app);
    state.followup_offer_active.store(true, Ordering::SeqCst);
    let gen = state.followup_offer_gen.fetch_add(1, Ordering::SeqCst) + 1;

    let shortcut = followup_binding(app).current_binding;
    crate::bridge::emit(app, DaemonEvent::AgentFollowupOffer { shortcut });

    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FOLLOWUP_OFFER_TTL);
        let Some(state) = app2.try_state::<AgentState>() else {
            return;
        };
        // Only the newest offer may expire itself; a fresh offer or the panel
        // taking over invalidates this timer.
        if state.followup_offer_gen.load(Ordering::SeqCst) == gen
            && state.followup_offer_active.load(Ordering::SeqCst)
        {
            clear_followup_offer(&app2);
            if app2.get_webview_window(PANEL_LABEL).is_none()
                && app2.get_webview_window(PALETTE_LABEL).is_none()
            {
                unregister_followup_shortcut(&app2);
            }
        }
    });
}

/// Withdraw the pill offer (if any). Does NOT touch the shortcut registration —
/// callers decide whether a surface still needs it.
fn clear_followup_offer(app: &AppHandle) {
    if let Some(state) = app.try_state::<AgentState>() {
        if state.followup_offer_active.swap(false, Ordering::SeqCst) {
            crate::bridge::emit(app, DaemonEvent::AgentFollowupClear);
        }
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

    // Warm the local model only when this dictation will actually be transcribed
    // locally. `will_route_to_cloud` is the same predicate the batch press path
    // uses, so the warm-up matches what `agent_stop_dictation` routes to: when
    // rotation is on AND a cloud provider is eligible we skip the load (the model
    // would otherwise sit resident in RAM unused); when rotation is off, or on
    // but with no eligible cloud provider (local fallback), we pre-warm it so the
    // transcript is ready quickly on stop.
    if !crate::stt_router::will_route_to_cloud(&app) {
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

/// Consume the retained Quick-Agent conversation (the panel calls this on
/// mount). Non-empty only when the panel is reopening from a follow-up offer —
/// in that case the panel starts EXPANDED with this history.
#[tauri::command]
#[specta::specta]
pub fn agent_take_conversation(app: AppHandle) -> Vec<AgentMessage> {
    app.try_state::<AgentState>()
        .and_then(|s| {
            s.conversation
                .lock()
                .ok()
                .map(|mut g| std::mem::take(&mut *g))
        })
        .unwrap_or_default()
}

/// Confirm (⏎ on the reply card): close the panel, refocus the summon target,
/// and paste `text` — the latest assistant reply — at the cursor. A selection
/// still held in the target app is replaced by the paste.
#[tauri::command]
#[specta::specta]
pub fn agent_confirm_paste(app: AppHandle, text: String) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("Nothing to paste yet".to_string());
    }
    std::thread::spawn(move || {
        let close_handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(panel) = close_handle.get_webview_window(PANEL_LABEL) {
                let _ = panel.close();
            }
        });
        // Let the close land and focus settle, then force our summon target.
        std::thread::sleep(Duration::from_millis(120));
        refocus_target(&app);
        std::thread::sleep(Duration::from_millis(140));
        if let Err(e) = crate::clipboard::paste(text, app.clone()) {
            error!("[GRAIN] agent: confirm paste failed: {e}");
            crate::bridge::emit(&app, DaemonEvent::PasteError { error: e });
        }
    });
    Ok(())
}

/// Run the conversation against the configured AI and return the assistant reply.
/// Uses the post-processing provider config: a single provider, or the smart
/// rotation pool (round-robin + daily quota + health-ordered failover). The
/// focused-field context captured at summon (if any) is injected backend-side.
#[tauri::command]
#[specta::specta]
pub async fn agent_run(
    app: AppHandle,
    messages: Vec<AgentMessage>,
    context: Option<String>,
) -> Result<String, String> {
    let field = app
        .try_state::<AgentState>()
        .and_then(|s| s.field_context.lock().ok().and_then(|g| g.clone()));
    run_conversation(&app, &messages, context.as_deref(), field.as_ref()).await
}

/// The Agent's LLM driver, shared by the panel (`agent_run`) and Quick Agent.
pub async fn run_conversation(
    app: &AppHandle,
    messages: &[AgentMessage],
    context: Option<&str>,
    field: Option<&FieldContext>,
) -> Result<String, String> {
    info!(
        "[GRAIN] agent: running AI request ({} messages, context: {}, field: {})",
        messages.len(),
        if context.map(|c| !c.trim().is_empty()).unwrap_or(false) {
            "yes"
        } else {
            "no"
        },
        match field {
            Some(f) if f.full => "full",
            Some(_) => "unique",
            None => "no",
        }
    );
    let settings = get_settings(app);

    let full = build_messages(messages, context, field);

    if settings.post_process_smart_rotation {
        return agent_run_rotated(app, &full).await;
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
/// optional field context + the conversation turns (normalising every role to
/// user/assistant).
///
/// The framing separates the SELECTED TEXT (the subject the instruction operates
/// on) from the FIELD CONTEXT (background reference only) — so when the user
/// selects one paragraph inside a long document and full-context is on, the
/// model rewrites only the selection instead of the whole field.
fn build_messages(
    messages: &[AgentMessage],
    context: Option<&str>,
    field: Option<&FieldContext>,
) -> Vec<(String, String)> {
    let mut full: Vec<(String, String)> = Vec::with_capacity(messages.len() + 3);
    full.push(("system".to_string(), AGENT_SYSTEM_PROMPT.to_string()));

    if let Some(ctx) = context.map(str::trim).filter(|c| !c.is_empty()) {
        full.push((
            "system".to_string(),
            format!(
                "The user has SELECTED the following text. It is the subject of their instruction — operate on it (and reply with only the transformed result) unless they say otherwise:\n\n{ctx}"
            ),
        ));
    }

    if let Some(f) = field.filter(|f| !f.text.trim().is_empty()) {
        if f.full {
            full.push((
                "system".to_string(),
                format!(
                    "Background — the surrounding content of the text field the user is working in, provided for context ONLY (style, terminology, what came before). Do NOT rewrite, repeat, or output it, and do NOT treat it as the subject of the instruction; the selected text above (if any) or the user's request is the subject:\n\n{}",
                    f.text
                ),
            ));
        } else {
            full.push((
                "system".to_string(),
                format!(
                    "Background — names and identifiers found near the user's cursor. Use them ONLY to spell such terms correctly in your reply; never insert ones the user did not mention: {}",
                    f.text
                ),
            ));
        }
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

    let Some(http_client) = app.try_state::<reqwest::Client>() else {
        return Err("Agent: shared HTTP client unavailable".into());
    };
    let http_client = http_client.inner().clone();

    // Failover walk lives in the shared driver; we supply only how to run one
    // provider (resolve model/key + call) and how to record quota on success.
    crate::rotation_state::run_with_rotation(
        &trackers.llm,
        &candidates,
        est_tokens,
        |id| {
            let http_client = http_client.clone();
            let eligible = &eligible;
            let settings = &settings;
            let full = full;
            async move {
                let Some(provider) = eligible.iter().find(|p| p.id == id) else {
                    return CallOutcome::Failed;
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
                run_agent_once(&http_client, provider, model, api_key, full).await
            }
        },
        |id| {
            crate::post_process_router::record_usage(app, id);
            log::info!("[GRAIN] agent routed to '{id}'");
        },
    )
    .await
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
