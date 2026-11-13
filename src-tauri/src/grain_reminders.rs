//! [GRAIN] Reminders — the desktop half (`docs/Reminders/PLAN.md`).
//!
//! Store: `<app data>/reminders.json`, read on demand and written atomically;
//! nothing is cached in memory. Firing: ONE timer task that sleeps until the
//! earliest pending reminder and is replaced on every change; with nothing
//! pending there is no task at all. Ringing: the Agent notch opens on its
//! reminder view (no window of its own). If the notch cannot open right now
//! (dictation, a summon, the Agent off) the same timer tries again a minute
//! later.
//!
//! The pure half (record, validation, day/time phrases, Agent tool schemas)
//! lives in `grain_core::reminders`.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{Datelike, TimeZone, Timelike};
use grain_core::capability_index::{ActionInput, ExtensionDirectoryEntry};
use grain_core::execution::{ActionOutcome, FailureClass, SuccessData};
use grain_core::reminders::{
    self, Call, Date, Reminder, ReminderInput, ReminderSource, ReminderStatus, Validated,
};
use grain_sdk::action::ActionRisk;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event as _;

pub(crate) use grain_core::reminders::EXTENSION_ID;

const FILE: &str = "reminders.json";
/// The timer re-reads the wall clock at least this often while waiting, so a
/// laptop sleep or a clock change cannot make a reminder ring late.
const SLICE_MS: u64 = 5 * 60 * 1000;
/// When the notch could not open for a ringing reminder, try again after this.
const RETRY_MS: u64 = 60 * 1000;

/// Serialises read-modify-write of the file.
static LOCK: Mutex<()> = Mutex::new(());
/// The single next-due timer; `None` when nothing is pending.
static TIMER: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);
/// When to try opening the notch again for a ringing reminder; `None` = no
/// retry owed (it opened, or nothing rings).
static RETRY_AT: Mutex<Option<u64>> = Mutex::new(None);

/// Sent whenever the reminder list changes. The tab and the notch re-read.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct RemindersChanged;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    reminders: Vec<Reminder>,
}

// ── Store ──────────────────────────────────────────────────────────────────

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::portable::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join(FILE))
}

fn read(path: &std::path::Path) -> Store {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| match serde_json::from_slice(&bytes) {
            Ok(store) => Some(store),
            Err(error) => {
                log::warn!("[GRAIN] reminders: unreadable store ignored: {error}");
                None
            }
        })
        .unwrap_or_default()
}

fn write(path: &std::path::Path, store: &Store) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| format!("Could not save reminders: {e}"))
}

fn load(app: &AppHandle) -> Result<Vec<Reminder>, String> {
    let path = path(app)?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(read(&path).reminders)
}

/// Read, change and write the store under the lock.
fn mutate<T>(
    app: &AppHandle,
    change: impl FnOnce(&mut Vec<Reminder>) -> Result<T, String>,
) -> Result<T, String> {
    let path = path(app)?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read(&path);
    let out = change(&mut store.reminders)?;
    write(&path, &store)?;
    Ok(out)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

// ── Local time ─────────────────────────────────────────────────────────────

fn today() -> Date {
    let now = chrono::Local::now().date_naive();
    Date::new(now.year(), now.month(), now.day()).unwrap_or(Date {
        year: 1970,
        month: 1,
        day: 1,
    })
}

/// The instant of a local date and time. A time inside a DST gap moves on to
/// the next hour that exists.
fn local_ms(date: Date, hour: u32, minute: u32) -> Option<u64> {
    let at = |h: u32| {
        chrono::Local
            .with_ymd_and_hms(date.year, date.month, date.day, h, minute, 0)
            .earliest()
    };
    at(hour)
        .or_else(|| (hour < 23).then(|| at(hour + 1)).flatten())
        .map(|t| t.timestamp_millis().max(0) as u64)
}

/// When a validated reminder should ring; `None` if that is already past.
fn ring_at(validated: &Validated, now: u64) -> Option<u64> {
    let (hour, minute) = validated.time.unwrap_or((reminders::ALL_DAY_RING_HOUR, 0));
    local_ms(validated.date, hour, minute).filter(|at| *at > now)
}

// ── Saving ─────────────────────────────────────────────────────────────────

/// Create or edit. Changing the day or time (or editing a pending reminder)
/// reschedules it; editing the words of a done or ringing one leaves its state.
fn save(
    app: &AppHandle,
    input: &ReminderInput,
    source: ReminderSource,
) -> Result<Reminder, String> {
    let validated = reminders::validate(input)?;
    let now = now_ms();
    let date = validated.date.iso();
    let time = validated.time.map(|(h, m)| reminders::format_time(h, m));
    let ring = ring_at(&validated, now);
    let saved = mutate(app, |list| {
        if let Some(id) = input.id.as_deref() {
            let existing = list
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or("That reminder no longer exists.")?;
            let rescheduled = existing.date != date || existing.time != time;
            if rescheduled || existing.status == ReminderStatus::Pending {
                existing.status = ReminderStatus::Pending;
                existing.ring_at_ms = ring;
            }
            existing.title = validated.title.clone();
            existing.date = date.clone();
            existing.time = time.clone();
            existing.links = validated.links.clone();
            existing.details = validated.details.clone();
            existing.updated_ms = now;
            return Ok(existing.clone());
        }
        let reminder = Reminder {
            id: new_id(),
            title: validated.title.clone(),
            date: date.clone(),
            time: time.clone(),
            links: validated.links.clone(),
            details: validated.details.clone(),
            kind: Default::default(),
            status: ReminderStatus::Pending,
            ring_at_ms: ring,
            source,
            created_ms: now,
            updated_ms: now,
        };
        list.push(reminder.clone());
        Ok(reminder)
    })?;
    changed(app);
    Ok(saved)
}

/// After any change: tell the webviews and re-arm the timer.
fn changed(app: &AppHandle) {
    if let Err(error) = RemindersChanged.emit(app) {
        log::warn!("[GRAIN] reminders: emit failed: {error}");
    }
    rearm(app);
}

fn find_mut<'a>(list: &'a mut [Reminder], id: &str) -> Result<&'a mut Reminder, String> {
    list.iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| "That reminder no longer exists.".to_string())
}

fn retry_at() -> std::sync::MutexGuard<'static, Option<u64>> {
    RETRY_AT.lock().unwrap_or_else(|e| e.into_inner())
}

// ── Timer ──────────────────────────────────────────────────────────────────

/// Replace the timer with one for the earliest pending reminder (or a notch
/// retry owed to a ringing one), or none.
pub(crate) fn rearm(app: &AppHandle) {
    let list = load(app).unwrap_or_default();
    let retry = if list.iter().any(|r| r.status == ReminderStatus::Ringing) {
        *retry_at()
    } else {
        retry_at().take();
        None
    };
    let next = [reminders::next_ring(&list), retry]
        .into_iter()
        .flatten()
        .min();
    let mut slot = TIMER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(old) = slot.take() {
        old.abort();
    }
    let Some(due) = next else {
        return;
    };
    let app = app.clone();
    *slot = Some(tauri::async_runtime::spawn(async move {
        loop {
            let now = now_ms();
            if now >= due {
                break;
            }
            tokio::time::sleep(Duration::from_millis((due - now).min(SLICE_MS))).await;
        }
        // Synchronous from here on: `fire` re-arms, which aborts this task's
        // handle, and an abort only lands at an await point.
        fire(&app);
    }));
}

/// Ring every pending reminder that is due, and open the notch for them (or
/// try again, when a retry is owed).
fn fire(app: &AppHandle) {
    let now = now_ms();
    let rang = mutate(app, |list| {
        let mut rang = 0;
        for reminder in list.iter_mut() {
            if reminder.status == ReminderStatus::Pending
                && reminder.ring_at_ms.is_some_and(|at| at <= now)
            {
                reminder.status = ReminderStatus::Ringing;
                reminder.ring_at_ms = None;
                rang += 1;
            }
        }
        Ok(rang)
    })
    .unwrap_or_else(|error| {
        log::warn!("[GRAIN] reminders: could not ring: {error}");
        0
    });
    let retry_due = retry_at().is_some_and(|at| at <= now);
    if rang > 0 || retry_due {
        log::info!("[GRAIN] reminders: {rang} newly ringing");
        retry_at().take();
        present(app);
    }
    changed(app);
}

/// Open the Agent notch on its reminder view. If it cannot open now, owe a
/// retry and re-arm. Window work runs on the main loop.
fn present(app: &AppHandle) {
    let handle = app.clone();
    let posted = app.run_on_main_thread(move || {
        if !crate::agent::open_notch_for_reminder(&handle) {
            *retry_at() = Some(now_ms() + RETRY_MS);
            rearm(&handle);
        }
    });
    if let Err(error) = posted {
        log::warn!("[GRAIN] reminders: could not reach the main loop: {error}");
    }
}

/// Start-up: re-arm (missed reminders ring at once) and reopen the notch for
/// any reminder still ringing when Grain closed.
pub(crate) fn start(app: &AppHandle) {
    let ringing = load(app)
        .map(|list| list.iter().any(|r| r.status == ReminderStatus::Ringing))
        .unwrap_or(false);
    if ringing {
        present(app);
    }
    rearm(app);
}

// ── Commands ───────────────────────────────────────────────────────────────

/// Every reminder, in no particular order (the tab groups and sorts).
#[tauri::command]
#[specta::specta]
pub async fn reminders_list(app: AppHandle) -> Result<Vec<Reminder>, String> {
    load(&app)
}

/// Create (no `id`) or edit a reminder.
#[tauri::command]
#[specta::specta]
pub async fn reminders_save(app: AppHandle, input: ReminderInput) -> Result<Reminder, String> {
    save(&app, &input, ReminderSource::User)
}

#[tauri::command]
#[specta::specta]
pub async fn reminders_delete(app: AppHandle, id: String) -> Result<(), String> {
    mutate(&app, |list| {
        list.retain(|r| r.id != id);
        Ok(())
    })?;
    changed(&app);
    Ok(())
}

/// Mark done, or bring a done reminder back (it rings again if still ahead).
#[tauri::command]
#[specta::specta]
pub async fn reminders_set_done(app: AppHandle, id: String, done: bool) -> Result<(), String> {
    let now = now_ms();
    mutate(&app, |list| {
        let reminder = find_mut(list, &id)?;
        if done {
            reminder.status = ReminderStatus::Done;
            reminder.ring_at_ms = None;
        } else {
            reminder.status = ReminderStatus::Pending;
            reminder.ring_at_ms = reminders::validate(&ReminderInput {
                id: None,
                title: reminder.title.clone(),
                date: reminder.date.clone(),
                time: reminder.time.clone(),
                links: Vec::new(),
                details: None,
            })
            .ok()
            .and_then(|validated| ring_at(&validated, now));
        }
        reminder.updated_ms = now;
        Ok(())
    })?;
    changed(&app);
    Ok(())
}

/// Ring again in `minutes`.
#[tauri::command]
#[specta::specta]
pub async fn reminders_snooze(app: AppHandle, id: String, minutes: u32) -> Result<(), String> {
    let minutes = minutes.clamp(1, reminders::MAX_SNOOZE_MINUTES);
    let now = now_ms();
    mutate(&app, |list| {
        let reminder = find_mut(list, &id)?;
        reminder.status = ReminderStatus::Pending;
        reminder.ring_at_ms = Some(now + u64::from(minutes) * 60_000);
        reminder.updated_ms = now;
        Ok(())
    })?;
    changed(&app);
    Ok(())
}

/// Remove every done reminder.
#[tauri::command]
#[specta::specta]
pub async fn reminders_clear_done(app: AppHandle) -> Result<(), String> {
    mutate(&app, |list| {
        list.retain(|r| r.status != ReminderStatus::Done);
        Ok(())
    })?;
    changed(&app);
    Ok(())
}

/// Open one of a reminder's stored links. The webview names the reminder and
/// the link's position; it never hands the host a URL to open.
#[tauri::command]
#[specta::specta]
pub async fn reminders_open_link(app: AppHandle, id: String, index: u32) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = load(&app)?
        .into_iter()
        .find(|r| r.id == id)
        .and_then(|r| r.links.into_iter().nth(index as usize))
        .ok_or("That link no longer exists.")?
        .url;
    let url = grain_core::reach::validate_link(&url)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Could not open the link: {e}"))
}

// ── Agent provider ─────────────────────────────────────────────────────────

/// Always listed: Reminders only writes Grain's own local file.
pub(crate) fn directory() -> ExtensionDirectoryEntry {
    ExtensionDirectoryEntry {
        extension_id: EXTENSION_ID.to_string(),
        name: reminders::PROVIDER_NAME.to_string(),
        // The Agent prompt carries no date; the directory line does.
        description: format!("{} Today is {}.", reminders::DESCRIPTION, today().label()),
        action_count: reminders::tools().len(),
    }
}

/// The compiled tool set and its digest. No I/O.
pub(crate) fn catalog() -> (String, Vec<crate::grain_reach::CatalogTool>) {
    let tools = reminders::tools()
        .into_iter()
        .map(|tool| crate::grain_reach::CatalogTool {
            action: ActionInput {
                extension_id: EXTENSION_ID.to_string(),
                action_id: tool.name.to_string(),
                canonical_id: format!("{EXTENSION_ID}:{}", tool.name),
                provider_name: reminders::PROVIDER_NAME.to_string(),
                title: tool.title.to_string(),
                aliases: vec![reminders::PROVIDER_NAME.to_string(), tool.name.to_string()],
                namespaces: Vec::new(),
                tags: tool.keywords.iter().map(|k| k.to_string()).collect(),
                examples: Vec::new(),
                phrases: grain_core::capability_agent::tool_search_phrases(tool.name),
                params: Vec::new(),
                when_to_use: String::new(),
                when_not_to_use: String::new(),
                description: format!(
                    "{} {}",
                    tool.description,
                    grain_core::capability_agent::schema_search_text(&tool.schema)
                ),
                provider_context: vec![reminders::DESCRIPTION.to_string()],
                risk: if tool.read_only {
                    ActionRisk::Safe
                } else {
                    ActionRisk::Confirm
                },
                enabled: true,
                platform_ok: true,
                quarantined: false,
            },
            description: tool.description.to_string(),
            parameters: tool.schema,
        })
        .collect();
    (reminders::catalog_digest(), tools)
}

fn failed(class: FailureClass, message: impl Into<String>) -> ActionOutcome {
    ActionOutcome::Failed {
        class,
        message: message.into(),
    }
}

fn succeeded(title: &str, body: String, receipt: bool) -> ActionOutcome {
    ActionOutcome::Succeeded(SuccessData {
        source: Some(reminders::PROVIDER_NAME.to_string()),
        title: Some(title.to_string()),
        body: Some(body),
        structured_content: None,
        details: Vec::new(),
        receipt,
    })
}

/// Run one Agent call. Every failure is a typed outcome the model can read.
pub(crate) async fn call(
    app: &AppHandle,
    action_id: &str,
    arguments: &serde_json::Value,
) -> ActionOutcome {
    let today = today();
    let parsed = match reminders::parse(action_id, arguments, today) {
        Ok(call) => call,
        Err(message) => return failed(FailureClass::InvalidArgument, message),
    };
    log::info!("[GRAIN] reminders: agent {action_id}");
    match parsed {
        Call::Create(input) => {
            let validated = match reminders::validate(&input) {
                Ok(validated) => validated,
                Err(message) => return failed(FailureClass::InvalidArgument, message),
            };
            if validated.time.is_some() && ring_at(&validated, now_ms()).is_none() {
                let now = chrono::Local::now();
                return failed(
                    FailureClass::InvalidArgument,
                    format!(
                        "That time has already passed. It is now {} {}.",
                        today.label(),
                        reminders::time_label(now.hour(), now.minute())
                    ),
                );
            }
            match save(app, &input, ReminderSource::Agent) {
                Ok(reminder) => {
                    let rings = if reminder.ring_at_ms.is_some() {
                        "Grain will ring then."
                    } else {
                        "It is listed for today; the all-day ring time has passed."
                    };
                    succeeded(
                        "Reminder set",
                        format!("Reminder set: {}. {rings}", reminders::describe(&reminder)),
                        true,
                    )
                }
                Err(message) => failed(FailureClass::Internal, message),
            }
        }
        Call::List { day } => {
            let list = match load(app) {
                Ok(list) => list,
                Err(message) => return failed(FailureClass::Internal, message),
            };
            succeeded("Reminders", agenda(list, today, day), false)
        }
    }
}

/// The model-facing list: one day, or everything not done from today on.
fn agenda(mut list: Vec<Reminder>, today: Date, day: Option<Date>) -> String {
    const MAX_LINES: usize = 25;
    let today_iso = today.iso();
    list.retain(|r| match day {
        Some(day) => r.date == day.iso(),
        None => r.status != ReminderStatus::Done && r.date >= today_iso,
    });
    list.sort_by(|a, b| {
        (&a.date, a.time.as_deref().unwrap_or("")).cmp(&(&b.date, b.time.as_deref().unwrap_or("")))
    });
    let mut out = format!("Today is {}.\n", today.label());
    if list.is_empty() {
        out.push_str(match day {
            Some(_) => "No reminders on that day.",
            None => "No upcoming reminders.",
        });
        return out;
    }
    for reminder in list.iter().take(MAX_LINES) {
        out.push_str("- ");
        out.push_str(&reminders::describe(reminder));
        out.push('\n');
    }
    if list.len() > MAX_LINES {
        out.push_str(&format!("…and {} more.", list.len() - MAX_LINES));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_core::reminders::{ReminderKind, ReminderLink};

    fn reminder(date: &str, time: Option<&str>, status: ReminderStatus) -> Reminder {
        Reminder {
            id: new_id(),
            title: format!("At {date}"),
            date: date.into(),
            time: time.map(str::to_string),
            links: vec![ReminderLink {
                url: "https://meet.google.com/abc".into(),
            }],
            details: None,
            kind: ReminderKind::Notify,
            status,
            ring_at_ms: None,
            source: ReminderSource::Agent,
            created_ms: 0,
            updated_ms: 0,
        }
    }

    #[test]
    fn agenda_lists_upcoming_in_order_and_skips_done_and_past() {
        let today = Date::new(2026, 11, 13).unwrap();
        let list = vec![
            reminder("2026-11-25", Some("18:00"), ReminderStatus::Pending),
            reminder("2026-11-14", None, ReminderStatus::Pending),
            reminder("2026-11-12", Some("09:00"), ReminderStatus::Pending),
            reminder("2026-11-20", Some("09:00"), ReminderStatus::Done),
        ];
        let text = agenda(list.clone(), today, None);
        assert!(text.starts_with("Today is Fri 13 Nov 2026."));
        let first = text.find("Sat 14 Nov").unwrap();
        let second = text.find("Wed 25 Nov").unwrap();
        assert!(first < second);
        assert!(!text.contains("12 Nov"));
        assert!(!text.contains("20 Nov"));
        assert!(text.contains("(1 link)"));

        let day = agenda(list, today, Date::new(2026, 11, 20));
        assert!(day.contains("[done]"));
        let empty = agenda(Vec::new(), today, None);
        assert!(empty.ends_with("No upcoming reminders."));
    }

    #[test]
    fn ring_time_is_local_and_never_in_the_past() {
        let date = Date::new(2030, 6, 1).unwrap();
        let timed = Validated {
            title: "x".into(),
            date,
            time: Some((18, 0)),
            links: Vec::new(),
            details: None,
        };
        let at = ring_at(&timed, 0).unwrap();
        let local = chrono::Local.timestamp_millis_opt(at as i64).unwrap();
        assert_eq!((local.hour(), local.minute()), (18, 0));
        assert_eq!(local.day(), 1);
        // All-day rings at the default hour.
        let all_day = Validated {
            time: None,
            ..timed.clone()
        };
        let at = ring_at(&all_day, 0).unwrap();
        let local = chrono::Local.timestamp_millis_opt(at as i64).unwrap();
        assert_eq!(local.hour(), reminders::ALL_DAY_RING_HOUR);
        // Past instants never ring.
        assert_eq!(ring_at(&timed, u64::MAX), None);
    }

    #[test]
    fn store_round_trips_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        assert!(read(&path).reminders.is_empty());
        let store = Store {
            reminders: vec![reminder(
                "2026-11-25",
                Some("18:00"),
                ReminderStatus::Pending,
            )],
        };
        write(&path, &store).unwrap();
        assert_eq!(read(&path).reminders, store.reminders);
        std::fs::write(&path, b"{not json").unwrap();
        assert!(read(&path).reminders.is_empty());
    }
}
