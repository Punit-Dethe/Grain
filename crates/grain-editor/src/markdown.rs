//! [GRAIN] Markdown → decoration spans (EXECUTION-PLAN.md P4 milestone 2/3
//! groundwork). The editor's live-preview layer will map these ranges onto
//! text-layout attributes; for now the parse step exists and is proven fast.

/// Every markdown construct as a byte range + tag over the raw text.
/// Not wired into the editor yet — milestone 2 consumes it.
#[allow(dead_code)]
pub fn markdown_spans(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
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
        let sample = "# Head\n\nSome **bold** text.\n\n- item\n";
        let spans = markdown_spans(sample);
        assert!(spans.iter().any(|(_, t)| t.contains("Heading")));
        assert!(spans.iter().any(|(_, t)| t.contains("Strong")));
        assert!(spans.iter().any(|(_, t)| t.contains("List")));
    }
}
