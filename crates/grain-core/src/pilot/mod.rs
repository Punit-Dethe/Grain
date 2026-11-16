//! Grain Pilot: the Agent acting in the user's own browser and apps through
//! the Cua Driver sidecar (`docs/Pilot/PLAN.md`).
//!
//! Grain does not maintain an automation engine. Cua Driver (MIT, Rust) reads
//! windows through the OS accessibility tree and acts on them in the
//! background. Grain owns a small, fixed set of Agent tools, the mapping from
//! those tools to driver calls, and the part that keeps page reads small.
//!
//! This module is the pure half. It performs no I/O: no processes, no
//! network, no Win32. The desktop crate (`grain_pilot.rs`) runs the driver.
//!
//! - [`tools`]: the tools the model sees and argument parsing into a [`tools::Call`].
//! - [`numbering`]: Pilot's own control numbers, stable for a window's lifetime.
//! - [`compact`]: turning a driver read into a short list the model reads.
//! - [`request`]: the exact driver tool and JSON arguments for each step.
//! - [`render`]: driver replies and refusals as plain text for the model.
//! - [`policy`]: links, keys, text limits, refused apps and controls.
//! - [`manifest`]: the capability manifest that caps what the driver may do.

pub mod compact;
pub mod manifest;
pub mod numbering;
pub mod policy;
pub mod render;
pub mod request;
pub mod tools;

/// Extension id the Agent directory, prepared calls and settings use.
pub const EXTENSION_ID: &str = "grain.pilot";
/// Name the model and the Extensions page show.
pub const PROVIDER_NAME: &str = "Grain Pilot";
/// Directory description the model reads when choosing a provider. The Agent
/// directory shows at most 240 bytes.
pub const DESCRIPTION: &str = "Act in the user's own browser (signed in as them) and apps: open pages, read, click, type, pick options. Works in the background. Prefer a connector when one exists. Hand over with show before paying or signing in.";

/// Driver session label carried on every call, so read snapshots survive
/// between the short-lived `cua-driver call` processes.
pub const SESSION: &str = "grain";

/// The one Cua Driver release this build drives. Moving it is a deliberate
/// change that re-runs the Phase 3 acceptance tests.
pub const DRIVER_VERSION: &str = "0.52.0";

/// Environment set on every driver process Grain starts: telemetry, the
/// update check and the script tool off.
pub const DRIVER_ENV: &[(&str, &str)] = &[
    ("CUA_DRIVER_RS_TELEMETRY_ENABLED", "false"),
    ("DO_NOT_TRACK", "1"),
    ("CUA_DRIVER_DISABLE_RUN_SCRIPT", "1"),
    ("CUA_DRIVER_RS_UPDATE_CHECK", "false"),
];

/// Most characters one read hands the model (about 4k tokens).
pub const MAX_READ_CHARS: usize = 16_000;
/// Most characters a "what changed" reply hands the model.
pub const MAX_CHANGE_CHARS: usize = 4_000;
/// Longest text one `type` sends.
pub const MAX_TYPE_CHARS: usize = 5_000;
/// Longest option text, search phrase, app name or other short field.
pub const MAX_FIELD_CHARS: usize = 300;
/// Longest note `show` passes on for the user.
pub const MAX_SHOW_MESSAGE_CHARS: usize = 200;
/// Most actions one `steps` call runs.
pub const MAX_STEPS: usize = 8;
/// Largest scroll, in wheel notches.
pub const MAX_SCROLL_AMOUNT: u32 = 20;
/// How long the settle-read after an action may wait for the page to stop
/// changing.
pub const SETTLE_MS: u64 = 3_000;

/// Bump when the tool set or a policy here changes meaning.
const POLICY_VERSION: u32 = 1;

/// Content digest of the tool set and policy, the compiled equivalent of an
/// MCP manifest digest. Changes only with a new build.
pub fn catalog_digest() -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(POLICY_VERSION.to_le_bytes());
    hasher.update(DRIVER_VERSION.as_bytes());
    hasher.update([0]);
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
    fn driver_env_turns_off_telemetry_updates_and_scripts() {
        let has = |key: &str, value: &str| DRIVER_ENV.contains(&(key, value));
        assert!(has("CUA_DRIVER_RS_TELEMETRY_ENABLED", "false"));
        assert!(has("CUA_DRIVER_RS_UPDATE_CHECK", "false"));
        assert!(has("CUA_DRIVER_DISABLE_RUN_SCRIPT", "1"));
    }

    #[test]
    fn clip_marks_the_cut() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("abcdef", 3), "abc…");
    }
}
