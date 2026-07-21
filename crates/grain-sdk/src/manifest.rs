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
    /// surface, not before).
    #[serde(default)]
    pub permissions: Vec<String>,
}

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
    /// Structural validation for Phase 1 (tier-A-inert only).
    pub fn validate(&self) -> Result<(), String> {
        let m = &self.manifest;
        if m.id.is_empty() || !m.id.contains('.') {
            return Err("manifest.id must be a reverse-dns identifier".into());
        }
        if m.id.starts_with("grain.") {
            return Err("the 'grain.' id prefix is reserved for built-ins".into());
        }
        if m.name.trim().is_empty() {
            return Err("manifest.name is required".into());
        }
        if m.tier != Tier::Pack {
            return Err("only tier-A packs can be imported in this version".into());
        }
        if !m.permissions.is_empty() {
            // A-inert by definition (SPEC §1.1): data consumed locally needs no
            // grants. Egress packs (providers) ship with their consent surface.
            return Err(format!(
                "packs requesting permissions ({}) are not supported yet",
                m.permissions.join(", ")
            ));
        }
        for p in &self.payloads.prompts {
            if p.id.is_empty() || p.name.trim().is_empty() || p.prompt.trim().is_empty() {
                return Err(format!("prompt entry '{}' is incomplete", p.id));
            }
        }
        Ok(())
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
    fn guards_hold() {
        // reserved prefix, bad id, wrong tier, permissions on an inert pack
        assert!(pack(r#"{"manifest":{"id":"grain.x","name":"n","version":"1","tier":"pack"}}"#).is_err());
        assert!(pack(r#"{"manifest":{"id":"noreversedns","name":"n","version":"1","tier":"pack"}}"#).is_err());
        assert!(pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"scripted"}}"#).is_err());
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
