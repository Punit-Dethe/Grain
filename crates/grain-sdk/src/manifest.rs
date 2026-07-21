//! The extension manifest (SPEC §2) — Phase 1 subset.
//!
//! Deliberately only what the current platform consumes (R1: grant narrowly,
//! widen later): identity/display fields, tier, permissions as opaque names,
//! and tier-A pack payloads. Activation events, surfaces, slots, `provides:`,
//! `requires:` and the settings schema join as their consumers land
//! (Phases 2–3). Unknown JSON fields are ignored on read, so manifests written
//! against a NEWER contract still install here with their known subset.
//!
//! Packaging (Phase 1): a `.grainpack` is ONE JSON file — the manifest plus
//! embedded payloads — because tier-A packs are small data and a single file
//! is trivially shareable. Multi-file bundles (tier B/C) arrive with their
//! tiers.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Pack,
    Scripted,
    Native,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionManifest {
    /// Reverse-dns, unique in the index (SPEC §2).
    pub id: String,
    pub name: String,
    pub version: String,
    /// Contract semver the pack was written against (informational in Phase 1;
    /// enforced when the runtime tiers land).
    #[serde(default)]
    pub grain_api: String,
    pub tier: Tier,
    /// One line, shown in Overview; full text on hover.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub repository: Option<String>,
    /// Capability names (SPEC §1.3). Tier-A-inert packs must have none — the
    /// import path rejects otherwise (egress packs arrive with their consent
    /// surface, not before). Scripted packs may request from
    /// [`KNOWN_CAPABILITIES`]; the user grants them at first enable.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// [GRAIN] SPEC §2 activation events (tier B): when the worker wakes —
    /// `onEvent:<DaemonEventVariant>`, `onTransform`, `onShortcut:<id>`,
    /// `onStartup` (requires `resident`). The reaper is the inverse.
    #[serde(default)]
    pub activation: Vec<String>,
    /// [GRAIN] Tier-B only: the extension's JS, embedded so a scripted pack
    /// stays a single shareable file (guide Step 4). Empty for tier-A.
    #[serde(default)]
    pub entry_source: String,
}

/// Capabilities a scripted pack may request in Phase 2. Anything outside this
/// set is rejected at import (R1: grant narrowly, widen with each consumer).
/// `session:start` is reserved + plumbed even though no built-in dogfoods it
/// yet (structural capabilities land early or never).
pub const KNOWN_CAPABILITIES: &[&str] = &[
    "events:sessions",
    "events:transcripts",
    "transform:transcript",
    "session:start",
    "storage",
    "settings",
    "llm",
    "embed",
];

/// One prompt in a prompt pack. Applied to the user's prompt list under the
/// namespaced id `ext:<extension-id>:<id>` (SPEC chokepoint #15 — collisions
/// unrepresentable), and removed by that prefix on disable.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptPackEntry {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

/// Embedded tier-A payloads. All optional; a pack ships any subset.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PackPayloads {
    #[serde(default)]
    pub prompts: Vec<PromptPackEntry>,
    /// Pill theme JSON (SPEC §9.4) — stored and validated on import; rendering
    /// lands with the pill-side evaluator. Kept opaque here so the theme
    /// schema can evolve without an sdk release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pill_theme: Option<serde_json::Value>,
}

/// The `.grainpack` file: manifest + payloads in one JSON document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GrainPack {
    pub manifest: ExtensionManifest,
    #[serde(default)]
    pub payloads: PackPayloads,
}

impl GrainPack {
    /// Structural validation (Phase 2: tier-A packs and tier-B scripted;
    /// `native` still rejected — it arrives with the tier-C supervisor).
    pub fn validate(&self) -> Result<(), String> {
        let m = &self.manifest;
        if m.id.is_empty() || !m.id.contains('.') {
            return Err("manifest.id must be a reverse-dns identifier".into());
        }
        // NOTE: `grain.`-prefixed ids are built-in-only for USER imports; the
        // startup seed of built-in scripted packs (e.g. auto-categorize) calls
        // its own path, not this validator, so the prefix stays reserved here.
        if m.id.starts_with("grain.") {
            return Err("the 'grain.' id prefix is reserved for built-ins".into());
        }
        if m.name.trim().is_empty() {
            return Err("manifest.name is required".into());
        }
        match m.tier {
            Tier::Native => {
                return Err("native extensions are not supported yet".into());
            }
            Tier::Pack => {
                if !m.permissions.is_empty() {
                    // A-inert by definition (SPEC §1.1): data consumed locally
                    // needs no grants. Egress/provider packs arrive with their
                    // consent surface later.
                    return Err(format!(
                        "tier-A packs requesting permissions ({}) are not supported yet",
                        m.permissions.join(", ")
                    ));
                }
                if !m.entry_source.is_empty() {
                    return Err("tier-A packs must not carry entry_source".into());
                }
            }
            Tier::Scripted => {
                if m.entry_source.trim().is_empty() {
                    return Err("scripted extensions require entry_source".into());
                }
                for cap in &m.permissions {
                    if !KNOWN_CAPABILITIES.contains(&cap.as_str()) {
                        return Err(format!("unknown capability '{cap}'"));
                    }
                }
            }
        }
        for p in &self.payloads.prompts {
            if p.id.is_empty() || p.name.trim().is_empty() || p.prompt.trim().is_empty() {
                return Err(format!("prompt entry '{}' is incomplete", p.id));
            }
        }
        Ok(())
    }

    /// True for tier-B extensions (drive a worker), false for data packs.
    pub fn is_scripted(&self) -> bool {
        self.manifest.tier == Tier::Scripted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(json: &str) -> Result<(), String> {
        serde_json::from_str::<GrainPack>(json)
            .map_err(|e| e.to_string())
            .and_then(|p| p.validate())
    }

    #[test]
    fn valid_prompt_pack_passes() {
        assert_eq!(
            pack(
                r#"{"manifest":{"id":"com.x.zh","name":"Zh Prompts","version":"1.0","tier":"pack"},
                    "payloads":{"prompts":[{"id":"formal","name":"Formal","prompt":"Rewrite formally."}]}}"#
            ),
            Ok(())
        );
    }

    #[test]
    fn scripted_pack_passes_with_entry_and_known_caps() {
        assert_eq!(
            pack(
                r#"{"manifest":{"id":"com.x.cat","name":"Cat","version":"1","tier":"scripted",
                    "permissions":["storage","llm"],"activation":["onEvent:TranscriptionComplete"],
                    "entry_source":"grain.log.info('hi')"}}"#
            ),
            Ok(())
        );
    }

    #[test]
    fn guards_hold() {
        // reserved prefix, bad id, native tier, permissions on an inert pack
        assert!(pack(r#"{"manifest":{"id":"grain.x","name":"n","version":"1","tier":"pack"}}"#).is_err());
        assert!(pack(r#"{"manifest":{"id":"noreversedns","name":"n","version":"1","tier":"pack"}}"#).is_err());
        assert!(pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"native"}}"#).is_err());
        // scripted without entry_source, and with an unknown capability
        assert!(pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"scripted"}}"#).is_err());
        assert!(pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"scripted","entry_source":"x","permissions":["root"]}}"#).is_err());
        // tier-A pack must not carry code
        assert!(pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"pack","entry_source":"x"}}"#).is_err());
        assert!(
            pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"pack","permissions":["llm"]}}"#)
                .is_err()
        );
        // unknown fields from a newer contract are tolerated
        assert_eq!(
            pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"pack","futureField":1}}"#),
            Ok(())
        );
    }
}
