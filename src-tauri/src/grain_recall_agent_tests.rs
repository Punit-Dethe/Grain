use super::*;
use grain_recall::fixture::Fixture;
use serde_json::json;

fn body(outcome: ActionOutcome) -> String {
    match outcome {
        ActionOutcome::Succeeded(data) => {
            assert!(!data.receipt, "reads are not receipts");
            data.body.unwrap()
        }
        other => panic!("expected success, got {other:?}"),
    }
}

#[test]
fn catalog_is_four_read_only_tools_with_a_stable_digest() {
    let (digest, tools) = catalog();
    assert_eq!(digest, catalog_digest());
    let names: Vec<&str> = tools.iter().map(|t| t.action.action_id.as_str()).collect();
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
        assert_eq!(tool.action.extension_id, EXTENSION_ID);
        assert_eq!(tool.action.risk, grain_sdk::action::ActionRisk::Safe);
        assert_eq!(tool.parameters["type"], "object");
    }
    let entry = directory_entry();
    assert_eq!(entry.action_count, 4);
    assert!(entry.description.contains("Today is "));
}

#[test]
fn calls_return_compact_text_and_typed_failures() {
    let f = Fixture::new();
    let root = f.root();
    let id = f.meeting(
        "Weekly standup",
        1_794_312_000_000,
        600,
        Some(&[
            (0, 0, "Morning everyone."),
            (5_000, 1, "We decided to ship the beta on Friday."),
        ]),
    );

    let found = body(run(&root, "search_meetings", json!({ "query": "beta" })));
    assert!(found.starts_with(&format!("Weekly standup (2026-11-")));
    assert!(found.contains(&format!("id {id}")));
    assert!(found.contains("> [00:05] Speaker 2: We decided to ship the beta on Friday."));

    let page = body(run(&root, "get_transcript", json!({ "meeting_id": id })));
    assert!(page.ends_with("[00:05] Speaker 2: We decided to ship the beta on Friday."));

    assert!(matches!(
        run(&root, "get_meeting", json!({ "meeting_id": "nope" })),
        ActionOutcome::Failed {
            class: FailureClass::NotFound,
            ..
        }
    ));
    assert!(matches!(
        run(&root, "search_meetings", json!({})),
        ActionOutcome::Failed {
            class: FailureClass::InvalidArgument,
            ..
        }
    ));
    assert!(matches!(
        run(&root, "delete_meeting", json!({})),
        ActionOutcome::Failed {
            class: FailureClass::NotFound,
            ..
        }
    ));
}
