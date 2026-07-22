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

/// Cap on texts per `embed` call. The engine holds every input's tokens in
/// memory at once, so an unbounded batch is a memory-exhaustion lever; a
/// low-RAM device would rather the extension chunk its work.
const EMBED_MAX_BATCH: usize = 64;

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
        "doc.get" | "doc.put" | "doc.delete" | "doc.list" => Some("storage"),
        "settings.get" | "settings.set" => Some("settings"),
        "llm.complete" => Some("llm"),
        "embed" => Some("embed"),
        "session.start" => Some("session:start"),
        "capture.selection" => Some("capture:selection"),
        "workspace.open" | "workspace.close" => Some("surface:workspace"),
        "overlay.show" | "overlay.dismiss" => Some("surface:overlay"),
        _ => Some("__unknown__"), // unknown methods map to an ungrantable cap
    }
}

/// A per-extension JSON key/value store: `<data>/extensions/<id>.storage.json`.
/// Loaded and saved whole (fine at this scale). Pure over its path, so quota
/// and round-trip behavior are unit-tested directly.
pub struct ExtStorage {
    path: PathBuf,
    /// `<data>/extensions/<id>.docs/` — the document store's own directory, one
    /// file per document, so a large collection is not one blob rewritten on
    /// every edit (SPEC §3.4: notes are documents, not KV values).
    docs_dir: PathBuf,
}

impl ExtStorage {
    pub fn new(data_dir: &std::path::Path, ext_id: &str) -> Self {
        let ext_dir = data_dir.join("extensions");
        Self {
            path: ext_dir.join(format!("{ext_id}.storage.json")),
            docs_dir: ext_dir.join(format!("{ext_id}.docs")),
        }
    }

    /// A document key sanitized to a single safe filename. Rejects anything that
    /// could escape the store's directory or collide across keys — the whole
    /// point of a per-document file is undone if a key can be `../../secrets`.
    /// Pure and total, so it is exhaustively unit-tested.
    pub fn safe_doc_name(key: &str) -> Result<String, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("a document key must not be empty".into());
        }
        if key.len() > 200 {
            return Err("a document key is at most 200 characters".into());
        }
        // Allowlist, not denylist: only characters that are unambiguously safe
        // in a filename on every platform, and never a path separator or dot-run.
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err("a document key may contain only letters, digits, '-', '_' and '.'".into());
        }
        // `.` and `..` (and any all-dots name) are path traversal, not documents.
        if key.chars().all(|c| c == '.') {
            return Err("a document key must not be all dots".into());
        }
        Ok(format!("{key}.json"))
    }

    fn doc_path(&self, key: &str) -> Result<PathBuf, String> {
        Ok(self.docs_dir.join(Self::safe_doc_name(key)?))
    }

    /// Total bytes the document store currently occupies (for the quota).
    fn docs_bytes(&self) -> u64 {
        std::fs::read_dir(&self.docs_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    }

    pub fn doc_get(&self, key: &str) -> Result<Value, String> {
        let path = self.doc_path(key)?;
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string()),
            Err(_) => Ok(Value::Null), // absent document reads as null, like KV
        }
    }

    pub fn doc_put(&self, key: &str, value: Value) -> Result<(), String> {
        let path = self.doc_path(key)?;
        let json = serde_json::to_string(&value).map_err(|e| e.to_string())?;
        // Quota covers the doc store as a whole; charge the NEW size of this doc
        // against the total minus whatever this key used to occupy.
        let prev = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let projected = self.docs_bytes().saturating_sub(prev) + json.len() as u64;
        if projected > STORAGE_QUOTA_BYTES {
            return Err(format!(
                "document storage quota exceeded ({} MB max)",
                STORAGE_QUOTA_BYTES / (1024 * 1024)
            ));
        }
        std::fs::create_dir_all(&self.docs_dir).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    pub fn doc_delete(&self, key: &str) -> Result<(), String> {
        let path = self.doc_path(key)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Every document key currently stored (without the `.json` suffix), sorted.
    pub fn doc_list(&self) -> Vec<String> {
        let mut keys: Vec<String> = std::fs::read_dir(&self.docs_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|n| n.strip_suffix(".json"))
                    .map(str::to_string)
            })
            .collect();
        keys.sort();
        keys
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
        // [GRAIN] SPEC §3.4: the document store — one file per key, so a large
        // note collection is not one blob rewritten on every edit. Same
        // `storage` grant, same quota; the key is a path-safe filename.
        "doc.get" => store.doc_get(&param_str(&params, "key")?),
        "doc.put" => {
            let key = param_str(&params, "key")?;
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            store.doc_put(&key, value)?;
            Ok(Value::Null)
        }
        "doc.delete" => {
            store.doc_delete(&param_str(&params, "key")?)?;
            Ok(Value::Null)
        }
        "doc.list" => Ok(json!({ "keys": store.doc_list() })),
        // [GRAIN] SPEC §1.2: the extension asks for ITS OWN workspace and gets
        // nothing else — there is no id parameter to point at another
        // extension's surface, because identity comes from the channel.
        "workspace.open" => {
            crate::surfaces::extension::open(app, &identity.id, params.get("payload").cloned())?;
            Ok(Value::Null)
        }
        "workspace.close" => {
            crate::surfaces::extension::close(app, &identity.id);
            Ok(Value::Null)
        }
        // [GRAIN] SPEC §1.2: a transient HUD for THIS extension, host-budgeted
        // in size and lifetime — same channel-derived identity as workspace.
        "overlay.show" => {
            crate::surfaces::overlay::show(app, &identity.id, params.get("payload").cloned())?;
            Ok(Value::Null)
        }
        "overlay.dismiss" => {
            crate::surfaces::overlay::dismiss(app, &identity.id);
            Ok(Value::Null)
        }
        "settings.get" => {
            let key = param_str(&params, "key")?;
            let stored = store.settings_get(&key);
            // A declared setting reads back through the schema, so a value left
            // behind by an older version resolves to something the extension's
            // own declaration says is legal.
            Ok(
                match crate::grain_commands::setting_decl(app, &identity.id, &key) {
                    Some(decl) => grain_sdk::settings_schema::resolve(&decl, Some(&stored)).value,
                    None => stored,
                },
            )
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
                Some(decl) => {
                    grain_sdk::settings_schema::coerce(&decl, &value)
                        .map_err(|e| format!("'{key}': {e}"))?
                        .value
                }
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
            // [GRAIN] SPEC §1.3 / Grain Space Test: the same on-device BGE
            // embedder Grain Space uses, offered to extensions. Local, free,
            // private — the reason a Grain-Space-class extension is buildable
            // without shipping its own model. Blocking (model inference), so it
            // runs on the blocking pool; failure to load the model is surfaced
            // verbatim rather than pretended around.
            let texts: Vec<String> = params
                .get("texts")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or_default().to_string())
                        .collect()
                })
                .ok_or("embed requires a 'texts' array of strings")?;
            if texts.len() > EMBED_MAX_BATCH {
                return Err(format!(
                    "embed accepts at most {EMBED_MAX_BATCH} texts per call"
                ));
            }
            let vectors =
                tokio::task::spawn_blocking(move || crate::grain_space::embed::embed(texts))
                    .await
                    .map_err(|e| format!("embed task failed: {e}"))?
                    .map_err(|e| e.to_string())?;
            Ok(json!({ "vectors": vectors }))
        }
        "capture.selection" => {
            // [GRAIN] Grain Space Test: the selection quick-add path. Simulates
            // a copy in the foreground app, reads the result, and restores the
            // clipboard — the same primitive the Agent and Grain Space capture
            // use. Blocking (it polls the clipboard), so off the async thread;
            // `null` when there was nothing selected.
            let app2 = app.clone();
            let text = tokio::task::spawn_blocking(move || crate::agent::capture_selection(&app2))
                .await
                .map_err(|e| format!("capture task failed: {e}"))?;
            Ok(json!({ "text": text }))
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
        assert_eq!(
            required_capability("capture.selection"),
            Some("capture:selection")
        );
        assert_eq!(required_capability("embed"), Some("embed"));
        // The document store shares the storage grant.
        assert_eq!(required_capability("doc.put"), Some("storage"));
        assert_eq!(required_capability("doc.list"), Some("storage"));
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

    #[test]
    fn document_keys_cannot_escape_the_store() {
        // The security-critical part: a key is a filename, never a path.
        for bad in [
            "",
            "  ",
            "../secrets",
            "a/b",
            "a\\b",
            ".",
            "..",
            "...",
            "a b",
            "a:b",
            "note\0",
            &"x".repeat(201),
        ] {
            assert!(
                ExtStorage::safe_doc_name(bad).is_err(),
                "key {bad:?} must be rejected"
            );
        }
        for ok in ["note", "note-1", "note_1", "a.b", "2026-07-22", "Z9"] {
            assert_eq!(ExtStorage::safe_doc_name(ok).unwrap(), format!("{ok}.json"));
        }
    }

    #[test]
    fn document_store_roundtrips_and_lists() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "com.example.docs");
        assert_eq!(s.doc_get("a").unwrap(), Value::Null); // absent → null
        assert!(s.doc_list().is_empty());

        s.doc_put("a", json!({"body": "one"})).unwrap();
        s.doc_put("b", json!({"body": "two"})).unwrap();
        assert_eq!(s.doc_get("a").unwrap(), json!({"body": "one"}));
        assert_eq!(s.doc_list(), vec!["a".to_string(), "b".to_string()]);

        // A document is its own file — the KV store is untouched by doc writes.
        assert_eq!(s.get("a"), Value::Null);

        s.doc_delete("a").unwrap();
        assert_eq!(s.doc_get("a").unwrap(), Value::Null);
        assert_eq!(s.doc_list(), vec!["b".to_string()]);
        // Deleting an absent document is not an error.
        assert!(s.doc_delete("gone").is_ok());
    }

    #[test]
    fn document_put_rejects_an_unsafe_key_before_touching_disk() {
        let dir = tempfile::tempdir().unwrap();
        let s = ExtStorage::new(dir.path(), "com.example.docs");
        assert!(s.doc_put("../escape", json!(1)).is_err());
        assert!(s.doc_get("../escape").is_err());
        assert!(s.doc_delete("../escape").is_err());
    }
}
