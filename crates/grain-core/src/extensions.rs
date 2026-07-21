//! [GRAIN] The extension registry (SPEC §4.2, §5.1, §10.1) — Phase 1.
//!
//! Persists which extensions are installed, enabled, and in what **toggle
//! order** (SPEC §4.4: the order the user *enabled* them in — first enabled
//! sits top; re-enabling moves to the end). Stored as owned JSON
//! (`extensions.json`) in the same data dir as settings but **physically
//! separate from `AppSettings`**, so core settings migrations never touch
//! extension state and vice versa.
//!
//! Two kinds of entry:
//! - **Built-ins** (Snippets, Context Awareness, Agent) are *not stored here*.
//!   They are descriptors in code, and their enabled state delegates to the
//!   core settings flags (`snippets_enabled`, `context_awareness_enabled`,
//!   `agent_enabled`) — manifest-first per PLAN.md D4: the registry and UI are
//!   new; the implementation stays where it is. Their toggle order is tracked
//!   here by id (a toggle bumps the sequence without creating a record's
//!   install data).
//! - **Installed packs** (the Agent centre-layout variant now; imported
//!   `.grainpack` files in the next chunk) are full records.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const EXTENSIONS_FILE: &str = "extensions.json";

/// The Agent centre-layout surface-variant pack (SPEC §10.2) — pre-known so
/// the upgrade import can install it for existing users, who had the centre
/// option before the platform existed.
pub const AGENT_CENTER_VARIANT_ID: &str = "grain.agent-center-layout";

/// Built-in extension ids (enabled state delegates to settings flags).
pub const BUILTIN_SNIPPETS: &str = "grain.snippets";
pub const BUILTIN_CONTEXT: &str = "grain.context-awareness";
pub const BUILTIN_AGENT: &str = "grain.agent";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionRecord {
    pub id: String,
    pub enabled: bool,
    /// Position in toggle order (SPEC §4.4): set from `next_toggle_seq` every
    /// time the extension is enabled, so re-enabling moves it to the end.
    #[serde(default)]
    pub toggle_seq: u64,
    #[serde(default)]
    pub installed_version: String,
    /// Granted capability names (empty for A-inert packs, which need none).
    #[serde(default)]
    pub granted: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    /// Installed pack records, keyed by extension id.
    #[serde(default)]
    records: HashMap<String, ExtensionRecord>,
    /// Toggle sequence for BUILT-INS (id → seq); their enabled state lives in
    /// settings, but their position in toggle order is registry business.
    #[serde(default)]
    builtin_toggle_seq: HashMap<String, u64>,
    #[serde(default)]
    next_toggle_seq: u64,
}

pub struct ExtensionsRegistry {
    path: PathBuf,
    state: RwLock<RegistryFile>,
}

impl ExtensionsRegistry {
    /// Load (or initialize) the registry. On first initialization,
    /// `settings_file_preexisted` drives the SPEC §10.1 upgrade rule for the
    /// centre-layout variant: existing users had the centre option before the
    /// platform, so it installs enabled for them; new installs start without it.
    pub fn load(data_dir: &Path, settings_file_preexisted: bool) -> Result<Self> {
        let path = data_dir.join(EXTENSIONS_FILE);
        let state = if path.exists() {
            let raw =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str(&raw).unwrap_or_else(|e| {
                // A corrupt registry must not brick startup; extensions revert
                // to "fresh" state, settings-backed built-ins are unaffected.
                log::warn!("extensions.json unreadable ({e}); reinitializing registry");
                RegistryFile::default()
            })
        } else {
            let mut fresh = RegistryFile::default();
            if settings_file_preexisted {
                fresh.next_toggle_seq = 1;
                fresh.records.insert(
                    AGENT_CENTER_VARIANT_ID.to_string(),
                    ExtensionRecord {
                        id: AGENT_CENTER_VARIANT_ID.to_string(),
                        enabled: true,
                        toggle_seq: 0,
                        installed_version: "1.0.0".into(),
                        granted: Vec::new(),
                    },
                );
            }
            fresh
        };
        let reg = Self {
            path,
            state: RwLock::new(state),
        };
        reg.save()?;
        Ok(reg)
    }

    fn save(&self) -> Result<()> {
        let state = self.state.read().unwrap();
        let json = serde_json::to_string_pretty(&*state)?;
        fs::write(&self.path, json).with_context(|| format!("write {}", self.path.display()))
    }

    pub fn record(&self, id: &str) -> Option<ExtensionRecord> {
        self.state.read().unwrap().records.get(id).cloned()
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.state.read().unwrap().records.contains_key(id)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.state
            .read()
            .unwrap()
            .records
            .get(id)
            .map(|r| r.enabled)
            .unwrap_or(false)
    }

    /// Enable/disable an installed pack. Enabling assigns the next toggle
    /// sequence (SPEC §4.4: re-enabling moves to the end of toggle order).
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let changed = {
            let mut state = self.state.write().unwrap();
            let next = state.next_toggle_seq;
            match state.records.get_mut(id) {
                Some(rec) if rec.enabled != enabled => {
                    rec.enabled = enabled;
                    if enabled {
                        rec.toggle_seq = next;
                        state.next_toggle_seq += 1;
                    }
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.save()?;
        }
        Ok(changed)
    }

    /// Record a built-in's enable moment so it participates in toggle order
    /// (its actual enabled bit lives in settings; call this when flipping it on).
    pub fn touch_builtin_toggle(&self, id: &str) -> Result<()> {
        {
            let mut state = self.state.write().unwrap();
            let seq = state.next_toggle_seq;
            state.builtin_toggle_seq.insert(id.to_string(), seq);
            state.next_toggle_seq += 1;
        }
        self.save()
    }

    /// Toggle-order position for any id (built-in or pack); `u64::MAX` for
    /// never-toggled (sorts last, stable).
    pub fn toggle_seq(&self, id: &str) -> u64 {
        let state = self.state.read().unwrap();
        state
            .records
            .get(id)
            .map(|r| r.toggle_seq)
            .or_else(|| state.builtin_toggle_seq.get(id).copied())
            .unwrap_or(u64::MAX)
    }

    /// Install a pack record (import path lands in the next chunk; the
    /// centre-variant import uses this today via `load`).
    pub fn install(&self, record: ExtensionRecord) -> Result<()> {
        self.state
            .write()
            .unwrap()
            .records
            .insert(record.id.clone(), record);
        self.save()
    }

    pub fn uninstall(&self, id: &str) -> Result<bool> {
        let removed = self.state.write().unwrap().records.remove(id).is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn fresh_install_has_no_center_variant() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(!reg.is_installed(AGENT_CENTER_VARIANT_ID));
    }

    #[test]
    fn upgrade_installs_center_variant_enabled() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), true).unwrap();
        assert!(reg.is_enabled(AGENT_CENTER_VARIANT_ID));
        // …and the decision persists across reloads (marker file exists now).
        drop(reg);
        let reg2 = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(reg2.is_enabled(AGENT_CENTER_VARIANT_ID));
    }

    #[test]
    fn toggle_order_is_enable_order_and_reenable_moves_to_end() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        for id in ["a", "b"] {
            reg.install(ExtensionRecord {
                id: id.into(),
                enabled: false,
                toggle_seq: 0,
                installed_version: "1".into(),
                granted: vec![],
            })
            .unwrap();
        }
        reg.set_enabled("a", true).unwrap();
        reg.set_enabled("b", true).unwrap();
        assert!(reg.toggle_seq("a") < reg.toggle_seq("b"), "first enabled = top");

        // Built-ins participate in the same ordering space.
        reg.touch_builtin_toggle(BUILTIN_SNIPPETS).unwrap();
        assert!(reg.toggle_seq("b") < reg.toggle_seq(BUILTIN_SNIPPETS));

        // Disable + re-enable moves to the end (SPEC §4.4).
        reg.set_enabled("a", false).unwrap();
        reg.set_enabled("a", true).unwrap();
        assert!(reg.toggle_seq("a") > reg.toggle_seq(BUILTIN_SNIPPETS));

        // Never-toggled sorts last.
        assert_eq!(reg.toggle_seq("never"), u64::MAX);
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = tmp();
        {
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            reg.install(ExtensionRecord {
                id: "x".into(),
                enabled: false,
                toggle_seq: 0,
                installed_version: "2.0".into(),
                granted: vec![],
            })
            .unwrap();
            reg.set_enabled("x", true).unwrap();
        }
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(reg.is_enabled("x"));
        assert_eq!(reg.record("x").unwrap().installed_version, "2.0");
        // Corrupt file → reinitialize, not crash.
        fs::write(dir.path().join(EXTENSIONS_FILE), "{not json").unwrap();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(!reg.is_installed("x"));
    }
}
