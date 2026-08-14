//! [GRAIN] Paste Catch — the missed-field safety net.
//!
//! When a dictation paste lands somewhere that cannot receive text — the
//! desktop, a rendered web page, a PDF viewer, a button — the synthesized
//! `Ctrl+V` is a no-op and the transcript is **destroyed**, not merely
//! un-pasted: upstream's `paste_via_clipboard` restores the user's previous
//! clipboard afterwards, and clears the clipboard outright when there was
//! nothing to restore. The words the user just spoke survive only in history.
//!
//! This module catches that. On a confirmed miss the transcript is held on the
//! clipboard behind a visible offer for a grace period; the user moves to a real
//! field and presses the deliver key (or plain `Ctrl+V` — the transcript really
//! is on the clipboard). When the grace period ends, the clipboard is handed
//! back **exactly** as Handy would have left it.
//!
//! # Detection
//!
//! Two probes that fail differently, neither trusted alone:
//!
//! - **Pre-flight** ([`intercept`], top of `clipboard::paste`): classify the
//!   focused element. Only `NotEditable` — positive evidence — takes ownership,
//!   and then the OS paste is never sent at all, which also stops a stray
//!   `Ctrl+V` from firing a real command in a game or a CAD app.
//! - **Post-flight** ([`verify`], bottom of `clipboard::paste`): ground truth.
//!   Did the text actually arrive at the caret?
//!
//! The governing rule is asymmetric: **act only on positive evidence of a
//! miss.** A false positive interrupts a user who pasted fine *and* holds their
//! clipboard for the grace period; a false negative merely reproduces today's
//! behaviour. So every ambiguous reading is silent.
//!
//! # Why the clipboard rather than a private buffer
//!
//! Holding the transcript on the real clipboard means plain `Ctrl+V` works in
//! every application, including those that reject synthesized input, and it
//! needs no delivery machinery at all. The cost — occupying the user's clipboard
//! for the grace period — is bounded by the TTL and abandoned the moment they
//! copy something else.

use grain_core::DaemonEvent;
use log::{info, warn};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::context_detect::{focus_target, FocusTarget};
use crate::settings::{get_settings, ClipboardHandling, PasteMethod, ShortcutBinding};

/// Settings id of the deliver binding. Registered only while a hold is armed —
/// see `grain_core::capture::is_dynamic_binding`.
const BINDING_ID: &str = "paste_catch_deliver";

/// How long to wait after the paste before asking the field what it holds.
/// Applications process a paste asynchronously; this is measured from a point
/// Grain controls (the bottom of `paste()`), not inferred from upstream's delay
/// constants, so it stays correct if upstream re-times that body.
const VERIFY_SETTLE_MS: u64 = 150;

/// Below this length the tail match is too weak to trust in either direction —
/// a three-character transcript can match existing text by coincidence.
const MIN_VERIFY_CHARS: usize = 8;

/// How much of the transcript's tail must be present at the caret. Enough to be
/// unambiguous, short enough to survive an application trimming the edges.
const TAIL_CHARS: usize = 24;

/// Modifier hold for a manual delivery chord, at parity with the paste paths.
const CHORD_HOLD_MS: u64 = 100;

/// What the clipboard held before Paste Catch took it over. Mirrors upstream's
/// legacy restore: text wins, an image is only captured when there is no text
/// (reading one decodes the full bitmap), and "nothing" is a real case.
enum Snapshot {
    Empty,
    Text(String),
    Image(tauri::image::Image<'static>),
}

/// One armed hold.
struct Held {
    text: String,
    saved: Snapshot,
    /// Clipboard sequence number right after publishing. If it has moved by the
    /// time we act, the user copied something else and their copy wins.
    seq_at_publish: u32,
    /// Generation, so a superseded hold's timer cannot expire a newer one.
    gen: u64,
}

#[derive(Default)]
pub struct PasteCatchState {
    held: Mutex<Option<Held>>,
    gen: AtomicU64,
}

// -- Pure logic (unit-tested; no COM, no Tauri) --------------------------------

/// Collapse whitespace runs to single spaces and trim. Applications reflow
/// pasted text more often than they alter it — a chat composer that strips a
/// newline has still received the paste.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            in_space = true;
        } else {
            if in_space && !out.is_empty() {
                out.push(' ');
            }
            in_space = false;
            out.push(ch);
        }
    }
    out
}

/// The normalized last `n` CHARACTERS of the transcript (not bytes — this text
/// is arbitrary Unicode).
fn tail_of(text: &str, n: usize) -> String {
    let normalized = normalize(text);
    let count = normalized.chars().count();
    if count <= n {
        return normalized;
    }
    normalized.chars().skip(count - n).collect()
}

/// Did the paste land? Compares the caret's preceding text against the
/// transcript's tail.
///
/// Uses the caret NEIGHBOURHOOD rather than the field's full text on purpose:
/// `read_focused_value` truncates from the start, so a paste at the end of a
/// long document would fall outside the window entirely.
fn landed(caret_before: &str, transcript: &str) -> bool {
    let tail = tail_of(transcript, TAIL_CHARS);
    if tail.is_empty() {
        // Nothing to look for — no evidence of a miss, so do not act.
        return true;
    }
    normalize(caret_before).ends_with(&tail)
}

/// Whether a paste is worth verifying at all.
///
/// `auto_submit` is the notable exclusion: a successful paste *clears* the
/// field, which is indistinguishable from a miss, so verification would report
/// a false miss on every successful send. `reliable_paste` is excluded because
/// `paste_tx` still owns the clipboard as a delayed-render promise — reading it
/// would return the transcript rather than the user's content AND forge a read
/// receipt that makes `paste_tx` conclude the paste succeeded.
fn should_verify(
    enabled: bool,
    reliable_paste: bool,
    auto_submit: bool,
    method_verifiable: bool,
    transcript_chars: usize,
) -> bool {
    enabled
        && !reliable_paste
        && !auto_submit
        && method_verifiable
        && transcript_chars >= MIN_VERIFY_CHARS
}

/// Paste methods whose outcome Grain can reason about. `None` pastes nothing by
/// design; `ExternalScript` hands off to code Grain cannot model.
fn method_verifiable(method: PasteMethod) -> bool {
    matches!(
        method,
        PasteMethod::CtrlV
            | PasteMethod::CtrlShiftV
            | PasteMethod::ShiftInsert
            | PasteMethod::Direct
    )
}

// -- The two hooks -------------------------------------------------------------

/// Pre-flight. Called at the top of `clipboard::paste`.
///
/// Returns `true` when Grain has taken ownership of the transcript — focus is
/// provably not editable, so the caller must NOT paste.
pub fn intercept(app: &AppHandle, text: &str) -> bool {
    let settings = get_settings(app);
    if !settings.paste_catch_enabled || text.trim().is_empty() {
        return false;
    }
    // A new transcript supersedes any held one: hand the old clipboard back now,
    // so a stale hold's restore cannot land on top of what is about to happen.
    supersede(app);

    // `PasteMethod::None` means the user asked for no paste at all. Catching a
    // paste that was never going to happen would be a surprise, not a rescue.
    if settings.paste_method == PasteMethod::None {
        return false;
    }
    if focus_target() != FocusTarget::NotEditable {
        return false;
    }
    info!("[paste-catch] focus is not editable — holding the transcript instead of pasting");
    arm(app, text);
    true
}

/// Post-flight. Called at the bottom of `clipboard::paste`, after the paste and
/// upstream's own clipboard handling have finished.
///
/// Fire-and-forget: the check runs on a short-lived worker so it costs the user
/// no latency, and every failure path is silent.
pub fn verify(app: &AppHandle, text: &str) {
    let settings = get_settings(app);
    if !should_verify(
        settings.paste_catch_enabled,
        settings.reliable_paste,
        settings.auto_submit,
        method_verifiable(settings.paste_method),
        text.chars().count(),
    ) {
        return;
    }

    let app = app.clone();
    let text = text.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(VERIFY_SETTLE_MS));
        // No caret to read means no evidence — not a miss. Password fields
        // return `None` here too, and are deliberately never held.
        let Some(caret) = crate::context_detect::read_caret_now() else {
            return;
        };
        if landed(&caret.before, &text) {
            return;
        }
        info!("[paste-catch] transcript is not at the caret — the paste missed; holding it");
        arm(&app, &text);
    });
}

// -- Hold lifecycle ------------------------------------------------------------

/// Snapshot whatever the clipboard holds right now.
fn snapshot(app: &AppHandle) -> Snapshot {
    let clipboard = app.clipboard();
    if let Some(text) = clipboard.read_text().ok().filter(|t| !t.is_empty()) {
        return Snapshot::Text(text);
    }
    // Only probed when there is no text: reading an image decodes the full
    // bitmap, and text is by far the common case.
    match clipboard.read_image().ok().map(|image| image.to_owned()) {
        Some(image) => Snapshot::Image(image),
        None => Snapshot::Empty,
    }
}

/// Arm a fresh hold, snapshotting the user's current clipboard first.
fn arm(app: &AppHandle, text: &str) {
    // Hand any older hold back before snapshotting, or the snapshot would
    // capture the PREVIOUS transcript instead of the user's own clipboard.
    supersede(app);
    let saved = snapshot(app);
    arm_with(app, text.to_string(), saved);
}

/// Arm a hold over an already-captured snapshot. Split out for re-arming after a
/// delivery that missed again: at that moment the clipboard holds the
/// transcript, so re-snapshotting would lose the user's content.
fn arm_with(app: &AppHandle, text: String, saved: Snapshot) {
    let Some(state) = app.try_state::<PasteCatchState>() else {
        return;
    };
    if let Err(e) = app.clipboard().write_text(text.clone()) {
        warn!("[paste-catch] could not publish the held transcript: {e}");
        return;
    }

    let gen = state.gen.fetch_add(1, Ordering::SeqCst) + 1;
    let chars = text.chars().count() as u32;
    let held = Held {
        text,
        saved,
        seq_at_publish: clipboard_sequence(),
        gen,
    };
    match state.held.lock() {
        Ok(mut slot) => *slot = Some(held),
        Err(_) => return,
    }

    let shortcut = deliver_binding(app).current_binding;
    register_deliver_deferred(app);
    crate::bridge::emit(app, DaemonEvent::PasteMissed { shortcut, chars });

    let ttl = Duration::from_millis(get_settings(app).paste_catch_hold_ms);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(ttl);
        expire(&app, gen);
    });
}

/// The TTL elapsed. Only the hold that armed this timer may be expired by it.
fn expire(app: &AppHandle, gen: u64) {
    let Some(state) = app.try_state::<PasteCatchState>() else {
        return;
    };
    let held = {
        let Ok(mut slot) = state.held.lock() else {
            return;
        };
        match slot.as_ref() {
            Some(held) if held.gen == gen => slot.take(),
            _ => None,
        }
    };
    if let Some(held) = held {
        hand_off(app, held, "hold expired");
    }
}

/// Drop any armed hold and hand the clipboard back. Called when a new
/// transcript arrives, so a stale restore cannot clobber it.
pub fn supersede(app: &AppHandle) {
    let Some(state) = app.try_state::<PasteCatchState>() else {
        return;
    };
    let held = state.held.lock().ok().and_then(|mut slot| slot.take());
    if let Some(held) = held {
        hand_off(app, held, "superseded");
    }
}

/// Hand the clipboard back **exactly** the way Handy would have left it, then
/// withdraw the offer.
fn hand_off(app: &AppHandle, held: Held, reason: &str) {
    let clipboard = app.clipboard();
    if !still_ours(app, &held) {
        // The user copied something else while we were holding. Their action
        // wins — the same rule `paste_tx` applies via the sequence number.
        info!("[paste-catch] clipboard changed externally; leaving it untouched ({reason})");
    } else if get_settings(app).clipboard_handling == ClipboardHandling::CopyToClipboard {
        // Upstream's contract under this setting is "the transcript stays".
        info!("[paste-catch] leaving the transcript on the clipboard ({reason})");
    } else {
        match held.saved {
            Snapshot::Text(text) => {
                let _ = clipboard.write_text(text);
            }
            Snapshot::Image(image) => {
                let _ = clipboard.write_image(&image);
            }
            // Nothing was there to begin with — upstream clears rather than
            // leaving the transcript behind, so this does too.
            Snapshot::Empty => {
                let _ = clipboard.clear();
            }
        }
        info!("[paste-catch] clipboard handed back ({reason})");
    }

    crate::bridge::emit(app, DaemonEvent::PasteMissedClear);
    unregister_deliver_deferred(app);
}

/// Whether the transcript we published is still the clipboard's content.
fn still_ours(app: &AppHandle, held: &Held) -> bool {
    #[cfg(windows)]
    {
        let _ = app;
        clipboard_sequence() == held.seq_at_publish
    }
    // No sequence number elsewhere: compare the content instead. Cheaper than
    // it looks — the alternative is restoring over a copy the user just made.
    #[cfg(not(windows))]
    {
        app.clipboard()
            .read_text()
            .map(|current| current == held.text)
            .unwrap_or(false)
    }
}

/// Clipboard generation counter. `0` where the platform has no equivalent;
/// [`still_ours`] does not consult it there.
fn clipboard_sequence() -> u32 {
    #[cfg(windows)]
    {
        unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
    }
    #[cfg(not(windows))]
    {
        0
    }
}

// -- Delivery ------------------------------------------------------------------

/// The deliver shortcut fired. Puts the held transcript where the caret is now.
///
/// Everything runs off the input thread: this is reached from a `ShortcutAction`
/// and it unregisters a shortcut, which must never happen synchronously there —
/// doing so hangs every global shortcut.
pub fn deliver(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<PasteCatchState>() else {
            return;
        };
        let held = state.held.lock().ok().and_then(|mut slot| slot.take());
        let Some(held) = held else {
            return;
        };
        // Invalidate the armed timer: this hold is ours now.
        state.gen.fetch_add(1, Ordering::SeqCst);

        // If the user copied over the transcript, put it back before pasting.
        if !still_ours(&app, &held) {
            if let Err(e) = app.clipboard().write_text(held.text.clone()) {
                warn!("[paste-catch] could not re-publish the transcript: {e}");
                crate::bridge::emit(&app, DaemonEvent::PasteMissedClear);
                unregister_deliver_deferred(&app);
                return;
            }
        }

        if let Err(e) = send_paste_chord(&app) {
            warn!("[paste-catch] delivery chord failed: {e}");
        }

        // Verify the delivery the same way the original paste was verified. If
        // it missed again the hold simply comes back — reusing the ORIGINAL
        // snapshot, because the clipboard currently holds the transcript.
        std::thread::sleep(Duration::from_millis(VERIFY_SETTLE_MS));
        let delivered = crate::context_detect::read_caret_now()
            .map(|caret| landed(&caret.before, &held.text))
            // No caret to read is no evidence; treat it as delivered rather
            // than trapping the user in a loop of re-offers.
            .unwrap_or(true);

        if delivered {
            info!("[paste-catch] delivered");
            hand_off(&app, held, "delivered");
        } else {
            info!("[paste-catch] delivery missed as well — re-arming the hold");
            let Held { text, saved, .. } = held;
            arm_with(&app, text, saved);
        }
    });
}

/// Send the configured paste chord.
///
/// Deliberately NOT `clipboard::paste`: that would re-run post-processing, the
/// trailing space, auto-submit and the Agent-panel guard over text which has
/// already been through all of them.
fn send_paste_chord(app: &AppHandle) -> Result<(), String> {
    let method = get_settings(app).paste_method;
    let state = app
        .try_state::<crate::input::EnigoState>()
        .ok_or("Enigo state not initialized")?;
    let mut enigo = state
        .0
        .lock()
        .map_err(|e| format!("Failed to lock Enigo: {e}"))?;
    match method {
        PasteMethod::CtrlShiftV => crate::input::send_paste_ctrl_shift_v(&mut enigo, CHORD_HOLD_MS),
        PasteMethod::ShiftInsert => crate::input::send_paste_shift_insert(&mut enigo, CHORD_HOLD_MS),
        // Direct typing and external scripts have no chord of their own, and a
        // manual delivery is a clipboard paste by construction.
        _ => crate::input::send_paste_ctrl_v(&mut enigo, CHORD_HOLD_MS),
    }
}

// -- Shortcut registration -----------------------------------------------------

fn deliver_binding(app: &AppHandle) -> ShortcutBinding {
    get_settings(app)
        .bindings
        .get(BINDING_ID)
        .cloned()
        .unwrap_or_else(|| {
            crate::settings::get_default_settings()
                .bindings
                .get(BINDING_ID)
                .cloned()
                .expect("paste_catch_deliver default binding exists")
        })
}

/// Hold the deliver key for the offer's lifetime.
///
/// A conflict with an existing global binding is logged and otherwise ignored:
/// the offer still works via plain `Ctrl+V`, so a busy accelerator degrades the
/// feature rather than breaking it.
fn register_deliver_deferred(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let binding = deliver_binding(&app);
        if binding.current_binding.trim().is_empty() {
            return;
        }
        let _ = crate::shortcut::unregister_shortcut(&app, binding.clone());
        if let Err(e) = crate::shortcut::register_shortcut(&app, binding.clone()) {
            warn!(
                "[paste-catch] could not register '{}': {e} — Ctrl+V still delivers",
                binding.current_binding
            );
        }
    });
}

/// Release the deliver key. "Destroy if not in use": outside an armed hold the
/// accelerator belongs to whatever else the user has bound it to.
fn unregister_deliver_deferred(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = crate::shortcut::unregister_shortcut(&app, deliver_binding(&app));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize("  a \n\t b  "), "a b");
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("   "), "");
    }

    #[test]
    fn tail_takes_characters_not_bytes() {
        // Multi-byte input must not be sliced mid-character.
        let text = "aaaaaaaaaaaaaaaaaaaaaaaaaaaa日本語のテキスト";
        let tail = tail_of(text, 5);
        assert_eq!(tail.chars().count(), 5);
        assert_eq!(tail, "のテキスト");
    }

    #[test]
    fn tail_of_short_text_is_the_whole_text() {
        assert_eq!(tail_of("hi there", 24), "hi there");
    }

    #[test]
    fn landed_accepts_text_at_the_caret() {
        let transcript = "the quick brown fox jumps over the lazy dog";
        let before = format!("Existing content. {transcript}");
        assert!(landed(&before, transcript));
    }

    #[test]
    fn landed_tolerates_reflowed_whitespace() {
        // A composer that collapsed the newline still received the paste.
        let transcript = "first line\nsecond line of the transcript";
        let before = "first line second line of the transcript";
        assert!(landed(before, transcript));
    }

    #[test]
    fn landed_tolerates_a_trailing_space() {
        let transcript = "a reasonably long transcript ";
        assert!(landed("a reasonably long transcript", transcript));
    }

    #[test]
    fn landed_rejects_an_untouched_field() {
        assert!(!landed(
            "whatever was already here",
            "the transcript that never arrived"
        ));
    }

    #[test]
    fn landed_rejects_an_empty_caret() {
        assert!(!landed("", "the transcript that never arrived"));
    }

    #[test]
    fn landed_on_a_paste_into_the_middle_still_matches() {
        // The caret sits immediately after the inserted text, so `before` ends
        // with the transcript even though more text follows it in the field.
        let transcript = "inserted sentence goes right here";
        let before = format!("Preamble. {transcript}");
        assert!(landed(&before, transcript));
    }

    #[test]
    fn empty_transcript_is_never_reported_as_a_miss() {
        assert!(landed("", ""));
    }

    #[test]
    fn verification_is_skipped_for_auto_submit() {
        // A successful send clears the field, which reads exactly like a miss.
        assert!(!should_verify(true, false, true, true, 100));
    }

    #[test]
    fn verification_is_skipped_under_reliable_paste() {
        // paste_tx still owns the clipboard as a delayed-render promise.
        assert!(!should_verify(true, true, false, true, 100));
    }

    #[test]
    fn verification_is_skipped_when_disabled_or_unverifiable() {
        assert!(!should_verify(false, false, false, true, 100));
        assert!(!should_verify(true, false, false, false, 100));
    }

    #[test]
    fn verification_is_skipped_for_very_short_transcripts() {
        assert!(!should_verify(true, false, false, true, MIN_VERIFY_CHARS - 1));
        assert!(should_verify(true, false, false, true, MIN_VERIFY_CHARS));
    }

    #[test]
    fn only_clipboard_and_direct_methods_are_verifiable() {
        assert!(method_verifiable(PasteMethod::CtrlV));
        assert!(method_verifiable(PasteMethod::CtrlShiftV));
        assert!(method_verifiable(PasteMethod::ShiftInsert));
        assert!(method_verifiable(PasteMethod::Direct));
        assert!(!method_verifiable(PasteMethod::None));
        assert!(!method_verifiable(PasteMethod::ExternalScript));
    }
}
