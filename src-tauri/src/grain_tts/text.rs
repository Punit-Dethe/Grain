//! Agent replies as speakable text: markdown in, plain sentences out.

/// Hard cap on what is read aloud; longer replies stop at a sentence end.
pub(super) const MAX_SPOKEN_CHARS: usize = 3000;

/// A line longer than this is not a short confirmation; it is not read.
const MAX_STEP_CHARS: usize = 80;

/// The line to read from a tool round's text, if it is one: its first line,
/// when short. (The caller leaves out a same-round final reply.)
pub(crate) fn step_line(text: &str) -> Option<String> {
    let line = text.trim().lines().next()?.trim();
    let count = line.chars().count();
    (count > 0 && count <= MAX_STEP_CHARS).then(|| line.to_string())
}

/// Turn a markdown chat reply into text worth hearing: code blocks, link
/// targets, URLs and markup are dropped; list items and lines get a pause.
pub(super) fn speakable(markdown: &str) -> String {
    let mut sentences: Vec<String> = Vec::new();
    let mut in_code = false;
    let mut skipped_code = false;
    for raw in markdown.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code = !in_code;
            skipped_code = true;
            continue;
        }
        if in_code || trimmed.is_empty() || is_rule(trimmed) {
            continue;
        }
        let line = strip_line_marker(trimmed);
        let line = plain_inline(line);
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        let ends_sentence = line.ends_with(['.', '!', '?', ':', ';']);
        sentences.push(if ends_sentence {
            line
        } else {
            format!("{line}.")
        });
    }
    if sentences.is_empty() && skipped_code {
        return "The code is in the reply.".into();
    }
    truncate_at_sentence(sentences.join(" "), MAX_SPOKEN_CHARS)
}

fn is_rule(line: &str) -> bool {
    line.len() >= 3
        && line
            .chars()
            .all(|c| matches!(c, '-' | '*' | '_' | '=' | ' '))
        || line.starts_with('|') && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// Drop heading, quote and bullet markers. Numbered lists keep their number.
fn strip_line_marker(line: &str) -> &str {
    let line = line
        .trim_start_matches('#')
        .trim_start_matches('>')
        .trim_start();
    for bullet in ["- [ ] ", "- [x] ", "- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return rest;
        }
    }
    line
}

/// Links keep their label, bare URLs become "link", markup characters go.
fn plain_inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.split(' ')
        .map(|word| {
            if word.starts_with("http://") || word.starts_with("https://") {
                "link".to_string()
            } else {
                let chars: Vec<char> = word
                    .chars()
                    .filter(|c| !matches!(c, '*' | '`' | '|' | '~'))
                    .collect();
                // `_emphasis_` goes; `snake_case` keeps its underscore.
                (0..chars.len())
                    .filter(|&i| {
                        chars[i] != '_'
                            || i > 0
                                && i + 1 < chars.len()
                                && chars[i - 1].is_alphanumeric()
                                && chars[i + 1].is_alphanumeric()
                    })
                    .map(|i| chars[i])
                    .collect()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_at_sentence(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    let cut: String = text.chars().take(max).collect();
    match cut.rfind(['.', '!', '?']) {
        Some(end) => cut[..=end].to_string(),
        None => cut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_becomes_plain_sentences() {
        let reply = "## Summary\n\n- **First** point\n- See [the docs](https://x.y/z)\n\nVisit https://example.com now";
        assert_eq!(
            speakable(reply),
            "Summary. First point. See the docs. Visit link now."
        );
    }

    #[test]
    fn code_blocks_are_not_read() {
        assert_eq!(
            speakable("Run this:\n```sh\nrm -rf x\n```\nDone!"),
            "Run this: Done!"
        );
        assert_eq!(
            speakable("```rust\nfn main() {}\n```"),
            "The code is in the reply."
        );
    }

    #[test]
    fn inline_markup_and_rules_go() {
        assert_eq!(
            speakable("Use `cargo` and _care_.\n---\n> quoted"),
            "Use cargo and care. quoted."
        );
        assert_eq!(speakable("| a | b |\n|---|---|\n| 1 | 2 |"), "a b. 1 2.");
    }

    #[test]
    fn long_replies_stop_at_a_sentence() {
        let text = "One two three. Four five six.".to_string();
        assert_eq!(truncate_at_sentence(text, 20), "One two three.");
    }

    #[test]
    fn only_a_short_first_line_is_spoken() {
        assert_eq!(
            step_line("  Checking your calendar.\nThen more.").as_deref(),
            Some("Checking your calendar.")
        );
        assert_eq!(step_line("   \n"), None);
        assert_eq!(step_line(&"word ".repeat(40)), None);
    }
}
