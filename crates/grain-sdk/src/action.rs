//! Shared action vocabulary for tool retrieval and evaluation.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActionRisk {
    /// Cheap and obvious when wrong: skip a track, open an app, set volume.
    Safe,
    /// Potentially consequential, such as sending, deleting, or spending.
    Confirm,
}

impl ActionRisk {
    pub fn is_safe(self) -> bool {
        matches!(self, ActionRisk::Safe)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActionParamKind {
    /// Something in the extension's own data: an artist, a contact, a playlist.
    Entity,
    /// Free text, passed through verbatim — a message body, a note.
    Text,
    /// A bare number: a volume, a count.
    Number,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UtterancePart {
    Literal(String),
    Param(String),
}

/// Split an utterance into literals and `{param}` placeholders.
pub fn parse_utterance(utterance: &str) -> Result<Vec<UtterancePart>, String> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = utterance;
    while let Some(open) = rest.find('{') {
        literal.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after
            .find('}')
            .ok_or_else(|| format!("unclosed '{{' in \"{utterance}\""))?;
        let name = &after[..close];
        if name.is_empty() {
            return Err(format!("empty placeholder in \"{utterance}\""));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(format!(
                "placeholder '{{{name}}}' must be lowercase letters, digits or '_'"
            ));
        }
        if !literal.trim().is_empty() {
            parts.push(UtterancePart::Literal(literal.trim().to_string()));
        }
        literal.clear();
        parts.push(UtterancePart::Param(name.to_string()));
        rest = &after[close + 1..];
    }
    if rest.contains('}') {
        return Err(format!("unmatched '}}' in \"{utterance}\""));
    }
    literal.push_str(rest);
    if !literal.trim().is_empty() {
        parts.push(UtterancePart::Literal(literal.trim().to_string()));
    }
    Ok(parts)
}
