//! On-disk copy of each connected account's MCP tool list.
//!
//! The Agent reads this copy instead of connecting to every server at the start
//! of a turn. It is a hint, never an authority: every tool call still compares
//! the server's live tool set against the digest the Agent saw before anything
//! is dispatched. Nothing is held in memory between reads.
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const SCHEMA: u32 = 1;
const DIR: &str = "mcp-tool-cache";
/// Upper bound for one saved catalogue; larger files are ignored and rediscovered.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Identity a saved catalogue must match before it is offered again.
pub(super) struct Key<'a> {
    /// Vault account of the connection (unique per catalog provider or configured connection).
    pub account: &'a str,
    /// Endpoint the tools were discovered from.
    pub endpoint: &'a str,
    /// Connection binding (revision stamp for configured connections; empty for presets).
    pub binding: &'a str,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    schema: u32,
    account: String,
    endpoint: String,
    binding: String,
    digest: String,
    tools: Vec<Tool>,
}

fn path(app: &AppHandle, account: &str) -> Option<PathBuf> {
    let context = app.try_state::<std::sync::Arc<grain_core::AppContext>>()?;
    let name = format!("{:x}.json", Sha256::digest(account.as_bytes()));
    Some(context.data_dir.join(DIR).join(name))
}

/// Saved tools and their raw digest, when the file matches `key` exactly.
pub(super) async fn load(app: &AppHandle, key: &Key<'_>) -> Option<(String, Vec<Tool>)> {
    let path = path(app, key.account)?;
    let metadata = tokio::fs::metadata(&path).await.ok()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return None;
    }
    let bytes = tokio::fs::read(&path).await.ok()?;
    let saved: Saved = serde_json::from_slice(&bytes).ok()?;
    matches(&saved, key).then_some((saved.digest, saved.tools))
}

fn matches(saved: &Saved, key: &Key<'_>) -> bool {
    saved.schema == SCHEMA
        && saved.account == key.account
        && saved.endpoint == key.endpoint
        && saved.binding == key.binding
}

/// Replace the saved catalogue. Best effort: a failed write only costs a live
/// discovery on the next turn.
pub(super) async fn store(app: &AppHandle, key: &Key<'_>, digest: &str, tools: &[Tool]) {
    let Some(path) = path(app, key.account) else {
        return;
    };
    let saved = Saved {
        schema: SCHEMA,
        account: key.account.to_owned(),
        endpoint: key.endpoint.to_owned(),
        binding: key.binding.to_owned(),
        digest: digest.to_owned(),
        tools: tools.to_vec(),
    };
    let Ok(bytes) = serde_json::to_vec(&saved) else {
        return;
    };
    if bytes.len() as u64 > MAX_BYTES {
        return;
    }
    let written = tokio::task::spawn_blocking(move || write_atomic(&path, &bytes)).await;
    if !matches!(written, Ok(Ok(()))) {
        log::warn!("[GRAIN] mcp tool cache write failed");
    }
}

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

/// Forget a connection's saved catalogue (sign-in, disconnect, removal, drift).
pub(super) async fn clear(app: &AppHandle, account: &str) {
    if let Some(path) = path(app, account) {
        let _ = tokio::task::spawn_blocking(move || clear_path(&path)).await;
    }
}

/// Synchronous variant for callers already on a blocking storage thread.
pub(super) fn clear_blocking(app: &AppHandle, account: &str) {
    if let Some(path) = path(app, account) {
        clear_path(&path);
    }
}

fn clear_path(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("json.tmp"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> Tool {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "description": "d",
            "inputSchema": { "type": "object", "properties": { "q": { "type": "string" } } },
        }))
        .unwrap()
    }

    #[test]
    fn saved_catalogue_round_trips_and_matches_only_its_exact_identity() {
        let dir = std::env::temp_dir().join(format!("grain-tool-cache-{}", std::process::id()));
        let path = dir.join("x.json");
        let saved = Saved {
            schema: SCHEMA,
            account: "acct".into(),
            endpoint: "https://example.com/mcp".into(),
            binding: "stamp:".into(),
            digest: "abc".into(),
            tools: vec![tool("one"), tool("two")],
        };
        write_atomic(&path, &serde_json::to_vec(&saved).unwrap()).unwrap();
        let read: Saved = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(read.tools.len(), 2);
        assert_eq!(read.tools[1].name, "two");
        assert_eq!(read.digest, "abc");
        assert!(!path.with_extension("json.tmp").exists());

        let key = |account, endpoint, binding| Key {
            account,
            endpoint,
            binding,
        };
        assert!(matches(
            &read,
            &key("acct", "https://example.com/mcp", "stamp:")
        ));
        assert!(!matches(
            &read,
            &key("other", "https://example.com/mcp", "stamp:")
        ));
        assert!(!matches(
            &read,
            &key("acct", "https://evil.example/mcp", "stamp:")
        ));
        assert!(!matches(
            &read,
            &key("acct", "https://example.com/mcp", "newer:")
        ));

        clear_path(&path);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
