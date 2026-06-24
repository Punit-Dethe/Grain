mod actions;
mod agent; // [GRAIN] summoned voice-first AI window (Phase 7)
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple_intelligence;
mod audio_feedback;
pub mod audio_toolkit;
mod bridge; // [GRAIN] Tauri-shell → headless DaemonEvent bus
pub mod cli;
mod clipboard;
mod commands;
mod events_server; // [GRAIN] local WebSocket event transport to the pill
mod helpers;
mod input;
mod llm_client;
mod managers;
mod overlay;
pub mod portable;
mod post_process_router; // [GRAIN] post-process (LLM) dispatcher (single vs rotation)
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
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tauri::image::Image;
pub use transcription_coordinator::TranscriptionCoordinator;

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_log::{Builder as LogBuilder, RotationStrategy, Target, TargetKind};

use crate::settings::get_settings;

// Global atomic to store the file log level filter
// We use u8 to store the log::LevelFilter as a number
pub static FILE_LOG_LEVEL: AtomicU8 = AtomicU8::new(log::LevelFilter::Debug as u8);

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

    // Initialize the managers
    let recording_manager = Arc::new(
        AudioRecordingManager::new(app_handle).expect("Failed to initialize recording manager"),
    );
    let model_manager =
        Arc::new(ModelManager::new(app_handle).expect("Failed to initialize model manager"));
    let transcription_manager = Arc::new(
        TranscriptionManager::new(app_handle, model_manager.clone())
            .expect("Failed to initialize transcription manager"),
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
    // [GRAIN] real-time rolling transcription engine (on-demand model + idle-unload).
    let rolling_transcriber = Arc::new(rolling::RollingTranscriber::default());
    rolling_transcriber.start_idle_watcher(app_handle.clone());
    app_handle.manage(rolling_transcriber);
    // [GRAIN] smart-rotation health trackers (one per domain), shared by the STT
    // and post-process routers for cooldown-aware provider ordering.
    app_handle.manage(Arc::new(rotation_state::RotationTrackers::default()));
    // [GRAIN] Agent: holds the selection captured at summon time until the window
    // reads it on mount. The window itself is created on demand and destroyed on close.
    app_handle.manage(agent::AgentState::default());

    // [GRAIN] start the local WebSocket event transport + launch/supervise the pill.
    if let Some(ctx) = app_handle.try_state::<Arc<grain_core::AppContext>>() {
        events_server::start(ctx.inner().clone());
    }
    events_server::spawn_pill_supervisor();

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
            "check_updates" => {
                let settings = settings::get_settings(app);
                if settings.update_checks_enabled {
                    show_main_window(app);
                    let _ = app.emit("check-for-updates", ());
                }
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
fn trigger_update_check(app: AppHandle) -> Result<(), String> {
    let settings = settings::get_settings(&app);
    if !settings.update_checks_enabled {
        return Ok(());
    }
    app.emit("check-for-updates", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn show_main_window_command(app: AppHandle) -> Result<(), String> {
    show_main_window(&app);
    Ok(())
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
            shortcut::change_paste_method_setting,
            shortcut::get_available_typing_tools,
            shortcut::change_typing_tool_setting,
            shortcut::change_external_script_path_setting,
            shortcut::change_clipboard_handling_setting,
            shortcut::change_auto_submit_setting,
            shortcut::change_auto_submit_key_setting,
            shortcut::change_post_process_enabled_setting,
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
            shortcut::suspend_binding,
            shortcut::resume_binding,
            shortcut::change_mute_while_recording_setting,
            shortcut::change_audio_conditioning_setting,
            shortcut::change_append_trailing_space_setting,
            shortcut::change_lazy_stream_close_setting,
            shortcut::change_app_language_setting,
            shortcut::change_update_checks_setting,
            shortcut::change_keyboard_implementation_setting,
            shortcut::get_keyboard_implementation,
            shortcut::change_show_tray_icon_setting,
            shortcut::change_whisper_accelerator_setting,
            shortcut::change_ort_accelerator_setting,
            shortcut::change_whisper_gpu_device,
            shortcut::get_available_accelerators,
            shortcut::handy_keys::start_handy_keys_recording,
            shortcut::handy_keys::stop_handy_keys_recording,
            trigger_update_check,
            show_main_window_command,
            agent::agent_get_context,
            agent::agent_set_instruction,
            agent::agent_take_instruction,
            agent::agent_show_panel,
            agent::agent_submit_instruction,
            agent::agent_start_dictation,
            agent::agent_stop_dictation,
            agent::agent_cancel_dictation,
            agent::agent_copy,
            agent::agent_run,
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
        .events(collect_events![managers::history::HistoryUpdatePayload,]);

    #[cfg(debug_assertions)] // <- Only export on non-release builds
    specta_builder
        .export(
            Typescript::default().bigint(BigIntExportBehavior::Number),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

    let invoke_handler = specta_builder.invoke_handler();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .device_event_filter(tauri::DeviceEventFilter::Always)
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            LogBuilder::new()
                .level(log::LevelFilter::Trace) // Set to most verbose level globally
                .max_file_size(500_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .clear_targets()
                .targets([
                    // Console output respects RUST_LOG environment variable
                    Target::new(TargetKind::Stdout).filter({
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
                ])
                .build(),
        );

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--toggle-transcription") {
                signal_handle::send_transcription_input(app, "transcribe", "CLI");
            } else if args.iter().any(|a| a == "--toggle-post-process") {
                signal_handle::send_transcription_input(app, "transcribe_with_post_process", "CLI");
            } else if args.iter().any(|a| a == "--cancel") {
                crate::utils::cancel_current_operation(app);
            } else {
                show_main_window(app);
            }
        }))
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

            // Create main window programmatically so we can set data_directory
            // for portable mode (redirects WebView2 cache to portable Data dir).
            // [GRAIN] Shared with show_main_window so the window can be destroyed
            // on close (free RAM) and recreated on reopen.
            build_main_window(&app.handle())?;

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

            // [GRAIN] Accelerator enumeration is NOT pre-warmed at boot anymore.
            // `transcribe_rs::whisper_cpp::gpu::list_gpu_devices` loads the
            // Vulkan/CUDA/DirectML backends (the full NVIDIA compute stack —
            // nvgpucomp64/nvoglv64/nvcuda64/nvptxJitCompiler64/directml, ~200MB of
            // mapped images + committed driver heaps) just to fill the Advanced
            // accelerator dropdown. Grain is a tray app that idles in the
            // background, so holding that resident for a settings page nobody may
            // open is the wrong trade ("if it's not in use, destroy it"). The
            // `get_available_accelerators` command still runs (and caches in its
            // OnceLock) the first time the Advanced page asks for it — that page
            // already shows a loading state, so the one-time cost is paid lazily,
            // only when actually needed.

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
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { .. } => {
                if window.label() == agent::PALETTE_LABEL {
                    if let Some(rm) = window
                        .app_handle()
                        .try_state::<Arc<AudioRecordingManager>>()
                    {
                        rm.cancel_recording();
                    }
                    if window
                        .app_handle()
                        .get_webview_window(agent::PANEL_LABEL)
                        .is_none()
                    {
                        agent::unregister_transient_shortcuts_deferred(&window.app_handle());
                    }
                    return;
                }

                if window.label() == agent::PANEL_LABEL {
                    if window
                        .app_handle()
                        .get_webview_window(agent::PALETTE_LABEL)
                        .is_none()
                    {
                        agent::unregister_transient_shortcuts_deferred(&window.app_handle());
                    }
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
            // tray "Quit" uses app.exit(0), which bypasses this prevention.
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                if !app.state::<CliArgs>().no_tray {
                    api.prevent_exit();
                }
            }
            let _ = (app, event); // suppress unused warnings on non-macOS
        });
}
