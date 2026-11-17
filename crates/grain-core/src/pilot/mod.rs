//! Grain Pilot: the Agent acting in the user's own browser, signed in as
//! them, through the Grain Pilot browser extension
//! (`docs/Pilot/EXTENSION-PLAN.md`).
//!
//! The extension (`browser-extension/`) opens tabs in a "Grain" tab group and
//! reads and acts on pages with page-agent's page controller. Grain owns the
//! tools the model sees, every safety rule, and the shaping of page reads.
//!
//! This module is the pure half. It performs no I/O. The desktop crate
//! (`grain_pilot.rs`, `grain_browser_link.rs`) talks to the extension.
//!
//! - [`tools`]: the tools the model sees and argument parsing into a [`tools::Call`].
//! - [`page`]: turning the extension's page reads into short rows, views and
//!   change reports.
//! - [`render`]: replies and refusals as plain text for the model.
//! - [`policy`]: links, keys, text limits and payment controls.

pub mod page;
pub mod policy;
pub mod render;
pub mod tools;

/// Extension id the Agent directory, prepared calls and settings use.
pub const EXTENSION_ID: &str = "grain.pilot";
/// Name the model and the Extensions page show.
pub const PROVIDER_NAME: &str = "Grain Pilot";
/// Directory description the model reads when choosing a provider. The Agent
/// directory shows at most 240 bytes.
pub const DESCRIPTION: &str = "Do things in the user's own browser (signed in as them): book, order, fill forms, change settings. Never to look things up or research: use web search. Prefer a connector. Hand over with show before paying.";

/// Most characters one read hands the model (about 4k tokens).
pub const MAX_READ_CHARS: usize = 16_000;
/// Most characters a "what changed" reply hands the model.
pub const MAX_CHANGE_CHARS: usize = 4_000;
/// Longest text one `type` sends.
pub const MAX_TYPE_CHARS: usize = 5_000;
/// Longest option text, search phrase or other short field.
pub const MAX_FIELD_CHARS: usize = 300;
/// Longest note `show` passes on for the user.
pub const MAX_SHOW_MESSAGE_CHARS: usize = 200;
/// Most steps one `act` call runs.
pub const MAX_STEPS: usize = 8;
/// Pixels one `scroll` step moves: about one screen.
pub const SCROLL_PIXELS: u32 = 600;

/// Bump when the tool set or a policy here changes meaning.
const POLICY_VERSION: u32 = 3;

/// Content digest of the tool set and policy, the compiled equivalent of an
/// MCP manifest digest. Changes only with a new build.
pub fn catalog_digest() -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(POLICY_VERSION.to_le_bytes());
    for tool in tools::tools() {
        hasher.update(tool.name.as_bytes());
        hasher.update([0]);
        hasher.update(tool.description.as_bytes());
        hasher.update([0]);
        hasher.update(tool.schema.to_string().as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Cut `value` to `max_chars` characters, marking the cut.
pub(crate) fn clip(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        let mut out: String = value.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_fits_the_directory_and_digest_is_stable() {
        assert!(DESCRIPTION.len() <= 240, "{}", DESCRIPTION.len());
        assert_eq!(catalog_digest(), catalog_digest());
        assert_eq!(catalog_digest().len(), 64);
    }

    #[test]
    fn clip_marks_the_cut() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("abcdef", 3), "abc…");
    }
}
