//! [GRAIN] Grain Space over the Model Context Protocol (see
//! `docs/Grain Space 2.0/MCP-PLAN.md`).
//!
//! This is a **proxy**, not a second copy of Grain Space. The app owns the
//! vault, the SQLite index (FTS + vectors + entity graph) and the embedding
//! engine; this binary owns none of them. An MCP client spawns it over stdio, it
//! authenticates to the running app on the local event port, and it forwards
//! each tool call there.
//!
//! Why a spawned proxy rather than a server inside Grain:
//!
//! - **One writer.** Opening the index here would race the app's own writes.
//! - **One model in memory.** Semantic search needs the embedding engine; a
//!   second process loading it would cost hundreds of MB per connected client.
//! - **Nothing idle.** The client spawns this and it dies with the client, so a
//!   user who never connects one pays nothing — no port, no thread, no process.
//!
//! # stdout is the protocol
//!
//! stdout carries JSON-RPC frames and NOTHING else. A stray `println!` corrupts
//! the stream and the client drops the connection. All diagnostics go to stderr.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use serde::Deserialize;
use serde_json::{json, Value};

/// What the app is reachable through, and what to say when it is not.
use grain_mcp::AppLink;

#[derive(Clone)]
struct GrainSpace {
    link: AppLink,
    /// Read by the code `#[tool_handler]` generates, which the dead-code lint
    /// cannot see through — routing is proven by the protocol probe, not by this
    /// field appearing in hand-written code.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct NoParams {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchParams {
    /// What to look for. Plain language works — the search is not keyword-only.
    query: String,
    /// How many notes to return. Defaults to 8.
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct IdParams {
    /// The note's id, as returned by `search_notes`.
    id: String,
}

#[tool_router]
impl GrainSpace {
    fn new(link: AppLink) -> Self {
        Self {
            link,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "list_collections",
        description = "List the collections (folders) in the user's Grain Space notebook."
    )]
    async fn list_collections(
        &self,
        Parameters(_): Parameters<NoParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.relay("space.collections", json!({}), |v| {
            let names: Vec<&str> = v
                .get("collections")
                .and_then(|c| c.as_array())
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if names.is_empty() {
                "The notebook has no collections yet.".to_string()
            } else {
                names.join("\n")
            }
        })
        .await
    }

    #[tool(
        name = "search_notes",
        description = "Search the user's Grain Space notebook by meaning, wording and the things notes mention. Use this before answering anything about the user's own work, decisions or past sessions."
    )]
    async fn search_notes(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = json!({ "query": p.query, "limit": p.limit.unwrap_or(8) });
        self.relay("space.search", params, |v| {
            let empty = Vec::new();
            let results = v
                .get("results")
                .and_then(|r| r.as_array())
                .unwrap_or(&empty);
            if results.is_empty() {
                return "No notes matched.".to_string();
            }
            results
                .iter()
                .map(|n| {
                    let id = n.get("id").and_then(Value::as_str).unwrap_or("");
                    let title = n.get("title").and_then(Value::as_str).unwrap_or("Untitled");
                    let snippet = n.get("snippet").and_then(Value::as_str).unwrap_or("");
                    format!("[{id}] {title}\n    {snippet}")
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .await
    }

    #[tool(
        name = "get_note",
        description = "Read one bounded note excerpt from the user's Grain Space notebook, by id."
    )]
    async fn get_note(
        &self,
        Parameters(p): Parameters<IdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.relay("space.get", json!({ "id": p.id }), |v| {
            let title = v.get("title").and_then(Value::as_str).unwrap_or("Untitled");
            let body = v.get("body").and_then(Value::as_str).unwrap_or("");
            format!("# {title}\n\n{body}")
        })
        .await
    }

    /// One call into the app, rendered both ways: `describe` writes the prose a
    /// model reads, and the raw answer rides along as `structuredContent` so a
    /// client can chain on an id without parsing that prose back apart.
    async fn relay(
        &self,
        method: &str,
        params: Value,
        describe: impl Fn(&Value) -> String,
    ) -> Result<CallToolResult, ErrorData> {
        match self.link.call(method, params).await {
            Ok(value) => {
                let mut result =
                    CallToolResult::success(vec![ContentBlock::text(describe(&value))]);
                result.structured_content = Some(value);
                Ok(result)
            }
            Err(e) => Ok(unreachable(e)),
        }
    }
}

/// A failure to reach the app is a normal, expected state — Grain may simply not
/// be running yet — so it becomes a TOOL-level error, whose message the client
/// actually renders, rather than a protocol error, which clients show opaquely
/// as "internal error" and which hides the one sentence the user needs.
fn unreachable(e: anyhow::Error) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(e.to_string())])
}

#[tool_handler]
impl ServerHandler for GrainSpace {
    fn get_info(&self) -> ServerInfo {
        // `ServerInfo` is #[non_exhaustive], so it is built from its default and
        // amended: fields the protocol adds later then arrive with sane values
        // instead of breaking this build.
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::LATEST;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = Implementation::new("grain-space", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Read-only access to the user's Grain Space notebook: notes they dictated or saved, \
             searchable by full text, meaning and the entities they mention. Search it before \
             answering questions about the user's own work, decisions or past sessions. Note \
             mutations remain available through Grain's Agent, where the user can approve them."
                .into(),
        );
        info
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // stderr only — see the module note.
    eprintln!("grain-mcp {} starting", env!("CARGO_PKG_VERSION"));
    let service = GrainSpace::new(AppLink::discover())
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
