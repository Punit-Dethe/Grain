//! [`AppContext`] — the headless heart every part of Grain shares.
//!
//! Replaces the four ways Handy reached through `tauri::AppHandle`:
//! - `app.emit(...)`          → [`AppContext::emit`] over a broadcast channel
//! - `app.state::<T>()`       → managers hold `Arc<AppContext>` + each other
//! - `get_settings(&app)`     → [`AppContext::settings`] (owned `RwLock`)
//! - `app.path().resolve(..)` → [`AppContext::resource_dir`] / [`data_dir`]
//!
//! Settings persist as our own JSON (no `tauri-plugin-store`); API keys go to a
//! SEPARATE credential file so the main settings file never holds secrets.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use tokio::sync::broadcast;

use crate::event::DaemonEvent;
use crate::settings::{ensure_post_process_defaults, AppSettings, SecretMap};

const SETTINGS_FILE: &str = "grain.settings.json";
const SECRETS_FILE: &str = "grain.secrets.json";
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Shared, headless application context. Cheaply cloneable via `Arc`.
pub struct AppContext {
    settings: RwLock<AppSettings>,
    event_tx: broadcast::Sender<DaemonEvent>,
    /// Bundled, read-only assets (VAD model, feedback sounds).
    pub resource_dir: PathBuf,
    /// User data (settings, secrets, downloaded models, history DB).
    pub data_dir: PathBuf,
}

impl AppContext {
    /// Build a context, loading persisted settings from `data_dir` (falling back
    /// to defaults if absent or unreadable).
    pub fn new(resource_dir: impl Into<PathBuf>, data_dir: impl Into<PathBuf>) -> Arc<Self> {
        let resource_dir = resource_dir.into();
        let data_dir = data_dir.into();
        let settings = load_settings(&data_dir).unwrap_or_else(|e| {
            log::warn!("settings load failed ({e:#}); using defaults");
            AppSettings::default()
        });
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Arc::new(Self {
            settings: RwLock::new(settings),
            event_tx,
            resource_dir,
            data_dir,
        })
    }

    // -- settings ----------------------------------------------------------

    /// A clone of the current settings (the headless replacement for
    /// `get_settings(&app)`).
    pub fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .expect("settings lock poisoned")
            .clone()
    }

    /// Read a value out of settings under a shared lock without cloning the whole
    /// struct.
    pub fn with_settings<R>(&self, f: impl FnOnce(&AppSettings) -> R) -> R {
        f(&self.settings.read().expect("settings lock poisoned"))
    }

    /// Mutate settings under the write lock, then persist to disk.
    pub fn update_settings<R>(&self, f: impl FnOnce(&mut AppSettings) -> R) -> Result<R> {
        let (ret, snapshot) = {
            let mut guard = self.settings.write().expect("settings lock poisoned");
            let ret = f(&mut guard);
            (ret, guard.clone())
        };
        save_settings(&self.data_dir, &snapshot)?;
        Ok(ret)
    }

    /// Replace settings wholesale and persist (the headless `write_settings`).
    pub fn replace_settings(&self, settings: AppSettings) -> Result<()> {
        {
            let mut guard = self.settings.write().expect("settings lock poisoned");
            *guard = settings;
        }
        save_settings(&self.data_dir, &self.settings())
    }

    // -- events ------------------------------------------------------------

    /// Broadcast an event to all subscribers. Errs only when there are none,
    /// which is not a failure — ignored.
    pub fn emit(&self, event: DaemonEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Subscribe to the event stream (pill, settings window, server).
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    /// The sender, for components that need to clone it.
    pub fn event_sender(&self) -> broadcast::Sender<DaemonEvent> {
        self.event_tx.clone()
    }
}

fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SETTINGS_FILE)
}
fn secrets_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SECRETS_FILE)
}

/// Load settings + merge in the separately-stored secrets, applying provider
/// migrations. Returns defaults (persisted) when no settings file exists yet.
fn load_settings(data_dir: &Path) -> Result<AppSettings> {
    let path = settings_path(data_dir);
    let mut salvaged = false;
    let mut settings = if path.exists() {
        let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        match serde_json::from_str::<AppSettings>(&raw) {
            Ok(settings) => settings,
            Err(e) => {
                // One bad field must not reset the user's whole configuration
                // (upstream #1619). Rebuild from the raw JSON, keeping every
                // individually-valid field. If the file isn't even JSON, err
                // out to the caller's defaults fallback.
                log::warn!("Failed to parse stored settings ({e}); salvaging valid fields");
                let value: serde_json::Value = serde_json::from_str(&raw)
                    .with_context(|| format!("parse {}", path.display()))?;
                salvaged = true;
                salvage_settings(&value)
            }
        }
    } else {
        let defaults = AppSettings::default();
        save_settings(data_dir, &defaults)?;
        defaults
    };

    // Merge secrets from the separate credential file.
    let secrets = load_secrets(data_dir)?;
    for (id, key) in secrets.post_process_api_keys.0 {
        settings.post_process_api_keys.insert(id, key);
    }
    for (id, key) in secrets.stt_api_keys.0 {
        settings.stt_api_keys.insert(id, key);
    }

    // Provider/key migrations. A salvaged store is persisted here too — only
    // after the secrets merge, since save_settings rewrites the credential
    // file from the in-memory settings.
    if ensure_post_process_defaults(&mut settings) || salvaged {
        save_settings(data_dir, &settings)?;
    }
    Ok(settings)
}

/// Rebuilds settings from a store value that failed to deserialize as a whole.
/// Every stored field that is individually valid is kept; only broken values
/// (e.g. an enum variant written by a newer or older version) fall back to
/// their default. This means one bad field can never reset the rest of the
/// user's configuration (upstream #1619).
fn salvage_settings(stored: &serde_json::Value) -> AppSettings {
    let Some(stored_map) = stored.as_object() else {
        log::warn!("Stored settings are not a JSON object; falling back to defaults");
        return AppSettings::default();
    };

    let mut merged = serde_json::to_value(AppSettings::default())
        .expect("default settings serialize to a JSON object");

    for (key, value) in stored_map {
        let previous = merged
            .as_object_mut()
            .expect("merged settings stay an object")
            .insert(key.clone(), value.clone());
        if serde_json::from_value::<AppSettings>(merged.clone()).is_err() {
            // Log only the key: values may hold secrets (e.g. API keys).
            log::warn!("Dropping invalid settings field '{key}', keeping its default");
            let map = merged
                .as_object_mut()
                .expect("merged settings stay an object");
            match previous {
                Some(previous) => map.insert(key.clone(), previous),
                None => map.remove(key),
            };
        }
    }

    serde_json::from_value(merged).unwrap_or_else(|e| {
        log::warn!("Failed to reassemble salvaged settings ({e}); falling back to defaults");
        AppSettings::default()
    })
}

/// On-disk shape of `grain.secrets.json`: one named sub-map per key-bearing
/// layer. Legacy files were a bare `{id: key}` map of post-process keys; those
/// are migrated transparently in [`load_secrets`].
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct StoredSecrets {
    #[serde(default)]
    post_process_api_keys: SecretMap,
    #[serde(default)]
    stt_api_keys: SecretMap,
}

fn load_secrets(data_dir: &Path) -> Result<StoredSecrets> {
    let path = secrets_path(data_dir);
    if !path.exists() {
        return Ok(StoredSecrets::default());
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    // New format has the named sub-maps; anything else is a legacy bare map of
    // post-process keys.
    if value.get("post_process_api_keys").is_some() || value.get("stt_api_keys").is_some() {
        serde_json::from_value(value).with_context(|| format!("parse {}", path.display()))
    } else {
        let legacy: SecretMap = serde_json::from_value(value)
            .with_context(|| format!("parse legacy {}", path.display()))?;
        Ok(StoredSecrets {
            post_process_api_keys: legacy,
            stt_api_keys: SecretMap::default(),
        })
    }
}

/// Persist settings to JSON and secrets to the separate credential file. The
/// main settings file is written with secrets stripped, so keys never land in it.
fn save_settings(data_dir: &Path, settings: &AppSettings) -> Result<()> {
    fs::create_dir_all(data_dir)
        .with_context(|| format!("create data dir {}", data_dir.display()))?;

    // Split secrets out of the persisted settings.
    let mut sanitized = settings.clone();
    let secrets = StoredSecrets {
        post_process_api_keys: std::mem::take(&mut sanitized.post_process_api_keys),
        stt_api_keys: std::mem::take(&mut sanitized.stt_api_keys),
    };

    let settings_json = serde_json::to_string_pretty(&sanitized)?;
    write_atomic(&settings_path(data_dir), settings_json.as_bytes())?;

    let secrets_json = serde_json::to_string_pretty(&secrets)?;
    write_atomic(&secrets_path(data_dir), secrets_json.as_bytes())?;
    Ok(())
}

/// Write via a temp file + rename so a crash mid-write can't corrupt the file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("rename into {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (Arc<AppContext>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = AppContext::new(dir.path().join("res"), dir.path().join("data"));
        (ctx, dir)
    }

    #[test]
    fn defaults_persisted_on_first_load() {
        let (ctx, dir) = ctx();
        assert!(dir.path().join("data").join(SETTINGS_FILE).exists());
        // Known Grain defaults: push-to-talk ships OFF (toggle-style capture),
        // update checks ship ON.
        assert!(!ctx.settings().push_to_talk);
        assert!(ctx.settings().update_checks_enabled);
    }

    /// Every field must survive a partial store: a missing key must never fail
    /// the whole-settings parse (upstream #1619). `{}` is the extreme case.
    #[test]
    fn empty_settings_object_parses_with_defaults() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({}))
            .expect("all AppSettings fields need serde defaults");
        assert_eq!(settings.selected_model, AppSettings::default().selected_model);
    }

    #[test]
    fn salvage_preserves_valid_fields_when_one_value_is_invalid() {
        let mut stored = serde_json::to_value(AppSettings::default()).unwrap();
        let map = stored.as_object_mut().unwrap();
        map.insert(
            "selected_model".into(),
            serde_json::json!("parakeet-tdt-0.6b-v3"),
        );
        map.insert("custom_words".into(), serde_json::json!(["grain"]));
        // An enum variant this build doesn't know, e.g. written by a newer
        // version before a downgrade.
        map.insert("sound_theme".into(), serde_json::json!("theremin"));

        // Precondition: exactly the whole-store parse failure that used to
        // reset everything to defaults.
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "parakeet-tdt-0.6b-v3");
        assert_eq!(salvaged.custom_words, vec!["grain".to_string()]);
    }

    #[test]
    fn salvage_drops_only_wrong_typed_fields() {
        let mut stored = serde_json::to_value(AppSettings::default()).unwrap();
        let map = stored.as_object_mut().unwrap();
        map.insert("paste_delay_ms".into(), serde_json::json!("sixty"));
        map.insert("custom_words".into(), serde_json::json!(["grain"]));

        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.paste_delay_ms, AppSettings::default().paste_delay_ms);
        assert_eq!(salvaged.custom_words, vec!["grain".to_string()]);
    }

    /// A corrupt settings file must not take the credential file down with it:
    /// the salvage-triggered persist runs only after the secrets merge.
    #[test]
    fn corrupt_settings_salvage_keeps_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        fs::create_dir_all(&data).unwrap();

        let mut stored = serde_json::to_value(AppSettings::default()).unwrap();
        stored
            .as_object_mut()
            .unwrap()
            .insert("sound_theme".into(), serde_json::json!(42));
        fs::write(
            data.join(SETTINGS_FILE),
            serde_json::to_string(&stored).unwrap(),
        )
        .unwrap();
        fs::write(
            data.join(SECRETS_FILE),
            r#"{"post_process_api_keys":{"openai":"sk-keepme"},"stt_api_keys":{}}"#,
        )
        .unwrap();

        let ctx = AppContext::new(dir.path().join("res"), &data);
        assert_eq!(
            ctx.settings()
                .post_process_api_keys
                .get("openai")
                .map(String::as_str),
            Some("sk-keepme")
        );
        let secrets_raw = fs::read_to_string(data.join(SECRETS_FILE)).unwrap();
        assert!(
            secrets_raw.contains("sk-keepme"),
            "salvage persist wiped the credential file"
        );
    }

    #[test]
    fn settings_round_trip_across_reload() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        {
            let ctx = AppContext::new("res", &data);
            ctx.update_settings(|s| s.selected_model = "parakeet-v3".into())
                .unwrap();
        }
        // Fresh context reads the same value back from disk.
        let ctx2 = AppContext::new("res", &data);
        assert_eq!(ctx2.settings().selected_model, "parakeet-v3");
    }

    #[test]
    fn secrets_go_to_separate_file_not_settings() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let ctx = AppContext::new("res", &data);
        ctx.update_settings(|s| {
            s.post_process_api_keys
                .insert("openai".into(), "sk-supersecret".into());
        })
        .unwrap();

        let settings_raw = fs::read_to_string(data.join(SETTINGS_FILE)).unwrap();
        let secrets_raw = fs::read_to_string(data.join(SECRETS_FILE)).unwrap();
        assert!(
            !settings_raw.contains("sk-supersecret"),
            "secret leaked into settings file"
        );
        assert!(
            secrets_raw.contains("sk-supersecret"),
            "secret missing from credential file"
        );

        // And it's transparently merged back on reload.
        let ctx2 = AppContext::new("res", &data);
        assert_eq!(
            ctx2.settings()
                .post_process_api_keys
                .get("openai")
                .map(String::as_str),
            Some("sk-supersecret")
        );
    }

    #[test]
    fn events_broadcast_to_subscribers() {
        let (ctx, _dir) = ctx();
        let mut rx = ctx.subscribe();
        ctx.emit(DaemonEvent::ModelLoaded {
            model_id: "parakeet".into(),
        });
        match rx.try_recv() {
            Ok(DaemonEvent::ModelLoaded { model_id }) => assert_eq!(model_id, "parakeet"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
