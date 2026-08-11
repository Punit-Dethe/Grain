//! Canonical text normalization for model-agnostic rolling assembly.
//!
//! ASR models disagree about sentence punctuation and capitalization. Rolling
//! treats those as presentation, not lexical evidence: chunks are lowercased
//! and stripped of sentence formatting before overlap reconciliation. Internal
//! structure in machine tokens (URLs, paths, identifiers, versions) is kept so
//! code dictation is not irreversibly damaged before optional LLM formatting.

/// Normalize a chunk transcript into the canonical rolling representation.
pub fn canonicalize_text(text: &str) -> String {
    text.split_whitespace()
        .flat_map(canonicalize_token)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalize one model token. A token may become multiple lexical words when
/// punctuation glued them together (for example `done.Next`).
pub fn canonicalize_token(token: &str) -> Vec<String> {
    let token = token.trim_matches(is_edge_presentation);
    if token.is_empty() {
        return Vec::new();
    }

    if looks_like_machine_token(token) {
        let normalized = token.to_lowercase();
        return (!normalized.is_empty())
            .then_some(normalized)
            .into_iter()
            .collect();
    }

    let chars: Vec<char> = token.chars().collect();
    let mut normalized = String::with_capacity(token.len());
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch.is_alphanumeric() {
            normalized.extend(ch.to_lowercase());
            continue;
        }

        let joins_lexeme = matches!(ch, '\'' | '’' | '-')
            && index > 0
            && index + 1 < chars.len()
            && chars[index - 1].is_alphanumeric()
            && chars[index + 1].is_alphanumeric();
        if joins_lexeme {
            normalized.push(ch);
        } else if !normalized.ends_with(' ') {
            // Presentation punctuation becomes a boundary rather than being
            // deleted, so `done.Next` cannot collapse into `donenext`.
            normalized.push(' ');
        }
    }

    normalized.split_whitespace().map(str::to_string).collect()
}

/// Aggressive comparison-only form used by overlap matching. Surface text may
/// retain machine-token structure; the match key deliberately ignores it.
pub fn comparison_key(token: &str) -> String {
    canonicalize_text(token)
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

fn is_edge_presentation(ch: char) -> bool {
    matches!(
        ch,
        '.' | ','
            | '!'
            | '?'
            | ';'
            | ':'
            | '…'
            | '。'
            | '，'
            | '！'
            | '？'
            | '；'
            | '：'
            | '"'
            | '“'
            | '”'
            | '‘'
            | '’'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
    )
}

fn looks_like_machine_token(token: &str) -> bool {
    if token.contains("://")
        || token.contains('@')
        || token.contains('/')
        || token.contains('\\')
        || token.contains('_')
        || token.contains('+')
        || token.contains('#')
        || token.starts_with('-')
    {
        return true;
    }

    let dotted: Vec<&str> = token.split('.').collect();
    if dotted.len() < 2 || dotted.iter().any(|part| part.is_empty()) {
        return false;
    }

    // Preserve decimals and ordinary lower-case domains/extensions. A capital
    // immediately after a dot is instead the common ASR `sentence.Next`
    // artifact and must remain splittable.
    let decimal = dotted
        .iter()
        .all(|part| part.chars().all(|ch| ch.is_ascii_digit()));
    let lower_suffix = dotted[1..].iter().all(|part| {
        part.chars().all(|ch| ch.is_ascii_alphanumeric())
            && part.chars().any(|ch| ch.is_ascii_lowercase())
            && !part.chars().any(|ch| ch.is_ascii_uppercase())
    });
    decimal || lower_suffix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_sentence_formatting_and_lowercases() {
        assert_eq!(canonicalize_text("Hello, World!"), "hello world");
        assert_eq!(canonicalize_text("This is Done."), "this is done");
        assert_eq!(canonicalize_text("One.Two.Three"), "one two three");
    }

    #[test]
    fn punctuation_only_tokens_disappear_without_spacing_artifacts() {
        assert_eq!(canonicalize_text("hello . , world ?"), "hello world");
        assert_eq!(canonicalize_text("[ hello ]"), "hello");
    }

    #[test]
    fn preserves_lexical_connectors() {
        assert_eq!(canonicalize_text("DON'T re-enter"), "don't re-enter");
    }

    #[test]
    fn preserves_machine_token_structure_but_not_terminal_sentence_marks() {
        assert_eq!(canonicalize_text("HTTPS://Grain.App."), "https://grain.app");
        assert_eq!(canonicalize_text("Test@Example.com,"), "test@example.com");
        assert_eq!(
            canonicalize_text("SRC-TAURI/src/Main.rs"),
            "src-tauri/src/main.rs"
        );
        assert_eq!(
            canonicalize_text("C++ --FLAG foo_bar"),
            "c++ --flag foo_bar"
        );
        assert_eq!(canonicalize_text("Version 3.14."), "version 3.14");
    }

    #[test]
    fn supports_unicode_and_caseless_scripts() {
        assert_eq!(canonicalize_text("CAFÉ。 APRÈS！"), "café après");
        assert_eq!(canonicalize_text("これは。次です"), "これは 次です");
    }

    #[test]
    fn comparison_key_ignores_retained_structure() {
        assert_eq!(comparison_key("Foo_Bar"), "foobar");
        assert_eq!(comparison_key("Hello,"), "hello");
    }
}
