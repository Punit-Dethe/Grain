//! [GRAIN] Grain-only Tauri settings commands, out of the Handy-derived
//! `shortcut/mod.rs` (Handy Isolation phase 6). Upstream owns the shortcut
//! registration/dispatch machinery in that module; these are the setting
//! mutators for Grain's own features — context awareness,
//! "scrap that", snippets, voice actions, app modes, the Agent, Grain Space,
//! rolling preview, audio conditioning.
//!
//! Each is still a `#[tauri::command]`, so the command NAME (and therefore the
//! frontend `invoke` + generated bindings) is unchanged by the move; only the
//! path in `lib.rs`'s `collect_commands!` differs.

use crate::settings;
use crate::settings::{DefaultPanel, PillSkin};
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

/// [GRAIN] One context-awareness profile, as the settings UI needs it.
///
/// Carries BOTH texts on purpose. The UI has to be able to show the effective
/// instruction, tell whether it has been edited, and put the shipped wording
/// back — and deriving "edited" from a copy of the defaults kept in TypeScript
/// is how the two drift apart. Rust ships the text; the frontend owns only the
/// label and the icon.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, specta::Type)]
pub struct ContextProfileInfo {
    /// `email` / `work` / `casual` / `technical`.
    pub id: String,
    /// What is sent to the model today: the user's edit, or the default.
    pub instruction: String,
    /// The shipped wording, for "reset to default".
    pub default_instruction: String,
    /// Whether the user has edited this profile. An override trimmed to empty
    /// counts as edited — "this profile says nothing" is a deliberate choice,
    /// not an absent one.
    pub edited: bool,
    /// A few real hosts this profile covers, for the card's icon stack. Derived
    /// from the site table, so the card cannot claim a site the profile does
    /// not actually apply to.
    pub sample_sites: Vec<String>,
}

/// How many icons the card stacks. Beyond three the overlap stops reading as
/// distinct icons and starts reading as a smudge.
const SAMPLE_SITES: usize = 3;

/// [GRAIN] The four editable profiles, in display order.
#[tauri::command]
#[specta::specta]
pub fn context_profiles(app: AppHandle) -> Vec<ContextProfileInfo> {
    let settings = settings::get_settings(&app);
    crate::context_detect::PROFILE_IDS
        .iter()
        .filter_map(|id| {
            let category = crate::context_detect::AppCategory::from_profile_id(id)?;
            let default_instruction = category.default_instruction().unwrap_or_default();
            Some(ContextProfileInfo {
                id: (*id).to_string(),
                instruction: category
                    .instruction(&settings)
                    .unwrap_or_default()
                    .to_string(),
                default_instruction: default_instruction.to_string(),
                edited: settings
                    .context_profile_instructions
                    .iter()
                    .any(|o| o.id == *id),
                sample_sites: crate::context_detect::sample_sites(category, SAMPLE_SITES),
            })
        })
        .collect()
}

/// [GRAIN] Set (or clear) one profile's instruction.
///
/// Passing text equal to the default CLEARS the override rather than storing a
/// copy of it. That is what keeps an untouched-in-effect profile tracking the
/// shipped wording as it improves, instead of being pinned by a user who opened
/// the editor, changed their mind, and typed it back.
#[tauri::command]
#[specta::specta]
pub fn set_context_profile_instruction(
    app: AppHandle,
    id: String,
    instruction: String,
) -> Result<(), String> {
    let Some(category) = crate::context_detect::AppCategory::from_profile_id(&id) else {
        return Err(format!("unknown context profile '{id}'"));
    };
    let mut settings = settings::get_settings(&app);
    settings.context_profile_instructions.retain(|o| o.id != id);
    if instruction.trim() != category.default_instruction().unwrap_or_default().trim() {
        settings
            .context_profile_instructions
            .push(settings::ContextProfileInstruction { id, instruction });
    }
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] A supported site's favicon as a PNG data URL, or `None`.
///
/// Async and one host per call, so the settings UI paints immediately and each
/// icon appears as it resolves. A batch command would make the whole row wait
/// for the slowest site — and these are cached after the first fetch, so the
/// call is nearly free on every subsequent open.
#[tauri::command]
#[specta::specta]
pub async fn site_icon(app: AppHandle, host: String) -> Option<String> {
    crate::pill_icon::site_icon_data_url(&app, &host).await
}

/// [GRAIN] Every application this user can launch, for the profile app picker.
///
/// Async and off the runtime's threads: this walks a Shell namespace and reads
/// two properties per entry, which on a well-populated machine is a couple of
/// hundred milliseconds. The picker asks once when it opens and filters the
/// result locally, so a person typing never waits on this.
#[tauri::command]
#[specta::specta]
pub async fn installed_apps() -> Vec<crate::context_detect::app_catalog::InstalledApp> {
    tauri::async_runtime::spawn_blocking(crate::context_detect::app_catalog::installed_apps)
        .await
        .unwrap_or_default()
}

/// [GRAIN] An installed application's icon as a PNG data URL, or `None`.
///
/// One app per call for the same reason [`site_icon`] is: the list paints at
/// once and each icon lands as it resolves, rather than the whole picker waiting
/// on the slowest entry.
#[tauri::command]
#[specta::specta]
pub async fn app_icon(app: AppHandle, id: String) -> Option<String> {
    crate::pill_icon::app_icon_data_url(&app, id).await
}

/// [GRAIN] The user's own context profiles, as stored.
///
/// Read back through a command rather than off the settings blob so the UI sees
/// the NORMALISED targets — a pasted URL is stored as a bare host, and an editor
/// showing what was typed instead of what was saved is how a user ends up
/// wondering why their profile never fires.
#[tauri::command]
#[specta::specta]
pub fn context_custom_profiles(app: AppHandle) -> Vec<settings::CustomContextProfile> {
    settings::get_settings(&app).context_custom_profiles
}

/// [GRAIN] Replace the whole set of user-made context profiles.
///
/// Whole-set rather than per-profile because the UI edits a list and the list is
/// small; a partial update API would only add ordering questions. Targets are
/// normalised here rather than trusted, since precedence and icon eligibility
/// both key off them: an application is matched against a lowercased exe stem,
/// and a website against a bare host, so a user who types `Figma.exe` or
/// `https://figma.com/files` gets what they meant.
#[tauri::command]
#[specta::specta]
pub fn update_context_custom_profiles(
    app: AppHandle,
    profiles: Vec<settings::CustomContextProfile>,
) -> Result<(), String> {
    let profiles: Vec<settings::CustomContextProfile> = profiles
        .into_iter()
        .filter(|p| !p.title.trim().is_empty())
        .map(|mut p| {
            p.title = p.title.trim().to_string();
            p.instruction = p.instruction.trim().to_string();
            p.targets = p
                .targets
                .into_iter()
                .filter_map(|mut t| {
                    t.value = match t.kind.as_str() {
                        "application" => normalise_exe(&t.value),
                        "website" => normalise_host(&t.value),
                        _ => return None, // a kind we cannot match is not a target
                    };
                    (!t.value.is_empty()).then_some(t)
                })
                .collect();
            p
        })
        .collect();
    let mut settings = settings::get_settings(&app);
    settings.context_custom_profiles = profiles;
    settings::write_settings(&app, settings);
    Ok(())
}

/// `C:\Program Files\Figma\Figma.exe` / `Figma.exe` / `figma` → `figma`.
///
/// A packaged app's AppUserModelID is passed through untouched, because it is
/// already the identity the matcher wants and stem extraction would destroy it:
/// `Microsoft.WindowsNotepad_8wekyb3d8bbwe!App` has dots in it, so `file_stem`
/// would read the tail as an extension and hand back `Microsoft`. The `!` is
/// what tells them apart — it separates package family from application id, and
/// no executable path contains one.
fn normalise_exe(raw: &str) -> String {
    let raw = raw.trim().trim_matches('"');
    if raw.contains('!') {
        return raw.to_string();
    }
    std::path::Path::new(raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(raw)
        .to_ascii_lowercase()
}

/// `https://www.Figma.com/files?x=1` → `figma.com`.
///
/// Hand-parsed rather than via a URL crate because the input is as likely to be
/// a bare host someone typed as it is a pasted URL, and a parser strict enough
/// to reject `figma.com` would be worse than useless here.
fn normalise_host(raw: &str) -> String {
    let raw = raw.trim().trim_matches('"');
    let after_scheme = raw.split_once("://").map(|(_, rest)| rest).unwrap_or(raw);
    let host = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@') // strip any user:pass@
        .next()
        .unwrap_or("");
    // Drop a port, then `www.`, which the matcher also strips from live hosts.
    let host = host.split(':').next().unwrap_or("");
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    // Must look like a domain, or it can never match a real address bar.
    if host.contains('.') && !host.starts_with('.') && !host.ends_with('.') {
        host
    } else {
        String::new()
    }
}

#[cfg(test)]
mod custom_profile_target_tests {
    use super::{normalise_exe, normalise_host};

    /// Whatever the user pastes has to end up as the lowercased stem the
    /// detector compares against, or the profile silently never matches.
    #[test]
    fn an_application_target_becomes_an_exe_stem() {
        assert_eq!(normalise_exe(r"C:\Program Files\Figma\Figma.exe"), "figma");
        assert_eq!(normalise_exe("Figma.exe"), "figma");
        assert_eq!(normalise_exe("  figma  "), "figma");
        assert_eq!(normalise_exe("\"Code.exe\""), "code");
    }

    /// A packaged app is named by its AppUserModelID, and stem extraction would
    /// silently mangle it — `Microsoft.WindowsNotepad_…!App` would be stored as
    /// `Microsoft`, matching nothing and colliding with everything.
    #[test]
    fn a_packaged_application_target_keeps_its_appusermodelid() {
        for aumid in [
            "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App",
            "Claude_pzs8sxrjxfjjc!Claude",
            "TelegramMessengerLLP.TelegramDesktop_t4vj0pshhgkwm!Telegram.TelegramDesktop.Store",
        ] {
            assert_eq!(normalise_exe(aumid), aumid);
            assert_eq!(normalise_exe(&format!("  {aumid}  ")), aumid);
        }
    }

    /// Same for a website: a pasted URL and a typed host must reach the same
    /// bare host, since that is what an address bar yields.
    #[test]
    fn a_website_target_becomes_a_bare_host() {
        assert_eq!(
            normalise_host("https://www.Figma.com/files?x=1"),
            "figma.com"
        );
        assert_eq!(normalise_host("figma.com"), "figma.com");
        assert_eq!(normalise_host("http://localhost:3000/app"), "");
        assert_eq!(
            normalise_host("https://user:pw@app.figma.com/"),
            "app.figma.com"
        );
        assert_eq!(normalise_host("figma.com:8443"), "figma.com");
    }

    /// Anything that could never match a real address bar is dropped rather
    /// than stored, so a profile never carries a target that does nothing.
    #[test]
    fn a_target_that_can_never_match_is_rejected() {
        for junk in ["", "   ", "not a host", "com", ".com", "figma."] {
            assert_eq!(normalise_host(junk), "", "{junk:?} should not be a host");
        }
    }
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

/// [GRAIN] Which mode the AI shortcut starts from idle. Runtime-only — every
/// capture mode is always registered, so no key is added or dropped by this.
#[tauri::command]
#[specta::specta]
pub fn change_capture_ai_start_mode_setting(app: AppHandle, mode: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.capture_ai_start_mode = mode;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Whether the AI shortcut, pressed during a capture, ends it and routes
/// the transcript to AI.
#[tauri::command]
#[specta::specta]
pub fn change_capture_end_with_ai_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.capture_end_with_ai = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Send every capture to AI, whichever shortcut started it.
#[tauri::command]
#[specta::specta]
pub fn change_capture_always_ai_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.capture_always_ai = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Toggle context awareness (post-processing SOFT context).
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

/// [GRAIN] Toggle Paste Catch, the missed-text-field clipboard safety net.
/// Turning it off also releases any active hold so the clipboard, temporary
/// delivery shortcut, and native pill notice do not outlive the preference.
#[tauri::command]
#[specta::specta]
pub fn change_paste_catch_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.paste_catch_enabled = enabled;
    settings::write_settings(&app, settings);
    if !enabled {
        crate::paste_catch::supersede(&app);
        crate::bridge::emit(&app, grain_sdk::DaemonEvent::PasteCatchDisabled);
    }
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

/// [GRAIN] Toggle seamless insertion — reading a short span either side of the
/// caret so dictated text flows into what surrounds it. Its own switch rather
/// than part of nearby terms, because it sends a raw excerpt where that one
/// promises only unique tokens. Only effective when context awareness is on.
#[tauri::command]
#[specta::specta]
pub fn change_context_caret_text_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.context_caret_text = enabled;
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

/// [GRAIN] Agent screen vision: send a picture of the summoned-from window with
/// the instruction. OFF by default; see `Settings::agent_screen_image`.
#[tauri::command]
#[specta::specta]
pub fn change_agent_screen_image_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.agent_screen_image = enabled;
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

/// [GRAIN] Detect the foreground app right now. Returns `None` when nothing can be
/// resolved (unsupported platform, no foreground window). Silent — no UI.
#[tauri::command]
#[specta::specta]
pub fn detect_active_app() -> Option<DetectedApp> {
    // The capture button only needs the app/URL, not focused-field terms.
    crate::context_detect::detect_active_context(false, false).map(|c| DetectedApp {
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

/// [GRAIN] Which built-in look the collapsed pill wears. Unlike a pill *theme*
/// (an extension's colours), a skin changes the pill's geometry — so the pill
/// resizes its own window on receipt. An unknown name resolves to the default
/// rather than erroring: the user must never end up with no pill.
#[tauri::command]
#[specta::specta]
pub fn change_pill_skin_setting(app: AppHandle, skin: String) -> Result<(), String> {
    let parsed = PillSkin::from_wire(&skin);
    if parsed.as_wire() != skin {
        warn!(
            "Invalid pill skin '{}', defaulting to {}",
            skin,
            parsed.as_wire()
        );
    }
    let mut settings = settings::get_settings(&app);
    settings.pill_skin = parsed;
    settings::write_settings(&app, settings);

    // Drive the live pill: it re-sizes and re-centers on the next frame. An idle
    // pill picks the skin up from its welcome frame on the next connect.
    crate::pill_skin::broadcast(&app, parsed);
    Ok(())
}

/// [GRAIN] Pill identity: show the icon of the app being dictated into in place
/// of the pill's state dot. Takes effect on the next session — the icon is
/// resolved at record-start, never held between sessions.
#[tauri::command]
#[specta::specta]
pub fn change_pill_show_app_icon_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.pill_show_app_icon = enabled;
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
    settings::write_settings(&app, settings);
    crate::grain_space::apply_enabled(&app, enabled);
    Ok(())
}
/// [GRAIN] Where the Grain store keeps its notes. Empty restores the default.
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_store_path_setting(app: AppHandle, path: String) -> Result<(), String> {
    let trimmed = path.trim().to_string();
    let mut settings = settings::get_settings(&app);
    if settings.grain_space_store_path == trimmed {
        return Ok(());
    }
    settings.grain_space_store_path = trimmed;
    settings::write_settings(&app, settings);
    // A different notes folder is a different corpus.
    crate::grain_space::emit_corpus_changed(&app);
    crate::grain_space::embed::shutdown_engine();
    crate::grain_space::reminders::sync(&app);
    Ok(())
}

/// [GRAIN] Where `grain-mcp` is on this machine, for the config snippet the
/// Grain Space tab shows.
///
/// Resolved rather than assumed: the proxy sits beside the app binary in an
/// install and beside it in the cargo target dir in development, and an MCP
/// client is given an absolute path — it does not search a PATH we control.
/// Falls back to the bare name so the snippet is still copyable (and the
/// mistake obvious) if the binary has not been built yet.
#[tauri::command]
#[specta::specta]
pub fn grain_space_mcp_path() -> String {
    let name = if cfg!(windows) {
        "grain-mcp.exe"
    } else {
        "grain-mcp"
    };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .filter(|path| path.exists())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| name.to_string())
}

/// [GRAIN] The Grain Space MCP bridge. Switching it ON mints the proxy's token
/// and writes it where `grain-mcp` looks; switching it OFF revokes the token and
/// deletes the file, so a client that is already connected stops being able to
/// reconnect and a client that starts later finds nothing to authenticate with.
#[tauri::command]
#[specta::specta]
pub fn change_grain_space_mcp_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.grain_space_mcp = enabled;
    settings::write_settings(&app, settings);
    crate::grain_space::apply_mcp(&app, enabled);
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
    // The corpus changes wholesale — tell the Notes tab to drop everything it is
    // showing and re-list, and drop the embedding engine so nothing keeps serving
    // the old backend's vectors.
    crate::grain_space::emit_corpus_changed(&app);
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
    // Different vault ⇒ different corpus: re-list from scratch, drop the vectors.
    crate::grain_space::emit_corpus_changed(&app);
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
    if settings.grain_space_vault_folder == trimmed {
        return Ok(());
    }
    settings.grain_space_vault_folder = trimmed;
    settings::write_settings(&app, settings);
    // Which subfolder is "Grain's" decides which of the vault's notes are ours
    // and which are foreign, so this changes the corpus as surely as swapping the
    // vault does.
    crate::grain_space::emit_corpus_changed(&app);
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
    /// "pack" | "scripted" | "native"
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
    /// Manifest-declared capabilities. The installed UI needs these even for
    /// local/imported packs which have no signed-store catalogue entry.
    pub capabilities: Vec<String>,
    /// The pack declares settings or shortcuts, so it has a section of its own
    /// worth opening. Free to compute — Overview already reads every manifest.
    pub has_detail: bool,
    /// [GRAIN] Host surfaces this extension takes over, e.g.
    /// `agent.reply-surface`. An "in-place" extension has no settings page of
    /// its own — it changes a control that already exists somewhere in the app
    /// — so this is what lets the UI open the place it actually affects
    /// instead of dead-ending on a preview.
    pub slots: Vec<String>,
    /// [GRAIN] Prompt layers this pack contributes.
    ///
    /// Carried on the card because **attribution is not optional for this
    /// contribution**: a prompt layer changes what the model does to the user's
    /// own words, and unlike a capability it is invisible once approved. The
    /// approval sheet is where the user first reads it; this is where they can
    /// go back and read it again without uninstalling anything.
    pub prompt_layers: Vec<PromptLayerInfo>,
}

/// [GRAIN] One contributed prompt layer, as every surface that shows one needs
/// it: the approval sheet, the extension card, and the prompt-stack view.
///
/// One type for all three deliberately. The sheet's copy and the card's copy
/// drifting apart would mean the user approved one wording and can later only
/// review another.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, specta::Type)]
pub struct PromptLayerInfo {
    pub id: String,
    /// The instruction, verbatim. Never summarised anywhere it is displayed.
    pub text: String,
    /// No conditions at all — it applies to every dictation.
    pub everywhere: bool,
    pub app: Vec<String>,
    pub website: Vec<String>,
    pub category: Vec<String>,
}

impl PromptLayerInfo {
    fn from_decl(decl: &grain_sdk::manifest::PromptLayerDecl) -> Self {
        Self {
            id: decl.id.clone(),
            text: decl.text.clone(),
            everywhere: decl.when.is_unconditional(),
            app: decl.when.app.clone(),
            website: decl.when.website.clone(),
            category: decl.when.category.clone(),
        }
    }
}

/// [GRAIN] Start or stop listening for an action
/// (`docs/Action Routing/PLAN.md` §3).
///
/// The extension surface's own trigger calls this; **Grain registers no
/// shortcut for it here**. What the design depends on is only that the user's
/// intent was unambiguous by the time audio started, and the trigger mechanism
/// is decided separately.
///
/// One command for all three transitions rather than three, because the
/// invoke-handler list lives in the Handy-derived `lib.rs` and every entry is a
/// merge-conflict surface.
#[tauri::command]
#[specta::specta]
pub fn grain_action_listen(app: AppHandle, phase: String) -> Result<bool, String> {
    use crate::grain_actions::action_session;
    match phase.as_str() {
        "start" => match action_session::start(&app) {
            Ok(()) => Ok(true),
            // Not an error the user needs shown: something else already owns the
            // microphone, or nothing is installed that could answer.
            Err(action_session::StartError::Busy)
            | Err(action_session::StartError::NothingInstalled) => Ok(false),
            Err(action_session::StartError::Unavailable(why)) => Err(why),
        },
        "stop" => {
            action_session::stop(&app);
            Ok(true)
        }
        "cancel" => Ok(action_session::cancel(&app)),
        other => Err(format!("unknown action phase '{other}'")),
    }
}

/// [GRAIN] Read the action log, optionally clearing it first
/// (`docs/Action Routing/PLAN.md` §8.3).
///
/// The "why did that happen" surface. It holds what was heard, so it is capped,
/// lives only in memory, and clearing it is one call with no confirmation
/// dance — this is the user's own speech and asking twice before letting them
/// delete it is the wrong default.
#[tauri::command]
#[specta::specta]
pub fn grain_action_log(
    clear: bool,
) -> Result<Vec<crate::grain_actions::action_log::ActionLogEntry>, String> {
    if clear {
        crate::grain_actions::action_log::clear();
        return Ok(Vec::new());
    }
    Ok(crate::grain_actions::action_log::entries())
}

/// [GRAIN] One declared action, as the approval sheet and the extension card
/// need it (`docs/Action Routing/PLAN.md` §5).
///
/// Note what is **absent**: the utterance list. The consent question is "what
/// can this do", not "what words does it listen for" — a list of phrasings is
/// review and `doctor` material, and putting it on a sheet trains people to
/// scroll past the part that matters. What the user decides on is the title, the
/// domain, and whether it will ask before acting.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, specta::Type)]
pub struct ActionInfo {
    pub id: String,
    /// One plain line, written for the user.
    pub title: String,
    /// The preference group — "media", "messaging" — used as the sheet's
    /// heading and as the key for "always use this one".
    pub domain: String,
    /// Whether performing this reads the resolved action back first. The single
    /// most important thing on the row.
    pub confirms: bool,
    /// No conditions at all — offered on every request.
    pub everywhere: bool,
    pub app: Vec<String>,
    pub website: Vec<String>,
}

impl ActionInfo {
    fn from_decl(decl: &grain_sdk::manifest::ActionDecl) -> Self {
        Self {
            id: decl.id.clone(),
            title: decl.title.clone(),
            domain: decl.domain.clone(),
            confirms: !decl.risk.is_safe(),
            everywhere: decl.when.is_unconditional(),
            app: decl.when.app.clone(),
            website: decl.when.website.clone(),
        }
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
    // [GRAIN] Grain's three always-present features (Snippets, Context
    // Awareness, Agent) are NOT listed here. They ship with the app, have a tab
    // each, and their master switch is that tab's header — listing them beside
    // installed packs implied they could be uninstalled and buried the list of
    // things the user actually chose to install.
    //
    // `extension_set_enabled` still accepts their ids for compatibility, but the
    // tab headers write those settings flags directly.
    let mut cards: Vec<ExtensionCard> = Vec::new();
    // Installed packs — including the Agent centre layout, which is now a real
    // external pack (Phase 5C) rendered through this same path, not a
    // host-synthesised special case.
    for rec in reg.records() {
        let (name, description, repository, capabilities, has_detail, tier, prompt_layers) =
            match load_pack(&app, &rec.id) {
                Ok(p) => {
                    // A pack with prompt layers has something worth opening even
                    // with no settings or shortcuts of its own — the text it puts
                    // in front of the model is the whole reason to look.
                    let prompt_layers: Vec<PromptLayerInfo> = p
                        .manifest
                        .contributes
                        .prompt_layers
                        .iter()
                        .map(PromptLayerInfo::from_decl)
                        .collect();
                    let has_detail = !p.manifest.contributes.settings.is_empty()
                        || !p.manifest.contributes.shortcuts.is_empty()
                        || !prompt_layers.is_empty();
                    let tier = match p.manifest.tier {
                        grain_sdk::Tier::Pack => "pack",
                        grain_sdk::Tier::Scripted => "scripted",
                        grain_sdk::Tier::Native => "native",
                    };
                    (
                        p.manifest.name,
                        p.manifest.description,
                        p.manifest.repository,
                        p.manifest.permissions,
                        has_detail,
                        tier,
                        prompt_layers,
                    )
                }
                // SPEC §6 last row: a broken/missing pack file renders an error
                // card; it never takes the page down.
                Err(e) => (
                    rec.id.clone(),
                    format!("Unreadable pack: {e}"),
                    None,
                    Vec::new(),
                    false,
                    "pack",
                    Vec::new(),
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
            capabilities,
            prompt_layers,
            has_detail,
            slots: rec
                .slots
                .iter()
                .chain(rec.variant_slots.iter())
                .cloned()
                .collect(),
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
            //
            // [GRAIN] Prompt layers are asked for in the SAME sheet, because a
            // prompt layer needs no capability — which is what makes it the safe
            // way to contribute, and also what would let a pack ship harmless
            // wording, get approved, and change it in an update with nothing
            // asking again. That is the rug pull (CVE-2025-54136). The TEXT is
            // part of what was approved, so when it no longer matches, the
            // extension is held and the user reads the new wording before it can
            // shape a single dictation.
            //
            // Both are gathered before either is returned: two sheets in a row
            // for one enable is how a user learns to click Approve without
            // reading, which defeats the point of asking at all.
            if enabled {
                let granted = reg.record(pack_id).map(|r| r.granted).unwrap_or_default();
                let missing: Vec<String> = if pack.has_runtime() {
                    pack.manifest
                        .permissions
                        .iter()
                        .filter(|p| !granted.contains(p))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };
                // Inert packs included, deliberately: a tier-A pack with a
                // prompt layer is exactly the case the sheet would never see.
                let declared = &pack.manifest.contributes.prompt_layers;
                let unapproved = !declared.is_empty() && {
                    let approved = reg
                        .record(pack_id)
                        .and_then(|r| r.prompt_layers_approved)
                        .unwrap_or_default();
                    approved != ext::prompt_layers_fingerprint(declared)
                };
                // Same question for actions, and it has to be asked here or an
                // extension that declares one stays permanently inert: the
                // routing gate refuses an unapproved declaration, and nothing
                // else would ever ask the user about it.
                let declared_actions = &pack.manifest.contributes.actions;
                let actions_unapproved = !declared_actions.is_empty() && {
                    let approved = reg
                        .record(pack_id)
                        .and_then(|r| r.actions_approved)
                        .unwrap_or_default();
                    approved != ext::actions_fingerprint(declared_actions)
                };
                if !missing.is_empty() || unapproved || actions_unapproved {
                    let layers: Vec<PromptLayerInfo> = if unapproved {
                        declared.iter().map(PromptLayerInfo::from_decl).collect()
                    } else {
                        Vec::new()
                    };
                    let actions: Vec<ActionInfo> = if actions_unapproved {
                        declared_actions.iter().map(ActionInfo::from_decl).collect()
                    } else {
                        Vec::new()
                    };
                    // One sheet carrying all three. Two sheets in a row is how a
                    // user learns to click through without reading.
                    return Err(serde_json::json!({
                        "needsPermissions": missing,
                        "needsPromptLayers": layers,
                        "needsActions": actions,
                    })
                    .to_string());
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
        // Dev records approve their own prompt layers. The user pointed Grain
        // at a folder on their own disk, which is the same act as approving it,
        // and the alternative is a permission sheet on every iteration of a
        // sentence the author is actively writing. Store and manual-import
        // packs get no such shortcut — see `extension_import_pack`.
        prompt_layers_approved: (!loaded.pack.manifest.contributes.prompt_layers.is_empty())
            .then(|| {
                ext::prompt_layers_fingerprint(&loaded.pack.manifest.contributes.prompt_layers)
            }),
        actions_approved: (!loaded.pack.manifest.contributes.actions.is_empty())
            .then(|| ext::actions_fingerprint(&loaded.pack.manifest.contributes.actions)),
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
    /// The card's self-contained HTML, for a `panel` field (None otherwise).
    pub ui_source: Option<String>,
}

/// Flatten a declaration into its renderer schema (no value). Shared by
/// top-level rows and nested list fields.
fn field_schema(decl: &grain_sdk::SettingDecl) -> ExtensionSettingField {
    use grain_sdk::SettingKind as K;
    // A `panel` carries its markup out to the renderer; every other kind has none.
    let ui_source = match &decl.kind {
        K::Panel { ui_source, .. } => Some(ui_source.clone()),
        _ => None,
    };
    let (kind, min, max, step, options, fields, item_label) = match &decl.kind {
        K::Bool => ("bool", None, None, None, vec![], vec![], None),
        K::String => ("string", None, None, None, vec![], vec![], None),
        K::Secret => ("secret", None, None, None, vec![], vec![], None),
        K::Shortcut => ("shortcut", None, None, None, vec![], vec![], None),
        K::Color => ("color", None, None, None, vec![], vec![], None),
        K::AppPath => ("app_path", None, None, None, vec![], vec![], None),
        K::Url => ("url", None, None, None, vec![], vec![], None),
        K::Number { min, max } => ("number", *min, *max, None, vec![], vec![], None),
        K::Slider { min, max, step } => (
            "slider",
            Some(*min),
            Some(*max),
            *step,
            vec![],
            vec![],
            None,
        ),
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
        K::Panel { .. } => ("panel", None, None, None, vec![], vec![], None),
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
        ui_source,
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
    /// The card's self-contained HTML, for a `panel` row (None otherwise).
    pub ui_source: Option<String>,
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
        ui_source: schema.ui_source,
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
            // A panel (custom card) holds no host-managed value — it manages its
            // own state through the extension API — so it skips schema resolution.
            if matches!(decl.kind, grain_sdk::SettingKind::Panel { .. }) {
                return setting_row(decl, serde_json::Value::Null, None);
            }
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
        // A manually imported pack is a third-party file, so its prompt text is
        // NOT approved by importing it: the prior approval is carried forward
        // and a re-import with changed wording therefore stops matching, holds
        // the enable, and shows the user what changed. This is the update path
        // the rug-pull incidents of 2025 walked through.
        prompt_layers_approved: prior.as_ref().and_then(|r| r.prompt_layers_approved.clone()),
        // Carried, never recomputed. Importing is not approving: if the
        // declaration changed, this stops matching, the actions go inert, and
        // the enable path shows the user what is different.
        actions_approved: prior.as_ref().and_then(|r| r.actions_approved.clone()),
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

/// [GRAIN] The custom card's host channel (SPEC §4.1 Level 3). A `panel` setting
/// renders the extension's own UI in a sandboxed iframe; that iframe posts host
/// calls up to Grain's (trusted) settings page, which relays them here. The
/// panel gets EXACTLY the capabilities the user granted this extension: the
/// grants come from the registry RECORD, never from the caller, so a card can
/// neither assert another identity nor widen its own. Every method is then
/// capability-checked by `host_api::dispatch`, identically to a worker. The
/// error crosses back as the same `{code,message,hint,…}` shape the worker gets.
#[tauri::command]
#[specta::specta]
pub async fn extension_host_call(
    app: AppHandle,
    id: String,
    method: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, serde_json::Value> {
    use grain_core::extensions as ext;
    let internal = |message: &str, hint: &str| {
        serde_json::to_value(grain_sdk::HostError::new(
            grain_sdk::HostErrorCode::Internal,
            message,
            hint,
        ))
        .unwrap_or_else(|_| serde_json::json!({ "code": "E_INTERNAL", "message": message }))
    };
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or_else(|| internal("extensions registry unavailable", ""))?;
    let record = reg.record(&id).ok_or_else(|| {
        internal(
            "unknown extension",
            "Reinstall the extension and try again.",
        )
    })?;
    if !record.enabled {
        return Err(internal(
            "extension is disabled",
            "Enable the extension to use its settings card.",
        ));
    }
    let identity = crate::events_auth::ClientIdentity {
        id: id.clone(),
        role: crate::events_auth::ClientRole::Surface,
        caps: crate::events_auth::CapabilitySet::Named(record.granted.iter().cloned().collect()),
    };
    crate::host_api::dispatch(&app, &identity, &method, params)
        .await
        .map_err(|error| {
            serde_json::to_value(&error).unwrap_or_else(
                |_| serde_json::json!({ "code": "E_INTERNAL", "message": error.message }),
            )
        })
}

/// Record the user's approval of what an extension asked for (SPEC §6) —
/// capabilities, and the prompt layers it contributes. Called by the permission
/// sheet on Approve; the caller then retries enable.
///
/// Grants are clamped to what the manifest actually requests, so neither a
/// compromised frontend nor a stale sheet can widen an extension's reach beyond
/// what the user was shown.
///
/// **Prompt layers are approved here too**, by the same act and with no
/// parameter of their own: the approved value is recomputed from the pack on
/// disk, so what gets recorded is necessarily the text the sheet just rendered
/// and never something the caller supplies. An inert pack whose only ask is a
/// prompt layer therefore approves through `extension_grant(id, [])` — one
/// approval act rather than a second command that could drift from this one.
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
    let manifest = load_pack(&app, &id)?.manifest;
    if let Some(extra) = permissions
        .iter()
        .find(|p| !manifest.permissions.contains(p))
    {
        return Err(format!("'{extra}' is not requested by this extension"));
    }
    for p in permissions {
        if !rec.granted.contains(&p) {
            rec.granted.push(p);
        }
    }
    rec.prompt_layers_approved = (!manifest.contributes.prompt_layers.is_empty())
        .then(|| ext::prompt_layers_fingerprint(&manifest.contributes.prompt_layers));
    // Recomputed from disk, never taken from the caller — the fingerprint IS the
    // grant, so accepting one over the wire would let a caller approve a
    // declaration the user never saw.
    rec.actions_approved = (!manifest.contributes.actions.is_empty())
        .then(|| ext::actions_fingerprint(&manifest.contributes.actions));
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
    // Grain's own features have no record to remove: they are turned off in
    // their own tab, never uninstalled. Everything else is a real installed pack
    // with a store to reinstall from.
    if id == ext::BUILTIN_SNIPPETS
        || id == ext::BUILTIN_CONTEXT
        || id == ext::BUILTIN_AGENT
        || id == ext::BUILTIN_GRAIN_SPACE
    {
        return Err("built-in features can be turned off, not uninstalled".into());
    }
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let dev_active = reg.dev_path(&id).is_some();
    if !reg.uninstall(&id).map_err(|e| e.to_string())? && !dev_active {
        return Err("not installed".into());
    }
    crate::extension_host::refresh_index(&app);
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
