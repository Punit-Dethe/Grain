//! [GRAIN] Links in Agent replies. The model writes a link on its own line as
//! a Markdown link; the notch renders it as a button (title chosen by the
//! model, host shown beside it) and asks the host to open it here.
//!
//! The reply is model output, so the URL is untrusted: the host — not the
//! webview — decides what opens. Only plain web addresses reach the user's
//! default browser; the notch webview itself never navigates.

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Appended to the Agent's system prompt (see `agent::build_messages`).
pub(crate) const LINK_RULE: &str = "Your replies are shown in a panel that renders Markdown. When you give the user a link they may want to open, put it on its own line as a Markdown link whose text is a short action title (2 to 5 words) saying what opening it does, for example [Open the Notion page](https://www.notion.so/...). A link alone on its own line is shown as a button that opens in the user's browser. Never use the raw URL as the title. Only give real http or https addresses that came from the user or from a tool result; never invent or guess one.";

const MAX_URL: usize = 4096;

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

/// Open a link from an Agent reply in the default browser.
#[tauri::command]
#[specta::specta]
pub fn agent_open_link(app: AppHandle, url: String) -> Result<(), String> {
    let url = openable(&url)?;
    app.opener().open_url(&url, None::<&str>).map_err(|error| {
        log::warn!("[GRAIN] agent: could not open a link: {error}");
        "Could not open the link in your browser".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::openable;

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
