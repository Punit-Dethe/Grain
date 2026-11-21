//! [GRAIN] Links in Agent replies. The model writes a link on its own line as
//! a Markdown link; the notch renders it as a button (title chosen by the
//! model, host shown beside it) and asks the host to open it here.
//!
//! The reply is model output, so the URL is untrusted: the host — not the
//! webview — decides what opens. Only plain web addresses reach the user's
//! default browser; the notch webview itself never navigates. The one
//! exception is Grain's own meeting link (`grain://recall/meetings/<id>`),
//! which opens that meeting in Grain's main window.

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Appended to the Agent's system prompt (see `agent::build_messages`).
pub(crate) const LINK_RULE: &str = "Your replies are shown in a panel that renders Markdown. When you give the user a link they may want to open, put it on its own line as a Markdown link whose text is a short action title (2 to 5 words) saying what opening it does, for example [Open the Notion page](https://www.notion.so/...). A link alone on its own line is shown as a button that opens in the user's browser. Never use the raw URL as the title. Only give real http or https addresses that came from the user or from a tool result; never invent or guess one. Grain's own grain://recall/meetings/<id> links are allowed too, for meeting ids a tool returned.";

/// [GRAIN] Whether `text` is nothing but link buttons: every line holds only
/// Markdown links to a web or Grain meeting address (as the notch's
/// `LINK_ONLY` renders them). With `partial`, a link still arriving at the end
/// counts too (a stream). Such text stays with a reply's spoken part instead
/// of becoming a message of its own.
pub(crate) fn only_link_buttons(text: &str, partial: bool) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    !lines.is_empty()
        && lines.iter().enumerate().all(|(n, line)| {
            let last = n + 1 == lines.len();
            links_only(line, partial && last)
        })
}

/// One line of only `[title](url)` links; `partial` = its last link may be
/// cut off.
fn links_only(mut line: &str, partial: bool) -> bool {
    const SCHEMES: [&str; 3] = ["https://", "http://", "grain://recall/"];
    while !line.is_empty() {
        let Some(rest) = line.strip_prefix('[') else {
            return false;
        };
        let Some(mid) = rest.find("](") else {
            return partial && !rest.contains(['[', ']']);
        };
        if mid == 0 || rest[..mid].contains(['[', ']']) {
            return false;
        }
        let tail = &rest[mid + 2..];
        let end = tail.find(')');
        let url = &tail[..end.unwrap_or(tail.len())];
        let scheme_ok = SCHEMES.iter().any(|s| url.starts_with(s))
            || (end.is_none() && partial && SCHEMES.iter().any(|s| s.starts_with(url)));
        if !scheme_ok || url.contains(char::is_whitespace) {
            return false;
        }
        let Some(end) = end else {
            return partial;
        };
        line = tail[end + 1..].trim_start();
    }
    true
}

const MAX_URL: usize = 4096;

/// Grain's meeting link; the same address the Recall MCP server uses.
const MEETING_LINK: &str = "grain://recall/meetings/";

/// The meeting id of a Grain meeting link (an optional `/transcript` or `/`
/// tail is ignored), or `None` for any other link.
fn meeting_link(raw: &str) -> Option<String> {
    let rest = raw.trim().strip_prefix(MEETING_LINK)?;
    let id = rest
        .strip_suffix("/transcript")
        .or_else(|| rest.strip_suffix('/'))
        .unwrap_or(rest);
    grain_recall::manifest::parse_session_id(id).ok()?;
    Some(id.to_string())
}

/// The normalised URL to open, or why it is refused.
fn openable(raw: &str) -> Result<String, &'static str> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > MAX_URL {
        return Err("That link can't be opened");
    }
    let url = reqwest::Url::parse(raw).map_err(|_| "That link can't be opened")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only web links can be opened");
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("That link can't be opened");
    }
    // `https://bank.com@evil.example` reads as one site and opens another.
    if !url.username().is_empty() || url.password().is_some() {
        return Err("That link can't be opened");
    }
    Ok(url.into())
}

/// Open a link from an Agent reply: a Grain meeting link in Grain, a web link
/// in the default browser.
#[tauri::command]
#[specta::specta]
pub fn agent_open_link(app: AppHandle, url: String) -> Result<(), String> {
    if let Some(id) = meeting_link(&url) {
        crate::grain_app_route::open(&app, format!("/meet/{id}"));
        return Ok(());
    }
    let url = openable(&url)?;
    app.opener().open_url(&url, None::<&str>).map_err(|error| {
        log::warn!("[GRAIN] agent: could not open a link: {error}");
        "Could not open the link in your browser".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::{meeting_link, only_link_buttons, openable};

    #[test]
    fn link_only_text_is_recognised() {
        assert!(only_link_buttons("[Open the page](https://x.dev/a)", false));
        assert!(only_link_buttons(
            "[Open it](https://x.dev) [Docs](http://y.dev)\n\n[Meeting](grain://recall/meetings/7)",
            false
        ));
        for not in [
            "",
            "See [the page](https://x.dev).",
            "[Open](ftp://x.dev)",
            "[](https://x.dev)",
            "[Open](https://x.dev)\nMore text",
        ] {
            assert!(!only_link_buttons(not, false), "{not}");
        }
        // Streaming: a link still arriving at the end counts, only there.
        for arriving in ["[Op", "[Open the page](htt", "[Open](https://x.d"] {
            assert!(only_link_buttons(arriving, true), "{arriving}");
            assert!(!only_link_buttons(arriving, false), "{arriving}");
        }
        assert!(!only_link_buttons("[Open](https://x.dev)\nThe", true));
    }

    #[test]
    fn only_well_formed_meeting_links_open_in_grain() {
        let id = "0b6f3c1e-8d2a-4c5b-9e7f-1a2b3c4d5e6f";
        for link in [
            format!("grain://recall/meetings/{id}"),
            format!(" grain://recall/meetings/{id}/transcript"),
            format!("grain://recall/meetings/{id}/"),
        ] {
            assert_eq!(meeting_link(&link).as_deref(), Some(id), "{link}");
        }
        for refused in [
            "grain://recall/meetings/../settings",
            "grain://recall/meetings/NOT-A-UUID",
            "grain://recall/meetings/0B6F3C1E-8D2A-4C5B-9E7F-1A2B3C4D5E6F",
            "grain://other/0b6f3c1e-8d2a-4c5b-9e7f-1a2b3c4d5e6f",
            "https://example.com",
        ] {
            assert!(meeting_link(refused).is_none(), "{refused}");
        }
        assert!(openable(&format!("grain://recall/meetings/{id}")).is_err());
    }

    #[test]
    fn only_plain_web_links_open() {
        assert_eq!(
            openable(" https://www.notion.so/page?x=1 ").unwrap(),
            "https://www.notion.so/page?x=1"
        );
        assert!(openable("http://example.com").is_ok());
        for refused in [
            "",
            "javascript:alert(1)",
            "file:///C:/Windows/System32/calc.exe",
            "ms-settings:privacy",
            "data:text/html,hi",
            "https://bank.com@evil.example/",
            "https://user:pw@example.com/",
            "notaurl",
        ] {
            assert!(openable(refused).is_err(), "{refused}");
        }
        assert!(openable(&format!("https://e.com/{}", "a".repeat(5000))).is_err());
    }
}
