//! [GRAIN] Grain-only Tauri settings commands, out of the Handy-derived
//! `shortcut/mod.rs` (Handy Isolation phase 6). Upstream owns the shortcut
//! registration/dispatch machinery in that module; these are the setting
//! mutators for Grain's own features — context awareness,
//! "scrap that", snippets, voice actions, app modes, the Agent,
//! rolling preview.
//!
//! Each is still a `#[tauri::command]`, so the command NAME (and therefore the
//! frontend `invoke` + generated bindings) is unchanged by the move; only the
//! path in `lib.rs`'s `collect_commands!` differs.

use crate::settings;
use crate::settings::DefaultPanel;
use log::warn;
use tauri::{AppHandle, Manager};

/// Capture, Agent and Prompt Record keys reserve their saved chords even while a feature
/// is disabled and its OS shortcut is unregistered. Other bindings are outside
/// this policy.
pub(crate) fn capture_shortcut_conflicts(
    settings: &settings::AppSettings,
    id: &str,
    candidate: &str,
) -> bool {
    const IDS: [&str; 4] = [
        "transcribe",
        "summon_agent",
        "transcribe_send_to_ai",
        "prompt_record",
    ];
    if !IDS.contains(&id) {
        return false;
    }

    fn conflicts<T: std::str::FromStr + PartialEq>(
        settings: &settings::AppSettings,
        ids: &[&str],
        id: &str,
        candidate: &str,
    ) -> bool {
        let Ok(candidate) = candidate.parse::<T>() else {
            return false; // The existing validator reports malformed shortcuts.
        };
        settings
            .bindings
            .iter()
            .filter(|(other, _)| {
                other.as_str() != id && (ids.contains(&other.as_str()) || id == "prompt_record")
            })
            .any(|(_, binding)| {
                binding
                    .current_binding
                    .parse::<T>()
                    .ok()
                    .is_some_and(|stored| stored == candidate)
            })
    }

    match settings.keyboard_implementation {
        settings::KeyboardImplementation::Tauri => {
            conflicts::<tauri_plugin_global_shortcut::Shortcut>(settings, &IDS, id, candidate)
        }
        settings::KeyboardImplementation::HandyKeys => {
            conflicts::<handy_keys::Hotkey>(settings, &IDS, id, candidate)
        }
    }
}

#[cfg(test)]
mod capture_shortcut_conflict_tests {
    use super::*;

    #[test]
    fn every_capture_and_agent_pair_conflicts_even_when_features_are_off() {
        let ids = [
            "transcribe",
            "summon_agent",
            "transcribe_send_to_ai",
            "prompt_record",
        ];
        for implementation in [
            settings::KeyboardImplementation::Tauri,
            settings::KeyboardImplementation::HandyKeys,
        ] {
            for owner in ids {
                let mut settings = settings::get_default_settings();
                settings.keyboard_implementation = implementation;
                settings.agent_enabled = false;
                settings.post_process_enabled = false;
                settings.bindings.get_mut(owner).unwrap().current_binding =
                    "alt+shift+space".into();

                for target in ids.into_iter().filter(|id| *id != owner) {
                    assert!(
                        capture_shortcut_conflicts(&settings, target, "Shift+Alt+Space"),
                        "{implementation:?}: {owner} should reserve its key from {target}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_policy_does_not_claim_a_binding_it_does_not_own() {
        for implementation in [
            settings::KeyboardImplementation::Tauri,
            settings::KeyboardImplementation::HandyKeys,
        ] {
            let mut settings = settings::get_default_settings();
            settings.keyboard_implementation = implementation;
            assert!(!capture_shortcut_conflicts(
                &settings,
                "transcribe",
                "not_a_key"
            ));
            assert!(!capture_shortcut_conflicts(
                &settings,
                "transcribe",
                &settings.bindings["transcribe"].current_binding
            ));
            settings
                .bindings
                .get_mut("paste_catch_deliver")
                .unwrap()
                .current_binding = "alt+ctrl+f9".into();
            assert!(!capture_shortcut_conflicts(
                &settings,
                "summon_agent",
                "ctrl+alt+f9"
            ));
        }
    }

    #[test]
    fn prompt_record_default_is_supported_and_cannot_override_cancel() {
        for implementation in [
            settings::KeyboardImplementation::Tauri,
            settings::KeyboardImplementation::HandyKeys,
        ] {
            let mut settings = settings::get_default_settings();
            settings.keyboard_implementation = implementation;
            let chord = &settings.bindings["prompt_record"].current_binding;
            assert!(chord
                .parse::<tauri_plugin_global_shortcut::Shortcut>()
                .is_ok());
            assert!(chord.parse::<handy_keys::Hotkey>().is_ok());
            assert!(!capture_shortcut_conflicts(
                &settings,
                "prompt_record",
                chord
            ));
            assert!(capture_shortcut_conflicts(
                &settings,
                "prompt_record",
                &settings.bindings["cancel"].current_binding
            ));
        }
    }
}

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
        let native_path = if cfg!(windows) {
            r"C:\Program Files\Figma\Figma.exe"
        } else {
            "/opt/Figma/Figma.exe"
        };
        assert_eq!(normalise_exe(native_path), "figma");
        assert_eq!(normalise_exe(&format!("\"{native_path}\"")), "figma");
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
/// matcher is never invoked for it).
#[tauri::command]
#[specta::specta]
pub fn change_scrap_that_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.scrap_that_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Toggle Paste Catch, the missed-text-field clipboard safety net.
/// Turning it off discards any pending copy and releases its delivery shortcut.
#[tauri::command]
#[specta::specta]
pub fn change_paste_catch_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.paste_catch_enabled = enabled;
    settings::write_settings(&app, settings);
    if !enabled {
        crate::paste_catch::supersede(&app);
    }
    Ok(())
}

/// Enable the core Agent and reconcile its summon shortcut independently of extensions.
#[tauri::command]
#[specta::specta]
pub fn change_agent_enabled_setting(
    app: AppHandle,
    window: tauri::WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    require_main_window(&window)?;
    let mut settings = settings::get_settings(&app);
    if settings.agent_enabled == enabled {
        return Ok(());
    }
    // Before onboarding initializes shortcuts, saving the setting is enough:
    // initialization will register the enabled Agent alongside the other keys.
    if app
        .try_state::<crate::commands::ShortcutsInitialized>()
        .is_some()
    {
        let binding = settings::get_stored_binding(&app, "summon_agent")?;
        if enabled {
            crate::shortcut::register_shortcut(&app, binding)?;
        } else {
            crate::shortcut::unregister_shortcut(&app, binding)?;
        }
    }
    settings.agent_enabled = enabled;
    settings::write_settings(&app, settings);
    // [GRAIN] The resting notch sliver exists only while the Agent is enabled.
    crate::grain_notch_sliver::sync(&app);
    Ok(())
}

/// Enable core snippet matching independently of extensions.
#[tauri::command]
#[specta::specta]
pub fn change_snippets_enabled_setting(
    app: AppHandle,
    window: tauri::WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    require_main_window(&window)?;
    let mut settings = settings::get_settings(&app);
    settings.snippets_enabled = enabled;
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

/// [GRAIN] Recall: summarise each meeting once its transcript is published. OFF
/// by default; see `Settings::meet_auto_summary`.
#[tauri::command]
#[specta::specta]
pub fn change_meet_auto_summary_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.meet_auto_summary = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Recall: transcribe a meeting as soon as it is stopped. OFF by
/// default; see `Settings::meet_auto_transcribe`.
#[tauri::command]
#[specta::specta]
pub fn change_meet_auto_transcribe_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.meet_auto_transcribe = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Recall: title a freshly transcribed meeting from what was said. OFF
/// by default; see `Settings::meet_auto_title`.
#[tauri::command]
#[specta::specta]
pub fn change_meet_auto_title_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.meet_auto_title = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// [GRAIN] Detect the foreground app right now. Returns `None` when nothing can be
/// resolved (unsupported platform, no foreground window). Silent — no UI.
#[tauri::command]
#[specta::specta]
pub fn detect_active_app() -> Option<DetectedApp> {
    // The capture button only needs the app/URL, not focused-field terms.
    crate::context_detect::detect_active_context().map(|c| DetectedApp {
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

/// [GRAIN] Pill identity: show the icon of the app being dictated into in place
/// of the pill's state dot. Takes effect on the next session — the icon is
/// resolved at record-start, never held between sessions.
#[tauri::command]
#[specta::specta]
pub fn change_pill_show_app_icon_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    app.state::<std::sync::Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.pill_show_app_icon = enabled)
        .map_err(|error| error.to_string())?;
    crate::grain_overlay::set_app_icon_enabled(&app, enabled);
    Ok(())
}

pub(crate) fn require_main_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    (window.label() == "main")
        .then_some(())
        .ok_or_else(|| "extension management is available only from Grain settings".to_string())
}

/// Catalogue preferences do not alter the recognition language or selected model.
#[tauri::command]
#[specta::specta]
pub fn change_spoken_languages_setting(
    app: AppHandle,
    languages: Vec<String>,
) -> Result<(), String> {
    app.state::<std::sync::Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.spoken_languages = languages)
        .map_err(|error| error.to_string())
}
