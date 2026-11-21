//! [GRAIN] Recall MCP: the "Use your meetings in AI apps" switch and the
//! one-click Connect for AI apps.
//!
//! The MCP server itself is a separate binary (`grain-recall-mcp`, see
//! `crates/grain-recall-mcp`) that an AI app launches over stdio. Nothing here
//! runs in the background: these commands only write a gate file and edit the
//! AI apps' own MCP config files when the user asks. Off means fully off: the
//! gate file is removed (a running sidecar refuses at once) and every entry
//! Grain added is taken out again, so no AI app starts the sidecar.
//!
//! The gate file is the switch's only state, so it can never disagree with a
//! setting. Config edits touch only our `grain-recall` entry, write
//! atomically, and refuse a file they cannot parse rather than replace it.

use std::fs;
use std::path::{Path, PathBuf};

use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use specta::Type;
use tauri::{AppHandle, Manager, WebviewWindow};

/// The server's name in every AI app's config.
const SERVER_NAME: &str = "grain-recall";
#[cfg(windows)]
const SERVER_EXE: &str = "grain-recall-mcp.exe";
#[cfg(not(windows))]
const SERVER_EXE: &str = "grain-recall-mcp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum McpClient {
    ClaudeDesktop,
    ClaudeCode,
    GeminiCli,
    Codex,
    Cursor,
}

const CLIENTS: [McpClient; 5] = [
    McpClient::ClaudeDesktop,
    McpClient::ClaudeCode,
    McpClient::GeminiCli,
    McpClient::Codex,
    McpClient::Cursor,
];

impl McpClient {
    fn name(self) -> &'static str {
        match self {
            Self::ClaudeDesktop => "Claude Desktop",
            Self::ClaudeCode => "Claude Code",
            Self::GeminiCli => "Gemini CLI",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum McpClientState {
    /// The app does not seem to be installed for this user.
    NotInstalled,
    NotConnected,
    Connected,
    /// Our entry exists but points at an old server path or data folder.
    NeedsUpdate,
    /// The app's config file exists but could not be parsed; Grain leaves it alone.
    Unreadable,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct McpClientStatus {
    pub client: McpClient,
    pub name: String,
    pub state: McpClientState,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct RecallMcpStatus {
    pub enabled: bool,
    /// The bundled server; `None` when it is missing from this install.
    pub server_path: Option<String>,
    pub clients: Vec<McpClientStatus>,
    /// Config for any other MCP app, as JSON.
    pub setup_json: String,
}

// --- What an entry looks like --------------------------------------------------

/// The command an AI app runs to start the server.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Launch {
    command: String,
    args: Vec<String>,
}

impl Launch {
    fn new(server: &Path, data_dir: &Path) -> Self {
        Self {
            command: server.to_string_lossy().into_owned(),
            args: vec!["--data-dir".into(), data_dir.to_string_lossy().into_owned()],
        }
    }

    fn matches_json(&self, entry: &Value) -> bool {
        entry.get("command").and_then(Value::as_str) == Some(&self.command)
            && entry.get("args") == Some(&json!(self.args))
    }

    fn shell_words(&self) -> String {
        std::iter::once(&self.command)
            .chain(&self.args)
            .map(|w| {
                if w.contains([' ', '\t']) {
                    format!("\"{w}\"")
                } else {
                    w.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// --- Where each app keeps its config -------------------------------------------

/// The user folders the AI apps' configs live under.
#[derive(Debug, Clone)]
struct Places {
    home: PathBuf,
    /// Roaming AppData / Application Support / ~/.config.
    config: PathBuf,
    /// Local AppData (Windows) / Application Support / ~/.local/share.
    local: PathBuf,
}

impl Places {
    fn detect(app: &AppHandle) -> Result<Self, String> {
        let path = app.path();
        Ok(Self {
            home: path.home_dir().map_err(|e| e.to_string())?,
            config: path.config_dir().map_err(|e| e.to_string())?,
            local: path.local_data_dir().map_err(|e| e.to_string())?,
        })
    }

    /// Claude Desktop's folder. The Microsoft Store build redirects its
    /// roaming data into its package folder, so that copy wins when present.
    fn claude_desktop_dir(&self) -> PathBuf {
        let packaged = fs::read_dir(self.local.join("Packages"))
            .ok()
            .and_then(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with("Claude_"))
                    .map(|e| e.path().join("LocalCache").join("Roaming").join("Claude"))
                    .find(|p| p.is_dir())
            });
        packaged.unwrap_or_else(|| self.config.join("Claude"))
    }

    /// The JSON config with an `mcpServers` map, and the folder whose
    /// existence means the app is installed.
    fn json_config(&self, client: McpClient) -> Option<(PathBuf, PathBuf)> {
        let dir = match client {
            McpClient::ClaudeDesktop => self.claude_desktop_dir(),
            McpClient::GeminiCli => self.home.join(".gemini"),
            McpClient::Cursor => self.home.join(".cursor"),
            McpClient::ClaudeCode | McpClient::Codex => return None,
        };
        let file = match client {
            McpClient::ClaudeDesktop => "claude_desktop_config.json",
            McpClient::GeminiCli => "settings.json",
            _ => "mcp.json",
        };
        Some((dir.join(file), dir))
    }

    fn codex_config(&self) -> (PathBuf, PathBuf) {
        let dir = self.home.join(".codex");
        (dir.join("config.toml"), dir)
    }

    /// Claude Code's user config, read only to show the state.
    fn claude_code_state_file(&self) -> PathBuf {
        self.home.join(".claude.json")
    }

    /// The `claude` CLI: on PATH, or where its installers put it.
    fn claude_cli(&self) -> Option<PathBuf> {
        let names: &[&str] = if cfg!(windows) {
            &["claude.exe", "claude.cmd"]
        } else {
            &["claude"]
        };
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        dirs.push(self.home.join(".local").join("bin"));
        dirs.push(self.home.join(".claude").join("local"));
        if cfg!(windows) {
            dirs.push(self.config.join("npm"));
        }
        names
            .iter()
            .flat_map(|name| dirs.iter().map(move |d| d.join(name)))
            .find(|p| p.is_file())
    }
}

// --- JSON configs (Claude Desktop, Gemini CLI, Cursor) -------------------------

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("grain-tmp");
    fs::write(&tmp, bytes)
        .and_then(|()| fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("Could not save {}: {e}", path.display())
        })
}

/// The file's JSON object; an absent or empty file is an empty object.
fn read_json(path: &Path) -> Result<Map<String, Value>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
    };
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(not_plain_json(path)),
    }
}

fn not_plain_json(path: &Path) -> String {
    format!(
        "{} is not plain JSON, so Grain left it unchanged. Add the setup by hand.",
        path.display()
    )
}

fn json_state(path: &Path, launch: &Launch) -> McpClientState {
    match read_json(path) {
        Err(_) => McpClientState::Unreadable,
        Ok(root) => match root.get("mcpServers").and_then(|s| s.get(SERVER_NAME)) {
            None => McpClientState::NotConnected,
            Some(entry) if launch.matches_json(entry) => McpClientState::Connected,
            Some(_) => McpClientState::NeedsUpdate,
        },
    }
}

/// JSON that keeps objects in file order (the app's `serde_json` sorts keys),
/// so an edit leaves the user's own keys where they were.
#[derive(Debug, Clone)]
enum Ordered {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Ordered>),
    Object(Vec<(String, Ordered)>),
}

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{MapAccess, SeqAccess, Visitor};
        struct Visit;
        impl<'de> Visitor<'de> for Visit {
            type Value = Ordered;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON")
            }
            fn visit_unit<E>(self) -> Result<Ordered, E> {
                Ok(Ordered::Null)
            }
            fn visit_bool<E>(self, v: bool) -> Result<Ordered, E> {
                Ok(Ordered::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Ordered, E> {
                Ok(Ordered::Number(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Ordered, E> {
                Ok(Ordered::Number(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Ordered, E> {
                serde_json::Number::from_f64(v)
                    .map(Ordered::Number)
                    .ok_or_else(|| E::custom("not a JSON number"))
            }
            fn visit_str<E>(self, v: &str) -> Result<Ordered, E> {
                Ok(Ordered::String(v.to_string()))
            }
            fn visit_string<E>(self, v: String) -> Result<Ordered, E> {
                Ok(Ordered::String(v))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Ordered, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Ordered::Array(items))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Ordered::Object(entries))
            }
        }
        deserializer.deserialize_any(Visit)
    }
}

impl Serialize for Ordered {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(v) => serializer.serialize_bool(*v),
            Self::Number(v) => v.serialize(serializer),
            Self::String(v) => serializer.serialize_str(v),
            Self::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Self::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

impl Ordered {
    /// The file's top-level object (absent or empty file: an empty one),
    /// refused on the same terms as [`read_json`].
    fn read(path: &Path) -> Result<Vec<(String, Ordered)>, String> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
        };
        let text = text.trim_start_matches('\u{feff}');
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        match serde_json::from_str(text) {
            Ok(Ordered::Object(entries)) => Ok(entries),
            _ => Err(not_plain_json(path)),
        }
    }

    /// Our entry, `command` first as every MCP doc writes it.
    fn launch(launch: &Launch) -> Self {
        Self::Object(vec![
            ("command".into(), Self::String(launch.command.clone())),
            (
                "args".into(),
                Self::Array(launch.args.iter().cloned().map(Self::String).collect()),
            ),
        ])
    }
}

fn write_ordered(path: &Path, root: Vec<(String, Ordered)>) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(&Ordered::Object(root)).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

/// `root`'s `mcpServers` entries, created empty when absent.
fn servers_mut<'a>(
    root: &'a mut Vec<(String, Ordered)>,
    path: &Path,
) -> Result<&'a mut Vec<(String, Ordered)>, String> {
    let at = match root.iter().position(|(k, _)| k == "mcpServers") {
        Some(at) => at,
        None => {
            root.push(("mcpServers".into(), Ordered::Object(Vec::new())));
            root.len() - 1
        }
    };
    match &mut root[at].1 {
        Ordered::Object(servers) => Ok(servers),
        _ => Err(format!(
            "\"mcpServers\" in {} is not an object; Grain left it unchanged.",
            path.display()
        )),
    }
}

fn json_connect(path: &Path, launch: &Launch) -> Result<(), String> {
    let mut root = Ordered::read(path)?;
    let servers = servers_mut(&mut root, path)?;
    let ours = Ordered::launch(launch);
    match servers.iter_mut().find(|(k, _)| k == SERVER_NAME) {
        Some((_, slot)) => *slot = ours,
        None => servers.push((SERVER_NAME.into(), ours)),
    }
    write_ordered(path, root)
}

/// Remove our entry; `false` when there was none.
fn json_disconnect(path: &Path) -> Result<bool, String> {
    if !path.is_file() {
        return Ok(false);
    }
    let mut root = Ordered::read(path)?;
    if !root.iter().any(|(k, _)| k == "mcpServers") {
        return Ok(false);
    }
    let servers = servers_mut(&mut root, path)?;
    let before = servers.len();
    servers.retain(|(k, _)| k != SERVER_NAME);
    if servers.len() == before {
        return Ok(false);
    }
    write_ordered(path, root)?;
    Ok(true)
}

// --- Codex (TOML) ----------------------------------------------------------------

fn read_toml(path: &Path) -> Result<toml_edit::DocumentMut, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
    };
    text.parse().map_err(|_| {
        format!(
            "{} is not valid TOML, so Grain left it unchanged. Add the setup by hand.",
            path.display()
        )
    })
}

fn toml_state(path: &Path, launch: &Launch) -> McpClientState {
    let Ok(doc) = read_toml(path) else {
        return McpClientState::Unreadable;
    };
    let Some(entry) = doc.get("mcp_servers").and_then(|s| s.get(SERVER_NAME)) else {
        return McpClientState::NotConnected;
    };
    let command = entry.get("command").and_then(|c| c.as_str());
    let args: Option<Vec<&str>> = entry
        .get("args")
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect());
    let same = command == Some(launch.command.as_str())
        && args == Some(launch.args.iter().map(String::as_str).collect());
    if same {
        McpClientState::Connected
    } else {
        McpClientState::NeedsUpdate
    }
}

fn toml_connect(path: &Path, launch: &Launch) -> Result<(), String> {
    use toml_edit::{value, Array, Item, Table};
    let mut doc = read_toml(path)?;
    if !doc.contains_key("mcp_servers") {
        let mut servers = Table::new();
        servers.set_implicit(true);
        doc.insert("mcp_servers", Item::Table(servers));
    }
    let Some(servers) = doc["mcp_servers"].as_table_mut() else {
        return Err(format!(
            "mcp_servers in {} is not a table; Grain left it unchanged.",
            path.display()
        ));
    };
    let mut entry = Table::new();
    entry.insert("command", value(launch.command.as_str()));
    let mut args = Array::new();
    for arg in &launch.args {
        args.push(arg.as_str());
    }
    entry.insert("args", value(args));
    servers.insert(SERVER_NAME, Item::Table(entry));
    atomic_write(path, doc.to_string().as_bytes())
}

fn toml_disconnect(path: &Path) -> Result<bool, String> {
    if !path.is_file() {
        return Ok(false);
    }
    let mut doc = read_toml(path)?;
    let removed = doc
        .get_mut("mcp_servers")
        .and_then(|s| s.as_table_mut())
        .is_some_and(|servers| servers.remove(SERVER_NAME).is_some());
    if removed {
        atomic_write(path, doc.to_string().as_bytes())?;
    }
    Ok(removed)
}

// --- Claude Code (its own CLI) -----------------------------------------------------

fn claude_code_state(places: &Places, launch: &Launch) -> McpClientState {
    let file = places.claude_code_state_file();
    if places.claude_cli().is_none() && !file.is_file() {
        return McpClientState::NotInstalled;
    }
    // Claude Code rewrites this file constantly; Grain only reads it.
    match read_json(&file) {
        Err(_) => McpClientState::NotConnected,
        Ok(root) => match root.get("mcpServers").and_then(|s| s.get(SERVER_NAME)) {
            None => McpClientState::NotConnected,
            Some(entry) if launch.matches_json(entry) => McpClientState::Connected,
            Some(_) => McpClientState::NeedsUpdate,
        },
    }
}

fn claude_code_command(launch: &Launch) -> String {
    format!(
        "claude mcp add --scope user {SERVER_NAME} -- {}",
        launch.shell_words()
    )
}

fn run_claude(cli: &Path, args: &[&str]) -> Result<(), String> {
    let mut command = if cli
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd"))
    {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(cli);
        c
    } else {
        std::process::Command::new(cli)
    };
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|e| format!("Could not run Claude Code: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Claude Code said: {}", stderr.trim()))
    }
}

fn claude_code_connect(places: &Places, launch: &Launch) -> Result<(), String> {
    let Some(cli) = places.claude_cli() else {
        return Err(format!(
            "Claude Code's `claude` command was not found. Run this in a terminal instead:\n{}",
            claude_code_command(launch)
        ));
    };
    // Replace any older entry; a missing one is not an error.
    let _ = run_claude(&cli, &["mcp", "remove", "--scope", "user", SERVER_NAME]);
    let mut args = vec!["mcp", "add", "--scope", "user", SERVER_NAME, "--"];
    args.push(&launch.command);
    args.extend(launch.args.iter().map(String::as_str));
    run_claude(&cli, &args)
}

fn claude_code_disconnect(places: &Places) -> Result<bool, String> {
    let Some(cli) = places.claude_cli() else {
        return Err(format!(
            "Claude Code's `claude` command was not found. Run this in a terminal: claude mcp remove --scope user {SERVER_NAME}"
        ));
    };
    run_claude(&cli, &["mcp", "remove", "--scope", "user", SERVER_NAME]).map(|()| true)
}

// --- Per-client dispatch ---------------------------------------------------------

fn client_status(places: &Places, client: McpClient, launch: &Launch) -> McpClientStatus {
    let state = match client {
        McpClient::ClaudeCode => claude_code_state(places, launch),
        McpClient::Codex => match places.codex_config() {
            (file, dir) if dir.is_dir() => toml_state(&file, launch),
            _ => McpClientState::NotInstalled,
        },
        _ => match places.json_config(client).expect("JSON-config client") {
            (file, dir) if dir.is_dir() => json_state(&file, launch),
            _ => McpClientState::NotInstalled,
        },
    };
    McpClientStatus {
        client,
        name: client.name().into(),
        state,
    }
}

fn connect(places: &Places, client: McpClient, launch: &Launch) -> Result<(), String> {
    match client {
        McpClient::ClaudeCode => claude_code_connect(places, launch),
        McpClient::Codex => toml_connect(&places.codex_config().0, launch),
        _ => json_connect(
            &places.json_config(client).expect("JSON-config client").0,
            launch,
        ),
    }
}

fn disconnect(places: &Places, client: McpClient) -> Result<bool, String> {
    match client {
        McpClient::ClaudeCode => claude_code_disconnect(places),
        McpClient::Codex => toml_disconnect(&places.codex_config().0),
        _ => json_disconnect(&places.json_config(client).expect("JSON-config client").0),
    }
}

fn wired(state: McpClientState) -> bool {
    matches!(
        state,
        McpClientState::Connected | McpClientState::NeedsUpdate
    )
}

// --- App context ---------------------------------------------------------------------

struct Context {
    places: Places,
    data_dir: PathBuf,
    server: Option<PathBuf>,
}

impl Context {
    fn new(app: &AppHandle) -> Result<Self, String> {
        Ok(Self {
            places: Places::detect(app)?,
            data_dir: crate::portable::app_data_dir(app).map_err(|e| e.to_string())?,
            server: server_path(),
        })
    }

    fn gate(&self) -> PathBuf {
        self.data_dir.join("meetings").join(grain_recall::GATE_FILE)
    }

    /// The launch line written into configs. Without a bundled server the
    /// expected install path is still shown, so the setup text is useful.
    fn launch(&self) -> Launch {
        let server = self
            .server
            .clone()
            .unwrap_or_else(|| PathBuf::from(SERVER_EXE));
        Launch::new(&server, &self.data_dir)
    }

    fn status(&self) -> RecallMcpStatus {
        let launch = self.launch();
        let setup = Ordered::Object(vec![(
            "mcpServers".into(),
            Ordered::Object(vec![(SERVER_NAME.into(), Ordered::launch(&launch))]),
        )]);
        RecallMcpStatus {
            enabled: self.gate().is_file(),
            server_path: self
                .server
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            clients: CLIENTS
                .iter()
                .map(|&c| client_status(&self.places, c, &launch))
                .collect(),
            setup_json: serde_json::to_string_pretty(&setup).unwrap_or_default(),
        }
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let gate = self.gate();
        if enabled {
            atomic_write(&gate, b"")?;
            info!("Recall MCP access turned on");
            return Ok(());
        }
        match fs::remove_file(&gate) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Could not turn off AI app access: {e}")),
        }
        info!("Recall MCP access turned off");
        // Off means no AI app starts the server any more.
        let launch = self.launch();
        let mut failed = Vec::new();
        for client in CLIENTS {
            if wired(client_status(&self.places, client, &launch).state) {
                if let Err(error) = disconnect(&self.places, client) {
                    warn!("Could not disconnect {}: {error}", client.name());
                    failed.push(format!("{}: {error}", client.name()));
                }
            }
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Access is off, but Grain could not remove itself from:\n{}",
                failed.join("\n")
            ))
        }
    }
}

/// The bundled server: `GRAIN_RECALL_MCP` when set (development), otherwise
/// next to Grain's own executable, where the installer puts it.
fn server_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GRAIN_RECALL_MCP") {
        return Some(PathBuf::from(path)).filter(|p| p.is_file());
    }
    let next_to_app = std::env::current_exe().ok()?.parent()?.join(SERVER_EXE);
    next_to_app.is_file().then_some(next_to_app)
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&Context) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&Context::new(&app)?))
        .await
        .map_err(|e| e.to_string())?
}

// --- Commands --------------------------------------------------------------------------

/// The switch, the bundled server and each AI app's connection.
#[tauri::command]
#[specta::specta]
pub async fn recall_mcp_status(app: AppHandle) -> Result<RecallMcpStatus, String> {
    blocking(app, |ctx| Ok(ctx.status())).await
}

/// Turn AI app access on or off. Off also removes Grain from every AI app it
/// was added to. On a partial failure the error names what is left to remove.
/// Like Connect and Disconnect, only the main window may call it: these edit
/// other apps' config files.
#[tauri::command]
#[specta::specta]
pub async fn recall_mcp_set_enabled(
    app: AppHandle,
    window: WebviewWindow,
    enabled: bool,
) -> Result<RecallMcpStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    blocking(app, move |ctx| {
        ctx.set_enabled(enabled)?;
        Ok(ctx.status())
    })
    .await
}

/// Add Grain Recall to one AI app's MCP config. The app must be restarted
/// (or reload its MCP servers) to see it.
#[tauri::command]
#[specta::specta]
pub async fn recall_mcp_connect(
    app: AppHandle,
    window: WebviewWindow,
    client: McpClient,
) -> Result<RecallMcpStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    blocking(app, move |ctx| {
        if !ctx.gate().is_file() {
            return Err("Turn on \"Use your meetings in AI apps\" first.".into());
        }
        if ctx.server.is_none() {
            return Err("The Recall MCP server is missing from this Grain install.".into());
        }
        connect(&ctx.places, client, &ctx.launch())?;
        info!("Recall MCP connected to {}", client.name());
        Ok(ctx.status())
    })
    .await
}

/// Remove Grain Recall from one AI app's MCP config.
#[tauri::command]
#[specta::specta]
pub async fn recall_mcp_disconnect(
    app: AppHandle,
    window: WebviewWindow,
    client: McpClient,
) -> Result<RecallMcpStatus, String> {
    crate::grain_commands::require_main_window(&window)?;
    blocking(app, move |ctx| {
        disconnect(&ctx.places, client)?;
        info!("Recall MCP disconnected from {}", client.name());
        Ok(ctx.status())
    })
    .await
}

#[cfg(test)]
#[path = "grain_recall_mcp_tests.rs"]
mod tests;
