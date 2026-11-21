//! The four Recall tools as one definition (name, description, input schema)
//! plus a dispatcher. The MCP sidecar and Grain's Agent both list and run the
//! tools from here, so a description or schema change reaches both.

use std::path::Path;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::query::{self, ToolResult};
use crate::render;

/// How a model should use the tools; the MCP server's instructions.
pub const INSTRUCTIONS: &str = "Grain Recall holds the user's recorded meetings, \
transcribed on their own computer. Find meetings with search_meetings (by what \
was said) or list_meetings (by date or title), then read one with get_meeting \
(summary and facts) and get_transcript (what was said, paged). Never invent \
meeting ids: use ids these tools return. Speakers are anonymous (\"Speaker 1\", \
\"Speaker 2\"); never guess a real name for a speaker. Dates are in the user's \
local time zone. Quote times (mm:ss) when they help the user find a moment.";

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Retrieval words for the Agent's tool search.
    pub keywords: &'static [&'static str],
    pub schema: Value,
}

/// The tool list, most useful first. Every tool is read-only.
pub fn tools() -> Vec<ToolSpec> {
    let date = |what: &str| json!({ "type": "string", "description": format!("{what}: YYYY-MM-DD (whole day, user's time zone) or an RFC 3339 time.") });
    let meeting_id = json!({ "type": "string", "description": "Meeting id from list_meetings or search_meetings." });
    vec![
        ToolSpec {
            name: "search_meetings",
            title: "Search meetings",
            description: "Search what was said across meetings. Returns matching passages with the meeting, time (mm:ss), speaker and the lines around it. All words must appear close together; word starts match (\"decid\" finds \"decided\"). Also lists meetings whose title matches.",
            keywords: &["meeting", "call", "said", "decided", "discussed", "transcript"],
            schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words to find." },
                    "from": date("Earliest meeting start"),
                    "to": date("Latest meeting start"),
                    "meeting_id": { "type": "string", "description": "Search only this meeting." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 30, "default": 10 }
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "list_meetings",
            title: "List meetings",
            description: "List recorded meetings, newest first, with id, title, start time, duration, transcript status and whether a summary exists. Filter by date range or title words; page with offset.",
            keywords: &["meetings", "recordings", "calls", "yesterday", "today", "week"],
            schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Only meetings whose title contains this." },
                    "from": date("Earliest meeting start"),
                    "to": date("Latest meeting start"),
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 20 },
                    "offset": { "type": "integer", "minimum": 0, "default": 0 }
                }
            }),
        },
        ToolSpec {
            name: "get_meeting",
            title: "Get meeting",
            description: "One meeting's facts (start, end, duration, audio sources, speakers, transcript size) and its AI summary in Markdown when one was written.",
            keywords: &["meeting", "summary", "notes", "recap"],
            schema: json!({
                "type": "object",
                "properties": {
                    "meeting_id": meeting_id,
                    "include_summary": { "type": "boolean", "default": true }
                },
                "required": ["meeting_id"]
            }),
        },
        ToolSpec {
            name: "get_transcript",
            title: "Get transcript",
            description: "Read a meeting's transcript as \"[mm:ss] Speaker N: text\" lines, a page at a time (up to 500 lines). Use from/to to read around a moment, and next_offset to continue.",
            keywords: &["transcript", "meeting", "said", "words"],
            schema: json!({
                "type": "object",
                "properties": {
                    "meeting_id": meeting_id,
                    "from": { "type": "string", "description": "Start at this meeting time (mm:ss or h:mm:ss)." },
                    "to": { "type": "string", "description": "Stop at this meeting time (mm:ss or h:mm:ss)." },
                    "offset": { "type": "integer", "minimum": 0, "default": 0 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 200 }
                },
                "required": ["meeting_id"]
            }),
        },
    ]
}

fn run<A: DeserializeOwned>(
    root: &Path,
    args: Value,
    tool: fn(&Path, A) -> ToolResult,
) -> ToolResult {
    let args: A = serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {e}"))?;
    tool(root, args)
}

/// Run one tool by name over the meetings folder `root`. `None` for a name
/// that is not a Recall tool. `args` is the tool's JSON object (`null` = `{}`).
pub fn call(root: &Path, name: &str, args: Value) -> Option<ToolResult> {
    let args = if args.is_null() { json!({}) } else { args };
    Some(match name {
        "list_meetings" => run(root, args, query::list_meetings),
        "search_meetings" => run(root, args, query::search_meetings),
        "get_meeting" => run(root, args, query::get_meeting),
        "get_transcript" => run(root, args, query::get_transcript),
        _ => return None,
    })
}

/// The compact text form of a successful `call` result, for the Agent.
pub fn render(name: &str, result: &Value) -> String {
    match name {
        "list_meetings" => render::list(result),
        "search_meetings" => render::search(result),
        "get_meeting" => render::meeting(result),
        "get_transcript" => render::transcript(result),
        _ => result.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Fixture;

    #[test]
    fn every_tool_dispatches_and_unknown_names_do_not() {
        let f = Fixture::new();
        let root = f.root();
        let id = f.meeting("Sync", 1, 10, Some(&[(0, 0, "hello there")]));
        for tool in tools() {
            assert_eq!(tool.schema["type"], "object");
            let args = match tool.name {
                "search_meetings" => json!({ "query": "hello" }),
                "list_meetings" => Value::Null,
                _ => json!({ "meeting_id": id }),
            };
            let result = call(&root, tool.name, args).unwrap().unwrap();
            assert!(!render(tool.name, &result).is_empty());
        }
        assert!(call(&root, "delete_everything", Value::Null).is_none());
        let bad = call(&root, "get_meeting", json!({ "meeting_id": 5 })).unwrap();
        assert!(bad.unwrap_err().starts_with("Invalid arguments"));
    }
}
