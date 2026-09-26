//! Local HTTP fault fixtures exercising the real rmcp transport, production
//! catalog/dispatch/result path and action-outcome projection. No app, vault,
//! external accounts, alternate UI or mock SDK is involved.

use super::*;
use grain_core::execution::{ActionOutcome, RiskClass, SideEffect};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Clone)]
enum Behavior {
    Complete(Value),
    DropAfterWrite,
    HangAfterWrite,
    HangSseAfterWrite,
    ProtocolError,
    RepeatedCursor,
    EmptyPages,
}

struct Fixture {
    endpoint: String,
    calls: Arc<AtomicUsize>,
    lists: Arc<AtomicUsize>,
    unexpected: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn start(behavior: Behavior) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let lists = Arc::new(AtomicUsize::new(0));
        let unexpected = Arc::new(AtomicUsize::new(0));
        let counters = (calls.clone(), lists.clone(), unexpected.clone());
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let request =
                    tokio::time::timeout(Duration::from_secs(3), read_request(&mut stream))
                        .await
                        .unwrap();
                let response = match request["method"].as_str() {
                    Some("server/discover") => json!({
                        "resultType": "complete", "supportedVersions": ["2026-07-28"],
                        "capabilities": { "tools": {} }, "ttlMs": 0, "cacheScope": "private"
                    }),
                    Some("tools/list") => {
                        let page = counters.1.fetch_add(1, Ordering::SeqCst);
                        match &behavior {
                            Behavior::RepeatedCursor => json!({
                                "resultType": "complete", "tools": [], "nextCursor": "same",
                                "ttlMs": 0, "cacheScope": "private"
                            }),
                            Behavior::EmptyPages => json!({
                                "resultType": "complete", "tools": [], "nextCursor": format!("page-{page}"),
                                "ttlMs": 0, "cacheScope": "private"
                            }),
                            _ => {
                                json!({ "resultType": "complete", "tools": [tool_json()], "ttlMs": 0, "cacheScope": "private" })
                            }
                        }
                    }
                    Some("tools/call") => {
                        // This counter is the simulated side effect, recorded
                        // before dropping or delaying the response.
                        counters.0.fetch_add(1, Ordering::SeqCst);
                        match &behavior {
                            Behavior::DropAfterWrite => {
                                drop(stream);
                                continue;
                            }
                            Behavior::HangAfterWrite => {
                                tokio::time::sleep(Duration::from_secs(30)).await;
                                continue;
                            }
                            Behavior::HangSseAfterWrite => {
                                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await.unwrap();
                                stream.flush().await.unwrap();
                                tokio::time::sleep(Duration::from_secs(30)).await;
                                continue;
                            }
                            Behavior::ProtocolError => json!({ "fixtureError": {
                                "code": -32603, "message": "token=secret-provider-payload"
                            } }),
                            Behavior::Complete(result) => result.clone(),
                            _ => panic!("pagination failures must never dispatch a tool"),
                        }
                    }
                    _ => {
                        counters.2.fetch_add(1, Ordering::SeqCst);
                        panic!("unexpected MCP request: {request}");
                    }
                };
                let body = if let Some(error) = response.get("fixtureError") {
                    json!({ "jsonrpc": "2.0", "id": request["id"], "error": error })
                } else {
                    json!({ "jsonrpc": "2.0", "id": request["id"], "result": response })
                }
                .to_string();
                let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                if stream.write_all(header.as_bytes()).await.is_ok() {
                    let _ = stream.write_all(body.as_bytes()).await;
                    let _ = stream.shutdown().await;
                }
            }
        });
        Self {
            endpoint,
            calls,
            lists,
            unexpected,
            task,
        }
    }

    async fn service(&self, http: reqwest_mcp::Client) -> McpService {
        serve_http(
            http,
            &self.endpoint,
            tokio::time::Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap()
    }

    async fn stop(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

// A deliberately tiny HTTP fixture reader, with hard bounds. It accepts only
// the Content-Length requests sent by the real reqwest client in these tests.
async fn read_request(stream: &mut TcpStream) -> Value {
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut chunk = [0u8; 1024];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0 && bytes.len() + count <= 128 * 1024);
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
    assert!(
        headers.starts_with("POST "),
        "no idle GET/session request is expected: {headers}"
    );
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap();
    assert!(length <= 64 * 1024);
    while bytes.len() < header_end + length {
        let mut chunk = [0u8; 1024];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
    }
    serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap()
}

fn tool_json() -> Value {
    json!({ "name": "write", "description": "Record one test operation.", "inputSchema": { "type": "object" } })
}

fn digest() -> String {
    tool_set_digest(
        provider("linear").unwrap(),
        &[serde_json::from_value(tool_json()).unwrap()],
    )
    .unwrap()
}

fn prepared(side_effect: SideEffect) -> grain_core::execution::PreparedCall {
    crate::action_exec::prepare(
        "mcp.linear:write",
        "mcp.linear",
        "write",
        "Linear",
        json!({}),
        RiskClass::Confirm,
        side_effect,
        &digest(),
    )
}

async fn execute(
    service: &McpService,
    timeout: Duration,
) -> Result<McpCallOutput, ExecutionFailure> {
    call_on_service(
        service,
        provider("linear").unwrap(),
        "write",
        serde_json::Map::new(),
        &digest(),
        tokio::time::Instant::now() + timeout,
        || true,
    )
    .await
}

#[tokio::test]
async fn a_recorded_write_with_a_lost_response_is_unknown_and_never_replayed() {
    let fixture = Fixture::start(Behavior::DropAfterWrite).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let result = execute(&service, Duration::from_secs(2)).await;
    let outcome = crate::action_exec::mcp_outcome(result, &prepared(SideEffect::Write));
    assert!(matches!(outcome, ActionOutcome::UnknownOutcome { .. }));
    assert!(!outcome.model_summary().contains("did not run"));
    assert!(outcome.model_summary().contains("Do not repeat"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn a_write_response_deadline_is_unknown_and_cleanup_is_bounded() {
    let fixture = Fixture::start(Behavior::HangAfterWrite).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let failure = execute(&service, Duration::from_millis(250))
        .await
        .unwrap_err();
    assert_eq!(failure.phase, DispatchPhase::Dispatched);
    assert!(failure.message.contains("deadline"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn repeated_and_unbounded_empty_pages_stop_before_dispatch() {
    for behavior in [Behavior::RepeatedCursor, Behavior::EmptyPages] {
        let fixture = Fixture::start(behavior).await;
        let service = fixture.service(McpHttpClient::build().unwrap().0).await;
        let failure = execute(&service, Duration::from_secs(3)).await.unwrap_err();
        assert_eq!(failure.phase, DispatchPhase::NotDispatched);
        assert!(failure.message.contains("incomplete"));
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        assert!(fixture.lists.load(Ordering::SeqCst) <= MAX_DISCOVERY_PAGES);
        assert!(close_service(service).await);
        fixture.stop().await;
    }
}

#[tokio::test]
async fn a_hanging_sse_response_releases_the_service_on_deadline() {
    let fixture = Fixture::start(Behavior::HangSseAfterWrite).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let failure = execute(&service, Duration::from_millis(250))
        .await
        .unwrap_err();
    assert_eq!(failure.phase, DispatchPhase::Dispatched);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn a_protocol_error_is_received_and_its_payload_is_not_exposed() {
    let fixture = Fixture::start(Behavior::ProtocolError).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let failure = execute(&service, Duration::from_secs(3)).await.unwrap_err();
    assert_eq!(failure.phase, DispatchPhase::ResponseReceived);
    let outcome = crate::action_exec::mcp_outcome(Err(failure), &prepared(SideEffect::Write));
    assert!(matches!(outcome, ActionOutcome::ResultUnavailable { .. }));
    assert!(!outcome.model_summary().contains("secret-provider-payload"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn unsupported_in_call_interaction_does_not_claim_non_execution() {
    let fixture = Fixture::start(Behavior::Complete(json!({
        "resultType": "input_required", "inputRequests": {}, "requestState": "pending"
    })))
    .await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let failure = execute(&service, Duration::from_secs(3)).await.unwrap_err();
    assert_eq!(failure.phase, DispatchPhase::ResponseReceived);
    assert!(failure.message.contains("in-call interaction"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn schema_change_or_disable_blocks_the_call_after_discovery() {
    let fixture = Fixture::start(Behavior::Complete(json!({ "content": [] }))).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    for (expected, enabled) in [("old-digest".to_string(), true), (digest(), false)] {
        let failure = call_on_service(
            &service,
            provider("linear").unwrap(),
            "write",
            serde_json::Map::new(),
            &expected,
            tokio::time::Instant::now() + Duration::from_secs(3),
            || enabled,
        )
        .await
        .unwrap_err();
        assert_eq!(failure.phase, DispatchPhase::NotDispatched);
    }
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn structured_and_unsupported_results_do_not_erase_execution_status() {
    let fixture = Fixture::start(Behavior::Complete(json!({
        "resultType": "complete", "isError": false,
        "content": [{ "type": "text", "text": "if ready {\n\twrite(\"a  b\");\n}" },
            { "type": "image", "data": "AA==", "mimeType": "image/png" }],
        "structuredContent": { "value": "a  b", "false": false }
    })))
    .await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let outcome = crate::action_exec::mcp_outcome(
        execute(&service, Duration::from_secs(3)).await,
        &prepared(SideEffect::Write),
    );
    let ActionOutcome::Succeeded(data) = outcome else {
        panic!("rendering support must not imply non-execution");
    };
    assert!(data.receipt);
    assert_eq!(data.structured_content.unwrap()["value"], "a  b");
    assert!(data.body.unwrap().contains("\n\twrite(\"a  b\");\n"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn provider_tool_error_remains_distinct_from_non_dispatch() {
    let fixture = Fixture::start(Behavior::Complete(
        json!({ "resultType": "complete", "isError": true,
        "content": [{ "type": "text", "text": "Second step failed." }] }),
    ))
    .await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let outcome = crate::action_exec::mcp_outcome(
        execute(&service, Duration::from_secs(3)).await,
        &prepared(SideEffect::Write),
    );
    assert!(matches!(outcome, ActionOutcome::ToolReportedError { .. }));
    assert!(outcome.model_summary().contains("partial effects"));
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn one_hundred_discovery_services_join_their_owned_tasks_on_close() {
    let fixture = Fixture::start(Behavior::Complete(json!({ "content": [] }))).await;
    let http = McpHttpClient::build().unwrap().0;
    for _ in 0..100 {
        let service = fixture.service(http.clone()).await;
        assert_eq!(
            discover_on_service(
                &service,
                tokio::time::Instant::now() + Duration::from_secs(3)
            )
            .await
            .unwrap()
            .len(),
            1
        );
        assert!(close_service(service).await);
    }
    assert_eq!(fixture.lists.load(Ordering::SeqCst), 100);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.unexpected.load(Ordering::SeqCst), 0);
    fixture.stop().await;
}

#[test]
fn oversized_text_and_json_are_explicitly_omitted_without_invalid_json() {
    let result = rmcp::model::CallToolResult::success(vec![ContentBlock::text(
        "🙂".repeat(MAX_RESULT_BYTES),
    )]);
    let output = bounded_call_output(result).unwrap();
    assert!(output.truncated);
    assert!(output.text.len() <= MAX_RESULT_BYTES);
    assert!(output.text.contains("[Result truncated"));
    let result: rmcp::model::CallToolResult = serde_json::from_value(json!({
        "content": [], "structuredContent": { "large": "x".repeat(MAX_RESULT_BYTES) }
    }))
    .unwrap();
    let output = bounded_call_output(result).unwrap();
    assert!(output.truncated);
    assert!(output.structured_content.is_none());
    assert!(!output.text.contains("\"large\":"));
}

#[test]
fn structured_scalars_and_escaped_invisible_data_round_trip() {
    for value in [
        json!(false),
        json!([1, "a  b"]),
        json!({ "value": "a\u{202e}b" }),
    ] {
        let result: rmcp::model::CallToolResult =
            serde_json::from_value(json!({ "content": [], "structuredContent": value })).unwrap();
        let output = bounded_call_output(result).unwrap();
        assert_eq!(output.structured_content.as_ref(), Some(&value));
        let encoded = output.text.split_once('\n').unwrap().1;
        assert_eq!(serde_json::from_str::<Value>(encoded).unwrap(), value);
    }
}
