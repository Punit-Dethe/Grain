//! Tool-extension manifests and package validation.
//!
//! Grain owns recording, prompts, settings and UI. Extensions declare tools,
//! scoped storage/network permissions and optional host-owned authentication.
//! Package identity, source/icon bounds and protocol admission are checked at
//! every import, developer and signed-catalogue boundary.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Scripted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionManifest {
    /// Reverse-dns, unique in the index (SPEC §2).
    pub id: String,
    pub name: String,
    pub version: String,
    /// Required native API profile, using supported numeric exact/caret grammar.
    #[serde(default, rename = "grainApi", alias = "grain_api")]
    pub grain_api: String,
    pub tier: Tier,
    /// One line, shown in Overview; full text on hover.
    #[serde(default)]
    pub description: String,
    /// [GRAIN] Pack-relative path to the extension's icon — a square PNG at
    /// exactly [`ICON_MASTER_DIM`]² (`docs/Extensions V1/PLAN.md` §13.3). One
    /// master the author supplies; Grain downscales to every size it shows
    /// (pill row, card, store), so consistency is Grain's, not the author's.
    ///
    /// Required for store submission by `doctor` and registry checks. Local
    /// tool packages may omit artwork.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default)]
    pub repository: Option<String>,
    /// Scoped permissions from [`KNOWN_CAPABILITIES`] or bounded `net:<host>`
    /// grants. The user approves them before enablement.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Embedded JavaScript source, required for every tool package.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub entry_source: String,
    /// Tools and optional host-owned authentication.
    #[serde(default)]
    pub contributes: Contributes,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Contributes {
    /// Host-owned OAuth connections. An extension receives status and may ask
    /// Grain to perform authenticated requests, but never receives tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationDecl>,
    /// Tools exposed to the Agent, subject to host-owned approval and admission.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionDecl>,
}

/// A public-client OAuth 2.0 Authorization Code + PKCE connection.
///
/// Unknown fields are rejected deliberately: in particular, a `clientSecret`
/// must never be smuggled into a distributable extension manifest.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthenticationDecl {
    #[serde(rename = "type")]
    pub auth_type: AuthenticationType,
    #[serde(rename = "providerName", alias = "provider_name")]
    pub provider_name: String,
    #[serde(rename = "clientId", alias = "client_id")]
    pub client_id: String,
    #[serde(rename = "authorizationEndpoint", alias = "authorization_endpoint")]
    pub authorization_endpoint: String,
    #[serde(rename = "tokenEndpoint", alias = "token_endpoint")]
    pub token_endpoint: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(
        default = "default_redirect_methods",
        rename = "redirectMethods",
        alias = "redirect_methods"
    )]
    pub redirect_methods: Vec<RedirectMethod>,
    #[serde(default, rename = "apiHosts", alias = "api_hosts")]
    pub api_hosts: Vec<String>,
    #[serde(
        default,
        rename = "authorizationParameters",
        alias = "authorization_parameters"
    )]
    pub authorization_parameters: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthenticationType {
    #[serde(rename = "oauth2-pkce")]
    OAuth2Pkce,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RedirectMethod {
    Loopback,
}

fn default_redirect_methods() -> Vec<RedirectMethod> {
    vec![RedirectMethod::Loopback]
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

/// The edge of the icon master an extension submits (`ExtensionManifest::icon`,
/// §13.3): a square PNG at exactly this many pixels each side. One high-res
/// master; Grain downscales to the sizes it shows and never upscales, so 512 is
/// comfortably above every surface (a pill row is tens of px, a card ~100).
pub const ICON_MASTER_DIM: u32 = 512;

/// Hard cap on the icon PNG's byte length. Bounds the pack and stops a tiny
/// 512×512 header from hiding megabytes of IDAT (a decompression bomb).
pub const ICON_MAX_BYTES: u64 = 512 * 1024;

/// Maximum UTF-8 source carried by a scripted single-file pack. Registry CI
/// enforces the same limit before packaging; keeping it in the wire contract
/// prevents a manually supplied pack from bypassing that boundary.
pub const PACK_ENTRY_MAX_BYTES: usize = 5 * 1024 * 1024;

/// Hard ceiling for one JSON `.grainpack` document. This covers the 5 MiB
/// entry, the base64-encoded icon, and bounded manifest/payload metadata while
/// preventing an import or startup load from allocating an attacker-sized
/// document before validation can run.
pub const PACK_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Read PNG dimensions from the mandatory IHDR header. This intentionally does
/// not claim the image is decodable; authoring checks and the host perform a
/// full decode before accepting/materialising artwork.
pub fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some((
        u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
        u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
    ))
}

/// One action an extension can perform when the user asks for it by voice.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActionDecl {
    /// Stable id, unique within the extension. Appears in the action log and in
    /// `doctor` output, so it should read as a name.
    pub id: String,
    /// One plain line, shown in the permission sheet and the chooser. Written
    /// for the user, not the model: "Skip to the next track".
    pub title: String,
    /// Whether performing this needs a read-back first. Required, and never
    /// inferred — see [`ActionRisk`].
    pub risk: ActionRisk,
    /// Ways a user might ask for this, in English.
    ///
    /// `{param}` placeholders name a declared parameter and mark where its span
    /// begins.
    #[serde(default)]
    pub utterances: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ActionParamDecl>,
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
/// Keeps action identifiers safe in registry keys and bounded provider tool
/// names. The host still hashes provider-facing names, but the manifest id is a
/// protocol identifier and must not contain deceptive punctuation or Unicode.
pub const ACTION_ID_MAX_BYTES: usize = 64;

/// Hard ceiling on parameters per action. An action needing five spans out of
/// one spoken sentence is an Agent turn, not a route.
pub const ACTION_PARAMS_MAX: usize = 4;

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
    let has_sink = permissions
        .iter()
        .any(|p| network_capability_host(p).is_some());

    let mut seen = std::collections::HashSet::new();
    for action in actions {
        let id = action.id.trim();
        if id.is_empty() {
            return Err("an action is missing its id".into());
        }
        if id.len() > ACTION_ID_MAX_BYTES {
            return Err(format!(
                "action id '{id}' is {} bytes; the limit is {ACTION_ID_MAX_BYTES}",
                id.len()
            ));
        }
        // `:` separates the globally unique extension id from its local action
        // id. Other existing V1 identifiers remain compatible; provider-facing
        // tool names are independently hashed and grammar-safe.
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
        // What the reviewer read must be what the router matches.
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

/// Validate the identifier before it is used in a registry key, prompt
/// attribution, shortcut namespace, log line, or filesystem path.
pub fn validate_extension_id(id: &str) -> Result<(), String> {
    if id.len() > 253 || id != id.trim() {
        return Err("manifest.id must be a canonical reverse-dns identifier".into());
    }
    let parts: Vec<&str> = id.split('.').collect();
    if parts.len() < 2
        || parts.iter().any(|part| {
            part.is_empty()
                || part.len() > 63
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                || !part
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                || !part
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
    {
        return Err(
            "manifest.id must be lowercase reverse-dns segments using letters, digits, or '-'"
                .into(),
        );
    }
    Ok(())
}

/// Versions are used as directory components by store installs. Grain accepts
/// simple semver-like versions (including prerelease/build separators) without
/// requiring a semver dependency in the SDK leaf.
pub fn validate_extension_version(version: &str) -> Result<(), String> {
    if version.is_empty()
        || version.len() > 64
        || version != version.trim()
        || version.contains("..")
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        || !version
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !version
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err("manifest.version must be a safe semver-like value".into());
    }
    Ok(())
}

/// Capabilities a scripted pack may request in API 1.0. Anything outside this
/// set is rejected at import (R1: grant narrowly, widen with each consumer).
/// Parameterised `net:<host>` grants are validated separately below.
pub const KNOWN_CAPABILITIES: &[&str] = &["storage", "auth"];

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

/// Allowlist shared by pack validation and runtime token issuance. Old grants
/// cannot restore retired host privileges, even after a package update.
pub fn tool_permission_allowed(permission: &str) -> bool {
    matches!(permission, "storage" | "auth") || network_capability_host(permission).is_some()
}

fn valid_https_endpoint(value: &str) -> bool {
    if value.len() > 2048
        || value.contains(['?', '#', '\\'])
        || value.chars().any(char::is_whitespace)
    {
        return false;
    }
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() || authority.contains(['@', '%']) {
        return false;
    }
    let (host, valid_port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host,
            !port.is_empty()
                && port.bytes().all(|byte| byte.is_ascii_digit())
                && port.parse::<u16>().is_ok(),
        ),
        None => (authority, true),
    };
    valid_port && network_capability_host(&format!("net:{host}")) == Some(host)
}

fn validate_authentication(
    decl: Option<&AuthenticationDecl>,
    permissions: &[String],
) -> Result<(), String> {
    let Some(decl) = decl else {
        if permissions.iter().any(|permission| permission == "auth") {
            return Err("permission 'auth' has no authentication declaration".into());
        }
        return Ok(());
    };
    const RESERVED: &[&str] = &[
        "client_id",
        "redirect_uri",
        "response_type",
        "state",
        "code_challenge",
        "code_challenge_method",
        "scope",
    ];
    if decl.provider_name.trim().is_empty() || decl.provider_name.len() > 80 {
        return Err("authentication requires a providerName".into());
    }
    if decl.client_id.trim().is_empty() || decl.client_id.len() > 512 {
        return Err("authentication requires a bounded clientId".into());
    }
    if !valid_https_endpoint(&decl.authorization_endpoint)
        || !valid_https_endpoint(&decl.token_endpoint)
    {
        return Err("authentication endpoints must be HTTPS URLs without query or fragment".into());
    }
    if decl.scopes.is_empty()
        || decl.scopes.len() > 32
        || decl.scopes.iter().any(|scope| {
            scope.is_empty() || scope.len() > 256 || scope.bytes().any(|b| b <= 0x20 || b == 0x7f)
        })
    {
        return Err("authentication requires 1-32 valid OAuth scopes".into());
    }
    if decl.redirect_methods != [RedirectMethod::Loopback] {
        return Err("authentication redirectMethods must be ['loopback'] in API 1.x".into());
    }
    if decl.api_hosts.is_empty() || decl.api_hosts.len() > 16 {
        return Err("authentication requires 1-16 apiHosts".into());
    }
    for host in &decl.api_hosts {
        if network_capability_host(&format!("net:{host}")) != Some(host.as_str())
            || !permissions
                .iter()
                .any(|permission| permission == &format!("net:{host}"))
        {
            return Err(format!(
                "authentication apiHost '{host}' requires the matching net permission"
            ));
        }
    }
    if !permissions.iter().any(|permission| permission == "auth") {
        return Err("authentication requires the 'auth' permission".into());
    }
    if decl.authorization_parameters.len() > 16
        || decl.authorization_parameters.iter().any(|(key, value)| {
            key.is_empty()
                || key.len() > 64
                || !key.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
                })
                || value.len() > 1024
                || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
                || RESERVED
                    .iter()
                    .any(|reserved| key.eq_ignore_ascii_case(reserved))
        })
    {
        return Err("authentication has invalid authorizationParameters".into());
    }
    Ok(())
}

/// Embedded package assets.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PackPayloads {
    /// The submission-validated 512² PNG master, embedded in the single-file
    /// pack. The manifest path remains the authoring/source identity; installed
    /// packs cannot safely depend on that source tree still existing.
    #[serde(
        default,
        rename = "iconPng",
        alias = "icon_png",
        skip_serializing_if = "Option::is_none"
    )]
    pub icon_png: Option<String>,
}

/// The `.grainpack` file: manifest + payloads in one JSON document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GrainPack {
    pub manifest: ExtensionManifest,
    #[serde(default)]
    pub payloads: PackPayloads,
}

impl GrainPack {
    /// Admit tools, scoped storage/network and host-owned authentication only.
    pub fn validate_tool_only(&self) -> Result<(), String> {
        let m = &self.manifest;
        if !crate::compatibility::api_requirement_supported(&m.grain_api, crate::GRAIN_API_VERSION)
        {
            return Err(crate::compatibility::UNSUPPORTED_EXTENSION_API.into());
        }
        for permission in &m.permissions {
            if !tool_permission_allowed(permission) {
                return Err(format!("Retired or unsupported extension permission '{permission}'. Grain extensions now provide tools only."));
            }
        }
        Ok(())
    }

    /// Decode the optional icon master with a strict allocation bound. Full PNG
    /// decoding belongs to the host/checker; the SDK cheaply enforces the wire
    /// shape and advertised dimensions at every pack-validation boundary.
    pub fn embedded_icon_png(&self) -> Result<Option<Vec<u8>>, String> {
        let Some(encoded) = self.payloads.icon_png.as_deref() else {
            return Ok(None);
        };
        if self.manifest.icon.trim().is_empty() {
            return Err("payloads.iconPng requires manifest.icon".into());
        }
        let max_encoded = (ICON_MAX_BYTES as usize).div_ceil(3) * 4;
        if encoded.len() > max_encoded {
            return Err(format!(
                "payloads.iconPng is larger than {} KB",
                ICON_MAX_BYTES / 1024
            ));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "payloads.iconPng is not valid base64".to_string())?;
        if bytes.len() as u64 > ICON_MAX_BYTES {
            return Err(format!(
                "payloads.iconPng is larger than {} KB",
                ICON_MAX_BYTES / 1024
            ));
        }
        match png_dimensions(&bytes) {
            Some((width, height)) if width == ICON_MASTER_DIM && height == ICON_MASTER_DIM => {
                Ok(Some(bytes))
            }
            Some((width, height)) => Err(format!(
                "payloads.iconPng is {width}×{height}; it must be exactly \
                 {ICON_MASTER_DIM}×{ICON_MASTER_DIM}"
            )),
            None => Err("payloads.iconPng is not a PNG".into()),
        }
    }

    /// Validate an imported tool package.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_inner(false)
    }

    /// Developer projects use the same tool contract as imported packages.
    pub fn validate_dev(&self) -> Result<(), String> {
        self.validate_inner(false)
    }

    /// Validation for artifacts whose bytes were authenticated by Grain's
    /// signed catalogue. Only that trust path may claim the `grain.` namespace.
    pub fn validate_trusted(&self) -> Result<(), String> {
        self.validate_inner(true)
    }

    fn validate_inner(&self, allow_reserved_id: bool) -> Result<(), String> {
        self.validate_tool_only()?;
        let m = &self.manifest;
        validate_extension_id(&m.id)?;
        validate_extension_version(&m.version)?;
        // `grain.` is reserved: a USER-imported pack may not claim a first-party
        // identity. Packs Grain publishes are validated on the path that installs
        // them from the signed catalogue, not through this importer.
        if m.id.starts_with("grain.") && !allow_reserved_id {
            return Err("the 'grain.' id prefix is reserved for built-ins".into());
        }
        if m.name.trim().is_empty() {
            return Err("manifest.name is required".into());
        }
        if m.entry_source.trim().is_empty() {
            return Err("scripted extensions require entry_source".into());
        }
        if m.entry_source.len() > PACK_ENTRY_MAX_BYTES {
            return Err(format!(
                "scripted extension entry is larger than {} MB",
                PACK_ENTRY_MAX_BYTES / (1024 * 1024)
            ));
        }
        self.embedded_icon_png()?;
        validate_actions(&m.contributes.actions, &m.permissions)?;
        validate_authentication(m.contributes.authentication.as_ref(), &m.permissions)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_api_requirements_are_checked_at_every_pack_boundary() {
        let mut pack: GrainPack = serde_json::from_value(serde_json::json!({"manifest": {
            "id":"com.example.api","name":"API","version":"1.0.0","tier":"scripted",
            "entry_source":"grain.actions({read: async () => ({ok:null})});",
            "contributes":{"actions":[{"id":"read","title":"Read","risk":"confirm","utterances":["read data"]}]}
        }})).unwrap();
        // Missing metadata is refused at every runtime admission boundary.
        for result in [
            pack.validate_tool_only(),
            pack.validate(),
            pack.validate_dev(),
            pack.validate_trusted(),
        ] {
            assert_eq!(
                result.unwrap_err(),
                crate::compatibility::UNSUPPORTED_EXTENSION_API
            );
        }
        for supported in ["^1.0", "^1.0.0", "1.0", "1.0.0"] {
            pack.manifest.grain_api = supported.into();
            assert!(pack.validate_tool_only().is_ok());
            assert!(pack.validate().is_ok());
            assert!(pack.validate_dev().is_ok());
            assert!(pack.validate_trusted().is_ok());
        }
        for unsupported in ["", "^2.0", "^1.0.1", "*", "latest", " ", "01.0", "+1.0"] {
            pack.manifest.grain_api = unsupported.into();
            for result in [
                pack.validate_tool_only(),
                pack.validate(),
                pack.validate_dev(),
                pack.validate_trusted(),
            ] {
                assert_eq!(
                    result.unwrap_err(),
                    crate::compatibility::UNSUPPORTED_EXTENSION_API,
                    "{unsupported}"
                );
            }
        }
    }

    #[test]
    fn unsupported_permissions_fail_at_every_trust_boundary() {
        for permission in [
            "capture:selection",
            "llm",
            "embed",
            "settings",
            "session:start",
            "resident",
            "events:transcripts",
            "transform:transcript",
            "open:url",
            "open:app",
            "surface:overlay",
            "os:exec",
            "all",
        ] {
            let candidate: GrainPack = serde_json::from_value(serde_json::json!({
                "manifest": {"id":"com.example.tools","name":"Tools","version":"1",
                    "grainApi":"^1.0","tier":"scripted","entry_source":"//", "permissions":[permission]}
            }))
            .unwrap();
            for result in [
                candidate.validate_tool_only(),
                candidate.validate(),
                candidate.validate_dev(),
                candidate.validate_trusted(),
            ] {
                assert!(result.is_err(), "{permission}");
            }
        }
    }

    fn pack(json: &str) -> Result<(), String> {
        serde_json::from_str::<GrainPack>(json)
            .map_err(|e| e.to_string())
            .and_then(|p| p.validate())
    }

    #[test]
    fn extension_ids_are_path_and_prompt_safe() {
        for id in [
            "com.example.good/../evil",
            "com.example..evil",
            "com.example.evil\nforged",
            "Com.Example.evil",
            ".com.example",
        ] {
            let json = format!(
                r#"{{"manifest":{{"id":{id:?},"name":"P","version":"1.0","grainApi":"^1.0","tier":"scripted","entry_source":"//"}}}}"#
            );
            assert!(pack(&json).is_err(), "unsafe id passed: {id:?}");
        }
        assert_eq!(validate_extension_id("com.example.safe-id"), Ok(()));
        for version in ["../escape", "1/escape", "1\\escape", "1..2", " 1.0"] {
            assert!(validate_extension_version(version).is_err());
        }
        assert_eq!(validate_extension_version("1.2.3-beta+7"), Ok(()));
    }

    #[test]
    fn prompt_record_is_not_an_extension_capability() {
        assert!(KNOWN_CAPABILITIES
            .iter()
            .all(|capability| !capability.contains("prompt")));
    }

    /// A scripted extension declaring `actions`, with permissions and the
    /// action array spliced in.
    fn pack_with_actions(permissions: &str, actions: &str) -> Result<(), String> {
        pack(&format!(
            r#"{{"manifest":{{"id":"com.x.a","name":"A","version":"1.0","grainApi":"^1.0","tier":"scripted",
                "entry_source":"//","permissions":{permissions},
                "contributes":{{"actions":{actions}}}}}}}"#
        ))
    }

    const NEXT_TRACK: &str = r#"[{"id":"next","title":"Skip to the next track",
        "risk":"safe",
        "utterances":["skip this","next song","play something else"]}]"#;

    #[test]
    fn a_plain_action_declares_and_validates() {
        assert_eq!(pack_with_actions("[]", NEXT_TRACK), Ok(()));
    }

    #[test]
    fn an_action_needs_a_runtime_to_perform_it() {
        // Only scripted tool packages have a callable runtime. Data-only pack
        // tiers are not part of the current contract.
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
            r#"[{"id":"next","title":"Next","utterances":["next song"]}]"#
        )
        .is_err());
    }

    #[test]
    fn free_text_into_a_sink_cannot_be_safe() {
        // A network grant can send arbitrary misheard text to a remote host.
        // A safe action must not accept an unbounded span for that sink.
        let open_anything = r#"[{"id":"open","title":"Open a link",
            "risk":"safe","utterances":["open {url}"],
            "params":[{"name":"url","kind":"text"}]}]"#;
        assert!(pack_with_actions(r#"["net:api.example.com"]"#, open_anything).is_err());

        // The same declaration is fine once it reads the action back first.
        let confirmed = open_anything.replace(r#""risk":"safe""#, r#""risk":"confirm""#);
        assert_eq!(
            pack_with_actions(r#"["net:api.example.com"]"#, &confirmed),
            Ok(())
        );

        // And fine as `safe` when there is no sink to feed.
        assert_eq!(pack_with_actions("[]", open_anything), Ok(()));
    }

    #[test]
    fn a_resolved_entity_is_bounded_so_it_stays_safe() {
        // The span goes to the extension, which matches it against its OWN
        // catalogue before anything happens — so the value that reaches the
        // network came from a bounded set, not from the microphone.
        let play_artist = r#"[{"id":"play_artist","title":"Play music by an artist",
            "risk":"safe","utterances":["play {artist}","put on some {artist}"],
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
            r#"[{"id":"p","title":"Play","risk":"safe",
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
            r#"[{"id":"p","title":"Play","risk":"safe",
                 "utterances":["play {artist}"]}]"#
        )
        .is_err());
        // A required parameter that appears in no template can never be filled,
        // so the action would route and then always fall to the chooser.
        assert!(pack_with_actions(
            "[]",
            r#"[{"id":"p","title":"Play","risk":"safe",
                 "utterances":["play something"],
                 "params":[{"name":"artist","kind":"entity","resolve":true}]}]"#
        )
        .is_err());
        // Optional is the escape hatch for a parameter filled another way.
        assert_eq!(
            pack_with_actions(
                "[]",
                r#"[{"id":"p","title":"Play","risk":"safe",
                     "utterances":["play something"],
                     "params":[{"name":"artist","kind":"entity","resolve":true,
                                "required":false}]}]"#
            ),
            Ok(())
        );
    }

    #[test]
    fn actions_are_bounded_in_every_direction() {
        let many: Vec<String> = (0..ACTIONS_MAX_PER_EXTENSION + 1)
            .map(|i| {
                format!(
                    r#"{{"id":"a{i}","title":"A","risk":"safe",
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
                r#"[{{"id":"n","title":"Next","risk":"safe",
                     "utterances":[{}]}}]"#,
                phrasings.join(",")
            )
        )
        .is_err());

        let fat = "x".repeat(ACTION_UTTERANCE_MAX_BYTES + 1);
        assert!(pack_with_actions(
            "[]",
            &format!(
                r#"[{{"id":"n","title":"Next","risk":"safe",
                     "utterances":["{fat}"]}}]"#
            )
        )
        .is_err());
    }

    #[test]
    fn an_utterance_may_not_hide_what_a_reviewer_read() {
        // What the reviewer read must be what the router matches.
        let bidi = format!(
            r#"[{{"id":"n","title":"Next","risk":"safe",
                 "utterances":["skip{}this"]}}]"#,
            '\u{202E}'
        );
        assert!(pack_with_actions("[]", &bidi).is_err());
    }

    #[test]
    fn action_ids_and_declarations_are_checked() {
        let with = |body: &str| pack_with_actions("[]", body);
        assert!(with(r#"[{"id":"","title":"N","risk":"safe","utterances":["next"]}]"#).is_err());
        assert!(with(r#"[{"id":"a:b","title":"N","risk":"safe","utterances":["next"]}]"#).is_err());
        let long_id = "a".repeat(ACTION_ID_MAX_BYTES + 1);
        assert!(with(&format!(
            r#"[{{"id":"{long_id}","title":"N","risk":"safe","utterances":["next"]}}]"#
        ))
        .is_err());
        assert!(with(r#"[{"id":"n","title":"","risk":"safe","utterances":["next"]}]"#).is_err());
        assert!(with(r#"[{"id":"n","title":"N","risk":"safe","utterances":[]}]"#).is_err());
        // Duplicate ids, and a repeated utterance within one action.
        assert!(with(
            r#"[{"id":"n","title":"N","risk":"safe","utterances":["next"]},
                {"id":"n","title":"M","risk":"safe","utterances":["prev"]}]"#
        )
        .is_err());
        assert!(with(
            r#"[{"id":"n","title":"N","risk":"safe",
                 "utterances":["Next Song","next song"]}]"#
        )
        .is_err());
    }

    #[test]
    fn an_extension_may_not_make_its_own_action_unreachable() {
        // Ranking is global and deterministic, so of two actions claiming one
        // phrase, the same one always wins and the other is dead. The author
        // would never see that from their own extension in isolation.
        assert!(pack_with_actions(
            "[]",
            r#"[{"id":"a","title":"A","risk":"safe","utterances":["skip this"]},
                {"id":"b","title":"B","risk":"safe","utterances":["Skip This"]}]"#
        )
        .is_err());

        // The parameter's NAME is the author's private business — the router
        // hears the same words either way, so these collide too.
        assert!(pack_with_actions(
            "[]",
            r#"[{"id":"a","title":"A","risk":"safe",
                 "utterances":["play {artist}"],
                 "params":[{"name":"artist","kind":"entity","resolve":true}]},
                {"id":"b","title":"B","risk":"safe",
                 "utterances":["play {track}"],
                 "params":[{"name":"track","kind":"entity","resolve":true}]}]"#
        )
        .is_err());

        // Distinct phrases across actions are the normal case.
        assert_eq!(
            pack_with_actions(
                "[]",
                r#"[{"id":"a","title":"A","risk":"safe",
                     "utterances":["skip this"]},
                    {"id":"b","title":"B","risk":"safe",
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
    fn scripted_pack_passes_with_entry_and_known_caps() {
        assert_eq!(
            pack(
                r#"{"manifest":{"id":"com.x.cat","name":"Cat","version":"1","grainApi":"^1.0","tier":"scripted",
                    "permissions":["storage","net:api.example.com"],
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
            r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","grainApi":"^1.0","tier":"scripted"}}"#
        )
        .is_err());
        assert!(pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x","permissions":["root"]}}"#).is_err());
        // tier-A pack must not carry code
        assert!(pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"pack","entry_source":"x"}}"#).is_err());
        assert!(
            pack(r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","tier":"pack","permissions":["llm"]}}"#)
                .is_err()
        );
        // unknown fields from a newer contract are tolerated
        assert_eq!(
            pack(
                r#"{"manifest":{"id":"com.x.y","name":"n","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x","futureField":1}}"#
            ),
            Ok(())
        );
    }

    #[test]
    fn only_trusted_catalogue_artifacts_may_claim_reserved_ids() {
        let pack: GrainPack = serde_json::from_str(
            r#"{"manifest":{"id":"grain.first-party","name":"First party","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x"}}"#,
        )
        .unwrap();

        assert!(pack.validate().is_err());
        assert!(pack.validate_dev().is_err());
        assert!(pack.validate_trusted().is_ok());
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
            r#"{"manifest":{"id":"com.x.net","name":"n","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x","permissions":["net:api.example.com"]}}"#
        )
        .is_ok());
        assert!(pack(
            r#"{"manifest":{"id":"com.x.net","name":"n","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x","permissions":["net:*.example.com"]}}"#
        )
        .is_err());
    }

    #[test]
    fn authentication_is_one_public_pkce_account_per_extension() {
        let github = r#"{"manifest":{"id":"com.x.github","name":"GitHub","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x","permissions":["auth","net:api.github.com"],"contributes":{"authentication":{"type":"oauth2-pkce","providerName":"GitHub","clientId":"github-public","authorizationEndpoint":"https://github.com/login/oauth/authorize","tokenEndpoint":"https://github.com/login/oauth/access_token","scopes":["read:user"],"apiHosts":["api.github.com"]}}}}"#;
        assert_eq!(pack(github), Ok(()));

        let missing_host_grant = github.replace(",\"net:api.github.com\"", "");
        assert!(pack(&missing_host_grant).is_err());

        let with_secret = github.replace(
            "\"clientId\":\"github-public\"",
            "\"clientId\":\"github-public\",\"clientSecret\":\"must-not-ship\"",
        );
        assert!(serde_json::from_str::<GrainPack>(&with_secret).is_err());

        let two_services = r#"{"manifest":{"id":"com.x.bridge","name":"Bridge","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x","permissions":["auth","net:api.github.com"],"contributes":{"authentication":[{"type":"oauth2-pkce","providerName":"GitHub","clientId":"github-public","authorizationEndpoint":"https://github.com/login/oauth/authorize","tokenEndpoint":"https://github.com/login/oauth/access_token","scopes":["read:user"],"apiHosts":["api.github.com"]},{"type":"oauth2-pkce","providerName":"Linear","clientId":"linear-public","authorizationEndpoint":"https://linear.app/oauth/authorize","tokenEndpoint":"https://api.linear.app/oauth/token","scopes":["read"],"apiHosts":["api.linear.app"]}]}}}"#;
        assert!(serde_json::from_str::<GrainPack>(two_services).is_err());

        assert!(pack(&github.replace("\"auth\",", "")).is_err());
        assert!(pack(
            r#"{"manifest":{"id":"com.x.auth","name":"Auth","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x","permissions":["auth"]}}"#
        )
        .is_err());
    }

    #[test]
    fn authentication_rejects_ambiguous_endpoints_and_reserved_parameter_case() {
        let raw = r#"{"manifest":{"id":"com.x.auth","name":"Auth","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x","permissions":["auth","net:api.example.com"],"contributes":{"authentication":{"type":"oauth2-pkce","providerName":"Service","clientId":"public","authorizationEndpoint":"https://login.example.com/oauth/authorize","tokenEndpoint":"https://login.example.com/oauth/token","scopes":["read"],"apiHosts":["api.example.com"]}}}}"#;
        let mut parsed: GrainPack = serde_json::from_str(raw).unwrap();
        parsed
            .manifest
            .contributes
            .authentication
            .as_mut()
            .unwrap()
            .authorization_parameters
            .insert("STATE".into(), "override".into());
        assert!(parsed.validate().is_err());

        let mut parsed: GrainPack = serde_json::from_str(raw).unwrap();
        parsed
            .manifest
            .contributes
            .authentication
            .as_mut()
            .unwrap()
            .token_endpoint = r"https://login.example.com\@evil.example/token".into();
        assert!(parsed.validate().is_err());
    }

    #[test]
    fn scripted_pack_entry_has_a_wire_size_ceiling() {
        let mut parsed: GrainPack = serde_json::from_str(
            r#"{"manifest":{"id":"com.x.large","name":"Large","version":"1","grainApi":"^1.0","tier":"scripted","entry_source":"x"}}"#,
        )
        .unwrap();
        parsed.manifest.entry_source = "x".repeat(PACK_ENTRY_MAX_BYTES + 1);
        assert!(parsed.validate().is_err());
    }
}
