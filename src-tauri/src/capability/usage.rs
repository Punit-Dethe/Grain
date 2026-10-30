//! [GRAIN] Learned tool usage for the Agent's first-round shortlist.
//!
//! A tiny JSON file in the app data directory records how often each tool
//! succeeded. It is read once while an Agent task prepares its tools and
//! rewritten after a successful call; nothing stays in memory between tasks.
//! Only public tool identities, counts and timestamps are stored: never
//! arguments, results, accounts or credentials.
//!
//! Learned usage only *orders* candidates. A tool is offered only if it exists
//! in the current, freshly fetched catalogue, so a stale or edited file cannot
//! expose or authorise anything.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const FILE_NAME: &str = "agent-tool-usage.json";
const SCHEMA: u32 = 1;
/// Upper bound on remembered tools across all extensions.
const MAX_TOOLS: usize = 256;
/// Larger files are ignored rather than parsed.
const MAX_FILE_BYTES: u64 = 64 * 1024;
/// Halve every count once any count reaches this, so old habits fade.
const DECAY_AT: u32 = 64;
const MAX_CANONICAL_BYTES: usize = 512;

/// Serialises read-modify-write cycles within this process.
static FILE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    count: u32,
    last_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct Usage {
    schema: u32,
    tools: BTreeMap<String, Entry>,
}

impl Usage {
    fn parse(bytes: &[u8]) -> Self {
        match serde_json::from_slice::<Usage>(bytes) {
            Ok(usage) if usage.schema == SCHEMA => usage,
            _ => Usage::default(),
        }
    }

    pub(super) fn record(&mut self, canonical: &str, now_ms: u64) {
        if canonical.is_empty()
            || canonical.len() > MAX_CANONICAL_BYTES
            || canonical.split_once(':').is_none()
        {
            return;
        }
        self.schema = SCHEMA;
        let entry = self.tools.entry(canonical.to_string()).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.last_ms = entry.last_ms.max(now_ms);
        if entry.count >= DECAY_AT {
            for entry in self.tools.values_mut() {
                entry.count = (entry.count / 2).max(1);
            }
        }
        if self.tools.len() > MAX_TOOLS {
            let mut ranked: Vec<_> = self
                .tools
                .iter()
                .map(|(id, entry)| (id.clone(), *entry))
                .collect();
            ranked.sort_by(|(_, a), (_, b)| b.last_ms.cmp(&a.last_ms));
            for (id, _) in ranked.into_iter().skip(MAX_TOOLS) {
                self.tools.remove(&id);
            }
        }
    }

    /// The most used tools of one extension, most frequent first, ties broken
    /// by recency then identity so the order is deterministic.
    pub(super) fn hot(&self, extension_id: &str, limit: usize) -> Vec<String> {
        let mut ranked: Vec<_> = self
            .tools
            .iter()
            .filter(|(id, _)| {
                id.split_once(':')
                    .is_some_and(|(extension, _)| extension == extension_id)
            })
            .collect();
        ranked.sort_by(|(a_id, a), (b_id, b)| {
            b.count
                .cmp(&a.count)
                .then(b.last_ms.cmp(&a.last_ms))
                .then(a_id.cmp(b_id))
        });
        ranked
            .into_iter()
            .take(limit)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    crate::portable::app_data_dir(app)
        .ok()
        .map(|dir| dir.join(FILE_NAME))
}

fn read_file(path: &PathBuf) -> Usage {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() <= MAX_FILE_BYTES => std::fs::read(path)
            .map(|bytes| Usage::parse(&bytes))
            .unwrap_or_default(),
        _ => Usage::default(),
    }
}

/// Load the usage file for one task. Missing or malformed files are empty.
pub(super) async fn load(app: &AppHandle) -> Usage {
    let Some(path) = path(app) else {
        return Usage::default();
    };
    tokio::task::spawn_blocking(move || {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        read_file(&path)
    })
    .await
    .unwrap_or_default()
}

/// Count one successful call. Fire-and-forget: the Agent never waits on disk.
pub(super) fn record_success(app: &AppHandle, canonical: &str) {
    let Some(path) = path(app) else {
        return;
    };
    let canonical = canonical.to_string();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    tokio::task::spawn_blocking(move || {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut usage = read_file(&path);
        usage.record(&canonical, now_ms);
        let Ok(bytes) = serde_json::to_vec(&usage) else {
            return;
        };
        let temp = path.with_extension("json.tmp");
        if std::fs::write(&temp, bytes).is_err() || std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
            log::warn!("[GRAIN] agent tool-usage file could not be updated");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_ranks_by_count_then_recency_within_one_extension() {
        let mut usage = Usage::default();
        for (id, at) in [
            ("mcp.notion:search", 1),
            ("mcp.notion:fetch", 2),
            ("mcp.notion:search", 3),
            ("mcp.notion:update", 4),
            ("mcp.github:issue_write", 5),
        ] {
            usage.record(id, at);
        }
        assert_eq!(
            usage.hot("mcp.notion", 3),
            ["mcp.notion:search", "mcp.notion:update", "mcp.notion:fetch"]
        );
        assert_eq!(usage.hot("mcp.github", 4), ["mcp.github:issue_write"]);
        assert!(usage.hot("mcp.linear", 4).is_empty());
        // A prefix of another extension id is not that extension.
        assert!(usage.hot("mcp.not", 4).is_empty());
    }

    #[test]
    fn counts_decay_and_size_is_bounded() {
        let mut usage = Usage::default();
        for at in 0..DECAY_AT as u64 {
            usage.record("mcp.a:x", at);
        }
        assert!(usage.tools["mcp.a:x"].count < DECAY_AT);
        for index in 0..(MAX_TOOLS + 10) {
            usage.record(&format!("mcp.b:tool{index}"), 1_000 + index as u64);
        }
        assert_eq!(usage.tools.len(), MAX_TOOLS);
        // The most recently used tools survive pruning.
        assert!(usage
            .tools
            .contains_key(&format!("mcp.b:tool{}", MAX_TOOLS + 9)));
    }

    #[test]
    fn malformed_unversioned_or_invalid_input_is_ignored() {
        assert!(Usage::parse(b"not json").tools.is_empty());
        assert!(
            Usage::parse(br#"{"schema":99,"tools":{"mcp.a:x":{"count":3,"last_ms":1}}}"#)
                .tools
                .is_empty()
        );
        let round_trip = {
            let mut usage = Usage::default();
            usage.record("mcp.a:x", 7);
            Usage::parse(&serde_json::to_vec(&usage).unwrap())
        };
        assert_eq!(round_trip.hot("mcp.a", 1), ["mcp.a:x"]);
        let mut usage = Usage::default();
        usage.record("no-extension-separator", 1);
        usage.record(&"x:".repeat(MAX_CANONICAL_BYTES), 1);
        assert!(usage.tools.is_empty());
    }
}
