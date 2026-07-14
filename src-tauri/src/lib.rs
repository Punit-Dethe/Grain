mod actions;
mod agent; // [GRAIN] summoned voice-first AI window (Phase 7)
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple_intelligence;
mod audio_feedback;
pub mod audio_toolkit;
mod bridge; // [GRAIN] Tauri-shell → headless DaemonEvent bus
mod catalog;
pub mod cli;
mod clipboard;
mod commands;
mod context_detect; // [GRAIN] foreground app/site detection + three-stage prompt composition
mod dictionary; // [GRAIN] auto-add-to-dictionary: watch pasted-field edits, learn respellings
mod events_server; // [GRAIN] local WebSocket event transport to the pill
mod grain_space; // [GRAIN] Grain Space: zero-idle-RAM local notes (flat JSON + derived index)
mod helpers;
mod input;
mod llm_client;
mod managers;
mod overlay;
pub mod portable;
mod post_process_router; // [GRAIN] post-process (LLM) dispatcher (single vs rotation)
mod prompt_record; // [GRAIN] Prompt Record: split content vs spoken AI instruction at the pill-click mark
mod rolling; // [GRAIN] real-time rolling-window transcription engine
mod rotation_state; // [GRAIN] smart-rotation trackers (cooldowns + headroom), shared by both routers
mod settings;
mod shortcut;
mod signal_handle;
mod stt_client; // [GRAIN] S2: HTTP STT adapters (OpenAI / Deepgram / AssemblyAI)
mod stt_router; // [GRAIN] S3: STT dispatcher (local vs cloud rotation)
mod transcription_coordinator;
mod tray;
mod tray_i18n;
mod utils;
mod voice_actions; // [GRAIN] voice actions: spoken trigger → open apps/sites

pub use cli::CliArgs;
#[cfg(debug_assertions)]
use specta_typescript::{BigIntExportBehavior, Typescript};
use tauri_specta::{collect_commands, collect_events, Builder};

use env_filter::Builder as EnvFilterBuilder;
use managers::audio::AudioRecordingManager;
use managers::history::HistoryManager;
use managers::model::ModelManager;
use managers::transcription::TranscriptionManager;
#[cfg(unix)]
use signal_hook::consts::{SIGUSR1, SIGUSR2};
#[cfg(unix)]
use signal_hook::iterator::Signals;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tauri::image::Image;

// Global static to allow tray "Quit" to bypass prevent_exit
pub static INTENTIONAL_QUIT: AtomicBool = AtomicBool::new(false);
pub use transcription_coordinator::TranscriptionCoordinator;

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Listener, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_log::{Builder as LogBuilder, RotationStrategy, Target, TargetKind};

use crate::settings::get_settings;

// Global atomic to store the file log level filter
// We use u8 to store the log::LevelFilter as a number
pub static FILE_LOG_LEVEL: AtomicU8 = AtomicU8::new(log::LevelFilter::Debug as u8);

/// When `true`, log records are also forwarded to the webview via the
/// `log://log` event for the debug panel's live log viewer. Gated on debug
/// mode — the live log viewer is its only consumer and only exists in debug
/// mode — so normal runs never broadcast log records (which can include file
/// paths or transcribed text) onto the frontend event bus. Synced at startup
/// and whenever debug mode is toggled (see `shortcut::change_debug_mode_setting`).
pub static WEBVIEW_LOG_STREAMING: AtomicBool = AtomicBool::new(false);

fn level_filter_from_u8(value: u8) -> log::LevelFilter {
    match value {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        5 => log::LevelFilter::Trace,
        _ => log::LevelFilter::Trace,
    }
}

fn build_console_filter() -> env_filter::Filter {
    let mut builder = EnvFilterBuilder::new();

    match std::env::var("RUST_LOG") {
        Ok(spec) if !spec.trim().is_empty() => {
            if let Err(err) = builder.try_parse(&spec) {
                log::warn!(
                    "Ignoring invalid RUST_LOG value '{}': {}. Falling back to info-level console logging",
                    spec,
                    err
                );
                builder.filter_level(log::LevelFilter::Info);
            }
        }
        _ => {
            builder.filter_level(log::LevelFilter::Info);
        }
    }

    builder.build()
}

/// [GRAIN] Build the main settings window (hidden). Extracted so it can be
/// recreated on demand after the window is destroyed on close (freeing WebView2
/// RAM). Sets the portable data_directory like the original setup did.
fn build_main_window(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let mut win_builder =
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
            .title("Grain")
            // [GRAIN] Native Quick Panel console size (1280×760). The window is
            // locked to this 1280:760 aspect ratio (see the Resized handler) so it
            // only scales up/down — never stretches — and the content (scaled by a
            // single transform) stays pixel-proportional with no letterboxing.
            .inner_size(1280.0, 760.0)
            .min_inner_size(960.0, 570.0)
            .resizable(true)
            // [GRAIN] Custom themed title bar: drop the native OS frame and let
            // the webview own the top strip (drag region + minimize/maximize/
            // close), styled in Grain's paper/ink/dither language. macOS keeps
            // its overlay traffic-light buttons instead of a custom frame.
            .maximizable(true)
            .decorations(cfg!(target_os = "macos"));

    // [GRAIN] Windows/Linux: OPAQUE frameless window. We tried a transparent
    // window so the React card's `rounded-[36px]` could be the window edge, but
    // on a large always-visible main window that produces a translucent corner
    // halo + a faint WebView2 edge outline (inherent to transparent WebView2
    // compositing). Instead keep the window opaque and let Windows 11 round +
    // clip the real window via DWM (see apply_window_corner_rounding below) —
    // pixel-crisp corners, no translucency, no compositing overhead. Windows 10
    // (no DWM corner support) gets square corners, consistent with other apps.
    // macOS keeps native decorations + vibrancy.
    #[cfg(not(target_os = "macos"))]
    {
        win_builder = win_builder.transparent(false);
    }

    win_builder = win_builder.visible(false);

    if let Some(data_dir) = portable::data_dir() {
        win_builder = win_builder.data_directory(data_dir.join("webview"));
    }

    let window = win_builder.build()?;

    // [GRAIN] On Windows 11, restore the OS default corner preference so the
    // window gets Windows 11's natural subtle rounding, clipped cleanly at the
    // real window frame with no CSS compositing artifacts.
    #[cfg(target_os = "windows")]
    apply_window_corner_rounding(&window);

    Ok(window)
}

/// [GRAIN] Windows 11: force the OS window corner preference to `DWMWCP_ROUND`.
///
/// This lets Windows 11 apply its natural subtle window corner rounding, clipped
/// at the real window frame — pixel-crisp, no CSS compositing overhead, no dark
/// corner halo. Falls back gracefully on Windows 10 (no-op).
///
/// Silently no-ops on any call that fails — corner style is cosmetic.
#[cfg(target_os = "windows")]
fn apply_window_corner_rounding(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    let hwnd = match window.hwnd() {
        Ok(h) => HWND(h.0 as *mut _),
        Err(e) => {
            log::warn!("[GRAIN] could not get main window HWND for corner preference: {e}");
            return;
        }
    };
    // DWMWCP_ROUND == 2: force rounding on borderless windows (Win11).
    let preference = DWMWCP_ROUND.0;
    unsafe {
        match DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        ) {
            Ok(()) => {}
            Err(e) => log::debug!("[GRAIN] DWM corner preference not applied (likely Win10): {e}"),
        }
    }
}

fn show_main_window(app: &AppHandle) {
    // [GRAIN] The window is destroyed on close to free RAM, so recreate it if
    // absent. Every reopen path (tray, single-instance, macOS Reopen) lands here.
    let main_window = match app.get_webview_window("main") {
        Some(w) => w,
        None => match build_main_window(app) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to recreate main window: {}", e);
                return;
            }
        },
    };

    if let Err(e) = main_window.unminimize() {
        log::error!("Failed to unminimize webview window: {}", e);
    }
    if let Err(e) = main_window.show() {
        log::error!("Failed to show webview window: {}", e);
    }
    if let Err(e) = main_window.set_focus() {
        log::error!("Failed to focus webview window: {}", e);
    }
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
            log::error!("Failed to set activation policy to Regular: {}", e);
        }
    }
}

#[allow(unused_variables)]
fn should_force_show_permissions_window(app: &AppHandle) -> bool {
    #[cfg(target_os = "windows")]
    {
        let model_manager = app.state::<Arc<ModelManager>>();
        let has_downloaded_models = model_manager
            .get_available_models()
            .iter()
            .any(|model| model.is_downloaded);

        if !has_downloaded_models {
            return false;
        }

        let status = commands::audio::get_windows_microphone_permission_status();
        if status.supported && status.overall_access == commands::audio::PermissionAccess::Denied {
            log::info!(
                "Windows microphone permissions are denied; forcing main window visible for onboarding"
            );
            return true;
        }
    }

    false
}

fn initialize_core_logic(app_handle: &AppHandle) {
    // Note: Enigo (keyboard/mouse simulation) is NOT initialized here.
    // The frontend is responsible for calling the `initialize_enigo` command
    // after onboarding completes. This avoids triggering permission dialogs
    // on macOS before the user is ready.

    // Initialize the managers. The audio recorder receives the streaming router
    // explicitly, so always-on microphone startup can wire live-preview frames
    // even before Tauri state is populated.
    let model_manager =
        Arc::new(ModelManager::new(app_handle).expect("Failed to initialize model manager"));
    let transcription_manager = Arc::new(
        TranscriptionManager::new(app_handle, model_manager.clone())
            .expect("Failed to initialize transcription manager"),
    );
    let recording_manager = Arc::new(
        AudioRecordingManager::new(app_handle, transcription_manager.stream_router())
            .expect("Failed to initialize recording manager"),
    );
    let history_manager =
        Arc::new(HistoryManager::new(app_handle).expect("Failed to initialize history manager"));

    // Apply accelerator preferences before any model loads
    managers::transcription::apply_accelerator_settings(app_handle);

    // Add managers to Tauri's managed state
    app_handle.manage(recording_manager.clone());
    app_handle.manage(model_manager.clone());
    app_handle.manage(transcription_manager.clone());
    app_handle.manage(history_manager.clone());
    // [GRAIN] Register the transcribe-cpp compute backends ONCE before any model
    // load — with `dynamic-backends` this dlopens the ggml modules next to the
    // exe; skipping it leaves ZERO compute devices and every GGUF load fails.
    managers::transcription::init_transcribe_backend();
    // [GRAIN] Rolling-window driver. Since the transcribe-cpp unification it owns
    // NO engine of its own — chunks are transcribed through the shared
    // TranscriptionManager (one resident model across Batch/Rolling/Native ASR).
    let rolling_transcriber = Arc::new(rolling::RollingTranscriber::new(
        transcription_manager.clone(),
    ));
    app_handle.manage(rolling_transcriber);
    // [GRAIN] smart-rotation health trackers (one per domain), shared by the STT
    // and post-process routers for cooldown-aware provider ordering.
    app_handle.manage(Arc::new(rotation_state::RotationTrackers::default()));
    // [GRAIN] One shared reqwest::Client for ALL outbound HTTP calls (LLM + STT).
    // reqwest::Client is designed to be cloned/shared — it manages a connection pool,
    // TLS sessions, and keep-alive internally. Building one per request throws all of
    // that away. Centralising here means every provider call reuses connections.
    let shared_http_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("failed to build shared HTTP client");
    app_handle.manage(shared_http_client);
    // [GRAIN] Agent: holds the selection captured at summon time until the window
    // reads it on mount. The window itself is created on demand and destroyed on close.
    app_handle.manage(agent::AgentState::default());

    // [GRAIN] start the local WebSocket event transport + launch/supervise the pill.
    if let Some(ctx) = app_handle.try_state::<Arc<grain_core::AppContext>>() {
        events_server::start(ctx.inner().clone(), app_handle.clone());
    }
    events_server::spawn_pill_supervisor();

    // [GRAIN] Grain Space reminders: fire anything that came due while the app
    // was closed and park a timer for the next one. No-op (no disk touch, no
    // timer) when the feature is disabled.
    grain_space::reminders::sync(app_handle);
    // Seed the embedding precision from the persisted setting (the engine layer
    // has no AppHandle; it reads this at spawn).
    grain_space::embed::set_use_f16(get_settings(app_handle).grain_space_embed_f16);

    // Note: Shortcuts are NOT initialized here.
    // The frontend is responsible for calling the `initialize_shortcuts` command
    // after permissions are confirmed (on macOS) or after onboarding completes.
    // This matches the pattern used for Enigo initialization.

    #[cfg(unix)]
    let signals = Signals::new(&[SIGUSR1, SIGUSR2]).unwrap();
    // Set up signal handlers for toggling transcription
    #[cfg(unix)]
    signal_handle::setup_signal_handler(app_handle.clone(), signals);

    // Apply macOS Accessory policy if starting hidden and tray is available.
    // If the tray icon is disabled, keep the dock icon so the user can reopen.
    #[cfg(target_os = "macos")]
    {
        let settings = settings::get_settings(app_handle);
        if settings.start_hidden && settings.show_tray_icon {
            let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
    }
    // Get the current theme to set the appropriate initial icon
    let initial_theme = tray::get_current_theme(app_handle);

    // Choose the appropriate initial icon based on theme
    let initial_icon_path = tray::get_icon_path(initial_theme, tray::TrayIconState::Idle);

    let tray = TrayIconBuilder::new()
        .icon(
            Image::from_path(
                app_handle
                    .path()
                    .resolve(initial_icon_path, tauri::path::BaseDirectory::Resource)
                    .unwrap(),
            )
            .unwrap(),
        )
        .tooltip(tray::tray_tooltip())
        .show_menu_on_left_click(true)
        .icon_as_template(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => {
                show_main_window(app);
            }
            "copy_last_transcript" => {
                tray::copy_last_transcript(app);
            }
            "unload_model" => {
                let transcription_manager = app.state::<Arc<TranscriptionManager>>();
                if !transcription_manager.is_model_loaded() {
                    log::warn!("No model is currently loaded.");
                    return;
                }
                match transcription_manager.unload_model() {
                    Ok(()) => log::info!("Model unloaded via tray."),
                    Err(e) => log::error!("Failed to unload model via tray: {}", e),
                }
            }
            "cancel" => {
                use crate::utils::cancel_current_operation;

                // Use centralized cancellation that handles all operations
                cancel_current_operation(app);
            }
            "quit" => {
                INTENTIONAL_QUIT.store(true, Ordering::Relaxed);
                app.exit(0);
            }
            id if id.starts_with("model_select:") => {
                let model_id = id.strip_prefix("model_select:").unwrap().to_string();
                let current_model = settings::get_settings(app).selected_model;
                if model_id == current_model {
                    return;
                }
                let app_clone = app.clone();
                std::thread::spawn(move || {
                    match commands::models::switch_active_model(&app_clone, &model_id) {
                        Ok(()) => {
                            log::info!("Model switched to {} via tray.", model_id);
                        }
                        Err(e) => {
                            log::error!("Failed to switch model via tray: {}", e);
                        }
                    }
                    tray::update_tray_menu(&app_clone, &tray::TrayIconState::Idle, None);
                });
            }
            _ => {}
        })
        .build(app_handle)
        .unwrap();
    app_handle.manage(tray);

    // Initialize tray menu with idle state
    utils::update_tray_menu(app_handle, &utils::TrayIconState::Idle, None);

    // Apply show_tray_icon setting
    let settings = settings::get_settings(app_handle);
    if !settings.show_tray_icon {
        tray::set_tray_visibility(app_handle, false);
    }

    // Refresh tray menu when model state changes
    let app_handle_for_listener = app_handle.clone();
    app_handle.listen("model-state-changed", move |_| {
        tray::update_tray_menu(&app_handle_for_listener, &tray::TrayIconState::Idle, None);
    });

    // Get the autostart manager and configure based on user setting
    let autostart_manager = app_handle.autolaunch();
    let settings = settings::get_settings(&app_handle);

    if settings.autostart_enabled {
        // Enable autostart if user has opted in
        let _ = autostart_manager.enable();
    } else {
        // Disable autostart if user has opted out
        let _ = autostart_manager.disable();
    }

    // [GRAIN] The Handy webview recording overlay is retired — the winit
    // grain-pill is now the SINGLE overlay surface for both batch and rolling
    // (driven by DaemonEvents over the local WS). Nothing to create here; the
    // pill is launched + supervised separately (events_server::spawn_pill_supervisor).
}

#[tauri::command]
#[specta::specta]
fn show_main_window_command(app: AppHandle) -> Result<(), String> {
    show_main_window(&app);
    Ok(())
}

/// Headless one-shot transcription for the `--transcribe-file` / `--list-devices`
/// path. Drives the same `TranscriptionManager::transcribe` the app uses; no
/// mic, no VAD, no download. Returns a process exit code (0 ok, 1 runtime
/// failure, 2 bad input/usage).
fn run_headless_transcription(app: &AppHandle, args: &CliArgs) -> i32 {
    use std::time::Instant;

    // --list-devices: print the registered transcribe-cpp compute devices and
    // exit. Pass an index here to --device-index.
    if args.list_devices {
        println!("transcribe-cpp compute devices:");
        for d in managers::transcription::describe_compute_devices() {
            println!("  {}", d);
        }
        if args.transcribe_file.is_none() {
            return 0;
        }
    }

    let Some(wav) = args.transcribe_file.clone() else {
        return 0;
    };

    // read_wav_samples reads 16-bit int samples and does no validation; the app
    // only ever saves 16 kHz mono 16-bit PCM, so reject anything else rather than
    // transcribe garbage / mis-time / mis-decode.
    match hound::WavReader::open(&wav) {
        Ok(reader) => {
            let spec = reader.spec();
            if spec.sample_rate != 16_000
                || spec.channels != 1
                || spec.bits_per_sample != 16
                || spec.sample_format != hound::SampleFormat::Int
            {
                eprintln!(
                    "error: expected 16 kHz mono 16-bit PCM WAV, got {} Hz / {} ch / {}-bit {:?}",
                    spec.sample_rate, spec.channels, spec.bits_per_sample, spec.sample_format
                );
                return 2;
            }
        }
        Err(e) => {
            eprintln!("error: cannot open {}: {}", wav.display(), e);
            return 2;
        }
    }

    let samples = match crate::audio_toolkit::read_wav_samples(&wav) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to read {}: {}", wav.display(), e);
            return 2;
        }
    };
    let audio_secs = samples.len() as f64 / 16_000.0;

    let tm = app.state::<Arc<TranscriptionManager>>();

    let model_id = args
        .model
        .clone()
        .unwrap_or_else(|| settings::get_settings(app).selected_model);
    if model_id.is_empty() {
        eprintln!("error: no model selected (pass --model or pick one in the app)");
        return 2;
    }

    // --device-index hard-selects the compute device for this load by its
    // --list-devices index (whisper.cpp models only; not persisted). Omit it to
    // use the persisted accelerator setting.
    let device_index = args.device_index;
    let requested_device = match device_index {
        Some(idx) => format!("index {}", idx),
        None => "settings".to_string(),
    };

    // Cold load (timed).
    let load_start = Instant::now();
    if let Err(e) = tm.load_model_with_device(&model_id, device_index) {
        eprintln!("error: load_model('{}') failed: {}", model_id, e);
        return 1;
    }
    let load_ms = load_start.elapsed().as_millis() as u64;
    let bound_backend = tm.current_backend();

    let runs = args.repeat.unwrap_or(1).max(1);
    let mut times_ms: Vec<u64> = Vec::new();
    let mut text = String::new();
    for i in 0..runs {
        // If the model's unload-timeout is "Immediately", transcribe() unloads
        // the engine after each run; reload (untimed) so repeats keep working
        // and the inference timing below stays clean.
        if !tm.is_model_loaded() {
            if let Err(e) = tm.load_model_with_device(&model_id, device_index) {
                eprintln!("error: reload before run {} failed: {}", i + 1, e);
                return 1;
            }
        }
        let t = Instant::now();
        match tm.transcribe(samples.clone()) {
            Ok(out) => text = out,
            Err(e) => {
                eprintln!("error: transcribe failed: {}", e);
                return 1;
            }
        }
        times_ms.push(t.elapsed().as_millis() as u64);
    }
    let best_ms = times_ms.iter().copied().min().unwrap_or(0);
    let rtf = if best_ms > 0 {
        audio_secs / (best_ms as f64 / 1000.0)
    } else {
        0.0
    };

    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "model": model_id,
                "requested_device": requested_device,
                "bound_backend": bound_backend,
                "audio_secs": audio_secs,
                "load_ms": load_ms,
                "transcribe_ms": times_ms,
                "best_ms": best_ms,
                "rtf": rtf,
                "text": text,
            })
        );
    } else {
        println!(
            "model={} device={} backend={} audio={:.2}s load={}ms best={}ms rtf={:.2}x",
            model_id,
            requested_device,
            bound_backend.as_deref().unwrap_or("unknown"),
            audio_secs,
            load_ms,
            best_ms,
            rtf,
        );
        println!("text: {}", text);
    }
    0
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(cli_args: CliArgs) {
    // Detect portable mode before anything else
    portable::init();

    // Parse console logging directives from RUST_LOG, falling back to info-level logging
    // when the variable is unset
    let console_filter = build_console_filter();

    let specta_builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            shortcut::change_binding,
            shortcut::reset_binding,
            shortcut::change_ptt_setting,
            shortcut::change_audio_feedback_setting,
            shortcut::change_audio_feedback_volume_setting,
            shortcut::change_sound_theme_setting,
            shortcut::change_default_panel_setting,
            shortcut::change_start_hidden_setting,
            shortcut::change_autostart_setting,
            shortcut::change_translate_to_english_setting,
            shortcut::change_selected_language_setting,
            shortcut::change_overlay_position_setting,
            shortcut::change_debug_mode_setting,
            shortcut::change_word_correction_threshold_setting,
            shortcut::change_extra_recording_buffer_setting,
            shortcut::change_paste_delay_ms_setting,
            shortcut::change_paste_delay_after_ms_setting,
            shortcut::change_paste_method_setting,
            shortcut::get_available_typing_tools,
            shortcut::change_typing_tool_setting,
            shortcut::change_external_script_path_setting,
            shortcut::change_clipboard_handling_setting,
            shortcut::change_auto_submit_setting,
            shortcut::change_auto_submit_key_setting,
            shortcut::change_post_process_enabled_setting,
            shortcut::change_grain_space_enabled_setting,
            shortcut::change_grain_space_semantic_setting,
            shortcut::change_grain_space_embed_f16_setting,
            shortcut::change_grain_space_auto_reminders_setting,
            shortcut::change_grain_space_auto_categorize_setting,
            shortcut::change_grain_space_backend_setting,
            shortcut::change_grain_space_vault_path_setting,
            shortcut::change_grain_space_vault_folder_setting,
            grain_space::commands::grain_space_list_notes,
            grain_space::commands::grain_space_list_cards,
            grain_space::commands::grain_space_list_folders,
            grain_space::commands::grain_space_move_note,
            grain_space::commands::grain_space_search_notes,
            grain_space::commands::grain_space_get_note,
            grain_space::commands::grain_space_export_notes,
            grain_space::commands::grain_space_save_note,
            grain_space::commands::grain_space_create_note,
            grain_space::commands::grain_space_delete_note,
            grain_space::commands::grain_space_set_pinned,
            grain_space::commands::grain_space_arm_reminder,
            grain_space::commands::grain_space_dismiss_reminder,
            grain_space::commands::grain_space_rebuild_index,
            grain_space::commands::grain_space_pick_vault,
            grain_space::commands::grain_space_open_in_obsidian,
            grain_space::commands::grain_space_open_window,
            grain_space::commands::grain_space_close_window,
            grain_space::commands::grain_space_ui_ready,
            grain_space::commands::grain_space_sleep_ready,
            grain_space::commands::grain_space_take_focus_note,
            grain_space::commands::grain_space_embed_model_status,
            grain_space::commands::grain_space_uninstall_embed_model,
            grain_space::commands::grain_space_download_embed_model,
            grain_space::commands::grain_space_cancel_embed_model_download,
            grain_space::commands::grain_space_semantic_search,
            grain_space::commands::grain_space_recall_turn,
            grain_space::commands::grain_space_recall_reset,
            shortcut::change_experimental_enabled_setting,
            shortcut::change_post_process_base_url_setting,
            shortcut::change_post_process_api_key_setting,
            shortcut::change_post_process_model_setting,
            shortcut::set_post_process_provider,
            shortcut::fetch_post_process_models,
            shortcut::add_post_process_prompt,
            shortcut::update_post_process_prompt,
            shortcut::delete_post_process_prompt,
            shortcut::set_post_process_selected_prompt,
            shortcut::update_custom_words,
            shortcut::update_snippets,
            shortcut::update_actions,
            voice_actions::run_action,
            voice_actions::pick_action_app,
            shortcut::change_context_awareness_enabled_setting,
            shortcut::change_context_nearby_terms_setting,
            shortcut::change_agent_autocopy_setting,
            shortcut::change_agent_quick_enabled_setting,
            shortcut::change_agent_context_mode_setting,
            shortcut::change_agent_input_type_to_expand_setting,
            shortcut::change_agent_panel_position_setting,
            shortcut::change_auto_dictionary_enabled_setting,
            shortcut::change_scrap_that_enabled_setting,
            shortcut::update_app_modes,
            shortcut::detect_active_app,
            shortcut::suspend_binding,
            shortcut::resume_binding,
            shortcut::change_mute_while_recording_setting,
            shortcut::change_audio_conditioning_setting,
            shortcut::change_append_trailing_space_setting,
            shortcut::change_rolling_live_preview_setting,
            shortcut::change_lazy_stream_close_setting,
            shortcut::change_app_language_setting,
            shortcut::change_update_checks_setting,
            shortcut::change_keyboard_implementation_setting,
            shortcut::get_keyboard_implementation,
            shortcut::change_show_tray_icon_setting,
            shortcut::change_transcribe_accelerator_setting,
            shortcut::change_transcribe_gpu_device,
            shortcut::get_available_accelerators,
            shortcut::handy_keys::start_handy_keys_recording,
            shortcut::handy_keys::stop_handy_keys_recording,
            show_main_window_command,
            agent::agent_get_context,
            agent::agent_take_instruction,
            agent::agent_copy,
            agent::agent_run,
            agent::agent_set_panel_mode,
            agent::agent_resize_panel,
            agent::agent_confirm_paste,
            agent::agent_take_conversation,
            commands::cancel_operation,
            commands::is_portable,
            commands::get_app_dir_path,
            commands::get_app_settings,
            commands::stt::stt_get_pool,
            commands::stt::stt_set_smart_rotation,
            commands::stt::stt_upsert_provider,
            commands::stt::stt_remove_provider,
            commands::post_process::pp_get_pool,
            commands::post_process::pp_set_smart_rotation,
            commands::post_process::pp_upsert_provider,
            commands::post_process::pp_remove_provider,
            commands::get_default_settings,
            commands::get_log_dir_path,
            commands::set_log_level,
            commands::open_recordings_folder,
            commands::open_log_dir,
            commands::open_app_data_dir,
            commands::check_apple_intelligence_available,
            commands::initialize_enigo,
            commands::initialize_shortcuts,
            commands::models::get_available_models,
            commands::models::get_model_info,
            commands::models::download_model,
            commands::models::delete_model,
            commands::models::cancel_download,
            commands::models::set_active_model,
            commands::models::get_current_model,
            commands::models::get_transcription_model_status,
            commands::models::is_model_loading,
            commands::models::has_any_models_available,
            commands::models::has_any_models_or_downloads,
            commands::models::rescan_local_models,
            commands::native_asr::list_asr_models,
            commands::native_asr::select_asr_model,
            commands::audio::update_microphone_mode,
            commands::audio::get_microphone_mode,
            commands::audio::get_windows_microphone_permission_status,
            commands::audio::open_microphone_privacy_settings,
            commands::audio::get_available_microphones,
            commands::audio::set_selected_microphone,
            commands::audio::get_selected_microphone,
            commands::audio::get_available_output_devices,
            commands::audio::set_selected_output_device,
            commands::audio::get_selected_output_device,
            commands::audio::play_test_sound,
            commands::audio::check_custom_sounds,
            commands::audio::set_clamshell_microphone,
            commands::audio::get_clamshell_microphone,
            commands::audio::is_recording,
            commands::transcription::set_model_unload_timeout,
            commands::transcription::get_model_load_status,
            commands::transcription::unload_model_manually,
            commands::history::get_history_entries,
            commands::history::toggle_history_entry_saved,
            commands::history::get_audio_file_path,
            commands::history::delete_history_entry,
            commands::history::retry_history_entry_transcription,
            commands::history::update_history_limit,
            commands::history::update_recording_retention_period,
            helpers::clamshell::is_laptop,
        ])
        .events(collect_events![
            managers::history::HistoryUpdatePayload,
            // The live-preview events MUST be registered even though Grain's
            // webview doesn't render them (the native pill does, via the WS
            // bridge): tauri-specta's Event::emit PANICS on an unregistered
            // event, which killed the stream worker mid-lease (no pill text,
            // engine dropped, batch fallback found nothing loaded).
            managers::transcription::StreamTextEvent,
        ]);

    #[cfg(debug_assertions)] // <- Only export on non-release builds
    specta_builder
        .export(
            Typescript::default().bigint(BigIntExportBehavior::Number),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

    let invoke_handler = specta_builder.invoke_handler();

    // The headless path must run as its own instance (see the single-instance
    // note below), not forward to an already-running app.
    let headless_mode = cli_args.transcribe_file.is_some() || cli_args.list_devices;

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .device_event_filter(tauri::DeviceEventFilter::Always)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            LogBuilder::new()
                .level(log::LevelFilter::Trace) // Set to most verbose level globally
                .max_file_size(500_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .clear_targets()
                .targets([
                    // Console output respects RUST_LOG environment variable. In
                    // headless mode (--transcribe-file/--list-devices) stdout
                    // carries only the result (JSON or plain), so send console
                    // logs to stderr instead to keep stdout clean for parsing.
                    Target::new(if headless_mode {
                        TargetKind::Stderr
                    } else {
                        TargetKind::Stdout
                    })
                    .filter({
                        let console_filter = console_filter.clone();
                        move |metadata| console_filter.enabled(metadata)
                    }),
                    // File logs respect the user's settings (stored in FILE_LOG_LEVEL atomic)
                    Target::new(if let Some(data_dir) = portable::data_dir() {
                        TargetKind::Folder {
                            path: data_dir.join("logs"),
                            file_name: Some("handy".into()),
                        }
                    } else {
                        TargetKind::LogDir {
                            file_name: Some("handy".into()),
                        }
                    })
                    .filter(|metadata| {
                        let file_level = FILE_LOG_LEVEL.load(Ordering::Relaxed);
                        metadata.level() <= level_filter_from_u8(file_level)
                    }),
                    // Stream logs to the webview (via the `log://log` event) so the
                    // debug panel's live log viewer can show them in real time. Only
                    // active while debug mode is on (its sole consumer), and shares the
                    // file log level so the "Log Level" setting controls verbosity.
                    Target::new(TargetKind::Webview).filter(|metadata| {
                        WEBVIEW_LOG_STREAMING.load(Ordering::Relaxed)
                            && metadata.level()
                                <= level_filter_from_u8(FILE_LOG_LEVEL.load(Ordering::Relaxed))
                    }),
                ])
                .build(),
        );

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    // Single-instance forwards CLI args to an already-running Handy and exits.
    // That would make the headless path (--transcribe-file/--list-devices) a
    // silent no-op whenever the app is already open, so skip it in headless mode
    // and run a standalone instance instead.
    if !headless_mode {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--toggle-transcription") {
                signal_handle::send_transcription_input(app, "transcribe", "CLI");
            } else if args.iter().any(|a| a == "--toggle-post-process") {
                signal_handle::send_transcription_input(app, "transcribe_with_post_process", "CLI");
            } else if args.iter().any(|a| a == "--cancel") {
                crate::utils::cancel_current_operation(app);
            } else {
                show_main_window(app);
            }
        }));
    }

    builder
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .manage(cli_args.clone())
        .setup(move |app| {
            specta_builder.mount_events(app);

            // Headless one-shot path (`--transcribe-file` / `--list-devices`):
            // initialize only what transcription needs — store/paths (via the
            // registered plugins), the model + transcription managers, and the
            // accelerator settings — then run on a worker thread and exit. This
            // deliberately skips the window, tray, overlay, audio recorder (so it
            // never opens the mic, even with always_on_microphone set), signal
            // handlers, and autostart that the normal UI path
            // (initialize_core_logic) sets up.
            if headless_mode {
                let app_handle = app.handle().clone();
                // [GRAIN] AppContext must be staged before the managers — the
                // ModelManager reads settings during construction.
                {
                    let resource_dir = app
                        .path()
                        .resource_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."));
                    let data_dir = crate::portable::app_data_dir(&app_handle)
                        .unwrap_or_else(|_| std::path::PathBuf::from("."));
                    app.manage(grain_core::AppContext::new(resource_dir, data_dir));
                }
                // Register transcribe-cpp compute backends (required for both
                // --list-devices and any GGUF model load).
                managers::transcription::init_transcribe_backend();
                let model_manager = Arc::new(
                    ModelManager::new(&app_handle).expect("Failed to initialize model manager"),
                );
                let transcription_manager = Arc::new(
                    TranscriptionManager::new(&app_handle, model_manager.clone())
                        .expect("Failed to initialize transcription manager"),
                );
                app_handle.manage(model_manager);
                app_handle.manage(transcription_manager);
                managers::transcription::apply_accelerator_settings(&app_handle);

                let handle = app_handle.clone();
                let args = cli_args.clone();
                std::thread::spawn(move || {
                    let code = run_headless_transcription(&handle, &args);
                    // Drop the loaded engine before teardown: ggml-metal's global
                    // device free asserts (SIGABRT) if a model's Metal resources
                    // are still alive at C++ static-destructor time.
                    if let Some(tm) = handle.try_state::<Arc<TranscriptionManager>>() {
                        let _ = tm.unload_model();
                    }
                    // process::exit (not app.exit, which exits 0 regardless) so the
                    // exit code propagates to the shell. Flush first since
                    // process::exit runs no destructors / buffer flushes.
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    let _ = std::io::stderr().flush();
                    std::process::exit(code);
                });
                return Ok(());
            }

            // Create main window programmatically so we can set data_directory
            // for portable mode (redirects WebView2 cache to portable Data dir).
            // [GRAIN] This is now fully lazy (invoked by show_main_window) to save
            // RAM if the app is started hidden.

            // [GRAIN] Build the headless core context and own it as managed state
            // BEFORE any get_settings() call routes through it. AppContext loads +
            // migrates the owned settings JSON (secrets in a separate file).
            {
                let resource_dir = app
                    .path()
                    .resource_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                let data_dir = crate::portable::app_data_dir(&app.handle())
                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                app.manage(grain_core::AppContext::new(resource_dir, data_dir));
            }

            let mut settings = get_settings(&app.handle());

            // CLI --debug flag overrides debug_mode and log level (runtime-only, not persisted)
            if cli_args.debug {
                settings.debug_mode = true;
                settings.log_level = settings::LogLevel::Trace;
            }

            let tauri_log_level: tauri_plugin_log::LogLevel =
                settings::to_tauri_log_level(settings.log_level); // [GRAIN]
            let file_log_level: log::Level = tauri_log_level.into();
            // Store the file log level in the atomic for the filter to use
            FILE_LOG_LEVEL.store(file_log_level.to_level_filter() as u8, Ordering::Relaxed);
            let app_handle = app.handle().clone();
            app.manage(TranscriptionCoordinator::new(app_handle.clone()));

            initialize_core_logic(&app_handle);

            // [GRAIN] Accelerator enumeration is NOT pre-warmed at boot:
            // `get_available_accelerators` enumerates transcribe-cpp's GPU
            // devices lazily (and caches in its OnceLock) the first time the
            // Advanced page asks for it — that page already shows a loading
            // state, so the one-time cost is paid only when actually needed
            // ("if it's not in use, destroy it").

            // Hide tray icon if --no-tray was passed
            if cli_args.no_tray {
                tray::set_tray_visibility(&app_handle, false);
            }

            // Show main window only if not starting hidden.
            // CLI --start-hidden flag overrides the setting.
            // But if permission onboarding is required, always show the window.
            let should_hide = settings.start_hidden || cli_args.start_hidden;
            let should_force_show = should_force_show_permissions_window(&app_handle);

            // If start_hidden but tray is disabled, we must show the window
            // anyway. Without a tray icon, the dock is the only way back in.
            let tray_available = settings.show_tray_icon && !cli_args.no_tray;
            if should_force_show || !should_hide || !tray_available {
                show_main_window(&app_handle);
            } else {
                // [GRAIN] If we skip the frontend on startup, we must manually
                // initialize global systems (shortcuts, enigo) here because the
                // frontend won't mount and call the initialization IPC commands.
                let _ = commands::initialize_shortcuts(app_handle.clone());
                let _ = commands::initialize_enigo(app_handle.clone());
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { .. } => {
                // [GRAIN] The Agent panel is the only Agent webview (the summon
                // input is native, in the pill process). On its close, release
                // the transient Enter/Escape shortcuts — unless the native input
                // phase still owns them (guarded inside the deferred helper).
                if window.label() == agent::PANEL_LABEL {
                    agent::unregister_transient_shortcuts_deferred(&window.app_handle());
                    return;
                }

                // [GRAIN] Let the window actually CLOSE (destroy) so WebView2's
                // RAM is freed — "if it's not in use, destroy it". The app lives
                // on in the tray (auto-exit is prevented in the run loop unless
                // --no-tray), and show_main_window recreates the window on reopen.
                // The Agent window also closes-to-destroy; only the MAIN settings
                // window drives the macOS dock/activation policy.
                #[cfg(target_os = "macos")]
                if window.label() == "main" {
                    let settings = get_settings(&window.app_handle());
                    let tray_visible =
                        settings.show_tray_icon && !window.app_handle().state::<CliArgs>().no_tray;
                    if tray_visible {
                        // Tray is available: hide the dock icon, app lives in the tray
                        let res = window
                            .app_handle()
                            .set_activation_policy(tauri::ActivationPolicy::Accessory);
                        if let Err(e) = res {
                            log::error!("Failed to set activation policy: {}", e);
                        }
                    }
                    // No tray: keep the dock icon visible so the user can reopen
                }
            }
            tauri::WindowEvent::ThemeChanged(theme) => {
                log::info!("Theme changed to: {:?}", theme);
                // Update tray icon to match new theme, maintaining idle state
                utils::change_tray_icon(&window.app_handle(), utils::TrayIconState::Idle);
            }
            tauri::WindowEvent::Resized(size) => {
                // [GRAIN] Lock the MAIN window to the console's 1280:760 aspect ratio
                // so it only scales up/down — never stretches thicker/longer. The
                // Agent windows are backend-placed and must stay exempt.
                if window.label() != "main" {
                    return;
                }
                const RATIO: f64 = 1280.0 / 760.0;
                if window.is_maximized().unwrap_or(false) || size.width == 0 || size.height == 0 {
                    return;
                }

                use std::sync::atomic::{AtomicU64, Ordering};
                static LAST_SEEN: AtomicU64 = AtomicU64::new(0);
                static LAST_SET: AtomicU64 = AtomicU64::new(0);
                let pack = |w: u32, h: u32| ((w as u64) << 32) | h as u64;
                let cur = pack(size.width, size.height);

                // Ignore the Resized event our own set_size triggers — that feedback
                // loop is what made the window wobble/jitter during a drag.
                if cur == LAST_SET.load(Ordering::Relaxed) {
                    return;
                }

                let prev = LAST_SEEN.swap(cur, Ordering::Relaxed);
                let dw = (size.width as i64 - (prev >> 32) as i64).abs();
                let dh = (size.height as i64 - (prev & 0xFFFF_FFFF) as i64).abs();

                // Follow the edge the user is actively dragging: derive the OTHER
                // dimension from it, so neither edge snaps back mid-drag.
                let (target_w, target_h) = if dh > dw {
                    (((size.height as f64) * RATIO).round() as u32, size.height)
                } else {
                    (size.width, ((size.width as f64) / RATIO).round() as u32)
                };

                if target_w > 0
                    && target_h > 0
                    && ((size.width as i64 - target_w as i64).abs() > 1
                        || (size.height as i64 - target_h as i64).abs() > 1)
                {
                    LAST_SET.store(pack(target_w, target_h), Ordering::Relaxed);
                    let _ = window.set_size(tauri::PhysicalSize::new(target_w, target_h));
                }
            }
            _ => {}
        })
        .invoke_handler(invoke_handler)
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = &event {
                show_main_window(app);
            }
            // [GRAIN] Closing the settings window destroys it to free RAM. Keep the
            // process alive in the tray so dictation/rolling/pill keep working —
            // unless launched with --no-tray, where closing is meant to quit. The
            // tray "Quit" uses app.exit(0) alongside setting INTENTIONAL_QUIT, which bypasses this prevention.
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                if !app.state::<CliArgs>().no_tray && !INTENTIONAL_QUIT.load(Ordering::Relaxed) {
                    api.prevent_exit();
                }
            }
            let _ = (app, event); // suppress unused warnings on non-macOS
        });
}
