//! Fixed native-account fixture controls. Compiled only into the debug harness.
//! No token export, arbitrary IDs/paths/endpoints, or declaration approval bypass.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager, WebviewWindow};

pub(crate) const FIXTURE_ID: &str = "com.grain.harness.auth";
pub(crate) const PEER_ID: &str = "com.grain.harness.auth-peer";
static AUTHORIZATION: Mutex<Option<(uuid::Uuid, String)>> = Mutex::new(None);
static PEER_AUTHORIZATION: Mutex<Option<(uuid::Uuid, String)>> = Mutex::new(None);

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    #[default]
    Primary,
    Peer,
}
impl Target {
    fn id(self) -> &'static str {
        match self {
            Self::Primary => FIXTURE_ID,
            Self::Peer => PEER_ID,
        }
    }
    fn handoff(self) -> &'static Mutex<Option<(uuid::Uuid, String)>> {
        match self {
            Self::Primary => &AUTHORIZATION,
            Self::Peer => &PEER_AUTHORIZATION,
        }
    }
}
fn target_for_id(id: &str) -> Option<Target> {
    match id {
        FIXTURE_ID => Some(Target::Primary),
        PEER_ID => Some(Target::Peer),
        _ => None,
    }
}

fn owned_path(name: &str) -> Result<PathBuf, String> {
    let (root, _) = super::grain_agent_harness::auth_fixture_config()?;
    let path = root
        .join(name)
        .canonicalize()
        .map_err(|_| "Auth fixture file is missing")?;
    if !path.starts_with(&root) {
        return Err("Auth fixture escaped its owned root".into());
    }
    Ok(path)
}

fn check_origin(raw: &str, paths: &[&str]) -> Result<(), String> {
    let (_, port) = super::grain_agent_harness::auth_fixture_config()?;
    validate_url(raw, port, paths)
}

fn validate_url(raw: &str, port: u16, paths: &[&str]) -> Result<(), String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "Invalid fixture URL")?;
    if url.scheme() != "https"
        || url.host_str() != Some("127.0.0.1")
        || url.port() != Some(port)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !paths.contains(&url.path())
    {
        return Err("Auth fixture requires its exact HTTPS origin/path".into());
    }
    Ok(())
}

/// Trust an owned test certificate only for the marker's exact HTTPS token URL.
/// All other production auth requests retain the default trust store.
pub(crate) fn scoped_tls(
    builder: reqwest::ClientBuilder,
    endpoint: &str,
) -> Result<reqwest::ClientBuilder, String> {
    let Ok((_, port)) = super::grain_agent_harness::auth_fixture_config() else {
        return Ok(builder);
    };
    let url = reqwest::Url::parse(endpoint).map_err(|_| "Invalid token endpoint")?;
    if url.host_str() != Some("127.0.0.1") {
        return Ok(builder);
    }
    validate_url(
        endpoint,
        port,
        &["/token", "/token-v2", "/me", "/peer/token", "/peer/me"],
    )?;
    if url.query().is_some() {
        return Err("Fixture token/API URL cannot have a query".into());
    }
    let path = owned_path("auth-tls/cert.pem")?;
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "Cannot open test certificate")?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read test certificate")?;
    if bytes.len() > 8192 {
        return Err("Test certificate is oversized".into());
    }
    let cert = reqwest::Certificate::from_pem(&bytes).map_err(|_| "Invalid test certificate")?;
    Ok(builder.add_root_certificate(cert).no_proxy())
}

pub(crate) fn api_client(id: &str, url: &str) -> Result<reqwest::Client, String> {
    check_origin(
        url,
        match target_for_id(id) {
            Some(Target::Primary) => &["/me"],
            Some(Target::Peer) => &["/peer/me"],
            None => return Err("Unknown account fixture".into()),
        },
    )?;
    scoped_tls(
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20)),
        url,
    )?
    .build()
    .map_err(|_| "Cannot create scoped fixture HTTP client".into())
}

/// Replace only the external browser handoff, not URL/PKCE/callback/exchange.
/// The runner consumes this URL privately; it never enters status or evidence.
pub(crate) fn capture_authorization(
    id: &str,
    owner: uuid::Uuid,
    url: &str,
) -> Result<bool, String> {
    let Some(target) = target_for_id(id) else {
        return Ok(false);
    };
    check_origin(
        url,
        match target {
            Target::Primary => &["/authorize"],
            Target::Peer => &["/peer/authorize"],
        },
    )?;
    let mut slot = target
        .handoff()
        .lock()
        .map_err(|_| "Fixture consent state unavailable")?;
    *slot = Some((owner, url.into()));
    Ok(true)
}

fn release_if_current(slot: &mut Option<(uuid::Uuid, String)>, owner: uuid::Uuid) {
    if slot.as_ref().is_some_and(|(current, _)| *current == owner) {
        *slot = None;
    }
}

/// A dropped/cancelled flow may clear only its own unconsumed handoff.
pub(crate) fn finish_authorization(id: &str, owner: uuid::Uuid) {
    if let Some(target) = target_for_id(id) {
        if let Ok(mut slot) = target.handoff().lock() {
            release_if_current(&mut slot, owner);
        }
    }
}

fn validate_pack(pack: &grain_sdk::GrainPack, target: Target) -> Result<(), String> {
    pack.validate()?;
    let m = &pack.manifest;
    if m.id != target.id()
        || (m.permissions != ["auth", "net:127.0.0.1"]
            && m.permissions != ["auth", "net:127.0.0.1", "net:localhost"])
        || m.contributes.actions.len() != 1
        || m.contributes.actions[0].id != "account_read"
    {
        return Err("Harness only admits its fixed scoped account fixture".into());
    }
    let d = m
        .contributes
        .authentication
        .as_ref()
        .ok_or("Auth fixture declaration missing")?;
    if matches!(target, Target::Peer) {
        if m.permissions != ["auth", "net:127.0.0.1"]
            || d.provider_name != "Harness Peer OAuth"
            || d.client_id != "grain-harness-peer"
            || d.scopes != ["fixture.peer.read"]
            || d.api_hosts != ["127.0.0.1"]
            || !d.authorization_parameters.is_empty()
        {
            return Err("Unexpected peer account fixture declaration".into());
        }
        check_origin(&d.authorization_endpoint, &["/peer/authorize"])?;
        check_origin(&d.token_endpoint, &["/peer/token"])?;
        return Ok(());
    }
    if d.provider_name != "Harness OAuth"
        || !["grain-harness-public", "grain-harness-public-v2"].contains(&d.client_id.as_str())
        || (d.scopes != ["fixture.read"] && d.scopes != ["fixture.read", "fixture.extra"])
        || (d.api_hosts != ["127.0.0.1"] && d.api_hosts != ["127.0.0.1", "localhost"])
        || !d.authorization_parameters.is_empty()
    {
        return Err("Unexpected account fixture declaration".into());
    }
    if (d.api_hosts.len() == 2) != (m.permissions.len() == 3) {
        return Err("Fixture API hosts require their exact paired permissions".into());
    }
    check_origin(&d.authorization_endpoint, &["/authorize"])?;
    check_origin(&d.token_endpoint, &["/token", "/token-v2"])?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Import,
    Status,
    Connect,
    Authorization,
    Disconnect,
    Remove,
    LoadA,
    LoadB,
    Unload,
    Enable,
    RemoveInstalled,
    SeedUnbound,
    SeedLegacy,
    Cancel,
    Refresh,
}

#[tauri::command]
pub async fn agent_harness_auth(
    app: AppHandle,
    window: WebviewWindow,
    operation: Operation,
    target: Option<Target>,
) -> Result<Value, String> {
    super::grain_agent_harness::guard(&app, &window)?;
    super::grain_agent_harness::auth_fixture_config()?;
    let target = target.unwrap_or_default();
    let id = target.id();
    if matches!(target, Target::Peer)
        && matches!(
            operation,
            Operation::LoadA
                | Operation::LoadB
                | Operation::Unload
                | Operation::SeedUnbound
                | Operation::SeedLegacy
                | Operation::RemoveInstalled
                | Operation::Refresh
        )
    {
        return Err("Peer fixture supports installed-account operations only".into());
    }
    match operation {
        Operation::Refresh => {
            // Fixed auth-only concurrency probe: execute the production refresh
            // path for its current approved registry owner, then erase the token.
            // No tool dispatch, generation input or credential export is added.
            let reg = app.state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>();
            let generation = reg
                .record(id)
                .ok_or("Account fixture missing")?
                .execution_generation;
            drop(crate::grain_auth::access_token(&app, id, "127.0.0.1", generation).await?);
            Ok(Value::Null)
        }
        Operation::Import => {
            let path = owned_path(match target {
                Target::Primary => "auth-fixture.grainpack",
                Target::Peer => "auth-peer.grainpack",
            })?;
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(&path)
                .map_err(|_| "Auth package missing")?
                .take(grain_sdk::PACK_MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Cannot read auth package")?;
            if bytes.len() as u64 > grain_sdk::PACK_MAX_BYTES {
                return Err("Auth package oversized".into());
            }
            let pack: grain_sdk::GrainPack =
                serde_json::from_slice(&bytes).map_err(|_| "Invalid auth package")?;
            validate_pack(&pack, target)?;
            crate::grain_commands::extension_import_pack(
                app,
                window,
                path.to_string_lossy().into_owned(),
            )?;
            Ok(Value::Null)
        }
        Operation::Status => {
            let reg = app.state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>();
            let record = reg.record(id);
            let connection =
                crate::grain_auth::extension_auth_connection(app.clone(), window, id.into())
                    .await?;
            let (root, _) = super::grain_agent_harness::auth_fixture_config()?;
            let owner = record.as_ref().map(|record| match &record.dev {
                None => "installed",
                Some(dev) if dev.path == root.join("auth-fixture-a") => "developer-a",
                Some(dev) if dev.path == root.join("auth-fixture-b") => "developer-b",
                Some(_) => "unexpected",
            });
            Ok(json!({"enabled": record.as_ref().is_some_and(|r|r.enabled),
                "installed": reg.installed_record(id).is_some(),
                "owner": owner,
                "connection": connection,
                "worker": crate::extension_host::harness_snapshot(id)}))
        }
        Operation::Connect => {
            *target
                .handoff()
                .lock()
                .map_err(|_| "Fixture consent state unavailable")? = None;
            let result = crate::grain_auth::extension_auth_connect(app, window, id.into()).await;
            result.and_then(|value| {
                serde_json::to_value(value).map_err(|_| "Cannot encode auth status".into())
            })
        }
        Operation::Authorization => Ok(target
            .handoff()
            .lock()
            .map_err(|_| "Fixture consent state unavailable")?
            .take()
            .map_or(Value::Null, |(_, url)| Value::String(url))),
        Operation::Disconnect => {
            crate::grain_auth::extension_auth_disconnect(app, window, id.into()).await?;
            Ok(Value::Null)
        }
        Operation::Cancel => {
            // Cancel only the fixed fixture's pending production flow; retain
            // its previously selected account, as on normal owner teardown.
            crate::grain_auth::cancel_extension(id);
            Ok(Value::Null)
        }
        Operation::Remove => {
            crate::grain_auth::cancel_extension(id);
            *target
                .handoff()
                .lock()
                .map_err(|_| "Fixture consent state unavailable")? = None;
            let reg = app.state::<std::sync::Arc<grain_core::extensions::ExtensionsRegistry>>();
            if reg.dev_path(id).is_some() {
                crate::grain_commands::extension_unload_dev(
                    app.clone(),
                    window.clone(),
                    id.into(),
                )?;
            }
            if reg.installed_record(id).is_some() {
                crate::grain_commands::extension_uninstall(app, window, id.into(), true).await?;
            }
            Ok(Value::Null)
        }
        Operation::LoadA | Operation::LoadB => {
            let root = owned_path(if matches!(operation, Operation::LoadA) {
                "auth-fixture-a"
            } else {
                "auth-fixture-b"
            })?;
            let loaded = crate::dev_extensions::load_project(&root)?;
            validate_pack(&loaded.pack, target)?;
            crate::grain_commands::load_unpacked_project(&app, &root)?;
            Ok(Value::Null)
        }
        Operation::Unload => {
            crate::grain_commands::extension_unload_dev(app, window, id.into())?;
            Ok(Value::Null)
        }
        Operation::Enable => {
            crate::grain_commands::extension_set_enabled(app, window, id.into(), true)?;
            Ok(Value::Null)
        }
        Operation::RemoveInstalled => {
            crate::grain_commands::extension_uninstall(app, window, id.into(), false).await?;
            Ok(Value::Null)
        }
        Operation::SeedUnbound | Operation::SeedLegacy => {
            if crate::agent::harness_snapshot(&app)["active"].as_bool() != Some(false) {
                return Err("Legacy credential fixture requires an idle Agent".into());
            }
            crate::grain_auth::harness_legacy_account(
                &app,
                matches!(operation, Operation::SeedLegacy),
            )
            .await?;
            Ok(Value::Null)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_targets_are_finite_and_handoffs_are_independent() {
        assert!(matches!(
            serde_json::from_str::<Target>("\"primary\""),
            Ok(Target::Primary)
        ));
        assert!(matches!(
            serde_json::from_str::<Target>("\"peer\""),
            Ok(Target::Peer)
        ));
        assert!(serde_json::from_str::<Target>("\"com.example.other\"").is_err());
        assert!(target_for_id("com.grain.harness.auth-peer-extra").is_none());
        assert!(!std::ptr::eq(
            Target::Primary.handoff(),
            Target::Peer.handoff()
        ));
        assert!(validate_url("https://127.0.0.1:32100/peer/me", 32100, &["/peer/me"]).is_ok());
        assert!(validate_url("https://127.0.0.1:32100/me", 32100, &["/peer/me"]).is_err());
    }
    #[test]
    fn old_consent_cleanup_cannot_clear_a_replacement() {
        let old = uuid::Uuid::new_v4();
        let current = uuid::Uuid::new_v4();
        let mut slot = Some((current, "owned URL".into()));
        release_if_current(&mut slot, old);
        assert!(slot.is_some());
        release_if_current(&mut slot, current);
        assert!(slot.is_none());
    }
    #[test]
    fn scoped_origin_refuses_other_ports_hosts_schemes_and_paths() {
        assert!(validate_url("https://127.0.0.1:32100/token", 32100, &["/token"]).is_ok());
        for raw in [
            "http://127.0.0.1:32100/token",
            "https://127.0.0.1:32101/token",
            "https://localhost:32100/token",
            "https://user@127.0.0.1:32100/token",
            "https://127.0.0.1:32100/other",
            "https://127.0.0.1:32100/token#fragment",
        ] {
            assert!(validate_url(raw, 32100, &["/token"]).is_err());
        }
    }
}
