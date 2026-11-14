//! [GRAIN] Generative UI in the Agent chat (experiment, see
//! `docs/Generative UI/PLAN.md`). A chat reply may carry an interactive view:
//! the model writes its short spoken/chat answer, then an OpenUI Lang program
//! the notch renders with its own component library. The host splits the two
//! here; the chat keeps both (`AgentHistoryMessage::ui`).

/// The component reference the notch sends; anything larger is refused.
pub(crate) const MAX_SPEC_BYTES: usize = 96 * 1024;

/// How the model answers when the chat can show a view. `spec` (the
/// component reference) follows it, then [`UI_FORMAT_RULE`].
pub(crate) const UI_RULE: &str = "GENERATIVE UI: a reply in this chat can carry an interactive view, drawn in a small panel at the top of the screen. Prefer a view in most replies: whenever the answer has something worth seeing (weather, places, travel, prices, numbers, comparisons, lists, steps, options). Skip it only for pure conversation, a short fact one sentence fully covers, a rewrite or transformation of selected text, or a question back to the user. Use tools first whenever the answer needs live or external data, then build the view from their results; never invent live data. The view never scrolls: it must fit the space and the limits given in the component reference; show the top items and offer a follow-up button for the rest instead of going long. Earlier assistant turns may include the view you showed; a follow-up may refine or replace it.";

/// Last in the prompt, so it wins over the component reference's own
/// "start with root" rule.
pub(crate) const UI_FORMAT_RULE: &str = "REPLY FORMAT WITH A VIEW: first your chat reply, which is shown in the chat and read aloud: one or two short, warm, conversational sentences that give the answer straight away, like a friend telling you (\"It's 14 degrees with light rain until 3, then it clears up nicely.\"). Never describe the view or say you opened, showed or made something; no lists, URLs, symbols or markdown in it. Then, on a new line, the OpenUI Lang program starting with `root = Canvas(`: no code fences and nothing after it. WITHOUT A VIEW: reply normally and write no program.";

/// With Polished UI on (`Settings::agent_polished_ui`): design every view
/// instead of dumping the answer into the plainest blocks.
pub(crate) const UI_POLISH_RULE: &str = "POLISHED UI: every view must look designed, never dumped. Before writing it, pick the richest structure that fits the content: a designed view first; otherwise compose a small layout, never a bare list of lines. Lead with the single most important thing, large (a Hero, or a Heading with a short grey label above it). Give each item its attributes: a ranking or chart becomes List rows with the rank as meta, the title, a subtitle (artist, author, place), and a matching icon or a real thumbnail; options to choose between become Cards side by side or a Table; key numbers become a row of 2-3 Stats; facts become a Details card; a how-to becomes Steps plus a Details card of what you need. Use Badge only for real status, icons that match the meaning, and Divider to separate sections. Hierarchy comes from size, weight and the grey levels, not from colour. A simple request still gets a well-composed view, just a smaller one. Never a plain numbered Text or Markdown list when the items have any attributes.";

/// The system message for a turn that may carry a view: what a view is and
/// when to use one, the component reference, polish (when on), the format.
pub(crate) fn ui_prompt(spec: &str, polished: bool) -> String {
    let polish = if polished { UI_POLISH_RULE } else { "" };
    format!("{UI_RULE}\n\n{spec}\n\n{polish}\n\n{UI_FORMAT_RULE}")
}

/// The latest view a model sees again with its reply is cut to this size.
pub(crate) const MAX_UI_CHARS: usize = 40_000;

/// Whether `line` opens an OpenUI Lang program (`root = …`).
fn opens_program(line: &str) -> bool {
    line.trim_start()
        .strip_prefix("root")
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

/// Split a reply into its chat text and the view program after it (`None`
/// when there is none). Tolerates a code fence around the program and a
/// leftover `SAY:` tag on the text.
pub(crate) fn split_ui(reply: &str) -> (String, Option<String>) {
    let mut offset = 0;
    let mut start = None;
    for line in reply.split_inclusive('\n') {
        if opens_program(line) {
            start = Some(offset);
            break;
        }
        offset += line.len();
    }
    let Some(start) = start else {
        return (reply.trim().to_owned(), None);
    };
    let mut text = reply[..start].trim_end();
    // An opening fence right above the program.
    if let Some((before, last)) = text.rsplit_once('\n').or(Some(("", text))) {
        if last.trim_start().starts_with("```") {
            text = before.trim_end();
        }
    }
    let text = text.trim();
    let text = match text.get(..4) {
        Some(tag) if tag.eq_ignore_ascii_case("say:") => text[4..].trim(),
        _ => text,
    };
    let text = text.trim_matches('"').trim().to_owned();
    let mut program = &reply[start..];
    if let Some(end) = program.find("\n```") {
        program = &program[..end];
    }
    let program = program.trim();
    (text, (!program.is_empty()).then(|| program.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_splits_into_text_and_view() {
        assert_eq!(
            split_ui("It's 14 degrees and drizzly.\nroot = Canvas([])"),
            (
                "It's 14 degrees and drizzly.".to_owned(),
                Some("root = Canvas([])".to_owned())
            )
        );
        assert_eq!(
            split_ui("SAY: \"Sunny all day.\"\r\n```openui\r\nroot = Canvas([])\r\n```\r\n"),
            (
                "Sunny all day.".to_owned(),
                Some("root = Canvas([])".to_owned())
            )
        );
        assert_eq!(
            split_ui("root=Canvas([])"),
            (String::new(), Some("root=Canvas([])".to_owned()))
        );
    }

    #[test]
    fn polish_is_only_asked_for_when_on() {
        assert!(ui_prompt("SPEC", true).contains(UI_POLISH_RULE));
        let plain = ui_prompt("SPEC", false);
        assert!(!plain.contains("POLISHED UI"));
        assert!(plain.contains("SPEC") && plain.ends_with(UI_FORMAT_RULE));
    }

    #[test]
    fn a_plain_reply_has_no_view() {
        assert_eq!(
            split_ui("  Just a sentence about rooted trees.  "),
            ("Just a sentence about rooted trees.".to_owned(), None)
        );
        // Multi-byte text right at the tag boundary must not panic.
        assert_eq!(split_ui("Sé"), ("Sé".to_owned(), None));
    }
}
