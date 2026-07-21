//! [GRAIN] Authentication + capability filtering for the local events
//! WebSocket (SPEC §7.1) — the enforcement point of the extension platform.
//!
//! Identity is bound to the **channel**: a connection is whoever the token in
//! its first frame maps to in the server-side [`TokenRegistry`] — never what
//! the hello's `client` label claims, and never anything asserted in later
//! messages. Impersonating another client is therefore not expressible: to be
//! the pill you must hold the pill's token, which only the pill's environment
//! ever contains.
//!
//! This module is deliberately pure (no sockets, no Tauri) so the security
//! properties are unit-tested directly: tokenless/unknown rejection, identity
//! from the table, and per-capability event filtering.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use grain_core::DaemonEvent;
use grain_sdk::ClientHello;

/// What a connected client may receive/do. The pill is `All`; extension
/// workers (Phase 2) get `Named` sets derived from user-granted manifests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilitySet {
    All,
    Named(HashSet<String>),
}

/// A resolved identity: the registry entry the presented token mapped to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientIdentity {
    /// Stable id ("pill", or an extension id). From the registry, never the wire.
    pub id: String,
    pub caps: CapabilitySet,
}

/// token → identity. Static for the life of the app run; tokens are minted at
/// spawn time and revoked by removal (extension disable/uninstall, Phase 2).
pub struct TokenRegistry {
    map: RwLock<HashMap<String, ClientIdentity>>,
}

impl TokenRegistry {
    pub fn new() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, token: String, identity: ClientIdentity) {
        self.map.write().unwrap().insert(token, identity);
    }

    #[allow(dead_code)] // Phase 2: revocation on extension disable/uninstall.
    pub fn revoke(&self, token: &str) {
        self.map.write().unwrap().remove(token);
    }

    /// Authenticate a connection's FIRST frame. `None` = drop the connection.
    ///
    /// Rejects: non-JSON, JSON that isn't a [`ClientHello`], empty tokens, and
    /// tokens not in the registry. The returned identity is the registry's —
    /// the hello's `client` field is used for nothing but logs upstream.
    pub fn authenticate(&self, first_frame: &str) -> Option<ClientIdentity> {
        let hello: ClientHello = serde_json::from_str(first_frame).ok()?;
        if hello.token.is_empty() {
            return None;
        }
        self.map.read().unwrap().get(&hello.token).cloned()
    }
}

/// The capability an event requires. Phase 0 granularity: transcript-bearing
/// events, the high-frequency level feed, and everything else as session/UI
/// signals. Refined in Phase 2 when `Named` consumers exist.
fn required_capability(ev: &DaemonEvent) -> &'static str {
    use DaemonEvent::*;
    match ev {
        ChunkComplete { .. }
        | TranscriptionComplete { .. }
        | ProcessingComplete { .. }
        | AsrStreamText { .. }
        | AsrPartial { .. }
        | AsrCommit { .. }
        | AsrSegmentFinal { .. }
        | AsrSessionFinal { .. } => "events:transcripts",
        AudioLevel { .. } => "events:audio-levels",
        _ => "events:sessions",
    }
}

/// May this identity receive this event? (Filtered = never sent, not blanked.)
pub fn allows_event(identity: &ClientIdentity, ev: &DaemonEvent) -> bool {
    match &identity.caps {
        CapabilitySet::All => true,
        CapabilitySet::Named(caps) => caps.contains(required_capability(ev)),
    }
}

/// May this identity use the reverse channel (PillAction)? Pill-only surface;
/// extensions get their own namespaced commands in Phase 2.
pub fn allows_reverse(identity: &ClientIdentity) -> bool {
    match &identity.caps {
        CapabilitySet::All => true,
        CapabilitySet::Named(caps) => caps.contains("reverse:pill"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with_pill_and_ext() -> TokenRegistry {
        let reg = TokenRegistry::new();
        reg.register(
            "pill-secret".into(),
            ClientIdentity {
                id: "pill".into(),
                caps: CapabilitySet::All,
            },
        );
        reg.register(
            "ext-a-secret".into(),
            ClientIdentity {
                id: "com.example.a".into(),
                caps: CapabilitySet::Named(
                    ["events:sessions".to_string()].into_iter().collect(),
                ),
            },
        );
        reg
    }

    #[test]
    fn tokenless_and_unknown_clients_are_rejected() {
        let reg = registry_with_pill_and_ext();
        assert!(reg.authenticate("not json").is_none());
        assert!(reg.authenticate(r#"{"action":"prompt_record"}"#).is_none()); // an action, not a hello
        assert!(reg.authenticate(r#"{"token":""}"#).is_none());
        assert!(reg.authenticate(r#"{"token":"wrong"}"#).is_none());
    }

    #[test]
    fn identity_comes_from_the_table_not_the_label() {
        let reg = registry_with_pill_and_ext();
        // SPEC §8 Phase 0: a client holding A's token cannot act as anyone
        // else — even while *claiming* to be the pill in its hello.
        let id = reg
            .authenticate(r#"{"token":"ext-a-secret","client":"pill"}"#)
            .unwrap();
        assert_eq!(id.id, "com.example.a");
        assert_ne!(id.caps, CapabilitySet::All);
    }

    #[test]
    fn capability_filter_gates_transcripts_and_levels() {
        let reg = registry_with_pill_and_ext();
        let ext = reg.authenticate(r#"{"token":"ext-a-secret"}"#).unwrap();
        let pill = reg.authenticate(r#"{"token":"pill-secret"}"#).unwrap();

        let transcript = DaemonEvent::TranscriptionComplete {
            session_id: 1,
            text: "secret words".into(),
        };
        let session = DaemonEvent::RecordingStopped { session_id: 1 };
        let levels = DaemonEvent::AudioLevel { levels: vec![0.5] };

        assert!(!allows_event(&ext, &transcript), "no transcript cap");
        assert!(!allows_event(&ext, &levels), "no audio-levels cap");
        assert!(allows_event(&ext, &session), "sessions granted");
        assert!(allows_event(&pill, &transcript) && allows_event(&pill, &levels));

        assert!(!allows_reverse(&ext));
        assert!(allows_reverse(&pill));
    }

    #[test]
    fn revocation_kills_the_token() {
        let reg = registry_with_pill_and_ext();
        assert!(reg.authenticate(r#"{"token":"ext-a-secret"}"#).is_some());
        reg.revoke("ext-a-secret");
        assert!(reg.authenticate(r#"{"token":"ext-a-secret"}"#).is_none());
    }
}
