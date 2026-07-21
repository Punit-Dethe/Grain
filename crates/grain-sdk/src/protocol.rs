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

/// The contract's version, sent back in [`ServerWelcome`]. Clients decide for
/// themselves whether they can speak it (additive changes bump the minor).
pub const GRAIN_API_VERSION: &str = "1.0";

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
}
