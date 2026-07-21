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
    sync_agent_reply_surface_slot(&app);
    Ok(())
}

/// [GRAIN] Keep the `agent.reply-surface` slot claim in step with the Agent's
/// position setting (SPEC §3.2 + §10.2).
///
/// The centre-layout variant is the one occupant whose truth lives outside the
/// registry: enabling it merely adds it to the position dropdown, and
/// *selecting* it is what takes the slot. Reconciling here means a third-party
/// reply-surface pack sees the centre variant as the incumbent — rather than
/// seeing Grain's default and displacing a shipped look nobody mentioned.
///
/// A third-party occupant is never overwritten; this only ever moves the slot
/// between core and the centre variant.
pub fn sync_agent_reply_surface_slot(app: &AppHandle) {
    use grain_core::extensions as ext;
    let Some(reg) = app.try_state::<std::sync::Arc<ext::ExtensionsRegistry>>() else {
        return;
    };
    match reg.slot_occupant(ext::AGENT_REPLY_SURFACE_SLOT).as_deref() {
        Some(ext::CORE_DEFAULT) | Some(ext::AGENT_CENTER_VARIANT_ID) | None => {}
        Some(_) => return,
    }
    let center = settings::get_settings(app).agent_panel_position
        == settings::AgentPanelPosition::Center
        && reg.is_enabled(ext::AGENT_CENTER_VARIANT_ID);
    let occupant = if center {
        ext::AGENT_CENTER_VARIANT_ID
    } else {
        ext::CORE_DEFAULT
    };
    if let Err(e) = reg.set_slot_claim(ext::AGENT_REPLY_SURFACE_SLOT, occupant) {
        log::warn!("[GRAIN] could not sync the agent reply-surface slot: {e}");
    }
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

// ── [GRAIN] Extension platform, Phase 1 (SPEC §5.1, §10.1) ──────────────────

/// One row of the Extensions Overview tab. Built-ins delegate their enabled
/// state to core settings flags (manifest-first, PLAN.md D4); installed packs
/// read the registry.
#[derive(serde::Serialize, specta::Type, Clone)]
pub struct ExtensionCard {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    /// "builtin" | "pack"
    pub tier: String,
    pub enabled: bool,
    /// Toggle-order position (SPEC §4.4); u64::MAX = never toggled (sorts last).
    /// Sent as string — u64 doesn't survive JS numbers.
    pub toggle_seq: String,
    pub repository: Option<String>,
}

fn builtin_card(
    id: &str,
    name: &str,
    description: &str,
    enabled: bool,
    reg: &grain_core::extensions::ExtensionsRegistry,
) -> ExtensionCard {
    ExtensionCard {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        tier: "builtin".to_string(),
        enabled,
        toggle_seq: reg.toggle_seq(id).to_string(),
        repository: None,
    }
}

/// The Overview tab's data: every extension, enabled and disabled alike.
#[tauri::command]
#[specta::specta]
pub fn extensions_overview(app: AppHandle) -> Result<Vec<ExtensionCard>, String> {
    use grain_core::extensions as ext;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let settings = settings::get_settings(&app);

    let mut cards = vec![
        builtin_card(
            ext::BUILTIN_SNIPPETS,
            "Snippets",
            "Speak a trigger word and Grain expands it into your saved text.",
            settings.snippets_enabled,
            &reg,
        ),
        builtin_card(
            ext::BUILTIN_CONTEXT,
            "Context Awareness",
            "Detects the app you're dictating into and adapts AI formatting to it.",
            settings.context_awareness_enabled,
            &reg,
        ),
        builtin_card(
            ext::BUILTIN_AGENT,
            "Agent",
            "Summon a voice-first AI assistant on your current selection.",
            settings.agent_enabled,
            &reg,
        ),
    ];
    // Imported packs (everything in the registry that isn't the pre-known
    // centre variant handled below).
    for rec in reg.records() {
        if rec.id == ext::AGENT_CENTER_VARIANT_ID {
            continue;
        }
        let (name, description, repository) = match load_pack(&app, &rec.id) {
            Ok(p) => (
                p.manifest.name,
                p.manifest.description,
                p.manifest.repository,
            ),
            // SPEC §6 last row: a broken/missing pack file renders an error
            // card; it never takes the page down.
            Err(e) => (rec.id.clone(), format!("Unreadable pack: {e}"), None),
        };
        cards.push(ExtensionCard {
            id: rec.id.clone(),
            name,
            description,
            version: rec.installed_version.clone(),
            tier: "pack".to_string(),
            enabled: rec.enabled,
            toggle_seq: rec.toggle_seq.to_string(),
            repository,
        });
    }
    if let Some(rec) = reg.record(ext::AGENT_CENTER_VARIANT_ID) {
        cards.push(ExtensionCard {
            id: rec.id.clone(),
            name: "Agent — Centre layout".to_string(),
            description: "An alternative centred look for the Agent's reply panel.".to_string(),
            version: rec.installed_version.clone(),
            tier: "pack".to_string(),
            enabled: rec.enabled,
            toggle_seq: rec.toggle_seq.to_string(),
            repository: None,
        });
    }
    Ok(cards)
}

/// Flip an extension on/off (SPEC §5.1 inline toggle). Built-ins write their
/// settings flag + bump toggle order; packs write the registry. The Agent
/// toggle re-registers its binding so the change is zero-overhead-when-off.
#[tauri::command]
#[specta::specta]
pub fn extension_set_enabled(app: AppHandle, id: String, enabled: bool) -> Result<(), String> {
    use grain_core::extensions as ext;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;

    match id.as_str() {
        ext::BUILTIN_SNIPPETS => {
            let mut settings = settings::get_settings(&app);
            settings.snippets_enabled = enabled;
            settings::write_settings(&app, settings);
        }
        ext::BUILTIN_CONTEXT => {
            let mut settings = settings::get_settings(&app);
            settings.context_awareness_enabled = enabled;
            settings::write_settings(&app, settings);
        }
        ext::BUILTIN_AGENT => {
            let mut settings = settings::get_settings(&app);
            settings.agent_enabled = enabled;
            settings::write_settings(&app, settings.clone());
            // Mirror the Grain Space pattern: the summon binding registers/
            // unregisters live so disabled truly means no global hook.
            if let Some(binding) = settings.bindings.get("summon_agent") {
                if enabled {
                    let _ = register_shortcut(&app, binding.clone());
                } else {
                    let _ = unregister_shortcut(&app, binding.clone());
                }
            }
        }
        ext::AGENT_CENTER_VARIANT_ID => {
            reg.set_enabled(&id, enabled).map_err(|e| e.to_string())?;
            // SPEC §10.2: disabling the variant while it is the active look
            // falls the position back to the built-in default (side).
            if !enabled {
                let mut settings = settings::get_settings(&app);
                if settings.agent_panel_position == settings::AgentPanelPosition::Center {
                    settings.agent_panel_position = settings::AgentPanelPosition::Side;
                    settings::write_settings(&app, settings);
                }
            }
            sync_agent_reply_surface_slot(&app);
            return Ok(());
        }
        // Imported packs: registry bit + payload application.
        pack_id if reg.is_installed(pack_id) => {
            let pack = load_pack(&app, pack_id)?;
            // [GRAIN] SPEC §6 (the Chrome model): a scripted extension is HELD
            // at first enable until the user approves the capabilities its
            // manifest requests. Never grant implicitly — the whole point is
            // that code cannot start running on capabilities nobody approved.
            // The frontend catches this structured error, shows the permission
            // sheet, calls `extension_grant`, and retries.
            if enabled && pack.is_scripted() {
                let granted = reg.record(pack_id).map(|r| r.granted).unwrap_or_default();
                let missing: Vec<String> = pack
                    .manifest
                    .permissions
                    .iter()
                    .filter(|p| !granted.contains(p))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    return Err(
                        serde_json::json!({ "needsPermissions": missing }).to_string()
                    );
                }
            }
            // [GRAIN] SPEC §3.2: at most one enabled occupant per slot, and a
            // contested claim reaches the user as an explicit takeover — never
            // a silent steal, never load-order dependent. Same structured-error
            // shape as the permission sheet above, so the frontend flow matches.
            if enabled {
                if let Some(c) = reg.slot_conflict(pack_id) {
                    return Err(serde_json::json!({ "slotConflict": c }).to_string());
                }
            }
            reg.set_enabled(pack_id, enabled).map_err(|e| e.to_string())?;
            if let Some(ctx) = app.try_state::<std::sync::Arc<grain_core::AppContext>>() {
                ctx.update_settings(|s| {
                    if enabled {
                        ext::apply_prompt_pack(s, pack_id, &pack.payloads.prompts);
                    } else {
                        ext::remove_prompt_pack(s, pack_id);
                    }
                })
                .map_err(|e| e.to_string())?;
            }
            // The activation/transform index is what the paste path and event
            // bus read; it must never lag the registry.
            crate::extension_host::refresh_index(&app);
            return Ok(());
        }
        other => return Err(format!("unknown extension id '{other}'")),
    }
    if enabled {
        let _ = reg.touch_builtin_toggle(&id);
    }
    Ok(())
}

/// Where imported `.grainpack` files live: `<data>/extensions/<id>.grainpack.json`.
fn pack_path(app: &AppHandle, id: &str) -> Result<std::path::PathBuf, String> {
    let ctx = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .ok_or("app context unavailable")?;
    let dir = ctx.data_dir.join("extensions");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(format!("{id}.grainpack.json")))
}

fn load_pack(app: &AppHandle, id: &str) -> Result<grain_sdk::GrainPack, String> {
    let raw = std::fs::read_to_string(pack_path(app, id)?).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// Import a `.grainpack` file (SPEC §1.1 tier A-inert). Validates, copies into
/// the extensions dir, registers it DISABLED — enabling is the user's explicit
/// second step in Overview, where toggle order is assigned.
#[tauri::command]
#[specta::specta]
pub fn extension_import_pack(app: AppHandle, path: String) -> Result<String, String> {
    use grain_core::extensions as ext;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("read {path}: {e}"))?;
    let pack: grain_sdk::GrainPack =
        serde_json::from_str(&raw).map_err(|e| format!("not a valid .grainpack: {e}"))?;
    pack.validate()?;

    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let id = pack.manifest.id.clone();
    std::fs::write(
        pack_path(&app, &id)?,
        serde_json::to_string_pretty(&pack).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // Re-import/update of an installed pack must PRESERVE the user's state
    // (SPEC §6 update row) — resetting enabled/toggle order on update would
    // silently disable a working pack.
    let prior = reg.record(&id);
    let was_enabled = prior.as_ref().map(|r| r.enabled).unwrap_or(false);
    reg.install(ext::ExtensionRecord {
        id: id.clone(),
        enabled: was_enabled,
        toggle_seq: prior.as_ref().map(|r| r.toggle_seq).unwrap_or(0),
        installed_version: pack.manifest.version.clone(),
        granted: prior.map(|r| r.granted).unwrap_or_default(),
        slots: pack.manifest.slots.clone(),
    })
    .map_err(|e| e.to_string())?;
    // An enabled pack's payloads refresh in place (apply is idempotent).
    if was_enabled {
        if let Some(ctx) = app.try_state::<std::sync::Arc<grain_core::AppContext>>() {
            ctx.update_settings(|s| ext::apply_prompt_pack(s, &id, &pack.payloads.prompts))
                .map_err(|e| e.to_string())?;
        }
    }
    crate::extension_host::refresh_index(&app);
    Ok(id)
}

/// Record the user's approval of a scripted extension's capabilities (SPEC §6).
/// Called by the permission sheet on Approve; the caller then retries enable.
///
/// Grants are clamped to what the manifest actually requests, so neither a
/// compromised frontend nor a stale sheet can widen an extension's reach beyond
/// what the user was shown.
#[tauri::command]
#[specta::specta]
pub fn extension_grant(
    app: AppHandle,
    id: String,
    permissions: Vec<String>,
) -> Result<(), String> {
    use grain_core::extensions as ext;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let mut rec = reg
        .record(&id)
        .ok_or_else(|| format!("'{id}' is not installed"))?;
    let requested = load_pack(&app, &id)?.manifest.permissions;
    if let Some(extra) = permissions.iter().find(|p| !requested.contains(p)) {
        return Err(format!("'{extra}' is not requested by this extension"));
    }
    for p in permissions {
        if !rec.granted.contains(&p) {
            rec.granted.push(p);
        }
    }
    reg.install(rec).map_err(|e| e.to_string())
}

/// Record the user's answer to a slot takeover prompt (SPEC §3.2). Hands `slot`
/// to `id` and disables whoever held it, in one step — the counterpart to
/// `extension_grant` for the `slotConflict` error. The caller then retries
/// enable, which now sees the slot as its own.
///
/// This is the ONLY path that moves a slot between extensions: `set_enabled`
/// refuses a contested claim, so a takeover is always something the user chose.
#[tauri::command]
#[specta::specta]
pub fn extension_take_slot(app: AppHandle, id: String, slot: String) -> Result<(), String> {
    use grain_core::extensions as ext;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let displaced = reg.take_slot(&id, &slot).map_err(|e| e.to_string())?;

    // Displacing the centre layout must also drop the position setting, or the
    // Agent would keep rendering a look whose slot it no longer owns.
    let center_lost = displaced.as_deref() == Some(ext::AGENT_CENTER_VARIANT_ID)
        || (slot == ext::AGENT_REPLY_SURFACE_SLOT && id != ext::AGENT_CENTER_VARIANT_ID);
    if center_lost {
        let mut settings = settings::get_settings(&app);
        if settings.agent_panel_position == settings::AgentPanelPosition::Center {
            settings.agent_panel_position = settings::AgentPanelPosition::Side;
            settings::write_settings(&app, settings);
        }
    }
    if let Some(prev) = &displaced {
        // The loser is disabled by `take_slot`; its payloads must come off too.
        if let Some(ctx) = app.try_state::<std::sync::Arc<grain_core::AppContext>>() {
            let _ = ctx.update_settings(|s| ext::remove_prompt_pack(s, prev));
        }
        log::info!("[GRAIN] slot '{slot}' taken by '{id}' (was '{prev}')");
    }
    crate::extension_host::refresh_index(&app);
    Ok(())
}

/// Export an installed pack to `dest` (SPEC §5.1 "shareable data packs").
#[tauri::command]
#[specta::specta]
pub fn extension_export_pack(app: AppHandle, id: String, dest: String) -> Result<(), String> {
    std::fs::copy(pack_path(&app, &id)?, &dest)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Uninstall a pack. `purge` also deletes the stored pack file; without it the
/// file stays for lossless reinstall (SPEC §6 keep-by-default). Applied
/// payloads are always removed.
#[tauri::command]
#[specta::specta]
pub fn extension_uninstall(app: AppHandle, id: String, purge: bool) -> Result<(), String> {
    use grain_core::extensions as ext;
    if id == ext::AGENT_CENTER_VARIANT_ID || id.starts_with("grain.") {
        // Built-in-shipped entries have no reinstall source until the store
        // ships; disabling is their supported off-switch.
        return Err("built-in extensions can be disabled, not uninstalled".into());
    }
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    if let Some(ctx) = app.try_state::<std::sync::Arc<grain_core::AppContext>>() {
        let _ = ctx.update_settings(|s| ext::remove_prompt_pack(s, &id));
    }
    reg.uninstall(&id).map_err(|e| e.to_string())?;
    if purge {
        let _ = std::fs::remove_file(pack_path(&app, &id)?);
    }
    crate::extension_host::refresh_index(&app);
    Ok(())
}
