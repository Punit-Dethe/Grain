//! What Pilot refuses, decided before anything reaches the driver.
//!
//! - Only `http(s)` links that pass [`crate::links::validate_link`] are opened.
//! - Typed text and short fields are bounded and may not carry control
//!   characters.
//! - `press` takes a closed list of keys.
//! - Payment controls are never clicked: the Agent hands the window to the
//!   user with `show` instead.
//! - Terminals, shells and Grain's own windows are never driven.
//! - Text is never typed into a password field. The driver gives no password
//!   flag, so the desktop crate asks Windows UI Automation for `IsPassword`
//!   rectangles and [`is_password_target`] decides.

use super::{clip, MAX_FIELD_CHARS, MAX_SHOW_MESSAGE_CHARS, MAX_TYPE_CHARS};

/// Keys `press` accepts, in the driver's key names.
pub const KEYS: &[&str] = &[
    "return",
    "tab",
    "escape",
    "backspace",
    "delete",
    "space",
    "up",
    "down",
    "left",
    "right",
    "pageup",
    "pagedown",
    "home",
    "end",
];

/// A web link Pilot may open in the user's browser.
pub fn web_link(raw: &str) -> Result<String, String> {
    let url = crate::links::validate_link(raw)?;
    if !crate::links::is_web(&url) {
        return Err("Grain Pilot only opens https and http pages".into());
    }
    Ok(url)
}

/// Text to type into a field: bounded, no control characters except a line
/// break or tab.
pub fn typed_text(value: &str) -> Result<String, String> {
    bounded(value, MAX_TYPE_CHARS, "the text", true)
}

/// A short single-line value: an option, a search phrase, an app name.
pub fn short_field(value: &str, what: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{what} is empty"));
    }
    bounded(value, MAX_FIELD_CHARS, what, false)
}

/// The one line `show` passes on for the user.
pub fn show_message(value: &str) -> Result<String, String> {
    bounded(
        &value.trim().replace(['\n', '\t'], " "),
        MAX_SHOW_MESSAGE_CHARS,
        "the message",
        false,
    )
}

fn bounded(value: &str, max_chars: usize, what: &str, multiline: bool) -> Result<String, String> {
    if value.chars().count() > max_chars {
        return Err(format!("{what} is longer than {max_chars} characters"));
    }
    let allowed = |c: char| multiline && (c == '\n' || c == '\t');
    if value.chars().any(|c| c.is_control() && !allowed(c)) {
        return Err(format!("{what} contains control characters"));
    }
    Ok(value.to_string())
}

/// A key from [`KEYS`], accepting a few common spellings.
pub fn key(raw: &str) -> Result<&'static str, String> {
    let lower = raw.trim().to_ascii_lowercase().replace([' ', '_', '-'], "");
    let name = match lower.as_str() {
        "enter" => "return",
        "esc" => "escape",
        "arrowup" => "up",
        "arrowdown" => "down",
        "arrowleft" => "left",
        "arrowright" => "right",
        "pgup" => "pageup",
        "pgdn" | "pgdown" => "pagedown",
        other => other,
    };
    KEYS.iter()
        .copied()
        .find(|key| *key == name)
        .ok_or_else(|| format!("`{}` is not a key Grain Pilot presses", clip(raw, 40)))
}

/// Words on a control that commit money. Pilot never clicks such a control;
/// the user does, after `show`. Stopping one step early (a "Pay" button that
/// only leads to the payment page) is the safe side.
const PAYMENT_WORDS: &[&str] = &[
    "pay",
    "pay now",
    "place order",
    "place your order",
    "buy now",
    "purchase",
    "complete purchase",
    "complete order",
    "confirm payment",
    "confirm order",
    "confirm and pay",
    "proceed to pay",
    "proceed to payment",
    "make payment",
    "checkout and pay",
];

/// Whether a control's name commits money, by whole words ("Pay ₹ 340" and
/// "Place order" do; "Paytm wallet" and "Display" do not).
pub fn is_payment_control(name: &str) -> bool {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    PAYMENT_WORDS.iter().any(|phrase| {
        let phrase: Vec<&str> = phrase.split(' ').collect();
        words
            .windows(phrase.len())
            .any(|window| window.iter().map(String::as_str).eq(phrase.iter().copied()))
    })
}

/// Executables Pilot never opens, reads or types into: shells and terminals
/// (typing there runs commands) and Grain itself.
const REFUSED_EXECUTABLES: &[&str] = &[
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "windowsterminal.exe",
    "wt.exe",
    "conhost.exe",
    "openconsole.exe",
    "bash.exe",
    "wsl.exe",
    "mintty.exe",
    "alacritty.exe",
    "wezterm-gui.exe",
    "grain.exe",
    "regedit.exe",
    "mmc.exe",
];

/// Whether an app (by executable path or file name) is one Pilot refuses.
pub fn is_refused_app(executable: &str) -> bool {
    let name = executable
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(executable)
        .trim()
        .to_ascii_lowercase();
    REFUSED_EXECUTABLES.contains(&name.as_str())
}

/// Packaged (Store) apps Pilot refuses, by words in their AUMID: Windows
/// Terminal, PowerShell and other shells ship as packages too.
const REFUSED_PACKAGE_WORDS: &[&str] = &["terminal", "powershell", "wsl", "ubuntu", "debian"];

/// Whether a packaged app (by AUMID) is one Pilot refuses.
pub fn is_refused_packaged(aumid: &str) -> bool {
    let lower = aumid.to_ascii_lowercase();
    REFUSED_PACKAGE_WORDS
        .iter()
        .any(|word| lower.contains(word))
}

/// A rectangle in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Whether the control at `target` is one of the password fields Windows UI
/// Automation reported for the window. The driver's frame and UIA's rectangle
/// matched exactly in testing; a small tolerance covers rounding.
pub fn is_password_target(target: Rect, password_fields: &[Rect]) -> bool {
    const TOLERANCE: f64 = 2.0;
    password_fields.iter().any(|field| {
        (field.x - target.x).abs() <= TOLERANCE
            && (field.y - target.y).abs() <= TOLERANCE
            && (field.w - target.w).abs() <= TOLERANCE
            && (field.h - target.h).abs() <= TOLERANCE
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_web_only() {
        assert!(web_link("https://in.bookmyshow.com/").is_ok());
        assert!(web_link("file:///C:/Windows/win.ini").is_err());
        assert!(web_link("javascript:alert(1)").is_err());
    }

    #[test]
    fn text_limits_and_control_characters() {
        assert!(typed_text("line one\nline two").is_ok());
        assert!(typed_text("bell\u{7}").is_err());
        assert!(typed_text(&"x".repeat(MAX_TYPE_CHARS + 1)).is_err());
        assert!(short_field("  ", "the option").is_err());
        assert!(short_field("two\nlines", "the option").is_err());
        assert_eq!(show_message("Sign in\tplease").unwrap(), "Sign in please");
    }

    #[test]
    fn keys_are_a_closed_list() {
        assert_eq!(key("Enter").unwrap(), "return");
        assert_eq!(key("ArrowDown").unwrap(), "down");
        assert_eq!(key("page down").unwrap(), "pagedown");
        assert!(key("ctrl+w").is_err());
        assert!(key("f5").is_err());
    }

    #[test]
    fn payment_controls_are_recognised_by_whole_words() {
        for name in [
            "Pay ₹ 340",
            "Place order",
            "BUY NOW",
            "Proceed to Payment",
            "Confirm & pay",
        ] {
            assert!(is_payment_control(name), "{name}");
        }
        assert!(is_payment_control("Confirm and pay"));
        for name in [
            "Paytm wallet",
            "Display",
            "Select Seats",
            "Book 10:15 PM",
            "Accept",
            "Payment options info",
        ] {
            assert!(!is_payment_control(name), "{name}");
        }
    }

    #[test]
    fn shells_terminals_and_grain_are_refused() {
        assert!(is_refused_app(r"C:\Windows\System32\cmd.exe"));
        assert!(is_refused_app("WindowsTerminal.exe"));
        assert!(is_refused_app("C:/Program Files/Grain/grain.exe"));
        assert!(!is_refused_app(r"C:\Program Files\Zen Browser\zen.exe"));
        assert!(is_refused_packaged(
            "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"
        ));
        assert!(!is_refused_packaged(
            "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"
        ));
    }

    #[test]
    fn password_target_matches_by_rectangle() {
        let field = Rect {
            x: 595.0,
            y: 405.0,
            w: 469.0,
            h: 32.0,
        };
        assert!(is_password_target(field, &[field]));
        let username = Rect { y: 342.0, ..field };
        assert!(!is_password_target(username, &[field]));
        assert!(!is_password_target(field, &[]));
    }
}
