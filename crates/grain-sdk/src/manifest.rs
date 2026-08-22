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
    #[serde(default, rename = "grainApi", alias = "grain_api")]
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub entry_source: String,
    /// [GRAIN] Phase 3 (SPEC §1.2): surfaces the extension DECLARES. Extensions
    /// never create windows — the host builds, places, sleeps and destroys them.
    #[serde(default)]
    pub surfaces: Surfaces,
    /// [GRAIN] Phase 3 (SPEC §3): exclusive positions claimed. At most one
    /// enabled occupant per slot; claiming an occupied slot prompts a takeover.
    #[serde(default)]
    pub slots: Vec<String>,
    /// [GRAIN] SPEC §10.2 surface variants: positions the pack *offers* itself
    /// for rather than claims. Enabling adds it to a host-owned chooser; a core
    /// setting decides occupancy, so enabling alone changes no occupant and is
    /// not a takeover. The Agent centre layout is the canonical example
    /// (occupancy = `agent_panel_position`). Externalised in Phase 5C: a real
    /// pack declares this instead of the host synthesising it.
    #[serde(default)]
    pub variant_slots: Vec<String>,
    /// [GRAIN] Phase 3 (SPEC §4): declarative contributions the host renders or
    /// registers on the extension's behalf.
    #[serde(default)]
    pub contributes: Contributes,
    /// [GRAIN] Phase 4 Tier-C companion binaries. Native manifests remain
    /// developer-only until signed distribution lands in Phase 5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion: Option<CompanionDecl>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CompanionDecl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<String>,
}

impl CompanionDecl {
    pub fn current_platform(&self) -> Option<&str> {
        #[cfg(target_os = "windows")]
        return self.windows.as_deref();
        #[cfg(target_os = "macos")]
        return self.macos.as_deref();
        #[cfg(target_os = "linux")]
        return self.linux.as_deref();
        #[allow(unreachable_code)]
        None
    }

    fn has_any(&self) -> bool {
        self.windows.is_some() || self.macos.is_some() || self.linux.is_some()
    }
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
    #[serde(
        default,
        rename = "minSize",
        alias = "min_size",
        skip_serializing_if = "Option::is_none"
    )]
    pub min_size: Option<[u32; 2]>,
    /// The workspace UI as a self-contained HTML document, embedded so a
    /// scripted pack stays one shareable file.
    ///
    /// It is loaded into a **sandboxed iframe** — opaque origin, no Tauri IPC,
    /// no reach into the page around it (SPEC §7.1: a UI surface gets its own
    /// realm). That surrounding page is Grain's code and is the only thing
    /// holding the surface token, so the extension's own markup cannot forge an
    /// identity by asserting one in a payload.
    #[serde(default, rename = "uiSource", alias = "ui_source")]
    pub ui_source: String,
}

/// A transient HUD: created per invocation, destroyed on dismiss. The host
/// enforces the size and lifetime budget — an overlay cannot linger.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayDecl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[u32; 2]>,
    /// Auto-dismiss budget; the host caps this regardless of what is asked.
    #[serde(
        default,
        rename = "timeoutMs",
        alias = "timeout_ms",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout_ms: Option<u32>,
    /// The overlay UI as a self-contained HTML document, rendered into the same
    /// sandboxed iframe a workspace uses (SPEC §7.1). Embedded so the pack stays
    /// one shareable file.
    #[serde(default, rename = "uiSource", alias = "ui_source")]
    pub ui_source: String,
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
    /// One host-owned recording mode. Its suggested binding starts/stops the
    /// serialized capture session; the extension owns only the bounded slow
    /// stage after transcription.
    #[serde(
        default,
        rename = "sessionMode",
        alias = "session_mode",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_mode: Option<SessionModeDecl>,
    /// [GRAIN] Lines added to the dictation prompt when a surface matches.
    ///
    /// Declared as an ARRAY even though most packs ship one: the motivating
    /// case is per-app rules (STRESS-TEST §4b, "App Modes as an extension"),
    /// which is many narrow layers rather than one broad one. Accepts the
    /// singular `promptLayer` spelling from the SPEC example as an alias.
    #[serde(
        default,
        rename = "promptLayers",
        alias = "promptLayer",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub prompt_layers: Vec<PromptLayerDecl>,
    /// [GRAIN] Things this extension can DO when the user asks for them out
    /// loud (`docs/Action Routing/PLAN.md`).
    ///
    /// Declared, never claimed. The extension supplies ways a request might be
    /// phrased; the HOST ranks every installed action against what was actually
    /// said and picks. That is the whole reason two music extensions can coexist
    /// without one of them owning the word "play".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionDecl>,
}

/// One contributed prompt layer (SPEC §4, STRESS-TEST GAP-4).
///
/// # Why this shape and not a callback
///
/// The text is **static and lives in the manifest**. That is the entire
/// security argument for letting an inert pack contribute to the prompt at all:
///
/// - It is reviewable. A store reviewer, and the user in the permission sheet,
///   read the exact string the model will receive.
/// - It cannot carry anything fetched. Text an extension pulls off a web page or
///   an API is evidence, never an instruction — see `docs/Prompt Priority/PLAN.md`
///   §T4. A static declaration makes that structural rather than a promise.
/// - Matching is done by the HOST against context it already has, so the
///   extension is never told what application the user is in, and is never told
///   whether it matched (§T7). Contributing a layer is strictly weaker than the
///   `context:app` capability, and must stay that way.
///
/// The text is also part of the **permission surface**: changing it in an update
/// holds the extension until the user approves the new wording, because approval
/// that does not survive an update is approval of a version nobody runs (§T1,
/// CVE-2025-54136).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PromptLayerDecl {
    /// Stable id, unique within the extension. Appears in logs and in the
    /// user-visible stack, so it should read as a name.
    pub id: String,
    /// The surfaces this layer applies to. An EMPTY match applies everywhere —
    /// allowed, and deliberately the loudest thing in the permission sheet.
    #[serde(default)]
    pub when: LayerWhen,
    /// The instruction, verbatim. Never framed by the extension: the host writes
    /// every header and every scoping sentence around it.
    pub text: String,
}

/// Host-evaluated match conditions. Every field is a set; a layer applies when
/// EVERY non-empty field has a member that matches, so fields intersect and
/// members alternate.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct LayerWhen {
    /// Executable stems (`code`) or AppUserModelIDs, matched the same way the
    /// user's own context profiles match an application.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app: Vec<String>,
    /// Address-bar hosts. `jira.` is a prefix wildcard, exactly as in Grain's
    /// own site table, and matching is dot-bounded so `notgithub.com` cannot
    /// inherit `github.com`'s layer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub website: Vec<String>,
    /// Detected profile names: `email`, `work`, `casual`, `technical`,
    /// `ai_chat`, `other`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub category: Vec<String>,
    /// `single_line` or `multi_line`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl LayerWhen {
    pub fn is_unconditional(&self) -> bool {
        self.app.is_empty()
            && self.website.is_empty()
            && self.category.is_empty()
            && self.field.is_none()
    }
}

/// Hard ceiling on one layer's text, in bytes.
///
/// Not a suggestion and not per-extension configurable. Instruction-following
/// degrades with length well before any context limit, and these ride on every
/// matching dictation in front of models Grain deliberately keeps small. The
/// number is the same order as Grain's own profile instructions (~340 bytes),
/// with room for a rule that genuinely needs two sentences.
pub const PROMPT_LAYER_MAX_BYTES: usize = 600;

/// Hard ceiling on how many layers ONE extension may declare.
///
/// Bounds the review surface, not just the prompt: a pack with forty rules is
/// one nobody reads before approving.
pub const PROMPT_LAYERS_MAX_PER_EXTENSION: usize = 12;

/// The detected profiles a `when.category` may name. Mirrors `AppCategory` in
/// the host; kept as strings because the sdk is the dependency leaf and must not
/// learn about context detection.
const LAYER_CATEGORIES: &[&str] = &["email", "work", "casual", "technical", "ai_chat", "other"];

/// Structural validation of contributed prompt layers.
///
/// Structural ONLY. Whether the text tries to talk its way up the ladder is a
/// question about Grain's prompt, so it is asked by the host next to the prompt
/// code (`prompt_stack::screen_contributed_text`) rather than duplicated here
/// against a copy of the header list that would drift.
fn validate_prompt_layers(layers: &[PromptLayerDecl]) -> Result<(), String> {
    if layers.len() > PROMPT_LAYERS_MAX_PER_EXTENSION {
        return Err(format!(
            "an extension may declare at most {PROMPT_LAYERS_MAX_PER_EXTENSION} prompt layers"
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for layer in layers {
        let id = layer.id.trim();
        if id.is_empty() {
            return Err("a prompt layer is missing its id".into());
        }
        // `:` is the namespacing separator everywhere else in the contract, so
        // allowing it here would make a layer's qualified name ambiguous.
        if id.contains(':') {
            return Err(format!("prompt layer id '{id}' must not contain ':'"));
        }
        if !seen.insert(id) {
            return Err(format!("duplicate prompt layer id '{id}'"));
        }
        let text = layer.text.trim();
        if text.is_empty() {
            return Err(format!("prompt layer '{id}' has no text"));
        }
        if text.len() > PROMPT_LAYER_MAX_BYTES {
            return Err(format!(
                "prompt layer '{id}' is {} bytes; the limit is {PROMPT_LAYER_MAX_BYTES}",
                text.len()
            ));
        }
        // What the reviewer read must be what the model receives. Bidi overrides
        // and zero-width characters break that equivalence, and no legitimate
        // instruction needs them.
        if let Some(bad) = text.chars().find(|c| is_deceptive_char(*c)) {
            return Err(format!(
                "prompt layer '{id}' contains a control or direction-override character (U+{:04X})",
                bad as u32
            ));
        }
        for category in &layer.when.category {
            if !LAYER_CATEGORIES.contains(&category.as_str()) {
                return Err(format!(
                    "prompt layer '{id}' names unknown category '{category}'"
                ));
            }
        }
        if let Some(field) = &layer.when.field {
            if field != "single_line" && field != "multi_line" {
                return Err(format!(
                    "prompt layer '{id}' field must be 'single_line' or 'multi_line'"
                ));
            }
        }
    }
    Ok(())
}

/// Characters that let reviewed text render as one thing and tokenize as
/// another: C0/C1 controls (bar the ordinary whitespace an instruction may
/// legitimately contain), bidi overrides, and zero-width joiners/spaces.
fn is_deceptive_char(c: char) -> bool {
    if c == '\n' || c == '\t' || c == '\r' {
        return false;
    }
    c.is_control()
        || matches!(c,
            '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}')
}

// ── Actions (`contributes.actions`) ─────────────────────────────────────────
//
// See `docs/Action Routing/PLAN.md`. The shape deliberately mirrors
// `PromptLayerDecl`: static text in the manifest, a host-evaluated `when`, and
// everything here inside the approval digest. Two things differ, and both are
// because an action has a SIDE EFFECT where a prompt layer has only wording:
//
//   · `risk` is required. An action's blast radius is never implicit, and a
//     silent `confirm → safe` downgrade in an update is exactly the attack the
//     digest exists to close.
//   · `title` is required and is what the permission sheet shows. The utterance
//     list never reaches a consent surface — "what can this do" is the consent
//     question, not "what words".
//
// The extension is never told the transcript unless one of its actions wins,
// and then it receives only the extracted spans. Losing the route means
// learning nothing, exactly as with prompt layers.

/// One action an extension can perform when the user asks for it by voice.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActionDecl {
    /// Stable id, unique within the extension. Appears in the action log and in
    /// `doctor` output, so it should read as a name.
    pub id: String,
    /// One plain line, shown in the permission sheet and the chooser. Written
    /// for the user, not the model: "Skip to the next track".
    pub title: String,
    /// Which preference group this belongs to — the key behind "always use
    /// Spotify for media", the chooser's heading, and the scope within which
    /// two extensions' actions can be recognised as the same request.
    ///
    /// NOT a fulfilment contract: it has no parameters and no version, and
    /// getting it wrong costs a mis-grouped default, never a broken extension.
    pub domain: String,
    /// Whether performing this needs a read-back first. Required, and never
    /// inferred — see [`ActionRisk`].
    pub risk: ActionRisk,
    /// Surfaces this action is offered on. Empty = everywhere. Reuses the
    /// prompt-layer matcher verbatim, so "next slide" can be scoped to a deck
    /// without the extension ever learning what application the user is in.
    #[serde(default)]
    pub when: LayerWhen,
    /// Ways a user might ask for this, in English. Ranked by the HOST against
    /// what was said; the extension neither sees the utterance nor learns
    /// whether it matched.
    ///
    /// `{param}` placeholders name a declared parameter and mark where its span
    /// begins. Locale sets are a later additive field (`utterancesByLocale`),
    /// reserved so nobody squats the name; this list is the `en` set.
    #[serde(default)]
    pub utterances: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ActionParamDecl>,
    /// Short guidance hydrated into the Agent's context *only* when this action
    /// is retrieved for a turn. Never in the permanent prompt.
    #[serde(
        default,
        rename = "agentRules",
        alias = "agent_rules",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent_rules: Option<String>,
}

/// Irreversibility, which is a separate axis from confidence.
///
/// A high similarity score is evidence about the transcript, never about the
/// speech: ASR substitutions reverse intent ("cancel my order" heard as
/// "schedule my order"), score well, and parse cleanly. So `Confirm` is not
/// confidence-gated and no score retires it.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActionRisk {
    /// Cheap and obvious when wrong: skip a track, open an app, set volume.
    Safe,
    /// Sends, deletes, spends, posts. Always reads the resolved action back
    /// before running it, however confident the router was.
    Confirm,
}

impl ActionRisk {
    pub fn is_safe(self) -> bool {
        matches!(self, ActionRisk::Safe)
    }
}

/// One parameter, filled from a span of what the user said.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActionParamDecl {
    pub name: String,
    pub kind: ActionParamKind,
    /// Entity only: the host hands over the raw span and the EXTENSION resolves
    /// it against its own catalogue, returning zero, one, or several
    /// candidates. Grain does not know Spotify's library and never should.
    #[serde(default)]
    pub resolve: bool,
    /// A required parameter with an empty span sends the route to the chooser
    /// rather than guessing.
    #[serde(default = "default_true")]
    pub required: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActionParamKind {
    /// Something in the extension's own data: an artist, a contact, a playlist.
    Entity,
    /// Free text, passed through verbatim — a message body, a note.
    Text,
    /// A bare number: a volume, a count.
    Number,
}

fn default_true() -> bool {
    true
}

/// The preference groups an action may name. Short, closed, and host-owned; a
/// new one is added when a second provider appears in a new area.
///
/// Distinct from [`LAYER_CATEGORIES`], which classifies the surface the user is
/// typing into. Similar-looking, unrelated vocabulary — naming one where the
/// other belongs is rejected with that sentence.
pub const ACTION_DOMAINS: &[&str] = &[
    "media",
    "messaging",
    "mail",
    "calendar",
    "issues",
    "files",
    "browser",
    "notes",
    "system",
];

/// Hard ceiling on actions per extension. Bounds the review surface and the
/// permission sheet, not just the index.
pub const ACTIONS_MAX_PER_EXTENSION: usize = 24;

/// Hard ceiling on utterances per action. More phrasings help recall and hurt
/// separation — past this an action starts eating its neighbours' language,
/// and ranking is global, so a greedy author degrades everyone.
pub const ACTION_UTTERANCES_MAX: usize = 24;

/// Hard ceiling on one utterance, in bytes. An utterance is a way of asking for
/// something, not a sentence of prose.
pub const ACTION_UTTERANCE_MAX_BYTES: usize = 120;

/// Hard ceiling on the permission-sheet line.
pub const ACTION_TITLE_MAX_BYTES: usize = 80;

/// Hard ceiling on `agentRules`, which ride into the Agent's context.
pub const ACTION_AGENT_RULES_MAX_BYTES: usize = 300;

/// Hard ceiling on parameters per action. An action needing five spans out of
/// one spoken sentence is an Agent turn, not a route.
pub const ACTION_PARAMS_MAX: usize = 4;

/// Capabilities whose effect leaves the machine or changes the user's data, and
/// which therefore cannot be driven by an unbounded span of what Grain *thought*
/// it heard. See [`validate_actions`].
const SIDE_EFFECT_CAPABILITIES: &[&str] = &["open:url", "open:app", "notes"];

/// One piece of a parsed utterance template.
///
/// Lives in the sdk because the host's span extraction and `doctor`'s
/// validation must agree on where a parameter starts, and two parsers would
/// drift.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UtterancePart {
    Literal(String),
    Param(String),
}

/// Split an utterance into literals and `{param}` placeholders.
pub fn parse_utterance(utterance: &str) -> Result<Vec<UtterancePart>, String> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = utterance;
    while let Some(open) = rest.find('{') {
        literal.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after
            .find('}')
            .ok_or_else(|| format!("unclosed '{{' in \"{utterance}\""))?;
        let name = &after[..close];
        if name.is_empty() {
            return Err(format!("empty placeholder in \"{utterance}\""));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(format!(
                "placeholder '{{{name}}}' must be lowercase letters, digits or '_'"
            ));
        }
        if !literal.trim().is_empty() {
            parts.push(UtterancePart::Literal(literal.trim().to_string()));
        }
        literal.clear();
        parts.push(UtterancePart::Param(name.to_string()));
        rest = &after[close + 1..];
    }
    if rest.contains('}') {
        return Err(format!("unmatched '}}' in \"{utterance}\""));
    }
    literal.push_str(rest);
    if !literal.trim().is_empty() {
        parts.push(UtterancePart::Literal(literal.trim().to_string()));
    }
    Ok(parts)
}

/// Structural validation of contributed actions.
///
/// Takes the extension's capabilities because one rule genuinely needs them:
/// a free-text span feeding a side-effecting sink cannot be `safe`. Everything
/// else here is shape and ceilings.
fn validate_actions(actions: &[ActionDecl], permissions: &[String]) -> Result<(), String> {
    if actions.len() > ACTIONS_MAX_PER_EXTENSION {
        return Err(format!(
            "an extension may declare at most {ACTIONS_MAX_PER_EXTENSION} actions"
        ));
    }
    // A free-text span becomes an argument to whatever this extension can
    // reach. `net:` counts: an exact-host grant still means the span is sent
    // somewhere. A RESOLVED entity does not, because the extension matched it
    // against its own catalogue first, so the value came from a bounded set.
    let has_sink = permissions.iter().any(|p| {
        SIDE_EFFECT_CAPABILITIES.contains(&p.as_str()) || network_capability_host(p).is_some()
    });

    let mut seen = std::collections::HashSet::new();
    for action in actions {
        let id = action.id.trim();
        if id.is_empty() {
            return Err("an action is missing its id".into());
        }
        // `:` namespaces everywhere else in the contract; allowing it here makes
        // a qualified action name ambiguous.
        if id.contains(':') {
            return Err(format!("action id '{id}' must not contain ':'"));
        }
        if !seen.insert(id) {
            return Err(format!("duplicate action id '{id}'"));
        }

        let title = action.title.trim();
        if title.is_empty() {
            return Err(format!("action '{id}' has no title"));
        }
        if title.len() > ACTION_TITLE_MAX_BYTES {
            return Err(format!(
                "action '{id}' title is {} bytes; the limit is {ACTION_TITLE_MAX_BYTES}",
                title.len()
            ));
        }

        if !ACTION_DOMAINS.contains(&action.domain.as_str()) {
            // The two vocabularies look alike and mean different things, so say
            // which one was reached for rather than just "unknown".
            if LAYER_CATEGORIES.contains(&action.domain.as_str()) {
                return Err(format!(
                    "action '{id}' names '{}', which is a prompt-layer category (the surface the \
                     user is typing into), not an action domain (which provider performs this)",
                    action.domain
                ));
            }
            return Err(format!(
                "action '{id}' names unknown domain '{}'; expected one of: {}",
                action.domain,
                ACTION_DOMAINS.join(", ")
            ));
        }

        if let Some(rules) = &action.agent_rules {
            if rules.len() > ACTION_AGENT_RULES_MAX_BYTES {
                return Err(format!(
                    "action '{id}' agentRules is {} bytes; the limit is \
                     {ACTION_AGENT_RULES_MAX_BYTES}",
                    rules.len()
                ));
            }
            if let Some(bad) = rules.chars().find(|c| is_deceptive_char(*c)) {
                return Err(format!(
                    "action '{id}' agentRules contains a control or direction-override character \
                     (U+{:04X})",
                    bad as u32
                ));
            }
        }

        validate_action_when(id, &action.when)?;
        let declared = validate_action_params(id, &action.params, has_sink, action.risk)?;
        validate_action_utterances(id, &action.utterances, &declared, &action.params)?;
    }
    validate_no_self_collision(actions)?;
    Ok(())
}

/// Two of ONE extension's own actions declaring the same phrase.
///
/// Rejected, because it is unambiguously a bug rather than a judgement call:
/// ranking is global and deterministic, so one of the two can never win, and
/// the author has made their own action unreachable without being told.
///
/// The interesting case — two *different* extensions declaring the same phrase —
/// is deliberately NOT an error. That is two providers of one request, which is
/// a legitimate steady state resolved by provider selection, and it cannot be
/// seen from inside one manifest anyway.
fn validate_no_self_collision(actions: &[ActionDecl]) -> Result<(), String> {
    let mut owner: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for action in actions {
        for utterance in &action.utterances {
            // Compare on the literal skeleton, so `play {artist}` and
            // `play {track}` collide — the parameter's NAME is the author's
            // private business and changes nothing about what the router hears.
            let Ok(parts) = parse_utterance(utterance.trim()) else {
                continue;
            };
            let skeleton = parts
                .iter()
                .map(|part| match part {
                    UtterancePart::Literal(l) => l.to_lowercase(),
                    UtterancePart::Param(_) => "\u{1}".into(),
                })
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(first) = owner.insert(skeleton, action.id.trim()) {
                if first != action.id.trim() {
                    return Err(format!(
                        "actions '{first}' and '{}' both claim \"{}\", so one of them can never \
                         win",
                        action.id.trim(),
                        utterance.trim()
                    ));
                }
            }
        }
    }
    Ok(())
}

/// `when` reuses the prompt-layer vocabulary, so it reuses its checks too.
fn validate_action_when(id: &str, when: &LayerWhen) -> Result<(), String> {
    for category in &when.category {
        if !LAYER_CATEGORIES.contains(&category.as_str()) {
            return Err(format!("action '{id}' names unknown category '{category}'"));
        }
    }
    if let Some(field) = &when.field {
        if field != "single_line" && field != "multi_line" {
            return Err(format!(
                "action '{id}' field must be 'single_line' or 'multi_line'"
            ));
        }
    }
    Ok(())
}

/// Returns the declared parameter names, in declaration order.
fn validate_action_params(
    id: &str,
    params: &[ActionParamDecl],
    has_sink: bool,
    risk: ActionRisk,
) -> Result<Vec<String>, String> {
    if params.len() > ACTION_PARAMS_MAX {
        return Err(format!(
            "action '{id}' declares {} parameters; the limit is {ACTION_PARAMS_MAX}",
            params.len()
        ));
    }
    let mut names = Vec::with_capacity(params.len());
    for param in params {
        let name = param.name.trim();
        if name.is_empty() {
            return Err(format!("action '{id}' has a parameter with no name"));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(format!(
                "action '{id}' parameter '{name}' must be lowercase letters, digits or '_'"
            ));
        }
        if names.iter().any(|existing| existing == name) {
            return Err(format!("action '{id}' has duplicate parameter '{name}'"));
        }
        if param.resolve && param.kind != ActionParamKind::Entity {
            return Err(format!(
                "action '{id}' parameter '{name}' sets resolve on a non-entity parameter"
            ));
        }
        // The span is whatever Grain THOUGHT it heard. Unbounded, it becomes an
        // argument to a sink the extension can already reach — "open {url}" is
        // "open whatever Grain mishears". A resolved entity is bounded by the
        // extension's own catalogue, so it does not trip this.
        let unbounded = param.kind == ActionParamKind::Text
            || (param.kind == ActionParamKind::Entity && !param.resolve);
        if unbounded && has_sink && risk.is_safe() {
            return Err(format!(
                "action '{id}' takes free text in '{name}' and this extension can act outside \
                 Grain, so it cannot declare risk 'safe' — use 'confirm', or resolve the \
                 parameter against the extension's own data"
            ));
        }
        names.push(name.to_string());
    }
    Ok(names)
}

fn validate_action_utterances(
    id: &str,
    utterances: &[String],
    declared: &[String],
    params: &[ActionParamDecl],
) -> Result<(), String> {
    if utterances.is_empty() {
        return Err(format!("action '{id}' declares no utterances"));
    }
    if utterances.len() > ACTION_UTTERANCES_MAX {
        return Err(format!(
            "action '{id}' declares {} utterances; the limit is {ACTION_UTTERANCES_MAX}",
            utterances.len()
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let mut mentioned = std::collections::HashSet::new();
    for utterance in utterances {
        let trimmed = utterance.trim();
        if trimmed.is_empty() {
            return Err(format!("action '{id}' has an empty utterance"));
        }
        if trimmed.len() > ACTION_UTTERANCE_MAX_BYTES {
            return Err(format!(
                "action '{id}' utterance \"{trimmed}\" is {} bytes; the limit is \
                 {ACTION_UTTERANCE_MAX_BYTES}",
                trimmed.len()
            ));
        }
        // Same equivalence rule as prompt-layer text: what the reviewer read
        // must be what the router matches.
        if let Some(bad) = trimmed.chars().find(|c| is_deceptive_char(*c)) {
            return Err(format!(
                "action '{id}' utterance contains a control or direction-override character \
                 (U+{:04X})",
                bad as u32
            ));
        }
        let normalised = trimmed.to_lowercase();
        if !seen.insert(normalised) {
            return Err(format!("action '{id}' repeats the utterance \"{trimmed}\""));
        }
        let parts = parse_utterance(trimmed).map_err(|e| format!("action '{id}': {e}"))?;
        let mut has_literal = false;
        for part in &parts {
            match part {
                UtterancePart::Literal(_) => has_literal = true,
                UtterancePart::Param(name) => {
                    if !declared.iter().any(|d| d == name) {
                        return Err(format!(
                            "action '{id}' utterance \"{trimmed}\" uses undeclared parameter \
                             '{{{name}}}'"
                        ));
                    }
                    mentioned.insert(name.clone());
                }
            }
        }
        // A bare placeholder matches every utterance ever spoken. Ranking is
        // global, so this is not the author's problem to discover in the wild.
        if !has_literal {
            return Err(format!(
                "action '{id}' utterance \"{trimmed}\" is only a placeholder, so it would match \
                 anything the user says"
            ));
        }
    }
    // A required parameter that appears in no template can never be filled, so
    // the action would route and then always fall to the chooser.
    for param in params {
        if param.required && !mentioned.contains(param.name.trim()) {
            return Err(format!(
                "action '{id}' requires '{}' but no utterance shows where it appears",
                param.name.trim()
            ));
        }
    }
    Ok(())
}

/// A recording mode contributed by one extension (SPEC §1.3, §3.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionModeDecl {
    pub id: String,
    pub label: String,
    #[serde(
        default,
        rename = "defaultBinding",
        alias = "default_binding",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_binding: Option<String>,
}

/// One schema-declared setting (SPEC §4, levels 1–2).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
    /// Write-only credential. The host stores it outside extension settings
    /// and returns only a redacted marker to UI and extension code.
    Secret,
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
    /// [GRAIN] Phase 5C — reusable structured primitives, so an extension can
    /// build a rich native config (workflows, rules, mappings) at an anchor with
    /// no webview.
    ///
    /// A **repeatable list of rows**. Each row is a group of the declared
    /// `fields` (themselves any `SettingKind`, so lists nest). The stored value
    /// is an array of objects keyed by each field's `key`. The single
    /// most-requested primitive — this is what lets Voice Actions (many
    /// workflows, each opening several targets) be declared rather than coded.
    List {
        fields: Vec<SettingDecl>,
        /// Singular noun for the "Add" button and row header (e.g. "action",
        /// "target"). Defaults to "item".
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "itemLabel",
            alias = "item_label"
        )]
        item_label: Option<String>,
    },
    /// A filesystem path to an application, chosen through the host's **native
    /// picker**. Picking is the user-mediated act that also records the path as
    /// approved for `open:app` — an extension can never launch a path the user
    /// did not choose here.
    #[serde(rename = "app_path")]
    AppPath,
    /// A URL field. The host validates the scheme against the same allowlist as
    /// `open:url` (http/https/mailto/tel), so a stored value is always safe to
    /// open and a typo is caught on entry.
    Url,
    /// [GRAIN] A CUSTOM CARD (SPEC §4.1 Level 3): the extension's own HTML/JS,
    /// rendered in a sandboxed iframe where the declarative controls can't
    /// express the UI. Unlike every other kind it stores NO value — it manages
    /// its own state through `grain.settings`/`grain.storage`. It carries the
    /// shared `anchor`, so a card renders at a feature (e.g. `snippets.after`)
    /// or in the extension's own section. Same opaque-origin sandbox a workspace
    /// surface uses (SPEC §7.1); every host call it makes is capability-checked
    /// in Rust, exactly like a worker's.
    Panel {
        /// The card UI as a self-contained HTML document, embedded so the pack
        /// stays one shareable file.
        #[serde(default, rename = "uiSource", alias = "ui_source")]
        ui_source: String,
        /// Words settings-search must find this card by — the host cannot read
        /// inside the iframe, so without these the card is unsearchable. At
        /// least one non-blank term is required at import.
        #[serde(default, rename = "searchTerms", alias = "search_terms")]
        search_terms: Vec<String>,
    },
    /// A kind this build doesn't know (SPEC §4.1). Without this, one unknown
    /// kind makes the WHOLE pack fail to deserialize — a manifest written
    /// against a newer contract must still install with its known subset. The
    /// host skips rendering it.
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
    #[serde(
        default,
        rename = "defaultBinding",
        alias = "default_binding",
        skip_serializing_if = "Option::is_none"
    )]
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
    "grainspace.after",
    "models.after",
];

/// The surface a prompt payload extends: Grain's post-processing prompt list.
///
/// Payload packs are the one shape that declares nothing structural — no slot
/// to claim, no settings row to anchor — so without a name for what they feed
/// they resolve nowhere, and a prompt pack ends up placed by guesswork.
pub const SURFACE_PROMPTS: &str = "dictation.prompts";

fn push_surface(out: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !out.iter().any(|existing| existing == value) {
        out.push(value.to_string());
    }
}

/// Exclusive positions (SPEC §3). Core defaults are occupants too, so a claim
/// on any of these can displace a shipped feature — never silently.
pub const KNOWN_SLOTS: &[&str] = &[
    "overlay.recording",
    "overlay.pointer",
    "pill.theme",
    "agent.reply-surface",
    "output.destination",
    PROMPT_CONTEXT_SLOT,
];

/// The soft-context line in the dictation prompt: **Grain's opinion about how to
/// write for the surface it detected**.
///
/// Claimable because it is a guess, and an extension may guess better. The three
/// layers around it are not:
///
/// - the user's own prompt and their custom profiles are theirs, and an
///   extension replacing them is the one thing the ladder exists to prevent;
/// - a spoken instruction is the user's voice about this transcript;
/// - the single-line rule, the cursor fit and nearby terms are structural facts
///   about the field, not opinions about writing, so there is nothing there to
///   disagree with.
///
/// Claiming it means taking responsibility for it: Grain then says nothing about
/// the surface, and if the claimant contributes no matching layer, no soft
/// context is sent at all.
pub const PROMPT_CONTEXT_SLOT: &str = "prompt.context";

/// Capabilities a scripted pack may request in API 1.0. Anything outside this
/// set is rejected at import (R1: grant narrowly, widen with each consumer).
/// Parameterised `net:<host>` grants are validated separately below.
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
    // Phase 3 (Grain Space Test): read the user's current selection — the
    // quick-add path a note-capture extension needs. Sensitive (it reads
    // whatever is selected in any app), so it is its own grant, meant to be
    // paired with a user-initiated trigger like a shortcut.
    "capture:selection",
    // [GRAIN] Read and change the user's Grain Space notes. The widest-reaching
    // grant the platform has — it is everything they have written down — so it
    // is flagged and its permission sheet says so in those words.
    //
    // It exists because a note VIEWER is a legitimate extension: Grain's own is
    // one (NOTE-UI-EXTENSION-PLAN.md), and so is a third-party publisher, sync
    // bridge or alternative editor. Refusing to have the capability would not
    // make the platform safer, only unable to express what people will build.
    //
    // Distinct from the MCP bridge's `space`, which is absent from this list on
    // purpose: that one is minted by Grain for its own proxy and can never be
    // requested by a manifest.
    "notes",
    // Phase 5C: observe the foreground application (name, executable, and the
    // browser URL host when it is a browser). Privacy-marked — it reveals which
    // app the user is in — and the foundation any context-aware extension needs
    // (the same primitive Grain's own Context Awareness uses).
    "capture:app",
    // [GRAIN] Read the foreground window's visible text from its accessibility
    // tree. Wider than `capture:selection` (which is only what the user
    // deliberately highlighted) and narrower than a screenshot: no image is ever
    // produced, no screen-recording permission is involved, other applications
    // are never in frame, and password fields are skipped. Flagged, because it
    // can still see whatever is on the window the user is looking at.
    "capture:screen-text",
    // [GRAIN] Capture the foreground window as an IMAGE. The widest-reaching
    // capture grant there is: a frame carries whatever happened to be on that
    // window, and unlike the accessibility tree there is no per-element flag to
    // skip a password with. Grain's own code never calls it — the capability
    // exists so an extension the user installed and granted deliberately can,
    // which keeps that decision with a component they opted into rather than
    // with the dictation app running all day. Flagged, and its permission sheet
    // says what a screenshot is in those words.
    "capture:screen-image",
    // Phase 5C (SPEC §1.3): launch side effects. Both are danger-marked and the
    // host enforces the security, not the extension:
    //  · `open:url` opens a link in the user's browser — the host allows ONLY
    //    http/https/mailto/tel schemes (never file:, javascript:, custom
    //    handlers, …), the exact scheme-allowlist lesson from a decade of
    //    Electron `openExternal` RCEs, and never touches a shell.
    //  · `open:app` launches a local application — but ONLY one the user picked
    //    through Grain's own native chooser; the extension can never launch an
    //    arbitrary path or its own bundled binary.
    "open:url",
    "open:app",
];

/// Parameterised network grants are deliberately narrower than URLs: exactly
/// one canonical host, with no scheme, port, path, wildcard, or suffix match.
pub fn network_capability_host(capability: &str) -> Option<&str> {
    let host = capability.strip_prefix("net:")?;
    if host.is_empty()
        || host.len() > 253
        || host != host.to_ascii_lowercase()
        || host.contains('*')
        || host.ends_with('.')
    {
        return None;
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Some(host);
    }
    let valid = host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
    });
    valid.then_some(host)
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

impl ExtensionManifest {
    /// The host surfaces this extension extends, in declaration order.
    ///
    /// This is the single answer to "where in Grain does this belong?", and it
    /// is derived only from what the manifest *declares* — the slots it claims
    /// or offers itself for ([`KNOWN_SLOTS`]) and the anchors its settings rows
    /// attach to ([`ANCHORS`]).
    ///
    /// It exists because that question was previously answered two different
    /// ways. Routing an installed card read anchors, while the store recommended
    /// extensions by scoring keywords against their name and description — so a
    /// prompt pack was recommended under Agent (its description says "prompt")
    /// and App Modes was recommended nowhere (it never says "context"), even
    /// though its manifest anchors it to `context.after` in as many words.
    /// Prose is not a placement contract; these declarations are.
    pub fn extends(&self) -> Vec<String> {
        let mut out = Vec::new();
        for slot in self.slots.iter().chain(self.variant_slots.iter()) {
            push_surface(&mut out, slot);
        }
        for decl in &self.contributes.settings {
            if let Some(anchor) = &decl.anchor {
                push_surface(&mut out, anchor);
            }
        }
        out
    }
}

/// The `.grainpack` file: manifest + payloads in one JSON document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GrainPack {
    pub manifest: ExtensionManifest,
    #[serde(default)]
    pub payloads: PackPayloads,
}

impl GrainPack {
    /// [`ExtensionManifest::extends`] plus the surfaces this pack's payloads
    /// feed — a tier-A pack contributes by shipping data, not by declaring.
    pub fn extends(&self) -> Vec<String> {
        let mut out = self.manifest.extends();
        if !self.payloads.prompts.is_empty() {
            push_surface(&mut out, SURFACE_PROMPTS);
        }
        if self.payloads.pill_theme.is_some() {
            push_surface(&mut out, "pill.theme");
        }
        out
    }

    /// Structural validation (Phase 2: tier-A packs and tier-B scripted;
    /// `native` still rejected — it arrives with the tier-C supervisor).
    pub fn validate(&self) -> Result<(), String> {
        self.validate_inner(false)
    }

    /// Load-unpacked validation is the only Phase-4 path allowed to admit a
    /// native companion. Installed/imported packs continue through `validate`.
    pub fn validate_dev(&self) -> Result<(), String> {
        self.validate_inner(true)
    }

    fn validate_inner(&self, allow_native: bool) -> Result<(), String> {
        let m = &self.manifest;
        if m.id.is_empty() || !m.id.contains('.') {
            return Err("manifest.id must be a reverse-dns identifier".into());
        }
        // `grain.` is reserved: a USER-imported pack may not claim a first-party
        // identity. Packs Grain publishes are validated on the path that installs
        // them from the signed catalogue, not through this importer.
        if m.id.starts_with("grain.") {
            return Err("the 'grain.' id prefix is reserved for built-ins".into());
        }
        if m.name.trim().is_empty() {
            return Err("manifest.name is required".into());
        }
        match m.tier {
            Tier::Native => {
                if !allow_native {
                    return Err("native extensions are developer-mode only".into());
                }
                if !m.entry_source.is_empty() {
                    return Err("native extensions must not carry entry_source".into());
                }
                if !m.companion.as_ref().is_some_and(CompanionDecl::has_any) {
                    return Err("native extensions require a companion binary map".into());
                }
                for cap in &m.permissions {
                    if !KNOWN_CAPABILITIES.contains(&cap.as_str())
                        && network_capability_host(cap).is_none()
                    {
                        return Err(format!("unknown capability '{cap}'"));
                    }
                }
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
                if m.companion.is_some() {
                    return Err("tier-A packs must not declare a companion".into());
                }
            }
            Tier::Scripted => {
                if m.entry_source.trim().is_empty() {
                    return Err("scripted extensions require entry_source".into());
                }
                if m.companion.is_some() {
                    return Err("scripted extensions must not declare a companion".into());
                }
                for cap in &m.permissions {
                    if !KNOWN_CAPABILITIES.contains(&cap.as_str())
                        && network_capability_host(cap).is_none()
                    {
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
        // Variant slots (SPEC §10.2) are offered, not claimed, but must still
        // name a real slot so a typo is caught rather than silently doing nothing.
        for slot in &m.variant_slots {
            if !KNOWN_SLOTS.contains(&slot.as_str()) {
                return Err(format!("unknown variant slot '{slot}'"));
            }
        }

        validate_prompt_layers(&m.contributes.prompt_layers)?;
        validate_actions(&m.contributes.actions, &m.permissions)?;

        // Surfaces and code-backed contributions need code to back them.
        //
        // Prompt layers are deliberately NOT in this list: static text plus a
        // host-evaluated match is exactly the kind of contribution an inert pack
        // should be able to make, and requiring a runtime for it would push
        // authors toward code they do not need — which is the more dangerous
        // outcome, not the safer one.
        let declares_surface = m.surfaces.workspace.is_some() || m.surfaces.overlay.is_some();
        let contributes_code = !m.contributes.settings.is_empty()
            || !m.contributes.shortcuts.is_empty()
            || m.contributes.session_mode.is_some()
            // Unlike a prompt layer, an action has to be PERFORMED. A pack with
            // no runtime that declares one would route, win, and then have
            // nothing to call.
            || !m.contributes.actions.is_empty();
        if (declares_surface || contributes_code) && m.tier == Tier::Pack {
            return Err("surfaces and contributes require a scripted or native runtime".into());
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

        // A surface with nothing to render is a window that opens blank and
        // cannot be explained to the user — reject it at import, not at open.
        if let Some(w) = &m.surfaces.workspace {
            if w.ui_source.trim().is_empty() {
                return Err("a workspace surface requires ui_source".into());
            }
        }
        if let Some(o) = &m.surfaces.overlay {
            if o.ui_source.trim().is_empty() {
                return Err("an overlay surface requires ui_source".into());
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
            // A custom card is the extension's own UI: it must ship markup, and
            // must be searchable (the host can't read inside its iframe).
            if let SettingKind::Panel {
                ui_source,
                search_terms,
            } = &s.kind
            {
                if ui_source.trim().is_empty() {
                    return Err(format!("panel setting '{}' requires uiSource", s.key));
                }
                if search_terms.iter().all(|t| t.trim().is_empty()) {
                    return Err(format!("panel setting '{}' requires searchTerms", s.key));
                }
                // `grain://<view-id>` renders a HOST component with the host's
                // own privileges instead of author markup in an opaque-origin
                // iframe. Only a builtin (already forced to a `grain.` id) may
                // name one; otherwise a community pack could ask to be rendered
                // as if it were Grain's own code.
            }
            if matches!(s.kind, SettingKind::Secret)
                && s.default.as_str().is_some_and(|value| !value.is_empty())
            {
                return Err(format!(
                    "secret setting '{}' cannot declare a non-empty default",
                    s.key
                ));
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

        if let Some(mode) = &m.contributes.session_mode {
            if mode.id.trim().is_empty() || mode.label.trim().is_empty() {
                return Err("a session mode requires both id and label".into());
            }
            if mode.id.contains(':') {
                return Err(format!(
                    "session mode id '{}' must not contain ':'",
                    mode.id
                ));
            }
            if !seen_sc.insert(&mode.id) {
                return Err(format!(
                    "session mode id '{}' conflicts with a shortcut id",
                    mode.id
                ));
            }
            if !m
                .permissions
                .iter()
                .any(|permission| permission == "session:start")
            {
                return Err(
                    "contributes.sessionMode requires the 'session:start' permission".into(),
                );
            }
        }

        // A pill theme (SPEC §9) is stored opaque so its schema can evolve, but
        // it is checked HERE so a malformed one is rejected at import rather than
        // silently ignored at delivery. It still degrades field-by-field once
        // valid — an unknown pattern or a partial theme is fine; a wrong shape
        // (a string, a number) is not.
        if let Some(theme) = &self.payloads.pill_theme {
            if serde_json::from_value::<crate::PillTheme>(theme.clone()).is_err() {
                return Err("payloads.pill_theme is not a valid pill theme".into());
            }
            // A theme only takes effect while the pack holds the `pill.theme`
            // slot; a theme with no claim would install and do nothing, which is
            // a packaging mistake worth catching early.
            if !m.slots.iter().any(|s| s == "pill.theme") {
                return Err("a pack shipping a pill theme must claim the 'pill.theme' slot".into());
            }
        }
        Ok(())
    }

    /// True for tier-B extensions (drive a worker), false for data packs.
    pub fn is_scripted(&self) -> bool {
        self.manifest.tier == Tier::Scripted
    }

    pub fn has_runtime(&self) -> bool {
        matches!(self.manifest.tier, Tier::Scripted | Tier::Native)
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

    fn extends_of(json: &str) -> Vec<String> {
        serde_json::from_str::<GrainPack>(json).unwrap().extends()
    }

    /// A pack declaring `promptLayers`, with the given layer array spliced in.
    fn pack_with_layers(layers: &str) -> Result<(), String> {
        pack(&format!(
            r#"{{"manifest":{{"id":"com.x.p","name":"P","version":"1.0","tier":"pack",
                "contributes":{{"promptLayers":{layers}}}}}}}"#
        ))
    }

    #[test]
    fn an_inert_pack_may_contribute_a_prompt_layer() {
        // Static text plus a host-evaluated match is exactly what a tier-A pack
        // should be able to do. Requiring a runtime for it would push authors
        // toward code they do not need, which is the more dangerous outcome.
        assert_eq!(
            pack_with_layers(
                r#"[{"id":"jira","when":{"website":["jira."]},
                     "text":"Write in imperative mood."}]"#
            ),
            Ok(())
        );
    }

    #[test]
    fn an_inert_pack_may_claim_the_context_slot() {
        // Door 2: replacing Grain's guess about a surface needs no runtime —
        // a pack that ships better wording for the same job is the point.
        assert_eq!(
            pack(&format!(
                r#"{{"manifest":{{"id":"com.x.p","name":"P","version":"1.0","tier":"pack",
                    "slots":["{PROMPT_CONTEXT_SLOT}"],
                    "contributes":{{"promptLayers":[{{"id":"a","text":"Be terse."}}]}}}}}}"#
            )),
            Ok(())
        );
    }

    #[test]
    fn the_singular_spelling_from_the_spec_still_parses() {
        assert_eq!(
            pack(
                r#"{"manifest":{"id":"com.x.p","name":"P","version":"1.0","tier":"pack",
                    "contributes":{"promptLayer":[{"id":"a","text":"Be terse."}]}}}"#
            ),
            Ok(())
        );
    }

    #[test]
    fn prompt_layers_are_bounded_in_count_and_size() {
        let many: Vec<String> = (0..PROMPT_LAYERS_MAX_PER_EXTENSION + 1)
            .map(|i| format!(r#"{{"id":"l{i}","text":"Be terse."}}"#))
            .collect();
        assert!(pack_with_layers(&format!("[{}]", many.join(","))).is_err());

        let fat = "x".repeat(PROMPT_LAYER_MAX_BYTES + 1);
        assert!(pack_with_layers(&format!(r#"[{{"id":"a","text":"{fat}"}}]"#)).is_err());
    }

    #[test]
    fn prompt_layer_text_may_not_hide_what_a_reviewer_read() {
        // A right-to-left override can make approved text render as one thing
        // and tokenize as another; no legitimate instruction needs one. Written
        // as JSON escapes because rustc refuses the raw codepoints in a source
        // literal — the same defence, one layer down.
        let bidi = format!(r#"[{{"id":"a","text":"Be terse.{}evil"}}]"#, '\u{202E}');
        assert!(pack_with_layers(&bidi).is_err());
        let zero_width = format!(r#"[{{"id":"a","text":"Be{}terse."}}]"#, '\u{200B}');
        assert!(pack_with_layers(&zero_width).is_err());
        // Ordinary line breaks are fine — an instruction may be two sentences.
        assert_eq!(
            pack_with_layers(r#"[{"id":"a","text":"Be terse.\nUse British spelling."}]"#),
            Ok(())
        );
    }

    #[test]
    fn prompt_layer_ids_and_match_values_are_checked() {
        assert!(pack_with_layers(r#"[{"id":"","text":"Be terse."}]"#).is_err());
        assert!(pack_with_layers(r#"[{"id":"a:b","text":"Be terse."}]"#).is_err());
        assert!(pack_with_layers(r#"[{"id":"a","text":"  "}]"#).is_err());
        assert!(pack_with_layers(
            r#"[{"id":"a","text":"Be terse."},{"id":"a","text":"Be brief."}]"#
        )
        .is_err());
        assert!(pack_with_layers(
            r#"[{"id":"a","when":{"category":["nope"]},"text":"Be terse."}]"#
        )
        .is_err());
        assert!(
            pack_with_layers(r#"[{"id":"a","when":{"field":"sideways"},"text":"Be terse."}]"#)
                .is_err()
        );
    }

    /// A scripted extension declaring `actions`, with permissions and the
    /// action array spliced in.
    fn pack_with_actions(permissions: &str, actions: &str) -> Result<(), String> {
        pack(&format!(
            r#"{{"manifest":{{"id":"com.x.a","name":"A","version":"1.0","tier":"scripted",
                "entry_source":"//","permissions":{permissions},
                "contributes":{{"actions":{actions}}}}}}}"#
        ))
    }

    const NEXT_TRACK: &str = r#"[{"id":"next","title":"Skip to the next track",
        "domain":"media","risk":"safe",
        "utterances":["skip this","next song","play something else"]}]"#;

    #[test]
    fn a_plain_action_declares_and_validates() {
        assert_eq!(pack_with_actions("[]", NEXT_TRACK), Ok(()));
    }

    #[test]
    fn an_action_needs_a_runtime_to_perform_it() {
        // Unlike a prompt layer, which is text the host renders, an action has
        // to be called. A pack that wins a route and has nothing to call is a
        // dead end the user cannot diagnose.
        assert!(pack(&format!(
            r#"{{"manifest":{{"id":"com.x.p","name":"P","version":"1.0","tier":"pack",
                "contributes":{{"actions":{NEXT_TRACK}}}}}}}"#
        ))
        .is_err());
    }

    #[test]
    fn risk_is_required_so_a_blast_radius_is_never_implicit() {
        // Omitting it must not silently mean "safe" — and it must not silently
        // mean "confirm" either, because then an update that adds a destructive
        // action reads the same as one that adds a harmless one.
        assert!(pack_with_actions(
            "[]",
            r#"[{"id":"next","title":"Next","domain":"media","utterances":["next song"]}]"#
        )
        .is_err());
    }

    #[test]
    fn free_text_into_a_sink_cannot_be_safe() {
        // "open {url}" in an extension that can open URLs is "open whatever
        // Grain mishears". The router is not the weak link here; the acoustic
        // model is.
        let open_anything = r#"[{"id":"open","title":"Open a link","domain":"browser",
            "risk":"safe","utterances":["open {url}"],
            "params":[{"name":"url","kind":"text"}]}]"#;
        assert!(pack_with_actions(r#"["open:url"]"#, open_anything).is_err());

        // The same declaration is fine once it reads the action back first.
        let confirmed = open_anything.replace(r#""risk":"safe""#, r#""risk":"confirm""#);
        assert_eq!(pack_with_actions(r#"["open:url"]"#, &confirmed), Ok(()));

        // And fine as `safe` when there is no sink to feed.
        assert_eq!(pack_with_actions("[]", open_anything), Ok(()));
    }

    #[test]
    fn a_resolved_entity_is_bounded_so_it_stays_safe() {
        // The span goes to the extension, which matches it against its OWN
        // catalogue before anything happens — so the value that reaches the
        // network came from a bounded set, not from the microphone.
        let play_artist = r#"[{"id":"play_artist","title":"Play music by an artist",
            "domain":"media","risk":"safe","utterances":["play {artist}","put on some {artist}"],
            "params":[{"name":"artist","kind":"entity","resolve":true}]}]"#;
        assert_eq!(
            pack_with_actions(r#"["net:api.spotify.com"]"#, play_artist),
            Ok(())
        );
        // Unresolved, the same parameter is raw ASR output again.
        let unresolved = play_artist.replace(r#""resolve":true"#, r#""resolve":false"#);
        assert!(pack_with_actions(r#"["net:api.spotify.com"]"#, &unresolved).is_err());
    }

    #[test]
    fn a_bare_placeholder_would_match_anything_the_user_says() {
        // Ranking is global, so this is not the author's problem to discover in
        // the wild — one greedy utterance degrades every other extension.
        assert!(pack_with_actions(
            "[]",
            r#"[{"id":"p","title":"Play","domain":"media","risk":"safe",
                 "utterances":["{anything}"],
                 "params":[{"name":"anything","kind":"text"}]}]"#
        )
        .is_err());
    }

    #[test]
    fn placeholders_and_parameters_must_agree() {
        // An undeclared placeholder has no span to fill.
        assert!(pack_with_actions(
            "[]",
            r#"[{"id":"p","title":"Play","domain":"media","risk":"safe",
                 "utterances":["play {artist}"]}]"#
        )
        .is_err());
        // A required parameter that appears in no template can never be filled,
        // so the action would route and then always fall to the chooser.
        assert!(pack_with_actions(
            "[]",
            r#"[{"id":"p","title":"Play","domain":"media","risk":"safe",
                 "utterances":["play something"],
                 "params":[{"name":"artist","kind":"entity","resolve":true}]}]"#
        )
        .is_err());
        // Optional is the escape hatch for a parameter filled another way.
        assert_eq!(
            pack_with_actions(
                "[]",
                r#"[{"id":"p","title":"Play","domain":"media","risk":"safe",
                     "utterances":["play something"],
                     "params":[{"name":"artist","kind":"entity","resolve":true,
                                "required":false}]}]"#
            ),
            Ok(())
        );
    }

    #[test]
    fn a_prompt_layer_category_is_not_an_action_domain() {
        // The two vocabularies look alike and mean different things: one is the
        // surface being typed into, the other is which provider performs this.
        let err = pack_with_actions(
            "[]",
            r#"[{"id":"n","title":"Next","domain":"email","risk":"safe",
                 "utterances":["next song"]}]"#,
        )
        .unwrap_err();
        assert!(err.contains("prompt-layer category"), "{err}");
    }

    #[test]
    fn actions_are_bounded_in_every_direction() {
        let many: Vec<String> = (0..ACTIONS_MAX_PER_EXTENSION + 1)
            .map(|i| {
                format!(
                    r#"{{"id":"a{i}","title":"A","domain":"media","risk":"safe",
                        "utterances":["do the thing {i}"]}}"#
                )
            })
            .collect();
        assert!(pack_with_actions("[]", &format!("[{}]", many.join(","))).is_err());

        let phrasings: Vec<String> = (0..ACTION_UTTERANCES_MAX + 1)
            .map(|i| format!(r#""skip it {i}""#))
            .collect();
        assert!(pack_with_actions(
            "[]",
            &format!(
                r#"[{{"id":"n","title":"Next","domain":"media","risk":"safe",
                     "utterances":[{}]}}]"#,
                phrasings.join(",")
            )
        )
        .is_err());

        let fat = "x".repeat(ACTION_UTTERANCE_MAX_BYTES + 1);
        assert!(pack_with_actions(
            "[]",
            &format!(
                r#"[{{"id":"n","title":"Next","domain":"media","risk":"safe",
                     "utterances":["{fat}"]}}]"#
            )
        )
        .is_err());

        let rules = "x".repeat(ACTION_AGENT_RULES_MAX_BYTES + 1);
        assert!(pack_with_actions(
            "[]",
            &format!(
                r#"[{{"id":"n","title":"Next","domain":"media","risk":"safe",
                     "utterances":["next song"],"agentRules":"{rules}"}}]"#
            )
        )
        .is_err());
    }

    #[test]
    fn an_utterance_may_not_hide_what_a_reviewer_read() {
        // Same equivalence rule as prompt-layer text: what the reviewer read
        // must be what the router matches.
        let bidi = format!(
            r#"[{{"id":"n","title":"Next","domain":"media","risk":"safe",
                 "utterances":["skip{}this"]}}]"#,
            '\u{202E}'
        );
        assert!(pack_with_actions("[]", &bidi).is_err());
    }

    #[test]
    fn action_ids_and_match_values_are_checked() {
        let with = |body: &str| pack_with_actions("[]", body);
        assert!(with(
            r#"[{"id":"","title":"N","domain":"media","risk":"safe","utterances":["next"]}]"#
        )
        .is_err());
        assert!(with(
            r#"[{"id":"a:b","title":"N","domain":"media","risk":"safe","utterances":["next"]}]"#
        )
        .is_err());
        assert!(with(
            r#"[{"id":"n","title":"","domain":"media","risk":"safe","utterances":["next"]}]"#
        )
        .is_err());
        assert!(
            with(r#"[{"id":"n","title":"N","domain":"media","risk":"safe","utterances":[]}]"#)
                .is_err()
        );
        // Duplicate ids, and a repeated utterance within one action.
        assert!(with(
            r#"[{"id":"n","title":"N","domain":"media","risk":"safe","utterances":["next"]},
                {"id":"n","title":"M","domain":"media","risk":"safe","utterances":["prev"]}]"#
        )
        .is_err());
        assert!(with(
            r#"[{"id":"n","title":"N","domain":"media","risk":"safe",
                 "utterances":["Next Song","next song"]}]"#
        )
        .is_err());
        // `when` reuses the prompt-layer vocabulary, so it reuses its checks.
        assert!(with(
            r#"[{"id":"n","title":"N","domain":"media","risk":"safe",
                 "utterances":["next"],"when":{"category":["nope"]}}]"#
        )
        .is_err());
    }

    #[test]
    fn the_shipped_voice_actions_declaration_validates() {
        // The first real consumer, pinned. It is also the case the risk rule was
        // written for and the one easiest to get backwards: this extension holds
        // `open:url` and `open:app`, so a free-text span would be "open whatever
        // Grain mishears" — but `target` is a RESOLVED entity, matched against
        // the user's own configured shortcuts before anything launches, so
        // `safe` is correct here.
        assert_eq!(
            pack_with_actions(
                r#"["transform:transcript","open:url","open:app","capture:app","settings"]"#,
                r#"[{"id":"open","title":"Open an app or site you set up","domain":"system",
                     "risk":"safe",
                     "utterances":["open {target}","launch {target}","go to {target}",
                                   "start up {target}","bring up {target}"],
                     "params":[{"name":"target","kind":"entity","resolve":true}]}]"#
            ),
            Ok(())
        );
    }

    #[test]
    fn an_extension_may_not_make_its_own_action_unreachable() {
        // Ranking is global and deterministic, so of two actions claiming one
        // phrase, the same one always wins and the other is dead. The author
        // would never see that from their own extension in isolation.
        assert!(pack_with_actions(
            "[]",
            r#"[{"id":"a","title":"A","domain":"media","risk":"safe","utterances":["skip this"]},
                {"id":"b","title":"B","domain":"media","risk":"safe","utterances":["Skip This"]}]"#
        )
        .is_err());

        // The parameter's NAME is the author's private business — the router
        // hears the same words either way, so these collide too.
        assert!(pack_with_actions(
            "[]",
            r#"[{"id":"a","title":"A","domain":"media","risk":"safe",
                 "utterances":["play {artist}"],
                 "params":[{"name":"artist","kind":"entity","resolve":true}]},
                {"id":"b","title":"B","domain":"media","risk":"safe",
                 "utterances":["play {track}"],
                 "params":[{"name":"track","kind":"entity","resolve":true}]}]"#
        )
        .is_err());

        // Distinct phrases across actions are the normal case.
        assert_eq!(
            pack_with_actions(
                "[]",
                r#"[{"id":"a","title":"A","domain":"media","risk":"safe",
                     "utterances":["skip this"]},
                    {"id":"b","title":"B","domain":"media","risk":"safe",
                     "utterances":["go back"]}]"#
            ),
            Ok(())
        );
    }

    #[test]
    fn utterance_templates_parse_into_literals_and_spans() {
        // The host's span extraction and doctor's validation must agree on
        // where a parameter starts; two parsers would drift.
        assert_eq!(
            parse_utterance("put on some {artist}"),
            Ok(vec![
                UtterancePart::Literal("put on some".into()),
                UtterancePart::Param("artist".into()),
            ])
        );
        assert_eq!(
            parse_utterance("tell {who} that {message}"),
            Ok(vec![
                UtterancePart::Literal("tell".into()),
                UtterancePart::Param("who".into()),
                UtterancePart::Literal("that".into()),
                UtterancePart::Param("message".into()),
            ])
        );
        assert!(parse_utterance("play {artist").is_err());
        assert!(parse_utterance("play artist}").is_err());
        assert!(parse_utterance("play {}").is_err());
        assert!(parse_utterance("play {Artist}").is_err());
    }

    #[test]
    fn extends_reads_slots_anchors_and_payloads() {
        // A slot claimed, a slot offered, a settings anchor and a payload are
        // four different ways of saying "I change this part of Grain"; all four
        // have to answer in the same vocabulary or the placement is a guess.
        assert_eq!(
            extends_of(
                r#"{"manifest":{"id":"com.x.a","name":"A","version":"1.0","tier":"pack",
                    "variant_slots":["agent.reply-surface"]}}"#
            ),
            vec!["agent.reply-surface"]
        );
        assert_eq!(
            extends_of(
                r#"{"manifest":{"id":"com.x.b","name":"B","version":"1.0","tier":"scripted",
                    "entry_source":"//","contributes":{"settings":[
                      {"key":"k","label":"L","kind":"bool","anchor":"context.after"}]}}}"#
            ),
            vec!["context.after"]
        );
        assert_eq!(
            extends_of(
                r#"{"manifest":{"id":"com.x.c","name":"C","version":"1.0","tier":"pack"},
                    "payloads":{"prompts":[{"id":"g","name":"G","prompt":"p"}]}}"#
            ),
            vec![SURFACE_PROMPTS]
        );
    }

    #[test]
    fn extends_is_deduplicated_and_empty_for_a_standalone_pack() {
        assert_eq!(
            extends_of(
                r#"{"manifest":{"id":"com.x.d","name":"D","version":"1.0","tier":"scripted",
                    "entry_source":"//","slots":["pill.theme"],"variant_slots":["pill.theme"]}}"#
            ),
            vec!["pill.theme"]
        );
        // Nothing declared = no host surface, which is what earns a page of its
        // own rather than a slot beside someone else's control.
        assert!(extends_of(
            r#"{"manifest":{"id":"com.x.e","name":"E","version":"1.0","tier":"pack"}}"#
        )
        .is_empty());
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
    fn pill_theme_pack_validates() {
        // A data pack claiming the pill.theme slot and carrying a partial theme.
        assert_eq!(
            pack(
                r#"{"manifest":{"id":"com.x.neon","name":"Neon","version":"1","tier":"pack",
                    "slots":["pill.theme"]},
                    "payloads":{"pill_theme":{"recording":{"dot":[0,255,120],"pattern":"breathe"}}}}"#
            ),
            Ok(())
        );
        // A theme with no slot claim would install and do nothing — rejected.
        assert!(pack(
            r#"{"manifest":{"id":"com.x.neon","name":"Neon","version":"1","tier":"pack"},
                "payloads":{"pill_theme":{"idle":{"dot":[1,2,3]}}}}"#
        )
        .is_err());
        // A wrong-shaped theme is rejected at import, not ignored at delivery.
        assert!(pack(
            r#"{"manifest":{"id":"com.x.neon","name":"Neon","version":"1","tier":"pack",
                "slots":["pill.theme"]},
                "payloads":{"pill_theme":"bright"}}"#
        )
        .is_err());
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
    fn manifest_writes_spec_casing_and_reads_legacy_field_names() {
        let json = r#"{"manifest":{"id":"com.x.cat","name":"Cat","version":"1",
            "grain_api":"^1.0","tier":"scripted","entry_source":"x",
            "contributes":{"shortcuts":[{"id":"open","label":"Open",
            "default_binding":"Alt+C"}]}}}"#;
        let pack: GrainPack = serde_json::from_str(json).unwrap();
        assert_eq!(pack.manifest.grain_api, "^1.0");
        assert_eq!(
            pack.manifest.contributes.shortcuts[0]
                .default_binding
                .as_deref(),
            Some("Alt+C")
        );

        let value = serde_json::to_value(pack).unwrap();
        let manifest = &value["manifest"];
        assert_eq!(manifest["grainApi"], "^1.0");
        assert!(manifest.get("grain_api").is_none());
        let shortcut = &manifest["contributes"]["shortcuts"][0];
        assert_eq!(shortcut["defaultBinding"], "Alt+C");
        assert!(shortcut.get("default_binding").is_none());
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
        // An overlay is the same story: needs its capability and its UI.
        assert!(scripted(
            r#","permissions":["surface:overlay"],
               "surfaces":{"overlay":{"ui_source":"<p>x"}}"#
        )
        .is_ok());
        assert!(scripted(r#","surfaces":{"overlay":{"ui_source":"<p>x"}}"#).is_err());
        assert!(scripted(
            r#","permissions":["surface:overlay"],"surfaces":{"overlay":{"timeout_ms":2000}}"#
        )
        .is_err());
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

    #[test]
    fn session_mode_requires_its_grant_and_a_unique_safe_id() {
        let base = |permissions: &str, contribution: &str| {
            pack(&format!(
                r#"{{"manifest":{{"id":"com.x.notes","name":"Notes","version":"1","tier":"scripted","entry_source":"x","permissions":{permissions},"contributes":{contribution}}}}}"#
            ))
        };
        assert!(base(
            r#"["session:start"]"#,
            r#"{"sessionMode":{"id":"note","label":"Dictate a note","default_binding":"Ctrl+Shift+N"}}"#,
        )
        .is_ok());
        assert!(base(
            "[]",
            r#"{"sessionMode":{"id":"note","label":"Dictate a note"}}"#,
        )
        .is_err());
        assert!(base(
            r#"["session:start"]"#,
            r#"{"sessionMode":{"id":"bad:id","label":"Bad"}}"#,
        )
        .is_err());
        assert!(base(
            r#"["session:start"]"#,
            r#"{"shortcuts":[{"id":"note","label":"Other"}],"sessionMode":{"id":"note","label":"Mode"}}"#,
        )
        .is_err());
    }

    #[test]
    fn network_grants_accept_one_canonical_host_and_reject_wildcards_or_urls() {
        for capability in ["net:api.example.com", "net:127.0.0.1"] {
            assert_eq!(
                network_capability_host(capability),
                capability.strip_prefix("net:")
            );
        }
        for capability in [
            "net:*",
            "net:*.example.com",
            "net:https://api.example.com",
            "net:api.example.com:443",
            "net:api.example.com/path",
            "net:API.example.com",
            "net:api.example.com.",
            "net:-api.example.com",
        ] {
            assert!(
                network_capability_host(capability).is_none(),
                "accepted {capability}"
            );
        }

        assert!(pack(
            r#"{"manifest":{"id":"com.x.net","name":"n","version":"1","tier":"scripted","entry_source":"x","permissions":["net:api.example.com"]}}"#
        )
        .is_ok());
        assert!(pack(
            r#"{"manifest":{"id":"com.x.net","name":"n","version":"1","tier":"scripted","entry_source":"x","permissions":["net:*.example.com"]}}"#
        )
        .is_err());
    }

    #[test]
    fn secret_settings_cannot_smuggle_credentials_in_manifest_defaults() {
        let valid: GrainPack = serde_json::from_str(
            r#"{"manifest":{"id":"com.x.secret","name":"n","version":"1","tier":"scripted","entry_source":"x","contributes":{"settings":[{"key":"api_key","label":"API key","kind":"secret"}]}}}"#,
        )
        .unwrap();
        valid.validate().unwrap();
        assert!(matches!(
            valid.manifest.contributes.settings[0].kind,
            SettingKind::Secret
        ));
        assert!(pack(
            r#"{"manifest":{"id":"com.x.secret","name":"n","version":"1","tier":"scripted","entry_source":"x","contributes":{"settings":[{"key":"api_key","label":"API key","kind":"secret","default":"shipped-key"}]}}}"#
        )
        .is_err());
    }

    /// The builtin tier (Grain Space's mechanism): first-party identity is
    /// mandatory, there is nothing to launch, and it may contribute settings a
    /// data pack cannot.

    /// A `grain://` panel renders a HOST component with Grain's own privileges,
    /// so the tier gate is a security boundary, not a convenience.

    #[test]
    fn native_companions_validate_only_through_the_developer_boundary() {
        let native: GrainPack = serde_json::from_str(
            r#"{"manifest":{"id":"com.x.native","name":"Native","version":"1","tier":"native","permissions":["storage"],"activation":["onStartup"],"companion":{"windows":"bin/native.exe","macos":"bin/native","linux":"bin/native"}}}"#,
        )
        .unwrap();
        assert!(native.validate().is_err());
        native.validate_dev().unwrap();

        let missing: GrainPack = serde_json::from_str(
            r#"{"manifest":{"id":"com.x.native","name":"Native","version":"1","tier":"native"}}"#,
        )
        .unwrap();
        assert!(missing.validate_dev().is_err());
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
    /// A surface declared the way every other part of a manifest spells things.
    ///
    /// This read as an EMPTY ui_source before — serde filled the missing snake
    /// field with its default and the camel one was ignored — so the pack built,
    /// installed, and opened a window with nothing in it. Silence is the reason
    /// this is a test and not a doc note.
    #[test]
    fn surfaces_accept_the_camel_case_spelling_the_rest_of_a_manifest_uses() {
        let json = r#"{
            "workspace": {
                "title": "Notes",
                "minSize": [900, 600],
                "uiSource": "<p>hi</p>"
            },
            "overlay": { "timeoutMs": 3000, "uiSource": "<p>hud</p>" }
        }"#;
        let s: Surfaces = serde_json::from_str(json).unwrap();
        let w = s.workspace.expect("workspace parsed");
        assert_eq!(w.ui_source, "<p>hi</p>");
        assert_eq!(w.min_size, Some([900, 600]));
        let o = s.overlay.expect("overlay parsed");
        assert_eq!(o.ui_source, "<p>hud</p>");
        assert_eq!(o.timeout_ms, Some(3000));

        // The snake spellings still read, so packs already published keep working.
        let legacy = r#"{"workspace":{"title":"N","min_size":[800,600],"ui_source":"<p>x</p>"}}"#;
        let s: Surfaces = serde_json::from_str(legacy).unwrap();
        let w = s.workspace.expect("legacy workspace parsed");
        assert_eq!(w.ui_source, "<p>x</p>");
        assert_eq!(w.min_size, Some([800, 600]));
    }

    #[test]
    fn anchor_list_matches_the_spec_v1_set() {
        assert_eq!(
            ANCHORS,
            &[
                "snippets.after",
                "dictation.pipeline.after",
                "context.after",
                "agent.after",
                "grainspace.after",
                "models.after",
            ]
        );
    }
}
