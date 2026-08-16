//! [GRAIN] The link to the running Grain app.
//!
//! Grain mints a token for MCP when the bridge is switched on in the Grain Space
//! tab and writes it beside its other data; this reads that file and
//! authenticates on the local event port. No token file means the bridge is off
//! — which is a different message to the user than "Grain is not running", and
//! both are different from a real failure, so all three are distinguished here
//! rather than collapsed into one unhelpful error.
//!
//! The port already carries a capability-checked request frame (it is how
//! extension workers make host calls), so the bridge is a new IDENTITY on an
//! existing channel rather than a channel of its own.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

/// Fallback when the token file predates the port being written into it.
const EVENTS_PORT: u16 = 7124;

/// Written by the app when the MCP bridge is on; removed when it is not.
const TOKEN_FILE: &str = "mcp-token.json";

#[derive(Debug, Deserialize)]
struct TokenFile {
    token: String,
    #[serde(default)]
    port: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct AppLink {
    /// Where the token file should be. Resolved once at startup — the app may
    /// create it later, so its ABSENCE is checked per call, not cached.
    token_path: Option<PathBuf>,
}

/// Correlates requests with responses. One connection per call keeps this
/// simple and costs nothing at this rate; a long-lived socket would have to
/// handle the app restarting underneath it.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl AppLink {
    /// Resolve where Grain keeps its data. Never fails: an unresolvable data
    /// directory is reported when a tool is actually called, so the MCP
    /// handshake always completes and the client stays connected.
    pub fn discover() -> Self {
        Self {
            token_path: grain_data_dir().map(|d| d.join(TOKEN_FILE)),
        }
    }

    /// Point the link at a specific token file. The discovery path resolves one
    /// per platform; a test needs to name its own.
    pub fn for_token_file(path: PathBuf) -> Self {
        Self {
            token_path: Some(path),
        }
    }

    /// The current token and port, or the reason there isn't one.
    fn credentials(&self) -> Result<(String, u16)> {
        let Some(path) = &self.token_path else {
            bail!("Couldn't find Grain's data folder on this machine.");
        };
        if !path.exists() {
            bail!(
                "Grain Space isn't sharing a notebook right now. Open Grain, go to \
                 Extensions → Grain Space, and switch the MCP bridge on."
            );
        }
        let raw = std::fs::read_to_string(path).context("reading Grain's MCP token")?;
        let parsed: TokenFile =
            serde_json::from_str(&raw).context("Grain's MCP token file is unreadable")?;
        Ok((parsed.token, parsed.port.unwrap_or(EVENTS_PORT)))
    }

    /// One capability-checked call into the app.
    ///
    /// The token is re-read every time rather than cached, so switching the
    /// bridge off in Grain takes effect on the very next call instead of
    /// whenever this process happens to restart.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let (token, port) = self.credentials()?;
        let url = format!("ws://127.0.0.1:{port}");
        let (mut socket, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|_| anyhow!("Grain isn't running. Start Grain and try again."))?;

        // Hello first — the server maps the token to an identity before it will
        // accept anything else.
        let hello = json!({
            "token": token,
            "client": "grain-mcp",
            "grain_api": "1.0",
        });
        socket.send(Message::Text(hello.to_string())).await?;

        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let req = json!({ "req": { "id": id, "method": method, "params": params } });
        socket.send(Message::Text(req.to_string())).await?;

        // The socket also carries events and the welcome frame; read past
        // anything that is not our answer.
        while let Some(frame) = socket.next().await {
            let Message::Text(txt) = frame? else { continue };
            let Ok(value) = serde_json::from_str::<Value>(&txt) else {
                continue;
            };
            let Some(res) = value.get("res") else {
                continue;
            };
            if res.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(err) = res.get("err") {
                let message = err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Grain refused the request");
                bail!("{message}");
            }
            let _ = socket.close(None).await;
            return Ok(res.get("ok").cloned().unwrap_or(Value::Null));
        }
        bail!("Grain closed the connection before answering.")
    }
}

/// Grain's per-user data directory, matching what the app itself uses.
fn grain_data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("com.grain.app"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join("Library/Application Support")
                .join("com.grain.app")
        })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .map(|d| d.join("com.grain.app"))
    }
}
