//! [GRAIN] The host API router (SPEC §1.3) — Phase 2.
//!
//! Every method an extension worker can call, each behind a **capability check
//! first** (return a "not granted" error, never partial data). The check reads
//! the connection's [`ClientIdentity`] — resolved server-side from its token —
//! so a worker can never reach a method it wasn't granted, regardless of what
//! its JS does.
//!
//! Testability: the security-critical pieces are pure and unit-tested here
//! ([`has_capability`], [`required_capability`], [`ExtStorage`]). [`dispatch`]
//! itself is a thin router that needs `AppHandle` state and is exercised at
//! integration level.
//!
//! Rate-limiting is enforced at the per-connection frame-read boundary in
//! `events_server` (where the connection lives), not here — a pure dispatcher
//! has no connection to meter.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::events_auth::{CapabilitySet, ClientIdentity};

/// Per-extension storage quota (SPEC §3.4). 200 MB, generous for KV + docs.
const STORAGE_QUOTA_BYTES: u64 = 200 * 1024 * 1024;

/// Reserved key inside the storage file for the extension's own settings
/// namespace (`ext.<id>.*`). There is no path to `AppSettings` — this is the
/// entire "settings" surface an extension has (SPEC §4.2).
const SETTINGS_KEY: &str = "__settings";

/// Does this identity hold `cap`? `All` (the pill) holds everything; a worker
/// holds exactly its granted `Named` set.
pub fn has_capability(identity: &ClientIdentity, cap: &str) -> bool {
    match &identity.caps {
        CapabilitySet::All => true,
        CapabilitySet::Named(set) => set.contains(cap),
    }
}

/// The capability a method requires, or `None` for always-allowed methods
/// (`log.*`). Pure, so the gating table is testable without app state.
pub fn required_capability(method: &str) -> Option<&'static str> {
    match method {
        "log.info" | "log.warn" => None,
        "storage.get" | "storage.set" | "storage.delete" => Some("storage"),
        "settings.get" | "settings.set" => Some("settings"),
        "llm.complete" => Some("llm"),
        "embed" => Some("embed"),
        "session.start" => Some("session:start"),
        _ => Some("__unknown__"), // unknown methods map to an ungrantable cap
    }
}

/// A per-extension JSON key/value store: `<data>/extensions/<id>.storage.json`.
/// Loaded and saved whole (fine at this scale). Pure over its path, so quota
/// and round-trip behavior are unit-tested directly.
pub struct ExtStorage {
    path: PathBuf,
}

impl ExtStorage {
    pub fn new(data_dir: &std::path::Path, ext_id: &str) -> Self {
        Self {
            path: data_dir
                .join("extensions")
                .join(format!("{ext_id}.storage.json")),
        }
    }

    fn load(&self) -> BTreeMap<String, Value> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn save(&self, map: &BTreeMap<String, Value>) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string(map).map_err(|e| e.to_string())?;
        if json.len() as u64 > STORAGE_QUOTA_BYTES {
            return Err(format!(
                "storage quota exceeded ({} MB max)",
                STORAGE_QUOTA_BYTES / (1024 * 1024)
            ));
        }
        std::fs::write(&self.path, json).map_err(|e| e.to_string())
    }

    pub fn get(&self, key: &str) -> Value {
        self.load().get(key).cloned().unwrap_or(Value::Null)
    }

    pub fn set(&self, key: &str, value: Value) -> Result<(), String> {
        let mut map = self.load();
        map.insert(key.to_string(), value);
        self.save(&map)
    }

    pub fn delete(&self, key: &str) -> Result<(), String> {
        let mut map = self.load();
        map.remove(key);
        self.save(&map)
    }

    /// The extension's own settings namespace (a nested object under one
    /// reserved key — never `AppSettings`).
    ///
    /// Public because the host's settings controls write here too: one store,
    /// one way in, so the schema cannot be enforced on one path and not the
    /// other.
    pub fn settings_get(&self, key: &str) -> Value {
        self.get(SETTINGS_KEY)
            .get(key)
            .cloned()
            .unwrap_or(Value::Null)
    }

    pub fn settings_set(&self, key: &str, value: Value) -> Result<(), String> {
        let mut ns = match self.get(SETTINGS_KEY) {
            Value::Object(m) => m,
            _ => serde_json::Map::new(),
        };
        ns.insert(key.to_string(), value);
        self.set(SETTINGS_KEY, Value::Object(ns))
    }
}

fn param_str(params: &Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing string param '{key}'"))
}

/// Route one worker API call. Capability check FIRST; on failure return the
/// error and touch nothing.
pub async fn dispatch(
    app: &AppHandle,
    identity: &ClientIdentity,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    if let Some(cap) = required_capability(method) {
        if !has_capability(identity, cap) {
            return Err(format!("capability '{cap}' not granted"));
        }
    }

    let data_dir = app
        .try_state::<std::sync::Arc<grain_core::AppContext>>()
        .map(|c| c.data_dir.clone())
        .ok_or("app context unavailable")?;
    let store = ExtStorage::new(&data_dir, &identity.id);

    match method {
        "log.info" => {
            log::info!("[ext:{}] {}", identity.id, param_str(&params, "msg")?);
            Ok(Value::Null)
        }
        "log.warn" => {
            log::warn!("[ext:{}] {}", identity.id, param_str(&params, "msg")?);
            Ok(Value::Null)
        }
        "storage.get" => Ok(store.get(&param_str(&params, "key")?)),
        "storage.set" => {
            let key = param_str(&params, "key")?;
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            store.set(&key, value)?;
            Ok(Value::Null)
        }
        "storage.delete" => {
            store.delete(&param_str(&params, "key")?)?;
            Ok(Value::Null)
        }
        "settings.get" => {
            let key = param_str(&params, "key")?;
            let stored = store.settings_get(&key);
            // A declared setting reads back through the schema, so a value left
            // behind by an older version resolves to something the extension's
            // own declaration says is legal.
            Ok(match crate::grain_commands::setting_decl(app, &identity.id, &key) {
                Some(decl) => grain_sdk::settings_schema::resolve(&decl, Some(&stored)).value,
                None => stored,
            })
        }
        "settings.set" => {
            let key = param_str(&params, "key")?;
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            // [GRAIN] SPEC §4.1: the schema is enforced HERE, not only in the
            // settings form — this is the same namespace the host's own
            // controls write to, and the extension can reach it directly. A
            // schema policed only in React is not policed at all.
            //
            // An *undeclared* key is free-form on purpose: the schema says what
            // the host renders, not everything the extension may remember.
            let value = match crate::grain_commands::setting_decl(app, &identity.id, &key) {
                Some(decl) => grain_sdk::settings_schema::coerce(&decl, &value)
                    .map_err(|e| format!("'{key}': {e}"))?
                    .value,
                None => value,
            };
            store.settings_set(&key, value)?;
            Ok(Value::Null)
        }
        "llm.complete" => {
            let prompt = param_str(&params, "prompt")?;
            let text = crate::grain_post_process::complete_for_extension(app, &prompt).await?;
            Ok(json!({ "text": text }))
        }
        "embed" => {
            // Reserved (SPEC): the grain_space embedder isn't exposed as a
            // shared host call yet. Fails cleanly rather than half-working.
            Err("embed is not available in this version".into())
        }
        "session.start" => {
            // Structural capability reserved + plumbed (guide step 7): the name
            // exists in the vocabulary and the router from day one, so a
            // Phase-3 extension never discovers the gap. Returns a clear
            // unimplemented until the coordinator wiring lands.
            Err("session.start is not implemented yet".into())
        }
        other => Err(format!("unknown method '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn named(caps: &[&str]) -> ClientIdentity {
        ClientIdentity {
            id: "com.example.a".into(),
            caps: CapabilitySet::Named(caps.iter().map(|s| s.to_string()).collect::<HashSet<_>>()),
        }
    }

    #[test]
    fn capability_gate_is_pure_and_correct() {
        let ext = named(&["storage"]);
        assert!(has_capability(&ext, "storage"));
        assert!(!has_capability(&ext, "llm"));

        // Method → capability mapping.
        assert_eq!(required_capability("storage.set"), Some("storage"));
        assert_eq!(required_capability("llm.complete"), Some("llm"));
        assert_eq!(required_capability("session.start"), Some("session:start"));
        assert_eq!(required_capability("log.info"), None);
        // Unknown methods require an ungrantable capability → always denied.
        assert_eq!(required_capability("os.exec"), Some("__unknown__"));
        assert!(!has_capability(
            &ClientIdentity {
                id: "x".into(),
                caps: CapabilitySet::Named(HashSet::new())
            },
            "__unknown__"
        ));
    }

    #[test]
    fn storage_roundtrip_and_settings_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "com.example.a");
        assert_eq!(s.get("k"), Value::Null);
        s.set("k", json!({"n": 1})).unwrap();
        assert_eq!(s.get("k"), json!({"n": 1}));
        s.delete("k").unwrap();
        assert_eq!(s.get("k"), Value::Null);

        // Settings namespace is isolated under its own reserved key.
        s.settings_set("theme", json!("dark")).unwrap();
        assert_eq!(s.settings_get("theme"), json!("dark"));
        assert_eq!(s.get("theme"), Value::Null); // not a top-level key
    }

    #[test]
    fn quota_is_enforced_on_set() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "big");
        let huge = "x".repeat((STORAGE_QUOTA_BYTES + 1) as usize);
        assert!(s.set("k", json!(huge)).is_err());
        // A normal value still writes.
        assert!(s.set("k", json!("small")).is_ok());
    }
}
