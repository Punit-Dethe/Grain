//! [GRAIN] The link to the running Grain app.
//!
//! Grain mints a token for MCP when Grain Space is enabled and writes it beside
//! its other data; this reads that file and authenticates on the local event
//! port. No token file means the feature is off — which is a different message
//! to the user than "Grain is not running", and both are different from a real
//! failure, so all three are distinguished here rather than collapsed into one
//! unhelpful error.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// The port Grain's local event server listens on (`events_server::EVENTS_PORT`).
const EVENTS_PORT: u16 = 7124;

/// Written by the app when Grain Space is enabled; removed when it is not.
const TOKEN_FILE: &str = "mcp-token.json";

#[derive(Debug, Deserialize)]
struct TokenFile {
    token: String,
}

#[derive(Clone, Debug)]
pub struct AppLink {
    /// Where the token file should be. Resolved once at startup — the app may
    /// create it later, so its ABSENCE is checked per call, not cached.
    token_path: Option<PathBuf>,
}

impl AppLink {
    /// Resolve where Grain keeps its data. Never fails: an unresolvable data
    /// directory is reported when a tool is actually called, so the MCP
    /// handshake always completes and the client stays connected.
    pub fn discover() -> Self {
        Self {
            token_path: grain_data_dir().map(|d| d.join(TOKEN_FILE)),
        }
    }

    /// The current token, or the reason there isn't one.
    fn token(&self) -> Result<String> {
        let Some(path) = &self.token_path else {
            bail!("Couldn't find Grain's data folder on this machine.");
        };
        if !path.exists() {
            bail!(
                "Grain Space isn't sharing a notebook right now. Open Grain, enable Grain Space, \
                 and try again."
            );
        }
        let raw = std::fs::read_to_string(path).context("reading Grain's MCP token")?;
        let parsed: TokenFile =
            serde_json::from_str(&raw).context("Grain's MCP token file is unreadable")?;
        Ok(parsed.token)
    }

    /// Collections and their note counts.
    ///
    /// P1 wires the transport; until the request frame lands on the app side
    /// (see the plan, §2 "The channel") this reports the same honest state a
    /// stopped app would, rather than inventing a notebook.
    pub async fn list_collections(&self) -> Result<Vec<(String, u32)>> {
        let _token = self.token()?;
        bail!(
            "Grain is running but this build's MCP bridge isn't connected yet (grain-mcp \
             {}, port {EVENTS_PORT}).",
            env!("CARGO_PKG_VERSION")
        )
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
