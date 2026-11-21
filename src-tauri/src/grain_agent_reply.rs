//! [GRAIN] What an Agent reply says aloud versus what it shows (see
//! `docs/Agent Replies/PLAN.md`). With talk-back on, a long reply starts with
//! one `SAY:` line: a short spoken version written by the model in the same
//! call. That line is read aloud in full; the rest is the reply shown in the
//! chat. Short replies carry no line and are read as they are.

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
