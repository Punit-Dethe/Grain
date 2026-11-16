//! The exact driver call for each step Pilot takes.
//!
//! Every call carries Grain's driver session and never asks for a screenshot.
//! Model values only ever fill JSON fields that were validated first; the
//! driver tool name is always one of the constants here.

use serde_json::{json, Value};

use super::SESSION;

/// A window the driver can address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    pub pid: i64,
    pub window_id: u64,
}

/// One driver tool call: the tool name and its JSON arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct DriverCall {
    pub tool: &'static str,
    pub args: Value,
}

impl DriverCall {
    fn new(tool: &'static str, mut args: Value) -> Self {
        if let Some(object) = args.as_object_mut() {
            object.insert("session".into(), json!(SESSION));
        }
        Self { tool, args }
    }

    /// The JSON written to the driver's stdin.
    pub fn stdin(&self) -> String {
        self.args.to_string()
    }
}

fn window(target: Target) -> Value {
    json!({"pid": target.pid, "window_id": target.window_id})
}

fn with(target: Target, extra: Value) -> Value {
    let mut args = window(target);
    if let (Some(object), Some(extra)) = (args.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            object.insert(key.clone(), value.clone());
        }
    }
    args
}

/// A driver snapshot id as the driver writes it (`s0000002a`).
pub fn valid_snapshot(snapshot: &str) -> bool {
    snapshot.len() <= 32
        && snapshot.starts_with('s')
        && snapshot[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn token(snapshot: &str, row: u32) -> String {
    format!("{snapshot}:{row}")
}

/// Read a window as the markdown tree compaction expects.
pub fn read(target: Target) -> DriverCall {
    DriverCall::new(
        "get_window_state",
        with(
            target,
            json!({"include_screenshot": false, "tree_format": "markdown"}),
        ),
    )
}

/// Read a window as structured elements, for control rectangles (the
/// password check compares them with Windows UI Automation's).
pub fn read_elements(target: Target) -> DriverCall {
    DriverCall::new(
        "get_window_state",
        with(
            target,
            json!({"include_screenshot": false, "tree_format": "elements"}),
        ),
    )
}

/// Click a row of the latest read. On a drop-down the driver expands it.
pub fn click(target: Target, snapshot: &str, row: u32) -> DriverCall {
    DriverCall::new(
        "click",
        with(target, json!({"element_token": token(snapshot, row)})),
    )
}

/// Type into a row of the latest read (UIA value write on that element).
pub fn type_into(target: Target, snapshot: &str, row: u32, text: &str) -> DriverCall {
    DriverCall::new(
        "type_text",
        with(
            target,
            json!({"element_token": token(snapshot, row), "text": text}),
        ),
    )
}

/// Press one key in the window.
pub fn press(target: Target, key: &'static str) -> DriverCall {
    DriverCall::new("press_key", with(target, json!({"key": key})))
}

/// Scroll the window.
pub fn scroll(target: Target, direction: &'static str, amount: u32) -> DriverCall {
    DriverCall::new(
        "scroll",
        with(target, json!({"direction": direction, "amount": amount})),
    )
}

/// Several actions in one driver call, stopping at the first failure.
pub fn steps(target: Target, calls: &[DriverCall]) -> DriverCall {
    let steps: Vec<Value> = calls
        .iter()
        .map(|call| {
            let mut args = call.args.clone();
            if let Some(object) = args.as_object_mut() {
                for key in ["session", "pid", "window_id"] {
                    object.remove(key);
                }
            }
            json!({"tool": call.tool, "args": args})
        })
        .collect();
    DriverCall::new("run_steps", with(target, json!({"steps": steps})))
}

/// List one app's windows. Never called without a pid: a whole-desktop list
/// counts as watching the screen and is outside Pilot's manifest.
pub fn list_windows(pid: i64) -> DriverCall {
    DriverCall::new("list_windows", json!({"pid": pid}))
}

/// Start an app by its absolute executable path, with fixed arguments.
pub fn launch(executable: &str, arguments: &[String]) -> DriverCall {
    DriverCall::new(
        "launch_app",
        json!({"path": executable.replace('\\', "/"), "additional_arguments": arguments}),
    )
}

/// Start a packaged (Store) app by its AUMID. Windows 11's Notepad,
/// Calculator and Paint are packaged; their System32 `.exe` is a stub.
pub fn launch_packaged(aumid: &str) -> DriverCall {
    DriverCall::new("launch_app", json!({"aumid": aumid}))
}

/// Bring a window to the front for the user (`show`).
pub fn bring_to_front(target: Target) -> DriverCall {
    DriverCall::new("bring_to_front", window(target))
}

// ── Browsers ───────────────────────────────────────────────────────────────

/// How a browser opens Pilot's window and what it allows in the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserFamily {
    /// Zen syncs tabs across windows; Pilot opens an unsynced window.
    Zen,
    /// Firefox and its other forks.
    Firefox,
    /// Chrome, Edge, Brave and other Chromium browsers.
    Chromium,
}

/// The family of a browser executable, or `None` for one Pilot does not
/// drive.
pub fn browser_family(executable: &str) -> Option<BrowserFamily> {
    let name = executable
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(executable)
        .to_ascii_lowercase();
    Some(match name.as_str() {
        "zen.exe" => BrowserFamily::Zen,
        "firefox.exe" | "librewolf.exe" | "floorp.exe" | "waterfox.exe" | "mullvadbrowser.exe" => {
            BrowserFamily::Firefox
        }
        "msedge.exe" | "chrome.exe" | "brave.exe" | "vivaldi.exe" | "opera.exe"
        | "chromium.exe" => BrowserFamily::Chromium,
        _ => return None,
    })
}

/// Command-line arguments that open `url` in a new window of its own.
/// Zen's `--blank-window` opens an unsynced window, so Pilot's tab never
/// appears in the user's synced windows.
pub fn open_arguments(family: BrowserFamily, url: &str) -> Vec<String> {
    let flag = match family {
        BrowserFamily::Zen => "--blank-window",
        BrowserFamily::Firefox | BrowserFamily::Chromium => "--new-window",
    };
    vec![flag.to_string(), url.to_string()]
}

impl BrowserFamily {
    /// Whether text can be typed into page fields while the window stays in
    /// the background. Chromium refuses background keystrokes and ignores a
    /// value write on web fields (checked 2026-11-16).
    pub fn types_in_background(self) -> bool {
        !matches!(self, BrowserFamily::Chromium)
    }
}

// ── Drop-downs ─────────────────────────────────────────────────────────────

/// The option `wanted` names: an exact match (ignoring case) first, then the
/// one option that contains it.
pub fn choose_target(options: &[String], wanted: &str) -> Result<usize, String> {
    let wanted = wanted.trim().to_lowercase();
    if let Some(at) = options
        .iter()
        .position(|o| o.trim().to_lowercase() == wanted)
    {
        return Ok(at);
    }
    let partial: Vec<usize> = options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.to_lowercase().contains(&wanted))
        .map(|(at, _)| at)
        .collect();
    match partial.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!(
            "No option matches. The options are: {}",
            list_options(options)
        )),
        _ => Err(format!(
            "More than one option matches. Name one exactly: {}",
            list_options(partial.iter().map(|at| &options[*at]))
        )),
    }
}

fn list_options<'a>(options: impl IntoIterator<Item = &'a String>) -> String {
    let all: Vec<String> = options
        .into_iter()
        .take(30)
        .map(|o| format!("\"{}\"", super::clip(o, 60)))
        .collect();
    all.join(", ")
}

/// Most arrow presses one `choose` sends.
const MAX_OPTION_MOVES: usize = 80;

/// Keys that move an open drop-down from the selected option to `target` and
/// confirm it. The open list starts on the selected option, or the first.
pub fn choose_keys(current: Option<usize>, target: usize) -> Result<Vec<&'static str>, String> {
    let from = current.unwrap_or(0);
    let moves = from.abs_diff(target);
    if moves > MAX_OPTION_MOVES {
        return Err("That option is too far down the list to reach with the keyboard.".into());
    }
    let arrow = if target >= from { "down" } else { "up" };
    let mut keys = vec![arrow; moves];
    keys.push("return");
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Target = Target {
        pid: 16644,
        window_id: 132036,
    };

    #[test]
    fn every_call_carries_the_session_and_no_screenshot() {
        for call in [read(T), read_elements(T), click(T, "s0000000b", 42)] {
            assert_eq!(call.args["session"], SESSION);
            assert_ne!(call.args["include_screenshot"], true);
        }
        assert_eq!(read(T).args["include_screenshot"], false);
        assert_eq!(
            click(T, "s0000000b", 42).args["element_token"],
            "s0000000b:42"
        );
    }

    #[test]
    fn steps_nest_actions_without_their_window_fields() {
        let call = steps(T, &[click(T, "s1", 3), press(T, "return")]);
        assert_eq!(call.tool, "run_steps");
        assert_eq!(call.args["pid"], 16644);
        let first = &call.args["steps"][0];
        assert_eq!(first["tool"], "click");
        assert!(first["args"].get("pid").is_none());
        assert_eq!(call.args["steps"][1]["args"]["key"], "return");
    }

    #[test]
    fn launch_uses_forward_slashes_and_fixed_arguments() {
        let url = "https://in.bookmyshow.com/";
        let call = launch(
            r"C:\Program Files\Zen Browser\zen.exe",
            &open_arguments(BrowserFamily::Zen, url),
        );
        assert_eq!(call.args["path"], "C:/Program Files/Zen Browser/zen.exe");
        assert_eq!(
            call.args["additional_arguments"],
            json!(["--blank-window", url])
        );
        assert_eq!(list_windows(7).args["pid"], 7);
    }

    #[test]
    fn browser_families() {
        assert_eq!(
            browser_family(r"C:\Program Files\Zen Browser\zen.exe"),
            Some(BrowserFamily::Zen)
        );
        assert_eq!(browser_family("BRAVE.EXE"), Some(BrowserFamily::Chromium));
        assert_eq!(browser_family("firefox.exe"), Some(BrowserFamily::Firefox));
        assert_eq!(browser_family("notepad.exe"), None);
        assert!(!BrowserFamily::Chromium.types_in_background());
        assert!(BrowserFamily::Zen.types_in_background());
        assert_eq!(
            open_arguments(BrowserFamily::Chromium, "https://a.example/")[0],
            "--new-window"
        );
    }

    #[test]
    fn snapshot_ids_are_checked() {
        assert!(valid_snapshot("s0000002a"));
        assert!(!valid_snapshot("s00:1"));
        assert!(!valid_snapshot("x1"));
    }

    #[test]
    fn choosing_an_option_by_text() {
        let options: Vec<String> = [
            "Select Category",
            "NORMAL - ₹340.00",
            "EXECUTIVE - ₹380.00",
            "PREMIUM - ₹400.00",
            "PREMIUM XL - ₹440.00",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(choose_target(&options, "normal - ₹340.00"), Ok(1));
        assert_eq!(choose_target(&options, "EXECUTIVE"), Ok(2));
        assert!(choose_target(&options, "PREMIUM")
            .unwrap_err()
            .contains("More than one"));
        assert!(choose_target(&options, "GOLD")
            .unwrap_err()
            .contains("NORMAL"));
        assert_eq!(choose_keys(Some(0), 1).unwrap(), vec!["down", "return"]);
        assert_eq!(choose_keys(Some(3), 1).unwrap(), vec!["up", "up", "return"]);
        assert_eq!(choose_keys(None, 0).unwrap(), vec!["return"]);
        assert!(choose_keys(Some(0), 500).is_err());
    }
}
