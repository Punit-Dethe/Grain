//! [GRAIN] What an Agent reply says aloud versus what it shows (see
//! `docs/Agent Replies/PLAN.md`). With talk-back on, a long reply starts with
//! one `SAY:` line: a short spoken version written by the model in the same
//! call. That line is read aloud in full; the rest is the reply shown in the
//! chat. Short replies carry no line and are read as they are.
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
pub(crate) const SAY_RULE: &str = "SPOKEN VERSION: your reply is read aloud. When it is long (more than about three sentences, or it has a list, table, code or a piece of writing), begin with one line `SAY: ` followed by one or two natural sentences that tell the user what you did or found, the way you would tell a friend; never read the content out. Describe a piece of writing (\"I've written you a poem about autumn; it ends on a hopeful note.\") and sum up finished actions briefly (\"Filed the bug on GitHub, added it to Linear and wrote it up in Notion.\"). Then the full reply from the next line, unchanged. If the user asked to hear something read out, put all of it in the SAY line. Short replies need no SAY line.";

const TAG: &str = "SAY:";

/// Leading markdown emphasis a model may wrap the tag in (`**SAY:**`).
fn strip_emphasis(text: &str) -> &str {
    text.trim_start_matches(['*', '_'])
}

/// The text after a leading `SAY:` tag, when `text` starts with one.
fn after_tag(text: &str) -> Option<&str> {
    let text = strip_emphasis(text.trim_start());
    let head = text.get(..TAG.len())?;
    head.eq_ignore_ascii_case(TAG)
        .then(|| strip_emphasis(&text[TAG.len()..]))
}

/// The spoken line itself, without padding or quotes.
fn clean_line(line: &str) -> String {
    line.trim()
        .trim_end_matches(['*', '_'])
        .trim()
        .trim_matches('"')
        .trim()
        .to_owned()
}

/// Split a finished reply into its spoken version (`None` when it has none)
/// and the reply to show. A reply that is only a `SAY:` line shows that line.
pub(crate) fn split_say(reply: &str) -> (Option<String>, String) {
    let Some(rest) = after_tag(reply) else {
        return (None, reply.to_owned());
    };
    let (line, body) = rest.split_once('\n').unwrap_or((rest, ""));
    let say = clean_line(line);
    let body = body.trim();
    match (say.is_empty(), body.is_empty()) {
        (true, _) => (None, body.to_owned()),
        (false, true) => (Some(say.clone()), say),
        (false, false) => (Some(say), body.to_owned()),
    }
}

/// A streaming reply so far, as far as its spoken version goes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StreamSay<'a> {
    /// Too little has arrived to tell whether it starts with `SAY:`.
    Undecided,
    /// No spoken version: the reply itself is read.
    Plain,
    /// The spoken version so far (`complete` once its line has ended) and
    /// the reply to show after it.
    Say {
        line: String,
        complete: bool,
        body: &'a str,
    },
}

pub(crate) fn stream_say(text: &str) -> StreamSay<'_> {
    let start = strip_emphasis(text.trim_start());
    if start.len() < TAG.len() {
        let could_be = TAG
            .get(..start.len())
            .is_some_and(|tag| tag.eq_ignore_ascii_case(start));
        return if could_be {
            StreamSay::Undecided
        } else {
            StreamSay::Plain
        };
    }
    let Some(rest) = after_tag(text) else {
        return StreamSay::Plain;
    };
    match rest.split_once('\n') {
        Some((line, body)) => StreamSay::Say {
            line: clean_line(line),
            complete: true,
            body: body.trim_start(),
        },
        None => StreamSay::Say {
            line: clean_line(rest),
            complete: false,
            body: "",
        },
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
    fn a_long_reply_splits_into_spoken_version_and_body() {
        let reply = "SAY: I've written you a story about a lighthouse keeper.\n\nThe storm came in at dusk...\n\nThe end.";
        assert_eq!(
            split_say(reply),
            (
                Some("I've written you a story about a lighthouse keeper.".into()),
                "The storm came in at dusk...\n\nThe end.".into()
            )
        );
    }

    #[test]
    fn tag_variants_are_tolerated() {
        for reply in [
            "**SAY:** Filed it on GitHub.\nDetails",
            "say: \"Filed it on GitHub.\"\r\nDetails",
            "  SAY:Filed it on GitHub.\nDetails",
        ] {
            assert_eq!(
                split_say(reply),
                (Some("Filed it on GitHub.".into()), "Details".into()),
                "{reply}"
            );
        }
    }

    #[test]
    fn short_and_plain_replies_have_no_spoken_version() {
        assert_eq!(split_say("Twelve."), (None, "Twelve.".into()));
        assert_eq!(
            split_say("Sayonara is Japanese for goodbye."),
            (None, "Sayonara is Japanese for goodbye.".into())
        );
        // A reply that is only the line still shows something.
        assert_eq!(
            split_say("SAY: Done, I'll remind you at 5."),
            (
                Some("Done, I'll remind you at 5.".into()),
                "Done, I'll remind you at 5.".into()
            )
        );
        // An empty tag is ignored.
        assert_eq!(split_say("SAY:\nThe reply"), (None, "The reply".into()));
    }

    #[test]
    fn streaming_decides_once_the_tag_is_clear() {
        assert_eq!(stream_say(""), StreamSay::Undecided);
        assert_eq!(stream_say("S"), StreamSay::Undecided);
        assert_eq!(stream_say("**sa"), StreamSay::Undecided);
        assert_eq!(stream_say("Su"), StreamSay::Plain);
        assert_eq!(stream_say("Sayonara"), StreamSay::Plain);
        assert_eq!(
            stream_say("SAY: I've written"),
            StreamSay::Say {
                line: "I've written".into(),
                complete: false,
                body: ""
            }
        );
        assert_eq!(
            stream_say("SAY: I've written you a poem.\nRoses"),
            StreamSay::Say {
                line: "I've written you a poem.".into(),
                complete: true,
                body: "Roses"
            }
        );
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
