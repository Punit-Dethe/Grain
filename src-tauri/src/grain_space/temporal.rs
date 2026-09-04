//! [GRAIN] Deterministic host-side temporal query parsing and date range filtering.
//!
//! Evaluates time constraints over note timestamps (`created` / `timestamp`)
//! according to `docs/Grain Space 2.0/GRAIN-SPACE-IMPROVEMENT-PLAN.md` §8.
//!
//! Supported deterministic phrases:
//! - `today`, `yesterday`, `day before yesterday`
//! - `this week`, `last week` (Monday to Sunday)
//! - `this month`, `last month` (1st to last day of month)
//! - `last N days`, `past N days` (rolling window)
//! - explicit ISO dates (`YYYY-MM-DD`, `YYYY/MM/DD`)
//! - explicit month-day dates (e.g. `September 4, 2026`, `Aug 15`)
//! - explicit date ranges (`from X to Y`, `between X and Y`, `X to Y`)
//!
//! The parser takes an explicit clock and timezone, requiring zero LLM or
//! external network calls. Ambiguous phrases remain ordinary query text.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone};

/// An inclusive epoch millisecond range `[start_ms, end_ms]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl DateRange {
    pub fn new(start_ms: i64, end_ms: i64) -> Self {
        Self { start_ms, end_ms }
    }

    pub fn as_tuple(&self) -> (i64, i64) {
        (self.start_ms, self.end_ms)
    }

}

/// The result of extracting temporal expressions from a search query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalExtraction {
    /// The search query with the temporal phrase stripped, trimmed of dangling prepositions.
    pub clean_query: String,
    /// The resolved epoch millisecond date range, if a supported temporal phrase was found.
    pub range: Option<DateRange>,
    /// The exact text phrase that matched (e.g. "yesterday", "last week", "2026-09-04").
    pub phrase: Option<String>,
}

/// Helper to calculate epoch milliseconds for the start of a calendar day (00:00:00.000).
fn start_of_day<Tz: TimeZone>(date: NaiveDate, tz: &Tz) -> Option<i64> {
    let naive = date.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
    tz.from_local_datetime(&naive)
        .earliest()
        .or_else(|| tz.from_local_datetime(&naive).latest())
        .map(|dt| dt.timestamp_millis())
}

/// Helper to calculate epoch milliseconds for the end of a calendar day (23:59:59.999).
fn end_of_day<Tz: TimeZone>(date: NaiveDate, tz: &Tz) -> Option<i64> {
    let naive = date.and_time(NaiveTime::from_hms_milli_opt(23, 59, 59, 999)?);
    tz.from_local_datetime(&naive)
        .latest()
        .or_else(|| tz.from_local_datetime(&naive).earliest())
        .map(|dt| dt.timestamp_millis())
}

/// Find an ASCII phrase case-insensitively at Unicode-safe word boundaries.
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let needle_len = needle.len();
    for (idx, _) in haystack.char_indices() {
        if idx + needle_len <= haystack.len() && haystack.is_char_boundary(idx + needle_len) {
            let before_ok = haystack[..idx]
                .chars()
                .next_back()
                .map_or(true, |c| !c.is_alphanumeric());
            let after_ok = haystack[idx + needle_len..]
                .chars()
                .next()
                .map_or(true, |c| !c.is_alphanumeric());
            if before_ok && after_ok && haystack[idx..idx + needle_len].eq_ignore_ascii_case(needle)
            {
                return Some(idx);
            }
        }
    }
    None
}

/// Strip a matched phrase and surrounding prepositions/articles ("from", "on", "in", "during", "since", "the") from the query.
fn clean_query_text(query: &str, phrase: &str) -> String {
    if let Some(pos) = find_ascii_case_insensitive(query, phrase) {
        let before = &query[..pos];
        let after = &query[pos + phrase.len()..];

        // Clean before part: strip prepositions and articles iteratively
        let mut before_trimmed = before.trim();
        let stop_words = [
            "from", "on", "in", "during", "since", "for", "between", "the", "a", "an",
        ];
        let mut changed = true;
        while changed {
            changed = false;
            let lower = before_trimmed.to_lowercase();
            for word in stop_words {
                if lower.ends_with(word)
                    && (lower.len() == word.len()
                        || lower[..lower.len() - word.len()].ends_with(char::is_whitespace))
                {
                    let end = before_trimmed.len() - word.len();
                    before_trimmed = before_trimmed[..end].trim();
                    changed = true;
                    break;
                }
            }
        }

        // Clean after part: prepositions must be matched on word boundaries
        let mut after_trimmed = after.trim();
        for prep in &["from", "on", "in", "during", "since", "for", "to"] {
            let lower = after_trimmed.to_lowercase();
            if lower == *prep {
                after_trimmed = "";
                break;
            } else if lower.starts_with(prep) {
                let rest = &lower[prep.len()..];
                if rest.starts_with(char::is_whitespace)
                    || rest.starts_with(|c: char| c == '?' || c == '!' || c == '.' || c == ',')
                {
                    after_trimmed = after_trimmed[prep.len()..].trim();
                    break;
                }
            }
        }

        let combined =
            if after_trimmed.starts_with(|c: char| c == '?' || c == '!' || c == '.' || c == ',') {
                format!("{before_trimmed}{after_trimmed}")
            } else if before_trimmed.is_empty() {
                after_trimmed.to_string()
            } else if after_trimmed.is_empty() {
                before_trimmed.to_string()
            } else {
                format!("{before_trimmed} {after_trimmed}")
            };
        combined.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        query.trim().to_string()
    }
}

/// Parse explicit month names into month numbers (1..=12).
fn parse_month_name(s: &str) -> Option<u32> {
    match s.to_lowercase().as_str() {
        "january" | "jan" => Some(1),
        "february" | "feb" => Some(2),
        "march" | "mar" => Some(3),
        "april" | "apr" => Some(4),
        "may" => Some(5),
        "june" | "jun" => Some(6),
        "july" | "jul" => Some(7),
        "august" | "aug" => Some(8),
        "september" | "sept" | "sep" => Some(9),
        "october" | "oct" => Some(10),
        "november" | "nov" => Some(11),
        "december" | "dec" => Some(12),
        _ => None,
    }
}

/// Extract temporal constraints from a natural-language query using the current local wall clock.
pub fn extract_temporal_range(query: &str) -> TemporalExtraction {
    extract_temporal_range_at(query, Local::now())
}

/// Extract temporal constraints using an explicit clock and timezone.
pub fn extract_temporal_range_at<Tz: TimeZone>(
    query: &str,
    now: DateTime<Tz>,
) -> TemporalExtraction {
    let q_lower = query.to_lowercase();
    let tz = now.timezone();
    let today_date = now.date_naive();

    // 1. "day before yesterday" (must precede "yesterday")
    if has_word_boundary(&q_lower, "day before yesterday") {
        let d = today_date - Duration::days(2);
        if let (Some(start), Some(end)) = (start_of_day(d, &tz), end_of_day(d, &tz)) {
            return TemporalExtraction {
                clean_query: clean_query_text(query, "day before yesterday"),
                range: Some(DateRange::new(start, end)),
                phrase: Some("day before yesterday".to_string()),
            };
        }
    }

    // 2. "today"
    if has_word_boundary(&q_lower, "today") {
        if let (Some(start), Some(end)) =
            (start_of_day(today_date, &tz), end_of_day(today_date, &tz))
        {
            return TemporalExtraction {
                clean_query: clean_query_text(query, "today"),
                range: Some(DateRange::new(start, end)),
                phrase: Some("today".to_string()),
            };
        }
    }

    // 3. "yesterday"
    if has_word_boundary(&q_lower, "yesterday") {
        let d = today_date - Duration::days(1);
        if let (Some(start), Some(end)) = (start_of_day(d, &tz), end_of_day(d, &tz)) {
            return TemporalExtraction {
                clean_query: clean_query_text(query, "yesterday"),
                range: Some(DateRange::new(start, end)),
                phrase: Some("yesterday".to_string()),
            };
        }
    }

    // 4. "this week" (Monday to Sunday)
    if has_word_boundary(&q_lower, "this week") {
        let days_from_mon = today_date.weekday().num_days_from_monday() as i64;
        let mon = today_date - Duration::days(days_from_mon);
        let sun = mon + Duration::days(6);
        if let (Some(start), Some(end)) = (start_of_day(mon, &tz), end_of_day(sun, &tz)) {
            return TemporalExtraction {
                clean_query: clean_query_text(query, "this week"),
                range: Some(DateRange::new(start, end)),
                phrase: Some("this week".to_string()),
            };
        }
    }

    // 5. "last week" (Previous Monday to previous Sunday)
    if has_word_boundary(&q_lower, "last week") {
        let days_from_mon = today_date.weekday().num_days_from_monday() as i64;
        let this_mon = today_date - Duration::days(days_from_mon);
        let last_mon = this_mon - Duration::days(7);
        let last_sun = last_mon + Duration::days(6);
        if let (Some(start), Some(end)) = (start_of_day(last_mon, &tz), end_of_day(last_sun, &tz)) {
            return TemporalExtraction {
                clean_query: clean_query_text(query, "last week"),
                range: Some(DateRange::new(start, end)),
                phrase: Some("last week".to_string()),
            };
        }
    }

    // 6. "this month"
    if has_word_boundary(&q_lower, "this month") {
        let y = today_date.year();
        let m = today_date.month();
        let first_day = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
        let next_month_first = if m == 12 {
            NaiveDate::from_ymd_opt(y + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(y, m + 1, 1).unwrap()
        };
        let last_day = next_month_first - Duration::days(1);
        if let (Some(start), Some(end)) = (start_of_day(first_day, &tz), end_of_day(last_day, &tz))
        {
            return TemporalExtraction {
                clean_query: clean_query_text(query, "this month"),
                range: Some(DateRange::new(start, end)),
                phrase: Some("this month".to_string()),
            };
        }
    }

    // 7. "last month"
    if has_word_boundary(&q_lower, "last month") {
        let y = today_date.year();
        let m = today_date.month();
        let (prev_y, prev_m) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
        let first_day = NaiveDate::from_ymd_opt(prev_y, prev_m, 1).unwrap();
        let this_month_first = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
        let last_day = this_month_first - Duration::days(1);
        if let (Some(start), Some(end)) = (start_of_day(first_day, &tz), end_of_day(last_day, &tz))
        {
            return TemporalExtraction {
                clean_query: clean_query_text(query, "last month"),
                range: Some(DateRange::new(start, end)),
                phrase: Some("last month".to_string()),
            };
        }
    }

    // 8. "last N days" or "past N days"
    if let Some((n, matched_str)) = parse_rolling_days(&q_lower) {
        let days_back = if n > 0 { (n as i64) - 1 } else { 0 };
        let start_date = today_date - Duration::days(days_back);
        if let (Some(start), Some(end)) =
            (start_of_day(start_date, &tz), end_of_day(today_date, &tz))
        {
            return TemporalExtraction {
                clean_query: clean_query_text(query, &matched_str),
                range: Some(DateRange::new(start, end)),
                phrase: Some(matched_str),
            };
        }
    }

    // 9. Explicit date ranges: e.g. "from 2026-08-10 to 2026-08-20" or "2026-08-10 to 2026-08-20"
    if let Some((d1, d2, matched_str)) = parse_explicit_date_range(&q_lower) {
        if let (Some(start), Some(end)) = (start_of_day(d1, &tz), end_of_day(d2, &tz)) {
            return TemporalExtraction {
                clean_query: clean_query_text(query, &matched_str),
                range: Some(DateRange::new(start, end)),
                phrase: Some(matched_str),
            };
        }
    }

    // 10. Explicit single dates: e.g. "2026-09-04" or "September 4, 2026"
    if let Some((d, matched_str)) = parse_explicit_single_date(&q_lower, today_date.year()) {
        if let (Some(start), Some(end)) = (start_of_day(d, &tz), end_of_day(d, &tz)) {
            return TemporalExtraction {
                clean_query: clean_query_text(query, &matched_str),
                range: Some(DateRange::new(start, end)),
                phrase: Some(matched_str),
            };
        }
    }

    // No recognized temporal phrase: keep query intact without invented constraints
    TemporalExtraction {
        clean_query: query.trim().to_string(),
        range: None,
        phrase: None,
    }
}

/// Helper to check whole-word boundary for keywords (preventing false matches like "yesterdays" or "uptodate").
fn has_word_boundary(text: &str, word: &str) -> bool {
    let word_len = word.len();
    for (idx, _) in text.char_indices() {
        if idx + word_len <= text.len() && text.is_char_boundary(idx + word_len) {
            if text[idx..idx + word_len].eq_ignore_ascii_case(word) {
                let before_ok = idx == 0
                    || text[..idx]
                        .chars()
                        .last()
                        .map_or(true, |c| !c.is_alphanumeric());
                let after_ok = idx + word_len >= text.len()
                    || text[idx + word_len..]
                        .chars()
                        .next()
                        .map_or(true, |c| !c.is_alphanumeric());
                if before_ok && after_ok {
                    return true;
                }
            }
        }
    }
    false
}

/// Parse "last N days" or "past N days".
fn parse_rolling_days(text: &str) -> Option<(u32, String)> {
    let lower = text.to_lowercase();
    for prefix in &["last ", "past "] {
        let mut search_idx = 0;
        while let Some(pos) = lower[search_idx..].find(prefix) {
            let abs_pos = search_idx + pos;
            let rest = &lower[abs_pos + prefix.len()..];
            let words: Vec<&str> = rest.split_whitespace().collect();
            if words.len() >= 2 && words[1].starts_with("day") {
                if let Ok(n) = words[0].parse::<u32>() {
                    if n > 0 && n <= 3650 {
                        let matched = format!(
                            "{}{}{}",
                            prefix,
                            words[0],
                            if words[1].starts_with("days") {
                                " days"
                            } else {
                                " day"
                            }
                        );
                        return Some((n, matched));
                    }
                }
            }
            search_idx = abs_pos + prefix.len();
        }
    }
    None
}

/// Parse explicit date ranges like "2026-08-10 to 2026-08-20" or "between 2026-08-10 and 2026-08-20".
fn parse_explicit_date_range(text: &str) -> Option<(NaiveDate, NaiveDate, String)> {
    let separators = [" to ", " - ", " and "];
    for sep in separators {
        let mut search_idx = 0;
        while let Some(pos) = text[search_idx..].find(sep) {
            let abs_pos = search_idx + pos;
            let left_part = text[..abs_pos].trim();
            let right_part = text[abs_pos + sep.len()..].trim();

            let left_token = left_part.split_whitespace().last().unwrap_or("");
            let right_token = right_part.split_whitespace().next().unwrap_or("");

            if let (Ok(d1), Ok(d2)) = (parse_iso_date(left_token), parse_iso_date(right_token)) {
                let matched = format!("{left_token}{sep}{right_token}");
                return Some((d1, d2, matched));
            }
            search_idx = abs_pos + sep.len();
        }
    }
    None
}

/// Parse ISO dates `YYYY-MM-DD` or `YYYY/MM/DD`.
fn parse_iso_date(token: &str) -> Result<NaiveDate, ()> {
    let clean = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '/');
    if let Ok(d) = NaiveDate::parse_from_str(clean, "%Y-%m-%d") {
        return Ok(d);
    }
    if let Ok(d) = NaiveDate::parse_from_str(clean, "%Y/%m/%d") {
        return Ok(d);
    }
    Err(())
}

/// Parse explicit single date: ISO or named month.
fn parse_explicit_single_date(text: &str, current_year: i32) -> Option<(NaiveDate, String)> {
    // Check words for ISO date format
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '/');
        if clean.len() >= 8 {
            if let Ok(d) = parse_iso_date(clean) {
                return Some((d, clean.to_string()));
            }
        }
    }

    // Check for named month formats: e.g. "September 4, 2026" or "Sep 4"
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        let clean_word = word.trim_matches(|c: char| !c.is_alphabetic());
        if let Some(month) = parse_month_name(clean_word) {
            if i + 1 < words.len() {
                let day_raw = words[i + 1].trim_matches(|c: char| !c.is_ascii_digit());
                if let Ok(day) = day_raw.parse::<u32>() {
                    if (1..=31).contains(&day) {
                        // Check if year follows: e.g. "September 4, 2026"
                        let mut year = current_year;
                        let mut end_word_idx = i + 1;
                        if i + 2 < words.len() {
                            let yr_raw = words[i + 2].trim_matches(|c: char| !c.is_ascii_digit());
                            if yr_raw.len() == 4 {
                                if let Ok(yr) = yr_raw.parse::<i32>() {
                                    year = yr;
                                    end_word_idx = i + 2;
                                }
                            }
                        }
                        if let Some(d) = NaiveDate::from_ymd_opt(year, month, day) {
                            let mut phrase = words[i..=end_word_idx].join(" ");
                            while phrase.ends_with(|c: char| c == '?' || c == '!' || c == '.') {
                                phrase.pop();
                            }
                            return Some((d, phrase));
                        }
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Utc};

    /// Returns a fixed DateTime for testing: Friday 2026-09-04 at 15:30:00 UTC.
    fn fixed_test_clock() -> DateTime<Utc> {
        let naive = NaiveDate::from_ymd_opt(2026, 9, 4)
            .unwrap()
            .and_hms_opt(15, 30, 0)
            .unwrap();
        DateTime::from_naive_utc_and_offset(naive, Utc)
    }

    #[test]
    fn parse_relative_days_fixed_clock() {
        let now = fixed_test_clock(); // Friday, 2026-09-04
        let tz = Utc;

        // "today"
        let res = extract_temporal_range_at("standup notes today", now);
        assert_eq!(res.clean_query, "standup notes");
        assert_eq!(res.phrase.as_deref(), Some("today"));
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(), &tz).unwrap()
        );
        assert_eq!(
            range.end_ms,
            end_of_day(NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(), &tz).unwrap()
        );

        // "yesterday"
        let res = extract_temporal_range_at("what did we discuss yesterday?", now);
        assert_eq!(res.clean_query, "what did we discuss?");
        assert_eq!(res.phrase.as_deref(), Some("yesterday"));
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(), &tz).unwrap()
        );
        assert_eq!(
            range.end_ms,
            end_of_day(NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(), &tz).unwrap()
        );

        // "day before yesterday"
        let res = extract_temporal_range_at("notes from the day before yesterday", now);
        assert_eq!(res.clean_query, "notes");
        assert_eq!(res.phrase.as_deref(), Some("day before yesterday"));
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 9, 2).unwrap(), &tz).unwrap()
        );
    }

    #[test]
    fn parse_relative_weeks_fixed_clock() {
        let now = fixed_test_clock(); // Friday, 2026-09-04
        let tz = Utc;

        // "this week" (Mon Aug 31 to Sun Sep 6)
        let res = extract_temporal_range_at("tasks created this week", now);
        assert_eq!(res.clean_query, "tasks created");
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(), &tz).unwrap()
        );
        assert_eq!(
            range.end_ms,
            end_of_day(NaiveDate::from_ymd_opt(2026, 9, 6).unwrap(), &tz).unwrap()
        );

        // "last week" (Mon Aug 24 to Sun Aug 30)
        let res = extract_temporal_range_at("sync notes from last week", now);
        assert_eq!(res.clean_query, "sync notes");
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 8, 24).unwrap(), &tz).unwrap()
        );
        assert_eq!(
            range.end_ms,
            end_of_day(NaiveDate::from_ymd_opt(2026, 8, 30).unwrap(), &tz).unwrap()
        );
    }

    #[test]
    fn parse_relative_months_fixed_clock() {
        let now = fixed_test_clock(); // 2026-09-04
        let tz = Utc;

        // "this month" (2026-09-01 to 2026-09-30)
        let res = extract_temporal_range_at("invoices this month", now);
        assert_eq!(res.clean_query, "invoices");
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(), &tz).unwrap()
        );
        assert_eq!(
            range.end_ms,
            end_of_day(NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(), &tz).unwrap()
        );

        // "last month" (2026-08-01 to 2026-08-31)
        let res = extract_temporal_range_at("travel expenses from last month", now);
        assert_eq!(res.clean_query, "travel expenses");
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(), &tz).unwrap()
        );
        assert_eq!(
            range.end_ms,
            end_of_day(NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(), &tz).unwrap()
        );
    }

    #[test]
    fn parse_rolling_days_fixed_clock() {
        let now = fixed_test_clock(); // 2026-09-04
        let tz = Utc;

        let res = extract_temporal_range_at("commits in the last 7 days", now);
        assert_eq!(res.clean_query, "commits");
        let range = res.range.unwrap();
        // Exact 7 calendar days inclusive of today: 2026-08-29 through 2026-09-04
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 8, 29).unwrap(), &tz).unwrap()
        );
        assert_eq!(
            range.end_ms,
            end_of_day(NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(), &tz).unwrap()
        );
    }

    #[test]
    fn parse_preposition_word_boundary() {
        let now = fixed_test_clock();

        // "inbox" must not have "in" stripped to become "box"
        let res1 = extract_temporal_range_at("today inbox", now);
        assert_eq!(res1.clean_query, "inbox");

        // "format" must not have "for" stripped to become "mat"
        let res2 = extract_temporal_range_at("today format check", now);
        assert_eq!(res2.clean_query, "format check");

        // The cleaner must remove the bounded occurrence, not an earlier
        // substring that merely contains the same letters.
        let res3 = extract_temporal_range_at("notoday notes today", now);
        assert_eq!(res3.clean_query, "notoday notes");

        // Multi-word phrases require a boundary after the complete phrase.
        let res4 = extract_temporal_range_at("plans for this weekend", now);
        assert!(res4.range.is_none());
    }

    #[test]
    fn parse_explicit_dates_and_ranges() {
        let now = fixed_test_clock();
        let tz = Utc;

        // ISO single date
        let res = extract_temporal_range_at("incident log on 2026-07-28", now);
        assert_eq!(res.clean_query, "incident log");
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 7, 28).unwrap(), &tz).unwrap()
        );
        assert_eq!(
            range.end_ms,
            end_of_day(NaiveDate::from_ymd_opt(2026, 7, 28).unwrap(), &tz).unwrap()
        );

        // Explicit date range
        let res = extract_temporal_range_at("records from 2026-08-10 to 2026-08-20", now);
        assert_eq!(res.clean_query, "records");
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(), &tz).unwrap()
        );
        assert_eq!(
            range.end_ms,
            end_of_day(NaiveDate::from_ymd_opt(2026, 8, 20).unwrap(), &tz).unwrap()
        );
    }

    #[test]
    fn timezone_boundary_different_offset() {
        // Test IST (+05:30): 2026-09-05 01:00:00 IST is 2026-09-04 19:30:00 UTC
        let ist_offset = FixedOffset::east_opt(5 * 3600 + 1800).unwrap();
        let naive = NaiveDate::from_ymd_opt(2026, 9, 5)
            .unwrap()
            .and_hms_opt(1, 0, 0)
            .unwrap();
        let now_ist = ist_offset.from_local_datetime(&naive).single().unwrap();

        // In IST, "today" is Sep 5th, not Sep 4th
        let res = extract_temporal_range_at("notes today", now_ist);
        let range = res.range.unwrap();
        let sep5_start_ist =
            start_of_day(NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(), &ist_offset).unwrap();
        assert_eq!(range.start_ms, sep5_start_ist);
    }

    #[test]
    fn unparseable_query_retains_text_without_range() {
        let now = fixed_test_clock();
        let res =
            extract_temporal_range_at("quantum cryptography lattice reduction algorithm", now);
        assert_eq!(
            res.clean_query,
            "quantum cryptography lattice reduction algorithm"
        );
        assert_eq!(res.range, None);
        assert_eq!(res.phrase, None);
    }

    #[test]
    fn parse_edge_cases_and_ordinals() {
        let now = fixed_test_clock(); // 2026-09-04
        let tz = Utc;

        // Named month with ordinal "September 4th, 2026"
        let res = extract_temporal_range_at("meeting minutes on September 4th, 2026", now);
        assert_eq!(res.clean_query, "meeting minutes");
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(), &tz).unwrap()
        );

        // Named month without year (defaults to current year 2026)
        let res = extract_temporal_range_at("notes from Aug 15th", now);
        assert_eq!(res.clean_query, "notes");
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2026, 8, 15).unwrap(), &tz).unwrap()
        );

        // Mixed casing and exclamation marks
        let res = extract_temporal_range_at("WHAT HAPPENED YESTERDAY?!", now);
        assert_eq!(res.clean_query, "WHAT HAPPENED?!");
        assert_eq!(res.phrase.as_deref(), Some("yesterday"));

        // Standalone temporal phrase
        let res = extract_temporal_range_at("today", now);
        assert_eq!(res.clean_query, "");
        assert!(res.range.is_some());

        // Leap year: Feb 29, 2024 is valid
        let res = extract_temporal_range_at("leap day log on 2024-02-29", now);
        assert_eq!(res.clean_query, "leap day log");
        let range = res.range.unwrap();
        assert_eq!(
            range.start_ms,
            start_of_day(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(), &tz).unwrap()
        );

        // Invalid leap day: Feb 29, 2026 should not parse as a date
        let res = extract_temporal_range_at("invalid leap day on 2026-02-29", now);
        assert_eq!(res.range, None);

        // Multiple "last" prefixes in query
        let res = extract_temporal_range_at("last meeting in the last 7 days", now);
        assert_eq!(res.clean_query, "last meeting");
        assert!(res.range.is_some());
        assert_eq!(res.phrase.as_deref(), Some("last 7 days"));

        // Multiple separators in query
        let res =
            extract_temporal_range_at("apples and oranges between 2026-08-10 and 2026-08-20", now);
        assert_eq!(res.clean_query, "apples and oranges");
        assert!(res.range.is_some());

        // Multibyte Unicode character without slicing panic
        let res = extract_temporal_range_at("GROßARTIGE NOTIZEN YESTERDAY", now);
        assert!(res.range.is_some());
        assert_eq!(res.clean_query, "GROßARTIGE NOTIZEN");

        let res = extract_temporal_range_at("日本語のメモ from today", now);
        assert!(res.range.is_some());
        assert_eq!(res.clean_query, "日本語のメモ");
    }
}
