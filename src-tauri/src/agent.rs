//! [GRAIN] The Agent — a summoned, voice-first AI scratchpad in its own
//! destroyable windows ("if it's not in use, destroy it").
//!
//! Two surfaces (faithful to the reference design):
//!   • INPUT — NATIVE (it lives in the pill process, shown instantly on summon):
//!     records by default, expands into a typing card on the first keystroke.
//!     The core captures the foreground selection at summon, starts dictation,
//!     and pre-creates the reply panel HIDDEN so it is warm at submit.
//!   • PANEL — a bottom-right reply card webview (COMPACT: pager over retry
//!     versions, the captured text, the reply, copy / retry / Confirm-⏎-paste)
//!     that grows into the EXPANDED conversation when the user asks a follow-up.
//!     Revealed the instant the user submits (loading state), not when the
//!     reply lands.
//!
//! QUICK AGENT (opt-in): submit runs the AI headlessly and pastes the reply at
//! the cursor; the pill then briefly offers "ask follow-up" (≈8s, Esc to
//! dismiss), and the warm hidden panel lives exactly as long as the offer —
//! reopening it expanded with the conversation restored is instant.
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

use grain_core::{AgentInputKind, DaemonEvent, PostProcessProvider};
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
    get_settings, AgentAutocopy, AgentContextMode, ShortcutBinding, APPLE_INTELLIGENCE_PROVIDER_ID,
};

/// Window label (matched by its capability + the frontend router in
/// `main.tsx`). The summon INPUT is native (it lives in the pill process); the
/// PANEL (`agent-panel`) is the only Agent webview — the bottom-right reply
/// card / conversation.
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
/// the transient follow-up shortcut released) — "destroy if not in use". Short
/// on purpose: the offer is a quick escape hatch, not a lingering surface.
const FOLLOWUP_OFFER_TTL: Duration = Duration::from_secs(8);
/// Cap on the FULL-mode field context handed to the LLM (chars).
const FIELD_CONTEXT_MAX_CHARS: usize = 6000;

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
    /// True while the NATIVE input (the pill's summon card) is up. Gates the
    /// transient global Enter/Escape routing and dedups double submits.
    pub input_active: AtomicBool,
    /// [GRAIN] True while the reply panel is in its EXPANDED conversation stage
    /// (it owns a follow-up text field). Dictation is routed INTO that field
    /// only when the panel is expanded AND focused — never over the compact
    /// reply card. Set by `agent_set_panel_mode` / `show_panel`.
    pub panel_expanded: AtomicBool,
    /// Which brain drives this session — set at summon by the binding that
    /// fired (assist vs Grain Recall), never re-derived from the request.
    pub mode: Mutex<AgentMode>,
    /// Grain Recall session state: the ordered memory registry so `SOURCES: Mn`
    /// numbering is stable and additive across follow-up turns. Index `i` holds
    /// the note id for memory `M(i+1)`. Cleared on each fresh summon.
    pub recall: Mutex<crate::grain_space::recall::RecallSession>,
}

/// One conversation turn from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentMessage {
    /// `"user"` or `"assistant"` (anything else is treated as `"user"`).
    pub role: String,
    pub content: String,
}

/// One evidence source behind a Grain Recall answer (RECALL-PLAN §6.2). `title`
/// is the note's title (falling back to its summary); `saved_at` is a Unix-
/// millis timestamp for the chip's relative-age label. Empty for Assist.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentSource {
    pub note_id: String,
    pub title: String,
    pub saved_at: i64,
}

/// The panel's per-turn reply. `text` is the display answer (any Recall
/// convention line already stripped). `sources` + `not_found` drive Recall's
/// evidence footer / escape hatch (RECALL-PLAN §6); Assist always returns an
/// empty `sources` and `not_found = false`, so the panel renders no footer.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentReply {
    pub text: String,
    pub sources: Vec<AgentSource>,
    pub not_found: bool,
    /// Set only by a Grain Recall `forget` turn (RECALL-PLAN §7.2): the memory
    /// the user asked to delete. Destructive, so the panel confirms in-place
    /// before calling `grain_space_delete_note`. `None` on every other turn.
    pub confirm_delete: Option<AgentSource>,
}

impl AgentReply {
    /// A plain answer with no evidence footer (Assist, empty-corpus, errors
    /// that still produce prose).
    pub fn plain(text: String) -> Self {
        Self {
            text,
            sources: Vec::new(),
            not_found: false,
            confirm_delete: None,
        }
    }
}

/// Which brain drives the summoned surfaces. Fixed at summon by the binding
/// that fired — never re-derived from the request text (two doors, not one
/// door with a bouncer). See `RECALL-PLAN.md` §3.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentMode {
    /// The generic assistant: operates on the selection / focused field.
    #[default]
    Assist,
    /// Grain Recall: answers the user's question from their saved memories.
    Recall,
    /// Grain Space capture: structures the spoken/typed text (plus any current
    /// selection, saved verbatim as the note body) into a note and saves it.
    /// Same surfaces as Assist/Recall, its own brain. Never pastes.
    Capture,
}

// ============================================================================
// Summon + windows
// ============================================================================

/// Summon the Agent (Assist mode): capture the foreground selection + field
/// context, present the NATIVE input, start dictation, and pre-warm the reply
/// panel. See [`summon_inner`].
pub fn summon(app: &AppHandle) {
    summon_inner(app, AgentMode::Assist);
}

/// Summon Grain Recall (memory mode): the SAME surfaces, but no selection /
/// field / paste-target capture — a memory question operates on the user's
/// saved notes, not on whatever they had highlighted. Distinct binding, so the
/// mode is fixed here, never guessed by the AI.
pub fn summon_memory(app: &AppHandle) {
    summon_inner(app, AgentMode::Recall);
}

/// Summon Grain Space capture (note mode): the SAME surfaces as Assist, and it
/// DOES grab the foreground selection (so the user can select text and save it
/// without retyping) — but no field-context read and no paste-target snapshot,
/// because capture never pastes back. Distinct binding, mode fixed here.
pub fn summon_capture(app: &AppHandle) {
    summon_inner(app, AgentMode::Capture);
}

/// Shared summon body. `agent_mode` selects what context is captured and which
/// brain the panel will use. Runs off the hotkey thread (the capture must
/// never block the input listener).
fn summon_inner(app: &AppHandle, agent_mode: AgentMode) {
    let app = app.clone();
    std::thread::spawn(move || {
        // Re-summon while the input is already up: just re-present it (the pill
        // refreshes the chip and re-grabs keyboard focus). No fresh capture — a
        // synthetic Ctrl+C now would clobber the user's original selection.
        let already_open = app
            .try_state::<AgentState>()
            .map(|s| s.input_active.load(Ordering::SeqCst))
            .unwrap_or(false);
        if already_open {
            let chars = app
                .try_state::<AgentState>()
                .and_then(|s| s.context.lock().ok().and_then(|g| g.clone()))
                .map(|c| c.chars().count() as u32)
                .unwrap_or(0);
            crate::bridge::emit(
                &app,
                DaemonEvent::AgentInputShow {
                    selection_chars: chars,
                    type_to_expand: get_settings(&app).agent_input_type_to_expand,
                    kind: input_kind(agent_mode),
                },
            );
            return;
        }

        // A fresh summon supersedes any lingering Quick-Agent offer.
        clear_followup_offer(&app);

        // What each mode captures at summon:
        // - Assist:  selection + field context + paste-target (it operates on
        //            the field and pastes back).
        // - Capture: selection ONLY (the selection becomes the note body); no
        //            field read, no paste-target — capture never pastes.
        // - Recall:  nothing — a memory question operates on saved notes, not on
        //            whatever is highlighted (also shaves ~300ms off summon).
        let assist = agent_mode == AgentMode::Assist;
        let want_selection = agent_mode != AgentMode::Recall;
        let hwnd = if assist { foreground_hwnd() } else { None };
        let c = if want_selection {
            capture_selection(&app)
        } else {
            None
        };
        let fc = if assist {
            capture_field_context(get_settings(&app).agent_context_mode)
        } else {
            None
        };
        let chars = c.as_ref().map(|s| s.chars().count() as u32).unwrap_or(0);
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
            if let Ok(mut g) = state.mode.lock() {
                *g = agent_mode;
            }
            if let Ok(mut g) = state.recall.lock() {
                g.clear();
            }
            state.input_active.store(true, Ordering::SeqCst);
            // Fresh session starts compact — dictation won't route to the panel
            // until it actually expands into the conversation stage.
            state.panel_expanded.store(false, Ordering::SeqCst);
        }

        // Start dictation + present the native input RIGHT AWAY — the panel
        // work below must never delay the "it's listening" feedback.
        start_dictation(&app);
        crate::bridge::emit(
            &app,
            DaemonEvent::AgentInputShow {
                selection_chars: chars,
                type_to_expand: get_settings(&app).agent_input_type_to_expand,
                kind: input_kind(agent_mode),
            },
        );
        // Global Enter (= submit request routed to the pill) + Escape (cancel)
        // while the input is up. The pill has real focus, but the globals cover
        // Windows' foreground-lock failures uniformly.
        register_transient_shortcuts(&app);

        // A new summon starts a fresh session — drop any open reply panel.
        let app_close = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(panel) = app_close.get_webview_window(PANEL_LABEL) {
                let _ = panel.close();
            }
        });
        // Capture saves headless (no panel), so don't pre-create one. Assist and
        // Recall pre-create a hidden panel so it's warm at submit.
        if agent_mode != AgentMode::Capture {
            std::thread::sleep(Duration::from_millis(120)); // let the close land
            let app_prep = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Err(e) = prepare_panel(&app_prep) {
                    warn!("[GRAIN] agent: failed to pre-create panel: {e}");
                }
            });
        }
    });
}

/// Map the summon mode to the native card's presentational kind (anchor +
/// labels). Assist keeps the original bottom card; the Grain Space kinds render
/// the top-anchored memory variants.
fn input_kind(mode: AgentMode) -> AgentInputKind {
    match mode {
        AgentMode::Assist => AgentInputKind::Assist,
        AgentMode::Capture => AgentInputKind::Capture,
        AgentMode::Recall => AgentInputKind::Recall,
    }
}

/// [GRAIN] True when a dictation transcript should be routed INTO the Agent
/// conversation window instead of OS-pasted: the panel must be in its EXPANDED
/// (follow-up field) stage AND be the focused window. Used by `clipboard::paste`
/// to fix "dictating into the panel pastes the auto-copied AI reply" — scoped
/// strictly to this window so upstream paste behavior is otherwise untouched.
pub fn panel_dictation_target(app: &AppHandle) -> bool {
    let expanded = app
        .try_state::<AgentState>()
        .map(|s| s.panel_expanded.load(Ordering::SeqCst))
        .unwrap_or(false);
    if !expanded {
        return false;
    }
    app.get_webview_window(PANEL_LABEL)
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false)
}

/// The current agent mode (defaults to Assist if state is somehow unavailable).
fn current_mode(app: &AppHandle) -> AgentMode {
    app.try_state::<AgentState>()
        .and_then(|s| s.mode.lock().ok().map(|g| *g))
        .unwrap_or_default()
}

/// Pre-create the reply panel HIDDEN (the webview loads in the background) so
/// showing it at submit is instant. Main-thread only. No-op if it exists.
fn prepare_panel(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(PANEL_LABEL).is_some() {
        return Ok(());
    }
    let w = build_window(app, PANEL_LABEL, PANEL_COMPACT_W, PANEL_COMPACT_H)
        .map_err(|e| format!("failed to build agent panel: {e}"))?;
    place_panel(&w, false); // placed but NOT shown
    info!("[GRAIN] agent: panel pre-created (hidden, warming)");
    Ok(())
}

/// Start the agent dictation (warm the local model/VAD exactly like the batch
/// press path would, so the transcript is ready quickly on submit).
fn start_dictation(app: &AppHandle) {
    let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    if !crate::stt_router::will_route_to_cloud(app) {
        let tm = app.state::<Arc<TranscriptionManager>>();
        tm.initiate_model_load();
    }
    {
        let rm = Arc::clone(&rm);
        std::thread::spawn(move || {
            let _ = rm.preload_vad();
        });
    }
    if let Err(e) = rm.try_start_recording(AGENT_BINDING) {
        warn!("[GRAIN] agent: failed to start dictation: {e}");
    }
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
                if app.get_webview_window(PANEL_LABEL).is_none() {
                    unregister_transient_shortcuts_deferred(&app);
                }
                // A Recall session may have spawned the embedding engine; drop
                // it unless the overlay browser is still using it (RECALL-PLAN
                // §3.4). No-op for Assist sessions, which never spawn it.
                crate::grain_space::embed::shutdown_engine_if_idle(&app);
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

/// Anchor the panel to the BOTTOM-RIGHT corner of the work area (the reference
/// design). Compact = the small reply card; expanded = the old sidebar footprint
/// (same width/height budget), still bottom-right.
fn place_panel(window: &tauri::WebviewWindow, expanded: bool) {
    let metrics = monitor_work_logical(window).or_else(|| monitor_logical(window));
    if let Some((ox, oy, sw, sh)) = metrics {
        let (w, h) = if expanded {
            (PANEL_W, (sh - 90.0).clamp(360.0, 880.0))
        } else {
            (
                PANEL_COMPACT_W,
                PANEL_COMPACT_H.min(sh - 2.0 * PANEL_MARGIN),
            )
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
// pub(crate): Grain Space quick-add reuses the same invisible selection grab.
pub(crate) fn capture_selection(app: &AppHandle) -> Option<String> {
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

/// Resize/reposition the panel between the COMPACT reply card and the EXPANDED
/// conversation, and swap the global Enter accordingly: compact owns a global
/// Enter (= Confirm/paste); expanded owns an in-window input, so a registered
/// global Enter would swallow the user's keystrokes.
///
/// ASYNC on purpose: a sync command runs on the MAIN thread, and calling
/// `set_size` on a visible window from inside a command on the main thread
/// deadlocks on Windows (tauri#3990 / tao#381) — that was the "panel ghosts a
/// few seconds after expanding" freeze. On a runtime worker the window ops are
/// proxied to the event loop safely.
#[tauri::command]
#[specta::specta]
pub async fn agent_set_panel_mode(app: AppHandle, expanded: bool) -> Result<(), String> {
    if let Some(state) = app.try_state::<AgentState>() {
        state.panel_expanded.store(expanded, Ordering::SeqCst);
    }
    if let Some(w) = app.get_webview_window(PANEL_LABEL) {
        place_panel(&w, expanded);
    }
    if expanded {
        let _ = crate::shortcut::unregister_shortcut(&app, submit_binding());
    } else {
        register_one_transient(&app, submit_binding());
    }
    Ok(())
}

fn show_panel(app: &AppHandle, expanded: bool) -> Result<(), String> {
    // Escape (close) + the configurable follow-up shortcut are live whenever the
    // panel is up. The global Enter (= Confirm/paste latest reply) is COMPACT
    // only — the expanded panel owns its own input field.
    register_one_transient(app, close_binding());
    register_followup_shortcut(app);
    if let Some(state) = app.try_state::<AgentState>() {
        state.panel_expanded.store(expanded, Ordering::SeqCst);
    }
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

// ============================================================================
// Native input → core (the pill's summon card talks back over the WS)
// ============================================================================

/// Pill → core: the user submitted TYPED text from the expanded input card.
/// `title` is the optional Grain Space note title (Capture only); `quick`
/// means the user held Shift → Quick Agent paste-in-place (Assist only).
pub fn input_submit_text(app: &AppHandle, text: String, title: String, quick: bool) {
    let Some(state) = app.try_state::<AgentState>() else {
        return;
    };
    if !state.input_active.swap(false, Ordering::SeqCst) {
        return; // stale double-submit (global Enter + window Enter, etc.)
    }
    // Typed text wins — abandon the voice capture and release the mic.
    app.state::<Arc<AudioRecordingManager>>().cancel_recording();

    let mode = current_mode(app);
    let text = text.trim().to_string();
    if text.is_empty() {
        crate::bridge::emit(app, DaemonEvent::AgentInputHide);
        input_cancel_cleanup(app);
        return;
    }
    // Capture (a typed note) keeps its card on screen so it can confirm the save
    // in place (Saved → hide, driven by `capture_run`) and carries the typed
    // title (empty → auto-generated). Every other mode hides now.
    if mode == AgentMode::Capture {
        let title = title.trim().to_string();
        let title = (!title.is_empty()).then_some(title);
        info!(
            "[GRAIN] agent: typed note submitted ({} body chars, title: {})",
            text.chars().count(),
            title.is_some()
        );
        capture_run(app.clone(), text, title, false);
        return;
    }
    crate::bridge::emit(app, DaemonEvent::AgentInputHide);
    info!(
        "[GRAIN] agent: typed instruction submitted ({} chars, quick: {quick})",
        text.chars().count()
    );
    dispatch_instruction(app.clone(), text, quick);
}

/// Pill → core: submit the in-progress VOICE capture. The panel is revealed
/// with its loading state IMMEDIATELY (before the transcript exists) so the
/// surface feels snappy even when STT/LLM are slow.
pub fn input_submit_voice(app: &AppHandle, quick: bool) {
    let Some(state) = app.try_state::<AgentState>() else {
        return;
    };
    if !state.input_active.swap(false, Ordering::SeqCst) {
        return;
    }
    // Capture keeps its card up to confirm the save in place; others hide now.
    if current_mode(app) != AgentMode::Capture {
        crate::bridge::emit(app, DaemonEvent::AgentInputHide);
    }

    let app = app.clone();
    std::thread::spawn(move || {
        // Capture is headless — never reveal or error a panel; on a bad
        // transcript it hides its still-open card and cleans up silently.
        let capture = current_mode(&app) == AgentMode::Capture;
        // Quick (paste-in-place) vs panel is now the user's per-submit choice
        // (Shift held → quick); no panel is pre-revealed for the quick path.
        if !quick && !capture {
            reveal_panel_loading(&app);
        }
        let no_speech = |app: &AppHandle, msg: &str| {
            if capture {
                crate::bridge::emit(app, DaemonEvent::AgentInputHide);
                unregister_transient_shortcuts_deferred(app);
            } else {
                deliver_agent_error(app, msg);
            }
        };

        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let samples = match rm.stop_recording(AGENT_BINDING) {
            Some(s) if !s.is_empty() => s,
            _ => {
                no_speech(&app, "Nothing was heard — try again.");
                return;
            }
        };
        info!(
            "[GRAIN] agent: transcribing dictation ({} samples)",
            samples.len()
        );
        // Blocking this detached thread on the shared runtime is fine — it is
        // not a runtime worker.
        let text =
            match tauri::async_runtime::block_on(crate::stt_router::transcribe(&app, samples)) {
                Ok(t) => t.trim().to_string(),
                Err(e) => {
                    warn!("[GRAIN] agent: dictation transcription failed: {e}");
                    no_speech(&app, &e);
                    return;
                }
            };
        if text.is_empty() {
            no_speech(&app, "Nothing was heard — try again.");
            return;
        }
        info!(
            "[GRAIN] agent: dictation transcript ready ({} chars)",
            text.chars().count()
        );
        dispatch_instruction(app, text, quick);
    });
}

/// Pill → core: the user cancelled the input (Esc in the card).
pub fn input_cancel(app: &AppHandle) {
    let Some(state) = app.try_state::<AgentState>() else {
        return;
    };
    if !state.input_active.swap(false, Ordering::SeqCst) {
        return;
    }
    app.state::<Arc<AudioRecordingManager>>().cancel_recording();
    crate::bridge::emit(app, DaemonEvent::AgentInputHide);
    input_cancel_cleanup(app);
}

/// Destroy the pre-warmed hidden panel and release the transient shortcuts —
/// Esc during input means "never mind" ("destroy if not in use").
fn input_cancel_cleanup(app: &AppHandle) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(panel) = app2.get_webview_window(PANEL_LABEL) {
            let _ = panel.close();
        }
    });
    // The Destroyed handler also triggers this, but a failed pre-create means
    // no window (and no Destroyed) — release explicitly either way.
    unregister_transient_shortcuts_deferred(app);
}

/// Pill → core: typing started (`true` → drop the voice capture) or the user
/// tabbed back to voice (`false` → restart dictation).
pub fn input_typing(app: &AppHandle, active: bool) {
    let live = app
        .try_state::<AgentState>()
        .map(|s| s.input_active.load(Ordering::SeqCst))
        .unwrap_or(false);
    if !live {
        return;
    }
    if active {
        app.state::<Arc<AudioRecordingManager>>().cancel_recording();
    } else {
        start_dictation(app);
    }
}

/// Route a submitted instruction: Quick Agent runs headlessly and pastes at the
/// cursor; the normal path hands it to the (already revealed or revealing)
/// panel, which runs the LLM itself.
fn dispatch_instruction(app: AppHandle, text: String, quick: bool) {
    // Grain Space capture runs HEADLESS — structure + save the note with no
    // panel and no confirmation surface (the app confirms the save elsewhere).
    if current_mode(&app) == AgentMode::Capture {
        capture_run(app, text, None, true);
        return;
    }
    // Quick Agent pastes the reply at the cursor. It is now the user's
    // per-submit choice (Shift held → quick), not a global setting — so a plain
    // Enter always opens the panel (fixing "nothing to paste into" when the
    // user just wants to ask). Recall never pastes, so it ignores `quick`.
    if current_mode(&app) == AgentMode::Assist && quick {
        quick_run(app, text);
        return;
    }
    // Queue first, then poke: the panel consumes via `agent_take_instruction`
    // on BOTH the event and its mount, so delivery is race-free and deduped
    // (take is consuming).
    set_pending_instruction(&app, text);
    reveal_panel_loading(&app);
    let _ = app.emit_to(PANEL_LABEL, "agent-instruction", ());
}

/// Show the (pre-created, warm) panel bottom-right in its loading state and arm
/// the panel-phase transients. Idempotent — safe to call when already visible.
/// Builds the window on the spot if pre-creation failed.
fn reveal_panel_loading(app: &AppHandle) {
    register_one_transient(app, close_binding());
    register_followup_shortcut(app);
    register_one_transient(app, submit_binding()); // compact → global Enter = Confirm

    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let win = match app2.get_webview_window(PANEL_LABEL) {
            Some(w) => w,
            None => match build_window(&app2, PANEL_LABEL, PANEL_COMPACT_W, PANEL_COMPACT_H) {
                Ok(w) => {
                    place_panel(&w, false);
                    w
                }
                Err(e) => {
                    error!("[GRAIN] agent: failed to build panel for reveal: {e}");
                    return;
                }
            },
        };
        let _ = app2.emit_to(PANEL_LABEL, "agent-loading", ());
        show_and_focus(&win);
    });
}

/// Surface an Agent failure on the panel (the only remaining Agent webview):
/// reveal it (building it if needed) and hand it the message. The emit is
/// slightly deferred so a freshly built webview has mounted its listener.
fn deliver_agent_error(app: &AppHandle, message: &str) {
    warn!("[GRAIN] agent: {message}");
    register_one_transient(app, close_binding());
    let message = message.to_string();
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        let existed = app.get_webview_window(PANEL_LABEL).is_some();
        if !existed {
            match build_window(&app, PANEL_LABEL, PANEL_COMPACT_W, PANEL_COMPACT_H) {
                Ok(w) => place_panel(&w, false),
                Err(e) => {
                    error!("[GRAIN] agent: failed to build panel for error: {e}");
                    return;
                }
            }
        }
        if let Some(w) = app.get_webview_window(PANEL_LABEL) {
            show_and_focus(&w);
        }
        let delay = if existed { 30 } else { 400 };
        let app_for_emit = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay));
            let _ = app_for_emit.emit_to(PANEL_LABEL, "agent-error", message);
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
                if id == AGENT_FOLLOWUP_BINDING || id == "cancel" || id == "transcribe_send_to_ai" {
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
        // Let an in-flight handoff settle (close-then-reopen happens within
        // ~100ms). If a surface is back up by then, IT owns the transients —
        // tearing down now would race its registration and leave Escape/Enter
        // dangling (seen as "Hotkey already registered" warnings followed by a
        // dead Escape).
        std::thread::sleep(Duration::from_millis(150));
        if app.get_webview_window(PANEL_LABEL).is_some() {
            return;
        }
        // The native input phase owns Enter/Escape too — never tear down under it.
        let input_live = app
            .try_state::<AgentState>()
            .map(|s| s.input_active.load(Ordering::SeqCst))
            .unwrap_or(false);
        if input_live {
            return;
        }
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

/// Called by the transient global Enter shortcut. During the INPUT phase the
/// pill owns the typed text, so the core asks it to submit (it answers with
/// SubmitText/SubmitVoice over the WS). On the compact panel the frontend owns
/// the displayed reply version and answers with `agent_confirm_paste`.
pub fn global_submit(app: &AppHandle) {
    let input_live = app
        .try_state::<AgentState>()
        .map(|s| s.input_active.load(Ordering::SeqCst))
        .unwrap_or(false);
    if input_live {
        crate::bridge::emit(app, DaemonEvent::AgentInputSubmitRequest);
    } else if app.get_webview_window(PANEL_LABEL).is_some() {
        let _ = app.emit_to(PANEL_LABEL, "agent-global-enter", ());
    }
}

/// Called by the transient follow-up shortcut (and the pill's offer click):
/// expand the VISIBLE panel in place; hand a HIDDEN (warm, quick-agent) panel
/// the retained conversation and reveal it expanded; or rebuild from scratch.
pub fn open_followup(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window(PANEL_LABEL) {
        if panel.is_visible().unwrap_or(false) {
            // The frontend expands itself (and calls `agent_set_panel_mode`).
            let _ = app.emit_to(PANEL_LABEL, "agent-followup", ());
            show_and_focus(&panel);
            return;
        }

        // Hidden warm panel (Quick Agent): only meaningful with history.
        if !has_conversation(app) {
            return;
        }
        clear_followup_offer(app);
        register_one_transient(app, close_binding());
        register_followup_shortcut(app);
        // Expanded → the global Enter must stay unregistered (in-window input).
        let _ = crate::shortcut::unregister_shortcut(app, submit_binding());

        // Not on the main thread (we're on a shortcut/WS thread), so resizing
        // the window here is safe (tauri#3990 only bites main-thread resizes).
        place_panel(&panel, true);
        // The panel is already mounted: it re-checks the retained conversation
        // on this event (take is consuming, so double delivery is harmless).
        let _ = app.emit_to(PANEL_LABEL, "agent-followup-open", ());
        show_and_focus(&panel);
        return;
    }

    // Windowless path (the warm panel died): only meaningful with history.
    if !has_conversation(app) {
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

fn has_conversation(app: &AppHandle) -> bool {
    app.try_state::<AgentState>()
        .map(|s| {
            s.conversation
                .lock()
                .map(|g| !g.is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

// ============================================================================
// Quick Agent (headless run → paste at cursor → pill follow-up offer)
// ============================================================================

/// [GRAIN] Quick Agent: run the conversation headlessly, then refocus the
/// summon target and paste the reply at the cursor. A selection still held in
/// the target app is replaced by the paste — which is exactly the "rewrite the
/// selected chunk" behavior, with no synthetic select-all/erase. Ends by
/// offering "ask follow-up" through the pill; the warm hidden panel stays alive
/// exactly as long as that offer does.
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
                deliver_agent_error(&app, &e);
            }
        }
    });
}

/// Grain Space capture, HEADLESS: structure the spoken/typed text (+ the
/// selection captured at summon) into a note and save it silently — no panel, no
/// paste, no reply surface. The save is confirmed elsewhere in the app, not by
/// the agent surfaces (user directive). Mirrors `quick_run`'s off-thread shape.
/// How long the in-card "Saved" confirmation stays up before the card hides.
const CAPTURE_SAVED_HOLD: Duration = Duration::from_millis(1100);

/// `title` is an explicit note title (typed note; `None` → auto-generate).
/// `use_selection` grabs the summon selection as the note body (voice capture of
/// a highlighted passage); a typed note authored in the card passes `false`.
fn capture_run(app: AppHandle, body: String, title: Option<String>, use_selection: bool) {
    std::thread::spawn(move || {
        let selection = if use_selection {
            read_summon_context(&app).0
        } else {
            None
        };
        let saved = matches!(
            tauri::async_runtime::block_on(crate::grain_space::capture::capture_and_save(
                &app,
                &body,
                selection.as_deref(),
                title.as_deref(),
            )),
            Ok(true)
        );
        if saved {
            // Confirm the save on the SAME summon card (green "Saved"), hold
            // briefly, then hide it — no new pill/surface.
            crate::bridge::emit(&app, DaemonEvent::AgentInputSaved);
            std::thread::sleep(CAPTURE_SAVED_HOLD);
        }
        crate::bridge::emit(&app, DaemonEvent::AgentInputHide);
        // No panel took over the input-phase Enter/Esc — release them.
        unregister_transient_shortcuts_deferred(&app);
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
    // Escape dismisses the offer (the pill fades out) — registered transiently
    // for the offer's short lifetime, released by the expiry/teardown paths.
    register_one_transient(app, close_binding());
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
            // The warm panel dies with the offer ("destroy if not in use") —
            // but only while still hidden; a visible panel belongs to the user.
            let app3 = app2.clone();
            let _ = app2.run_on_main_thread(move || {
                if let Some(panel) = app3.get_webview_window(PANEL_LABEL) {
                    if !panel.is_visible().unwrap_or(false) {
                        let _ = panel.close();
                    }
                }
            });
            // Release the transients (slightly deferred so the close lands; the
            // Destroyed teardown handles the rest, this covers no-window paths).
            std::thread::sleep(Duration::from_millis(200));
            if app2.get_webview_window(PANEL_LABEL).is_none() {
                unregister_followup_shortcut(&app2);
                let _ = crate::shortcut::unregister_shortcut(&app2, close_binding());
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
/// wedged webview can still be dismissed without quitting the whole app. During
/// the INPUT phase it cancels the whole summon (input + dictation + warm
/// panel); afterwards it closes the panel and dismisses a live follow-up offer
/// (the pill fades out).
pub fn global_close(app: &AppHandle) {
    let input_live = app
        .try_state::<AgentState>()
        .map(|s| s.input_active.load(Ordering::SeqCst))
        .unwrap_or(false);
    if input_live {
        input_cancel(app);
        return;
    }

    let app_for_main = app.clone();
    let _ = app.run_on_main_thread(move || {
        app_for_main
            .state::<Arc<AudioRecordingManager>>()
            .cancel_recording();

        // Withdrawing the offer FIRST means the window teardown below sees it
        // inactive and releases the transient follow-up shortcut too.
        clear_followup_offer(&app_for_main);

        if let Some(panel) = app_for_main.get_webview_window(PANEL_LABEL) {
            let _ = panel.close();
        }

        if app_for_main.get_webview_window(PANEL_LABEL).is_none() {
            unregister_transient_shortcuts_deferred(&app_for_main);
        }
    });
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
) -> Result<AgentReply, String> {
    // Grain Recall drives a different brain behind the panel: retrieve from
    // saved memories, synthesize an answer with evidence. (Capture never reaches
    // here — it runs headless from `dispatch_instruction`, no panel.) The mode is
    // whatever the summoning binding fixed — never guessed.
    if current_mode(&app) == AgentMode::Recall {
        return crate::grain_space::recall::run_turn(&app, &messages).await;
    }
    let field = app
        .try_state::<AgentState>()
        .and_then(|s| s.field_context.lock().ok().and_then(|g| g.clone()));
    let text = run_conversation(&app, &messages, context.as_deref(), field.as_ref()).await?;
    Ok(AgentReply::plain(text))
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
    let full = build_messages(messages, context, field);
    run_messages(app, full).await
}

/// Run an ALREADY-BUILT `(role, content)` message list through the configured
/// AI (single provider or the smart-rotation pool). Shared by the assistant
/// (`run_conversation`, which prepends its selection/field framing) and Grain
/// Recall (`recall.rs`, which builds its own system prompt + memories block).
pub(crate) async fn run_messages(
    app: &AppHandle,
    full: Vec<(String, String)>,
) -> Result<String, String> {
    let settings = get_settings(app);

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

// ============================================================================
// Native tool-calling path (Grain Recall's search_memory)
// ============================================================================

/// One tool-enabled turn's reply: the model's free-text answer (may be empty
/// when it only wants tools) plus any tool calls the caller must execute and
/// feed back. Grain Recall drives the bounded agentic loop in `recall.rs`.
pub(crate) struct LlmToolReply {
    pub content: String,
    pub tool_calls: Vec<crate::llm_client::ToolCallOut>,
}

/// Run ONE tool-enabled turn through the configured AI (single provider or the
/// smart-rotation pool). Mirrors [`run_messages`] but carries `tools` and can
/// return `tool_calls`. tool-call ids are opaque strings we echo back, so a
/// different rotation provider answering a later hop is harmless.
///
/// Providers that don't support tools (or the local Apple Intelligence path)
/// simply never return a tool call → the model answers from the context already
/// injected into the conversation. That silent degrade is intentional: native
/// tools are a refinement, never a hard dependency (RECALL retrieval always
/// pre-injects a strong first pass).
pub(crate) async fn run_messages_with_tools(
    app: &AppHandle,
    entries: Vec<crate::llm_client::ChatEntry>,
    tools: Vec<crate::llm_client::ToolSpec>,
) -> Result<LlmToolReply, String> {
    let settings = get_settings(app);

    let http_client = app
        .try_state::<reqwest::Client>()
        .map(|s| s.inner().clone())
        .ok_or("Agent: shared HTTP client unavailable")?;

    if settings.post_process_smart_rotation {
        return agent_run_rotated_tools(app, &http_client, entries, tools).await;
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

    let (outcome, reply) =
        run_agent_once_tools(&http_client, &provider, model, api_key, &entries, &tools).await;
    match outcome {
        CallOutcome::Ok { .. } => Ok(reply),
        CallOutcome::RateLimited { .. } => Err(format!(
            "{} is rate-limited right now — try again shortly.",
            provider.label
        )),
        CallOutcome::Failed => Err(format!("{} could not produce a response.", provider.label)),
    }
}

/// Smart-rotation failover for the tool path. Reuses the shared health-ordered
/// driver ([`run_with_rotation`]) for provider selection + tracker learning; the
/// structured reply is captured out-of-band (the driver's text return is unused
/// here) so `CallOutcome` stays a text-only contract for every other caller.
async fn agent_run_rotated_tools(
    app: &AppHandle,
    http_client: &reqwest::Client,
    entries: Vec<crate::llm_client::ChatEntry>,
    tools: Vec<crate::llm_client::ToolSpec>,
) -> Result<LlmToolReply, String> {
    crate::post_process_router::reset_quota_if_new_day(app);
    let settings = get_settings(app);

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

    let est_text: String = entries
        .iter()
        .map(|e| match e {
            crate::llm_client::ChatEntry::System(c)
            | crate::llm_client::ChatEntry::User(c)
            | crate::llm_client::ChatEntry::Assistant(c)
            | crate::llm_client::ChatEntry::ToolResult { content: c, .. } => c.as_str(),
            crate::llm_client::ChatEntry::AssistantToolCalls(_) => "",
        })
        .collect::<Vec<_>>()
        .join(" ");
    let est_tokens = provider_router::estimate_tokens(&est_text);
    let candidates: Vec<(String, String)> = eligible
        .iter()
        .map(|p| (p.id.clone(), p.base_url.clone()))
        .collect();

    // Captured out-of-band: the winning provider's structured reply. The driver
    // only knows about the (unused) text projection in `CallOutcome::Ok`.
    let captured: Arc<Mutex<Option<LlmToolReply>>> = Arc::new(Mutex::new(None));

    crate::rotation_state::run_with_rotation(
        &trackers.llm,
        &candidates,
        est_tokens,
        |id| {
            let http_client = http_client.clone();
            let eligible = &eligible;
            let settings = &settings;
            let entries = &entries;
            let tools = &tools;
            let captured = Arc::clone(&captured);
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
                let (outcome, reply) =
                    run_agent_once_tools(&http_client, provider, model, api_key, entries, tools)
                        .await;
                if matches!(outcome, CallOutcome::Ok { .. }) {
                    if let Ok(mut g) = captured.lock() {
                        *g = Some(reply);
                    }
                }
                outcome
            }
        },
        |id| {
            crate::post_process_router::record_usage(app, id);
            log::info!("[GRAIN] agent (tools) routed to '{id}'");
        },
    )
    .await?;

    captured
        .lock()
        .ok()
        .and_then(|mut g| g.take())
        .ok_or_else(|| "tool turn produced no reply".to_string())
}

/// Run ONE tool-enabled provider call with already-resolved model/key. Returns
/// a [`CallOutcome`] for the rotation tracker plus the structured reply. A
/// response that carries ONLY tool calls (empty content) is still a success.
async fn run_agent_once_tools(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    model: String,
    api_key: String,
    entries: &[crate::llm_client::ChatEntry],
    tools: &[crate::llm_client::ToolSpec],
) -> (CallOutcome, LlmToolReply) {
    let empty_reply = || LlmToolReply {
        content: String::new(),
        tool_calls: Vec::new(),
    };

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

    // Apple Intelligence (local, no HTTP, no tool support): flatten and answer
    // from the injected context. It never emits tool calls — that's fine.
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            if !crate::apple_intelligence::check_apple_intelligence_availability() {
                return (CallOutcome::Failed, empty_reply());
            }
            let pairs = tool_entries_to_pairs(entries);
            let (system, user) = flatten_for_single_prompt(&pairs);
            let token_limit = model.trim().parse::<i32>().unwrap_or(0);
            return match crate::apple_intelligence::process_text_with_system_prompt(
                &system,
                &user,
                token_limit,
            ) {
                Ok(result) if !result.trim().is_empty() => (
                    CallOutcome::Ok {
                        text: result.clone(),
                        remaining_requests: None,
                        remaining_tokens: None,
                        total_tokens: None,
                    },
                    LlmToolReply {
                        content: result,
                        tool_calls: Vec::new(),
                    },
                ),
                _ => (CallOutcome::Failed, empty_reply()),
            };
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            return (CallOutcome::Failed, empty_reply());
        }
    }

    let response = tokio::time::timeout(
        AGENT_LLM_TIMEOUT,
        crate::llm_client::send_chat_with_tools(
            client,
            provider,
            api_key,
            &model,
            entries.to_vec(),
            tools_cloned(tools),
            reasoning_effort,
            reasoning,
        ),
    )
    .await;

    match response {
        Err(_) => {
            warn!(
                "[GRAIN] agent (tools) provider '{}' timed out after {}s",
                provider.id,
                AGENT_LLM_TIMEOUT.as_secs()
            );
            (CallOutcome::Failed, empty_reply())
        }
        Ok(Ok(result)) => {
            let content = result.content.unwrap_or_default();
            let has_output = !content.trim().is_empty() || !result.tool_calls.is_empty();
            if has_output {
                (
                    CallOutcome::Ok {
                        text: content.clone(),
                        remaining_requests: result.remaining_requests,
                        remaining_tokens: result.remaining_tokens,
                        total_tokens: result.total_tokens,
                    },
                    LlmToolReply {
                        content,
                        tool_calls: result.tool_calls,
                    },
                )
            } else {
                (CallOutcome::Failed, empty_reply())
            }
        }
        Ok(Err(LlmError::RateLimited { retry_after_s })) => {
            (CallOutcome::RateLimited { retry_after_s }, empty_reply())
        }
        Ok(Err(LlmError::Other(e))) => {
            warn!("[GRAIN] agent (tools) provider '{}' failed: {e}", provider.id);
            (CallOutcome::Failed, empty_reply())
        }
    }
}

/// Clone tool specs for a retry/round-trip (the JSON schema is small and this
/// path is active-turn-only, so the copy is negligible).
fn tools_cloned(tools: &[crate::llm_client::ToolSpec]) -> Vec<crate::llm_client::ToolSpec> {
    tools
        .iter()
        .map(|t| crate::llm_client::ToolSpec {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.parameters.clone(),
        })
        .collect()
}

/// Flatten tool-path entries into `(role, content)` pairs for the local
/// single-prompt backend (Apple Intelligence). Tool-call assistant turns are
/// dropped (that backend never produces them) and tool results are folded in as
/// user context.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn tool_entries_to_pairs(entries: &[crate::llm_client::ChatEntry]) -> Vec<(String, String)> {
    use crate::llm_client::ChatEntry;
    entries
        .iter()
        .filter_map(|e| match e {
            ChatEntry::System(c) => Some(("system".to_string(), c.clone())),
            ChatEntry::User(c) => Some(("user".to_string(), c.clone())),
            ChatEntry::Assistant(c) => Some(("assistant".to_string(), c.clone())),
            ChatEntry::ToolResult { content, .. } => {
                Some(("user".to_string(), format!("Memory search results:\n{content}")))
            }
            ChatEntry::AssistantToolCalls(_) => None,
        })
        .collect()
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
