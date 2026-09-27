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
    CatalogPages(Vec<Vec<Value>>),
    Complete(Value),
    CompleteSse(Value),
    CompleteLegacy(Value),
    DropAfterWrite,
    HangAfterWrite,
    HangSseAfterWrite,
    ProtocolError,
    RepeatedCursor,
    EmptyPages,
    DuplicateToolsAcrossPages,
    Oversized { discovery: bool, framing: WireLimit },
    PaddedPages,
}

#[derive(Clone, Copy)]
enum WireLimit {
    Declared,
    Chunked,
    SseData,
    SseComments,
    HttpError,
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
        Self::start_with_tool(behavior, tool_json()).await
    }

    async fn start_with_tool(behavior: Behavior, tool: Value) -> Self {
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
                if let Behavior::Oversized { discovery, framing } = &behavior {
                    let method = if *discovery {
                        "tools/list"
                    } else {
                        "tools/call"
                    };
                    if request["method"] == method {
                        if *discovery {
                            counters.1.fetch_add(1, Ordering::SeqCst);
                        } else {
                            counters.0.fetch_add(1, Ordering::SeqCst);
                        }
                        send_oversized(&mut stream, *framing).await;
                        continue;
                    }
                }
                let response = match request["method"].as_str() {
                    Some("server/discover") if matches!(behavior, Behavior::CompleteLegacy(_)) => {
                        json!({
                            "fixtureError": {"code": -32601, "message": "modern discovery unavailable"}
                        })
                    }
                    Some("initialize") if matches!(behavior, Behavior::CompleteLegacy(_)) => {
                        json!({
                            "protocolVersion": "2025-11-25", "capabilities": {"tools": {}},
                            "serverInfo": {"name": "Legacy fixture", "version": "1"}
                        })
                    }
                    Some("notifications/initialized")
                        if matches!(behavior, Behavior::CompleteLegacy(_)) =>
                    {
                        let _ = stream.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                        continue;
                    }
                    Some("server/discover") => json!({
                        "resultType": "complete", "supportedVersions": ["2026-07-28"],
                        "capabilities": { "tools": {} }, "ttlMs": 0, "cacheScope": "private"
                    }),
                    Some("tools/list") => {
                        let page = counters.1.fetch_add(1, Ordering::SeqCst);
                        match &behavior {
                            Behavior::CatalogPages(pages) => {
                                let page = request["params"]["cursor"]
                                    .as_str()
                                    .and_then(|cursor| cursor.strip_prefix("page-"))
                                    .and_then(|page| page.parse::<usize>().ok())
                                    .unwrap_or(0);
                                let mut response = json!({"resultType": "complete",
                                    "tools": pages.get(page).cloned().unwrap_or_default(),
                                    "ttlMs": 0, "cacheScope": "private"});
                                if page + 1 < pages.len() {
                                    response["nextCursor"] = json!(format!("page-{}", page + 1));
                                }
                                response
                            }
                            Behavior::RepeatedCursor => json!({
                                "resultType": "complete", "tools": [], "nextCursor": "same",
                                "ttlMs": 0, "cacheScope": "private"
                            }),
                            Behavior::DuplicateToolsAcrossPages => {
                                let mut response = json!({"resultType": "complete", "tools": [tool.clone()],
                                    "ttlMs": 0, "cacheScope": "private"});
                                if page == 0 {
                                    response["nextCursor"] = json!("second-page");
                                }
                                response
                            }
                            Behavior::EmptyPages => json!({
                                "resultType": "complete", "tools": [], "nextCursor": format!("page-{page}"),
                                "ttlMs": 0, "cacheScope": "private"
                            }),
                            Behavior::PaddedPages => json!({
                                "resultType": "complete", "tools": [], "nextCursor": format!("page-{page}"),
                                "padding": "x".repeat(1024 * 1024), "ttlMs": 0, "cacheScope": "private"
                            }),
                            _ => {
                                json!({ "resultType": "complete", "tools": [tool.clone()], "ttlMs": 0, "cacheScope": "private" })
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
                            Behavior::Complete(result)
                            | Behavior::CompleteSse(result)
                            | Behavior::CompleteLegacy(result) => result.clone(),
                            Behavior::CatalogPages(_) => json!({"resultType": "complete",
                                "content": [{"type": "text", "text": "one supported call"}]}),
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
                let (kind, body) = if matches!(behavior, Behavior::CompleteSse(_)) {
                    (
                        "text/event-stream",
                        format!("event: message\ndata: {body}\n\n"),
                    )
                } else {
                    ("application/json", body)
                };
                let header = format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
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
            bounded_http::BoundedClient::new(http),
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

async fn send_oversized(stream: &mut TcpStream, framing: WireLimit) {
    let sse = matches!(framing, WireLimit::SseData | WireLimit::SseComments);
    let limit = if sse {
        MAX_SSE_EVENT_BYTES
    } else {
        bounded_http::MAX_JSON_BYTES
    };
    let (status, kind) = if matches!(framing, WireLimit::HttpError) {
        (500, "application/json")
    } else if sse {
        (200, "text/event-stream")
    } else {
        (200, "application/json")
    };
    let length = if matches!(framing, WireLimit::Declared) {
        format!("Content-Length: {}\r\n", limit + 1)
    } else {
        "Transfer-Encoding: chunked\r\n".into()
    };
    let header = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: {kind}\r\n{length}Connection: close\r\n\r\n"
    );
    if stream.write_all(header.as_bytes()).await.is_err() {
        return;
    }
    if matches!(framing, WireLimit::Declared) {
        // Header rejection must close without waiting for a single body byte.
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), stream.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        return;
    }
    let data = if matches!(framing, WireLimit::SseComments) {
        format!(":{}\n", "x".repeat(4094))
    } else {
        "x".repeat(4096)
    };
    if matches!(framing, WireLimit::SseData) {
        if stream.write_all(b"6\r\ndata: \r\n").await.is_err() {
            return;
        }
    }
    for _ in 0..(limit / data.len() + 8) {
        if stream
            .write_all(format!("{:x}\r\n", data.len()).as_bytes())
            .await
            .is_err()
            || stream.write_all(data.as_bytes()).await.is_err()
            || stream.write_all(b"\r\n").await.is_err()
        {
            return;
        }
    }
    let _ = stream.write_all(b"0\r\n\r\n").await;
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

#[tokio::test]
async fn oversized_raw_responses_stop_discovery_and_never_replay_writes() {
    for discovery in [true, false] {
        for framing in [
            WireLimit::Declared,
            WireLimit::Chunked,
            WireLimit::SseData,
            WireLimit::SseComments,
            WireLimit::HttpError,
        ] {
            let fixture = Fixture::start(Behavior::Oversized { discovery, framing }).await;
            let service = fixture.service(McpHttpClient::build().unwrap().0).await;
            if discovery {
                assert!(discover_on_service(
                    &service,
                    tokio::time::Instant::now() + Duration::from_secs(2)
                )
                .await
                .is_err());
                assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
            } else {
                let outcome = crate::action_exec::mcp_outcome(
                    execute(&service, Duration::from_secs(2)).await,
                    &prepared(SideEffect::Write),
                );
                assert!(
                    matches!(outcome, ActionOutcome::UnknownOutcome { .. }),
                    "{outcome:?}"
                );
                assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
                assert!(!outcome.model_summary().contains("xxxx"));
            }
            assert!(close_service(service).await);
            assert_eq!(fixture.unexpected.load(Ordering::SeqCst), 0);
            fixture.stop().await;
        }
    }
}

#[tokio::test]
async fn aggregate_wire_bytes_stop_empty_catalog_padding_before_page_limit() {
    let fixture = Fixture::start(Behavior::PaddedPages).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    assert!(discover_on_service(
        &service,
        tokio::time::Instant::now() + Duration::from_secs(3)
    )
    .await
    .is_err());
    assert!(fixture.lists.load(Ordering::SeqCst) <= 9);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn ordinary_sse_discovery_and_tool_result_still_complete_once() {
    let fixture = Fixture::start(Behavior::CompleteSse(json!({"resultType": "complete", "content": [{"type": "text", "text": "recorded"}], "isError": false}))).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let output = execute(&service, Duration::from_secs(2)).await.unwrap();
    assert!(output.text.contains("recorded"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn legacy_handshake_keeps_bounded_transport_and_one_tool_call() {
    let fixture = Fixture::start(Behavior::CompleteLegacy(
        json!({"content": [{"type": "text", "text": "legacy result"}], "isError": false}),
    ))
    .await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let output = execute(&service, Duration::from_secs(2)).await.unwrap();
    assert!(output.text.contains("legacy result"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.unexpected.load(Ordering::SeqCst), 0);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn resumed_get_stream_preserves_identity_and_bounds_raw_sse() {
    use futures_util::StreamExt;
    use rmcp::transport::streamable_http_client::StreamableHttpClient;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint: Arc<str> = format!("http://{}/mcp", listener.local_addr().unwrap()).into();
    let server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let mut bytes = [0; 1024];
            let size = socket.read(&mut bytes).await.unwrap();
            assert!(size > 0 && request.len() + size <= 16 * 1024);
            request.extend_from_slice(&bytes[..size]);
        }
        let headers = std::str::from_utf8(&request).unwrap().to_ascii_lowercase();
        for expected in [
            "get /mcp",
            "mcp-session-id: session",
            "last-event-id: cursor",
            "authorization: bearer fixture-token",
        ] {
            assert!(headers.contains(expected));
        }
        send_oversized(&mut socket, WireLimit::SseComments).await;
    };
    let http = bounded_http::BoundedClient::new(McpHttpClient::build().unwrap().0);
    let get = async {
        let mut stream = http
            .get_stream(
                endpoint,
                Some(Arc::from("session")),
                Some("cursor".into()),
                Some("fixture-token".into()),
                Default::default(),
            )
            .await
            .unwrap();
        loop {
            match stream.next().await {
                Some(Err(_)) => break,
                Some(Ok(_)) => {}
                None => panic!("unbounded comments were accepted"),
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(3), async { tokio::join!(server, get) })
        .await
        .unwrap();
}

#[tokio::test]
async fn bounded_binding_preserves_auth_protocol_headers_and_http_outcomes() {
    use rmcp::transport::streamable_http_client::{
        StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
    };
    for status in [200, 202, 204, 400, 401, 403, 404] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint: Arc<str> = format!("http://{}/mcp", listener.local_addr().unwrap()).into();
        let server = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let header_end = loop {
                let mut bytes = [0; 1024];
                let size = socket.read(&mut bytes).await.unwrap();
                assert!(size > 0 && request.len() + size <= 16 * 1024);
                request.extend_from_slice(&bytes[..size]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = std::str::from_utf8(&request[..header_end])
                .unwrap()
                .to_ascii_lowercase();
            for expected in [
                "authorization: bearer fixture-token",
                "mcp-session-id: fixture-session",
                "mcp-protocol-version: 2026-07-28",
                "mcp-method: tools/list",
                "mcp-param-example: projected",
            ] {
                assert!(headers.contains(expected), "missing {expected}");
            }
            let body = json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32603, "message": "token=secret"}}).to_string();
            let challenge = "Bearer error=\"insufficient_scope\", scope=\"read write\"";
            let reply = format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json; charset=utf-8\r\nWWW-Authenticate: {challenge}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = socket.write_all(reply.as_bytes()).await;
        };
        let message = serde_json::from_value(
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
        )
        .unwrap();
        let mut headers = std::collections::HashMap::new();
        for (name, value) in [
            ("mcp-protocol-version", "2026-07-28"),
            ("mcp-method", "tools/list"),
            ("mcp-param-example", "projected"),
        ] {
            headers.insert(
                reqwest_mcp::header::HeaderName::from_static(name),
                reqwest_mcp::header::HeaderValue::from_static(value),
            );
        }
        let http = bounded_http::BoundedClient::new(McpHttpClient::build().unwrap().0);
        let call = http.post_message(
            endpoint,
            message,
            Some(Arc::from("fixture-session")),
            Some("fixture-token".into()),
            headers,
        );
        let (_, response) =
            tokio::time::timeout(Duration::from_secs(3), async { tokio::join!(server, call) })
                .await
                .unwrap();
        match status {
            200 | 400 => assert!(matches!(
                response,
                Ok(StreamableHttpPostResponse::Json(_, _))
            )),
            202 | 204 => assert!(matches!(response, Ok(StreamableHttpPostResponse::Accepted))),
            401 => {
                let Err(StreamableHttpError::AuthRequired(challenge)) = response else {
                    panic!("lost 401 challenge")
                };
                assert_eq!(
                    challenge.www_authenticate_header,
                    "Bearer error=\"insufficient_scope\", scope=\"read write\""
                );
            }
            403 => {
                let Err(StreamableHttpError::InsufficientScope(challenge)) = response else {
                    panic!("lost 403 challenge")
                };
                assert_eq!(challenge.required_scope.as_deref(), Some("read write"));
            }
            404 => assert!(matches!(response, Err(StreamableHttpError::SessionExpired))),
            _ => unreachable!(),
        }
    }
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
async fn agent_session_close_drops_the_mcp_service_after_one_recorded_write() {
    for behavior in [Behavior::HangAfterWrite, Behavior::HangSseAfterWrite] {
        let fixture = Fixture::start(behavior).await;
        let service = fixture.service(McpHttpClient::build().unwrap().0).await;
        let cancelled = service.cancel.subscribe();
        let control = crate::agent::AgentRunControl::default();
        let (run, registration) = control.begin().unwrap();
        let operation = run.wait(registration, async move {
            let result = execute(&service, Duration::from_secs(30)).await;
            close_service(service).await;
            result
        });
        let close = async {
            tokio::time::timeout(Duration::from_secs(2), async {
                while fixture.calls.load(Ordering::SeqCst) == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            control.cancel();
        };
        let (result, _) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(operation, close)
        })
        .await
        .unwrap();
        let message = result.unwrap_err();
        assert!(message.contains("may have had effects"));
        assert!(!message.contains("did not run"));
        assert!(
            *cancelled.borrow(),
            "actual service Drop signals HTTP teardown"
        );
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.unexpected.load(Ordering::SeqCst), 0);
        fixture.stop().await;
    }
}

#[tokio::test]
async fn account_cancellation_after_a_recorded_write_remains_unknown() {
    let fixture = Fixture::start(Behavior::HangAfterWrite).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let control = session::Control::new();
    let ticket = control.ticket();
    let operation = async {
        ticket
            .run(execute(&service, Duration::from_secs(3)))
            .await
            .unwrap_or_else(|message| Err(session_cancelled(&service, message)))
    };
    let logout = async {
        tokio::time::timeout(Duration::from_secs(2), async {
            while fixture.calls.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        control.invalidate();
    };
    let (result, _) = tokio::join!(operation, logout);
    let outcome = crate::action_exec::mcp_outcome(result, &prepared(SideEffect::Write));
    assert!(matches!(outcome, ActionOutcome::UnknownOutcome { .. }));
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

#[tokio::test]
async fn nested_invalid_arguments_never_reach_tools_call() {
    let mut tool = tool_json();
    tool["inputSchema"] = json!({"type": "object", "required": ["request"],
        "properties": {"request": {"type": "object", "required": ["count"],
            "properties": {"count": {"type": "integer", "minimum": 1}},
            "additionalProperties": false}}, "additionalProperties": false});
    let digest = tool_set_digest(
        provider("linear").unwrap(),
        &[serde_json::from_value(tool.clone()).unwrap()],
    )
    .unwrap();
    let fixture = Fixture::start_with_tool(
        Behavior::Complete(json!({"resultType": "complete",
        "content": [{"type": "text", "text": "OK"}]})),
        tool,
    )
    .await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    for arguments in [
        json!({"request": {"count": "private-token"}}),
        json!({"request": {"count": 0}}),
        json!({"request": {"count": 1, "extra": true}}),
    ] {
        let failure = call_on_service(
            &service,
            provider("linear").unwrap(),
            "write",
            arguments.as_object().unwrap().clone(),
            &digest,
            tokio::time::Instant::now() + Duration::from_secs(3),
            || true,
        )
        .await
        .unwrap_err();
        assert_eq!(failure.phase, DispatchPhase::NotDispatched);
        assert_eq!(failure.class, FailureClass::InvalidArgument);
        assert!(!failure.message.contains("private-token"));
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        assert!(!service.dispatched.load(Ordering::SeqCst));
    }
    call_on_service(
        &service,
        provider("linear").unwrap(),
        "write",
        json!({"request": {"count": 1}})
            .as_object()
            .unwrap()
            .clone(),
        &digest,
        tokio::time::Instant::now() + Duration::from_secs(3),
        || true,
    )
    .await
    .unwrap();
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn malformed_schema_stops_discovery_without_a_tool_call() {
    let mut tool = tool_json();
    tool["inputSchema"]["required"] = json!([7]);
    let fixture = Fixture::start_with_tool(Behavior::Complete(json!({})), tool).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let failure = execute(&service, Duration::from_secs(3)).await.unwrap_err();
    assert_eq!(failure.phase, DispatchPhase::NotDispatched);
    assert!(failure.message.contains("changed after confirmation"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn duplicate_tool_names_across_pages_never_dispatch() {
    let fixture = Fixture::start(Behavior::DuplicateToolsAcrossPages).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let failure = execute(&service, Duration::from_secs(3)).await.unwrap_err();
    assert_eq!(failure.phase, DispatchPhase::NotDispatched);
    assert!(failure.message.contains("duplicate tool names"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.lists.load(Ordering::SeqCst), 2);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn unsupported_tools_are_isolated_across_pages_and_cannot_dispatch() {
    let mut bad = tool_json();
    bad["name"] = json!("unsupported");
    bad["inputSchema"]["properties"] = json!({"x": {"$ref": "https://invalid.example/schema"}});
    let valid = tool_json();
    let fixture =
        Fixture::start(Behavior::CatalogPages(vec![vec![bad], vec![valid.clone()]])).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let tools = discover_on_service(
        &service,
        tokio::time::Instant::now() + Duration::from_secs(3),
    )
    .await
    .unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name.as_ref(), "write");
    let item = provider("linear").unwrap();
    let digest = tool_set_digest(item, &tools).unwrap();
    let rejected = call_on_service(
        &service,
        item,
        "unsupported",
        serde_json::Map::new(),
        &digest,
        tokio::time::Instant::now() + Duration::from_secs(3),
        || true,
    )
    .await
    .unwrap_err();
    assert_eq!(rejected.phase, DispatchPhase::NotDispatched);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    call_on_service(
        &service,
        item,
        "write",
        serde_json::Map::new(),
        &digest,
        tokio::time::Instant::now() + Duration::from_secs(3),
        || true,
    )
    .await
    .unwrap();
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert!(close_service(service).await);
    fixture.stop().await;
}

#[tokio::test]
async fn rejected_definitions_cannot_hide_duplicates_or_tool_budget() {
    for over_budget in [false, true] {
        let mut bad = tool_json();
        bad["inputSchema"]["required"] = json!([7]);
        let pages = if over_budget {
            vec![(0..=MAX_TOOL_COUNT)
                .map(|index| {
                    let mut bad = bad.clone();
                    bad["name"] = json!(format!("unsupported{index}"));
                    bad
                })
                .collect()]
        } else {
            vec![vec![bad], vec![tool_json()]]
        };
        let fixture = Fixture::start(Behavior::CatalogPages(pages)).await;
        let service = fixture.service(McpHttpClient::build().unwrap().0).await;
        let error = discover_on_service(
            &service,
            tokio::time::Instant::now() + Duration::from_secs(3),
        )
        .await
        .unwrap_err();
        assert!(
            error.contains(if over_budget {
                "tool limit"
            } else {
                "duplicate tool names"
            }),
            "{error}"
        );
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        assert!(close_service(service).await);
        fixture.stop().await;
    }
}

#[tokio::test]
async fn entirely_unsupported_catalog_is_empty_and_digest_still_revalidates() {
    let mut bad = tool_json();
    bad["inputSchema"]["required"] = json!([7]);
    let fixture = Fixture::start(Behavior::CatalogPages(vec![vec![bad]])).await;
    let service = fixture.service(McpHttpClient::build().unwrap().0).await;
    let tools = discover_on_service(
        &service,
        tokio::time::Instant::now() + Duration::from_secs(3),
    )
    .await
    .unwrap();
    assert!(tools.is_empty());
    let failure = execute(&service, Duration::from_secs(3)).await.unwrap_err();
    assert_eq!(failure.phase, DispatchPhase::NotDispatched);
    assert!(failure.message.contains("changed after confirmation"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    assert!(close_service(service).await);
    fixture.stop().await;
}
