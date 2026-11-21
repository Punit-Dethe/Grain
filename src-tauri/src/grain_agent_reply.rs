//! [GRAIN] What an Agent reply says aloud versus what it shows (see
//! `docs/Agent Replies/PLAN.md`). With talk-back on, every reply starts with
//! its spoken part in `<say>…</say>`, written by the model in the same call:
//! a short reply whole, a long one as a short spoken version with the full
//! reply after the tag. The spoken part is read aloud in full; the chat shows
//! it as its own message, then the rest. A reply without the tag is read
//! whole.
//!
//! A text transform ("turn this into an email", "more formal") comes back as
//! a result: the new text in a `<result kind="…">…</result>` block, shown in
//! its own card with a Copy button rather than read aloud.

use serde::{Deserialize, Serialize};
use specta::Type;

/// [GRAIN] Text the user asked to get back, changed: shown in a result card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AgentResult {
    /// One of [`RESULT_KINDS`]; picks the card's layout and refine chips.
    pub kind: String,
    /// The subject line of an email result.
    #[serde(default)]
    pub subject: Option<String>,
    pub body: String,
    /// `"markdown"` when the text the user selected was Markdown (the card
    /// renders it; Copy puts both the source and the formatted text on the
    /// clipboard), else `"plain"`. Set by the host, never by the model.
    #[serde(default = "plain_format")]
    pub format: String,
}

fn plain_format() -> String {
    PLAIN.into()
}

pub(crate) const PLAIN: &str = "plain";
pub(crate) const MARKDOWN: &str = "markdown";

pub(crate) const RESULT_KINDS: [&str; 6] =
    ["email", "message", "rewrite", "list", "code", "translation"];

const RESULT_RULE: &str = "RESULT TEXT: when the user asks you to change, rewrite, reformat or translate text and wants that text back (\"turn this into an email\", \"make it more formal\", \"fix the grammar\"), put only the new text in one block: <result kind=\"email|message|rewrite|list|code|translation\"> … </result>, with subject=\"…\" added for an email. No preamble or comment inside the block; a short line before it may say what you did. Summaries, answers and actions about the text are not results: reply normally. To refine an earlier result, write the whole new text in a new block.";

/// Sent with every notch chat turn; `markdown` = the selection is Markdown.
pub(crate) fn result_rule(markdown: bool) -> String {
    let format = if markdown {
        "The selected text is Markdown: keep the result in Markdown."
    } else {
        "Write the result as plain text that pastes cleanly anywhere: no Markdown (no **, #, backticks or [links](…)); plain line breaks and simple dashes for lists are fine."
    };
    format!("{RESULT_RULE} {format}")
}

/// Whether text reads as Markdown: a heading, a code fence, bold, a link or
/// a table, or a list of three or more items.
pub(crate) fn looks_like_markdown(text: &str) -> bool {
    let mut list_items = 0;
    for line in text.lines().map(str::trim_start) {
        let heading = line.trim_start_matches('#');
        if (line.starts_with('#') && line.len() - heading.len() <= 6 && heading.starts_with(' '))
            || line.starts_with("```")
            || line.starts_with("|---")
            || line.starts_with("| ---")
        {
            return true;
        }
        if ["- ", "* ", "+ "]
            .iter()
            .any(|bullet| line.starts_with(bullet))
            || line
                .split_once(". ")
                .is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        {
            list_items += 1;
        }
    }
    let bold = text.lines().any(|line| {
        line.find("**")
            .is_some_and(|at| line[at + 2..].find("**").is_some_and(|len| len > 0))
    });
    let link = text
        .find("](")
        .is_some_and(|at| text[..at].contains('[') && text[at..].contains(')'));
    bold || link || list_items >= 3
}

/// Plain text from a result the model may have marked up anyway: heading
/// marks, bold, inline code ticks and link syntax go; lines and dashes stay.
pub(crate) fn plain_text(body: &str) -> String {
    body.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let heading = trimmed.trim_start_matches('#');
            let line = if trimmed.starts_with('#') && heading.starts_with(' ') {
                heading.trim_start()
            } else {
                line
            };
            let line = line.replace("**", "").replace("__", "").replace('`', "");
            unlink(&line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `[label](url)` → `label (url)`; a label that is the URL stays once.
fn unlink(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(mid) = rest[open..].find("](").map(|i| open + i) else {
            break;
        };
        let Some(end) = rest[mid..].find(')').map(|i| mid + i) else {
            break;
        };
        let label = &rest[open + 1..mid];
        let url = &rest[mid + 2..end];
        out.push_str(&rest[..open]);
        if label == url || label.is_empty() {
            out.push_str(url);
        } else {
            out.push_str(&format!("{label} ({url})"));
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Sent as a system message only while talk-back is on.
const SAY_RULE: &str = "SPOKEN PART: your reply is read aloud. Start every reply (after [final], when you use it) with the part to be spoken, inside <say>…</say>. A short reply goes inside it whole, with nothing after the closing tag: never repeat or reword it after </say>. A longer one (a piece of writing, a list, table or code, or more than is easy to take in by ear) opens with a short spoken version inside it, then the full reply after </say>. The spoken version tells the user what you did or found, naturally, the way you would tell a friend; it never reads the content out. Describe a piece of writing (\"I've written you a poem about autumn; it ends on a hopeful note.\") and sum up finished actions (\"Filed the bug on GitHub, added it to Linear and wrote it up in Notion.\"). Keep it short, but say what the user needs to hear. If the user asked to hear something read out, put all of it inside. Inside the tags: plain spoken sentences, no markdown, lists, links or code.";

/// The spoken-part rule, sent only while talk-back is on. With views
/// possible, a view reply's short explanation is its spoken part.
pub(crate) fn say_rule(views: bool) -> String {
    if views {
        format!("{SAY_RULE} With a view, its short explanation is the spoken part; the program comes last, after the closing tag.")
    } else {
        SAY_RULE.to_owned()
    }
}

const SAY_OPEN: &str = "<say>";
const SAY_CLOSE: &str = "</say>";

/// The same-round marker a reply may lead with (`agent::FINAL_REPLY_MARKER`).
const FINAL: &str = "[final]";

/// The text after a leading `[final]` marker, if any.
pub(crate) fn skip_final(text: &str) -> &str {
    let text = text.trim_start();
    match text.get(..FINAL.len()) {
        Some(head) if head.eq_ignore_ascii_case(FINAL) => text[FINAL.len()..].trim_start(),
        _ => text,
    }
}

/// The reply after a leading `<say>` tag, when it starts with one.
fn after_open(text: &str) -> Option<&str> {
    let text = skip_final(text);
    let head = text.get(..SAY_OPEN.len())?;
    head.eq_ignore_ascii_case(SAY_OPEN)
        .then(|| &text[SAY_OPEN.len()..])
}

/// Split the text after `<say>` into the spoken part and the reply after
/// the closing tag (`None` while it has not arrived). A tag still arriving
/// at the end (`</sa`) is held back.
fn split_close(rest: &str) -> (&str, Option<&str>) {
    let lower = rest.to_ascii_lowercase();
    if let Some(at) = lower.find(SAY_CLOSE) {
        return (&rest[..at], Some(&rest[at + SAY_CLOSE.len()..]));
    }
    match lower.rfind('<') {
        Some(at) if SAY_CLOSE.starts_with(&lower[at..]) => (&rest[..at], None),
        _ => (rest, None),
    }
}

/// Split a finished reply into its spoken part (`None` when it has none)
/// and the rest of the reply, without tags (the whole reply when it has no
/// spoken part). A missing closing tag makes the whole reply the spoken part.
pub(crate) fn split_say(reply: &str) -> (Option<String>, String) {
    let Some(rest) = after_open(reply) else {
        return (None, reply.trim().to_owned());
    };
    let (said, after) = split_close(rest);
    let (said, after) = (said.trim(), after.unwrap_or("").trim());
    (
        (!said.is_empty()).then(|| said.to_owned()),
        after.to_owned(),
    )
}

/// Put `text` right after a reply's spoken part (or in front of a reply
/// with none), keeping the tags for [`split_say`].
pub(crate) fn insert_after_say(reply: &str, text: &str) -> String {
    let Some(rest) = after_open(reply) else {
        return format!("{text}\n\n{}", reply.trim());
    };
    match split_close(rest) {
        (said, Some(after)) => {
            format!("{SAY_OPEN}{said}{SAY_CLOSE}\n{text}\n\n{}", after.trim())
        }
        (said, None) => format!("{SAY_OPEN}{said}{SAY_CLOSE}\n{text}"),
    }
}

/// Whether the text after the spoken part only says it again: a short reply
/// the model also wrote out after `</say>`, reworded or not. Most of its
/// words are the spoken part's, and it is not much longer. A stream's text so
/// far counts too (it is shorter still).
pub(crate) fn repeats_said(said: &str, rest: &str) -> bool {
    fn words(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect()
    }
    let said = words(said);
    let rest = words(rest);
    if said.is_empty() || rest.is_empty() || rest.len() > said.len() * 3 / 2 + 3 {
        return false;
    }
    let shared = rest.iter().filter(|word| said.contains(word)).count();
    shared * 10 >= rest.len() * 7
}

/// A streaming reply so far, as far as its spoken part goes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StreamSay<'a> {
    /// Too little has arrived to tell whether it starts with `<say>`.
    Undecided,
    /// No spoken part: the reply itself is read.
    Plain,
    /// The spoken part so far (`complete` once its tag has closed) and the
    /// reply after it so far.
    Say {
        line: &'a str,
        complete: bool,
        rest: &'a str,
    },
}

pub(crate) fn stream_say(text: &str) -> StreamSay<'_> {
    let prefix_of = |whole: &str, part: &str| {
        whole
            .get(..part.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(part))
    };
    let lead = text.trim_start();
    if lead.len() < FINAL.len() && !lead.is_empty() && prefix_of(FINAL, lead) {
        return StreamSay::Undecided;
    }
    let start = skip_final(text);
    if start.len() < SAY_OPEN.len() {
        let could_be = prefix_of(SAY_OPEN, start);
        return if could_be {
            StreamSay::Undecided
        } else {
            StreamSay::Plain
        };
    }
    let Some(rest) = after_open(text) else {
        return StreamSay::Plain;
    };
    let (said, after) = split_close(rest);
    StreamSay::Say {
        line: said.trim_start(),
        complete: after.is_some(),
        rest: after.unwrap_or(""),
    }
}

const OPEN: &str = "<result";
const CLOSE: &str = "</result>";

/// The value of `name="…"` in a tag's attributes.
fn attribute(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(name).map(|i| from + i) {
        let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric();
        let rest = attrs[at + name.len()..].trim_start();
        if let Some(rest) = rest.strip_prefix('=').filter(|_| before_ok) {
            let rest = rest.trim_start();
            let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
            let value = &rest[1..];
            let end = value.find(quote)?;
            return Some(value[..end].trim().to_owned());
        }
        from = at + name.len();
    }
    None
}

/// A body wrapped in one code fence loses the fence.
fn unfence(body: &str) -> &str {
    let body = body.trim();
    let Some(inner) = body
        .strip_prefix("```")
        .and_then(|rest| rest.strip_suffix("```"))
    else {
        return body;
    };
    // The first line is the fence's language tag (possibly empty).
    match inner.split_once('\n') {
        Some((_, text)) => text.trim(),
        None => body,
    }
}

/// Split a reply into the text around its result block and the result
/// (`None` when there is no block or it is empty). A missing closing tag
/// takes the rest of the reply.
pub(crate) fn split_result(reply: &str) -> (String, Option<AgentResult>) {
    let lower = reply.to_ascii_lowercase();
    let Some(open) = lower.find(OPEN) else {
        return (reply.trim().to_owned(), None);
    };
    let Some(tag_end) = lower[open..].find('>').map(|i| open + i) else {
        return (reply.trim().to_owned(), None);
    };
    let attrs = &reply[open + OPEN.len()..tag_end];
    let body_start = tag_end + 1;
    let (body_end, after) = match lower[body_start..].find(CLOSE) {
        Some(i) => (body_start + i, body_start + i + CLOSE.len()),
        None => (reply.len(), reply.len()),
    };
    let body = unfence(&reply[body_start..body_end]);
    if body.is_empty() {
        return (reply.trim().to_owned(), None);
    }
    let kind = attribute(attrs, "kind")
        .map(|kind| kind.to_ascii_lowercase())
        .filter(|kind| RESULT_KINDS.contains(&kind.as_str()))
        .unwrap_or_else(|| "rewrite".into());
    let subject =
        attribute(attrs, "subject").filter(|subject| kind == "email" && !subject.is_empty());
    let text = format!("{}\n{}", reply[..open].trim(), reply[after..].trim());
    (
        text.trim().to_owned(),
        Some(AgentResult {
            kind,
            subject,
            body: body.to_owned(),
            format: PLAIN.into(),
        }),
    )
}

/// Give a result the format of the text it came from: Markdown stays
/// Markdown; anything else is cleaned to plain text (code is left as is).
pub(crate) fn settle_format(result: &mut AgentResult, markdown_source: bool) {
    if markdown_source {
        result.format = MARKDOWN.into();
    } else {
        result.format = PLAIN.into();
        if result.kind != "code" {
            result.body = plain_text(&result.body);
            result.subject = result.subject.as_deref().map(plain_text);
        }
    }
}

/// A streaming reply so far, outside its result block: the text before the
/// block (the only part that may be spoken), and what to show (that text
/// plus the result's body so far, without tags).
pub(crate) fn stream_result(text: &str) -> (&str, String) {
    let lower = text.to_ascii_lowercase();
    let Some(open) = lower.find(OPEN) else {
        // Hold back a tag still arriving ("<res").
        let outside = match text.rfind('<') {
            Some(at) if OPEN.starts_with(&lower[at..]) => &text[..at],
            _ => text,
        };
        return (outside, outside.trim_end().to_owned());
    };
    let outside = &text[..open];
    let body = lower[open..]
        .find('>')
        .map(|i| {
            let body = &text[open + i + 1..];
            let body = match body.to_ascii_lowercase().find(CLOSE) {
                Some(end) => &body[..end],
                None => body,
            };
            body.trim()
        })
        .unwrap_or("");
    let shown = format!("{}\n\n{body}", outside.trim());
    (outside, shown.trim().to_owned())
}

/// A result as the model wrote it, to send back with the next turn.
pub(crate) fn result_block(result: &AgentResult) -> String {
    let subject = result
        .subject
        .as_deref()
        .map(|subject| format!(" subject=\"{}\"", subject.replace('"', "'")))
        .unwrap_or_default();
    format!(
        "<result kind=\"{}\"{subject}>\n{}\n</result>",
        result.kind, result.body
    )
}

/// What is said for a host-gated approval: the action in short, never the
/// whole confirmation (that stays on screen).
pub(crate) fn confirm_line(title: &str, summary: &str) -> String {
    let usable = |s: &str| {
        let s = s.trim().trim_end_matches(['.', '!', '?']).trim().to_owned();
        (!s.is_empty() && s.chars().count() <= 140).then_some(s)
    };
    let Some(what) = usable(summary) else {
        return match usable(title) {
            Some(title) => format!("I need your OK first: {title}."),
            None => "I need your OK before I go ahead.".into(),
        };
    };
    let what = what.as_str();
    // "Open a bug report" → "open a bug report"; "GitHub …" stays as it is.
    let first = what.chars().next().unwrap_or_default();
    let first_word = what.split_whitespace().next().unwrap_or_default();
    let name_like = first_word.chars().skip(1).any(char::is_uppercase);
    let what = if first.is_uppercase() && !name_like {
        first.to_lowercase().chain(what.chars().skip(1)).collect()
    } else {
        what.to_owned()
    };
    format!("I need your OK to {what}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_reply_splits_into_its_short_version_and_the_rest() {
        let reply = "<say>I've written you a story about a lighthouse keeper.</say>\n\nThe storm came in at dusk...\n\nThe end.";
        assert_eq!(
            split_say(reply),
            (
                Some("I've written you a story about a lighthouse keeper.".into()),
                "The storm came in at dusk...\n\nThe end.".into()
            )
        );
    }

    #[test]
    fn a_short_reply_is_spoken_whole() {
        let reply = "  <SAY>It's 12.\n\nAnd it rains at 3.</Say>";
        assert_eq!(
            split_say(reply),
            (Some("It's 12.\n\nAnd it rains at 3.".into()), String::new())
        );
        // A missing closing tag: the whole reply is the spoken part.
        assert_eq!(
            split_say("<say>Done, I'll remind you at 5."),
            (Some("Done, I'll remind you at 5.".into()), String::new())
        );
    }

    #[test]
    fn a_reply_without_the_tag_has_no_spoken_part() {
        assert_eq!(split_say("Twelve."), (None, "Twelve.".into()));
        assert_eq!(
            split_say("Sayonara is Japanese for goodbye."),
            (None, "Sayonara is Japanese for goodbye.".into())
        );
        // Not at the start: not a spoken part.
        assert_eq!(
            split_say("Hi <say>x</say>"),
            (None, "Hi <say>x</say>".into())
        );
        // An empty spoken part is ignored.
        assert_eq!(
            split_say("<say></say>\nThe reply"),
            (None, "The reply".into())
        );
    }

    #[test]
    fn text_goes_in_right_after_the_spoken_part() {
        assert_eq!(
            insert_after_say("<say>All done.</say>\nDetails", "Receipt"),
            "<say>All done.</say>\nReceipt\n\nDetails"
        );
        assert_eq!(
            split_say(&insert_after_say("<say>All done.</say>", "Receipt")),
            (Some("All done.".into()), "Receipt".into())
        );
        assert_eq!(insert_after_say("Done.", "Receipt"), "Receipt\n\nDone.");
    }

    #[test]
    fn streaming_decides_once_the_tag_is_clear() {
        assert_eq!(stream_say(""), StreamSay::Undecided);
        assert_eq!(stream_say(" <"), StreamSay::Undecided);
        assert_eq!(stream_say("<SA"), StreamSay::Undecided);
        assert_eq!(stream_say("<b>"), StreamSay::Plain);
        assert_eq!(stream_say("Sayonara"), StreamSay::Plain);
        assert_eq!(stream_say("[fin"), StreamSay::Undecided);
        assert_eq!(
            stream_say("[final] <say>Done.</say>"),
            StreamSay::Say {
                line: "Done.",
                complete: true,
                rest: ""
            }
        );
        assert_eq!(
            stream_say("<say>I've written</s"),
            StreamSay::Say {
                line: "I've written",
                complete: false,
                rest: ""
            }
        );
        assert_eq!(
            stream_say("<say>I've written you a poem.</say>\nRoses"),
            StreamSay::Say {
                line: "I've written you a poem.",
                complete: true,
                rest: "\nRoses"
            }
        );
    }

    #[test]
    fn a_short_reply_written_twice_is_told_apart() {
        let said = "Hey! Everything's going well, thanks for asking. How are you doing today, and how can I help?";
        assert!(repeats_said(
            said,
            "Hey! Everything's going well, thanks for asking. How are you doing today, and what can I help you with?"
        ));
        // While it streams in.
        assert!(repeats_said(said, "Hey! Everything's going"));
        // Real detail after a short version is never a repeat.
        let story = "The keeper climbed the stairs at dusk and lit the lamp. ".repeat(6);
        assert!(!repeats_said(
            "I wrote you a story about a lighthouse keeper.",
            &story
        ));
        assert!(!repeats_said(
            "The meeting is at 3.",
            "Room 4B, second floor."
        ));
        assert!(!repeats_said(
            "I found three flights to Paris.",
            "Here are the flights:"
        ));
    }

    #[test]
    fn the_same_round_marker_is_never_shown() {
        assert_eq!(stream_say("[final] Done, it is filed."), StreamSay::Plain);
        assert_eq!(skip_final("  [FINAL] Done."), "Done.");
        assert_eq!(skip_final("Done."), "Done.");
    }

    #[test]
    fn a_transform_splits_into_text_and_result() {
        let reply = "Here's your email.\n<result kind=\"email\" subject=\"Thursday follow-up\">\nHi Sam,\n\nThanks for the call.\n</result>";
        assert_eq!(
            split_result(reply),
            (
                "Here's your email.".into(),
                Some(AgentResult {
                    kind: "email".into(),
                    subject: Some("Thursday follow-up".into()),
                    body: "Hi Sam,\n\nThanks for the call.".into(),
                    format: PLAIN.into(),
                })
            )
        );
    }

    #[test]
    fn result_blocks_are_parsed_tolerantly() {
        // Unknown kind, no closing tag, a fence inside, single quotes.
        let (text, result) = split_result("<RESULT kind='poem'>```\nRoses\n```");
        assert_eq!(text, "");
        let result = result.unwrap();
        assert_eq!(
            (result.kind.as_str(), result.body.as_str()),
            ("rewrite", "Roses")
        );
        // A subject only belongs to an email; code keeps its lines.
        let (_, result) =
            split_result("<result kind=\"code\" subject=\"x\">```rust\nfn main() {}\n```</result>");
        let result = result.unwrap();
        assert_eq!(result.subject, None);
        assert_eq!(result.body, "fn main() {}");
        // `subkind=` is not `kind=`.
        let (_, result) = split_result("<result subkind=\"email\" kind=\"list\">- a</result>");
        assert_eq!(result.unwrap().kind, "list");
    }

    #[test]
    fn plain_and_empty_results_stay_plain() {
        assert_eq!(
            split_result("Just an answer."),
            ("Just an answer.".into(), None)
        );
        assert_eq!(
            split_result("<result kind=\"email\"> </result>"),
            ("<result kind=\"email\"> </result>".into(), None)
        );
        // Mid-tag (no `>` yet) is not a result.
        assert_eq!(
            split_result("a <result kind"),
            ("a <result kind".into(), None)
        );
    }

    #[test]
    fn streaming_never_speaks_or_shows_the_tags() {
        assert_eq!(stream_result("Done.\n<res"), ("Done.\n", "Done.".into()));
        assert_eq!(
            stream_result("Done.\n<result kind=\"email\">\nHi Sam,"),
            ("Done.\n", "Done.\n\nHi Sam,".into())
        );
        assert_eq!(
            stream_result("<result kind=\"rewrite\">Better text</result>"),
            ("", "Better text".into())
        );
        // "<" that is not a tag stays.
        assert_eq!(stream_result("a < b"), ("a < b", "a < b".into()));
    }

    #[test]
    fn a_result_round_trips_as_its_block() {
        let result = AgentResult {
            kind: "email".into(),
            subject: Some("Hello \"there\"".into()),
            body: "Hi.".into(),
            format: PLAIN.into(),
        };
        let block = result_block(&result);
        assert_eq!(
            split_result(&block).1.unwrap(),
            AgentResult {
                subject: Some("Hello 'there'".into()),
                ..result
            }
        );
    }

    #[test]
    fn markdown_is_recognised_by_its_marks() {
        for markdown in [
            "# Title\nBody",
            "Intro\n```\ncode\n```",
            "It is **very** good.",
            "See [the docs](https://x.dev).",
            "| a | b |\n|---|---|",
            "- one\n- two\n- three",
            "1. one\n2. two\n3. three",
        ] {
            assert!(looks_like_markdown(markdown), "{markdown}");
        }
        for plain in [
            "Hi Sam,\n\nThanks for the call on Thursday.\n\nBest,\nPunit",
            "#1 priority is the launch.",
            "Items:\n- milk\n- eggs",
            "2 * 3 = 6 and 4 ** 2",
            "Version 1. Then 2.",
        ] {
            assert!(!looks_like_markdown(plain), "{plain}");
        }
    }

    #[test]
    fn plain_results_lose_their_markup() {
        assert_eq!(
            plain_text("## Thursday\nIt is **really** `fine`, see [docs](https://x.dev).\n- one"),
            "Thursday\nIt is really fine, see docs (https://x.dev).\n- one"
        );
        assert_eq!(
            plain_text("[https://x.dev](https://x.dev)"),
            "https://x.dev"
        );
        assert_eq!(plain_text("#1 priority"), "#1 priority");
    }

    #[test]
    fn a_result_takes_its_sources_format() {
        let made = || AgentResult {
            kind: "email".into(),
            subject: Some("**Hi**".into()),
            body: "# Hello\n**Sam**".into(),
            format: PLAIN.into(),
        };
        let mut plain = made();
        settle_format(&mut plain, false);
        assert_eq!(
            (
                plain.format.as_str(),
                plain.subject.as_deref(),
                plain.body.as_str()
            ),
            (PLAIN, Some("Hi"), "Hello\nSam")
        );
        let mut markdown = made();
        settle_format(&mut markdown, true);
        assert_eq!(
            (markdown.format.as_str(), markdown.body.as_str()),
            (MARKDOWN, "# Hello\n**Sam**")
        );
        let mut code = AgentResult {
            kind: "code".into(),
            body: "let x = **y;".into(),
            ..made()
        };
        settle_format(&mut code, false);
        assert_eq!(code.body, "let x = **y;");
    }

    #[test]
    fn a_view_reply_speaks_its_own_explanation() {
        assert_eq!(say_rule(false), SAY_RULE);
        let with_views = say_rule(true);
        assert!(with_views.starts_with(SAY_RULE));
        assert!(with_views.contains("its short explanation is the spoken part"));
    }

    #[test]
    fn approvals_are_said_in_short() {
        assert_eq!(
            confirm_line("Create an issue", "Open a bug report in acme/webapp."),
            "I need your OK to open a bug report in acme/webapp."
        );
        assert_eq!(
            confirm_line("Send", "Send it"),
            "I need your OK to send it."
        );
        assert_eq!(
            confirm_line("x", "LinkedIn: share the launch post"),
            "I need your OK to LinkedIn: share the launch post."
        );
        assert_eq!(
            confirm_line("Publish the build", ""),
            "I need your OK first: Publish the build."
        );
        assert_eq!(confirm_line("", ""), "I need your OK before I go ahead.");
    }
}
