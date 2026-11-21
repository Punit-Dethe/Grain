//! MCP glue: tool and resource declarations over `grain_recall::spec`.

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
    Resource, ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::Value;

use crate::{Recall, OFF};
use grain_recall::query::{self, ToolResult};
use grain_recall::spec;

const RESOURCE_PREFIX: &str = "grain://recall/meetings/";
const TRANSCRIPT_SUFFIX: &str = "/transcript";
const LISTED_RESOURCES: usize = 50;

#[derive(Debug, Clone)]
pub struct RecallServer {
    recall: Recall,
}

impl RecallServer {
    pub fn new(recall: Recall) -> Self {
        Self { recall }
    }

    /// Run one tool by name. `Err` is a protocol error (unknown tool); a
    /// tool's own failure comes back as an error result the model can read.
    pub fn call(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
    ) -> Result<CallToolResult, McpError> {
        let known = spec::tools().iter().any(|t| t.name == name);
        if !known {
            return Err(McpError::invalid_params(
                format!("unknown tool `{name}`"),
                None,
            ));
        }
        let result: ToolResult = if self.recall.enabled() {
            let args = Value::Object(arguments.unwrap_or_default());
            spec::call(self.recall.meetings_root(), name, args)
                .unwrap_or_else(|| Err(format!("unknown tool `{name}`")))
        } else {
            Err(OFF.into())
        };
        Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
        })
    }

    fn read(&self, uri: &str) -> Result<String, McpError> {
        if !self.recall.enabled() {
            return Err(McpError::invalid_request(OFF, None));
        }
        let rest = uri.strip_prefix(RESOURCE_PREFIX).ok_or_else(|| {
            McpError::resource_not_found(format!("unknown resource `{uri}`"), None)
        })?;
        let read = match rest.strip_suffix(TRANSCRIPT_SUFFIX) {
            Some(id) => query::transcript_text(self.recall.meetings_root(), id),
            None => query::meeting_markdown(self.recall.meetings_root(), rest),
        };
        read.map_err(|message| McpError::resource_not_found(message, None))
    }
}

fn schema(value: Value) -> Arc<JsonObject> {
    match value {
        Value::Object(map) => Arc::new(map),
        _ => unreachable!("tool schemas are objects"),
    }
}

fn read_only(title: &str) -> ToolAnnotations {
    ToolAnnotations::with_title(title)
        .read_only(true)
        .destructive(false)
        .idempotent(true)
        .open_world(false)
}

/// The tool list, also used by tests and docs.
pub fn tools() -> Vec<Tool> {
    spec::tools()
        .into_iter()
        .map(|t| Tool::new(t.name, t.description, schema(t.schema)).annotate(read_only(t.title)))
        .collect()
}

impl ServerHandler for RecallServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new("grain-recall", env!("CARGO_PKG_VERSION"))
                .with_title("Grain Recall"),
        )
        .with_instructions(spec::INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.call(&request.name, request.arguments).map(Into::into)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        if !self.recall.enabled() {
            return Ok(ListResourcesResult::default());
        }
        let meetings = query::recent(self.recall.meetings_root(), LISTED_RESOURCES)
            .map_err(|message| McpError::internal_error(message, None))?;
        Ok(ListResourcesResult::with_all_items(
            meetings
                .into_iter()
                .map(|m| {
                    Resource::new(format!("{RESOURCE_PREFIX}{}", m.id), m.title)
                        .with_description("Meeting facts and AI summary")
                        .with_mime_type("text/markdown")
                })
                .collect(),
        ))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new(format!("{RESOURCE_PREFIX}{{id}}"), "meeting")
                .with_description("A meeting's facts and AI summary")
                .with_mime_type("text/markdown"),
            ResourceTemplate::new(
                format!("{RESOURCE_PREFIX}{{id}}{TRANSCRIPT_SUFFIX}"),
                "meeting-transcript",
            )
            .with_description("A meeting's full transcript")
            .with_mime_type("text/plain"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let text = self.read(&request.uri)?;
        let mime = if request.uri.ends_with(TRANSCRIPT_SUFFIX) {
            "text/plain"
        } else {
            "text/markdown"
        };
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, request.uri).with_mime_type(mime)
        ])
        .into())
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
