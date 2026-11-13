//! Reach: the Agent's first-party OS tools (`docs/Reach/PLAN.md`).
//!
//! Reach lets the Agent operate the computer without a mouse or the screen's
//! pixels: it opens links, site searches and app pages, launches apps, types
//! into the focused field, presses a few keys, and controls media and volume.
//!
//! This module is the pure half: the tool schemas the model sees, argument
//! parsing, the link policy, and every URL Reach builds. It performs no I/O.
//! The desktop crate owns the OS adapters and decides nothing about safety
//! that this module already decides.
//!
//! The rules that matter:
//!
//! - The model never assembles a search or app-page URL. It names a site or an
//!   app and supplies the words; this module builds and encodes the URL.
//! - A link the model supplies directly must pass [`validate_link`]: a fixed
//!   allowlist of schemes, with protocol handlers abused in real attacks refused
//!   outright.
//! - Apps launch by name from the user's own Start menu catalogue, never by path
//!   and never with arguments.

use serde_json::{json, Value};

/// Extension id the Agent directory, prepared calls and settings use.
pub const EXTENSION_ID: &str = "grain.reach";
/// The name shown to the model and the user.
pub const PROVIDER_NAME: &str = "Reach";
/// Directory description the model reads when deciding whether to use Reach.
/// The Agent directory shows at most 240 bytes of it.
pub const DESCRIPTION: &str = "Operate this computer without the mouse: open websites and site searches (YouTube, Google, Maps), app deep links, launch or switch apps, type into the focused field, press Enter, media and volume, settings, files.";
/// Bumped whenever a tool schema or the policy changes meaning, so a call
/// prepared against older rules can never run under newer ones.
const POLICY_VERSION: u32 = 1;

/// Longest text Reach will paste in one call.
pub const MAX_TYPE_CHARS: usize = 20_000;
/// Longest link Reach will open.
const MAX_LINK_BYTES: usize = 8 * 1024;
/// Longest single search query or app-link field.
const MAX_FIELD_CHARS: usize = 4_000;

/// One Reach tool, as the Agent catalogue needs it.
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Extra words the lexical shortlist should match on.
    pub keywords: &'static [&'static str],
    /// Reads only (no effect on the computer).
    pub read_only: bool,
    pub schema: Value,
}

// ── Enumerations the model chooses from ────────────────────────────────────

/// Sites [`search_url`] knows how to search.
pub const SEARCH_SITES: &[&str] = &[
    "google",
    "bing",
    "duckduckgo",
    "first_result",
    "youtube",
    "youtube_music",
    "images",
    "news",
    "maps",
    "directions",
    "wikipedia",
    "amazon",
    "github",
    "reddit",
    "x",
    "spotify_web",
    "translate",
    "chatgpt",
    "claude",
    "perplexity",
];

/// Apps [`app_link_url`] can open a specific place in.
pub const LINK_APPS: &[&str] = &[
    "email",
    "gmail",
    "outlook_web",
    "google_calendar",
    "whatsapp",
    "telegram",
    "teams",
    "zoom",
    "spotify",
    "obsidian",
    "vscode",
    "cursor",
    "steam",
];

/// Settings pages `open_settings` accepts.
pub const SETTINGS_PAGES: &[&str] = &[
    "home",
    "display",
    "night_light",
    "sound",
    "bluetooth",
    "wifi",
    "network",
    "vpn",
    "notifications",
    "focus",
    "power",
    "battery",
    "storage",
    "apps",
    "default_apps",
    "startup_apps",
    "personalization",
    "background",
    "themes",
    "mouse",
    "keyboard",
    "printers",
    "date_time",
    "language",
    "accounts",
    "privacy_microphone",
    "privacy_camera",
    "windows_update",
    "about",
    "clipboard",
];

const KEYS: &[&str] = &[
    "enter",
    "ctrl_enter",
    "shift_enter",
    "tab",
    "shift_tab",
    "escape",
    "space",
];

const MEDIA_ACTIONS: &[&str] = &[
    "play_pause",
    "next",
    "previous",
    "stop",
    "volume_up",
    "volume_down",
    "mute",
    "set_volume",
];

/// Every Reach tool, in the order the catalogue lists them.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "open_link",
            title: "Open a link",
            description: "Open a web address or app link in its default app (web pages open in the browser). Use for a known URL, for example a YouTube watch URL from a search result. Allowed: https, http, mailto, tel, sms and common app links (spotify:, ms-settings:, zoommtg:, …). Optionally name a browser to open a web page in.",
            keywords: &["website", "url", "browser", "web page", "visit", "go to", "link", "video"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "The full link, e.g. https://www.youtube.com/watch?v=…"},
                    "browser": {"type": "string", "description": "Optional browser app name for a web page, e.g. Chrome, Edge, Firefox."}
                },
                "required": ["url"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "search_site",
            title: "Search a website",
            description: "Open a search on a website in the browser: Google, Bing, DuckDuckGo, YouTube, YouTube Music, images, news, Google Maps, directions to a place, Wikipedia, Amazon, GitHub, Reddit, X, Spotify web, Google Translate (set language), or prefill ChatGPT/Claude/Perplexity. Grain builds the URL. Use site=first_result to jump straight to the top result for a query (best effort).",
            keywords: &["search", "find", "look up", "youtube", "google", "maps", "directions", "translate", "amazon", "play video"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "site": {"type": "string", "enum": SEARCH_SITES},
                    "query": {"type": "string", "description": "What to search for; for directions, the destination."},
                    "language": {"type": "string", "description": "translate only: target language code, e.g. fr, de, hi."},
                    "browser": {"type": "string", "description": "Optional browser app name."}
                },
                "required": ["site", "query"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "open_app",
            title: "Open or switch to an app",
            description: "Launch an installed app by name, or bring it to the front if it is already running. Use the name as it appears in the Start menu (e.g. Spotify, Notepad, Visual Studio Code, WhatsApp).",
            keywords: &["launch", "start", "open", "switch to", "application", "program", "bring up"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App name."}
                },
                "required": ["name"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "open_in_app",
            title: "Open a place inside an app",
            description: "Open a specific place or a prefilled draft inside an app via its deep link. Nothing is sent: the user (or press_key) sends it. app=email|gmail|outlook_web (to, subject, text), google_calendar (subject as title, text as details), whatsapp (phone with country code, text), telegram (username or text), teams (to = email addresses, text), zoom (meeting_id, password), spotify (query), obsidian (vault; note to open, or note+text to create, or query to search), vscode|cursor (path, line), steam (game_id).",
            keywords: &["compose", "draft", "email", "message", "chat", "meeting", "join", "play music", "note", "file", "deep link"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "app": {"type": "string", "enum": LINK_APPS},
                    "to": {"type": "string", "description": "Recipient email address(es), comma separated."},
                    "subject": {"type": "string"},
                    "text": {"type": "string", "description": "Message, email body, or note content."},
                    "phone": {"type": "string", "description": "Phone number with country code."},
                    "username": {"type": "string"},
                    "query": {"type": "string"},
                    "meeting_id": {"type": "string"},
                    "password": {"type": "string", "description": "Meeting passcode."},
                    "vault": {"type": "string"},
                    "note": {"type": "string"},
                    "path": {"type": "string", "description": "Absolute file or folder path."},
                    "line": {"type": "integer", "minimum": 1},
                    "game_id": {"type": "string"}
                },
                "required": ["app"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "type_text",
            title: "Type into the focused field",
            description: "Type text into the text field that has focus in the app the user is working in (or the app Reach just opened). mode=insert puts it at the cursor; mode=replace replaces the field's whole content. Does not press Enter; call press_key for that. Fails if no editable field has focus.",
            keywords: &["type", "write", "paste", "fill", "enter text", "reply", "input", "text box"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string"},
                    "mode": {"type": "string", "enum": ["insert", "replace"], "default": "insert"}
                },
                "required": ["text"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "press_key",
            title: "Press a key",
            description: "Press a key in the app the user is working in (or the app Reach just opened): enter (send / submit), ctrl_enter, shift_enter (new line in chat apps), tab / shift_tab (move between fields), escape, space (play/pause the video on a YouTube or other video page; presses the focused button).",
            keywords: &["enter", "send", "submit", "press", "key", "tab", "escape", "space", "pause video"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string", "enum": KEYS}
                },
                "required": ["key"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "media_control",
            title: "Control media and volume",
            description: "Play/pause, next, previous or stop whatever media is playing, or change the system volume: volume_up, volume_down, mute (toggle), set_volume with level 0-100.",
            keywords: &["music", "pause", "play", "skip", "next song", "volume", "louder", "quieter", "mute"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": MEDIA_ACTIONS},
                    "level": {"type": "integer", "minimum": 0, "maximum": 100}
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "open_settings",
            title: "Open a system settings page",
            description: "Open a page of the system Settings app, e.g. bluetooth, wifi, display, sound, notifications, default_apps, windows_update, privacy_microphone.",
            keywords: &["settings", "preferences", "control panel", "configure", "bluetooth", "wifi"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "page": {"type": "string", "enum": SETTINGS_PAGES}
                },
                "required": ["page"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "show_file",
            title: "Show or open a file or folder",
            description: "Show a file or folder in the file manager, or open it with its default app (open=true). Accepts an absolute path or one starting with ~, Downloads, Documents, Desktop, Pictures, Music or Videos. Programs and scripts are never opened.",
            keywords: &["file", "folder", "explorer", "finder", "reveal", "downloads", "documents", "open file"],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "open": {"type": "boolean", "default": false}
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "list_windows",
            title: "List open windows",
            description: "List the open app windows (app and title) and describe what has keyboard focus: which app, and whether it is an editable text field. Use before typing when unsure where the cursor is.",
            keywords: &["windows", "what is open", "focused", "focus", "which app", "text field"],
            read_only: true,
            schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
    ]
}

/// Content digest of the tool set and policy, the Reach equivalent of an MCP
/// tool-list digest. Stable for a build.
pub fn catalog_digest() -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(POLICY_VERSION.to_le_bytes());
    for tool in tools() {
        hasher.update(tool.name.as_bytes());
        hasher.update([0]);
        hasher.update(tool.description.as_bytes());
        hasher.update([0]);
        hasher.update(tool.schema.to_string().as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ── Parsed calls ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Enter,
    CtrlEnter,
    ShiftEnter,
    Tab,
    ShiftTab,
    Escape,
    Space,
}

impl Key {
    pub fn label(self) -> &'static str {
        match self {
            Key::Enter => "Enter",
            Key::CtrlEnter => "Ctrl+Enter",
            Key::ShiftEnter => "Shift+Enter",
            Key::Tab => "Tab",
            Key::ShiftTab => "Shift+Tab",
            Key::Escape => "Escape",
            Key::Space => "Space",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Media {
    PlayPause,
    Next,
    Previous,
    Stop,
    VolumeUp,
    VolumeDown,
    Mute,
    SetVolume(u8),
}

/// One validated Reach call, ready for the OS adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// Open a validated link. `browser` is set only for http(s) links.
    OpenLink {
        url: String,
        browser: Option<String>,
        /// What the result message calls it ("YouTube search for …").
        label: String,
    },
    OpenApp {
        name: String,
    },
    TypeText {
        text: String,
        replace: bool,
    },
    PressKey(Key),
    Media(Media),
    /// A settings page id from [`SETTINGS_PAGES`].
    OpenSettings(&'static str),
    ShowFile {
        path: String,
        open: bool,
    },
    ListWindows,
}

/// Validate one tool call's arguments into a [`Call`]. Errors are short and
/// addressed to the model so it can correct the call.
pub fn parse(action: &str, args: &Value) -> Result<Call, String> {
    let args = args
        .as_object()
        .ok_or_else(|| "arguments must be an object".to_string())?;
    let text = |key: &str| -> Option<String> {
        args.get(key)
            .and_then(|value| match value {
                Value::String(s) => Some(s.trim().to_string()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .filter(|s| !s.is_empty())
    };
    let required = |key: &str| text(key).ok_or_else(|| format!("`{key}` is required"));
    let browser = text("browser").map(|b| bounded(&b, 80)).transpose()?;
    match action {
        "open_link" => {
            let url = validate_link(&required("url")?)?;
            let browser = browser.filter(|_| is_web(&url));
            Ok(Call::OpenLink {
                label: url.clone(),
                url,
                browser,
            })
        }
        "search_site" => {
            let site = required("site")?;
            let query = bounded(&required("query")?, MAX_FIELD_CHARS)?;
            let url = search_url(&site, &query, text("language").as_deref())?;
            Ok(Call::OpenLink {
                url,
                browser,
                label: format!("{} search for \"{}\"", site_label(&site), clip(&query, 80)),
            })
        }
        "open_app" => Ok(Call::OpenApp {
            name: bounded(&required("name")?, 120)?,
        }),
        "open_in_app" => {
            let app = required("app")?;
            let fields = AppLinkFields {
                to: text("to"),
                subject: text("subject"),
                text: args
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .filter(|s| !s.trim().is_empty()),
                phone: text("phone"),
                username: text("username"),
                query: text("query"),
                meeting_id: text("meeting_id"),
                password: text("password"),
                vault: text("vault"),
                note: text("note"),
                path: text("path"),
                line: args.get("line").and_then(Value::as_u64),
                game_id: text("game_id"),
            };
            let url = app_link_url(&app, &fields)?;
            Ok(Call::OpenLink {
                url,
                browser: None,
                label: format!("{} link", app_label(&app)),
            })
        }
        "type_text" => {
            // Not trimmed: leading/trailing whitespace and newlines are content.
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "`text` is required".to_string())?;
            if text.chars().count() > MAX_TYPE_CHARS {
                return Err(format!("`text` is longer than {MAX_TYPE_CHARS} characters"));
            }
            let replace = match text_mode(args.get("mode")) {
                Some("replace") => true,
                Some("insert") | None => false,
                Some(other) => return Err(format!("unknown mode `{other}`")),
            };
            Ok(Call::TypeText {
                text: text.to_string(),
                replace,
            })
        }
        "press_key" => Ok(Call::PressKey(match required("key")?.as_str() {
            "enter" => Key::Enter,
            "ctrl_enter" => Key::CtrlEnter,
            "shift_enter" => Key::ShiftEnter,
            "tab" => Key::Tab,
            "shift_tab" => Key::ShiftTab,
            "escape" => Key::Escape,
            "space" => Key::Space,
            other => return Err(format!("unknown key `{other}`; use one of {KEYS:?}")),
        })),
        "media_control" => Ok(Call::Media(match required("action")?.as_str() {
            "play_pause" => Media::PlayPause,
            "next" => Media::Next,
            "previous" => Media::Previous,
            "stop" => Media::Stop,
            "volume_up" => Media::VolumeUp,
            "volume_down" => Media::VolumeDown,
            "mute" => Media::Mute,
            "set_volume" => {
                let level = args
                    .get("level")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| "`level` (0-100) is required for set_volume".to_string())?;
                if !(0.0..=100.0).contains(&level) {
                    return Err("`level` must be between 0 and 100".into());
                }
                Media::SetVolume(level.round() as u8)
            }
            other => {
                return Err(format!(
                    "unknown action `{other}`; use one of {MEDIA_ACTIONS:?}"
                ))
            }
        })),
        "open_settings" => {
            let page = required("page")?;
            let page = SETTINGS_PAGES
                .iter()
                .find(|known| **known == page)
                .ok_or_else(|| format!("unknown settings page `{page}`"))?;
            Ok(Call::OpenSettings(page))
        }
        "show_file" => {
            let path = validate_path(&required("path")?)?;
            let open = args.get("open").and_then(Value::as_bool).unwrap_or(false);
            if open && !is_folder_like(&path) && is_dangerous_file(&path) {
                return Err(
                    "Reach never opens programs, scripts or installers. Use open=false to show it in its folder instead."
                        .into(),
                );
            }
            Ok(Call::ShowFile { path, open })
        }
        "list_windows" => Ok(Call::ListWindows),
        other => Err(format!("Reach has no action `{other}`")),
    }
}

fn text_mode(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn bounded(value: &str, max_chars: usize) -> Result<String, String> {
    if value.chars().count() > max_chars {
        return Err(format!("a value is longer than {max_chars} characters"));
    }
    if value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err("a value contains control characters".into());
    }
    Ok(value.to_string())
}

fn clip(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        let mut out: String = value.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

// ── Link policy ────────────────────────────────────────────────────────────

/// Schemes Reach opens. Each is a navigation or prefill handler of a mainstream
/// app; none runs code or fetches a remote document into a privileged viewer.
const ALLOWED_SCHEMES: &[&str] = &[
    "https",
    "http",
    "mailto",
    "tel",
    "sms",
    // Windows built-ins.
    "ms-settings",
    "ms-windows-store",
    "ms-clock",
    "ms-screenclip",
    "ms-screensketch",
    "calculator",
    "microsoft-edge",
    // macOS built-ins.
    "x-apple.systempreferences",
    "facetime",
    "maps",
    "music",
    // Apps.
    "spotify",
    "whatsapp",
    "tg",
    "msteams",
    "zoommtg",
    "zoomus",
    "slack",
    "discord",
    "signal",
    "sgnl",
    "skype",
    "vscode",
    "vscode-insiders",
    "cursor",
    "zed",
    "obsidian",
    "notion",
    "figma",
    "linear",
    "todoist",
    "things",
    "steam",
    "claude",
];

/// Handlers abused in real attacks or able to fetch and run remote content.
/// They are refused even if a future allowlist edit would admit them.
const REFUSED_SCHEMES: &[&str] = &[
    "file",
    "javascript",
    "vbscript",
    "data",
    "blob",
    "shell",
    "ms-msdt",
    "search-ms",
    "search",
    "ms-search",
    "ms-officecmd",
    "ms-appinstaller",
    "ms-word",
    "ms-excel",
    "ms-powerpoint",
    "ms-visio",
    "ms-access",
    "ms-publisher",
    "ms-project",
    "ms-infopath",
    "ms-spd",
    "ms-cxh",
    "ms-cxh-full",
    "hcp",
    "its",
    "ms-its",
    "mk",
    "res",
    "jnlp",
    "ldap",
    "ldaps",
    "smb",
    "nfs",
    "webdav",
    "dav",
    "ftp",
    "telnet",
    "ssh",
];

/// Validate a link the model supplied. Returns the normalised link.
pub fn validate_link(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("the link is empty".into());
    }
    if raw.len() > MAX_LINK_BYTES {
        return Err("the link is too long".into());
    }
    if raw.chars().any(char::is_control) {
        return Err("the link contains control characters".into());
    }
    if raw.starts_with("\\\\") || raw.starts_with("//") {
        return Err("network paths are not links Reach opens".into());
    }
    let Some((scheme, rest)) = raw.split_once(':') else {
        return Err("the link needs a scheme such as https://".into());
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return Err("the link needs a scheme such as https://".into());
    }
    if REFUSED_SCHEMES.contains(&scheme.as_str()) {
        return Err(format!("`{scheme}:` links are refused for safety"));
    }
    if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
        return Err(format!(
            "Reach does not open `{scheme}:` links. Use https, mailto, tel, sms or a supported app link."
        ));
    }
    // A backslash would let a handler reinterpret the remainder as a UNC path.
    if rest.contains('\\') {
        return Err("the link contains a backslash".into());
    }
    if scheme == "http" || scheme == "https" {
        let parsed = url::Url::parse(raw)
            .map_err(|error| format!("the link is not a valid URL: {error}"))?;
        if parsed.host_str().is_none_or(str::is_empty) {
            return Err("the web link has no host".into());
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("links with embedded credentials are refused".into());
        }
        return Ok(parsed.to_string());
    }
    if scheme == "microsoft-edge" {
        // Only `microsoft-edge:https://…`, which opens a web page in Edge.
        let inner = rest.trim_start_matches('/');
        let inner_scheme = inner.split(':').next().unwrap_or("").to_ascii_lowercase();
        if inner_scheme != "https" && inner_scheme != "http" {
            return Err("microsoft-edge: links must wrap an https address".into());
        }
        validate_link(inner)?;
    }
    Ok(format!("{scheme}:{rest}"))
}

/// Whether a validated link is a web page (the only kind a browser override applies to).
pub fn is_web(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

// ── URL building ───────────────────────────────────────────────────────────

/// Percent-encode everything except RFC 3986 unreserved characters and `keep`.
fn encode(value: &str, keep: &[char]) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~') || keep.contains(&c) {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for byte in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

fn q(value: &str) -> String {
    encode(value, &[])
}

fn site_label(site: &str) -> &str {
    match site {
        "google" => "Google",
        "bing" => "Bing",
        "duckduckgo" => "DuckDuckGo",
        "first_result" => "Top result",
        "youtube" => "YouTube",
        "youtube_music" => "YouTube Music",
        "images" => "Image",
        "news" => "News",
        "maps" => "Maps",
        "directions" => "Directions",
        "wikipedia" => "Wikipedia",
        "amazon" => "Amazon",
        "github" => "GitHub",
        "reddit" => "Reddit",
        "x" => "X",
        "spotify_web" => "Spotify",
        "translate" => "Translate",
        "chatgpt" => "ChatGPT",
        "claude" => "Claude",
        "perplexity" => "Perplexity",
        other => other,
    }
}

/// The search URL for `site`. The query is always encoded by this function.
pub fn search_url(site: &str, query: &str, language: Option<&str>) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("`query` is required".into());
    }
    let q = q(query);
    Ok(match site {
        "google" => format!("https://www.google.com/search?q={q}"),
        "bing" => format!("https://www.bing.com/search?q={q}"),
        "duckduckgo" => format!("https://duckduckgo.com/?q={q}"),
        // DuckDuckGo's `!ducky` bang redirects to the first result.
        "first_result" => format!("https://duckduckgo.com/?q=%21ducky%20{q}"),
        "youtube" => format!("https://www.youtube.com/results?search_query={q}"),
        "youtube_music" => format!("https://music.youtube.com/search?q={q}"),
        "images" => format!("https://www.google.com/search?udm=2&q={q}"),
        "news" => format!("https://news.google.com/search?q={q}"),
        "maps" => format!("https://www.google.com/maps/search/?api=1&query={q}"),
        "directions" => format!("https://www.google.com/maps/dir/?api=1&destination={q}"),
        "wikipedia" => format!("https://en.wikipedia.org/w/index.php?search={q}"),
        "amazon" => format!("https://www.amazon.com/s?k={q}"),
        "github" => format!("https://github.com/search?q={q}"),
        "reddit" => format!("https://www.reddit.com/search/?q={q}"),
        "x" => format!("https://x.com/search?q={q}"),
        "spotify_web" => format!("https://open.spotify.com/search/{q}"),
        "translate" => {
            let language = language
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .unwrap_or("en");
            if language.len() > 12
                || !language
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err("`language` must be a language code such as fr or pt-BR".into());
            }
            format!("https://translate.google.com/?sl=auto&tl={language}&text={q}&op=translate")
        }
        "chatgpt" => format!("https://chatgpt.com/?q={q}"),
        "claude" => format!("https://claude.ai/new?q={q}"),
        "perplexity" => format!("https://www.perplexity.ai/search?q={q}"),
        other => {
            return Err(format!(
                "unknown site `{other}`; use one of {SEARCH_SITES:?}, or open_link for another site"
            ))
        }
    })
}

/// Fields `open_in_app` accepts. Which ones an app uses is documented in the tool.
#[derive(Debug, Default, Clone)]
pub struct AppLinkFields {
    pub to: Option<String>,
    pub subject: Option<String>,
    pub text: Option<String>,
    pub phone: Option<String>,
    pub username: Option<String>,
    pub query: Option<String>,
    pub meeting_id: Option<String>,
    pub password: Option<String>,
    pub vault: Option<String>,
    pub note: Option<String>,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub game_id: Option<String>,
}

fn app_label(app: &str) -> &str {
    match app {
        "email" => "Email",
        "gmail" => "Gmail",
        "outlook_web" => "Outlook",
        "google_calendar" => "Google Calendar",
        "whatsapp" => "WhatsApp",
        "telegram" => "Telegram",
        "teams" => "Teams",
        "zoom" => "Zoom",
        "spotify" => "Spotify",
        "obsidian" => "Obsidian",
        "vscode" => "VS Code",
        "cursor" => "Cursor",
        "steam" => "Steam",
        other => other,
    }
}

fn need<'a>(value: &'a Option<String>, key: &str, app: &str) -> Result<&'a str, String> {
    value
        .as_deref()
        .ok_or_else(|| format!("`{key}` is required for {app}"))
}

fn digits(value: &str, key: &str) -> Result<String, String> {
    let out: String = value.chars().filter(char::is_ascii_digit).collect();
    if out.is_empty() || out.len() > 20 {
        return Err(format!("`{key}` must be a number"));
    }
    Ok(out)
}

/// Query string from (name, value) pairs, skipping absent values.
fn query_string(pairs: &[(&str, Option<&str>)]) -> String {
    pairs
        .iter()
        .filter_map(|(name, value)| value.map(|value| format!("{name}={}", q(value))))
        .collect::<Vec<_>>()
        .join("&")
}

fn with_query(base: &str, pairs: &[(&str, Option<&str>)]) -> String {
    let query = query_string(pairs);
    if query.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{query}")
    }
}

fn recipients(to: Option<&str>) -> Result<Option<String>, String> {
    let Some(to) = to else { return Ok(None) };
    let list: Vec<&str> = to
        .split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    for address in &list {
        if !address.contains('@')
            || address
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '<' | '>' | '?' | '&' | '#'))
        {
            return Err(format!("`{address}` is not an email address"));
        }
    }
    Ok((!list.is_empty()).then(|| list.join(",")))
}

/// The deep link for `app` with `fields`. Every value is encoded here.
pub fn app_link_url(app: &str, fields: &AppLinkFields) -> Result<String, String> {
    for value in [
        &fields.to,
        &fields.subject,
        &fields.text,
        &fields.phone,
        &fields.username,
        &fields.query,
        &fields.meeting_id,
        &fields.password,
        &fields.vault,
        &fields.note,
        &fields.path,
        &fields.game_id,
    ]
    .into_iter()
    .flatten()
    {
        bounded(value, MAX_FIELD_CHARS)?;
    }
    let to = recipients(fields.to.as_deref())?;
    let subject = fields.subject.as_deref();
    let text = fields.text.as_deref();
    let url = match app {
        "email" => {
            let to = to
                .as_deref()
                .map(|to| encode(to, &['@', ',']))
                .unwrap_or_default();
            with_query(
                &format!("mailto:{to}"),
                &[("subject", subject), ("body", text)],
            )
        }
        "gmail" => with_query(
            "https://mail.google.com/mail/",
            &[
                ("view", Some("cm")),
                ("fs", Some("1")),
                ("to", to.as_deref()),
                ("su", subject),
                ("body", text),
            ],
        ),
        "outlook_web" => with_query(
            "https://outlook.office.com/mail/deeplink/compose",
            &[("to", to.as_deref()), ("subject", subject), ("body", text)],
        ),
        "google_calendar" => with_query(
            "https://calendar.google.com/calendar/render",
            &[
                ("action", Some("TEMPLATE")),
                ("text", subject),
                ("details", text),
            ],
        ),
        "whatsapp" => {
            let phone = fields
                .phone
                .as_deref()
                .map(|p| digits(p, "phone"))
                .transpose()?;
            if phone.is_none() && text.is_none() {
                return Err("whatsapp needs `phone`, `text`, or both".into());
            }
            with_query(
                "whatsapp://send",
                &[("phone", phone.as_deref()), ("text", text)],
            )
        }
        "telegram" => match (fields.username.as_deref(), text) {
            (Some(username), _) => {
                let username = username.trim_start_matches('@');
                if !username
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    return Err("`username` must be a Telegram username".into());
                }
                format!("tg://resolve?domain={username}")
            }
            (None, Some(text)) => format!("tg://msg?text={}", q(text)),
            (None, None) => return Err("telegram needs `username` or `text`".into()),
        },
        "teams" => {
            let users =
                to.ok_or_else(|| "`to` (email addresses) is required for teams".to_string())?;
            with_query(
                "msteams:/l/chat/0/0",
                &[("users", Some(users.as_str())), ("message", text)],
            )
        }
        "zoom" => {
            let id = digits(
                need(&fields.meeting_id, "meeting_id", "zoom")?,
                "meeting_id",
            )?;
            with_query(
                "zoommtg://zoom.us/join",
                &[
                    ("confno", Some(id.as_str())),
                    ("pwd", fields.password.as_deref()),
                ],
            )
        }
        "spotify" => format!(
            "spotify:search:{}",
            q(need(&fields.query, "query", "spotify")?)
        ),
        "obsidian" => {
            let vault = fields.vault.as_deref();
            match (fields.note.as_deref(), text, fields.query.as_deref()) {
                (name, Some(content), _) => with_query(
                    "obsidian://new",
                    &[("vault", vault), ("name", name), ("content", Some(content))],
                ),
                (_, None, Some(query)) => with_query(
                    "obsidian://search",
                    &[("vault", vault), ("query", Some(query))],
                ),
                (Some(note), None, None) => {
                    with_query("obsidian://open", &[("vault", vault), ("file", Some(note))])
                }
                (None, None, None) => match vault {
                    Some(vault) => format!("obsidian://open?vault={}", q(vault)),
                    None => return Err("obsidian needs `note`, `text`, `query` or `vault`".into()),
                },
            }
        }
        "vscode" | "cursor" => {
            let path = validate_path(need(&fields.path, "path", app)?)?;
            if path.starts_with('~') || !is_absolute(&path) {
                return Err("`path` must be absolute for an editor link".into());
            }
            let path = path.replace('\\', "/");
            let path = encode(path.trim_start_matches('/'), &['/', ':']);
            let line = fields
                .line
                .map(|line| format!(":{line}"))
                .unwrap_or_default();
            let scheme = if app == "vscode" { "vscode" } else { "cursor" };
            format!("{scheme}://file/{path}{line}")
        }
        "steam" => format!(
            "steam://rungameid/{}",
            digits(need(&fields.game_id, "game_id", "steam")?, "game_id")?
        ),
        other => {
            return Err(format!(
                "unknown app `{other}`; use one of {LINK_APPS:?}, or open_app to just launch it"
            ))
        }
    };
    validate_link(&url)
}

/// `ms-settings:` page for a [`SETTINGS_PAGES`] id.
pub fn windows_settings_uri(page: &str) -> &'static str {
    match page {
        "display" => "ms-settings:display",
        "night_light" => "ms-settings:nightlight",
        "sound" => "ms-settings:sound",
        "bluetooth" => "ms-settings:bluetooth",
        "wifi" => "ms-settings:network-wifi",
        "network" => "ms-settings:network-status",
        "vpn" => "ms-settings:network-vpn",
        "notifications" => "ms-settings:notifications",
        "focus" => "ms-settings:quiethours",
        "power" => "ms-settings:powersleep",
        "battery" => "ms-settings:batterysaver",
        "storage" => "ms-settings:storagesense",
        "apps" => "ms-settings:appsfeatures",
        "default_apps" => "ms-settings:defaultapps",
        "startup_apps" => "ms-settings:startupapps",
        "personalization" => "ms-settings:personalization",
        "background" => "ms-settings:personalization-background",
        "themes" => "ms-settings:themes",
        "mouse" => "ms-settings:mousetouchpad",
        "keyboard" => "ms-settings:typing",
        "printers" => "ms-settings:printers",
        "date_time" => "ms-settings:dateandtime",
        "language" => "ms-settings:regionlanguage",
        "accounts" => "ms-settings:yourinfo",
        "privacy_microphone" => "ms-settings:privacy-microphone",
        "privacy_camera" => "ms-settings:privacy-webcam",
        "windows_update" => "ms-settings:windowsupdate",
        "about" => "ms-settings:about",
        "clipboard" => "ms-settings:clipboard",
        _ => "ms-settings:",
    }
}

// ── Paths ──────────────────────────────────────────────────────────────────

/// Folder names `show_file` accepts as the first path segment.
pub const KNOWN_FOLDERS: &[&str] = &[
    "downloads",
    "documents",
    "desktop",
    "pictures",
    "music",
    "videos",
];

fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// Validate a path the model supplied. Local only: network and device paths
/// are refused. `~` and known folder names are expanded by the OS adapter.
pub fn validate_path(raw: &str) -> Result<String, String> {
    let path = raw.trim().trim_matches('"');
    if path.is_empty() {
        return Err("`path` is empty".into());
    }
    if path.len() > 1024 || path.chars().any(char::is_control) {
        return Err("`path` is not a valid path".into());
    }
    if path.starts_with("\\\\") || path.starts_with("//") {
        return Err("network and device paths are not opened".into());
    }
    if path.contains("://") || path.to_ascii_lowercase().starts_with("file:") {
        return Err("`path` must be a file path, not a link".into());
    }
    let first = path
        .split(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !(is_absolute(path) || first == "~" || KNOWN_FOLDERS.contains(&first.as_str())) {
        return Err(
            "`path` must be absolute or start with ~, Downloads, Documents, Desktop, Pictures, Music or Videos"
                .into(),
        );
    }
    Ok(path.to_string())
}

fn extension(path: &str) -> Option<String> {
    // Windows ignores trailing dots and spaces on file names, so `x.ps1.` runs
    // as `x.ps1`.
    let name = path
        .rsplit(['\\', '/'])
        .next()?
        .trim_end_matches(['.', ' ']);
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

fn is_folder_like(path: &str) -> bool {
    path.ends_with(['\\', '/']) || extension(path).is_none()
}

/// File types that run code, install software or redirect elsewhere when
/// opened. The OS adapter adds the platform's own check (`AssocIsDangerous`).
pub fn is_dangerous_file(path: &str) -> bool {
    const DANGEROUS: &[&str] = &[
        "exe",
        "com",
        "scr",
        "pif",
        "bat",
        "cmd",
        "ps1",
        "psm1",
        "psd1",
        "ps1xml",
        "psc1",
        "vbs",
        "vbe",
        "js",
        "jse",
        "wsf",
        "wsh",
        "ws",
        "wsc",
        "hta",
        "msi",
        "msp",
        "mst",
        "msix",
        "msixbundle",
        "appx",
        "appxbundle",
        "appinstaller",
        "application",
        "appref-ms",
        "lnk",
        "url",
        "website",
        "cpl",
        "msc",
        "jar",
        "jnlp",
        "reg",
        "inf",
        "scf",
        "dll",
        "ocx",
        "sys",
        "drv",
        "chm",
        "hlp",
        "gadget",
        "library-ms",
        "search-ms",
        "searchconnector-ms",
        "settingcontent-ms",
        "diagcab",
        "diagcfg",
        "diagpkg",
        "xbap",
        "xll",
        "iqy",
        "slk",
        "sh",
        "bash",
        "zsh",
        "command",
        "app",
        "pkg",
        "dmg",
        "workflow",
        "scpt",
        "applescript",
        "desktop",
        "run",
        "bin",
        "py",
        "pyw",
        "pyz",
        "rb",
        "pl",
        "php",
        "ahk",
        "au3",
        "vb",
        "iso",
        "img",
        "vhd",
        "vhdx",
    ];
    extension(path).is_some_and(|ext| DANGEROUS.contains(&ext.as_str()))
}

// ── App name matching ──────────────────────────────────────────────────────

fn normalise(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Common spoken names for apps whose Start menu name differs.
fn alias(query: &str) -> Option<&'static str> {
    Some(match query {
        "vs code" | "vscode" | "code" => "visual studio code",
        "chrome" => "google chrome",
        "edge" => "microsoft edge",
        "word" => "word",
        "excel" => "excel",
        "powerpoint" => "powerpoint",
        "outlook" => "outlook",
        "teams" | "ms teams" => "microsoft teams",
        "terminal" => "terminal",
        "explorer" | "files" | "file manager" => "file explorer",
        "store" => "microsoft store",
        "settings" | "control panel" => "settings",
        "calc" => "calculator",
        _ => return None,
    })
}

/// Index of the catalogue name that best matches `query`, or `None`. Exact
/// names win, then prefixes, then whole-word containment; ties go to the
/// shorter name (the unqualified product).
pub fn best_app_match<'a>(query: &str, names: impl IntoIterator<Item = &'a str>) -> Option<usize> {
    let query = normalise(query);
    if query.is_empty() {
        return None;
    }
    let target = alias(&query).map(str::to_string).unwrap_or(query.clone());
    let words: Vec<&str> = target.split(' ').collect();
    let mut best: Option<(u32, usize, usize)> = None;
    for (index, name) in names.into_iter().enumerate() {
        let name = normalise(name);
        let name_words: Vec<&str> = name.split(' ').collect();
        let score = if name == target || name == query {
            100
        } else if name.starts_with(&format!("{target} ")) {
            80
        } else if name_words
            .windows(words.len())
            .any(|w| w == words.as_slice())
        {
            60
        } else if words.iter().all(|w| name_words.contains(w)) {
            40
        } else if words.len() == 1
            && name_words.iter().any(|w| w.starts_with(words[0]))
            && words[0].len() >= 3
        {
            20
        } else {
            0
        };
        if score == 0 {
            continue;
        }
        let len = name.chars().count();
        let better = match best {
            None => true,
            Some((best_score, best_len, _)) => {
                score > best_score || (score == best_score && len < best_len)
            }
        };
        if better {
            best = Some((score, len, index));
        }
    }
    best.map(|(_, _, index)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_closed_object_schema_and_a_parse_arm() {
        let tools = tools();
        assert_eq!(tools.len(), 10);
        for tool in &tools {
            assert_eq!(tool.schema["type"], "object", "{}", tool.name);
            assert_eq!(tool.schema["additionalProperties"], false, "{}", tool.name);
            let error = parse(tool.name, &json!({})).err().unwrap_or_default();
            assert!(
                !error.contains("has no action"),
                "{} has no parse arm",
                tool.name
            );
        }
        assert_eq!(catalog_digest(), catalog_digest());
        assert_eq!(catalog_digest().len(), 64);
        // The Agent directory shows at most 240 bytes of a description.
        assert!(DESCRIPTION.len() <= 240, "{}", DESCRIPTION.len());
    }

    #[test]
    fn web_and_known_app_links_open_and_dangerous_handlers_are_refused() {
        for ok in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "http://example.com",
            "mailto:sam@example.com?subject=Hi",
            "tel:+15551234567",
            "spotify:search:daft%20punk",
            "ms-settings:bluetooth",
            "zoommtg://zoom.us/join?confno=123",
            "MS-SETTINGS:display",
            "microsoft-edge:https://example.com",
        ] {
            assert!(validate_link(ok).is_ok(), "{ok} should open");
        }
        for bad in [
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "data:text/html,hi",
            "ms-msdt:/id PCWDiagnostic",
            "search-ms:query=x&crumb=location:\\\\evil\\share",
            "ms-officecmd:{}",
            "ms-appinstaller:?source=https://evil/x.msix",
            "ms-word:ofe|u|https://evil/doc.docx",
            "\\\\evil\\share\\x.exe",
            "//evil/share",
            "https://user:pass@example.com",
            "https://",
            "unknownapp://do-something",
            "C:\\Windows\\notepad.exe",
            "spotify:search:x\\y",
            "microsoft-edge:file:///C:/x",
            "https://example.com/\u{0}",
        ] {
            assert!(validate_link(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn site_searches_are_built_and_encoded_by_the_host() {
        assert_eq!(
            search_url("youtube", "lo-fi beats & rain", None).unwrap(),
            "https://www.youtube.com/results?search_query=lo-fi%20beats%20%26%20rain"
        );
        assert_eq!(
            search_url("first_result", "veritasium longest line", None).unwrap(),
            "https://duckduckgo.com/?q=%21ducky%20veritasium%20longest%20line"
        );
        assert!(search_url("translate", "hello", Some("fr"))
            .unwrap()
            .contains("tl=fr"));
        assert!(search_url("translate", "hello", Some("fr&x=1")).is_err());
        assert!(search_url("evil", "x", None).is_err());
        assert!(search_url("google", "  ", None).is_err());
        for site in SEARCH_SITES {
            let url = search_url(site, "a b", Some("de")).unwrap();
            assert!(validate_link(&url).is_ok(), "{site}: {url}");
        }
    }

    #[test]
    fn app_links_prefill_without_sending_and_reject_bad_fields() {
        let mail = app_link_url(
            "email",
            &AppLinkFields {
                to: Some("sam@x.com, lee@y.org".into()),
                subject: Some("Friday?".into()),
                text: Some("See you at 6\nThanks".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            mail,
            "mailto:sam@x.com,lee@y.org?subject=Friday%3F&body=See%20you%20at%206%0AThanks"
        );
        let whatsapp = app_link_url(
            "whatsapp",
            &AppLinkFields {
                phone: Some("+44 7700 900123".into()),
                text: Some("on my way".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            whatsapp,
            "whatsapp://send?phone=447700900123&text=on%20my%20way"
        );
        let code = app_link_url(
            "vscode",
            &AppLinkFields {
                path: Some(r"C:\Projects\My App\main.rs".into()),
                line: Some(12),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(code, "vscode://file/C:/Projects/My%20App/main.rs:12");
        assert_eq!(
            app_link_url(
                "spotify",
                &AppLinkFields {
                    query: Some("daft punk".into()),
                    ..Default::default()
                }
            )
            .unwrap(),
            "spotify:search:daft%20punk"
        );
        assert!(app_link_url(
            "email",
            &AppLinkFields {
                to: Some("not an address".into()),
                ..Default::default()
            }
        )
        .is_err());
        assert!(app_link_url("zoom", &AppLinkFields::default()).is_err());
        assert!(app_link_url(
            "telegram",
            &AppLinkFields {
                username: Some("bad name&x".into()),
                ..Default::default()
            }
        )
        .is_err());
        assert!(app_link_url(
            "vscode",
            &AppLinkFields {
                path: Some(r"\\evil\share\x".into()),
                ..Default::default()
            }
        )
        .is_err());
        assert!(app_link_url("calculator", &AppLinkFields::default()).is_err());
    }

    #[test]
    fn parse_validates_arguments_and_bounds() {
        assert_eq!(
            parse("press_key", &json!({"key": "enter"})).unwrap(),
            Call::PressKey(Key::Enter)
        );
        assert_eq!(
            parse("press_key", &json!({"key": "space"})).unwrap(),
            Call::PressKey(Key::Space)
        );
        assert!(parse("press_key", &json!({"key": "alt_f4"})).is_err());
        assert_eq!(
            parse(
                "media_control",
                &json!({"action": "set_volume", "level": 35})
            )
            .unwrap(),
            Call::Media(Media::SetVolume(35))
        );
        assert!(parse(
            "media_control",
            &json!({"action": "set_volume", "level": 140})
        )
        .is_err());
        assert_eq!(
            parse("type_text", &json!({"text": " hi\n", "mode": "replace"})).unwrap(),
            Call::TypeText {
                text: " hi\n".into(),
                replace: true
            }
        );
        let long = "x".repeat(MAX_TYPE_CHARS + 1);
        assert!(parse("type_text", &json!({ "text": long })).is_err());
        assert!(parse("open_link", &json!({"url": "file:///etc/passwd"})).is_err());
        assert_eq!(
            parse(
                "open_link",
                &json!({"url": "spotify:search:x", "browser": "Chrome"})
            )
            .unwrap(),
            Call::OpenLink {
                url: "spotify:search:x".into(),
                browser: None,
                label: "spotify:search:x".into()
            }
        );
        assert_eq!(
            parse("open_settings", &json!({"page": "bluetooth"})).unwrap(),
            Call::OpenSettings("bluetooth")
        );
        assert!(parse("open_settings", &json!({"page": "registry"})).is_err());
        assert!(parse("nope", &json!({})).is_err());
        assert!(parse("open_app", &json!("Spotify")).is_err());
    }

    #[test]
    fn files_open_only_when_they_cannot_run_code() {
        assert!(parse(
            "show_file",
            &json!({"path": "Downloads/report.pdf", "open": true})
        )
        .is_ok());
        assert!(parse(
            "show_file",
            &json!({"path": "C:\\Users\\me\\Documents", "open": true})
        )
        .is_ok());
        assert!(parse(
            "show_file",
            &json!({"path": "~/Downloads/setup.exe", "open": false})
        )
        .is_ok());
        for bad in [
            "~/Downloads/setup.exe",
            "C:\\x\\run.BAT",
            "C:\\x\\invoice.pdf.lnk",
            "C:\\x\\tool.ps1. ",
            "Desktop/game.url",
        ] {
            assert!(
                parse("show_file", &json!({"path": bad, "open": true})).is_err(),
                "{bad} must not open"
            );
        }
        for bad in [
            "\\\\host\\share\\x.pdf",
            "relative/x.pdf",
            "https://x.com/a.pdf",
            "file:///C:/a.pdf",
        ] {
            assert!(validate_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn app_names_match_spoken_forms_and_prefer_the_plain_product() {
        let names = [
            "Visual Studio Code",
            "Visual Studio Code - Insiders",
            "Google Chrome",
            "Spotify",
            "Microsoft Edge",
            "Windows PowerShell",
            "Windows PowerShell ISE",
            "Notepad",
            "Notepad++",
        ];
        let pick = |q: &str| best_app_match(q, names.iter().copied()).map(|i| names[i]);
        assert_eq!(pick("vs code"), Some("Visual Studio Code"));
        assert_eq!(pick("chrome"), Some("Google Chrome"));
        assert_eq!(pick("SPOTIFY"), Some("Spotify"));
        assert_eq!(pick("edge"), Some("Microsoft Edge"));
        assert_eq!(pick("powershell"), Some("Windows PowerShell"));
        assert_eq!(pick("notepad"), Some("Notepad"));
        assert_eq!(pick("spot"), Some("Spotify"));
        assert_eq!(pick("photoshop"), None);
        assert_eq!(pick(""), None);
    }
}
