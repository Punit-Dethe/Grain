//! [GRAIN] The action log (`docs/Extensions V1/PLAN.md`).
//!
//! Every request that reached an extension, whatever became of it: what was
//! heard, who it went to, the score, and the outcome. It is three things at
//! once — the "why did that happen" surface a user needs when something
//! surprises them, the corpus the eval harness replays, and the only record
//! that an irreversible action ran at all.
//!
//! # It is a transcript store, and is treated as one
//!
//! "What was heard" is speech, so this file is as sensitive as dictation
//! history and gets the same handling: **in memory only, hard-capped, clearable
//! in one action, and excluded from every diagnostics bundle.** Nothing here is
//! written to disk and nothing leaves the machine.
//!
//! Bounded rather than rotated on purpose. A ring of the most recent entries
//! answers "what just happened", which is the question people actually ask; a
//! growing archive of everything ever spoken to Grain answers a question nobody
//! asked and creates a liability that has to be defended forever.

use std::sync::{Mutex, OnceLock};

/// How many entries are kept. Enough to explain a session, small enough that
/// the whole thing is a rounding error in memory and can be handed to the eval
/// harness whole.
const CAPACITY: usize = 200;

/// What became of one routed request. Mirrors the decision layer's outcomes,
/// flattened for display.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ActionLogOutcome {
    /// It ran. `confirmed` records whether the user read it back first.
    Ran { confirmed: bool },
    /// The user was asked and picked.
    Chose,
    /// The user was asked and walked away. **The single most useful signal in
    /// here**: a route that is regularly offered and regularly declined is one
    /// the ranking is getting wrong, and it is what feeds the misroute counter.
    Cancelled,
    /// Handed to the Agent.
    Escalated,
    /// Nothing installed could do it, or the capture was unusable.
    Refused { reason: String },
    /// It ran and failed for a reason worth showing.
    Failed { reason: String },
    /// The deadline passed with the call already in flight — it may well have
    /// happened. Kept distinct from `Failed` because telling someone their
    /// message failed when it was sent is worse than admitting uncertainty.
    Unknown,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActionLogEntry {
    /// Milliseconds since the Unix epoch, for ordering and display only.
    pub at: i64,
    /// What the acoustic model produced, before any routing. The whole point of
    /// the log: when an action surprises someone, this is usually the answer.
    pub heard: String,
    /// `<extension>:<action>`, when one was chosen.
    pub action: Option<String>,
    /// The user-facing title, so the log reads without a manifest.
    pub title: Option<String>,
    pub score: Option<f32>,
    pub outcome: ActionLogOutcome,
}

fn log() -> &'static Mutex<std::collections::VecDeque<ActionLogEntry>> {
    static LOG: OnceLock<Mutex<std::collections::VecDeque<ActionLogEntry>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(std::collections::VecDeque::with_capacity(CAPACITY)))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Record one routed request. Never fails and never blocks anything: a log that
/// can break the feature it observes is worse than no log.
pub fn record(
    heard: &str,
    action: Option<String>,
    title: Option<String>,
    score: Option<f32>,
    outcome: ActionLogOutcome,
) {
    let entry = ActionLogEntry {
        at: now_ms(),
        heard: heard.to_string(),
        action,
        title,
        score,
        outcome,
    };
    let Ok(mut log) = log().lock() else {
        return;
    };
    if log.len() >= CAPACITY {
        log.pop_front();
    }
    log.push_back(entry);
}

/// Newest first, which is the order anyone reading this wants.
pub fn entries() -> Vec<ActionLogEntry> {
    log()
        .lock()
        .map(|log| log.iter().rev().cloned().collect())
        .unwrap_or_default()
}

/// Forget everything. One action, no confirmation dance — this is the user's
/// own speech and asking twice before letting them delete it is the wrong
/// default.
pub fn clear() {
    if let Ok(mut log) = log().lock() {
        log.clear();
    }
}

/// How many times this was offered and declined.
///
/// The misroute signal: something the user is repeatedly shown and repeatedly
/// walks away from is one the ranking has wrong. Counted from the log rather
/// than kept as its own counter so it clears when the log does — a quarantine
/// the user cannot reset is a trap.
///
/// Unreferenced until V1-P1 puts a recommendation on screen for the user to
/// walk away from; the signal is kept because it is the honest one.
#[allow(dead_code)]
pub fn declines(qualified: &str) -> usize {
    log()
        .lock()
        .map(|log| {
            log.iter()
                .filter(|e| {
                    e.action.as_deref() == Some(qualified)
                        && matches!(e.outcome, ActionLogOutcome::Cancelled)
                })
                .count()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The log is process-global, and `cargo test` runs these in parallel — so
    /// without this the two tests below clear each other's entries and fail
    /// intermittently, which is worse than failing.
    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        static SERIAL: Mutex<()> = Mutex::new(());
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn ran(heard: &str, action: &str) {
        record(
            heard,
            Some(action.to_string()),
            Some("Skip".into()),
            Some(1.0),
            ActionLogOutcome::Ran { confirmed: false },
        );
    }

    #[test]
    fn the_log_is_bounded_and_newest_first() {
        let _serial = exclusive();
        clear();
        for i in 0..(CAPACITY + 25) {
            ran(&format!("skip {i}"), "spotify:next");
        }
        let recorded = entries();
        assert_eq!(recorded.len(), CAPACITY, "the ring has to actually bound");
        assert_eq!(
            recorded[0].heard,
            format!("skip {}", CAPACITY + 24),
            "newest first is the order anyone reading this wants"
        );
        clear();
        assert!(entries().is_empty());
    }

    #[test]
    fn declines_count_only_the_ones_the_user_walked_away_from() {
        // The misroute signal has to distinguish "shown and refused" from
        // "shown and used", or every popular action looks broken.
        let _serial = exclusive();
        clear();
        ran("skip this", "spotify:next");
        for _ in 0..3 {
            record(
                "skip this",
                Some("spotify:next".into()),
                None,
                Some(0.7),
                ActionLogOutcome::Cancelled,
            );
        }
        record(
            "skip this",
            Some("apple:next".into()),
            None,
            Some(0.7),
            ActionLogOutcome::Cancelled,
        );
        assert_eq!(declines("spotify:next"), 3);
        assert_eq!(declines("apple:next"), 1);
        assert_eq!(declines("slack:send_dm"), 0);
        clear();
        assert_eq!(
            declines("spotify:next"),
            0,
            "clearing the log has to clear the quarantine signal with it"
        );
    }
}
