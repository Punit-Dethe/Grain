use super::*;
use crate::fixture::Fixture;

fn args(value: Value) -> Option<JsonObject> {
    value.as_object().cloned()
}

#[test]
fn tools_are_read_only_with_object_schemas() {
    let tools = tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(
        names,
        [
            "search_meetings",
            "list_meetings",
            "get_meeting",
            "get_transcript"
        ]
    );
    for tool in &tools {
        assert_eq!(tool.input_schema["type"], "object");
        let a = tool.annotations.as_ref().unwrap();
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
    }
}

#[test]
fn switch_off_refuses_every_tool_and_resource() {
    let f = Fixture::new();
    let id = f.meeting("Sync", 1, 10, Some(&[(0, 0, "hello")]));
    let server = RecallServer::new(f.recall());
    let ok = server.call("list_meetings", None).unwrap();
    assert_ne!(ok.is_error, Some(true));
    assert_eq!(ok.structured_content.unwrap()["total"], 1);

    f.set_enabled(false);
    for (name, a) in [
        ("list_meetings", json!({})),
        ("search_meetings", json!({ "query": "hello" })),
        ("get_meeting", json!({ "meeting_id": id })),
        ("get_transcript", json!({ "meeting_id": id })),
    ] {
        let off = server.call(name, args(a)).unwrap();
        assert_eq!(off.is_error, Some(true), "{name}");
    }
    assert!(server.read(&format!("{RESOURCE_PREFIX}{id}")).is_err());
}

#[test]
fn bad_arguments_are_tool_errors_and_unknown_tools_protocol_errors() {
    let f = Fixture::new();
    let server = RecallServer::new(f.recall());
    let bad = server
        .call("get_meeting", args(json!({ "meeting_id": 5 })))
        .unwrap();
    assert_eq!(bad.is_error, Some(true));
    assert!(server.call("delete_everything", None).is_err());
}

#[test]
fn resources_resolve_by_uri() {
    let f = Fixture::new();
    let id = f.meeting("Sync", 1, 10, Some(&[(0, 0, "hello there")]));
    let server = RecallServer::new(f.recall());
    assert!(server
        .read(&format!("{RESOURCE_PREFIX}{id}"))
        .unwrap()
        .starts_with("# Sync"));
    assert_eq!(
        server
            .read(&format!("{RESOURCE_PREFIX}{id}{TRANSCRIPT_SUFFIX}"))
            .unwrap(),
        "[00:00] Speaker 1: hello there"
    );
    assert!(server.read("file:///etc/passwd").is_err());
    assert!(server.read(&format!("{RESOURCE_PREFIX}../x")).is_err());
}
