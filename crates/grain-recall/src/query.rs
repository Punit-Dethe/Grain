//! Recall queries (list / search / get meeting / get transcript) and the two
//! resource readers, over a meetings folder. The MCP sidecar and Grain's Agent
//! both call these, so they always give the same answers.
//!
//! Results are bounded: listings page at 50, search returns at most 30
//! passages, a transcript page holds at most 500 lines and ~40k characters.

use std::path::{Path, PathBuf};

use crate::manifest::{self, Manifest, SessionState, SessionSummary, TrackSource};
use crate::summary;
use crate::text::{self, Line};
use crate::transcript::{self, TrackStatus, TranscriptRevision};
use chrono::{DateTime, Days, Local, NaiveDate, SecondsFormat, TimeZone};
use serde::Deserialize;
use serde_json::{json, Value};

const LIST_DEFAULT: usize = 20;
const LIST_MAX: usize = 50;
const SEARCH_DEFAULT: usize = 10;
const SEARCH_MAX: usize = 30;
const PAGE_DEFAULT: usize = 200;
const PAGE_MAX: usize = 500;
/// A transcript page stops early once it holds this many characters.
const PAGE_CHAR_BUDGET: usize = 40_000;
const HIT_CHARS: usize = 600;
const CONTEXT_CHARS: usize = 300;
const MAX_QUERY_CHARS: usize = 500;

pub type ToolResult = Result<Value, String>;

// --- Arguments ----------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct ListArgs {
    pub query: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchArgs {
    pub query: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub meeting_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MeetingArgs {
    pub meeting_id: String,
    pub include_summary: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TranscriptArgs {
    pub meeting_id: String,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub from: Option<String>,
    pub to: Option<String>,
}

// --- Shared helpers -----------------------------------------------------------

fn iso(unix_ms: u64) -> Value {
    i64::try_from(unix_ms)
        .ok()
        .and_then(|ms| Local.timestamp_millis_opt(ms).single())
        .map_or(Value::Null, |time| {
            Value::String(time.to_rfc3339_opts(SecondsFormat::Secs, false))
        })
}

fn capped(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn status(state: SessionState, revision: Option<u32>) -> &'static str {
    match (state, revision) {
        (SessionState::Recording, _) => "recording",
        (_, Some(_)) => "transcribed",
        (_, None) => "not_transcribed",
    }
}

/// A meeting-clock or calendar range, inclusive at both ends.
#[derive(Debug, Default, Clone, Copy)]
struct Range {
    from_ms: Option<u64>,
    to_ms: Option<u64>,
}

impl Range {
    fn parse(from: Option<&str>, to: Option<&str>) -> Result<Self, String> {
        let range = Self {
            from_ms: from.map(|v| date_bound(v, false)).transpose()?,
            to_ms: to.map(|v| date_bound(v, true)).transpose()?,
        };
        if let (Some(from), Some(to)) = (range.from_ms, range.to_ms) {
            if from > to {
                return Err("`from` is after `to`".into());
            }
        }
        Ok(range)
    }

    fn contains(&self, ms: u64) -> bool {
        self.from_ms.is_none_or(|from| ms >= from) && self.to_ms.is_none_or(|to| ms <= to)
    }
}

/// `YYYY-MM-DD` (a whole day in the user's time zone) or an RFC 3339 time.
fn date_bound(value: &str, end: bool) -> Result<u64, String> {
    let value = value.trim();
    if let Ok(time) = DateTime::parse_from_rfc3339(value) {
        return u64::try_from(time.timestamp_millis())
            .map_err(|_| format!("`{value}` is too early"));
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("`{value}` is not a date; use YYYY-MM-DD"))?;
    let day = if end {
        date.checked_add_days(Days::new(1)).unwrap_or(date)
    } else {
        date
    };
    let start = day
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| Local.from_local_datetime(&naive).earliest())
        .ok_or_else(|| format!("`{value}` is not a valid local date"))?;
    let ms =
        u64::try_from(start.timestamp_millis()).map_err(|_| format!("`{value}` is too early"))?;
    Ok(if end { ms.saturating_sub(1) } else { ms })
}

/// "mm:ss", "h:mm:ss" or plain seconds, as meeting-clock milliseconds.
fn clock_ms(value: &str) -> Result<u64, String> {
    let bad = || format!("`{value}` is not a time; use mm:ss or h:mm:ss");
    let mut seconds: u64 = 0;
    let parts: Vec<&str> = value.trim().split(':').collect();
    if parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(bad());
    }
    for part in parts {
        let n: u64 = part.parse().map_err(|_| bad())?;
        seconds = seconds
            .checked_mul(60)
            .and_then(|s| s.checked_add(n))
            .ok_or_else(bad)?;
    }
    Ok(seconds.saturating_mul(1_000))
}

fn unknown(id: &str) -> String {
    format!("No meeting with id `{id}`. Use list_meetings or search_meetings to find meeting ids.")
}

struct Opened {
    dir: PathBuf,
    manifest: Manifest,
}

fn open(root: &Path, id: &str) -> Result<Opened, String> {
    let id = id.trim();
    let dir = manifest::session_dir(root, id).map_err(|_| unknown(id))?;
    if !dir.is_dir() {
        return Err(unknown(id));
    }
    let manifest = manifest::load_manifest(&dir)?;
    Ok(Opened { dir, manifest })
}

fn transcript_of(opened: &Opened) -> Result<TranscriptRevision, String> {
    match transcript::latest(&opened.dir)? {
        Some(revision) => Ok(revision),
        None if opened.manifest.state == SessionState::Recording => {
            Err("This meeting is still being recorded; it has no transcript yet.".into())
        }
        None => Err("This meeting has not been transcribed yet. The user can transcribe it in Grain's Recall page.".into()),
    }
}

fn sessions(root: &Path, range: Range) -> Result<Vec<SessionSummary>, String> {
    Ok(manifest::list_sessions(root)?
        .into_iter()
        .filter(|s| range.contains(s.created_unix_ms))
        .collect())
}

/// Lowercased search terms, edge punctuation trimmed, duplicates dropped.
fn terms(query: &str) -> Vec<String> {
    let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
    let mut out: Vec<String> = Vec::new();
    for raw in query.to_lowercase().split_whitespace() {
        let term = raw.trim_matches(|c: char| !c.is_alphanumeric());
        if !term.is_empty() && !out.iter().any(|t| t == term) {
            out.push(term.to_string());
        }
    }
    out
}

/// `term` appears at the start of a word in `hay` (so "decid" finds "decided"
/// but "art" does not find "start"). Both are lowercase.
fn has_word(hay: &str, term: &str) -> bool {
    hay.match_indices(term).any(|(at, _)| {
        hay[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric())
    })
}

// --- Tools ----------------------------------------------------------------------

pub fn list_meetings(root: &Path, args: ListArgs) -> ToolResult {
    let range = Range::parse(args.from.as_deref(), args.to.as_deref())?;
    let query = args
        .query
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let limit = args.limit.unwrap_or(LIST_DEFAULT).clamp(1, LIST_MAX);
    let offset = args.offset.unwrap_or(0);
    let matching: Vec<SessionSummary> = sessions(root, range)?
        .into_iter()
        .filter(|s| {
            query
                .as_ref()
                .is_none_or(|q| s.title.to_lowercase().contains(q))
        })
        .collect();
    let page: Vec<Value> = matching
        .iter()
        .skip(offset)
        .take(limit)
        .map(|s| {
            let dir = root.join(&s.id);
            let revision = transcript::latest_revision(&dir).ok().flatten();
            json!({
                "id": s.id,
                "title": s.title,
                "started_at": iso(s.created_unix_ms),
                "duration": text::clock(s.duration_ms),
                "status": status(s.state, revision),
                "has_summary": dir.join(summary::SUMMARY_FILE).is_file(),
            })
        })
        .collect();
    let next = offset + page.len();
    Ok(json!({
        "meetings": page,
        "total": matching.len(),
        "next_offset": (next < matching.len()).then_some(next),
    }))
}

pub fn search_meetings(root: &Path, args: SearchArgs) -> ToolResult {
    let terms = terms(&args.query);
    if terms.is_empty() {
        return Err("Give a word or phrase to search for.".into());
    }
    let limit = args.limit.unwrap_or(SEARCH_DEFAULT).clamp(1, SEARCH_MAX);
    let candidates: Vec<(String, String, u64)> = match args.meeting_id.as_deref() {
        Some(id) => {
            let opened = open(root, id)?;
            vec![(
                opened.manifest.id.clone(),
                opened.manifest.title.clone(),
                opened.manifest.created_unix_ms,
            )]
        }
        None => sessions(
            root,
            Range::parse(args.from.as_deref(), args.to.as_deref())?,
        )?
        .into_iter()
        .map(|s| (s.id, s.title, s.created_unix_ms))
        .collect(),
    };

    let mut hits: Vec<Value> = Vec::new();
    let mut title_matches: Vec<Value> = Vec::new();
    let mut truncated = false;
    'meetings: for (id, title, created) in &candidates {
        let lower_title = title.to_lowercase();
        if terms.iter().all(|t| has_word(&lower_title, t)) {
            title_matches.push(json!({ "id": id, "title": title, "started_at": iso(*created) }));
        }
        let dir = root.join(id);
        let Ok(Some(revision)) = transcript::latest(&dir) else {
            continue;
        };
        let lines: Vec<Line> = text::lines(&revision);
        let lower: Vec<String> = lines.iter().map(|l| l.text.to_lowercase()).collect();
        for (i, line) in lines.iter().enumerate() {
            if !terms.iter().any(|t| has_word(&lower[i], t)) {
                continue;
            }
            let before = i.checked_sub(1);
            let after = (i + 1 < lines.len()).then_some(i + 1);
            let near = |t: &String| {
                [before, Some(i), after]
                    .into_iter()
                    .flatten()
                    .any(|j| has_word(&lower[j], t))
            };
            if !terms.iter().all(near) {
                continue;
            }
            if hits.len() == limit {
                truncated = true;
                break 'meetings;
            }
            hits.push(json!({
                "meeting_id": id,
                "title": title,
                "started_at": iso(*created),
                "at": text::clock(line.start_ms),
                "speaker": line.speaker,
                "text": capped(&line.text, HIT_CHARS),
                "before": before.map(|j| capped(&lines[j].render(), CONTEXT_CHARS)),
                "after": after.map(|j| capped(&lines[j].render(), CONTEXT_CHARS)),
            }));
        }
    }
    let mut result = json!({
        "hits": hits,
        "title_matches": title_matches,
        "truncated": truncated,
    });
    if truncated {
        result["hint"] = json!("More passages match. Narrow with from/to or meeting_id, or read around a hit with get_transcript.");
    }
    Ok(result)
}

fn source(source: &TrackSource) -> Value {
    match source {
        TrackSource::Microphone { device } => json!({ "kind": "microphone", "name": device }),
        TrackSource::Application { name } => json!({ "kind": "system_audio", "name": name }),
        TrackSource::Imported { name } => json!({ "kind": "imported", "name": name }),
    }
}

fn track_note(track: u32, status: &TrackStatus) -> Option<String> {
    match status {
        TrackStatus::Complete | TrackStatus::Empty => None,
        TrackStatus::SpeakersUnavailable { reason } => Some(format!(
            "track {track}: speakers could not be told apart ({reason})"
        )),
        TrackStatus::Failed { reason } => {
            Some(format!("track {track}: not transcribed ({reason})"))
        }
        TrackStatus::TooLong {
            minutes,
            limit_minutes,
        } => Some(format!(
            "track {track}: {minutes} min is over the {limit_minutes} min limit; not transcribed"
        )),
    }
}

pub fn get_meeting(root: &Path, args: MeetingArgs) -> ToolResult {
    let opened = open(root, &args.meeting_id)?;
    let m = &opened.manifest;
    let revision = transcript::latest(&opened.dir)?;
    let duration_ms = m.duration_ns() / 1_000_000;
    let transcript = revision.as_ref().map(|r| {
        let notes: Vec<String> = r
            .tracks
            .iter()
            .filter_map(|t| track_note(t.track, &t.status))
            .collect();
        json!({
            "revision": r.revision,
            "lines": text::lines(r).len(),
            "transcribed_at": iso(r.created_unix_ms),
            "speakers": r.speakers.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
            "notes": notes,
        })
    });
    let summary = if args.include_summary.unwrap_or(true) {
        summary::read_summary(&opened.dir).map(|doc| {
            json!({
                "markdown": doc.text,
                "written_at": iso(doc.created_unix_ms),
                "matches_latest_transcript": revision.as_ref().is_some_and(|r| r.revision == doc.transcript_revision),
            })
        })
    } else {
        None
    };
    Ok(json!({
        "id": m.id,
        "title": m.title,
        "started_at": iso(m.created_unix_ms),
        "ended_at": iso(m.created_unix_ms.saturating_add(duration_ms)),
        "duration": text::clock(duration_ms),
        "status": status(m.state, revision.as_ref().map(|r| r.revision)),
        "sources": m.tracks.iter().map(|t| source(&t.source)).collect::<Vec<_>>(),
        "transcript": transcript,
        "summary": summary,
    }))
}

pub fn get_transcript(root: &Path, args: TranscriptArgs) -> ToolResult {
    let opened = open(root, &args.meeting_id)?;
    let revision = transcript_of(&opened)?;
    let from = args.from.as_deref().map(clock_ms).transpose()?;
    let to = args.to.as_deref().map(clock_ms).transpose()?;
    let lines: Vec<Line> = text::lines(&revision)
        .into_iter()
        .filter(|l| from.is_none_or(|f| l.end_ms >= f) && to.is_none_or(|t| l.start_ms <= t))
        .collect();
    let offset = args.offset.unwrap_or(0);
    let limit = args.limit.unwrap_or(PAGE_DEFAULT).clamp(1, PAGE_MAX);
    let mut out = String::new();
    let mut returned = 0;
    for line in lines.iter().skip(offset).take(limit) {
        let rendered = line.render();
        if returned > 0 && out.len() + rendered.len() > PAGE_CHAR_BUDGET {
            break;
        }
        if returned > 0 {
            out.push('\n');
        }
        out.push_str(&rendered);
        returned += 1;
    }
    let next = offset + returned;
    Ok(json!({
        "meeting_id": opened.manifest.id,
        "title": opened.manifest.title,
        "total_lines": lines.len(),
        "offset": offset,
        "returned": returned,
        "text": out,
        "next_offset": (next < lines.len()).then_some(next),
    }))
}

// --- Resources ------------------------------------------------------------------

/// Meetings offered as attachable resources, newest first.
pub fn recent(root: &Path, max: usize) -> Result<Vec<SessionSummary>, String> {
    let mut all = sessions(root, Range::default())?;
    all.truncate(max);
    Ok(all)
}

/// A meeting as Markdown: header facts plus its summary.
pub fn meeting_markdown(root: &Path, id: &str) -> Result<String, String> {
    let opened = open(root, id)?;
    let m = &opened.manifest;
    let revision = transcript::latest_revision(&opened.dir)?;
    let mut out = format!(
        "# {}\n\nStarted {}, {} long, {}.\n\n## Summary\n\n",
        m.title,
        iso(m.created_unix_ms)
            .as_str()
            .unwrap_or("at an unknown time"),
        text::clock(m.duration_ns() / 1_000_000),
        status(m.state, revision).replace('_', " "),
    );
    match summary::read_summary(&opened.dir) {
        Some(doc) => out.push_str(&doc.text),
        None => out.push_str("No summary has been written for this meeting."),
    }
    Ok(out)
}

/// The full transcript as "[mm:ss] Speaker N: text" lines.
pub fn transcript_text(root: &Path, id: &str) -> Result<String, String> {
    let opened = open(root, id)?;
    Ok(text::transcript_text(&transcript_of(&opened)?))
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
