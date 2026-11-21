//! Compact text versions of the `query` results, for Grain's Agent.
//!
//! External AI apps get the JSON as structured content. The Agent's model may
//! be a small local one, so it gets short headers and `[mm:ss] Speaker N: …`
//! lines instead. Rendering works from the same JSON, so both see the same
//! answers.

use serde_json::Value;

fn s<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

/// "2026-11-10T17:30:00+05:30" → "2026-11-10 17:30".
fn when(value: &Value) -> String {
    let raw = value.as_str().unwrap_or("unknown time");
    match raw.get(..16) {
        Some(head) if raw.len() > 16 && raw.as_bytes()[10] == b'T' => head.replacen('T', " ", 1),
        _ => raw.to_string(),
    }
}

fn more(next: &Value, what: &str) -> String {
    next.as_u64()
        .map(|n| format!("\nMore {what}: call again with offset {n}."))
        .unwrap_or_default()
}

pub fn list(result: &Value) -> String {
    let meetings = result["meetings"].as_array().map_or(&[][..], Vec::as_slice);
    if meetings.is_empty() {
        return "No meetings match.".into();
    }
    let mut out = format!("{} meeting(s):", result["total"]);
    for m in meetings {
        let summary = if m["has_summary"] == true {
            ", summary"
        } else {
            ""
        };
        out.push_str(&format!(
            "\n- {} | {} | {} | {}{} | id {}",
            s(m, "title"),
            when(&m["started_at"]),
            s(m, "duration"),
            s(m, "status").replace('_', " "),
            summary,
            s(m, "id"),
        ));
    }
    out + &more(&result["next_offset"], "meetings")
}

pub fn search(result: &Value) -> String {
    let hits = result["hits"].as_array().map_or(&[][..], Vec::as_slice);
    let titles = result["title_matches"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    if hits.is_empty() && titles.is_empty() {
        return "Nothing in the meetings matches.".into();
    }
    let mut out = String::new();
    let mut current = "";
    for hit in hits {
        let id = s(hit, "meeting_id");
        if id != current {
            current = id;
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!(
                "{} ({}) id {}",
                s(hit, "title"),
                when(&hit["started_at"]),
                id
            ));
        }
        if let Some(before) = hit["before"].as_str() {
            out.push_str(&format!("\n    {before}"));
        }
        out.push_str(&format!(
            "\n  > [{}] {}: {}",
            s(hit, "at"),
            s(hit, "speaker"),
            s(hit, "text")
        ));
        if let Some(after) = hit["after"].as_str() {
            out.push_str(&format!("\n    {after}"));
        }
    }
    if !titles.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("Titles that match:");
        for t in titles {
            out.push_str(&format!(
                "\n- {} ({}) id {}",
                s(t, "title"),
                when(&t["started_at"]),
                s(t, "id")
            ));
        }
    }
    if let Some(hint) = result["hint"].as_str() {
        out.push_str(&format!("\n\n{hint}"));
    }
    out
}

pub fn meeting(result: &Value) -> String {
    let mut out = format!(
        "{}\nid {}\n{} to {} ({}), {}",
        s(result, "title"),
        s(result, "id"),
        when(&result["started_at"]),
        when(&result["ended_at"]),
        s(result, "duration"),
        s(result, "status").replace('_', " "),
    );
    let transcript = &result["transcript"];
    if transcript.is_object() {
        let speakers: Vec<&str> = transcript["speakers"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        out.push_str(&format!(
            "\nTranscript: {} lines, speakers: {}",
            transcript["lines"],
            if speakers.is_empty() {
                "none".into()
            } else {
                speakers.join(", ")
            },
        ));
        for note in transcript["notes"].as_array().into_iter().flatten() {
            out.push_str(&format!("\nNote: {}", note.as_str().unwrap_or("")));
        }
    }
    let summary = &result["summary"];
    if summary.is_object() {
        let stale = if summary["matches_latest_transcript"] == false {
            " (written for an older transcript)"
        } else {
            ""
        };
        out.push_str(&format!("\n\nSummary{stale}:\n{}", s(summary, "markdown")));
    } else {
        out.push_str("\n\nNo summary.");
    }
    out
}

pub fn transcript(result: &Value) -> String {
    let offset = result["offset"].as_u64().unwrap_or(0);
    let returned = result["returned"].as_u64().unwrap_or(0);
    if returned == 0 {
        return format!(
            "{}: no transcript lines in that range ({} lines in all).",
            s(result, "title"),
            result["total_lines"]
        );
    }
    format!(
        "{} lines {}-{} of {}:\n{}{}",
        s(result, "title"),
        offset + 1,
        offset + returned,
        result["total_lines"],
        s(result, "text"),
        more(&result["next_offset"], "lines"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Fixture;
    use crate::query::{self, ListArgs, MeetingArgs, SearchArgs, TranscriptArgs};

    #[test]
    fn renders_each_result_compactly() {
        let f = Fixture::new();
        let root = f.root();
        let id = f.meeting(
            "Standup",
            1_794_312_000_000,
            60,
            Some(&[
                (0, 0, "Morning all."),
                (5_000, 1, "We decided to ship the beta Friday."),
            ]),
        );
        f.summary(&id, 1, "Ship Friday.");

        let listed = list(&query::list_meetings(&root, ListArgs::default()).unwrap());
        assert!(listed.starts_with("1 meeting(s):\n- Standup | 2026-11-"));
        assert!(listed.ends_with(&format!("01:00 | transcribed, summary | id {id}")));

        let found = search(
            &query::search_meetings(
                &root,
                SearchArgs {
                    query: "beta".into(),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        assert!(found.contains("\n    [00:00] Speaker 1: Morning all."));
        assert!(found.contains("\n  > [00:05] Speaker 2: We decided to ship the beta Friday."));

        let one = meeting(
            &query::get_meeting(
                &root,
                MeetingArgs {
                    meeting_id: id.clone(),
                    include_summary: None,
                },
            )
            .unwrap(),
        );
        assert!(one.contains("Transcript: 2 lines, speakers: Speaker 1, Speaker 2"));
        assert!(one.ends_with("Summary:\nShip Friday."));

        let page = transcript(
            &query::get_transcript(
                &root,
                TranscriptArgs {
                    meeting_id: id,
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        assert_eq!(
            page,
            "Standup lines 1-1 of 2:\n[00:00] Speaker 1: Morning all.\nMore lines: call again with offset 1."
        );
    }

    #[test]
    fn empty_results_say_so() {
        let f = Fixture::new();
        let root = f.root();
        assert_eq!(
            list(&query::list_meetings(&root, ListArgs::default()).unwrap()),
            "No meetings match."
        );
        assert_eq!(
            search(
                &query::search_meetings(
                    &root,
                    SearchArgs {
                        query: "x".into(),
                        ..Default::default()
                    }
                )
                .unwrap()
            ),
            "Nothing in the meetings matches."
        );
    }
}
