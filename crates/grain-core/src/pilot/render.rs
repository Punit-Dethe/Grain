//! Replies the model reads, as plain text it can act on.
//!
//! Every reply that carries page text has one header line ending in
//! [`UNTRUSTED`]: `t2 · Title (host) · untrusted page text` for a view of the
//! page, `t2 changes · untrusted page text` for what an action changed, and
//! `t2 · Title (host) · rows with "x" · untrusted page text` for `find`. The
//! Agent loop uses the header to drop superseded views from the conversation
//! ([`page_of`], [`superseded`]): browser agents spend most of their tokens
//! re-reading old copies of pages they have already left.

use super::clip;
use super::page::{body, changes, host, Change, Read, View};
use super::tools::{Handle, ShowReason};

/// Ends every header line over page text: marks it as data, never
/// instructions (the Agent's system prompt says the same of all tool results).
pub const UNTRUSTED: &str = "untrusted page text";

/// Longest title in a header.
const MAX_TITLE_CHARS: usize = 80;

fn header(handle: Handle, read: &Read, view: &View) -> String {
    let title = clip(read.title.trim(), MAX_TITLE_CHARS);
    let host = host(&read.url);
    match view {
        View::Find(words) => format!(
            "{handle} · {title} ({host}) · rows with \"{}\" · {UNTRUSTED}",
            clip(words, 60)
        ),
        _ => format!("{handle} · {title} ({host}) · {UNTRUSTED}"),
    }
}

fn view_block(handle: Handle, read: &Read, view: &View) -> String {
    format!("{}\n{}", header(handle, read, view), body(read, view))
}

/// A read for the model.
pub fn read_reply(handle: Handle, read: &Read, view: &View) -> String {
    view_block(handle, read, view)
}

/// The reply to `open`: the new tab's first view.
pub fn opened_reply(handle: Handle, read: &Read) -> String {
    format!("Opened.\n{}", view_block(handle, read, &View::Visible))
}

/// The reply to `act`: what it did, then what changed on screen (a whole
/// view when the tab moved to another page).
pub fn action_reply(handle: Handle, done: &str, before: &Read, after: &Read) -> String {
    match changes(before, after) {
        Change::NewPage => format!("{done}\n{}", view_block(handle, after, &View::Visible)),
        Change::Nothing => format!("{done} Nothing visible changed."),
        Change::Rows(rows) => format!("{done}\n{handle} changes · {UNTRUSTED}\n{rows}"),
    }
}

/// Pilot refused to click a payment control.
pub fn payment_refusal(name: &str) -> String {
    format!(
        "Grain Pilot does not press \"{}\": it commits money. Call show with reason user_action so the user reviews and pays.",
        clip(name, 80)
    )
}

/// Pilot refused to type into a secret field.
pub fn secret_refusal() -> String {
    "That is a password, card or code field. Grain Pilot never types there: call show with reason sign_in or user_action.".into()
}

/// The reply to `show`: what the model does next.
pub fn show_reply(handle: Handle, reason: ShowReason, message: Option<&str>) -> String {
    let ask = message
        .map(|m| format!(" Suggested words: \"{m}\"."))
        .unwrap_or_default();
    let next = match reason {
        ShowReason::SignIn => "Ask the user to sign in there. Never ask for or type their password or codes. End your turn and wait until they say they are done, then read the tab again.",
        ShowReason::UserAction => "Tell the user in one sentence exactly what to do there. End your turn and wait until they say they are done, then read the tab to check.",
        ShowReason::View => "Tell the user the tab is in front of them.",
    };
    format!("{handle} is now in front of the user.{ask} {next}")
}

/// The reply to `tabs`.
pub fn tabs_reply(rows: &[(Handle, String)]) -> String {
    if rows.is_empty() {
        return "Grain Pilot has no tabs open. Use open.".into();
    }
    rows.iter()
        .map(|(handle, title)| format!("{handle}: {}", clip(title, MAX_TITLE_CHARS)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The page text a tool result carries: its tab (`t2`) and whether it is a
/// whole view of the screen (which supersedes every earlier result for that
/// tab) rather than a change report or a `find`.
pub fn page_of(result: &str) -> Option<(&str, bool)> {
    result.lines().find_map(|line| {
        let rest = line.strip_suffix(UNTRUSTED)?.strip_suffix(" · ")?;
        let (tab, after) = rest.split_once(' ')?;
        let valid =
            tab.len() >= 2 && tab.starts_with('t') && tab[1..].chars().all(|c| c.is_ascii_digit());
        if !valid {
            return None;
        }
        let whole = after != "changes" && !after.contains(" · rows with \"");
        Some((tab, whole))
    })
}

/// What an earlier Pilot result shrinks to once a newer whole view of the
/// same tab exists: what it did, without the old page text.
pub fn superseded(result: &str) -> String {
    let mut kept = Vec::new();
    for line in result.lines() {
        if let Some((tab, _)) = page_of(line) {
            kept.push(format!(
                "[Older page text of {tab} dropped; the newest is below.]"
            ));
            break;
        }
        kept.push(line.to_string());
    }
    kept.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seats() -> Read {
        Read {
            title: "Seats".into(),
            url: "https://in.bookmyshow.com/seat?id=1".into(),
            rows: vec!["[3] Select Seats".into()],
            ..Read::default()
        }
    }

    #[test]
    fn replies_mark_page_text_untrusted() {
        let opened = opened_reply(Handle(1), &seats());
        assert_eq!(
            opened,
            "Opened.\nt1 · Seats (in.bookmyshow.com) · untrusted page text\n[3] Select Seats"
        );
        assert!(!opened.contains("id=1"), "full address leaked");
        let mut after = seats();
        after.rows.push("[4] Proceed".into());
        let acted = action_reply(Handle(2), "Clicked [3].", &seats(), &after);
        assert_eq!(
            acted,
            "Clicked [3].\nt2 changes · untrusted page text\n[4] Proceed"
        );
        assert_eq!(
            action_reply(Handle(2), "Clicked [3].", &seats(), &seats()),
            "Clicked [3]. Nothing visible changed."
        );
    }

    #[test]
    fn page_headers_are_recognised_and_old_views_shrink() {
        let view = opened_reply(Handle(2), &seats());
        assert_eq!(page_of(&view), Some(("t2", true)));
        let mut after = seats();
        after.rows.push("[4] Proceed".into());
        let change = action_reply(Handle(2), "Clicked [3].", &seats(), &after);
        assert_eq!(page_of(&change), Some(("t2", false)));
        let found = read_reply(Handle(3), &seats(), &View::Find("seat".into()));
        assert_eq!(page_of(&found), Some(("t3", false)));
        assert_eq!(page_of("Closed t2."), None);
        assert_eq!(page_of("x · untrusted page text"), None);
        let old = superseded(&change);
        assert_eq!(
            old,
            "Clicked [3].\n[Older page text of t2 dropped; the newest is below.]"
        );
        assert_eq!(page_of(&old), None, "a shrunk result is never shrunk again");
    }

    #[test]
    fn show_tells_the_model_to_wait() {
        let reply = show_reply(Handle(1), ShowReason::SignIn, Some("Sign in to continue"));
        assert!(reply.contains("t1 is now in front"));
        assert!(reply.contains("End your turn"));
        assert!(reply.contains("Sign in to continue"));
    }

    #[test]
    fn tabs_reply_lists_or_says_none() {
        assert!(tabs_reply(&[]).contains("no tabs"));
        assert_eq!(tabs_reply(&[(Handle(1), "Pune".into())]), "t1: Pune");
    }
}
