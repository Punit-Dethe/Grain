//! [GRAIN] grain-editor — Phase 3 milestone 1 (EXECUTION-PLAN.md P4):
//! a Floem window with an editable text buffer, plus the pulldown-cmark
//! parse step proven out on a sample document. Milestones 2–5 (parse on
//! every change, decoration spans → text-layout attributes, cursor-gated
//! marker hiding) build on this scaffold; the IPC bridge to the pill
//! daemon is deliberately last.

use floem::prelude::*;
use floem::views::editor::core::indent::IndentStyle;
use floem::views::text_editor::text_editor;

const SAMPLE: &str = "# Grain Editor\n\nThis is the **Floem** scaffold. \
Everything you type lives in a rope buffer; the markdown parser runs over \
it to produce *decoration spans*.\n\n- [ ] parse on change\n- [ ] span \
styling\n- [ ] cursor-gated markers\n";

fn main() {
    // Milestone-3 groundwork: prove the parse step works and is cheap enough
    // to consider running per keystroke (it is — pulldown-cmark is a pull
    // parser; a note-sized document parses in microseconds).
    let spans = markdown_spans(SAMPLE);
    eprintln!("[grain-editor] sample parse: {} spans", spans.len());

    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    v_stack((
        label(|| "Grain Editor — Floem scaffold".to_string()).style(|s| {
            s.padding(8.0)
                .width_full()
                .font_size(13.0)
                .color(Color::rgb8(160, 160, 160))
        }),
        text_editor(SAMPLE)
            .editor_style(|s| s.indent_style(IndentStyle::Spaces(2)))
            .style(|s| s.size_full()),
    ))
    .style(|s| s.size_full().flex_col())
}

/// The decoration-layer input: every markdown construct as a byte range +
/// tag over the raw text. Milestone 3 maps these to text-layout attributes;
/// milestone 4 hides the syntax-marker sub-ranges unless the cursor is on
/// that line.
fn markdown_spans(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    use pulldown_cmark::{Event, Options, Parser};
    let mut spans = Vec::new();
    for (event, range) in Parser::new_ext(text, Options::all()).into_offset_iter() {
        if let Event::Start(tag) = event {
            spans.push((range, format!("{tag:?}")));
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_parses_into_spans() {
        let spans = markdown_spans(SAMPLE);
        assert!(spans.iter().any(|(_, t)| t.contains("Heading")));
        assert!(spans.iter().any(|(_, t)| t.contains("Strong")));
        assert!(spans.iter().any(|(_, t)| t.contains("List")));
    }
}
