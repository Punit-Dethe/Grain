//! [GRAIN] Grain Space reminder scheduler.
//!
//! Zero-overhead-when-idle design: there is NO polling loop and NO resident
//! thread. [`sync`] scans the notes once, fires anything already due, and — if
//! (and only if) a future `Armed` reminder exists — parks ONE async timer on
//! the shared runtime until the earliest `fire_at`. Every call bumps a
//! generation counter, so a parked timer that has been superseded (reminder
//! edited/dismissed, feature toggled off, newer sync) wakes, notices, and
//! exits without doing anything. Feature disabled ⇒ `sync` returns before
//! touching the disk.

use std::sync::atomic::{AtomicU64, Ordering};

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use super::backend;
use super::store::{Note, ReminderStatus};

/// Bumped on every sync; a parked timer only acts if it still holds the
/// latest generation.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Re-evaluate the reminder schedule. Call after any mutation that can touch
/// reminder state (capture, save, delete, arm/dismiss), on app start, and on
/// the feature toggle. Cheap: one notes scan, only when the feature is on.
pub fn sync(app: &AppHandle) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if !super::is_enabled(app) {
        return; // parked timers die on generation mismatch; nothing else lives
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run_cycle(app, generation).await;
    });
}

async fn run_cycle(app: AppHandle, generation: u64) {
    let Ok(be) = backend::resolve(&app) else {
        // Vault backend not configured yet — nothing to schedule.
        return;
    };

    // Scan + fire-due in one blocking hop (store I/O off the async runtime).
    let scan = tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<ScanOutcome> {
        let now = chrono::Utc::now().timestamp_millis();
        let notes = backend::list_notes(&be)?;
        let mut due: Vec<Note> = Vec::new();
        let mut next_fire: Option<i64> = None;
        for note in notes {
            if note.reminder_state.status != ReminderStatus::Armed {
                continue;
            }
            let Some(fire_at) = note.reminder_state.fire_at else {
                continue;
            };
            if fire_at <= now {
                let mut state = note.reminder_state.clone();
                state.status = ReminderStatus::Fired;
                let updated = backend::set_reminder(&be, &note.id, state)?;
                due.push(updated);
            } else {
                next_fire = Some(next_fire.map_or(fire_at, |n: i64| n.min(fire_at)));
            }
        }
        Ok(ScanOutcome { due, next_fire })
    })
    .await;

    let outcome = match scan {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            log::error!("[GRAIN] reminder scan failed: {e:#}");
            return;
        }
        Err(e) => {
            log::error!("[GRAIN] reminder scan panicked: {e}");
            return;
        }
    };

    if !outcome.due.is_empty() {
        for note in &outcome.due {
            notify(&app, note);
        }
        super::emit_notes_changed(&app);
    }

    let Some(fire_at) = outcome.next_fire else {
        return; // nothing scheduled — nothing stays alive
    };

    let delay_ms = (fire_at - chrono::Utc::now().timestamp_millis()).max(0) as u64;
    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
    if GENERATION.load(Ordering::SeqCst) != generation {
        return; // superseded while parked
    }
    // Still current: re-scan, which fires the now-due reminder and re-arms.
    Box::pin(run_cycle(app, generation)).await;
}

struct ScanOutcome {
    due: Vec<Note>,
    next_fire: Option<i64>,
}

fn notify(app: &AppHandle, note: &Note) {
    let title = if note.title.trim().is_empty() {
        "Grain Space reminder".to_string()
    } else {
        note.title.clone()
    };
    let body = if note.tldr.trim().is_empty() {
        note.body.chars().take(120).collect::<String>()
    } else {
        note.tldr.clone()
    };
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::error!("[GRAIN] reminder notification failed: {e}");
    }
}
