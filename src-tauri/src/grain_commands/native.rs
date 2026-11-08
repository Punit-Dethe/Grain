use super::require_main_window;
use crate::settings;
use tauri::{AppHandle, Manager};

// ── [GRAIN] Extension platform, Phase 1 (SPEC §5.1, §10.1) ──────────────────

/// One row of the Extensions Overview tab. Built-ins delegate their enabled
/// state to core settings flags (manifest-first, PLAN.md D4); installed packs
/// read the registry.
#[derive(serde::Serialize, specta::Type, Clone)]
pub struct ExtensionCard {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Grain-derived 128² PNG for settings. Never the resident 512² master.
    pub icon: Option<String>,
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
    /// Manifest-declared capabilities displayed during fixture approval.
    pub capabilities: Vec<String>,
    /// [GRAIN] The tools this extension gives the Agent, so the card can show
    /// them again after the approval sheet closes.
    pub actions: Vec<ActionInfo>,
}

/// [GRAIN] One declared action, as the approval sheet and the extension card
/// need it (`docs/Extensions V1/PLAN.md` §5).
///
/// Note what is **absent**: the utterance list. The consent question is "what
/// can this do", not "what words does it listen for" — a list of phrasings is
/// review and `doctor` material, and putting it on a sheet trains people to
/// scroll past the part that matters. What the user decides on is the title and
/// whether it will ask before acting.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, specta::Type)]
pub struct ActionInfo {
    pub id: String,
    /// One plain line, written for the user.
    pub title: String,
    /// Whether performing this reads the resolved action back first. The single
    /// most important thing on the row.
    pub confirms: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct AuthenticationApprovalInfo {
    pub provider_name: String,
    pub scopes: Vec<String>,
    pub api_hosts: Vec<String>,
    pub authorization_host: String,
    pub token_host: String,
}

impl AuthenticationApprovalInfo {
    fn from_decl(decl: &grain_sdk::AuthenticationDecl) -> Self {
        let host = |value: &str| {
            reqwest::Url::parse(value)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_default()
        };
        Self {
            provider_name: decl.provider_name.clone(),
            scopes: decl.scopes.clone(),
            api_hosts: decl.api_hosts.clone(),
            authorization_host: host(&decl.authorization_endpoint),
            token_host: host(&decl.token_endpoint),
        }
    }
}

impl ActionInfo {
    fn from_decl(decl: &grain_sdk::manifest::ActionDecl) -> Self {
        Self {
            id: decl.id.clone(),
            title: decl.title.clone(),
            confirms: !decl.risk.is_safe(),
        }
    }
}

/// The Overview tab's data: every extension, enabled and disabled alike.
#[tauri::command]
#[specta::specta]
pub fn extensions_overview(app: AppHandle) -> Result<Vec<ExtensionCard>, String> {
    if !cfg!(feature = "agent-harness") {
        return Ok(Vec::new());
    }
    use grain_core::extensions as ext;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    // [GRAIN] Grain's three always-present features (Snippets, Context
    // Awareness, Agent) are NOT listed here. They ship with the app, have a tab
    // each, and their master switch is that tab's header — listing them beside
    // installed packs implied they could be uninstalled and buried the list of
    // things the user actually chose to install.
    let mut cards: Vec<ExtensionCard> = Vec::new();
    // Installed tool packages, including quarantined records for recovery.
    for rec in reg.records() {
        let facts = match load_pack(&app, &rec.id) {
            Ok(p) => {
                let actions: Vec<ActionInfo> = p
                    .manifest
                    .contributes
                    .actions
                    .iter()
                    .map(ActionInfo::from_decl)
                    .collect();
                PackFacts {
                    name: p.manifest.name,
                    description: p.manifest.description,
                    repository: p.manifest.repository,
                    capabilities: p.manifest.permissions,
                    tier: "scripted",
                    actions,
                }
            }
            // SPEC §6 last row: a broken/missing pack file renders an error
            // card; it never takes the page down.
            Err(e) => PackFacts {
                name: rec.id.clone(),
                description: format!(
                    "Extension unavailable: {}",
                    reg.quarantine_reason(&rec.id).unwrap_or(e)
                ),
                ..PackFacts::default()
            },
        };
        let PackFacts {
            name,
            description,
            repository,
            capabilities,
            tier,
            actions,
        } = facts;
        cards.push(ExtensionCard {
            id: rec.id.clone(),
            name,
            description,
            icon: crate::extension_icons::ui_icon(&app, &rec.id),
            version: rec.installed_version.clone(),
            tier: tier.to_string(),
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
            actions,
        });
    }
    Ok(cards)
}

/// Everything one card needs out of a manifest, so the read and the build are
/// not coupled by an eleven-element tuple whose fields are mostly `String` and
/// `Vec<String>` — a transposition there is invisible to the compiler and
/// mislabels an extension on a consent surface.
struct PackFacts {
    name: String,
    description: String,
    repository: Option<String>,
    capabilities: Vec<String>,
    tier: &'static str,
    actions: Vec<ActionInfo>,
}

impl Default for PackFacts {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            repository: None,
            capabilities: Vec::new(),
            tier: "pack",
            actions: Vec::new(),
        }
    }
}

/// Flip a tool extension on/off. Core Agent, Snippets and context settings use
/// their own commands and cannot be enabled through the extension registry.
#[tauri::command]
#[specta::specta]
pub fn extension_set_enabled(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    use grain_core::extensions as ext;
    require_main_window(&window)?;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;

    match id.as_str() {
        ext::BUILTIN_SNIPPETS | ext::BUILTIN_CONTEXT | ext::BUILTIN_AGENT => {
            return Err("Core features are configured independently of extensions.".into());
        }
        // Imported packs: registry bit + payload application.
        pack_id if reg.is_installed(pack_id) => {
            let revision = reg.record_revision(pack_id);
            let record = reg
                .record(pack_id)
                .ok_or("extension is no longer installed")?;
            if !enabled {
                let persisted = reg
                    .set_enabled(pack_id, false)
                    .map_err(|error| error.to_string());
                stop_extension_runtime(pack_id, "extension disabled");
                crate::extension_host::quarantine_invalid_packages(&app);
                // The in-memory disable already took effect. Disk failure must
                // still retire its worker/account before reporting persistence.
                persisted?;
                return Ok(());
            }
            let pack = load_pack(&app, pack_id)?;
            // [GRAIN] SPEC §6 (the Chrome model): a scripted extension is HELD
            // at first enable until the user approves the capabilities its
            // manifest requests. Never grant implicitly — the whole point is
            // that code cannot start running on capabilities nobody approved.
            // The frontend catches this structured error, shows the permission
            // sheet, calls `extension_grant`, and retries.
            //
            // Approve scoped permissions, exact tools and account configuration together.
            if enabled {
                let granted = &record.granted;
                let missing: Vec<String> = pack
                    .manifest
                    .permissions
                    .iter()
                    .filter(|p| !granted.contains(p))
                    .cloned()
                    .collect();
                let declared_actions = &pack.manifest.contributes.actions;
                let actions_unapproved = !declared_actions.is_empty() && {
                    record.actions_approved.as_deref()
                        != Some(ext::actions_fingerprint(declared_actions).as_str())
                };
                let declared_authentication = pack.manifest.contributes.authentication.as_ref();
                let authentication_unapproved = declared_authentication.is_some() && {
                    record.authentication_approved
                        != declared_authentication.map(ext::authentication_fingerprint)
                };
                if !missing.is_empty() || actions_unapproved || authentication_unapproved {
                    reg.with_current_record(pack_id, revision, |_| ())
                        .map_err(|error| error.to_string())?;
                    let approval_digest = ext::approval_fingerprint(&record, &pack)
                        .map_err(|error| error.to_string())?;
                    let actions: Vec<ActionInfo> = if actions_unapproved {
                        declared_actions.iter().map(ActionInfo::from_decl).collect()
                    } else {
                        Vec::new()
                    };
                    let authentication = authentication_unapproved
                        .then(|| declared_authentication.map(AuthenticationApprovalInfo::from_decl))
                        .flatten();
                    // One sheet carrying all three. Two sheets in a row is how a
                    // user learns to click through without reading.
                    return Err(serde_json::json!({
                        "approvalDigest": approval_digest,
                        "reviewedName": pack.manifest.name,
                        "needsPermissions": missing,
                        "needsActions": actions,
                        "needsAuthentication": authentication,
                    })
                    .to_string());
                }
            }
            reg.set_enabled_at_revision(pack_id, true, revision)
                .map_err(|error| error.to_string())?;
            // The activation/transform index is what the paste path and event
            // bus read; it must never lag the registry.
            crate::extension_host::quarantine_invalid_packages(&app);
            return Ok(());
        }
        other => return Err(format!("unknown extension id '{other}'")),
    }
}

pub(crate) fn load_pack(app: &AppHandle, id: &str) -> Result<grain_sdk::GrainPack, String> {
    crate::extension_host::load_manifest_result(app, id)
}

fn read_pack_input(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let file =
        std::fs::File::open(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(grain_sdk::PACK_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if bytes.len() as u64 > grain_sdk::PACK_MAX_BYTES {
        return Err(format!(
            ".grainpack exceeds the {} MiB import limit",
            grain_sdk::PACK_MAX_BYTES / (1024 * 1024)
        ));
    }
    Ok(bytes)
}

fn imported_update_can_stay_enabled(
    prior: Option<&grain_core::extensions::ExtensionRecord>,
    pack: &grain_sdk::GrainPack,
) -> bool {
    use grain_core::extensions as ext;

    let Some(prior) = prior.filter(|record| record.enabled) else {
        return false;
    };
    let manifest = &pack.manifest;
    if manifest
        .permissions
        .iter()
        .any(|permission| !prior.granted.contains(permission))
    {
        return false;
    }
    let actions_match = manifest.contributes.actions.is_empty()
        || prior.actions_approved.as_deref()
            == Some(ext::actions_fingerprint(&manifest.contributes.actions).as_str());
    let authentication_match =
        manifest
            .contributes
            .authentication
            .as_ref()
            .is_none_or(|declaration| {
                prior.authentication_approved.as_deref()
                    == Some(ext::authentication_fingerprint(declaration).as_str())
            });
    actions_match && authentication_match
}

#[cfg(test)]
mod imported_update_security_tests {
    use super::*;

    #[test]
    fn permission_review_binds_source_auth_grants_and_execution_identity() {
        let pack = auth_pack("https://login.example.com/oauth/token");
        let record = approved_record(&pack);
        let digest = grain_core::extensions::approval_fingerprint(&record, &pack).unwrap();
        for mode in ["source", "auth", "identity", "grant", "version", "wrong_id"] {
            let mut record = record.clone();
            let mut changed = pack.clone();
            match mode {
                "source" => changed.manifest.entry_source.push_str("// replacement"),
                "auth" => changed
                    .manifest
                    .contributes
                    .authentication
                    .as_mut()
                    .unwrap()
                    .scopes
                    .push("write".into()),
                "identity" => record.execution_generation += 1,
                "grant" => record.granted.clear(),
                "version" => record.installed_version.push('2'),
                "wrong_id" => changed.manifest.id = "com.example.other".into(),
                _ => unreachable!(),
            }
            assert!(
                reviewed_grant_record(record, &changed, vec![], &digest).is_err(),
                "{mode}"
            );
        }
        assert!(reviewed_grant_record(record, &pack, vec![], &digest).is_ok());
    }

    #[test]
    fn reviewed_grants_and_activation_have_one_owner_and_reject_replay() {
        let dir = tempfile::tempdir().unwrap();
        let registry = grain_core::extensions::ExtensionsRegistry::load(dir.path()).unwrap();
        let pack = auth_pack("https://login.example.com/oauth/token");
        let mut record = approved_record(&pack);
        record.enabled = false;
        record.granted.clear();
        record.authentication_approved = None;
        registry.install(record).unwrap();
        let record = registry.record(&pack.manifest.id).unwrap();
        let revision = registry.record_revision(&record.id);
        let digest = grain_core::extensions::approval_fingerprint(&record, &pack).unwrap();
        assert!(
            reviewed_grant_record(record.clone(), &pack, vec!["auth".into()], &digest).is_err()
        );
        assert!(
            reviewed_grant_record(record.clone(), &pack, vec!["capture".into()], &digest).is_err()
        );
        let approved = reviewed_grant_record(
            record.clone(),
            &pack,
            pack.manifest.permissions.clone(),
            &digest,
        )
        .unwrap();
        registry
            .approve_and_enable_if_current(approved, revision)
            .unwrap();
        let current = registry.record(&record.id).unwrap();
        assert!(current.enabled);
        assert_eq!(current.granted, pack.manifest.permissions);
        assert!(current.authentication_approved.is_some());
        assert!(
            reviewed_grant_record(current, &pack, pack.manifest.permissions.clone(), &digest)
                .is_err()
        );
    }

    #[test]
    fn explicit_disable_and_late_replacement_defeat_review_before_commit() {
        let dir = tempfile::tempdir().unwrap();
        let registry = grain_core::extensions::ExtensionsRegistry::load(dir.path()).unwrap();
        let pack = auth_pack("https://login.example.com/oauth/token");
        let mut record = approved_record(&pack);
        record.enabled = false;
        record.granted.clear();
        registry.install(record).unwrap();
        let record = registry.record(&pack.manifest.id).unwrap();
        let revision = registry.record_revision(&record.id);
        let digest = grain_core::extensions::approval_fingerprint(&record, &pack).unwrap();
        let approved =
            reviewed_grant_record(record, &pack, pack.manifest.permissions.clone(), &digest)
                .unwrap();
        registry.set_enabled(&pack.manifest.id, false).unwrap();
        assert!(registry
            .approve_and_enable_if_current(approved, revision)
            .is_err());
        assert!(!registry.is_enabled(&pack.manifest.id));
        assert!(registry
            .record(&pack.manifest.id)
            .unwrap()
            .granted
            .is_empty());
    }

    fn auth_pack(token_endpoint: &str) -> grain_sdk::GrainPack {
        serde_json::from_value(serde_json::json!({
            "manifest": {
                "id": "com.example.secure",
                "name": "Secure",
                "version": "1.0.0",
                "grainApi": "^1.0",
                "tier": "scripted",
                "entry_source": "export default {}",
                "permissions": ["auth", "net:api.example.com"],
                "contributes": {
                    "authentication": {
                        "type": "oauth2-pkce",
                        "providerName": "Service",
                        "clientId": "public-client",
                        "authorizationEndpoint": "https://login.example.com/oauth/authorize",
                        "tokenEndpoint": token_endpoint,
                        "scopes": ["read"],
                        "apiHosts": ["api.example.com"]
                    }
                }
            }
        }))
        .unwrap()
    }

    fn approved_record(pack: &grain_sdk::GrainPack) -> grain_core::extensions::ExtensionRecord {
        grain_core::extensions::ExtensionRecord {
            id: pack.manifest.id.clone(),
            enabled: true,
            execution_generation: 0,
            toggle_seq: 1,
            installed_version: pack.manifest.version.clone(),
            artifact_sha256: None,
            granted: pack.manifest.permissions.clone(),
            actions_approved: None,
            authentication_approved: Some(grain_core::extensions::authentication_fingerprint(
                pack.manifest.contributes.authentication.as_ref().unwrap(),
            )),
            authentication_session: None,
            dev: None,
            trust: grain_sdk::Trust::Dev,
        }
    }

    #[test]
    fn unchanged_approved_authentication_can_remain_enabled() {
        let pack = auth_pack("https://login.example.com/oauth/token");
        let record = approved_record(&pack);
        assert!(imported_update_can_stay_enabled(Some(&record), &pack));
    }

    #[test]
    fn changed_token_endpoint_holds_an_enabled_import_for_reapproval() {
        let approved = auth_pack("https://login.example.com/oauth/token");
        let record = approved_record(&approved);
        let changed = auth_pack("https://other.example.com/oauth/token");
        assert!(!imported_update_can_stay_enabled(Some(&record), &changed));
    }

    #[test]
    fn a_new_capability_holds_an_enabled_import_for_reapproval() {
        let approved = auth_pack("https://login.example.com/oauth/token");
        let record = approved_record(&approved);
        let mut changed = approved;
        changed
            .manifest
            .permissions
            .push("capture:selection".into());
        assert!(!imported_update_can_stay_enabled(Some(&record), &changed));
    }

    #[test]
    fn imported_artifact_publication_never_replaces_another_owner() {
        let directory = tempfile::tempdir().unwrap();
        let publish = |bytes: &[u8]| {
            grain_core::install::publish_artifact(
                directory.path(),
                "com.example.tools",
                "1",
                &grain_core::trust::sha256_hex(bytes),
                bytes,
            )
            .unwrap()
        };
        let old = publish(b"approved-old");
        let abandoned = publish(b"uncommitted-new");
        let current = publish(b"committed-new");
        assert_ne!(old, abandoned);
        assert_ne!(abandoned, current);
        assert_eq!(
            std::fs::read(old.join("pack.grainpack.json")).unwrap(),
            b"approved-old"
        );
        assert_eq!(
            std::fs::read(current.join("pack.grainpack.json")).unwrap(),
            b"committed-new"
        );
    }

    #[test]
    fn pack_input_is_bounded_before_json_deserialization() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("oversized.grainpack");
        std::fs::write(&input, vec![b'x'; grain_sdk::PACK_MAX_BYTES as usize + 1]).unwrap();
        assert!(read_pack_input(&input).is_err());
    }
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

fn stop_extension_runtime(id: &str, reason: &str) {
    crate::extension_host::stop_extension(id, reason);
    crate::grain_auth::cancel_extension(id);
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
    let mut cleanup_error = None;
    if !enabled {
        crate::grain_mcp::invalidate_development_sessions();
        let ids: Vec<String> = reg.dev_records().into_iter().map(|(id, _)| id).collect();
        for id in ids {
            stop_extension_runtime(&id, "developer mode disabled");
            reg.unload_dev(&id).map_err(|error| error.to_string())?;
        }
        if let Err(error) = crate::extension_icons::purge_dev_cache(&app) {
            log::warn!("[GRAIN] developer icon-cache cleanup failed: {error}");
            cleanup_error = Some(error);
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
    crate::extension_host::quarantine_invalid_packages(&app);
    match cleanup_error {
        Some(error) => Err(format!(
            "Developer mode is off, but developer artifacts could not be removed: {error}"
        )),
        None => Ok(()),
    }
}

fn register_unpacked_project(
    app: &AppHandle,
    loaded: crate::dev_extensions::LoadedDevProject,
) -> Result<String, String> {
    use grain_core::extensions as ext;
    // Keep the developer-mode gate in the mutation helper itself as well as in
    // every command caller. A future internal caller cannot accidentally turn
    // validated local code into a developer-mode bypass.
    if !settings::get_settings(app).extension_developer_mode {
        return Err("Developer mode is disabled".into());
    }
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let id = loaded.pack.manifest.id.clone();
    let revision = reg.record_revision(&id);
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
        execution_generation: 0,
        toggle_seq: prior.as_ref().map(|record| record.toggle_seq).unwrap_or(0),
        installed_version: loaded.pack.manifest.version.clone(),
        artifact_sha256: None,
        granted,
        // Loading the author's own project approves its current actions.
        actions_approved: (!loaded.pack.manifest.contributes.actions.is_empty())
            .then(|| ext::actions_fingerprint(&loaded.pack.manifest.contributes.actions)),
        authentication_approved: loaded
            .pack
            .manifest
            .contributes
            .authentication
            .as_ref()
            .map(ext::authentication_fingerprint),
        // The registry carries only the same dev project's account forward.
        authentication_session: None,
        dev: None,
        // Load-unpacked is the `dev` rung: never promotable, never verified.
        trust: grain_sdk::Trust::Dev,
    };
    let installed = reg.load_dev_if_current(record, loaded.root, revision);
    if let Some(prior) = prior.filter(|prior| {
        reg.record(&id)
            .is_none_or(|current| current.execution_generation != prior.execution_generation)
    }) {
        crate::extension_host::stop_extension_generation(
            &id,
            prior.execution_generation,
            "load-unpacked project replaced",
        );
        crate::grain_auth::cancel_extension_generation(&id, prior.execution_generation);
    }
    installed.map_err(|error| {
        #[cfg(feature = "agent-harness")]
        log::warn!("[agent-harness] developer registration failed: {error:#}");
        error.to_string()
    })?;
    Ok(id)
}

pub(crate) fn load_unpacked_project(
    app: &AppHandle,
    root: &std::path::Path,
) -> Result<String, String> {
    if !settings::get_settings(app).extension_developer_mode {
        return Err("Developer mode is disabled".into());
    }
    let loaded = crate::dev_extensions::load_project(root)?;
    let id = register_unpacked_project(app, loaded)?;
    if let Err(error) = crate::extension_icons::purge_dev_cache(app) {
        log::warn!("[GRAIN] developer icon-cache cleanup failed: {error}");
    }
    crate::extension_host::quarantine_invalid_packages(app);
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
    if !cfg!(feature = "agent-harness") {
        return Err(
            "Local extension packages are only available in the acceptance harness.".into(),
        );
    }
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
    stop_extension_runtime(&id, "load-unpacked project unloaded");
    reg.unload_dev(&id).map_err(|error| {
        #[cfg(feature = "agent-harness")]
        log::warn!("[agent-harness] developer unload failed: {error:#}");
        error.to_string()
    })?;
    if let Err(error) = crate::extension_icons::purge_dev_cache(&app) {
        log::warn!("[GRAIN] developer icon-cache cleanup failed: {error}");
    }
    crate::extension_host::quarantine_invalid_packages(&app);
    Ok(())
}

/// Import a `.grainpack` file (SPEC §1.1 tier A-inert). Validates, copies into
/// the extensions dir, registers it DISABLED — enabling is the user's explicit
/// second step in Overview, where toggle order is assigned.
#[tauri::command]
#[specta::specta]
pub fn extension_import_pack(
    app: AppHandle,
    window: tauri::WebviewWindow,
    path: String,
) -> Result<String, String> {
    use grain_core::extensions as ext;
    require_main_window(&window)?;
    if !cfg!(feature = "agent-harness") {
        return Err(
            "Local extension packages are only available in the acceptance harness.".into(),
        );
    }
    let raw = read_pack_input(std::path::Path::new(&path))?;
    let pack: grain_sdk::GrainPack =
        serde_json::from_slice(&raw).map_err(|e| format!("not a valid .grainpack: {e}"))?;
    pack.validate()?;
    let stored = serde_json::to_vec(&pack).map_err(|error| error.to_string())?;
    if stored.len() as u64 > grain_sdk::PACK_MAX_BYTES {
        return Err("validated .grainpack exceeds the storage limit".into());
    }
    // A declared embedded icon must fully decode before the import mutates the
    // registry. Only the current settings artwork is materialized.
    crate::extension_icons::materialize_pack(&app, &pack)?;

    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let id = pack.manifest.id.clone();
    let revision = reg.record_revision(&id);
    reg.with_current_record(&id, revision, |_| ())
        .map_err(|error| error.to_string())?;
    let ctx = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .ok_or("app context unavailable")?;
    let artifact_hash = grain_core::trust::sha256_hex(&stored);
    grain_core::install::publish_artifact(
        &ctx.data_dir.join("extensions"),
        &id,
        &pack.manifest.version,
        &artifact_hash,
        &stored,
    )
    .map_err(|error| error.to_string())?;
    // Re-import/update of an installed pack must PRESERVE the user's state
    // (SPEC §6 update row) — resetting enabled/toggle order on update would
    // silently disable a working pack.
    let dev_active = reg.dev_path(&id).is_some();
    let prior = reg.installed_record(&id);
    let was_enabled = prior.as_ref().map(|r| r.enabled).unwrap_or(false);
    let stays_enabled = imported_update_can_stay_enabled(prior.as_ref(), &pack);
    reg.install_if_current(
        ext::ExtensionRecord {
            id: id.clone(),
            // Re-import is an update, not an approval. Any newly requested grant or
            // changed action/auth digest holds the pack off
            // until the settings sheet has shown the new contract.
            enabled: stays_enabled,
            execution_generation: 0,
            toggle_seq: prior.as_ref().map(|r| r.toggle_seq).unwrap_or(0),
            installed_version: pack.manifest.version.clone(),
            artifact_sha256: Some(artifact_hash),
            granted: prior
                .as_ref()
                .map(|record| {
                    record
                        .granted
                        .iter()
                        .filter(|permission| pack.manifest.permissions.contains(permission))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
            // Carried, never recomputed. Importing is not approving: if the
            // declaration changed, this stops matching, the actions go inert, and
            // the enable path shows the user what is different.
            actions_approved: prior.as_ref().and_then(|r| r.actions_approved.clone()),
            authentication_approved: prior
                .as_ref()
                .and_then(|r| r.authentication_approved.clone()),
            authentication_session: prior
                .as_ref()
                .and_then(|r| r.authentication_session.clone()),
            dev: None,
            // A local import uses the untrusted fixture profile.
            trust: grain_sdk::Trust::UNTRUSTED_DEFAULT,
        },
        revision,
    )
    .map_err(|e| e.to_string())?;
    if was_enabled && !stays_enabled && !dev_active {
        if let Some(prior) = &prior {
            crate::extension_host::stop_extension_generation(
                &id,
                prior.execution_generation,
                "extension update requires renewed approval",
            );
            crate::grain_auth::cancel_extension_generation(&id, prior.execution_generation);
        }
    }
    crate::extension_host::quarantine_invalid_packages(&app);
    Ok(id)
}

/// Approve the reviewed tool permissions, declarations and account configuration,
/// then enable the extension in the same checked registry mutation.
///
/// Grants are clamped to what the manifest actually requests, so neither a
/// compromised frontend nor a stale sheet can widen an extension's reach beyond
/// what the user was shown.
///
/// Approve the exact package/registry identity shown by the permission sheet
/// and enable it in one checked mutation. A changed review needs a fresh sheet.
#[tauri::command]
#[specta::specta]
pub fn extension_grant(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    permissions: Vec<String>,
    approval_digest: String,
) -> Result<(), String> {
    use grain_core::extensions as ext;
    require_main_window(&window)?;
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let revision = reg.record_revision(&id);
    let rec = reg
        .record(&id)
        .ok_or_else(|| format!("'{id}' is not installed"))?;
    let pack = load_pack(&app, &id)?;
    let rec = reviewed_grant_record(rec, &pack, permissions, &approval_digest)?;
    let prior_generation = rec.execution_generation;
    let approved = reg.approve_and_enable_if_current(rec, revision);
    if reg
        .record(&id)
        .is_none_or(|current| current.execution_generation != prior_generation)
    {
        crate::extension_host::stop_extension_generation(
            &id,
            prior_generation,
            "extension permissions changed",
        );
        crate::grain_auth::cancel_extension_generation(&id, prior_generation);
    }
    crate::extension_host::quarantine_invalid_packages(&app);
    approved.map(|_| ()).map_err(|error| {
        #[cfg(feature = "agent-harness")]
        log::warn!("[agent-harness] developer approval failed: {error:#}");
        error.to_string()
    })
}

fn reviewed_grant_record(
    mut rec: grain_core::extensions::ExtensionRecord,
    pack: &grain_sdk::GrainPack,
    permissions: Vec<String>,
    approval_digest: &str,
) -> Result<grain_core::extensions::ExtensionRecord, String> {
    use grain_core::extensions as ext;
    pack.validate_tool_only()?;
    if pack.manifest.id != rec.id {
        return Err("Reviewed package identity does not match the installed extension.".into());
    }
    if ext::approval_fingerprint(&rec, pack).map_err(|error| error.to_string())? != approval_digest
    {
        return Err("The extension changed after review. Open a fresh permission sheet.".into());
    }
    let manifest = &pack.manifest;
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
    // Recomputed from disk, never taken from the caller — the fingerprint IS the
    // grant, so accepting one over the wire would let a caller approve a
    // declaration the user never saw.
    rec.actions_approved = (!manifest.contributes.actions.is_empty())
        .then(|| ext::actions_fingerprint(&manifest.contributes.actions));
    rec.authentication_approved = manifest
        .contributes
        .authentication
        .as_ref()
        .map(ext::authentication_fingerprint);
    if manifest
        .permissions
        .iter()
        .any(|permission| !rec.granted.contains(permission))
    {
        return Err("Review all requested permissions before enabling this extension.".into());
    }
    Ok(rec)
}

/// Uninstall a tool package and remove its scoped storage/authentication.
/// `purge` also clears derived artwork. Published immutable artifacts are never
/// deleted here: a concurrent installation may already be staging those bytes.
#[tauri::command]
#[specta::specta]
pub async fn extension_uninstall(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    purge: bool,
) -> Result<(), String> {
    use grain_core::extensions as ext;
    require_main_window(&window)?;
    // Grain's own features have no record to remove: they are turned off in
    // their own tab, never uninstalled. Everything else is a real installed pack
    // with a store to reinstall from.
    if id == ext::BUILTIN_SNIPPETS || id == ext::BUILTIN_CONTEXT || id == ext::BUILTIN_AGENT {
        return Err("built-in features can be turned off, not uninstalled".into());
    }
    let reg = app
        .try_state::<std::sync::Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let dev_active = reg.dev_path(&id).is_some();
    let removed = reg.uninstall_with_record(&id).map_err(|e| e.to_string())?;
    let old_generation = removed.as_ref().map(|record| record.execution_generation);
    let session = removed.and_then(|record| record.authentication_session);
    if old_generation.is_none() && !dev_active {
        return Err("not installed".into());
    }
    if let Some(generation) = old_generation {
        crate::grain_auth::cancel_extension_generation(&id, generation);
    }
    // Invalidate publication before blocking on the vault. Cleanup addresses
    // only the removed grant, never a replacement login or active dev account.
    let credential_cleanup = crate::grain_auth::purge_extension(&id, session);
    crate::extension_host::quarantine_invalid_packages(&app);
    if dev_active {
        if purge {
            crate::extension_icons::purge(&app, &id)?;
        }
        return credential_cleanup.await;
    }
    if let Some(ctx) = app.try_state::<std::sync::Arc<grain_core::AppContext>>() {
        crate::host_api::ExtStorage::new(&ctx.data_dir, &id)
            .purge()
            .map_err(|error| error.to_string())?;
    }
    if let Some(generation) = old_generation {
        crate::extension_host::stop_extension_generation(&id, generation, "extension uninstalled");
    }
    if purge {
        crate::extension_icons::purge(&app, &id)?;
    }
    crate::extension_host::quarantine_invalid_packages(&app);
    credential_cleanup.await
}
