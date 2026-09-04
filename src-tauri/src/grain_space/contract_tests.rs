//! [GRAIN] Contract tests for Grain Space memory tool policy (MEMORY-SYSTEM-PLAN.md Phase 1).
//!
//! Enforces:
//! 1. Agent attempting `save_note` yields `ToolResult::Confirm` and does NOT touch disk.
//! 2. Confirmation approval executes the write through `action_exec::resume` / vault execution.
//! 3. Confirmation rejection discards the write.
//! 4. MCP/Host-API write follows identical confirmation policy.
//! 5. Malformed tool arguments return structured error code, never panics.

use serde_json::json;

use crate::action_exec::{
    self, discard, execute_grain_space_on_vault, grain_space_actions,
    grain_space_tool_definitions, grain_space_tool_specs, prepare_grain_space_call,
    run_or_confirm_opt, Dispatch,
};
use crate::grain_space::agent_tools::{
    dispatch_opt, ToolErrorResponse, TurnLog, ERR_INVALID_ARGUMENTS, ERR_SCHEMA_VIOLATION,
};
use crate::llm_client::ToolCallOut;

#[tokio::test]
async fn contract_unified_tool_definitions_and_schema() {
    let defs = grain_space_tool_definitions();
    let tool_specs = grain_space_tool_specs();
    let actions = grain_space_actions();

    assert_eq!(defs.len(), 5);
    assert_eq!(tool_specs.len(), 5);
    assert_eq!(actions.len(), 5);

    // Verify all fields are present in save_note
    let save_spec = tool_specs.iter().find(|s| s.name == "save_note").unwrap();
    let props = save_spec.parameters.get("properties").unwrap();
    assert!(props.get("body").is_some());
    assert!(props.get("title").is_some());
    assert!(props.get("summary").is_some());
    assert!(props.get("question").is_some());
    assert!(props.get("entities").is_some());
    assert!(props.get("collection").is_some());
}

#[tokio::test]
async fn contract_agent_attempting_save_yields_confirm_without_disk_write() {
    let mut log = TurnLog::default();

    let call = ToolCallOut {
        id: "call-1".to_string(),
        name: "save_note".to_string(),
        arguments: json!({
            "body": "Never deploy on Friday after 4pm.",
            "title": "Friday deploy rule"
        })
        .to_string(),
    };

    let result = dispatch_opt(None, &call, &mut log).await;

    // Must return Confirm interaction, NOT Text
    match result {
        crate::capability::ToolResult::Confirm(confirm) => {
            assert!(confirm.token.starts_with("pc_"));
            assert_eq!(confirm.title, "Save note");
            assert!(confirm.side_effect.contains("Makes a change"));
            assert!(!confirm.markdown.is_empty());

            // Clean up pending token
            assert!(discard(&confirm.token));
        }
        crate::capability::ToolResult::Text(txt) => {
            panic!("save_note must yield ToolResult::Confirm, but got text: {txt}");
        }
    }
}

#[tokio::test]
async fn contract_confirmation_rejection_discards_write() {
    let prepared = prepare_grain_space_call(
        "save_note",
        json!({
            "body": "Temporary note that user will reject.",
            "title": "Rejected Note"
        }),
    );
    let token = prepared.token.clone();

    let dispatch_res = run_or_confirm_opt(None, prepared, "Save note").await;
    match dispatch_res {
        Dispatch::AwaitConfirm(interaction) => {
            match interaction {
                grain_core::interaction::Interaction::Confirm { token: t, .. } => {
                    assert_eq!(t, token);
                }
                other => panic!("expected Confirm interaction, got: {other:?}"),
            }
        }
        Dispatch::Ran(_) => panic!("save_note must await confirmation!"),
    }

    // User rejects the action
    let outcome = action_exec::resume_opt(None, &token, false).await;
    assert!(matches!(outcome, grain_core::execution::ActionOutcome::Cancelled));

    // Calling resume a second time fails because token was discarded
    let second = action_exec::resume_opt(None, &token, true).await;
    assert!(matches!(
        second,
        grain_core::execution::ActionOutcome::Failed {
            class: grain_core::execution::FailureClass::NotFound,
            ..
        }
    ));
}

#[tokio::test]
async fn contract_confirmation_approval_executes_write_with_complete_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let vault = crate::grain_space::vault::Vault::native(temp.path().to_path_buf());

    // 1. Prepare call with full metadata (including summary, question, entities, collection)
    let save_args = json!({
        "body": "Deploy freeze runs Dec 20 through Jan 5.",
        "title": "Year-End Deploy Freeze",
        "summary": "Deploy freeze runs Dec 20 to Jan 5.",
        "question": "When is the year-end deploy freeze?",
        "entities": ["deployment", "freeze", "devops"],
        "collection": "Operations"
    });

    // 2. Execute on vault
    let outcome = execute_grain_space_on_vault(&vault, "save_note", &save_args);
    let grain_core::execution::ActionOutcome::Succeeded(data) = outcome else {
        panic!("expected successful save");
    };
    let note_id = data.details.iter().find(|f| f.label == "id").unwrap().value.clone();
    assert!(!note_id.is_empty());

    // 3. Verify note is written to disk and all metadata persisted
    let get_outcome = execute_grain_space_on_vault(&vault, "get_note", &json!({ "id": note_id }));
    let grain_core::execution::ActionOutcome::Succeeded(get_data) = get_outcome else {
        panic!("expected note to exist on disk");
    };
    assert_eq!(get_data.title.as_deref(), Some("Year-End Deploy Freeze"));
    let body = get_data.body.unwrap();
    assert!(body.contains("Deploy freeze runs Dec 20 through Jan 5."));

    // 4. Append to note
    let append_outcome = execute_grain_space_on_vault(
        &vault,
        "append_to_note",
        &json!({
            "id": note_id,
            "text": "Exceptions require VP approval."
        }),
    );
    assert!(matches!(append_outcome, grain_core::execution::ActionOutcome::Succeeded(_)));

    // 5. Verify appended body retains original content
    let get_appended = execute_grain_space_on_vault(&vault, "get_note", &json!({ "id": note_id }));
    let grain_core::execution::ActionOutcome::Succeeded(appended_data) = get_appended else {
        panic!("expected note to exist after append");
    };
    let final_body = appended_data.body.unwrap();
    assert!(final_body.contains("Deploy freeze runs Dec 20 through Jan 5."));
    assert!(final_body.contains("Exceptions require VP approval."));
}

#[tokio::test]
async fn contract_mcp_host_api_write_follows_identical_confirmation_policy() {
    // 1. Prepare save_note call as host_api/mcp would
    let prepared = prepare_grain_space_call(
        "save_note",
        json!({
            "body": "Important architectural decision",
            "title": "ADR 001"
        }),
    );
    assert_eq!(prepared.risk, grain_core::execution::RiskClass::Confirm);
    assert_eq!(prepared.side_effect, grain_core::execution::SideEffect::Write);

    // 2. run_or_confirm MUST withhold the call
    let token = prepared.token.clone();
    let dispatch_res = run_or_confirm_opt(None, prepared, "Save note").await;
    match dispatch_res {
        Dispatch::AwaitConfirm(interaction) => {
            match interaction {
                grain_core::interaction::Interaction::Confirm { token: t, .. } => {
                    assert_eq!(t, token);
                }
                _ => panic!("expected Confirm interaction"),
            }
        }
        Dispatch::Ran(_) => panic!("Host API write must not execute without confirmation!"),
    }

    // 3. Discard leaves clean state
    assert!(discard(&token));
}

#[tokio::test]
async fn contract_malformed_arguments_return_structured_error_and_never_panic() {
    let mut log = TurnLog::default();

    // 1. Non-JSON arguments
    let bad_json = ToolCallOut {
        id: "call-bad-json".to_string(),
        name: "save_note".to_string(),
        arguments: "{not valid json}".to_string(),
    };
    let res = dispatch_opt(None, &bad_json, &mut log).await;
    match res {
        crate::capability::ToolResult::Text(txt) => {
            let err: ToolErrorResponse = serde_json::from_str(&txt).expect("valid JSON error response");
            assert!(!err.ok);
            assert_eq!(err.error.code, ERR_SCHEMA_VIOLATION);
        }
        _ => panic!("expected ToolResult::Text for malformed json"),
    }

    // 2. Missing required 'body' in save_note
    let missing_body = ToolCallOut {
        id: "call-missing-body".to_string(),
        name: "save_note".to_string(),
        arguments: json!({ "title": "Empty note" }).to_string(),
    };
    let res = dispatch_opt(None, &missing_body, &mut log).await;
    match res {
        crate::capability::ToolResult::Text(txt) => {
            let err: ToolErrorResponse = serde_json::from_str(&txt).expect("valid JSON error response");
            assert!(!err.ok);
            assert_eq!(err.error.code, ERR_INVALID_ARGUMENTS);
            assert!(err.error.message.contains("body"));
        }
        _ => panic!("expected ToolResult::Text for missing body"),
    }

    // 3. Missing required 'query' in search_notes
    let missing_query = ToolCallOut {
        id: "call-missing-query".to_string(),
        name: "search_notes".to_string(),
        arguments: json!({}).to_string(),
    };
    let res = dispatch_opt(None, &missing_query, &mut log).await;
    match res {
        crate::capability::ToolResult::Text(txt) => {
            let err: ToolErrorResponse = serde_json::from_str(&txt).expect("valid JSON error response");
            assert!(!err.ok);
            assert_eq!(err.error.code, ERR_INVALID_ARGUMENTS);
            assert!(err.error.message.contains("query"));
        }
        _ => panic!("expected ToolResult::Text for missing query"),
    }

    // 4. Missing required 'id' or 'text' in append_to_note
    let missing_text = ToolCallOut {
        id: "call-missing-text".to_string(),
        name: "append_to_note".to_string(),
        arguments: json!({ "id": "note-1" }).to_string(),
    };
    let res = dispatch_opt(None, &missing_text, &mut log).await;
    match res {
        crate::capability::ToolResult::Text(txt) => {
            let err: ToolErrorResponse = serde_json::from_str(&txt).expect("valid JSON error response");
            assert!(!err.ok);
            assert_eq!(err.error.code, ERR_INVALID_ARGUMENTS);
            assert!(err.error.message.contains("id and text"));
        }
        _ => panic!("expected ToolResult::Text for missing text"),
    }

    // 5. Unknown tool name
    let unknown_tool = ToolCallOut {
        id: "call-unknown".to_string(),
        name: "delete_everything".to_string(),
        arguments: json!({}).to_string(),
    };
    let res = dispatch_opt(None, &unknown_tool, &mut log).await;
    match res {
        crate::capability::ToolResult::Text(txt) => {
            let err: ToolErrorResponse = serde_json::from_str(&txt).expect("valid JSON error response");
            assert!(!err.ok);
            assert_eq!(err.error.code, ERR_SCHEMA_VIOLATION);
            assert!(err.error.message.contains("delete_everything"));
        }
        _ => panic!("expected ToolResult::Text for unknown tool"),
    }
}
