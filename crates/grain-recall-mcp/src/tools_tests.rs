use super::*;
use crate::fixture::Fixture;

const DAY: u64 = 86_400_000;
/// 2026-11-10 12:00 UTC: midday everywhere from UTC-11 to UTC+11.
const NOV_10: u64 = 1_794_312_000_000;

fn standup(f: &Fixture) -> String {
    f.meeting(
        "Weekly standup",
        NOV_10,
        600,
        Some(&[
            (0, 0, "Morning everyone, quick standup today."),
            (5_000, 1, "We decided to ship the beta on Friday."),
            (10_000, 0, "Great. Who owns the release notes?"),
            (15_000, 1, "I will write the release notes."),
        ]),
    )
}

fn list(r: &Recall, args: ListArgs) -> Value {
    list_meetings(r, args).unwrap()
}

fn search(r: &Recall, query: &str) -> Value {
    search_meetings(
        r,
        SearchArgs {
            query: query.into(),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn list_pages_newest_first_and_filters_by_title_and_day() {
    let f = Fixture::new();
    let r = f.recall();
    standup(&f);
    f.meeting("Design review", NOV_10 + DAY, 120, None);
    f.meeting("Budget call", NOV_10 + 2 * DAY, 60, None);

    let all = list(
        &r,
        ListArgs {
            limit: Some(2),
            ..Default::default()
        },
    );
    assert_eq!(all["total"], 3);
    assert_eq!(all["meetings"][0]["title"], "Budget call");
    assert_eq!(all["meetings"][1]["status"], "not_transcribed");
    assert_eq!(all["next_offset"], 2);

    let rest = list(
        &r,
        ListArgs {
            offset: Some(2),
            ..Default::default()
        },
    );
    assert_eq!(rest["meetings"][0]["title"], "Weekly standup");
    assert_eq!(rest["meetings"][0]["status"], "transcribed");
    assert_eq!(rest["meetings"][0]["duration"], "10:00");
    assert_eq!(rest["next_offset"], Value::Null);

    let by_title = list(
        &r,
        ListArgs {
            query: Some("REVIEW".into()),
            ..Default::default()
        },
    );
    assert_eq!(by_title["total"], 1);

    let day = list(
        &r,
        ListArgs {
            from: Some("2026-11-11".into()),
            to: Some("2026-11-11".into()),
            ..Default::default()
        },
    );
    assert_eq!(day["total"], 1);
    assert_eq!(day["meetings"][0]["title"], "Design review");

    let vague = ListArgs {
        from: Some("last week".into()),
        ..Default::default()
    };
    assert!(list_meetings(&r, vague).is_err());
    let backwards = ListArgs {
        from: Some("2026-11-12".into()),
        to: Some("2026-11-10".into()),
        ..Default::default()
    };
    assert!(list_meetings(&r, backwards).is_err());
}

#[test]
fn search_finds_passages_with_context_and_word_starts() {
    let f = Fixture::new();
    let r = f.recall();
    let id = standup(&f);
    f.meeting(
        "Release planning",
        NOV_10 - DAY,
        60,
        Some(&[(0, 0, "Nothing about it here.")]),
    );

    let found = search(&r, "decid beta");
    let hits = found["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["meeting_id"], id.as_str());
    assert_eq!(hits[0]["at"], "00:05");
    assert_eq!(hits[0]["speaker"], "Speaker 2");
    assert_eq!(
        hits[0]["before"],
        "[00:00] Speaker 1: Morning everyone, quick standup today."
    );

    // A term only matches at a word start; terms on adjacent lines match together.
    assert!(search(&r, "andup")["hits"].as_array().unwrap().is_empty());
    assert!(!search(&r, "owns write")["hits"]
        .as_array()
        .unwrap()
        .is_empty());

    assert_eq!(
        search(&r, "release")["title_matches"][0]["title"],
        "Release planning"
    );

    let capped = search_meetings(
        &r,
        SearchArgs {
            query: "release notes".into(),
            limit: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(capped["truncated"], true);
    assert!(capped["hint"].is_string());

    let empty = SearchArgs {
        query: " ?! ".into(),
        ..Default::default()
    };
    assert!(search_meetings(&r, empty).is_err());
}

#[test]
fn get_meeting_reports_facts_and_summary_freshness() {
    let f = Fixture::new();
    let r = f.recall();
    let id = standup(&f);
    f.summary(&id, 1, "## Overview\nShip Friday.");
    let get = |include: Option<bool>| {
        get_meeting(
            &r,
            MeetingArgs {
                meeting_id: id.clone(),
                include_summary: include,
            },
        )
        .unwrap()
    };

    let m = get(None);
    assert_eq!(m["title"], "Weekly standup");
    assert_eq!(m["sources"][0]["kind"], "microphone");
    assert_eq!(m["transcript"]["lines"], 4);
    assert_eq!(
        m["transcript"]["speakers"],
        json!(["Speaker 1", "Speaker 2"])
    );
    assert_eq!(m["summary"]["matches_latest_transcript"], true);
    assert!(m["summary"]["markdown"]
        .as_str()
        .unwrap()
        .contains("Ship Friday"));

    f.summary(&id, 0, "old");
    assert_eq!(
        get(Some(true))["summary"]["matches_latest_transcript"],
        false
    );
    assert_eq!(get(Some(false))["summary"], Value::Null);
}

#[test]
fn transcript_pages_and_time_windows() {
    let f = Fixture::new();
    let r = f.recall();
    let id = standup(&f);
    let read = |args: TranscriptArgs| get_transcript(&r, args);

    let page = read(TranscriptArgs {
        meeting_id: id.clone(),
        limit: Some(2),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(page["total_lines"], 4);
    assert_eq!(page["returned"], 2);
    assert_eq!(page["next_offset"], 2);
    assert!(page["text"]
        .as_str()
        .unwrap()
        .starts_with("[00:00] Speaker 1: Morning"));

    let window = read(TranscriptArgs {
        meeting_id: id.clone(),
        from: Some("0:09".into()),
        to: Some("00:12".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(window["total_lines"], 1);
    assert_eq!(
        window["text"],
        "[00:10] Speaker 1: Great. Who owns the release notes?"
    );

    let bad_time = TranscriptArgs {
        meeting_id: id,
        from: Some("ten".into()),
        ..Default::default()
    };
    assert!(read(bad_time).is_err());

    let silent = f.meeting("Untranscribed", NOV_10, 30, None);
    let err = read(TranscriptArgs {
        meeting_id: silent,
        ..Default::default()
    })
    .unwrap_err();
    assert!(err.contains("not been transcribed"));
}

#[test]
fn ids_are_validated_before_touching_the_disk() {
    let f = Fixture::new();
    let r = f.recall();
    let missing = uuid::Uuid::new_v4().to_string();
    for bad in ["../meetings", "..\\..\\x", "not-a-uuid", missing.as_str()] {
        let err = get_meeting(
            &r,
            MeetingArgs {
                meeting_id: bad.into(),
                include_summary: None,
            },
        )
        .unwrap_err();
        assert!(err.contains("No meeting with id"), "{bad}: {err}");
    }
}

#[test]
fn resources_render_summary_and_transcript() {
    let f = Fixture::new();
    let r = f.recall();
    let id = standup(&f);
    let md = meeting_markdown(&r, &id).unwrap();
    assert!(md.starts_with("# Weekly standup"));
    assert!(md.contains("No summary has been written"));
    assert_eq!(transcript_text(&r, &id).unwrap().lines().count(), 4);
    assert_eq!(recent(&r, 10).unwrap().len(), 1);
}

#[test]
fn clock_parsing() {
    assert_eq!(clock_ms("90").unwrap(), 90_000);
    assert_eq!(clock_ms("1:30").unwrap(), 90_000);
    assert_eq!(clock_ms("1:00:05").unwrap(), 3_605_000);
    assert!(clock_ms("1::2").is_err());
    assert!(clock_ms("1:2:3:4").is_err());
}
