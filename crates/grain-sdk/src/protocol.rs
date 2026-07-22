//! The authenticated-connection handshake for Grain's local WebSocket
//! (SPEC §7.1).
//!
//! Security model in one paragraph: the events server binds `127.0.0.1` only
//! and gives **nothing** to a connection until it authenticates. The first
//! frame a client sends must be a [`ClientHello`] carrying a per-client token
//! the host minted for it (pill: injected at spawn via `GRAIN_EVENTS_TOKEN`;
//! extension workers: injected at creation, each with its *own* token). The
//! server maps token → identity → capability set **server-side**; nothing in
//! any later message can change who you are, so impersonating another client
//! is not expressible. Unauthenticated connections are dropped on a short
//! deadline. Tokens travel only in this first frame — never in the URL, where
//! query strings leak into logs.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The contract's version, sent back in [`ServerWelcome`]. Clients decide for
/// themselves whether they can speak it (additive changes bump the minor).
pub const GRAIN_API_VERSION: &str = "1.0";

// ── Host-API framing (Phase 2, SPEC §7.1) ───────────────────────────────────
//
// After the hello/welcome handshake, an extension worker and the host exchange
// four message shapes over the same duplex text channel. They are wrapped in
// [`HostFrame`] — an externally-tagged enum, so each serializes with an
// unambiguous top-level key (`req` / `res` / `call` / `callres`) that cannot
// collide with a `DaemonEvent` variant name, `PillAction`'s `action` tag, or
// the hello/welcome field sets. The `protocol_frames_are_mutually_exclusive`
// test is the guarantee.

/// Worker → server: an API call (`grain.storage.get`, `grain.llm.complete`, …).
/// `id` correlates the [`ServerResponse`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Server → worker: the answer to a [`ClientRequest`]. Exactly one of `ok`/`err`
/// is set.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerResponse {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Server → worker: a host-initiated call (a transform step, an event
/// notification, a session result). The worker must answer with a
/// [`HostCallResult`] carrying the same `call_id` before the host's deadline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostCall {
    pub call_id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Worker → server: the answer to a [`HostCall`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostCallResult {
    pub call_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// The wire wrapper. Externally tagged → serializes as `{"req":{…}}`,
/// `{"res":{…}}`, `{"call":{…}}`, `{"callres":{…}}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HostFrame {
    #[serde(rename = "req")]
    Request(ClientRequest),
    #[serde(rename = "res")]
    Response(ServerResponse),
    #[serde(rename = "call")]
    Call(HostCall),
    #[serde(rename = "callres")]
    CallResult(HostCallResult),
}

/// First frame on every connection, client → server.
///
/// `client` is a **label for logs only** — identity comes exclusively from the
/// server-side token table. A hello carrying extension A's token is A no
/// matter what `client` claims.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientHello {
    /// The per-client secret minted by the host for this app run.
    pub token: String,
    /// Human-readable label for logs/diagnostics (e.g. "pill"). Not identity.
    #[serde(default)]
    pub client: String,
    /// The contract version the client was built against (informational; the
    /// server replies with its own in [`ServerWelcome`]).
    #[serde(default)]
    pub grain_api: String,
}

/// Server → client, immediately after a hello is accepted. Any client that
/// predates this handshake simply ignores the frame (it does not parse as a
/// [`crate::DaemonEvent`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerWelcome {
    /// The server's contract version ([`GRAIN_API_VERSION`]).
    pub grain_api: String,
}

/// Developer CLI -> host control message. This channel is deliberately tiny:
/// the CLI may request a reload of an already human-approved unpacked project,
/// but it cannot send source, alter grants, or invoke extension host APIs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DevControlFrame {
    DevReload {
        request_id: u64,
        extension_id: String,
    },
    DevResult {
        request_id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<DevReloadResult>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevReloadResult {
    pub restarted_worker: bool,
    pub remounted_surfaces: bool,
    pub enabled: bool,
    pub worker_count: usize,
    pub token_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrip_and_defaults() {
        let json = r#"{"token":"abc"}"#;
        let hello: ClientHello = serde_json::from_str(json).unwrap();
        assert_eq!(hello.token, "abc");
        assert_eq!(hello.client, "");
        assert_eq!(hello.grain_api, "");

        let full = ClientHello {
            token: "t".into(),
            client: "pill".into(),
            grain_api: GRAIN_API_VERSION.into(),
        };
        let back: ClientHello =
            serde_json::from_str(&serde_json::to_string(&full).unwrap()).unwrap();
        assert_eq!(back.client, "pill");
    }

    #[test]
    fn welcome_is_not_confusable_with_an_event() {
        let w = ServerWelcome {
            grain_api: GRAIN_API_VERSION.into(),
        };
        let json = serde_json::to_string(&w).unwrap();
        // Old clients parse incoming frames as DaemonEvent and ignore failures;
        // the welcome must therefore never deserialize as one.
        assert!(serde_json::from_str::<crate::DaemonEvent>(&json).is_err());
    }

    #[test]
    fn host_frames_roundtrip_and_wrap() {
        let req = HostFrame::Request(ClientRequest {
            id: 7,
            method: "storage.get".into(),
            params: serde_json::json!({"key": "k"}),
        });
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.starts_with(r#"{"req":"#), "wrapper key: {json}");
        matches!(
            serde_json::from_str::<HostFrame>(&json).unwrap(),
            HostFrame::Request(_)
        );

        // ok/err omitted when None (clean wire form).
        let res = serde_json::to_string(&HostFrame::Response(ServerResponse {
            id: 7,
            ok: Some(serde_json::json!("v")),
            err: None,
        }))
        .unwrap();
        assert!(res.contains(r#""ok":"v""#) && !res.contains("err"));
    }

    #[test]
    fn protocol_frames_are_mutually_exclusive() {
        // The five wire shapes that share the duplex channel must never parse
        // as one another, so the read loop can discriminate by trying each.
        let host_req = serde_json::to_string(&HostFrame::Request(ClientRequest {
            id: 1,
            method: "log.info".into(),
            params: Value::Null,
        }))
        .unwrap();
        let host_callres = serde_json::to_string(&HostFrame::CallResult(HostCallResult {
            call_id: 1,
            ok: None,
            err: None,
        }))
        .unwrap();
        let event =
            serde_json::to_string(&crate::DaemonEvent::RecordingStopped { session_id: 1 }).unwrap();
        let action = serde_json::to_string(&crate::PillAction::PromptRecord).unwrap();
        let hello = r#"{"token":"abc"}"#.to_string();

        // A HostFrame is nothing else.
        assert!(serde_json::from_str::<crate::DaemonEvent>(&host_req).is_err());
        assert!(serde_json::from_str::<crate::PillAction>(&host_req).is_err());
        assert!(serde_json::from_str::<ClientHello>(&host_req).is_err());
        assert!(serde_json::from_str::<HostFrame>(&host_callres).is_ok());

        // …and nothing else is a HostFrame.
        assert!(serde_json::from_str::<HostFrame>(&event).is_err());
        assert!(serde_json::from_str::<HostFrame>(&action).is_err());
        assert!(serde_json::from_str::<HostFrame>(&hello).is_err());

        let dev = serde_json::to_string(&DevControlFrame::DevReload {
            request_id: 4,
            extension_id: "com.example.dev".into(),
        })
        .unwrap();
        assert!(serde_json::from_str::<DevControlFrame>(&dev).is_ok());
        assert!(dev.contains("requestId") && dev.contains("extensionId"));
        assert!(serde_json::from_str::<HostFrame>(&dev).is_err());
        assert!(serde_json::from_str::<crate::PillAction>(&dev).is_err());
    }
}
