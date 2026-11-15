//! Link policy for links Grain hands to the OS's default handler.
//!
//! A link the model or a saved record supplies must pass [`validate_link`]: a
//! fixed allowlist of schemes, with protocol handlers abused in real attacks
//! refused outright. Reminders uses it for its "Open" links; the Agent Browser
//! uses it (web links only) before navigating. Pure: no I/O.

/// Longest link Grain will open.
const MAX_LINK_BYTES: usize = 8 * 1024;

/// Schemes Grain opens. Each is a navigation or prefill handler of a mainstream
/// app; none runs code or fetches a remote document into a privileged viewer.
const ALLOWED_SCHEMES: &[&str] = &[
    "https",
    "http",
    "mailto",
    "tel",
    "sms",
    // Windows built-ins.
    "ms-settings",
    "ms-windows-store",
    "ms-clock",
    "ms-screenclip",
    "ms-screensketch",
    "calculator",
    "microsoft-edge",
    // macOS built-ins.
    "x-apple.systempreferences",
    "facetime",
    "maps",
    "music",
    // Apps.
    "spotify",
    "whatsapp",
    "tg",
    "msteams",
    "zoommtg",
    "zoomus",
    "slack",
    "discord",
    "signal",
    "sgnl",
    "skype",
    "vscode",
    "vscode-insiders",
    "cursor",
    "zed",
    "obsidian",
    "notion",
    "figma",
    "linear",
    "todoist",
    "things",
    "steam",
    "claude",
];

/// Handlers abused in real attacks or able to fetch and run remote content.
/// They are refused even if a future allowlist edit would admit them.
const REFUSED_SCHEMES: &[&str] = &[
    "file",
    "javascript",
    "vbscript",
    "data",
    "blob",
    "shell",
    "ms-msdt",
    "search-ms",
    "search",
    "ms-search",
    "ms-officecmd",
    "ms-appinstaller",
    "ms-word",
    "ms-excel",
    "ms-powerpoint",
    "ms-visio",
    "ms-access",
    "ms-publisher",
    "ms-project",
    "ms-infopath",
    "ms-spd",
    "ms-cxh",
    "ms-cxh-full",
    "hcp",
    "its",
    "ms-its",
    "mk",
    "res",
    "jnlp",
    "ldap",
    "ldaps",
    "smb",
    "nfs",
    "webdav",
    "dav",
    "ftp",
    "telnet",
    "ssh",
];

/// Validate a link the model supplied. Returns the normalised link.
pub fn validate_link(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("the link is empty".into());
    }
    if raw.len() > MAX_LINK_BYTES {
        return Err("the link is too long".into());
    }
    if raw.chars().any(char::is_control) {
        return Err("the link contains control characters".into());
    }
    if raw.starts_with("\\\\") || raw.starts_with("//") {
        return Err("network paths are not links Grain opens".into());
    }
    let Some((scheme, rest)) = raw.split_once(':') else {
        return Err("the link needs a scheme such as https://".into());
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return Err("the link needs a scheme such as https://".into());
    }
    if REFUSED_SCHEMES.contains(&scheme.as_str()) {
        return Err(format!("`{scheme}:` links are refused for safety"));
    }
    if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
        return Err(format!(
            "Grain does not open `{scheme}:` links. Use https, mailto, tel, sms or a supported app link."
        ));
    }
    // A backslash would let a handler reinterpret the remainder as a UNC path.
    if rest.contains('\\') {
        return Err("the link contains a backslash".into());
    }
    if scheme == "http" || scheme == "https" {
        let parsed = url::Url::parse(raw)
            .map_err(|error| format!("the link is not a valid URL: {error}"))?;
        if parsed.host_str().is_none_or(str::is_empty) {
            return Err("the web link has no host".into());
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("links with embedded credentials are refused".into());
        }
        return Ok(parsed.to_string());
    }
    if scheme == "microsoft-edge" {
        // Only `microsoft-edge:https://…`, which opens a web page in Edge.
        let inner = rest.trim_start_matches('/');
        let inner_scheme = inner.split(':').next().unwrap_or("").to_ascii_lowercase();
        if inner_scheme != "https" && inner_scheme != "http" {
            return Err("microsoft-edge: links must wrap an https address".into());
        }
        validate_link(inner)?;
    }
    Ok(format!("{scheme}:{rest}"))
}

/// Whether a validated link is a web page (the only kind a browser override applies to).
pub fn is_web(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_and_known_app_links_open_and_dangerous_handlers_are_refused() {
        for ok in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "http://example.com",
            "mailto:sam@example.com?subject=Hi",
            "tel:+15551234567",
            "spotify:search:daft%20punk",
            "ms-settings:bluetooth",
            "zoommtg://zoom.us/join?confno=123",
            "MS-SETTINGS:display",
            "microsoft-edge:https://example.com",
        ] {
            assert!(validate_link(ok).is_ok(), "{ok} should open");
        }
        for bad in [
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "data:text/html,hi",
            "ms-msdt:/id PCWDiagnostic",
            "search-ms:query=x&crumb=location:\\\\evil\\share",
            "ms-officecmd:{}",
            "ms-appinstaller:?source=https://evil/x.msix",
            "ms-word:ofe|u|https://evil/doc.docx",
            "\\\\evil\\share\\x.exe",
            "//evil/share",
            "https://user:pass@example.com",
            "https://",
            "unknownapp://do-something",
            "C:\\Windows\\notepad.exe",
            "spotify:search:x\\y",
            "microsoft-edge:file:///C:/x",
            "https://example.com/\u{0}",
        ] {
            assert!(validate_link(bad).is_err(), "{bad} should be refused");
        }
    }
}
