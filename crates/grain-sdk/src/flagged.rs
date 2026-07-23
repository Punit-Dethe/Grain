//! Flagged capability combinations (DISTRIBUTION-PLAN §3.3) — the single source
//! of "read this part carefully", shared by the app, the CLI, and registry CI so
//! none of them can disagree about what a user is warned about.
//!
//! A flag **blocks nothing.** It says a reviewer must read a part closely and
//! ask the author for a written justification, and it puts a plain line on the
//! store card: *this extension can see something private and can send it
//! somewhere.* There is deliberately no numeric risk score — with 100% human
//! review nothing auto-publishes, so a number has no routing job (§3.3).

use crate::manifest::{network_capability_host, Tier};

/// A flagged combination present in a manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlaggedCombination {
    /// `screen:capture` + any `net:` grant.
    ScreenCaptureAndNetwork,
    /// `events:transcripts` + any `net:` grant.
    TranscriptsAndNetwork,
    /// A `native`-tier extension + any `net:` grant.
    NativeAndNetwork,
}

impl FlaggedCombination {
    /// A stable machine key (for labels, CI, and the store card wire form).
    pub fn key(&self) -> &'static str {
        match self {
            FlaggedCombination::ScreenCaptureAndNetwork => "screen-capture+net",
            FlaggedCombination::TranscriptsAndNetwork => "transcripts+net",
            FlaggedCombination::NativeAndNetwork => "native+net",
        }
    }

    /// Plain-language line shown to the user and the reviewer.
    pub fn reason(&self) -> &'static str {
        match self {
            FlaggedCombination::ScreenCaptureAndNetwork => {
                "can capture your screen and send it over the network"
            }
            FlaggedCombination::TranscriptsAndNetwork => {
                "can read your transcripts and send them over the network"
            }
            FlaggedCombination::NativeAndNetwork => {
                "runs a native program that can access the network"
            }
        }
    }
}

/// True if any permission is a valid per-host `net:` grant.
fn requests_network(permissions: &[String]) -> bool {
    permissions
        .iter()
        .any(|p| network_capability_host(p).is_some())
}

/// The flagged combinations present for a manifest's `permissions` + `tier`.
/// Empty means nothing needs a closer read on capability grounds.
pub fn flagged_combinations(permissions: &[String], tier: Tier) -> Vec<FlaggedCombination> {
    let net = requests_network(permissions);
    if !net {
        return Vec::new();
    }
    let mut flags = Vec::new();
    if permissions.iter().any(|p| p == "screen:capture") {
        flags.push(FlaggedCombination::ScreenCaptureAndNetwork);
    }
    if permissions.iter().any(|p| p == "events:transcripts") {
        flags.push(FlaggedCombination::TranscriptsAndNetwork);
    }
    if tier == Tier::Native {
        flags.push(FlaggedCombination::NativeAndNetwork);
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perms(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_network_means_no_flags() {
        let f = flagged_combinations(&perms(&["screen:capture", "events:transcripts"]), Tier::Native);
        assert!(f.is_empty(), "flags require a net grant to be present");
    }

    #[test]
    fn transcripts_plus_net_is_flagged() {
        let f = flagged_combinations(
            &perms(&["events:transcripts", "net:api.example.com"]),
            Tier::Scripted,
        );
        assert_eq!(f, vec![FlaggedCombination::TranscriptsAndNetwork]);
    }

    #[test]
    fn screen_capture_plus_net_is_flagged() {
        let f = flagged_combinations(
            &perms(&["screen:capture", "net:collector.example.com"]),
            Tier::Scripted,
        );
        assert_eq!(f, vec![FlaggedCombination::ScreenCaptureAndNetwork]);
    }

    #[test]
    fn native_plus_net_is_flagged() {
        let f = flagged_combinations(&perms(&["net:api.example.com"]), Tier::Native);
        assert_eq!(f, vec![FlaggedCombination::NativeAndNetwork]);
    }

    #[test]
    fn multiple_flags_can_coexist() {
        let f = flagged_combinations(
            &perms(&["events:transcripts", "screen:capture", "net:x.example.com"]),
            Tier::Native,
        );
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn storage_only_is_not_flagged() {
        let f = flagged_combinations(&perms(&["storage", "llm"]), Tier::Scripted);
        assert!(f.is_empty());
    }
}
