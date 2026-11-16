//! The capability manifest: the driver-side ceiling on what Pilot may do.
//!
//! The daemon runs in bounded mode with this manifest, so even a bug in Grain
//! cannot make the driver take a screenshot, read the clipboard, run a script,
//! kill a process or touch an app that is not listed. Format (checked against
//! driver 0.52.0): YAML, version 3, no byte-order mark, unknown keys rejected,
//! `expires_after` and `idle_timeout` required in bounded mode, apps by exact
//! absolute executable path (or AUMID for a packaged app).

use std::fmt::Write as _;

/// Driver tools Pilot uses. Everything else is denied or undeclared.
pub const ALLOWED_TOOLS: &[&str] = &[
    "launch_app",
    "list_windows",
    "get_window_state",
    "click",
    "type_text",
    "press_key",
    "scroll",
    "run_steps",
    "bring_to_front",
    "start_session",
    "end_session",
];

/// Driver tools named as denied, so a later driver default cannot widen them.
pub const DENIED_TOOLS: &[&str] = &[
    "run_script",
    "kill_app",
    "clipboard_read",
    "clipboard_write",
    "get_desktop_state",
    "zoom",
    "start_recording",
    "stop_recording",
    "replay_trajectory",
    "browser_prepare",
    "browser_navigate",
    "browser_click",
    "browser_type",
    "browser_download",
    "browser_set_input_files",
    "set_config",
    "install_extension",
    "move_cursor",
    "drag",
    "hotkey",
    "set_value",
];

/// How long a daemon's manifest stays valid, and how long it may sit idle.
/// Pilot stops the daemon long before either.
pub const EXPIRES_AFTER: &str = "4h";
pub const IDLE_TIMEOUT: &str = "30m";

/// One app the driver may launch, read and act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum App {
    /// A desktop app by its absolute executable path.
    Executable(String),
    /// A packaged (Store) app by its AUMID, e.g. `Microsoft.WindowsNotepad_8wekyb3d8bbwe!App`.
    Packaged(String),
}

/// The manifest text for `apps`.
pub fn manifest_yaml(apps: &[App]) -> Result<String, String> {
    if apps.is_empty() {
        return Err("the manifest needs at least one app".into());
    }
    let mut out = String::new();
    let _ = writeln!(out, "version: 3");
    let _ = writeln!(out, "expires_after: {EXPIRES_AFTER}");
    let _ = writeln!(out, "idle_timeout: {IDLE_TIMEOUT}");
    let _ = writeln!(out, "resources:");
    let _ = writeln!(out, "  apps:");
    for app in apps {
        let line = match app {
            App::Executable(path) => format!("executable: {}", yaml_string(&checked_path(path)?)),
            App::Packaged(aumid) => format!("bundle_id: {}", yaml_string(&checked_aumid(aumid)?)),
        };
        let _ = writeln!(out, "    - {line}");
        let _ = writeln!(out, "      launch: true");
        let _ = writeln!(out, "      windows: all");
        let _ = writeln!(out, "      terminate: deny");
    }
    let _ = writeln!(out, "allow:");
    let _ = writeln!(out, "  tools: [{}]", ALLOWED_TOOLS.join(", "));
    let _ = writeln!(out, "deny:");
    let _ = writeln!(out, "  tools: [{}]", DENIED_TOOLS.join(", "));
    Ok(out)
}

fn checked_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    let absolute =
        path.len() > 3 && path.as_bytes()[0].is_ascii_alphabetic() && &path[1..3] == ":\\";
    if !absolute || path.contains("..") || path.chars().any(char::is_control) {
        return Err(format!(
            "`{}` is not an absolute executable path",
            super::clip(path, 120)
        ));
    }
    if !path.to_ascii_lowercase().ends_with(".exe") {
        return Err("only .exe apps can be listed".into());
    }
    if super::policy::is_refused_app(path) {
        return Err("Grain Pilot never drives shells, terminals or Grain itself".into());
    }
    Ok(path.to_string())
}

fn checked_aumid(aumid: &str) -> Result<String, String> {
    let aumid = aumid.trim();
    let ok = aumid.contains('!')
        && aumid.len() <= 200
        && aumid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '!'));
    if ok {
        Ok(aumid.to_string())
    } else {
        Err(format!(
            "`{}` is not a packaged app id",
            super::clip(aumid, 80)
        ))
    }
}

/// A single-quoted YAML scalar (a quote is written twice).
fn yaml_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_matches_the_format_the_driver_accepted() {
        let yaml = manifest_yaml(&[
            App::Executable(r"C:\Program Files\Zen Browser\zen.exe".into()),
            App::Packaged("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App".into()),
        ])
        .unwrap();
        assert!(yaml.starts_with("version: 3\nexpires_after: 4h\nidle_timeout: 30m\n"));
        assert!(yaml.contains("    - executable: 'C:\\Program Files\\Zen Browser\\zen.exe'\n      launch: true\n      windows: all\n      terminate: deny\n"));
        assert!(yaml.contains("bundle_id: 'Microsoft.WindowsNotepad_8wekyb3d8bbwe!App'"));
        assert!(!yaml.starts_with('\u{feff}'));
        assert!(!yaml.contains("display"), "never grant whole-screen access");
    }

    #[test]
    fn allow_and_deny_never_overlap_and_risky_tools_are_denied() {
        for tool in ALLOWED_TOOLS {
            assert!(!DENIED_TOOLS.contains(tool), "{tool}");
        }
        for risky in [
            "run_script",
            "kill_app",
            "clipboard_read",
            "get_desktop_state",
            "browser_prepare",
        ] {
            assert!(DENIED_TOOLS.contains(&risky), "{risky}");
        }
    }

    #[test]
    fn only_absolute_exe_paths_and_real_aumids() {
        assert!(manifest_yaml(&[]).is_err());
        assert!(manifest_yaml(&[App::Executable("zen.exe".into())]).is_err());
        assert!(
            manifest_yaml(&[App::Executable(r"C:\x\..\Windows\System32\cmd.exe".into())]).is_err()
        );
        assert!(manifest_yaml(&[App::Executable(r"C:\Windows\System32\cmd.exe".into())]).is_err());
        assert!(manifest_yaml(&[App::Executable("C:\\a\nb.exe".into())]).is_err());
        assert!(manifest_yaml(&[App::Packaged("NoBang".into())]).is_err());
        let quoted = manifest_yaml(&[App::Executable(r"C:\Apps\O'Brien\app.exe".into())]).unwrap();
        assert!(quoted.contains(r"'C:\Apps\O''Brien\app.exe'"));
    }
}
