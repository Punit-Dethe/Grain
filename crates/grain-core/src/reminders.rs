//! [GRAIN] Reminders — the pure half (`docs/Reminders/PLAN.md`).
//!
//! The record, its validation, calendar arithmetic, the phrases the Agent may
//! use for a day or a time, and the Agent tool schemas. No clock, no time zone
//! and no I/O: the desktop crate passes in "today" and turns a local date and
//! time into an instant.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;

/// The Agent provider id for the compiled Reminders tools.
pub const EXTENSION_ID: &str = "grain.reminders";
/// The name shown to the model and the user.
pub const PROVIDER_NAME: &str = "Reminders";
/// Directory description; the host appends today's date.
pub const DESCRIPTION: &str = "Grain's own reminders: create one for a date and optional time with a title and links (e.g. a Google Meet URL); Grain rings then with an Open button. List upcoming reminders.";
/// Bumped whenever a tool schema or the policy changes meaning.
const POLICY_VERSION: u32 = 1;

pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_DETAILS_CHARS: usize = 2_000;
pub const MAX_LINKS: usize = 5;
/// An all-day reminder rings at this local hour on its date.
pub const ALL_DAY_RING_HOUR: u32 = 9;
/// Default snooze, minutes.
pub const DEFAULT_SNOOZE_MINUTES: u32 = 10;
/// Longest snooze the host accepts, minutes (one week).
pub const MAX_SNOOZE_MINUTES: u32 = 7 * 24 * 60;

// ── The record ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ReminderStatus {
    /// Waiting for `ring_at_ms` (or not ringing at all when that is `None`).
    #[default]
    Pending,
    /// Rang and still on screen.
    Ringing,
    Done,
}

/// What happens at the due time. Only `Notify` exists today; "scheduled
/// actions" (an instruction sent to the Agent) will be a second kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ReminderKind {
    #[default]
    Notify,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ReminderSource {
    #[default]
    User,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ReminderLink {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Reminder {
    pub id: String,
    pub title: String,
    /// Local calendar date, `YYYY-MM-DD`.
    pub date: String,
    /// Local wall-clock time, `HH:MM`; `None` = all-day.
    pub time: Option<String>,
    #[serde(default)]
    pub links: Vec<ReminderLink>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub kind: ReminderKind,
    #[serde(default)]
    pub status: ReminderStatus,
    /// Next ring instant (UTC ms); `None` = will not ring.
    #[serde(default)]
    pub ring_at_ms: Option<u64>,
    #[serde(default)]
    pub source: ReminderSource,
    #[serde(default)]
    pub created_ms: u64,
    #[serde(default)]
    pub updated_ms: u64,
}

/// What the tab (or the Agent) submits to create or edit a reminder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ReminderInput {
    /// `None` creates a new reminder.
    pub id: Option<String>,
    pub title: String,
    pub date: String,
    pub time: Option<String>,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub details: Option<String>,
}

/// The validated fields of an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub title: String,
    pub date: Date,
    /// `(hour, minute)`; `None` = all-day.
    pub time: Option<(u32, u32)>,
    pub links: Vec<ReminderLink>,
    pub details: Option<String>,
}

/// Check and normalise an input. Links go through Reach's link policy.
pub fn validate(input: &ReminderInput) -> Result<Validated, String> {
    let title = clean_line(&input.title);
    if title.is_empty() {
        return Err("the reminder needs a title".into());
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!(
            "the title is longer than {MAX_TITLE_CHARS} characters"
        ));
    }
    let date = Date::parse_iso(input.date.trim())
        .ok_or_else(|| format!("`{}` is not a date (YYYY-MM-DD)", input.date.trim()))?;
    let time = match input.time.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(parse_time(raw)?),
    };
    let mut links: Vec<ReminderLink> = Vec::new();
    for raw in &input.links {
        if raw.trim().is_empty() {
            continue;
        }
        let url = crate::reach::validate_link(raw)?;
        if !links.iter().any(|link| link.url == url) {
            links.push(ReminderLink { url });
        }
    }
    if links.len() > MAX_LINKS {
        return Err(format!("a reminder holds at most {MAX_LINKS} links"));
    }
    let details = input
        .details
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| text.chars().take(MAX_DETAILS_CHARS).collect::<String>());
    Ok(Validated {
        title,
        date,
        time,
        links,
        details,
    })
}

fn clean_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn format_time(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}")
}

/// The earliest instant at which a pending reminder rings.
pub fn next_ring(reminders: &[Reminder]) -> Option<u64> {
    reminders
        .iter()
        .filter(|r| r.status == ReminderStatus::Pending)
        .filter_map(|r| r.ring_at_ms)
        .min()
}

/// Whether a host-minted id is acceptable.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

// ── Calendar arithmetic (proleptic Gregorian, no time zone) ───────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAY_NAMES: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        ((1970..=9999).contains(&year)
            && (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(year, month))
        .then_some(Self { year, month, day })
    }

    pub fn parse_iso(raw: &str) -> Option<Self> {
        let mut parts = raw.split('-');
        let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
            return None;
        }
        Self::new(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
    }

    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
    pub fn days(self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    pub fn from_days(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Self { year, month, day }
    }

    pub fn add_days(self, days: i64) -> Self {
        Self::from_days(self.days() + days)
    }

    /// 0 = Monday … 6 = Sunday.
    pub fn weekday(self) -> usize {
        // 1970-01-01 was a Thursday (index 3).
        (self.days() + 3).rem_euclid(7) as usize
    }

    /// "Wed 25 Nov 2026".
    pub fn label(self) -> String {
        format!(
            "{} {} {} {}",
            WEEKDAYS[self.weekday()],
            self.day,
            MONTHS[self.month as usize - 1],
            self.year
        )
    }
}

/// A day phrase the Agent may send: `YYYY-MM-DD`, `MM-DD` (next occurrence),
/// `today`, `tomorrow` or a weekday name (next occurrence, never today).
pub fn parse_day(raw: &str, today: Date) -> Result<Date, String> {
    let phrase = raw.trim().to_ascii_lowercase();
    let phrase = phrase
        .trim_start_matches("next ")
        .trim_start_matches("this ");
    if let Some(date) = Date::parse_iso(phrase) {
        return Ok(date);
    }
    match phrase {
        "today" | "tonight" => return Ok(today),
        "tomorrow" => return Ok(today.add_days(1)),
        _ => {}
    }
    if let Some(index) = WEEKDAY_NAMES
        .iter()
        .position(|name| *name == phrase || name[..3] == *phrase)
    {
        let ahead = (index as i64 - today.weekday() as i64).rem_euclid(7);
        return Ok(today.add_days(if ahead == 0 { 7 } else { ahead }));
    }
    if let Some((m, d)) = phrase.split_once('-') {
        if let (Ok(month), Ok(day)) = (m.parse::<u32>(), d.parse::<u32>()) {
            for year in [today.year, today.year + 1] {
                if let Some(date) = Date::new(year, month, day) {
                    if date >= today {
                        return Ok(date);
                    }
                }
            }
        }
    }
    Err(format!(
        "`{}` is not a day I understand; use YYYY-MM-DD",
        raw.trim()
    ))
}

/// A time phrase: `18:00`, `6pm`, `6:30 pm`, `noon`, `midnight`.
pub fn parse_time(raw: &str) -> Result<(u32, u32), String> {
    let lower = raw.trim().to_ascii_lowercase().replace('.', "");
    let bad = || format!("`{}` is not a time (HH:MM)", raw.trim());
    match lower.as_str() {
        "noon" => return Ok((12, 0)),
        "midnight" => return Ok((0, 0)),
        _ => {}
    }
    let (clock, meridiem) = if let Some(rest) = lower.strip_suffix("am") {
        (rest.trim(), Some(false))
    } else if let Some(rest) = lower.strip_suffix("pm") {
        (rest.trim(), Some(true))
    } else {
        (lower.trim(), None)
    };
    let (h, m) = clock.split_once(':').unwrap_or((clock, "0"));
    if h.is_empty() || h.len() > 2 || m.len() > 2 {
        return Err(bad());
    }
    let mut hour: u32 = h.parse().map_err(|_| bad())?;
    let minute: u32 = m.parse().map_err(|_| bad())?;
    if minute > 59 {
        return Err(bad());
    }
    match meridiem {
        Some(pm) => {
            if !(1..=12).contains(&hour) {
                return Err(bad());
            }
            hour = match (hour, pm) {
                (12, false) => 0,
                (12, true) => 12,
                (h, true) => h + 12,
                (h, false) => h,
            };
        }
        None if hour > 23 => return Err(bad()),
        None => {}
    }
    Ok((hour, minute))
}

/// "6:00 PM" style label for a stored `HH:MM`.
pub fn time_label(hour: u32, minute: u32) -> String {
    let (h12, suffix) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        _ => (hour - 12, "PM"),
    };
    format!("{h12}:{minute:02} {suffix}")
}

// ── Agent tools ────────────────────────────────────────────────────────────

pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub keywords: &'static [&'static str],
    pub read_only: bool,
    pub schema: Value,
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "create_reminder",
            title: "Create a reminder",
            description: "Create a Grain reminder. At the time Grain shows it on screen with an Open button for each link. Put every link the user gave (e.g. a Google Meet or Zoom URL) in `links`. Omit `time` for an all-day reminder. Tell the user the day and time from the result.",
            keywords: &[
                "remind", "reminder", "remind me", "alarm", "alert", "notify", "schedule",
                "meeting", "appointment", "calendar", "don't forget",
            ],
            read_only: false,
            schema: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "What to remind about, short, e.g. \"Design review\"."},
                    "date": {"type": "string", "description": "The day: YYYY-MM-DD preferred; also accepts today, tomorrow, a weekday name or MM-DD."},
                    "time": {"type": "string", "description": "Optional local time, 24-hour HH:MM (e.g. 18:00) or 6pm."},
                    "links": {"type": "array", "items": {"type": "string"}, "description": "Links to open from the reminder, exactly as the user gave them."},
                    "details": {"type": "string", "description": "Optional extra notes worth showing with the reminder."}
                },
                "required": ["title", "date"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "list_reminders",
            title: "List reminders",
            description: "List the user's upcoming Grain reminders, or those on one day. The reply starts with today's date.",
            keywords: &["reminders", "upcoming", "what's on", "agenda", "schedule", "today", "tomorrow"],
            read_only: true,
            schema: json!({
                "type": "object",
                "properties": {
                    "date": {"type": "string", "description": "Optional day: YYYY-MM-DD, today, tomorrow or a weekday."}
                },
                "additionalProperties": false
            }),
        },
    ]
}

pub fn catalog_digest() -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(POLICY_VERSION.to_le_bytes());
    for tool in tools() {
        hasher.update(tool.name.as_bytes());
        hasher.update([0]);
        hasher.update(tool.description.as_bytes());
        hasher.update([0]);
        hasher.update(tool.schema.to_string().as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A parsed Agent call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Create(ReminderInput),
    List { day: Option<Date> },
}

/// Parse an Agent call against today's local date. Day phrases are resolved
/// to an ISO date here; the time is left for [`validate`].
pub fn parse(action: &str, args: &Value, today: Date) -> Result<Call, String> {
    let args = args
        .as_object()
        .ok_or_else(|| "arguments must be an object".to_string())?;
    let text = |key: &str| -> Option<String> {
        args.get(key)
            .and_then(|value| match value {
                Value::String(s) => Some(s.trim().to_string()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .filter(|s| !s.is_empty())
    };
    match action {
        "create_reminder" => {
            let title = text("title").ok_or("`title` is required")?;
            let date = parse_day(&text("date").ok_or("`date` is required")?, today)?;
            let links = match args.get("links") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::String(one)) => vec![one.clone()],
                Some(Value::Array(items)) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                Some(_) => return Err("`links` must be a list of strings".into()),
            };
            Ok(Call::Create(ReminderInput {
                id: None,
                title,
                date: date.iso(),
                time: text("time"),
                links,
                details: text("details"),
            }))
        }
        "list_reminders" => Ok(Call::List {
            day: text("date").map(|raw| parse_day(&raw, today)).transpose()?,
        }),
        other => Err(format!("Reminders has no action `{other}`")),
    }
}

/// One line about a reminder for the model: "Wed 25 Nov 2026, 6:00 PM — Title (1 link)".
pub fn describe(reminder: &Reminder) -> String {
    let day = Date::parse_iso(&reminder.date).map_or(reminder.date.clone(), Date::label);
    let when = match reminder.time.as_deref().and_then(|t| parse_time(t).ok()) {
        Some((h, m)) => format!("{day}, {}", time_label(h, m)),
        None => format!("{day}, all day"),
    };
    let links = match reminder.links.len() {
        0 => String::new(),
        1 => " (1 link)".into(),
        n => format!(" ({n} links)"),
    };
    let state = match reminder.status {
        ReminderStatus::Done => " [done]",
        ReminderStatus::Ringing => " [ringing now]",
        ReminderStatus::Pending => "",
    };
    format!("{when} — {}{links}{state}", reminder.title)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::new(y, m, day).unwrap()
    }

    #[test]
    fn civil_days_round_trip_and_weekdays() {
        for days in [-1_000, 0, 1, 19_000, 20_405, 30_000] {
            assert_eq!(Date::from_days(days).days(), days);
        }
        assert_eq!(d(1970, 1, 1).days(), 0);
        // 13 Nov 2026 is a Friday.
        assert_eq!(d(2026, 11, 13).weekday(), 4);
        assert_eq!(d(2026, 11, 25).label(), "Wed 25 Nov 2026");
        assert_eq!(d(2026, 12, 31).add_days(1), d(2027, 1, 1));
        assert_eq!(d(2028, 2, 28).add_days(1), d(2028, 2, 29));
        assert!(Date::new(2027, 2, 29).is_none());
    }

    #[test]
    fn day_phrases_resolve_forward() {
        let today = d(2026, 11, 13); // Friday
        assert_eq!(parse_day("2026-11-25", today).unwrap(), d(2026, 11, 25));
        assert_eq!(parse_day("today", today).unwrap(), today);
        assert_eq!(parse_day("Tomorrow", today).unwrap(), d(2026, 11, 14));
        assert_eq!(parse_day("monday", today).unwrap(), d(2026, 11, 16));
        assert_eq!(parse_day("next fri", today).unwrap(), d(2026, 11, 20));
        assert_eq!(parse_day("11-25", today).unwrap(), d(2026, 11, 25));
        assert_eq!(parse_day("01-05", today).unwrap(), d(2027, 1, 5));
        assert!(parse_day("someday", today).is_err());
        assert!(parse_day("2026-02-30", today).is_err());
    }

    #[test]
    fn time_phrases() {
        assert_eq!(parse_time("18:00").unwrap(), (18, 0));
        assert_eq!(parse_time("6pm").unwrap(), (18, 0));
        assert_eq!(parse_time("6:30 PM").unwrap(), (18, 30));
        assert_eq!(parse_time("12am").unwrap(), (0, 0));
        assert_eq!(parse_time("12 p.m.").unwrap(), (12, 0));
        assert_eq!(parse_time("9").unwrap(), (9, 0));
        assert_eq!(parse_time("noon").unwrap(), (12, 0));
        assert!(parse_time("25:00").is_err());
        assert!(parse_time("13pm").is_err());
        assert!(parse_time("6:75").is_err());
        assert_eq!(time_label(18, 5), "6:05 PM");
        assert_eq!(time_label(0, 0), "12:00 AM");
    }

    #[test]
    fn validation_normalises_and_refuses() {
        let ok = validate(&ReminderInput {
            title: "  Design   review ".into(),
            date: "2026-11-25".into(),
            time: Some("6pm".into()),
            links: vec![
                "https://meet.google.com/abc-defg-hij".into(),
                "https://meet.google.com/abc-defg-hij".into(),
                " ".into(),
            ],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(ok.title, "Design review");
        assert_eq!(ok.time, Some((18, 0)));
        assert_eq!(ok.links.len(), 1);

        let refuse = |input: ReminderInput| validate(&input).unwrap_err();
        refuse(ReminderInput {
            title: " ".into(),
            date: "2026-11-25".into(),
            ..Default::default()
        });
        refuse(ReminderInput {
            title: "x".into(),
            date: "25 Nov".into(),
            ..Default::default()
        });
        refuse(ReminderInput {
            title: "x".into(),
            date: "2026-11-25".into(),
            links: vec!["javascript:alert(1)".into()],
            ..Default::default()
        });
        refuse(ReminderInput {
            title: "x".into(),
            date: "2026-11-25".into(),
            links: vec!["file:///C:/Windows/system32/cmd.exe".into()],
            ..Default::default()
        });
    }

    #[test]
    fn agent_create_call_resolves_the_day() {
        let today = d(2026, 11, 13);
        let call = parse(
            "create_reminder",
            &json!({"title": "Standup", "date": "tomorrow", "time": "9:30am", "links": "https://zoom.us/j/1"}),
            today,
        )
        .unwrap();
        let Call::Create(input) = call else { panic!() };
        assert_eq!(input.date, "2026-11-14");
        assert_eq!(input.time.as_deref(), Some("9:30am"));
        assert_eq!(input.links, vec!["https://zoom.us/j/1".to_string()]);
        assert!(parse("create_reminder", &json!({"title": "x"}), today).is_err());
        assert!(parse("delete_everything", &json!({}), today).is_err());
        assert_eq!(
            parse("list_reminders", &json!({}), today).unwrap(),
            Call::List { day: None }
        );
    }

    #[test]
    fn next_ring_ignores_done_and_silent() {
        let base = Reminder {
            id: "a".into(),
            title: "t".into(),
            date: "2026-11-25".into(),
            time: None,
            links: Vec::new(),
            details: None,
            kind: ReminderKind::Notify,
            status: ReminderStatus::Pending,
            ring_at_ms: Some(50),
            source: ReminderSource::User,
            created_ms: 0,
            updated_ms: 0,
        };
        let done = Reminder {
            status: ReminderStatus::Done,
            ring_at_ms: Some(10),
            ..base.clone()
        };
        let silent = Reminder {
            ring_at_ms: None,
            ..base.clone()
        };
        let later = Reminder {
            ring_at_ms: Some(90),
            ..base.clone()
        };
        assert_eq!(next_ring(&[done, silent, later, base]), Some(50));
        assert_eq!(next_ring(&[]), None);
    }

    #[test]
    fn digest_is_stable() {
        assert_eq!(catalog_digest(), catalog_digest());
        assert_eq!(tools().len(), 2);
    }
}
