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
    let wants_center =
        settings::get_settings(app).agent_panel_position == settings::AgentPanelPosition::Center;
    let center = wants_center && reg.is_enabled(ext::AGENT_CENTER_VARIANT_ID);
    // Migration (Phase 5C): the centre layout is no longer shipped. An existing
    // user who had Centre selected but has not installed the pack falls back to
    // Side, so the Agent never tries to render a look whose extension is gone.
    if wants_center && !reg.is_enabled(ext::AGENT_CENTER_VARIANT_ID) {
        let mut s = settings::get_settings(app);
        s.agent_panel_position = settings::AgentPanelPosition::Side;
        settings::write_settings(app, s);
    }
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
    /// "builtin" | "pack" | "scripted" | "native"
    pub tier: String,
    /// "core" | "community" | "dev". Dev is permanent while loaded and is
    /// never allowed to masquerade as verified.
    pub trust: String,
    /// A separately installed copy with this id is parked beneath the active
    /// load-unpacked project.
    pub overrides_installed: bool,
    pub overridden_version: Option<String>,
    pub enabled: bool,
    /// Toggle-order position (SPEC §4.4); u64::MAX = never toggled (sorts last).
    /// Sent as string — u64 doesn't survive JS numbers.
    pub toggle_seq: String,
    pub repository: Option<String>,
    /// The pack declares settings or shortcuts, so it has a section of its own
    /// worth opening. Free to compute — Overview already reads every manifest.
    pub has_detail: bool,
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
        trust: "core".to_string(),
        overrides_installed: false,
        overridden_version: None,
        enabled,
        toggle_seq: reg.toggle_seq(id).to_string(),
        repository: None,
        // Built-ins have their own tab; the Overview name jumps there.
        has_detail: false,
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
        // Voice Actions is a built-in WITHOUT a sub-tab: its Overview row opens
        // an extension page whose custom card is the Actions editor. So unlike
        // the three above it carries `has_detail: true` and a repository link.
        ExtensionCard {
            id: ext::BUILTIN_ACTIONS.to_string(),
            name: "Voice Actions".to_string(),
            description: "Say a phrase to open apps and websites — fully local, no AI."
                .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            tier: "builtin".to_string(),
            trust: "core".to_string(),
            overrides_installed: false,
            overridden_version: None,
            enabled: settings.actions_enabled,
            toggle_seq: reg.toggle_seq(ext::BUILTIN_ACTIONS).to_string(),
            repository: Some("https://github.com/Punit-Dethe/Grain".to_string()),
            has_detail: true,
        },
    ];
    // Installed packs — including the Agent centre layout, which is now a real
    // external pack (Phase 5C) rendered through this same path, not a
    // host-synthesised special case.
    for rec in reg.records() {
        let (name, description, repository, has_detail, tier) = match load_pack(&app, &rec.id) {
            Ok(p) => {
                let has_detail = !p.manifest.contributes.settings.is_empty()
                    || !p.manifest.contributes.shortcuts.is_empty();
                let tier = match p.manifest.tier {
                    grain_sdk::Tier::Pack => "pack",
                    grain_sdk::Tier::Scripted => "scripted",
                    grain_sdk::Tier::Native => "native",
                };
                (
                    p.manifest.name,
                    p.manifest.description,
                    p.manifest.repository,
                    has_detail,
                    tier,
                )
            }
            // SPEC §6 last row: a broken/missing pack file renders an error
            // card; it never takes the page down.
            Err(e) => (
                rec.id.clone(),
                format!("Unreadable pack: {e}"),
                None,
                false,
                "pack",
            ),
        };
        cards.push(ExtensionCard {
            id: rec.id.clone(),
            name,
            description,
            version: rec.installed_version.clone(),
            tier: tier.to_string(),
            // Load-unpacked is always shown as `dev`; otherwise the rung comes
            // from the record's real trust (set only by a verified store
            // install, DISTRIBUTION-PLAN §3.2). A locally-imported pack is
            // untrusted, shown as `community`.
            trust: if rec.dev.is_some() {
                "dev".to_string()
            } else {
                match rec.trust {
                    grain_sdk::Trust::Core => "core",
                    grain_sdk::Trust::Verified => "verified",
                    grain_sdk::Trust::Experimental => "experimental",
                    grain_sdk::Trust::Dev => "community",
                }
                .to_string()
            },
            overrides_installed: reg.dev_overrides_installed(&rec.id),
            overridden_version: rec
                .dev
                .as_ref()
                .and_then(|_| reg.installed_record(&rec.id))
                .map(|installed| installed.installed_version),
            enabled: rec.enabled,
            toggle_seq: rec.toggle_seq.to_string(),
            repository,
            has_detail,
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
        ext::BUILTIN_ACTIONS => {
            let mut settings = settings::get_settings(&app);
            settings.actions_enabled = enabled;
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
            // [GRAIN] Phase 5A (DISTRIBUTION-PLAN §3.1, §5.3): a revoked
            // extension cannot run again, enforced from the cached revocation
            // list BEFORE a worker is ever spawned — so it holds even if the
            // machine has been offline since the revocation was published.
            if enabled {
                if let Some(store) =
                    app.try_state::<std::sync::Arc<crate::grain_store::StoreState>>()
                {
                    let version = reg
                        .record(pack_id)
                        .map(|r| r.installed_version)
                        .unwrap_or_default();
                    if let Some(grain_sdk::RevocationState::Revoked) =
                        store.revocation_state(pack_id, &version)
                    {
                        return Err(serde_json::json!({ "revoked": pack_id }).to_string());
                    }
                }
            }
            let pack = load_pack(&app, pack_id)?;
            // [GRAIN] SPEC §6 (the Chrome model): a scripted extension is HELD
            // at first enable until the user approves the capabilities its
            // manifest requests. Never grant implicitly — the whole point is
            // that code cannot start running on capabilities nobody approved.
            // The frontend catches this structured error, shows the permission
            // sheet, calls `extension_grant`, and retries.
            if enabled && pack.has_runtime() {
                let granted = reg.record(pack_id).map(|r| r.granted).unwrap_or_default();
                let missing: Vec<String> = pack
                    .manifest
                    .permissions
                    .iter()
                    .filter(|p| !granted.contains(p))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    return Err(serde_json::json!({ "needsPermissions": missing }).to_string());
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
            reg.set_enabled(pack_id, enabled)
                .map_err(|e| e.to_string())?;
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
            // SPEC §6: a disabled extension keeps no window and no live
            // credential — every surface is destroyed, not merely slept.
            if !enabled {
                crate::extension_host::stop_extension(pack_id, "extension disabled");
                crate::surfaces::extension::destroy(&app, pack_id);
                crate::surfaces::overlay::dismiss(&app, pack_id);
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
    crate::extension_host::load_manifest_result(app, id)
}

#[derive(serde::Serialize, specta::Type)]
pub struct DeveloperExtension {
    pub id: String,
    pub path: String,
}

#[derive(serde::Serialize, specta::Type)]
pub struct ExtensionDeveloperStatus {
    pub enabled: bool,
    pub loaded: Vec<DeveloperExtension>,
}

/// Developer mode is a distinct, explicit product setting. Reporting loaded
/// projects separately keeps the Overview card model focused on effective
/// extensions while still making every local path visible to the author.
#[tauri::command]
#[specta::specta]
pub fn extension_developer_status(app: AppHandle) -> Result<ExtensionDeveloperStatus, String> {
    use grain_core::extensions::ExtensionsRegistry;
    let reg = app
        .try_state::<std::sync::Arc<ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let mut loaded: Vec<DeveloperExtension> = reg
        .dev_records()
        .into_iter()
        .map(|(id, path)| DeveloperExtension {
            id,
            path: path.to_string_lossy().into_owned(),
        })
        .collect();
    loaded.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(ExtensionDeveloperStatus {
        enabled: settings::get_settings(&app).extension_developer_mode,
        loaded,
    })
}

fn stop_extension_runtime(app: &AppHandle, id: &str, reason: &str) {
    use grain_core::extensions as ext;
    crate::extension_host::stop_extension(id, reason);
    crate::surfaces::extension::destroy(app, id);
    crate::surfaces::overlay::dismiss(app, id);
    if let Some(ctx) = app.try_state::<std::sync::Arc<grain_core::AppContext>>() {
        let _ = ctx.update_settings(|state| ext::remove_prompt_pack(state, id));
    }
}

fn restore_enabled_extension(app: &AppHandle, id: &str) -> Result<(), String> {
    use grain_core::extensions as ext;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    if !reg.is_enabled(id) {
        return Ok(());
    }
    let pack = load_pack(app, id)?;
    if let Some(ctx) = app.try_state::<std::sync::Arc<grain_core::AppContext>>() {
        ctx.update_settings(|state| ext::apply_prompt_pack(state, id, &pack.payloads.prompts))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn require_main_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    (window.label() == "main")
        .then_some(())
        .ok_or_else(|| "developer mode can only be managed from Grain settings".to_string())
}

/// Toggle developer mode from the in-app settings surface. Turning it off is
/// also the cleanup boundary: all local projects are unloaded, workers and
/// surfaces die, and any parked installed versions are restored.
#[tauri::command]
#[specta::specta]
pub fn extension_set_developer_mode(
    app: AppHandle,
    window: tauri::WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    use grain_core::extensions::ExtensionsRegistry;
    require_main_window(&window)?;
    let reg = app
        .try_state::<std::sync::Arc<ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    if !enabled {
        let ids: Vec<String> = reg.dev_records().into_iter().map(|(id, _)| id).collect();
        for id in ids {
            stop_extension_runtime(&app, &id, "developer mode disabled");
            reg.unload_dev(&id).map_err(|error| error.to_string())?;
            restore_enabled_extension(&app, &id)?;
        }
    }
    let data_dir = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .ok_or("app context unavailable")?
        .data_dir
        .clone();
    if enabled {
        crate::events_server::enable_dev_control(&data_dir)?;
    } else {
        crate::events_server::disable_dev_control(&data_dir);
    }
    let mut current = settings::get_settings(&app);
    current.extension_developer_mode = enabled;
    settings::write_settings(&app, current);
    crate::refresh_webview_log_streaming(&app);
    crate::extension_host::refresh_index(&app);
    Ok(())
}

fn load_unpacked_project(app: &AppHandle, root: &std::path::Path) -> Result<String, String> {
    use grain_core::extensions as ext;
    if !settings::get_settings(app).extension_developer_mode {
        return Err("Developer mode is disabled".into());
    }
    let loaded = crate::dev_extensions::load_project(root)?;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let id = loaded.pack.manifest.id.clone();
    let prior = reg.record(&id);
    let requested = &loaded.pack.manifest.permissions;
    let granted = prior
        .as_ref()
        .map(|record| {
            record
                .granted
                .iter()
                .filter(|permission| requested.contains(permission))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let record = ext::ExtensionRecord {
        id: id.clone(),
        enabled: false,
        toggle_seq: prior.as_ref().map(|record| record.toggle_seq).unwrap_or(0),
        installed_version: loaded.pack.manifest.version.clone(),
        granted,
        slots: loaded.pack.manifest.slots.clone(),
        variant_slots: Vec::new(),
        dev: None,
        // Load-unpacked is the `dev` rung: never promotable, never verified.
        trust: grain_sdk::Trust::Dev,
    };
    reg.load_dev(record, loaded.root)
        .map_err(|error| error.to_string())?;
    stop_extension_runtime(app, &id, "load-unpacked project replaced");
    crate::extension_host::refresh_index(app);
    Ok(id)
}

/// Human-only load-unpacked entry point. The frontend cannot provide a path:
/// the backend always opens a native folder picker after confirming developer
/// mode, so links, downloads, and extensions cannot trigger a load.
#[tauri::command]
#[specta::specta]
pub async fn extension_load_unpacked(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    require_main_window(&window)?;
    if !settings::get_settings(&app).extension_developer_mode {
        return Err("Developer mode is disabled".into());
    }
    let picker_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        picker_app.dialog().file().blocking_pick_folder()
    })
    .await
    .map_err(|error| error.to_string())?;
    let Some(folder) = picked.and_then(|path| path.into_path().ok()) else {
        return Ok(None);
    };
    load_unpacked_project(&app, &folder).map(Some)
}

#[tauri::command]
#[specta::specta]
pub fn extension_unload_dev(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
) -> Result<(), String> {
    use grain_core::extensions::ExtensionsRegistry;
    require_main_window(&window)?;
    let reg = app
        .try_state::<std::sync::Arc<ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    if reg.dev_path(&id).is_none() {
        return Err(format!("'{id}' is not a load-unpacked extension"));
    }
    stop_extension_runtime(&app, &id, "load-unpacked project unloaded");
    reg.unload_dev(&id).map_err(|error| error.to_string())?;
    restore_enabled_extension(&app, &id)?;
    crate::extension_host::refresh_index(&app);
    Ok(())
}

/// One declared setting, or `None` if the pack doesn't declare that key.
///
/// The lookup the schema's two enforcement points share — the host UI below and
/// `host_api`'s `settings.get/set`, which the extension itself calls. Off the
/// hot path by construction: settings are read when the page opens and written
/// when someone moves a control, never per transcription or per event.
pub(crate) fn setting_decl(
    app: &AppHandle,
    ext_id: &str,
    key: &str,
) -> Option<grain_sdk::SettingDecl> {
    load_pack(app, ext_id)
        .ok()?
        .manifest
        .contributes
        .settings
        .into_iter()
        .find(|d| d.key == key)
}

#[derive(serde::Serialize, specta::Type, Clone)]
pub struct SelectOptionDto {
    pub value: String,
    pub label: String,
}

/// [GRAIN] Phase 5C: the SCHEMA of one field (no value), crossed to the host
/// renderer so it can draw a `list` row's inputs, or an `app_path`/`url` field.
/// Recursive: a list field carries its own `fields` so lists nest.
#[derive(serde::Serialize, specta::Type, Clone)]
pub struct ExtensionSettingField {
    pub key: String,
    pub label: String,
    pub description: String,
    pub kind: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub options: Vec<SelectOptionDto>,
    /// Sub-field schema for a `list` field (empty otherwise).
    pub fields: Vec<ExtensionSettingField>,
    /// Singular noun for a `list`'s Add button / row header.
    pub item_label: Option<String>,
}

/// Flatten a declaration into its renderer schema (no value). Shared by
/// top-level rows and nested list fields.
fn field_schema(decl: &grain_sdk::SettingDecl) -> ExtensionSettingField {
    use grain_sdk::SettingKind as K;
    let (kind, min, max, step, options, fields, item_label) = match &decl.kind {
        K::Bool => ("bool", None, None, None, vec![], vec![], None),
        K::String => ("string", None, None, None, vec![], vec![], None),
        K::Secret => ("secret", None, None, None, vec![], vec![], None),
        K::Shortcut => ("shortcut", None, None, None, vec![], vec![], None),
        K::Color => ("color", None, None, None, vec![], vec![], None),
        K::AppPath => ("app_path", None, None, None, vec![], vec![], None),
        K::Url => ("url", None, None, None, vec![], vec![], None),
        K::Number { min, max } => ("number", *min, *max, None, vec![], vec![], None),
        K::Slider { min, max, step } => {
            ("slider", Some(*min), Some(*max), *step, vec![], vec![], None)
        }
        K::Select { options } => (
            "select",
            None,
            None,
            None,
            options
                .iter()
                .map(|o| SelectOptionDto {
                    value: o.value.clone(),
                    label: o.label.clone(),
                })
                .collect(),
            vec![],
            None,
        ),
        K::List { fields, item_label } => (
            "list",
            None,
            None,
            None,
            vec![],
            fields.iter().map(field_schema).collect(),
            item_label.clone(),
        ),
        K::Unsupported => ("unsupported", None, None, None, vec![], vec![], None),
    };
    ExtensionSettingField {
        key: decl.key.clone(),
        label: decl.label.clone(),
        description: decl.description.clone(),
        kind: kind.to_string(),
        min,
        max,
        step,
        options,
        fields,
        item_label,
    }
}

/// One row of an extension's settings section: the declaration flattened into
/// exactly what a control needs, plus the value to show.
///
/// Deliberately NOT the manifest type: `SettingKind` is an internally-tagged
/// enum with per-variant fields, which crosses the bindings boundary as an
/// awkward union. The renderer wants `kind` plus optional extras, so that is
/// what it gets.
#[derive(serde::Serialize, specta::Type)]
pub struct ExtensionSettingRow {
    pub key: String,
    pub label: String,
    pub description: String,
    /// `bool | string | secret | number | select | shortcut | color | slider`.
    pub kind: String,
    /// Where the section renders (SPEC §4.3). An anchor this build doesn't know
    /// is passed through untouched — the frontend falls back to the extension's
    /// own section, because settings are never lost.
    pub anchor: Option<String>,
    pub order: i32,
    /// The resolved current value: bool, number, or string per `kind`.
    pub value: serde_json::Value,
    /// Set when a stored value had to be reset — a change the user did not make
    /// must never be silent.
    pub notice: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub options: Vec<SelectOptionDto>,
    /// Sub-field schema for a `list` row (empty otherwise).
    pub fields: Vec<ExtensionSettingField>,
    /// Singular noun for a `list`'s Add button / row header.
    pub item_label: Option<String>,
}

fn setting_row(
    decl: grain_sdk::SettingDecl,
    value: serde_json::Value,
    notice: Option<String>,
) -> ExtensionSettingRow {
    let schema = field_schema(&decl);
    ExtensionSettingRow {
        key: schema.key,
        label: schema.label,
        description: schema.description,
        kind: schema.kind,
        anchor: decl.anchor,
        order: decl.order,
        value,
        notice,
        min: schema.min,
        max: schema.max,
        step: schema.step,
        options: schema.options,
        fields: schema.fields,
        item_label: schema.item_label,
    }
}

/// The settings an extension declares, resolved against what is stored
/// (SPEC §4.1, levels 1–2). Ordered by `order`, ties on declaration order, so
/// the host renders straight down the list.
///
/// Controls this build doesn't understand are dropped rather than drawn blank;
/// their stored values stay untouched for a build that does understand them.
#[tauri::command]
#[specta::specta]
pub fn extension_settings_schema(
    app: AppHandle,
    id: String,
) -> Result<Vec<ExtensionSettingRow>, String> {
    let pack = load_pack(&app, &id)?;
    let ctx = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .ok_or("app context unavailable")?;
    Ok(rows_for(
        pack.manifest.contributes.settings,
        &crate::host_api::ExtStorage::new(&ctx.data_dir, &id),
        &ctx,
        &id,
    ))
}

/// Resolve a declaration list against what is stored. Split out so
/// [`extension_settings_sections`] reads each pack once rather than twice.
fn rows_for(
    decls: Vec<grain_sdk::SettingDecl>,
    store: &crate::host_api::ExtStorage,
    ctx: &grain_core::AppContext,
    ext_id: &str,
) -> Vec<ExtensionSettingRow> {
    let mut rows: Vec<ExtensionSettingRow> = decls
        .into_iter()
        .filter(|d| !matches!(d.kind, grain_sdk::SettingKind::Unsupported))
        .map(|decl| {
            let stored = if matches!(decl.kind, grain_sdk::SettingKind::Secret) {
                let marker = if ctx
                    .extension_secret(&crate::host_api::extension_secret_key(ext_id, &decl.key))
                    .is_some()
                {
                    crate::host_api::SECRET_REDACTED
                } else {
                    ""
                };
                serde_json::Value::String(marker.to_string())
            } else {
                store.settings_get(&decl.key).unwrap_or_else(|error| {
                    log::warn!("[GRAIN] extension settings storage read failed: {error}");
                    serde_json::Value::Null
                })
            };
            let resolved = grain_sdk::settings_schema::resolve(&decl, Some(&stored));
            setting_row(decl, resolved.value, resolved.notice)
        })
        .collect();
    rows.sort_by_key(|r| r.order);
    rows
}

/// The live state of one extension's contributed shortcuts (SPEC §3.3), so the
/// settings section can show a chord that is registered — and name the holder
/// of one that isn't, rather than leaving a dead hotkey unexplained.
#[tauri::command]
#[specta::specta]
pub fn extension_shortcuts_status(id: String) -> Vec<crate::extension_shortcuts::ShortcutStatus> {
    crate::extension_shortcuts::status_for(&id)
}

/// One enabled extension's settings, ready to render.
#[derive(serde::Serialize, specta::Type)]
pub struct ExtensionSettingsSection {
    pub id: String,
    pub name: String,
    pub rows: Vec<ExtensionSettingRow>,
}

/// Every **enabled** extension's declared settings, in toggle order.
///
/// One pass over the packs answers all five anchors, so opening a settings tab
/// costs one read rather than one per anchor. Disabled extensions are absent
/// entirely (SPEC §6: disable makes anchored sections disappear) — their values
/// are retained on disk, just not rendered.
#[tauri::command]
#[specta::specta]
pub fn extension_settings_sections(
    app: AppHandle,
) -> Result<Vec<ExtensionSettingsSection>, String> {
    use grain_core::extensions as ext;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;

    let ctx = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .ok_or("app context unavailable")?;

    let mut enabled: Vec<ext::ExtensionRecord> =
        reg.records().into_iter().filter(|r| r.enabled).collect();
    enabled.sort_by_key(|r| r.toggle_seq);

    let mut out = Vec::new();
    for rec in enabled {
        let Ok(pack) = load_pack(&app, &rec.id) else {
            // A broken pack file must not take the settings page down (SPEC §6).
            continue;
        };
        if pack.manifest.contributes.settings.is_empty() {
            continue;
        }
        let store = crate::host_api::ExtStorage::new(&ctx.data_dir, &rec.id);
        let rows = rows_for(pack.manifest.contributes.settings, &store, &ctx, &rec.id);
        out.push(ExtensionSettingsSection {
            id: rec.id,
            name: pack.manifest.name,
            rows,
        });
    }
    Ok(out)
}

/// Write one schema-declared setting from the host's own control.
///
/// Validated against the same schema as `host_api`'s `settings.set`, and
/// returns the row actually stored — a clamped number or a normalised colour
/// comes straight back, so the control shows the truth rather than what was
/// typed.
#[tauri::command]
#[specta::specta]
pub fn extension_setting_set(
    app: AppHandle,
    id: String,
    key: String,
    value: serde_json::Value,
) -> Result<ExtensionSettingRow, String> {
    let decl = setting_decl(&app, &id, &key)
        .ok_or_else(|| format!("'{key}' is not a declared setting of '{id}'"))?;
    let accepted = grain_sdk::settings_schema::coerce(&decl, &value)?;
    let ctx = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .ok_or("app context unavailable")?;
    if matches!(decl.kind, grain_sdk::SettingKind::Secret) {
        let secret = accepted.value.as_str().ok_or("secret value must be text")?;
        ctx.set_extension_secret(
            crate::host_api::extension_secret_key(&id, &key),
            secret.to_string(),
        )
        .map_err(|error| error.to_string())?;
        let marker = if secret.is_empty() {
            ""
        } else {
            crate::host_api::SECRET_REDACTED
        };
        return Ok(setting_row(
            decl,
            serde_json::Value::String(marker.to_string()),
            accepted.notice,
        ));
    }
    crate::host_api::ExtStorage::new(&ctx.data_dir, &id)
        .settings_set(&key, accepted.value.clone())
        .map_err(|error| error.to_string())?;
    Ok(setting_row(decl, accepted.value, accepted.notice))
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
    let dev_active = reg.dev_path(&id).is_some();
    let prior = reg.installed_record(&id);
    let was_enabled = prior.as_ref().map(|r| r.enabled).unwrap_or(false);
    reg.install(ext::ExtensionRecord {
        id: id.clone(),
        enabled: was_enabled,
        toggle_seq: prior.as_ref().map(|r| r.toggle_seq).unwrap_or(0),
        installed_version: pack.manifest.version.clone(),
        granted: prior
            .as_ref()
            .map(|r| r.granted.clone())
            .unwrap_or_default(),
        slots: pack.manifest.slots.clone(),
        // Phase 5C: variant slots (SPEC §10.2) are declared by the manifest now
        // that they are externalised — the Agent centre layout ships as a real
        // pack rather than a host-synthesised record.
        variant_slots: pack.manifest.variant_slots.clone(),
        dev: None,
        // A manually imported local file is UNTRUSTED, always — even if a
        // store-verified record for this id existed. Trust comes only from the
        // signed index (DISTRIBUTION-PLAN §3.2); inheriting it here would let a
        // local pack impersonate a verified one.
        trust: grain_sdk::Trust::UNTRUSTED_DEFAULT,
    })
    .map_err(|e| e.to_string())?;
    // An enabled pack's payloads refresh in place (apply is idempotent).
    if was_enabled && !dev_active {
        if let Some(ctx) = app.try_state::<std::sync::Arc<grain_core::AppContext>>() {
            ctx.update_settings(|s| ext::apply_prompt_pack(s, &id, &pack.payloads.prompts))
                .map_err(|e| e.to_string())?;
        }
    }
    crate::extension_host::refresh_index(&app);
    Ok(id)
}

/// [GRAIN] Phase 5C: the `app_path` settings control's native picker. Opens the
/// OS file chooser and, on a pick, records the path as **approved for this
/// extension** (the same user-mediated approval `open:app` requires) so a rule
/// the user builds here can actually launch. Returns the chosen path or `None`.
#[tauri::command]
#[specta::specta]
pub async fn extension_pick_app(app: AppHandle, id: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picker = app.clone();
    let picked =
        tauri::async_runtime::spawn_blocking(move || picker.dialog().file().blocking_pick_file())
            .await
            .map_err(|e| e.to_string())?;
    let path = picked
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().to_string());
    if let Some(ref p) = path {
        let ctx = app
            .try_state::<std::sync::Arc<grain_core::AppContext>>()
            .ok_or("app context unavailable")?;
        crate::host_api::approve_app(&ctx.data_dir, &id, p).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

/// [GRAIN] Phase 5C: capture the FOREGROUND app for an `app_path` control (the
/// user switches to their target app during the control's countdown, then this
/// snapshots it). Records the path as approved for this extension's `open:app`,
/// exactly like the file picker. Returns the executable path, or `None`.
#[tauri::command]
#[specta::specta]
pub fn extension_capture_app(app: AppHandle, id: String) -> Result<Option<String>, String> {
    let Some(detected) = detect_active_app() else {
        return Ok(None);
    };
    // `exe_path` is empty when the path couldn't be resolved — nothing to record.
    let path = detected.exe_path;
    if path.is_empty() {
        return Ok(None);
    }
    let ctx = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .ok_or("app context unavailable")?;
    crate::host_api::approve_app(&ctx.data_dir, &id, &path).map_err(|e| e.to_string())?;
    Ok(Some(path))
}

/// Record the user's approval of a scripted extension's capabilities (SPEC §6).
/// Called by the permission sheet on Approve; the caller then retries enable.
///
/// Grants are clamped to what the manifest actually requests, so neither a
/// compromised frontend nor a stale sheet can widen an extension's reach beyond
/// what the user was shown.
#[tauri::command]
#[specta::specta]
pub fn extension_grant(app: AppHandle, id: String, permissions: Vec<String>) -> Result<(), String> {
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
        stop_extension_runtime(&app, prev, "extension lost an exclusive slot");
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
    // The three settings-flag-backed built-ins have no record to remove — they
    // are disabled, never uninstalled. Everything else, including the now
    // externalised Agent centre layout, is a real installed pack with a store
    // reinstall source, so it uninstalls normally (Phase 5C).
    if id == ext::BUILTIN_SNIPPETS
        || id == ext::BUILTIN_CONTEXT
        || id == ext::BUILTIN_AGENT
        || id == ext::BUILTIN_ACTIONS
    {
        return Err("built-in features can be disabled, not uninstalled".into());
    }
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let dev_active = reg.dev_path(&id).is_some();
    let removed = reg.uninstall(&id).map_err(|e| e.to_string())?;
    if !removed {
        return Err(format!("'{id}' is not installed"));
    }
    if dev_active {
        if purge {
            let _ = std::fs::remove_file(pack_path(&app, &id)?);
        }
        return Ok(());
    }
    if let Some(ctx) = app.try_state::<std::sync::Arc<grain_core::AppContext>>() {
        let _ = ctx.update_settings(|s| ext::remove_prompt_pack(s, &id));
        crate::host_api::ExtStorage::new(&ctx.data_dir, &id)
            .purge()
            .map_err(|error| error.to_string())?;
        ctx.purge_extension_secrets(&id)
            .map_err(|error| error.to_string())?;
        // [GRAIN] Phase 5C: forget any user-approved launchable app paths, so a
        // reinstalled extension starts with no launch approvals (SPEC §6).
        let _ = std::fs::remove_file(
            ctx.data_dir
                .join("extensions")
                .join(format!("{id}.approved-apps.json")),
        );
    }
    // Disable keeps a rebind; uninstall is the transaction that clears it
    // (SPEC §6: shortcuts unregistered, slots released, storage wiped).
    crate::extension_host::stop_extension(&id, "extension uninstalled");
    crate::extension_shortcuts::forget(&app, &id);
    crate::surfaces::extension::destroy(&app, &id);
    crate::surfaces::overlay::dismiss(&app, &id);
    if purge {
        let _ = std::fs::remove_file(pack_path(&app, &id)?);
    }
    crate::extension_host::refresh_index(&app);
    Ok(())
}

// ── Extension workspace surfaces (SPEC §1.2, §7.1) ────────────────────────────
//
// These three are called by `extension-surface.html` — Grain's wrapper page —
// and never by extension code, which sits in a sandboxed iframe with no Tauri
// IPC. Every one of them derives WHICH extension is calling from the calling
// window's own label, so there is no argument to point at somebody else's
// surface.

/// The wrapper page collecting its identity and the markup to render. Handed
/// over once per open; a second asker gets nothing rather than a live token.
#[tauri::command]
#[specta::specta]
pub fn extension_surface_init(
    window: tauri::WebviewWindow,
) -> Option<crate::surfaces::extension::SurfaceInit> {
    crate::surfaces::extension::take_init(window.label())
}

/// Frontend ack: the surface UI is mounted — reveal the window.
#[tauri::command]
#[specta::specta]
pub fn extension_surface_ui_ready(app: AppHandle, window: tauri::WebviewWindow) {
    if let Some(id) = crate::surfaces::extension::id_for_label(window.label()) {
        crate::surfaces::workspace::ui_ready(&app, &id);
    }
}

/// Frontend ack: the surface UI is unmounted — hide and suspend now.
#[tauri::command]
#[specta::specta]
pub fn extension_surface_sleep_ready(app: AppHandle, window: tauri::WebviewWindow) {
    if let Some(id) = crate::surfaces::extension::id_for_label(window.label()) {
        crate::surfaces::workspace::sleep_ready(&app, &id);
    }
}

/// The wrapper page collecting the payload its surface was opened with, to hand
/// to the iframe on mount. Keyed on the calling window, so a surface only ever
/// receives its own — and consumed once, so a re-mount does not replay a stale
/// one.
#[tauri::command]
#[specta::specta]
pub fn extension_surface_payload(window: tauri::WebviewWindow) -> Option<serde_json::Value> {
    crate::surfaces::extension::take_payload(window.label())
}
