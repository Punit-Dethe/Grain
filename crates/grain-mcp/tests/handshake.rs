//! [GRAIN] The proxy's half of the local-port conversation, against a stand-in
//! for Grain.
//!
//! This exists because the handshake is the one part of the bridge that a
//! protocol probe on stdin cannot reach: the ordering (hello, then request) and
//! the requirement to read *past* the welcome frame and any events before the
//! answer arrives are only exercised when something is actually listening. The
//! stand-in speaks the same frames the app does, so getting this wrong here
//! means getting it wrong there.

use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

/// What the stand-in saw, so a test can assert on the proxy's side of it.
struct Seen {
    hello: Value,
    request: Value,
}

/// Accept one connection, behave like Grain, answer with `reply`, and report
/// what the client sent. `noise` is emitted before the answer — the welcome
/// frame and an event — because the proxy must read past both.
async fn stand_in(reply: Value, noise: bool) -> (SocketAddr, tokio::task::JoinHandle<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

        let hello: Value = match ws.next().await.unwrap().unwrap() {
            Message::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("expected a text hello, got {other:?}"),
        };
        if noise {
            // The app sends a welcome the moment a token is accepted, and may
            // push events at any time. Neither is our answer.
            ws.send(Message::Text(
                json!({ "grain_api": "1.0", "client": "grain" }).to_string(),
            ))
            .await
            .unwrap();
            ws.send(Message::Text(
                json!({ "RecordingStarted": { "mode": "dictate" } }).to_string(),
            ))
            .await
            .unwrap();
        }
        let request: Value = match ws.next().await.unwrap().unwrap() {
            Message::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("expected a text request, got {other:?}"),
        };
        let id = request["req"]["id"].as_u64().unwrap();
        let mut res = reply;
        res["id"] = json!(id);
        ws.send(Message::Text(json!({ "res": res }).to_string()))
            .await
            .unwrap();
        Seen { hello, request }
    });
    (addr, handle)
}

/// Point the proxy at the stand-in by writing the token file it reads.
fn token_file(dir: &std::path::Path, addr: SocketAddr) {
    std::fs::write(
        dir.join("mcp-token.json"),
        json!({ "token": "test-token", "port": addr.port() }).to_string(),
    )
    .unwrap();
}

#[tokio::test]
async fn a_call_says_hello_first_then_reads_past_the_noise_for_its_answer() {
    let dir = tempfile::tempdir().unwrap();
    let (addr, server) =
        stand_in(json!({ "ok": { "collections": ["Work", "Ideas"] } }), true).await;
    token_file(dir.path(), addr);

    let link = grain_mcp::AppLink::for_token_file(dir.path().join("mcp-token.json"));
    let answer = link.call("space.collections", json!({})).await.unwrap();

    let seen = server.await.unwrap();
    // Hello carries the minted token, identifying the proxy to the registry.
    assert_eq!(seen.hello["token"], "test-token");
    assert_eq!(seen.hello["client"], "grain-mcp");
    // The request rides the same frame extension workers use.
    assert_eq!(seen.request["req"]["method"], "space.collections");
    // And the welcome + event did not get mistaken for the answer.
    assert_eq!(answer["collections"][0], "Work");
}

#[tokio::test]
async fn an_error_from_the_app_reaches_the_caller_as_its_message() {
    let dir = tempfile::tempdir().unwrap();
    let (addr, server) = stand_in(
        json!({ "err": { "code": "E_DENIED", "message": "Grain Space is switched off." } }),
        false,
    )
    .await;
    token_file(dir.path(), addr);

    let link = grain_mcp::AppLink::for_token_file(dir.path().join("mcp-token.json"));
    let err = link
        .call("space.get", json!({ "id": "x" }))
        .await
        .unwrap_err();

    server.await.unwrap();
    // The app's own sentence, not a generic failure — it is what the user reads.
    assert_eq!(err.to_string(), "Grain Space is switched off.");
}

#[tokio::test]
async fn no_token_file_reads_as_the_bridge_being_off_not_as_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let link = grain_mcp::AppLink::for_token_file(dir.path().join("mcp-token.json"));
    let err = link
        .call("space.collections", json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("switch the MCP bridge on"),
        "expected the actionable message, got: {err}"
    );
}
