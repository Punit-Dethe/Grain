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
pub const DESCRIPTION: &str = "Do things in the user's own browser (signed in as them) and apps: book, order, fill forms, change settings. Never to look things up or research: use web search. Prefer a connector. Hand over with show before paying.";

/// Driver session label carried on every call, so read snapshots survive
/// between the short-lived `cua-driver call` processes.
pub const SESSION: &str = "grain";

/// The one Cua Driver release this build drives. Moving it is a deliberate
/// change that re-runs the Phase 3 acceptance tests.
pub const DRIVER_VERSION: &str = "0.52.0";

/// GitHub release tag of [`DRIVER_VERSION`].
const DRIVER_RELEASE_TAG: &str = "cua-driver-rs-v0.52.0";

/// The pinned release download for one platform: its file name and the
/// SHA-256 Grain checks it against before unpacking anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverAsset {
    pub file: &'static str,
    pub sha256: &'static str,
}

impl DriverAsset {
    /// Where the asset is downloaded from.
    pub fn url(&self) -> String {
        format!(
            "https://github.com/trycua/cua/releases/download/{DRIVER_RELEASE_TAG}/{}",
            self.file
        )
    }
}

/// The driver download for an OS and CPU (`std::env::consts` names), or
/// `None` where Pilot does not run yet (macOS and Linux this round).
pub fn driver_asset(os: &str, arch: &str) -> Option<DriverAsset> {
    match (os, arch) {
        ("windows", "x86_64") => Some(DriverAsset {
            file: "cua-driver-rs-0.52.0-windows-x86_64-binary.zip",
            sha256: "788178cbdcce4de0121f6d04ef6cb5e9d512503371e60a70563aa6f69cdd50af",
        }),
        ("windows", "aarch64") => Some(DriverAsset {
            file: "cua-driver-rs-0.52.0-windows-arm64-binary.zip",
            sha256: "73e0a7daae34b81a15aa16d3827cb44b975254130906f99dfcd49ea4c518d40c",
        }),
        _ => None,
    }
}

/// The files Grain unpacks from the download: the driver and its UI
/// Automation helper. The SDK library, Node runtime and cursor theme tool are
/// left out.
pub const DRIVER_FILES: &[&str] = &["cua-driver.exe", "cua-driver-uia.exe"];
/// Largest download Grain accepts (the Windows asset is ~32 MB).
pub const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
/// Largest single file Grain unpacks from it (the driver is ~33 MB).
pub const MAX_UNPACKED_FILE_BYTES: u64 = 96 * 1024 * 1024;

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
    fn driver_downloads_are_pinned_per_platform() {
        let asset = driver_asset("windows", "x86_64").expect("windows x64");
        assert_eq!(asset.sha256.len(), 64);
        assert!(asset
            .url()
            .starts_with("https://github.com/trycua/cua/releases/download/cua-driver-rs-v0.52.0/"));
        assert!(asset.url().ends_with(asset.file));
        assert!(asset.file.contains(DRIVER_VERSION));
        assert!(driver_asset("windows", "aarch64").is_some());
        assert!(driver_asset("macos", "aarch64").is_none());
        assert!(driver_asset("linux", "x86_64").is_none());
    }

    #[test]
    fn clip_marks_the_cut() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("abcdef", 3), "abc…");
    }
}
