//! [GRAIN] Markdown block codec for Grain Space documents (MEMORY-SYSTEM-PLAN.md Phase 2).
//!
//! Provides:
//! 1. Stable, human-readable block delimiter encoding using standard HTML comments:
//!    `<!-- grain:block id="..." kind="..." seq="..." recorded="..." -->`
//!    `... verbatim content ...`
//!    `<!-- /grain:block -->`
//! 2. Lossless block parsing that captures all user text, even if mixed with manual edits.
//! 3. Fallback for legacy / un-bracketed documents to a single whole-body block.
//! 4. Deterministic content hashing normalized across line endings.

use sha2::{Digest, Sha256};

use super::note::{MemoryBlock, MemoryBlockKind};

/// Canonical SHA-256 hash of markdown body content.
/// Normalizes CRLF and trailing whitespace per line so hashing is platform-invariant.
pub fn compute_content_hash(body: &str) -> String {
    let mut hasher = Sha256::new();
    for line in body.lines() {
        let trimmed = line.trim_end_matches('\r');
        hasher.update(trimmed.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

/// Parse attributes inside `<!-- grain:block key="value" ... -->`
fn parse_tag_attrs(tag_content: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let content = tag_content.trim();
    let s = content
        .strip_prefix("<!--")
        .unwrap_or(content)
        .strip_suffix("-->")
        .unwrap_or(content)
        .trim();
    let s = s.strip_prefix("grain:block").unwrap_or(s).trim();

    let mut chars = s.chars().peekable();
    while chars.peek().is_some() {
        // Skip whitespace
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
        // Read key
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }
        if key.is_empty() {
            break;
        }
        // Skip until '='
        while let Some(&c) = chars.peek() {
            chars.next();
            if c == '=' {
                break;
            }
        }
        // Skip whitespace
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
        // Read value (quoted or unquoted)
        let mut val = String::new();
        if let Some(&quote) = chars.peek() {
            if quote == '"' || quote == '\'' {
                chars.next(); // consume quote
                let mut escaped = false;
                for c in chars.by_ref() {
                    if escaped {
                        val.push(c);
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == quote {
                        break;
                    } else {
                        val.push(c);
                    }
                }
            } else {
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    val.push(c);
                    chars.next();
                }
            }
        }
        if !key.is_empty() {
            map.insert(key, val);
        }
    }
    map
}

fn parse_kind(s: &str) -> MemoryBlockKind {
    match s {
        "raw_capture" => MemoryBlockKind::RawCapture,
        "append" => MemoryBlockKind::Append,
        "transcript" => MemoryBlockKind::Transcript,
        "decision" => MemoryBlockKind::Decision,
        "action" => MemoryBlockKind::Action,
        "correction" => MemoryBlockKind::Correction,
        _ => MemoryBlockKind::Body,
    }
}

pub fn kind_str(k: MemoryBlockKind) -> &'static str {
    match k {
        MemoryBlockKind::RawCapture => "raw_capture",
        MemoryBlockKind::Append => "append",
        MemoryBlockKind::Transcript => "transcript",
        MemoryBlockKind::Decision => "decision",
        MemoryBlockKind::Action => "action",
        MemoryBlockKind::Correction => "correction",
        MemoryBlockKind::Body => "body",
    }
}

/// Render one memory block with HTML comment markers.
pub fn emit_block(block: &MemoryBlock) -> String {
    let mut tag = format!(
        "<!-- grain:block id=\"{}\" kind=\"{}\" seq=\"{}\" recorded=\"{}\"",
        block.id,
        kind_str(block.kind),
        block.sequence,
        block.recorded_at
    );
    if let Some(start) = block.event_start {
        tag.push_str(&format!(" start=\"{start}\""));
    }
    if let Some(end) = block.event_end {
        tag.push_str(&format!(" end=\"{end}\""));
    }
    if let Some(ref spk) = block.speaker {
        if !spk.is_empty() {
            tag.push_str(&format!(" speaker=\"{}\"", spk.replace('"', "\\\"")));
        }
    }
    if let Some(ref src) = block.source_ref {
        if !src.is_empty() {
            tag.push_str(&format!(" source=\"{}\"", src.replace('"', "\\\"")));
        }
    }
    if let Some(ref sup) = block.supersedes_block_id {
        if !sup.is_empty() {
            tag.push_str(&format!(" supersedes=\"{sup}\""));
        }
    }
    tag.push_str(" -->\n");
    let text = block.text.trim_matches(|c| c == '\r' || c == '\n');
    let close = "\n<!-- /grain:block -->\n";
    format!("{tag}{text}{close}")
}

/// Render a series of memory blocks separated by blank lines.
pub fn emit_blocks(blocks: &[MemoryBlock]) -> String {
    if blocks.is_empty() {
        return String::new();
    }
    // If there is only one block of type Body with default metadata, we can emit it clean
    // if desired, but for Schema V3 documents, emitting explicit blocks provides durability.
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&emit_block(b));
    }
    out
}

/// Parse a document's body into structured MemoryBlocks.
///
/// Guarantees:
/// 1. If no block markers exist, returns 1 synthetic block containing the entire body.
/// 2. If block markers exist, extracts all marked blocks.
/// 3. Any text outside block markers (e.g. hand-written notes before or between blocks)
///    is preserved into synthetic Body blocks so zero user text is lost.
pub fn parse_blocks(doc_id: &str, body: &str, default_recorded_at: i64) -> Vec<MemoryBlock> {
    const START_TAG: &str = "<!-- grain:block";
    const END_TAG: &str = "<!-- /grain:block -->";

    let trimmed = body.trim();
    if !trimmed.contains(START_TAG) {
        if trimmed.is_empty() {
            return Vec::new();
        }
        return vec![MemoryBlock {
            id: format!("{doc_id}-b0"),
            document_id: doc_id.to_string(),
            kind: MemoryBlockKind::Body,
            text: body.trim_end().to_string(),
            sequence: 0,
            recorded_at: default_recorded_at,
            event_start: None,
            event_end: None,
            source_ref: None,
            speaker: None,
            supersedes_block_id: None,
        }];
    }

    let mut blocks = Vec::new();
    let mut cursor = 0;
    let mut seq = 0u32;

    while cursor < body.len() {
        let Some(start_rel) = body[cursor..].find(START_TAG) else {
            // Remainder after last block
            let remainder = body[cursor..].trim();
            if !remainder.is_empty() {
                blocks.push(MemoryBlock {
                    id: format!("{doc_id}-b{seq}"),
                    document_id: doc_id.to_string(),
                    kind: MemoryBlockKind::Body,
                    text: remainder.to_string(),
                    sequence: seq,
                    recorded_at: default_recorded_at,
                    event_start: None,
                    event_end: None,
                    source_ref: None,
                    speaker: None,
                    supersedes_block_id: None,
                });
            }
            break;
        };

        let start_pos = cursor + start_rel;

        // Content before this block
        let prefix = body[cursor..start_pos].trim();
        if !prefix.is_empty() {
            blocks.push(MemoryBlock {
                id: format!("{doc_id}-b{seq}"),
                document_id: doc_id.to_string(),
                kind: MemoryBlockKind::Body,
                text: prefix.to_string(),
                sequence: seq,
                recorded_at: default_recorded_at,
                event_start: None,
                event_end: None,
                source_ref: None,
                speaker: None,
                supersedes_block_id: None,
            });
            seq += 1;
        }

        // Find end of the start tag `-->`
        let Some(tag_end_rel) = body[start_pos..].find("-->") else {
            // Malformed unclosed start tag; take rest of body
            let remainder = body[start_pos..].trim();
            blocks.push(MemoryBlock {
                id: format!("{doc_id}-b{seq}"),
                document_id: doc_id.to_string(),
                kind: MemoryBlockKind::Body,
                text: remainder.to_string(),
                sequence: seq,
                recorded_at: default_recorded_at,
                event_start: None,
                event_end: None,
                source_ref: None,
                speaker: None,
                supersedes_block_id: None,
            });
            break;
        };

        let tag_content = &body[start_pos..start_pos + tag_end_rel + 3];
        let attrs = parse_tag_attrs(tag_content);

        let content_start = start_pos + tag_end_rel + 3;

        // Find closing tag `<!-- /grain:block -->`
        let (content_end, next_cursor) = if let Some(end_rel) = body[content_start..].find(END_TAG) {
            (content_start + end_rel, content_start + end_rel + END_TAG.len())
        } else if let Some(next_start) = body[content_start..].find(START_TAG) {
            // Missing closing tag, but next block starts
            (content_start + next_start, content_start + next_start)
        } else {
            // Reaches end of string
            (body.len(), body.len())
        };

        let raw_block_text = body[content_start..content_end]
            .trim_matches(|c| c == '\r' || c == '\n')
            .to_string();

        let block_id = attrs
            .get("id")
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("{doc_id}-b{seq}"));
        let kind = attrs
            .get("kind")
            .map(|s| parse_kind(s))
            .unwrap_or(MemoryBlockKind::Body);
        let block_seq = attrs
            .get("seq")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(seq);
        let recorded_at = attrs
            .get("recorded")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(default_recorded_at);
        let event_start = attrs.get("start").and_then(|s| s.parse::<i64>().ok());
        let event_end = attrs.get("end").and_then(|s| s.parse::<i64>().ok());
        let speaker = attrs.get("speaker").cloned();
        let source_ref = attrs.get("source").cloned();
        let supersedes_block_id = attrs.get("supersedes").cloned();

        blocks.push(MemoryBlock {
            id: block_id,
            document_id: doc_id.to_string(),
            kind,
            text: raw_block_text,
            sequence: block_seq,
            recorded_at,
            event_start,
            event_end,
            source_ref,
            speaker,
            supersedes_block_id,
        });

        seq = block_seq + 1;
        cursor = next_cursor;
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic_crlf_lf() {
        let unix = "First line\nSecond line\n";
        let windows = "First line\r\nSecond line\r\n";
        assert_eq!(compute_content_hash(unix), compute_content_hash(windows));
        assert_eq!(compute_content_hash(unix).len(), 64);
    }

    #[test]
    fn test_legacy_body_without_markers_parses_to_single_block() {
        let doc_id = "doc-123";
        let body = "This is a simple note without any block delimiters.\nLine 2.";
        let blocks = parse_blocks(doc_id, body, 1725450000000);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, "doc-123-b0");
        assert_eq!(blocks[0].kind, MemoryBlockKind::Body);
        assert_eq!(blocks[0].text, body);
        assert_eq!(blocks[0].sequence, 0);
        assert_eq!(blocks[0].recorded_at, 1725450000000);
    }

    #[test]
    fn test_emit_and_parse_blocks_roundtrip() {
        let doc_id = "doc-roundtrip";
        let original_blocks = vec![
            MemoryBlock {
                id: "b-1".to_string(),
                document_id: doc_id.to_string(),
                kind: MemoryBlockKind::RawCapture,
                text: "Initial capture content with \"quotes\" and symbols: #tags, [brackets].".to_string(),
                sequence: 0,
                recorded_at: 1725400000000,
                event_start: Some(1725390000000),
                event_end: Some(1725393600000),
                source_ref: Some("voice".to_string()),
                speaker: Some("Alice".to_string()),
                supersedes_block_id: None,
            },
            MemoryBlock {
                id: "b-2".to_string(),
                document_id: doc_id.to_string(),
                kind: MemoryBlockKind::Append,
                text: "Appended followup paragraph.\nMulti-line note.".to_string(),
                sequence: 1,
                recorded_at: 1725450000000,
                event_start: None,
                event_end: None,
                source_ref: Some("mcp".to_string()),
                speaker: None,
                supersedes_block_id: Some("b-old".to_string()),
            },
        ];

        let emitted = emit_blocks(&original_blocks);
        assert!(emitted.contains("<!-- grain:block"));
        assert!(emitted.contains("Alice"));
        assert!(emitted.contains("b-old"));

        let parsed = parse_blocks(doc_id, &emitted, 0);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], original_blocks[0]);
        assert_eq!(parsed[1], original_blocks[1]);
    }

    #[test]
    fn test_mixed_manual_edits_preserved_without_loss() {
        let doc_id = "doc-mixed";
        let markdown = r#"Hand-written preamble at the top.

<!-- grain:block id="b-1" kind="raw_capture" seq="0" recorded="1000" -->
Original structured capture.
<!-- /grain:block -->

User added an un-delimited note in Obsidian in the middle.

<!-- grain:block id="b-2" kind="append" seq="1" recorded="2000" -->
Second block.
<!-- /grain:block -->

Trailing user postscript."#;

        let blocks = parse_blocks(doc_id, markdown, 500);
        assert_eq!(blocks.len(), 5);
        assert_eq!(blocks[0].text, "Hand-written preamble at the top.");
        assert_eq!(blocks[0].kind, MemoryBlockKind::Body);
        assert_eq!(blocks[1].text, "Original structured capture.");
        assert_eq!(blocks[1].kind, MemoryBlockKind::RawCapture);
        assert_eq!(blocks[2].text, "User added an un-delimited note in Obsidian in the middle.");
        assert_eq!(blocks[2].kind, MemoryBlockKind::Body);
        assert_eq!(blocks[3].text, "Second block.");
        assert_eq!(blocks[3].kind, MemoryBlockKind::Append);
        assert_eq!(blocks[4].text, "Trailing user postscript.");
        assert_eq!(blocks[4].kind, MemoryBlockKind::Body);
    }
}
