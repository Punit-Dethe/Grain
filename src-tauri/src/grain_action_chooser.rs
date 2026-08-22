//! [GRAIN] The action chooser (`docs/Action Routing/PLAN.md` §7).
//!
//! One component, four uses: pick an action, pick a provider, pick an entity,
//! and read a destructive action back before it runs. They are the same
//! interaction — *here are N things, which one* — and building them separately
//! would mean four places for the key grabs to be wrong.
//!
//! # Why it is the switcher's capsule and not a window
//!
//! Native, beside the pill, on the same window Grain already owns. A webview
//! would cost a second window, steal focus (the Agent panel's long-standing
//! pain), and add RAM to a surface that must stay flat. `master_key.rs` already
//! solved this shape for the prompt switcher; this reuses it wholesale.
//!
//! # The key grabs are the dangerous part
//!
//! Registering a global shortcut **synchronously inside a shortcut action
//! deadlocks every global shortcut in the app.** This codebase has paid for
//! that once already. Every register and unregister here goes through
//! [`deferred_register`]/[`deferred_unregister`], which hop onto the async
//! runtime first, and each re-checks the open flag after landing so a race
//! against a cancel releases the key instead of stranding it.

use grain_core::action_decision::Selection;
use grain_core::DaemonEvent;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::AppHandle;

use crate::settings::ShortcutBinding;

/// How long the capsule stays up with no keypress.
///
/// Much longer than the prompt switcher's 3.5 s, because the two ask different
/// things of the user: the switcher cycles a label they already know, this one
/// asks them to *read* options and decide. Timing out mid-read would be its own
/// bug, and the cost of waiting is a capsule on screen.
const IDLE_MS: u64 = 12_000;

/// The most options the capsule will show. Matches the decision layer's own
/// clarification cap — past a handful there is no honest question to ask.
pub(crate) const MAX_OPTIONS: usize = 4;

/// Digit keys, in order. One grab per visible option and no more: grabbing keys
/// that do nothing would take them from whatever the user is actually doing.
const DIGITS: [&str; MAX_OPTIONS] = ["1", "2", "3", "4"];
const CANCEL_KEY: &str = "escape";

fn binding_id(index: usize) -> String {
    format!("action_choice_{index}")
}
const CANCEL_ID: &str = "action_choice_cancel";

/// What the user is being asked, and what to do with the answer.
#[derive(Clone, Debug)]
pub enum Pending {
    /// Several different requests are plausible, or several providers can serve
    /// the one that is. Picking runs that selection.
    Pick {
        heard: String,
        kind: &'static str,
        options: Vec<Selection>,
    },
    /// The extension resolved a span to several candidates of its own. Picking
    /// re-runs the same action with that span pinned.
    Entity {
        heard: String,
        selection: Selection,
        param: String,
        options: Vec<String>,
    },
    /// A destructive action, read back before it runs.
    ///
    /// Modelled as a one-option pick rather than a yes/no with Enter, because
    /// grabbing Enter globally is how the Agent panel got into trouble — and
    /// because "press 1 to send this" and "press Escape" is the same muscle
    /// memory as every other capsule.
    Confirm { heard: String, selection: Selection },
}

impl Pending {
    fn kind(&self) -> &'static str {
        match self {
            Pending::Pick { kind, .. } => kind,
            Pending::Entity { .. } => "entity",
            Pending::Confirm { .. } => "confirm",
        }
    }

    fn heard(&self) -> &str {
        match self {
            Pending::Pick { heard, .. }
            | Pending::Entity { heard, .. }
            | Pending::Confirm { heard, .. } => heard,
        }
    }

    /// The lines the capsule shows. Never the utterance list, never an id — the
    /// user is choosing between things, not between declarations.
    fn labels(&self) -> Vec<String> {
        match self {
            Pending::Pick { options, kind, .. } => options
                .iter()
                .map(|s| {
                    // For a provider choice the action is already decided, so
                    // repeating its title in every row is noise; the extension
                    // is the only thing that differs.
                    if *kind == "provider" {
                        s.extension_id.clone()
                    } else {
                        s.title.clone()
                    }
                })
                .collect(),
            Pending::Entity { options, .. } => options.clone(),
            Pending::Confirm { selection, .. } => vec![describe(selection)],
        }
    }

    fn len(&self) -> usize {
        self.labels().len().min(MAX_OPTIONS)
    }
}

/// A destructive action, spelled out with its **resolved** parameters.
///
/// Resolved, not raw: reading back "send a message" tells the user nothing they
/// did not already intend, while "Send 'running late' to Jack" is the sentence
/// that catches an ASR substitution before it is somebody else's problem.
fn describe(selection: &Selection) -> String {
    if selection.spans.is_empty() {
        return selection.title.clone();
    }
    let detail = selection
        .spans
        .values()
        .map(|v| format!("\u{201c}{v}\u{201d}"))
        .collect::<Vec<_>>()
        .join(" \u{2192} ");
    format!("{} — {detail}", selection.title)
}

fn pending() -> &'static Mutex<Option<Pending>> {
    static PENDING: OnceLock<Mutex<Option<Pending>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

/// Whether the capsule is up (and therefore whether digits are grabbed).
static OPEN: AtomicBool = AtomicBool::new(false);
/// Bumped on every open and every keypress; the idle-close task only fires if
/// its captured generation still matches.
static GEN: AtomicU64 = AtomicU64::new(0);

/// Ask. Returns false when there is already a question on screen — one at a
/// time, because two stacked capsules have no unambiguous "1".
pub fn open(app: &AppHandle, question: Pending) -> bool {
    if OPEN.load(Ordering::SeqCst) {
        log::warn!("[GRAIN] action: a choice is already open; ignoring the new one");
        return false;
    }
    let count = question.len();
    if count == 0 {
        return false;
    }
    let mut labels = question.labels();
    labels.truncate(MAX_OPTIONS);

    crate::bridge::emit(
        app,
        DaemonEvent::ActionChoice {
            kind: question.kind().to_string(),
            heard: question.heard().to_string(),
            options: labels,
        },
    );
    *pending().lock().unwrap() = Some(question);
    OPEN.store(true, Ordering::SeqCst);

    for index in 0..count {
        deferred_register(app, &binding_id(index), DIGITS[index], "Choose");
    }
    deferred_register(app, CANCEL_ID, CANCEL_KEY, "Cancel");
    arm_idle_close(app);
    true
}

/// The user pressed a digit. `index` is zero-based.
pub fn pick(app: &AppHandle, index: usize) {
    let Some(question) = take(app) else {
        return;
    };
    match question {
        Pending::Pick { heard, options, .. } => match options.into_iter().nth(index) {
            Some(selection) => {
                crate::grain_actions::action_session::run_choice(app, heard, selection)
            }
            None => log::warn!("[GRAIN] action: choice {index} is out of range"),
        },
        Pending::Confirm { heard, selection } => {
            // Only reachable by an explicit keypress on a read-back the user
            // just saw. That is the entire safety property of the confirm tier.
            crate::grain_actions::action_session::run_choice(app, heard, selection)
        }
        Pending::Entity {
            heard,
            mut selection,
            param,
            options,
        } => match options.into_iter().nth(index) {
            Some(value) => {
                // Pin the span the extension resolved and run the same action
                // again. Its resolver now sees an exact match, so it cannot come
                // back ambiguous a second time.
                selection.spans.insert(param, value);
                crate::grain_actions::action_session::run_choice(app, heard, selection)
            }
            None => log::warn!("[GRAIN] action: entity choice {index} is out of range"),
        },
    }
}

/// Escape, a timeout, or anything else that ends the question without an answer.
///
/// Logged as a decline rather than silently dropped: an action repeatedly
/// offered and repeatedly walked away from is the clearest signal the ranking
/// has it wrong, and that signal only exists if this path records it.
pub fn cancel(app: &AppHandle) {
    let Some(question) = take(app) else {
        return;
    };
    let (heard, selection) = match &question {
        Pending::Pick { heard, options, .. } => (heard.clone(), options.first().cloned()),
        Pending::Entity {
            heard, selection, ..
        }
        | Pending::Confirm { heard, selection } => (heard.clone(), Some(selection.clone())),
    };
    crate::grain_actions::action_session::record_decline(&heard, selection.as_ref());
}

/// Whether a question is on screen. Used by the session's busy check — starting
/// a new capture while one is open would strand the capsule.
#[allow(dead_code)]
pub fn is_open() -> bool {
    OPEN.load(Ordering::SeqCst)
}

/// Close the capsule and release every key, whatever the outcome.
///
/// The single exit. Every way a question can end — answered, cancelled, timed
/// out — goes through here, so a stranded key grab has one place to be wrong
/// rather than four.
fn take(app: &AppHandle) -> Option<Pending> {
    if !OPEN.swap(false, Ordering::SeqCst) {
        return None;
    }
    GEN.fetch_add(1, Ordering::SeqCst);
    for index in 0..MAX_OPTIONS {
        deferred_unregister(app, &binding_id(index), DIGITS[index]);
    }
    deferred_unregister(app, CANCEL_ID, CANCEL_KEY);
    crate::bridge::emit(app, DaemonEvent::ActionChoiceClosed);
    pending().lock().unwrap().take()
}

fn arm_idle_close(app: &AppHandle) {
    let generation = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(IDLE_MS)).await;
        if OPEN.load(Ordering::SeqCst) && GEN.load(Ordering::SeqCst) == generation {
            log::info!("[GRAIN] action: choice timed out");
            cancel(&app);
        }
    });
}

// ── Deferred plugin plumbing (mirrors master_key.rs) ────────────────────────

fn transient_binding(id: &str, key: &str, name: &str) -> ShortcutBinding {
    ShortcutBinding {
        id: id.to_string(),
        name: name.to_string(),
        description: "Transient action-choice shortcut.".to_string(),
        default_binding: key.to_string(),
        current_binding: key.to_string(),
    }
}

/// Register off the dispatch thread, then re-check [`OPEN`]: if the question was
/// answered while this task was queued, release the key immediately rather than
/// leaving a digit grabbed with nothing behind it.
fn deferred_register(app: &AppHandle, id: &str, key: &str, name: &str) {
    let app = app.clone();
    let binding = transient_binding(id, key, name);
    tauri::async_runtime::spawn(async move {
        match crate::shortcut::register_shortcut(&app, binding.clone()) {
            Ok(()) => {
                if !OPEN.load(Ordering::SeqCst) {
                    let _ = crate::shortcut::unregister_shortcut(&app, binding);
                }
            }
            // Non-fatal: the capsule is still readable and Escape still works.
            // A digit the OS would not give us is one option the user has to
            // reach another way, not a broken app.
            Err(error) => log::warn!(
                "[GRAIN] action: couldn't grab '{}': {error}",
                binding.current_binding
            ),
        }
    });
}

fn deferred_unregister(app: &AppHandle, id: &str, key: &str) {
    let app = app.clone();
    let binding = transient_binding(id, key, "");
    tauri::async_runtime::spawn(async move {
        let _ = crate::shortcut::unregister_shortcut(&app, binding);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use grain_sdk::manifest::ActionRisk;
    use std::collections::BTreeMap;

    fn selection(ext: &str, title: &str, spans: &[(&str, &str)]) -> Selection {
        Selection {
            extension_id: ext.into(),
            action_id: "act".into(),
            domain: "media".into(),
            title: title.into(),
            risk: ActionRisk::Safe,
            spans: spans
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect::<BTreeMap<_, _>>(),
            missing: Vec::new(),
            score: 1.0,
        }
    }

    #[test]
    fn a_provider_choice_lists_who_not_what() {
        // The action is already decided, so repeating its title in every row is
        // noise — the extension is the only thing that differs.
        let question = Pending::Pick {
            heard: "skip this".into(),
            kind: "provider",
            options: vec![
                selection("spotify", "Skip to the next track", &[]),
                selection("apple", "Skip to the next track", &[]),
            ],
        };
        assert_eq!(question.labels(), vec!["spotify", "apple"]);
    }

    #[test]
    fn an_action_choice_lists_what_not_who() {
        let question = Pending::Pick {
            heard: "next".into(),
            kind: "action",
            options: vec![
                selection("spotify", "Skip to the next track", &[]),
                selection("deck", "Advance the slide", &[]),
            ],
        };
        assert_eq!(
            question.labels(),
            vec!["Skip to the next track", "Advance the slide"]
        );
    }

    #[test]
    fn a_read_back_spells_out_the_resolved_parameters() {
        // "Send a message" tells the user nothing they did not already intend.
        // The resolved values are the whole point — they are what catches an
        // ASR substitution before it becomes somebody else's problem.
        let question = Pending::Confirm {
            heard: "tell jack i am running late".into(),
            selection: selection(
                "slack",
                "Send a direct message",
                &[("message", "i am running late"), ("who", "jack")],
            ),
        };
        let shown = question.labels().remove(0);
        assert!(shown.contains("Send a direct message"), "{shown}");
        assert!(shown.contains("jack"), "{shown}");
        assert!(shown.contains("i am running late"), "{shown}");
    }

    #[test]
    fn more_options_than_keys_are_truncated_not_shown_unreachable() {
        // Grabbing only the digits that map to something is deliberate; showing
        // a fifth row with no key behind it would be worse than not showing it.
        let question = Pending::Entity {
            heard: "open mail".into(),
            selection: selection("voice-actions", "Open", &[]),
            param: "target".into(),
            options: (0..9).map(|i| format!("mail {i}")).collect(),
        };
        assert_eq!(question.len(), MAX_OPTIONS);
    }
}
