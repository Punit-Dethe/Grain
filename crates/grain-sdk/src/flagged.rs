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
    /// `notes` + any `net:` grant: it can read everything the user has written
    /// down AND send it somewhere. Not blocked — a publishing or sync extension
    /// is a real thing to want — but it is the combination that most warrants a
    /// human reading the source before it is listed.
    NotesAndNetwork,
    /// `capture:screen-image` + any `net:` grant.
    ScreenCaptureAndNetwork,
    /// `capture:screen-text` + any `net:` grant.
    ScreenTextAndNetwork,
    /// `events:transcripts` + any `net:` grant.
    TranscriptsAndNetwork,
    /// A `native`-tier extension + any `net:` grant.
    NativeAndNetwork,
}

impl FlaggedCombination {
    /// A stable machine key (for labels, CI, and the store card wire form).
    pub fn key(&self) -> &'static str {
        match self {
            FlaggedCombination::NotesAndNetwork => "notes+net",
            FlaggedCombination::ScreenCaptureAndNetwork => "screen-capture+net",
            FlaggedCombination::ScreenTextAndNetwork => "screen-text+net",
            FlaggedCombination::TranscriptsAndNetwork => "transcripts+net",
            FlaggedCombination::NativeAndNetwork => "native+net",
        }
    }

    /// Plain-language line shown to the user and the reviewer.
    pub fn reason(&self) -> &'static str {
        match self {
            FlaggedCombination::NotesAndNetwork => {
                "can read all your notes and send them over the network"
            }
            FlaggedCombination::ScreenCaptureAndNetwork => {
                "can take screenshots of your screen and send them over the network"
            }
            FlaggedCombination::ScreenTextAndNetwork => {
                "can read the text on your screen and send it over the network"
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
    if permissions.iter().any(|p| p == "notes") {
        flags.push(FlaggedCombination::NotesAndNetwork);
    }
    // [GRAIN] These check the capability names that actually exist. They used to
    // check `screen:capture`, which was never in KNOWN_CAPABILITIES and so could
    // never appear in an importable manifest — the flag was written ahead of the
    // capability and then pointed at a name the capability never took, meaning
    // it had never once fired.
    if permissions.iter().any(|p| p == "capture:screen-image") {
        flags.push(FlaggedCombination::ScreenCaptureAndNetwork);
    }
    if permissions.iter().any(|p| p == "capture:screen-text") {
        flags.push(FlaggedCombination::ScreenTextAndNetwork);
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
        let f = flagged_combinations(
            &perms(&["capture:screen-image", "events:transcripts"]),
            Tier::Native,
        );
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
            &perms(&["capture:screen-image", "net:collector.example.com"]),
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
            &perms(&[
                "events:transcripts",
                "capture:screen-image",
                "net:x.example.com",
            ]),
            Tier::Native,
        );
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn screen_text_plus_net_is_flagged() {
        let f = flagged_combinations(
            &perms(&["capture:screen-text", "net:collector.example.com"]),
            Tier::Scripted,
        );
        assert_eq!(f, vec![FlaggedCombination::ScreenTextAndNetwork]);
    }

    #[test]
    fn storage_only_is_not_flagged() {
        let f = flagged_combinations(&perms(&["storage", "llm"]), Tier::Scripted);
        assert!(f.is_empty());
    }

    /// Every capability a flag watches for must be one a manifest can actually
    /// declare.
    ///
    /// This is the bug this test exists for, not a hypothetical: the screen flag
    /// checked `screen:capture` for as long as it existed, a name that was never
    /// in `KNOWN_CAPABILITIES`. The import path rejects unknown capabilities, so
    /// no manifest could ever carry it and the flag could never fire — a warning
    /// that silently did nothing, which is worse than no warning at all. Nothing
    /// caught it because the tests asserted against the same wrong string.
    #[test]
    fn every_watched_capability_is_a_real_one() {
        use crate::manifest::KNOWN_CAPABILITIES;
        for watched in [
            "notes",
            "capture:screen-image",
            "capture:screen-text",
            "events:transcripts",
        ] {
            assert!(
                KNOWN_CAPABILITIES.contains(&watched),
                "flagged.rs watches '{watched}', which no manifest can declare"
            );
        }
    }
}
