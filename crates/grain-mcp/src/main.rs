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
use serde::{Deserialize, Serialize};

mod app;

/// What the app is reachable through, and what to say when it is not.
use app::AppLink;

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

/// One collection in the notebook, with how much is in it.
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Collection {
    name: String,
    notes: u32,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Collections {
    collections: Vec<Collection>,
}

#[tool_router]
impl GrainSpace {
    fn new(link: AppLink) -> Self {
        Self {
            link,
            tool_router: Self::tool_router(),
        }
    }

    /// P1's end-to-end proof: handshake, auth, routing, and the not-running
    /// path, on the cheapest possible call.
    #[tool(
        name = "list_collections",
        description = "List the collections (folders) in the user's Grain Space notebook, with the number of notes in each."
    )]
    async fn list_collections(
        &self,
        Parameters(_): Parameters<NoParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let names = match self.link.list_collections().await {
            Ok(names) => names,
            Err(e) => return Ok(unreachable(e)),
        };
        let collections: Vec<Collection> = names
            .into_iter()
            .map(|(name, notes)| Collection { name, notes })
            .collect();
        // Both forms (MCP 2025-06-18): prose for the model to read, and the
        // typed object so a client can chain another call on it without parsing
        // the prose back apart.
        let text = if collections.is_empty() {
            "The notebook has no collections yet.".to_string()
        } else {
            collections
                .iter()
                .map(|c| format!("{} ({} notes)", c.name, c.notes))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let structured = serde_json::to_value(Collections { collections })
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
        result.structured_content = Some(structured);
        Ok(result)
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
            "The user's Grain Space notebook: notes they dictated or saved, searchable by \
             full text, meaning and the entities they mention. Search it before answering \
             questions about the user's own work, decisions or past sessions."
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
