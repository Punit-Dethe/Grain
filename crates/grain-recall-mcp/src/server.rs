//! MCP glue: tool and resource declarations over [`crate::tools`].

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
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::tools::{self, ToolResult};
use crate::Recall;

const INSTRUCTIONS: &str = "Grain Recall holds the user's recorded meetings, \
transcribed on their own computer. Find meetings with search_meetings (by what \
was said) or list_meetings (by date or title), then read one with get_meeting \
(summary and facts) and get_transcript (what was said, paged). Never invent \
meeting ids: use ids these tools return. Speakers are anonymous (\"Speaker 1\", \
\"Speaker 2\"); never guess a real name for a speaker. Dates are in the user's \
local time zone. Quote times (mm:ss) when they help the user find a moment.";

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
        let args = Value::Object(arguments.unwrap_or_default());
        let result = match name {
            "list_meetings" => self.run(args, tools::list_meetings),
            "search_meetings" => self.run(args, tools::search_meetings),
            "get_meeting" => self.run(args, tools::get_meeting),
            "get_transcript" => self.run(args, tools::get_transcript),
            other => {
                return Err(McpError::invalid_params(
                    format!("unknown tool `{other}`"),
                    None,
                ))
            }
        };
        Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
        })
    }

    fn run<A: DeserializeOwned>(
        &self,
        args: Value,
        tool: fn(&Recall, A) -> ToolResult,
    ) -> ToolResult {
        if !self.recall.enabled() {
            return Err(tools::OFF.into());
        }
        let args: A =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {e}"))?;
        tool(&self.recall, args)
    }

    fn read(&self, uri: &str) -> Result<String, McpError> {
        if !self.recall.enabled() {
            return Err(McpError::invalid_request(tools::OFF, None));
        }
        let rest = uri.strip_prefix(RESOURCE_PREFIX).ok_or_else(|| {
            McpError::resource_not_found(format!("unknown resource `{uri}`"), None)
        })?;
        let read = match rest.strip_suffix(TRANSCRIPT_SUFFIX) {
            Some(id) => tools::transcript_text(&self.recall, id),
            None => tools::meeting_markdown(&self.recall, rest),
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
    let date = |what: &str| json!({ "type": "string", "description": format!("{what}: YYYY-MM-DD (whole day, user's time zone) or an RFC 3339 time.") });
    let meeting_id = json!({ "type": "string", "description": "Meeting id from list_meetings or search_meetings." });
    vec![
        Tool::new(
            "search_meetings",
            "Search what was said across meetings. Returns matching passages with the meeting, time (mm:ss), speaker and the lines around it. All words must appear close together; word starts match (\"decid\" finds \"decided\"). Also lists meetings whose title matches.",
            schema(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words to find." },
                    "from": date("Earliest meeting start"),
                    "to": date("Latest meeting start"),
                    "meeting_id": { "type": "string", "description": "Search only this meeting." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 30, "default": 10 }
                },
                "required": ["query"]
            })),
        )
        .annotate(read_only("Search meetings")),
        Tool::new(
            "list_meetings",
            "List recorded meetings, newest first, with id, title, start time, duration, transcript status and whether a summary exists. Filter by date range or title words; page with offset.",
            schema(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Only meetings whose title contains this." },
                    "from": date("Earliest meeting start"),
                    "to": date("Latest meeting start"),
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 20 },
                    "offset": { "type": "integer", "minimum": 0, "default": 0 }
                }
            })),
        )
        .annotate(read_only("List meetings")),
        Tool::new(
            "get_meeting",
            "One meeting's facts (start, end, duration, audio sources, speakers, transcript size) and its AI summary in Markdown when one was written.",
            schema(json!({
                "type": "object",
                "properties": {
                    "meeting_id": meeting_id,
                    "include_summary": { "type": "boolean", "default": true }
                },
                "required": ["meeting_id"]
            })),
        )
        .annotate(read_only("Get meeting")),
        Tool::new(
            "get_transcript",
            "Read a meeting's transcript as \"[mm:ss] Speaker N: text\" lines, a page at a time (up to 500 lines). Use from/to to read around a moment, and next_offset to continue.",
            schema(json!({
                "type": "object",
                "properties": {
                    "meeting_id": meeting_id,
                    "from": { "type": "string", "description": "Start at this meeting time (mm:ss or h:mm:ss)." },
                    "to": { "type": "string", "description": "Stop at this meeting time (mm:ss or h:mm:ss)." },
                    "offset": { "type": "integer", "minimum": 0, "default": 0 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 200 }
                },
                "required": ["meeting_id"]
            })),
        )
        .annotate(read_only("Get transcript")),
    ]
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
        .with_instructions(INSTRUCTIONS)
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
        let meetings = tools::recent(&self.recall, LISTED_RESOURCES)
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
