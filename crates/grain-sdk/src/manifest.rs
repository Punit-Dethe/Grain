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
    /// [GRAIN] Phase 3 (SPEC §1.2): surfaces the extension DECLARES. Extensions
    /// never create windows — the host builds, places, sleeps and destroys them.
    #[serde(default)]
    pub surfaces: Surfaces,
    /// [GRAIN] Phase 3 (SPEC §3): exclusive positions claimed. At most one
    /// enabled occupant per slot; claiming an occupied slot prompts a takeover.
    #[serde(default)]
    pub slots: Vec<String>,
    /// [GRAIN] Phase 3 (SPEC §4): declarative contributions the host renders or
    /// registers on the extension's behalf.
    #[serde(default)]
    pub contributes: Contributes,
}

/// Surfaces an extension may declare (SPEC §1.2). Each requires the matching
/// `surface:*` capability — declaring one without it is rejected at import.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Surfaces {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<OverlayDecl>,
}

/// An app-class window: built hidden once, shown on summon, UI unmounted +
/// hidden on close, destroyed after idle (the generalized Grain Space pattern).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceDecl {
    pub title: String,
    /// `[width, height]`; the host clamps to what the display can show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_size: Option<[u32; 2]>,
    /// The workspace UI as a self-contained HTML document, embedded so a
    /// scripted pack stays one shareable file.
    ///
    /// It is loaded into a **sandboxed iframe** — opaque origin, no Tauri IPC,
    /// no reach into the page around it (SPEC §7.1: a UI surface gets its own
    /// realm). That surrounding page is Grain's code and is the only thing
    /// holding the surface token, so the extension's own markup cannot forge an
    /// identity by asserting one in a payload.
    #[serde(default)]
    pub ui_source: String,
}

/// A transient HUD: created per invocation, destroyed on dismiss. The host
/// enforces the size and lifetime budget — an overlay cannot linger.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayDecl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[u32; 2]>,
    /// Auto-dismiss budget; the host caps this regardless of what is asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Contributes {
    /// Level 1–2 settings schema — the host renders the controls; the values
    /// live in the extension's own namespace (never `AppSettings`).
    #[serde(default)]
    pub settings: Vec<SettingDecl>,
    /// Global shortcuts, registered as `ext:<id>:<shortcut-id>`.
    #[serde(default)]
    pub shortcuts: Vec<ShortcutDecl>,
}

/// One schema-declared setting (SPEC §4, levels 1–2).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingDecl {
    pub key: String,
    pub label: String,
    #[serde(flatten)]
    pub kind: SettingKind,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub description: String,
    /// Where this section renders (SPEC §4). An anchor is a **versioned
    /// contract promise** — see [`ANCHORS`]. Absent = the extension's own
    /// section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// Sort position within its group; ties break on declaration order.
    #[serde(default)]
    pub order: i32,
}

/// The control the host renders. Internally tagged, so a declaration reads
/// `{"key":…, "kind":"select", "options":[…]}`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SettingKind {
    Bool,
    String,
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Select {
        options: Vec<SelectOption>,
    },
    Shortcut,
    Color,
    Slider {
        min: f64,
        max: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    /// A kind this build doesn't know (SPEC §4.1 also defines `rows`, and the
    /// list will grow). Without this, one unknown kind makes the WHOLE pack
    /// fail to deserialize — a manifest written against a newer contract must
    /// still install with its known subset. The host skips rendering it.
    #[serde(other)]
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShortcutDecl {
    pub id: String,
    pub label: String,
    /// Suggested binding; the user's choice always wins, and a conflict with an
    /// existing binding is resolved by the host, not the extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_binding: Option<String>,
}

/// Anchors an extension may attach a settings section to (SPEC §4.3 v1).
///
/// **Contract surface: few, semantic, versioned.** Adding one is a promise;
/// removing one is a breaking change — so this list is copied from the SPEC
/// verbatim and must not be extended casually.
///
/// An anchor OUTSIDE this list is **not an error**: per SPEC §4.3 the group
/// falls back to the extension's own settings section, because settings are
/// never lost. [`ANCHORS`] therefore drives rendering, not validation.
pub const ANCHORS: &[&str] = &[
    "snippets.after",
    "dictation.pipeline.after",
    "context.after",
    "agent.after",
    "models.after",
];

/// Exclusive positions (SPEC §3). Core defaults are occupants too, so a claim
/// on any of these can displace a shipped feature — never silently.
pub const KNOWN_SLOTS: &[&str] = &[
    "overlay.recording",
    "overlay.pointer",
    "pill.theme",
    "agent.reply-surface",
    "output.destination",
];

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
    // Phase 3 (SPEC §1.2): host-owned surfaces. Declaring a surface without
    // its capability is rejected — the grant is what the user actually approves.
    "surface:workspace",
    "surface:overlay",
    "pill:slots",
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
        self.validate_phase3()?;
        Ok(())
    }

    /// Phase 3 contract checks (SPEC §1.2, §3, §4). Split out so the tier
    /// branch above stays readable.
    fn validate_phase3(&self) -> Result<(), String> {
        let m = &self.manifest;

        // Slots may be claimed by any tier (a pill theme is tier-A), but only
        // from the known list — an unknown slot is a silent no-op otherwise.
        for slot in &m.slots {
            let known = KNOWN_SLOTS.contains(&slot.as_str()) || slot.starts_with("overrides:"); // `overrides:<core-setting>`
            if !known {
                return Err(format!("unknown slot '{slot}'"));
            }
        }

        // Surfaces and code-backed contributions need code to back them.
        let declares_surface = m.surfaces.workspace.is_some() || m.surfaces.overlay.is_some();
        let contributes_code =
            !m.contributes.settings.is_empty() || !m.contributes.shortcuts.is_empty();
        if (declares_surface || contributes_code) && m.tier != Tier::Scripted {
            return Err(
                "surfaces and contributes require tier 'scripted' (there is no code to back them)"
                    .into(),
            );
        }

        // A declared surface must be backed by the capability the user grants.
        for (declared, cap) in [
            (m.surfaces.workspace.is_some(), "surface:workspace"),
            (m.surfaces.overlay.is_some(), "surface:overlay"),
        ] {
            if declared && !m.permissions.iter().any(|p| p == cap) {
                return Err(format!(
                    "declaring this surface requires the '{cap}' permission"
                ));
            }
        }

        // A workspace with nothing to render is a window that opens blank and
        // cannot be explained to the user — reject it at import, not at open.
        if let Some(w) = &m.surfaces.workspace {
            if w.ui_source.trim().is_empty() {
                return Err("a workspace surface requires ui_source".into());
            }
        }

        let mut seen = std::collections::HashSet::new();
        for s in &m.contributes.settings {
            if s.key.trim().is_empty() {
                return Err("a setting is missing its key".into());
            }
            if !seen.insert(&s.key) {
                return Err(format!("duplicate setting key '{}'", s.key));
            }
            // NOTE: an unknown `anchor` is deliberately NOT an error — SPEC
            // §4.3 requires the group to fall back to the extension's own
            // section so settings are never lost.
            if let SettingKind::Select { options } = &s.kind {
                if options.is_empty() {
                    return Err(format!("select setting '{}' has no options", s.key));
                }
            }
        }

        // A contributed shortcut is registered as `ext:<extension-id>:<id>`,
        // parsed by splitting on the first two colons. A colon in either id
        // would make that ambiguous, so it is rejected at import rather than
        // producing a binding that routes to the wrong extension.
        if m.id.contains(':') {
            return Err("manifest.id must not contain ':'".into());
        }
        let mut seen_sc = std::collections::HashSet::new();
        for sc in &m.contributes.shortcuts {
            if sc.id.trim().is_empty() {
                return Err("a shortcut is missing its id".into());
            }
            if sc.id.contains(':') {
                return Err(format!("shortcut id '{}' must not contain ':'", sc.id));
            }
            if !seen_sc.insert(&sc.id) {
                return Err(format!("duplicate shortcut id '{}'", sc.id));
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
        assert!(
            pack(r#"{"manifest":{"id":"grain.x","name":"n","version":"1","tier":"pack"}}"#)
                .is_err()
        );
        assert!(pack(
            r#"{"manifest":{"id":"noreversedns","name":"n","version":"1","tier":"pack"}}"#
        )
        .is_err());
        assert!(
            pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"native"}}"#)
                .is_err()
        );
        // scripted without entry_source, and with an unknown capability
        assert!(pack(
            r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"scripted"}}"#
        )
        .is_err());
        assert!(pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"scripted","entry_source":"x","permissions":["root"]}}"#).is_err());
        // tier-A pack must not carry code
        assert!(pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"pack","entry_source":"x"}}"#).is_err());
        assert!(
            pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"pack","permissions":["llm"]}}"#)
                .is_err()
        );
        // unknown fields from a newer contract are tolerated
        assert_eq!(
            pack(
                r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"pack","futureField":1}}"#
            ),
            Ok(())
        );
    }

    /// A full Phase-3 scripted manifest parses and validates, and the settings
    /// schema keeps its internally-tagged shape.
    #[test]
    fn phase3_declarations_parse_and_validate() {
        let json = r#"{"manifest":{
            "id":"com.x.spaces","name":"Spaces","version":"1","tier":"scripted",
            "permissions":["storage","surface:workspace"],
            "activation":["onStartup"],
            "entry_source":"grain.log.info('hi')",
            "surfaces":{"workspace":{"title":"Spaces","min_size":[900,600],
                "ui_source":"<h1>Spaces</h1>"}},
            "slots":["agent.reply-surface","overrides:overlay_position"],
            "contributes":{
                "settings":[
                    {"key":"tone","label":"Tone","kind":"select",
                     "options":[{"value":"warm","label":"Warm"}],
                     "anchor":"space.after","order":2},
                    {"key":"auto","label":"Auto","kind":"bool","default":true}
                ],
                "shortcuts":[{"id":"open","label":"Open Spaces","default_binding":"Alt+S"}]
            }}}"#;
        let p: GrainPack = serde_json::from_str(json).unwrap();
        assert_eq!(p.validate(), Ok(()));
        assert_eq!(
            p.manifest.surfaces.workspace.unwrap().min_size,
            Some([900, 600])
        );
        assert!(matches!(
            p.manifest.contributes.settings[0].kind,
            SettingKind::Select { .. }
        ));
        assert_eq!(p.manifest.contributes.shortcuts[0].id, "open");
    }

    #[test]
    fn phase3_guards_hold() {
        let scripted = |extra: &str| {
            pack(&format!(
                r#"{{"manifest":{{"id":"com.x.y","name":"n","version":"1","tier":"scripted",
                    "entry_source":"x"{extra}}}}}"#
            ))
        };
        // A surface without its capability is rejected — the grant is the point.
        assert!(scripted(r#","surfaces":{"workspace":{"title":"T","ui_source":"<p>x"}}"#).is_err());
        assert!(scripted(
            r#","permissions":["surface:workspace"],
               "surfaces":{"workspace":{"title":"T","ui_source":"<p>x"}}"#
        )
        .is_ok());
        // …and a workspace with no UI would open a blank window nobody can
        // explain, so it is refused at import rather than at open.
        assert!(scripted(
            r#","permissions":["surface:workspace"],"surfaces":{"workspace":{"title":"T"}}"#
        )
        .is_err());
        // Unknown slot / anchor, duplicate keys, empty select.
        assert!(scripted(r#","slots":["not.a.slot"]"#).is_err());
        assert!(scripted(r#","slots":["pill.theme"]"#).is_ok());
        assert!(scripted(
            r#","contributes":{"settings":[{"key":"a","label":"A","kind":"bool"},{"key":"a","label":"B","kind":"bool"}]}"#
        )
        .is_err());
        assert!(scripted(
            r#","contributes":{"settings":[{"key":"a","label":"A","kind":"select","options":[]}]}"#
        )
        .is_err());
        // A colon in either id would make `ext:<extension-id>:<shortcut-id>`
        // ambiguous, so a press could route to the wrong extension.
        assert!(
            scripted(r#","contributes":{"shortcuts":[{"id":"go:now","label":"Go"}]}"#).is_err()
        );
        assert!(scripted(r#","contributes":{"shortcuts":[{"id":"go","label":"Go"}]}"#).is_ok());
        assert!(pack(
            r#"{"manifest":{"id":"com.x:y","name":"n","version":"1","tier":"scripted",
                "entry_source":"x","contributes":{"shortcuts":[{"id":"go","label":"Go"}]}}}"#
        )
        .is_err());
        // Data packs have no code, so they cannot declare surfaces or
        // contributions — but they CAN claim a slot (a pill theme does).
        assert!(pack(
            r#"{"manifest":{"id":"com.x.t","name":"T","version":"1","tier":"pack",
                "contributes":{"shortcuts":[{"id":"a","label":"A"}]}}}"#
        )
        .is_err());
        assert_eq!(
            pack(
                r#"{"manifest":{"id":"com.x.t","name":"T","version":"1","tier":"pack","slots":["pill.theme"]}}"#
            ),
            Ok(())
        );
    }

    /// Forward-compatibility (SPEC §4.1/§4.3): a pack written against a NEWER
    /// contract must still install with its known subset — never be rejected
    /// and never lose settings.
    #[test]
    fn newer_contract_settings_degrade_instead_of_failing() {
        let json = r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"scripted",
            "entry_source":"x","contributes":{"settings":[
                {"key":"hue","label":"Hue","kind":"color"},
                {"key":"mix","label":"Mix","kind":"slider","min":0,"max":1,"step":0.1},
                {"key":"cols","label":"Cols","kind":"rows"},
                {"key":"far","label":"Far","kind":"bool","anchor":"some.future.anchor"}
            ]}}}"#;
        let p: GrainPack = serde_json::from_str(json).expect("unknown kinds must still parse");
        // An unknown kind degrades to Unsupported rather than killing the pack.
        assert_eq!(
            p.manifest.contributes.settings[2].kind,
            SettingKind::Unsupported
        );
        assert_eq!(p.manifest.contributes.settings[0].kind, SettingKind::Color);
        // An unknown anchor is accepted; the host falls back to the extension's
        // own section (SPEC §4.3 — settings are never lost).
        assert_eq!(p.validate(), Ok(()));
        assert!(!ANCHORS.contains(&"some.future.anchor"));
    }

    /// The v1 anchor list is contract surface copied from SPEC §4.3 — a typo or
    /// an invented anchor here is a promise we cannot take back.
    #[test]
    fn anchor_list_matches_the_spec_v1_set() {
        assert_eq!(
            ANCHORS,
            &[
                "snippets.after",
                "dictation.pipeline.after",
                "context.after",
                "agent.after",
                "models.after",
            ]
        );
    }
}
