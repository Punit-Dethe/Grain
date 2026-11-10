//! [GRAIN] Paste Catch — the missed-field safety net.
//!
//! When a dictation paste lands somewhere that cannot receive text — the
//! desktop, a rendered web page, a PDF viewer, a button — the synthesized
//! `Ctrl+V` is a no-op and the transcript is **destroyed**, not merely
//! un-pasted: upstream's `paste_via_clipboard` restores the user's previous
//! clipboard afterwards, and clears the clipboard outright when there was
//! nothing to restore. The words the user just spoke survive only in history.
//!
//! On a confirmed miss the transcript stays in memory while a copy offer is
//! visible. The offer can copy, discard, or deliver it using the existing
//! shortcut. The timeout copies only if the user has not copied something else.
//!
//! # Detection
//!
//! Two probes that fail differently, neither trusted alone:
//!
//! - **Pre-flight** ([`intercept`], top of `clipboard::paste`): classify the
//!   focused element. Only `NotEditable` — positive evidence — takes ownership,
//!   and then the OS paste is never sent at all, which also stops a stray
//!   `Ctrl+V` from firing a real command in a game or a CAD app.
//! - **Post-flight** ([`verify`], bottom of `clipboard::paste`): re-read the
//!   same element and compare. Did anything actually change?
//!
//! Recognized terminal windows bypass both probes. Their accessibility tree
//! can describe the read-only output buffer while the terminal accepts paste.
//!
//! The governing rule is asymmetric: **act only on positive evidence of a
//! miss.** A false positive interrupts a user who pasted fine. Ambiguous readings
//! stay silent in both probes. See
//! [`verdict_after_paste`] for the three ways that rule was learned the hard way.
//!
//! # What this cannot prove
//!
//! Neither probe observes the paste itself; both infer it from the state of the
//! world a moment later. The only unforgeable signal on Windows is a delayed-
//! render clipboard promise — publish `CF_UNICODETEXT` as `NULL` and wait for
//! `WM_RENDERFORMAT`, which arrives if and only if something actually read the
//! transcript. `paste_tx` already does exactly that under `reliable_paste`, but
//! it is upstream code held byte-identical, and it owns the clipboard for the
//! whole transaction, so this module stands down entirely while it is on (see
//! [`should_verify`]). Giving Paste Catch its own receipt would mean taking over
//! the paste path from `clipboard::paste` rather than bracketing it, which costs
//! more upstream divergence than the remaining ambiguity is worth today.

use grain_core::DaemonEvent;
use log::{info, warn};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::context_detect::{classify, FocusProbe, FocusTarget};
use crate::settings::{get_settings, ClipboardHandling, PasteMethod, ShortcutBinding};

/// Settings id of the deliver binding. Registered only while a hold is armed —
/// see `grain_core::capture::is_dynamic_binding`.
const BINDING_ID: &str = "paste_catch_deliver";
const COPY_OFFER_DELAY: Duration = Duration::from_millis(400);
const COPY_COUNTDOWN: Duration = Duration::from_secs(3);

/// How long to wait after the paste before asking the field what it holds.
/// Applications process a paste asynchronously; this is measured from a point
/// Grain controls (the bottom of `paste()`), not inferred from upstream's delay
/// constants, so it stays correct if upstream re-times that body.
const VERIFY_SETTLE_MS: u64 = 150;

/// Extra wait before re-reading a suspected miss. Long enough that a loaded
/// application has finished handling the paste, short enough that the offer
/// still appears while the user is looking at the screen — and only ever paid
/// when the first read already said "missed".
const VERIFY_CONFIRM_MS: u64 = 350;

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

struct PendingCopy {
    id: u32,
    text: String,
    clipboard_sequence: u32,
}

#[derive(Default)]
pub struct PasteCatchState {
    pending: Mutex<Option<PendingCopy>>,
    gen: AtomicU32,
    deliver_registration: Mutex<()>,
    before: Mutex<Option<(u32, FocusProbe)>>,
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

/// What the post-paste read concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteOutcome {
    /// The transcript is there, or something says it is. Do nothing.
    Landed,
    /// Positive evidence it is not. Hold it.
    Missed,
    /// Genuinely cannot tell. Do nothing.
    Unknown,
}

/// Only a readable, unchanged value on the exact same control can be a miss.
fn verdict_after_paste(before: &FocusProbe, after: &FocusProbe, transcript: &str) -> PasteOutcome {
    if before.identity.is_none()
        || before.identity != after.identity
        || after.facts.is_password
        || after.facts.is_own_process
    {
        return PasteOutcome::Unknown;
    }
    match (&before.value, &after.value) {
        (Some(was), Some(now)) if was != now || landed_in(now, transcript) => PasteOutcome::Landed,
        (Some(_), Some(_)) => PasteOutcome::Missed,
        _ => PasteOutcome::Unknown,
    }
}

/// Whether `observed` shows the transcript's tail, by either mechanism.
fn landed_in(observed: &str, transcript: &str) -> bool {
    let tail = tail_of(transcript, TAIL_CHARS);
    if tail.is_empty() {
        return true;
    }
    let normalized = normalize(observed);
    // The caret sits just after inserted text, so `ends_with` is the precise
    // test; `contains` additionally covers a value read, where the caret
    // position is not reflected in the string at all.
    normalized.ends_with(&tail) || normalized.contains(&tail)
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
    has_text: bool,
) -> bool {
    enabled && !reliable_paste && !auto_submit && method_verifiable && has_text
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

/// Decide once for the entire paste, before either accessibility probe.
pub(crate) fn probe_generation(app: &AppHandle) -> Option<u32> {
    let id = supersede(app);
    if !crate::agent::foreground_is_terminal() {
        return Some(id);
    }
    log::debug!("[paste-catch] bypassing accessibility checks for terminal paste");
    None
}

/// Pre-flight. Called at the top of `clipboard::paste`.
///
/// Returns `true` when Grain has taken ownership of the transcript — focus is
/// provably not editable, so the caller must NOT paste.
pub fn intercept(app: &AppHandle, id: u32, text: &str, may_intercept: bool) -> bool {
    let settings = get_settings(app);
    if !settings.paste_catch_enabled || text.trim().is_empty() {
        return false;
    }
    // Clipboard-only and external-script modes own their own delivery.
    if !method_verifiable(settings.paste_method) {
        return false;
    }
    let sequence = clipboard_sequence();
    // ONE focus read serving two purposes: the pre-flight verdict, and the
    // before-image that post-flight differences against. Reading focus twice
    // would cost a second cross-process round trip on the paste path and risk
    // the two reads disagreeing.
    let probe = crate::context_detect::read_focus_probe();
    let target = probe
        .as_ref()
        .map(|p| classify(p.facts))
        .unwrap_or(FocusTarget::Unknown);
    if may_intercept && target == FocusTarget::NotEditable {
        info!("[paste-catch] focus is not editable — holding the transcript instead of pasting");
        arm(app, id, text.to_owned(), sequence);
        return true;
    }
    let state = app.state::<PasteCatchState>();
    *state.before.lock().unwrap() = probe
        .filter(|probe| probe.identity.is_some())
        .map(|probe| (id, probe));
    false
}

/// Snapshot the last Windows input tick. Equality, not ordering, is meaningful.
pub(crate) fn input_tick() -> Option<u32> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            ..Default::default()
        };
        unsafe { GetLastInputInfo(&mut info) }
            .as_bool()
            .then_some(info.dwTime)
    }
    #[cfg(not(windows))]
    None
}

/// Post-flight checks run on a worker. When verification is unavailable the
/// caller retains its usual clipboard handling.
pub fn verify(app: &AppHandle, id: u32, text: &str, copy: bool, input: Option<u32>) -> bool {
    let state = app.state::<PasteCatchState>();
    let mut slot = state.before.lock().unwrap();
    if slot.as_ref().is_none_or(|(owner, _)| *owner != id) {
        return false;
    }
    let (_, before) = slot.take().unwrap();
    drop(slot);

    let settings = get_settings(app);
    if !should_verify(
        settings.paste_catch_enabled,
        settings.reliable_paste,
        settings.auto_submit,
        method_verifiable(settings.paste_method),
        !text.trim().is_empty(),
    ) {
        return false;
    }
    let Some(input) = input else {
        return false;
    };
    let sequence = clipboard_sequence();
    let app = app.clone();
    let text = text.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(VERIFY_SETTLE_MS));
        let state = app.state::<PasteCatchState>();
        if state.gen.load(Ordering::Acquire) != id {
            return;
        }
        if get_settings(&app).paste_catch_enabled && confirmed_miss(&before, &text, input) {
            info!("[paste-catch] the same text field stayed unchanged after paste");
            arm(&app, id, text, sequence);
        } else if copy {
            let _pending = state.pending.lock().unwrap();
            if state.gen.load(Ordering::Acquire) == id && clipboard_sequence() == sequence {
                if let Err(error) = app.clipboard().write_text(text) {
                    warn!("[paste-catch] could not copy the transcript: {error}");
                }
            }
        }
    });
    true
}

/// Give slower editors another read. A new keyboard or mouse event can mean
/// the user submitted, cleared, or moved away from the field, so stay silent.
fn confirmed_miss(before: &FocusProbe, transcript: &str, input: u32) -> bool {
    let unchanged = || {
        unchanged_paste_sample(
            before,
            transcript,
            input,
            input_tick,
            crate::context_detect::read_focus_probe,
        )
    };
    if !unchanged() {
        return false;
    }
    std::thread::sleep(Duration::from_millis(VERIFY_CONFIRM_MS));
    unchanged()
}

fn unchanged_paste_sample(
    before: &FocusProbe,
    transcript: &str,
    input: u32,
    mut last_input: impl FnMut() -> Option<u32>,
    read_focus: impl FnOnce() -> Option<FocusProbe>,
) -> bool {
    last_input() == Some(input)
        && read_focus().is_some_and(|after| {
            verdict_after_paste(before, &after, transcript) == PasteOutcome::Missed
        })
        && last_input() == Some(input)
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

fn arm(app: &AppHandle, id: u32, text: String, sequence: u32) {
    let state = app.state::<PasteCatchState>();
    let mut pending = state.pending.lock().unwrap();
    let settings = get_settings(app);
    if state.gen.load(Ordering::Acquire) != id || !settings.paste_catch_enabled {
        return;
    }
    if settings.overlay_style == crate::settings::OverlayStyle::None {
        if settings.clipboard_handling == ClipboardHandling::CopyToClipboard
            && clipboard_sequence() == sequence
        {
            if let Err(error) = app.clipboard().write_text(text) {
                warn!("[paste-catch] could not copy the transcript: {error}");
            }
        }
        return;
    }
    let ttl = COPY_OFFER_DELAY + COPY_COUNTDOWN;
    let deadline = tokio::time::Instant::now() + ttl;
    let expires_at_ms = (std::time::SystemTime::now() + ttl)
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let chars = text.chars().count() as u32;
    *pending = Some(PendingCopy {
        id,
        text,
        clipboard_sequence: sequence,
    });
    drop(pending);
    reconcile_deliver_deferred(app);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep_until(deadline - COPY_COUNTDOWN).await;
        {
            let state = app.state::<PasteCatchState>();
            let pending = state.pending.lock().unwrap();
            if pending.as_ref().is_none_or(|offer| offer.id != id) {
                return;
            }
            crate::bridge::emit(
                &app,
                DaemonEvent::PasteMissed {
                    id,
                    expires_at_ms,
                    shortcut: deliver_binding(&app).current_binding,
                    chars,
                },
            );
        }
        tokio::time::sleep_until(deadline).await;
        if let Err(error) = resolve(&app, id, CopyDecision::Timeout) {
            warn!("[paste-catch] could not copy the transcript: {error}");
            crate::bridge::emit(&app, DaemonEvent::PasteError { error });
        }
    });
}

#[derive(Clone, Copy)]
enum CopyDecision {
    Copy,
    Discard,
    Timeout,
}

fn should_copy(decision: CopyDecision, original_sequence: u32, current_sequence: u32) -> bool {
    match decision {
        CopyDecision::Copy => true,
        CopyDecision::Discard => false,
        CopyDecision::Timeout => original_sequence == current_sequence,
    }
}

fn take_offer(slot: &mut Option<PendingCopy>, id: u32) -> Option<PendingCopy> {
    if slot.as_ref().is_some_and(|offer| offer.id == id) {
        slot.take()
    } else {
        None
    }
}

fn resolve(app: &AppHandle, id: u32, decision: CopyDecision) -> Result<(), String> {
    let state = app.state::<PasteCatchState>();
    let mut pending = state.pending.lock().unwrap();
    let Some(offer) = take_offer(&mut pending, id) else {
        return Ok(());
    };
    let result = if should_copy(decision, offer.clipboard_sequence, clipboard_sequence()) {
        app.clipboard()
            .write_text(&offer.text)
            .map_err(|error| error.to_string())
    } else {
        Ok(())
    };
    if result.is_err() && matches!(decision, CopyDecision::Copy) {
        *pending = Some(offer);
        return result;
    }
    crate::bridge::emit(app, DaemonEvent::PasteMissedClear { id });
    drop(pending);
    reconcile_deliver_deferred(app);
    result
}

#[tauri::command]
#[specta::specta]
pub fn resolve_clipboard_offer(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: u32,
    copy: bool,
) -> Result<(), String> {
    if window.label() != "recording_overlay" {
        return Err("The copy offer belongs to the recording overlay.".into());
    }
    resolve(
        &app,
        id,
        if copy {
            CopyDecision::Copy
        } else {
            CopyDecision::Discard
        },
    )
}

/// Cancel an older verifier and discard its pending copy before a new dictation.
pub fn supersede(app: &AppHandle) -> u32 {
    let Some(state) = app.try_state::<PasteCatchState>() else {
        return 0;
    };
    let mut pending = state.pending.lock().unwrap();
    let id = state.gen.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
    state.before.lock().unwrap().take();
    if let Some(offer) = pending.take() {
        crate::bridge::emit(app, DaemonEvent::PasteMissedClear { id: offer.id });
        drop(pending);
        reconcile_deliver_deferred(app);
    }
    id
}

fn restore(app: &AppHandle, saved: Snapshot) {
    let clipboard = app.clipboard();
    match saved {
        Snapshot::Text(text) => {
            let _ = clipboard.write_text(text);
        }
        Snapshot::Image(image) => {
            let _ = clipboard.write_image(&image);
        }
        Snapshot::Empty => {
            let _ = clipboard.clear();
        }
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

/// Deliver a pending transcript through the existing shortcut.
pub fn deliver(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<PasteCatchState>();
        let (offer, saved, sequence, id) = {
            let mut pending = state.pending.lock().unwrap();
            let Some(offer) = pending.take() else {
                return;
            };
            let id = state.gen.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
            crate::bridge::emit(&app, DaemonEvent::PasteMissedClear { id: offer.id });
            let saved = snapshot(&app);
            let written = app.clipboard().write_text(&offer.text);
            let sequence = clipboard_sequence();
            drop(pending);
            reconcile_deliver_deferred(&app);
            if let Err(error) = written {
                warn!("[paste-catch] could not deliver the transcript: {error}");
                arm(&app, id, offer.text, sequence);
                return;
            }
            (offer, saved, sequence, id)
        };

        let before = (!crate::agent::foreground_is_terminal())
            .then(crate::context_detect::read_focus_probe)
            .flatten();
        let chord_sent = state.gen.load(Ordering::Acquire) == id
            && send_paste_chord(&app)
                .inspect_err(|e| warn!("[paste-catch] delivery chord failed: {e}"))
                .is_ok();
        let input = input_tick();
        std::thread::sleep(Duration::from_millis(VERIFY_SETTLE_MS));
        let missed = before
            .as_ref()
            .zip(input)
            .is_some_and(|(before, input)| confirmed_miss(before, &offer.text, input));
        let delivered = chord_sent && !missed;
        let mut rearm_sequence = sequence;
        {
            let _pending = state.pending.lock().unwrap();
            if clipboard_sequence() == sequence
                && (!delivered
                    || get_settings(&app).clipboard_handling != ClipboardHandling::CopyToClipboard)
            {
                restore(&app, saved);
                rearm_sequence = clipboard_sequence();
            }
        }
        if !delivered {
            arm(&app, id, offer.text, rearm_sequence);
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
        PasteMethod::ShiftInsert => {
            crate::input::send_paste_shift_insert(&mut enigo, CHORD_HOLD_MS)
        }
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

/// Reconcile the shortcut off the input thread. Always consult the latest offer
/// so a queued registration cannot outlive a newer discard or replacement.
fn reconcile_deliver_deferred(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<PasteCatchState>();
        let _registration = state.deliver_registration.lock().unwrap();
        let active = state.pending.lock().unwrap().is_some();
        let binding = deliver_binding(&app);
        if binding.current_binding.trim().is_empty() {
            return;
        }
        let _ = crate::shortcut::unregister_shortcut(&app, binding.clone());
        if active {
            if let Err(error) = crate::shortcut::register_shortcut(&app, binding) {
                warn!("[paste-catch] could not register the delivery shortcut: {error}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_explicit_copy_can_replace_newer_clipboard_contents() {
        assert!(should_copy(CopyDecision::Timeout, 10, 10));
        assert!(!should_copy(CopyDecision::Timeout, 10, 11));
        assert!(should_copy(CopyDecision::Copy, 10, 11));
        assert!(!should_copy(CopyDecision::Discard, 10, 10));
    }

    #[test]
    fn a_stale_or_duplicate_resolution_cannot_consume_an_offer() {
        let mut pending = Some(PendingCopy {
            id: 2,
            text: "Transcript".into(),
            clipboard_sequence: 10,
        });
        assert!(take_offer(&mut pending, 1).is_none());
        assert!(take_offer(&mut pending, 2).is_some());
        assert!(take_offer(&mut pending, 2).is_none());
    }

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
    fn landed_accepts_text_after_existing_content() {
        let transcript = "the quick brown fox jumps over the lazy dog";
        let before = format!("Existing content. {transcript}");
        assert!(landed_in(&before, transcript));
    }

    #[test]
    fn landed_tolerates_reflowed_whitespace() {
        // A composer that collapsed the newline still received the paste.
        let transcript = "first line\nsecond line of the transcript";
        let before = "first line second line of the transcript";
        assert!(landed_in(before, transcript));
    }

    #[test]
    fn landed_tolerates_a_trailing_space() {
        let transcript = "a reasonably long transcript ";
        assert!(landed_in("a reasonably long transcript", transcript));
    }

    #[test]
    fn landed_rejects_an_untouched_field() {
        assert!(!landed_in(
            "whatever was already here",
            "the transcript that never arrived"
        ));
    }

    #[test]
    fn landed_rejects_an_empty_caret() {
        assert!(!landed_in("", "the transcript that never arrived"));
    }

    #[test]
    fn landed_on_a_paste_into_the_middle_still_matches() {
        let transcript = "inserted sentence goes right here";
        let field = format!("Preamble. {transcript} and more");
        assert!(landed_in(&field, transcript));
    }

    #[test]
    fn empty_transcript_is_never_reported_as_a_miss() {
        assert!(landed_in("", ""));
    }

    fn field(value: Option<&str>) -> FocusProbe {
        FocusProbe {
            identity: Some(crate::context_detect::FocusIdentity {
                foreground_window: 99,
                runtime_id: vec![42, 7],
            }),
            value: value.map(str::to_owned),
            ..Default::default()
        }
    }

    #[test]
    fn a_readable_empty_field_can_report_a_miss() {
        let empty = field(Some(""));
        assert_eq!(
            verdict_after_paste(&empty, &empty, "Hi"),
            PasteOutcome::Missed
        );
        assert_eq!(
            verdict_after_paste(&empty, &field(None), "Hi"),
            PasteOutcome::Unknown
        );
        assert_eq!(
            verdict_after_paste(&field(None), &empty, "Hi"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn input_while_the_field_is_being_read_cancels_the_miss() {
        let before = field(Some(""));
        let mut ticks = [10, 11].into_iter();
        assert!(!unchanged_paste_sample(
            &before,
            "Hi",
            10,
            || ticks.next(),
            || Some(field(Some(""))),
        ));
        assert!(unchanged_paste_sample(
            &before,
            "Hi",
            10,
            || Some(10),
            || Some(field(Some(""))),
        ));
    }

    #[test]
    fn unchanged_text_can_report_a_miss_but_a_duplicate_transcript_cannot() {
        let before = field(Some("Existing text"));
        assert_eq!(
            verdict_after_paste(&before, &before, "new text"),
            PasteOutcome::Missed
        );
        assert_eq!(
            verdict_after_paste(&before, &before, "Existing text"),
            PasteOutcome::Landed
        );
    }

    #[test]
    fn any_field_change_rules_out_a_miss_even_if_the_editor_rewrote_the_paste() {
        let empty = field(Some(""));
        let filled = field(Some("Reformatted text"));
        assert_eq!(
            verdict_after_paste(&empty, &filled, "some transcript"),
            PasteOutcome::Landed
        );
        assert_eq!(
            verdict_after_paste(&filled, &empty, "some transcript"),
            PasteOutcome::Landed
        );
    }

    #[test]
    fn a_different_control_in_the_same_window_is_inconclusive() {
        let before = field(Some(""));
        let mut after = field(Some(""));
        after.identity.as_mut().unwrap().runtime_id = vec![42, 8];
        assert_eq!(
            verdict_after_paste(&before, &after, "Hi"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn unknown_focus_and_protected_fields_never_report_a_miss() {
        let mut field = field(Some(""));
        field.facts.is_password = true;
        assert_eq!(
            verdict_after_paste(&field, &field, "password"),
            PasteOutcome::Unknown
        );
        field.facts.is_password = false;
        field.facts.is_own_process = true;
        assert_eq!(
            verdict_after_paste(&field, &field, "Hi"),
            PasteOutcome::Unknown
        );
        field.facts.is_own_process = false;
        field.identity = None;
        assert_eq!(
            verdict_after_paste(&field, &field, "Hi"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn verification_is_skipped_for_auto_submit() {
        // A successful send clears the field, which reads exactly like a miss.
        assert!(!should_verify(true, false, true, true, true));
    }

    #[test]
    fn verification_is_skipped_under_reliable_paste() {
        // paste_tx still owns the clipboard as a delayed-render promise.
        assert!(!should_verify(true, true, false, true, true));
    }

    #[test]
    fn verification_is_skipped_when_disabled_or_unverifiable() {
        assert!(!should_verify(false, false, false, true, true));
        assert!(!should_verify(true, false, false, false, true));
    }

    #[test]
    fn verification_checks_short_transcripts_but_skips_empty_ones() {
        assert!(should_verify(true, false, false, true, true));
        assert!(!should_verify(true, false, false, true, false));
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
