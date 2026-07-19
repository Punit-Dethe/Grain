//! [GRAIN] Grain-only Tauri settings commands, out of the Handy-derived
//! `shortcut/mod.rs` (Handy Isolation phase 6). Upstream owns the shortcut
//! registration/dispatch machinery in that module; these are the setting
//! mutators for Grain's own features — context awareness, auto-dictionary,
//! "scrap that", snippets, voice actions, app modes, the Agent, Grain Space,
//! rolling preview, audio conditioning.
//!
//! Each is still a `#[tauri::command]`, so the command NAME (and therefore the
//! frontend `invoke` + generated bindings) is unchanged by the move; only the
//! path in `lib.rs`'s `collect_commands!` differs.

use crate::settings;
use crate::settings::DefaultPanel;
use crate::shortcut::{register_shortcut, unregister_shortcut};
use log::warn;
use tauri::{AppHandle, Manager};

/// [GRAIN] A one-shot snapshot of the current foreground app, for the "capture
/// focused app" button when creating a mode. Backend-side detection so the same
/// exe-stem normalization used at match time pre-fills the matcher exactly.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, specta::Type)]
pub struct DetectedApp {
    /// Executable stem (the value a `Process` mode matches on).
    pub exe: String,
    /// Full, launchable executable path (for voice actions' app capture). Empty
    /// when it couldn't be resolved.
    pub exe_path: String,
    /// Human-facing name (window title, for display).
    pub name: String,
    /// Browser address-bar host, when the foreground app is a browser and the
    /// URL reader resolved it. `None` otherwise.
    pub url_host: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn update_snippets(app: AppHandle, snippets: Vec<settings::Snippet>) -> Result<(), String> {
    // Persist only usable rules: a snippet needs a non-blank trigger and a
    // non-empty expansion. The UI enforces this too; this guards direct
    // invoke calls.
    let snippets: Vec<settings::Snippet> = snippets
        .into_iter()
        .filter(|s| !s.trigger.trim().is_empty() && !s.replacement.is_empty())
        .collect();
    let mut settings = settings::get_settings(&app);
    settings.snippets = snippets;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Persist the user's voice actions (trigger → open apps/sites). Drops
/// entries with a blank trigger or no targets, and prunes blank target values —
/// the UI enforces this too, but this guards direct invoke calls.
#[tauri::command]
#[specta::specta]
pub fn update_actions(app: AppHandle, actions: Vec<settings::VoiceAction>) -> Result<(), String> {
    let actions: Vec<settings::VoiceAction> = actions
        .into_iter()
        .map(|mut a| {
            a.targets.retain(|t| match t {
                settings::ActionTarget::App(v) | settings::ActionTarget::Url(v) => {
                    !v.trim().is_empty()
                }
            });
            a
        })
        .filter(|a| !a.trigger.trim().is_empty() && !a.targets.is_empty())
        .collect();
    let mut settings = settings::get_settings(&app);
    settings.actions = actions;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Toggle context awareness (post-processing SOFT context + user MODES).
#[tauri::command]
#[specta::specta]
pub fn change_context_awareness_enabled_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.context_awareness_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Toggle auto-add-to-dictionary. Off = zero overhead (no watcher spawns).
#[tauri::command]
#[specta::specta]
pub fn change_auto_dictionary_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.auto_dictionary_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Toggle the "scrap that" voice reset. Off = zero overhead (the snippet
/// matcher is never invoked for it and the live preview takes its normal path).
#[tauri::command]
#[specta::specta]
pub fn change_scrap_that_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.scrap_that_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Toggle the silent nearby-term hints (reads focused-field unique tokens
/// via UI Automation). Only effective when context awareness is also on.
#[tauri::command]
#[specta::specta]
pub fn change_context_nearby_terms_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.context_nearby_terms = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Agent auto-copy policy (off / first reply / all replies).
#[tauri::command]
#[specta::specta]
pub fn change_agent_autocopy_setting(
    app: AppHandle,
    mode: settings::AgentAutocopy,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.agent_autocopy = mode;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Toggle Quick Agent (palette submit → headless AI run → paste at cursor).
#[tauri::command]
#[specta::specta]
pub fn change_agent_quick_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.agent_quick_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Agent context awareness mode (off / unique terms / full field text).
#[tauri::command]
#[specta::specta]
pub fn change_agent_context_mode_setting(
    app: AppHandle,
    mode: settings::AgentContextMode,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.agent_context_mode = mode;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Toggle "type to expand" on the native agent input.
#[tauri::command]
#[specta::specta]
pub fn change_agent_input_type_to_expand_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.agent_input_type_to_expand = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Agent reply-surface position (side card vs center-top panel).
#[tauri::command]
#[specta::specta]
pub fn change_agent_panel_position_setting(
    app: AppHandle,
    position: settings::AgentPanelPosition,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.agent_panel_position = position;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Persist the user's per-app / per-site modes (hard formatting). Drops
/// entries missing a name, prompt, or a non-blank matcher value — the UI enforces
/// this too, but this guards direct invoke calls.
#[tauri::command]
#[specta::specta]
pub fn update_app_modes(app: AppHandle, modes: Vec<settings::AppMode>) -> Result<(), String> {
    let modes: Vec<settings::AppMode> = modes
        .into_iter()
        .filter(|m| {
            let has_target = match &m.matcher {
                settings::AppMatch::Process(v) | settings::AppMatch::UrlHost(v) => {
                    !v.trim().is_empty()
                }
            };
            !m.name.trim().is_empty() && !m.prompt.trim().is_empty() && has_target
        })
        .collect();
    let mut settings = settings::get_settings(&app);
    settings.app_modes = modes;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Detect the foreground app right now. Returns `None` when nothing can be
/// resolved (unsupported platform, no foreground window). Silent — no UI.
#[tauri::command]
#[specta::specta]
pub fn detect_active_app() -> Option<DetectedApp> {
    // The capture button only needs the app/URL, not focused-field terms.
    crate::context_detect::detect_active_context(false).map(|c| DetectedApp {
        exe: c.exe,
        exe_path: c.exe_path,
        name: c.app_name,
        url_host: c.url_host,
    })
}

#[tauri::command]
#[specta::specta]
pub fn change_default_panel_setting(app: AppHandle, panel: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match panel.as_str() {
        "settings" => DefaultPanel::Settings,
        "quick_panel" => DefaultPanel::QuickPanel,
        other => {
            warn!("Invalid default panel '{}', defaulting to settings", other);
            DefaultPanel::Settings
        }
    };
    settings.default_panel = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Master toggle for Grain Space. Registers/unregisters the feature's
/// global shortcuts immediately so OFF is zero-overhead without a restart.
/// Never touches on-disk note data.
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.grain_space_enabled = enabled;
    settings::write_settings(&app, settings.clone());

    for (id, binding) in settings.bindings.iter() {
        if !id.starts_with("grain_space_") {
            continue;
        }
        if enabled {
            let _ = register_shortcut(&app, binding.clone());
        } else {
            let _ = unregister_shortcut(&app, binding.clone());
        }
    }

    // Arm (or tear down) the reminder timer for the new state.
    crate::grain_space::reminders::sync(&app);

    // Feature off ⇒ TRULY destroy the window (not sleep) so nothing of the
    // workspace stays resident, then drop the embedding engine. Disabled must
    // mean zero footprint.
    if !enabled {
        crate::grain_space::window::destroy(&app);
        crate::grain_space::embed::shutdown_engine();
    }

    Ok(())
}

/// [GRAIN] Grain Space semantic-search toggle. Flips the setting; the model
/// download (opt-in consent flow) is driven by the frontend before it turns
/// this on. OFF must guarantee the embedding model never loads — any resident
/// engine is dropped immediately.
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_semantic_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.grain_space_semantic = enabled;
    settings::write_settings(&app, settings);
    if !enabled {
        crate::grain_space::embed::shutdown_engine();
    }
    Ok(())
}

/// [GRAIN] Load the embedding model in f16 (half RAM) vs f32. Drops any resident
/// engine so the next embed re-loads at the chosen precision.
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_embed_f16_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.grain_space_embed_f16 = enabled;
    settings::write_settings(&app, settings);
    crate::grain_space::embed::set_use_f16(enabled);
    crate::grain_space::embed::shutdown_engine();
    Ok(())
}

/// [GRAIN] Grain Space backend hard switch (OBSIDIAN-PLAN.md §1). Swapping the
/// backend changes which corpus every surface sees; the overlay is closed and
/// the embedding engine dropped so nothing keeps serving the old corpus.
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_backend_setting(
    app: AppHandle,
    backend: settings::GrainSpaceBackend,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    if settings.grain_space_backend == backend {
        return Ok(());
    }
    settings.grain_space_backend = backend;
    settings::write_settings(&app, settings);
    // The corpus changes wholesale — destroy the window so it rebuilds against
    // the new backend rather than showing a stale (slept) view of the old one.
    crate::grain_space::window::destroy(&app);
    crate::grain_space::embed::shutdown_engine();
    crate::grain_space::reminders::sync(&app);
    Ok(())
}

/// [GRAIN] Set the Obsidian vault path (an existing folder). Validated here so
/// the vault backend never runs against a bogus path.
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_vault_path_setting(app: AppHandle, path: String) -> Result<(), String> {
    let trimmed = path.trim().to_string();
    if !trimmed.is_empty() && !std::path::Path::new(&trimmed).is_dir() {
        return Err("That folder does not exist.".to_string());
    }
    let mut settings = settings::get_settings(&app);
    settings.grain_space_vault_path = trimmed;
    settings::write_settings(&app, settings);
    // Different vault ⇒ different corpus: destroy so it rebuilds fresh.
    crate::grain_space::window::destroy(&app);
    crate::grain_space::embed::shutdown_engine();
    crate::grain_space::reminders::sync(&app);
    Ok(())
}

/// [GRAIN] Subfolder of the vault where Grain writes captures ("Grain" by
/// default). Kept a simple relative name — path separators and dot-segments
/// are rejected so it can never escape the vault.
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_vault_folder_setting(
    app: AppHandle,
    folder: String,
) -> Result<(), String> {
    let trimmed = folder
        .trim()
        .trim_matches('/')
        .trim_matches('\\')
        .to_string();
    if trimmed.is_empty() || trimmed.contains(['/', '\\', ':']) || trimmed.starts_with('.') {
        return Err("Folder must be a plain name like \"Grain\".".to_string());
    }
    let mut settings = settings::get_settings(&app);
    settings.grain_space_vault_folder = trimmed;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Auto-arm reminders extracted from captured notes (vs. manual arm).
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_auto_reminders_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.grain_space_auto_reminders = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Auto-categorization: route captured notes into existing Grain folders
/// (AUTO-CATEGORIZATION-PLAN.md). Off by default.
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_auto_categorize_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.grain_space_auto_categorize = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Toggle voice conditioning (85 Hz high-pass + boost-only AGC for quiet
/// mics). Persists the setting and live-updates the open recorder so it applies
/// to the next captured frame without a restart. (Rolling re-reads it per session.)
#[tauri::command]
#[specta::specta]
pub fn change_audio_conditioning_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.audio_conditioning = enabled;
    settings::write_settings(&app, settings);

    if let Some(rm) =
        app.try_state::<std::sync::Arc<crate::managers::audio::AudioRecordingManager>>()
    {
        rm.set_conditioning(enabled);
    }
    Ok(())
}

/// [GRAIN] Toggle the rolling live preview (Studio Window caption during
/// rolling dictation). Persisted only; each rolling session reads it at start,
/// so OFF sessions never spawn the preview machinery — zero compute overhead.
#[tauri::command]
#[specta::specta]
pub fn change_rolling_live_preview_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.rolling_live_preview = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}
