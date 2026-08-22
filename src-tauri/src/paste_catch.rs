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
//! - **Post-flight** ([`verify`], bottom of `clipboard::paste`): re-read the
//!   same element and compare. Did anything actually change?
//!
//! The governing rule is asymmetric: **act only on positive evidence of a
//! miss.** A false positive interrupts a user who pasted fine *and* holds their
//! clipboard for the grace period; a false negative merely reproduces today's
//! behaviour. So every ambiguous reading is silent, in *both* probes — see
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

use crate::context_detect::{classify, FocusTarget};
use crate::settings::{get_settings, ClipboardHandling, PasteMethod, ShortcutBinding};

/// Settings id of the deliver binding. Registered only while a hold is armed —
/// see `grain_core::capture::is_dynamic_binding`.
const BINDING_ID: &str = "paste_catch_deliver";

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
    /// The focused element as it was **immediately before** the paste, captured
    /// by [`intercept`] at no extra cost (it already reads focus for pre-flight).
    /// Differencing against the post-paste read is far stronger than matching
    /// the transcript's tail: it survives an application reflowing, autocorrecting
    /// or smart-quoting the text, because *any* change proves something arrived.
    before: Mutex<Option<crate::context_detect::FocusProbe>>,
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

/// The post-paste decision table.
///
/// # The governing rule: a miss is concluded from evidence about the PASTE
///
/// Never from the absence of evidence about the element. An earlier version of
/// this table inverted that — it treated "the focused element exposes no text
/// affordance" as proof that no text could have arrived — and the inversion is
/// unsound for three separate, common reasons:
///
/// - **GPU-drawn applications expose nothing.** A terminal like Warp paints its
///   own input and reports a bare container to UI Automation, while accepting
///   pasted text perfectly well.
/// - **Chromium builds its accessibility tree lazily.** The first UIA query into
///   a WebView2 or Electron app can answer with a patternless `Pane` and only
///   populate the real tree afterwards, which is what made the bug feel
///   intermittent rather than reproducible.
/// - **The evidence is read too late to be about the paste.** By the time this
///   runs, the user may have pressed Enter. The composer they pasted into has
///   sent its message and cleared, so an element that received the transcript
///   and an element that never saw it look *identical*.
///
/// So absence is inconclusive here, exactly as it is in [`classify`]. What
/// remains are three positive signals, in descending strength.
pub fn verdict_after_paste(
    before: Option<&ProbeView<'_>>,
    after: &ProbeView<'_>,
    transcript: &str,
) -> PasteOutcome {
    // A transcript dictated into a password box is never held on the clipboard.
    if after.facts.is_password {
        return PasteOutcome::Landed;
    }
    // Focus is one of Grain's own windows (the pill, a panel). Nothing about
    // someone else's paste can be concluded from our own window.
    if after.facts.is_own_process {
        return PasteOutcome::Unknown;
    }

    // 1. The transcript is demonstrably there. Strongest possible evidence.
    if let Some(observed) = after.readable() {
        if landed_in(observed, transcript) {
            return PasteOutcome::Landed;
        }
    }

    // Differencing is valid only for the SAME element the paste targeted. If
    // focus moved we are looking at something the paste never went to, and its
    // contents prove nothing either way.
    let same_element = before.filter(|b| b.identity == after.identity);

    // 2. The element still holds *exactly* the content it held before the
    //    paste. Non-empty on both sides is what makes this a statement about
    //    the paste: an empty field that is still empty is the signature of a
    //    composer that sent and cleared, not of a paste that never arrived.
    if let Some(before) = same_element {
        match (before.readable(), after.readable()) {
            (Some(was), Some(now)) if was == now && !was.trim().is_empty() => {
                return PasteOutcome::Missed;
            }
            // Content moved, but not into a tail we recognise. Something
            // arrived and the application reshaped it (autocorrect, smart
            // quotes, newline stripping). Landed is the safe reading.
            (Some(was), Some(now)) if was != now => return PasteOutcome::Landed,
            _ => {}
        }
    }

    // 3. This element is positively not editable — a read-only value, a leaf
    //    widget, a caret-less document. Skipped when focus has demonstrably
    //    MOVED since the paste, because "focus is on a button now" is what
    //    pressing Enter in a chat app looks like, not what a miss looks like.
    //    A missing before-image does not block it: that only means the
    //    pre-flight read failed, which is no reason to discard this one.
    let focus_moved = before.is_some() && same_element.is_none();
    if !focus_moved && classify(after.facts) == FocusTarget::NotEditable {
        return PasteOutcome::Missed;
    }

    // 4. Readable but without our text, or claims to accept text and told us
    //    nothing. Applications rewrite pasted text, and poor accessibility
    //    implementations report empty fields that are not empty.
    PasteOutcome::Unknown
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

/// The parts of a focus probe the verdict needs, borrowed so the decision table
/// stays a pure function over plain data.
pub struct ProbeView<'a> {
    pub facts: crate::context_detect::FocusFacts,
    pub identity: crate::context_detect::FocusIdentity,
    pub caret_before: Option<&'a str>,
    pub value: Option<&'a str>,
}

impl ProbeView<'_> {
    /// Whatever the element was willing to tell us, or `None` if it said
    /// nothing — which is not the same as saying "empty".
    fn readable(&self) -> Option<&str> {
        self.caret_before.or(self.value)
    }
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
    // ONE focus read serving two purposes: the pre-flight verdict, and the
    // before-image that post-flight differences against. Reading focus twice
    // would cost a second cross-process round trip on the paste path and risk
    // the two reads disagreeing.
    let probe = crate::context_detect::read_focus_probe();
    let target = probe
        .as_ref()
        .map(|p| classify(p.facts))
        .unwrap_or(FocusTarget::Unknown);
    if let Some(state) = app.try_state::<PasteCatchState>() {
        if let Ok(mut slot) = state.before.lock() {
            *slot = probe;
        }
    }

    if target != FocusTarget::NotEditable {
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
        // The preference can change while this short-lived verifier sleeps.
        // Re-read it before touching accessibility APIs or arming a hold so OFF
        // has no delayed work and cannot surface a stale clipboard notice.
        if !get_settings(&app).paste_catch_enabled {
            return;
        }
        if confirmed_miss(&app, &text) {
            info!("[paste-catch] the paste missed — holding the transcript");
            arm(&app, &text);
        }
    });
}

/// A miss, read twice.
///
/// One read is a race: a slow application can still be processing the paste at
/// [`VERIFY_SETTLE_MS`], and focus can be mid-flight while the user clicks
/// around. Requiring the same answer after a second, longer wait turns that race
/// into a confirmation — and costs nothing on the common path, because a paste
/// that landed answers `Landed` the first time and returns immediately.
fn confirmed_miss(app: &AppHandle, transcript: &str) -> bool {
    if observe(app, transcript) != PasteOutcome::Missed {
        return false;
    }
    std::thread::sleep(Duration::from_millis(VERIFY_CONFIRM_MS));
    let again = observe(app, transcript);
    if again != PasteOutcome::Missed {
        info!("[paste-catch] first read said missed, confirmation said {again:?} — standing down");
        return false;
    }
    true
}

/// Read the focused element and reach a verdict, differencing against the
/// before-image [`intercept`] captured. `Unknown` when focus cannot be resolved.
fn observe(app: &AppHandle, transcript: &str) -> PasteOutcome {
    let Some(after) = crate::context_detect::read_focus_probe() else {
        return PasteOutcome::Unknown;
    };
    // Borrowed, not taken: the confirmation read differences against the same
    // before-image as the first one.
    let before = app
        .try_state::<PasteCatchState>()
        .and_then(|state| state.before.lock().ok().and_then(|slot| slot.clone()));

    // Bound rather than chained: `before.as_ref().map(view_of).as_ref()` would
    // borrow a temporary that dies at the end of the statement.
    let before_view = before.as_ref().map(view_of);
    let after_view = view_of(&after);
    let outcome = verdict_after_paste(before_view.as_ref(), &after_view, transcript);

    // Every field report about this feature is "it held when it shouldn't have"
    // — or now "it didn't hold when it should have" — in some application
    // nobody can reproduce locally, so the reasons behind a non-`Landed`
    // verdict have to be recoverable from a log the user already has. Info
    // level while the decision table is still being tuned against real
    // applications; a paste that lands prints nothing.
    if outcome != PasteOutcome::Landed {
        let f = after_view.facts;
        log::info!(
            "[paste-catch] {outcome:?} — control={:?} edit_pattern={} text_pattern={} \
             read_only={:?} uia_caret={} native_caret={} desktop={} same_element={} \
             before={} after={}",
            f.control,
            f.has_text_edit_pattern,
            f.has_text_pattern,
            f.value_read_only,
            f.has_caret,
            f.has_native_caret,
            f.is_shell_desktop,
            before_view
                .as_ref()
                .is_some_and(|b| b.identity == after_view.identity),
            before_view.as_ref().and_then(|b| b.readable()).is_some(),
            after_view.readable().is_some(),
        );
    }
    outcome
}

fn view_of(probe: &crate::context_detect::FocusProbe) -> ProbeView<'_> {
    ProbeView {
        facts: probe.facts,
        identity: probe.identity,
        caret_before: probe.caret.as_ref().map(|c| c.before.as_str()),
        value: probe.value.as_deref(),
    }
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

        // Refresh the before-image so the delivery is verified against the
        // element it is actually going to, not the one the original paste
        // missed — otherwise the identity check disables differencing here.
        if let Ok(mut slot) = state.before.lock() {
            *slot = crate::context_detect::read_focus_probe();
        }

        if let Err(e) = send_paste_chord(&app) {
            warn!("[paste-catch] delivery chord failed: {e}");
        }

        // Verify the delivery the same way the original paste was verified. If
        // it missed again the hold simply comes back — reusing the ORIGINAL
        // snapshot, because the clipboard currently holds the transcript.
        std::thread::sleep(Duration::from_millis(VERIFY_SETTLE_MS));
        // Only a CONFIRMED miss re-arms. `Unknown` counts as delivered, so an
        // unreadable target cannot trap the user in a loop of re-offers.
        let delivered = !confirmed_miss(&app, &held.text);

        if delivered {
            info!("[paste-catch] delivered");
            hand_off(&app, held, "delivered");
        } else if get_settings(&app).paste_catch_enabled {
            info!("[paste-catch] delivery missed as well — re-arming the hold");
            let Held { text, saved, .. } = held;
            arm_with(&app, text, saved);
        } else {
            // The setting changed while delivery was being verified. Restore
            // the clipboard instead of resurrecting the disabled feature.
            hand_off(&app, held, "disabled");
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
        // The caret sits immediately after the inserted text, so `before` ends
        // with the transcript even though more text follows it in the field.
        let transcript = "inserted sentence goes right here";
        let before = format!("Preamble. {transcript}");
        assert!(landed_in(&before, transcript));
    }

    #[test]
    fn empty_transcript_is_never_reported_as_a_miss() {
        assert!(landed_in("", ""));
    }

    fn view<'a>(
        facts: crate::context_detect::FocusFacts,
        caret_before: Option<&'a str>,
        value: Option<&'a str>,
    ) -> ProbeView<'a> {
        ProbeView {
            facts,
            identity: crate::context_detect::FocusIdentity {
                process_id: 1234,
                control_type: 50004,
                native_window: 99,
            },
            caret_before,
            value,
        }
    }

    /// Same shape, but a different element than [`view`] produces.
    fn other_view<'a>(
        facts: crate::context_detect::FocusFacts,
        caret_before: Option<&'a str>,
    ) -> ProbeView<'a> {
        ProbeView {
            identity: crate::context_detect::FocusIdentity {
                process_id: 4321,
                control_type: 50030,
                native_window: 7,
            },
            ..view(facts, caret_before, None)
        }
    }

    /// Verdict with no before-image (the pre-paste read failed).
    fn verdict(after: &ProbeView<'_>, transcript: &str) -> PasteOutcome {
        verdict_after_paste(None, after, transcript)
    }

    /// A focused element that accepts text but told us nothing readable.
    fn editable_but_silent() -> crate::context_detect::FocusFacts {
        crate::context_detect::FocusFacts {
            has_text_edit_pattern: true,
            ..Default::default()
        }
    }

    /// An element that told us nothing at all: no patterns, no control type, no
    /// caret. A GPU-drawn editor and an inert pane are indistinguishable here,
    /// which is precisely why this shape may never be read as a miss.
    fn no_text_affordance() -> crate::context_detect::FocusFacts {
        crate::context_detect::FocusFacts::default()
    }

    #[test]
    fn seeing_the_transcript_is_decisive_without_any_before_image() {
        let t = "a reasonably long transcript to match";
        assert_eq!(
            verdict(
                &view(editable_but_silent(), Some(&format!("x. {t}")), None),
                t
            ),
            PasteOutcome::Landed
        );
    }

    #[test]
    fn value_is_used_when_there_is_no_caret() {
        // Single-line inputs expose ValuePattern but often no usable selection.
        let t = "search terms that were dictated";
        assert_eq!(
            verdict(&view(editable_but_silent(), None, Some(t)), t),
            PasteOutcome::Landed
        );
    }

    #[test]
    fn a_readable_field_without_our_text_needs_a_before_image() {
        // Deliberately NOT a miss on its own. An application that rewrote the
        // pasted text presents exactly this picture, and so does a field whose
        // accessibility implementation under-reports its contents. Only the
        // comparison against the pre-paste image settles it.
        let t = "a reasonably long transcript to match";
        let f = editable_but_silent();
        assert_eq!(
            verdict(&view(f, Some("untouched"), None), t),
            PasteOutcome::Unknown
        );
        assert_eq!(
            verdict(&view(f, None, Some("something else")), t),
            PasteOutcome::Unknown
        );

        // With a before-image showing the SAME content, it is conclusive.
        let before = view(f, Some("untouched"), None);
        let after = view(f, Some("untouched"), None);
        assert_eq!(
            verdict_after_paste(Some(&before), &after, t),
            PasteOutcome::Missed
        );
    }

    #[test]
    fn a_gpu_drawn_application_is_not_a_miss() {
        // THE REGRESSION TEST. Warp paints its own input and reports a bare
        // container with no patterns, no value and no system caret — while
        // accepting pasted text perfectly well. Reading that absence as proof
        // of a miss held a transcript that landed, on every single dictation.
        let f = no_text_affordance();
        let before = view(f, None, None);
        let after = view(f, None, None);
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "a transcript that did land"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn a_composer_that_sent_and_cleared_is_not_a_miss() {
        // The user pressed Enter before this read. The message went out and the
        // composer emptied, so the field reads exactly as it did before the
        // paste. Indistinguishable from a miss by content alone — and the
        // emptiness is what gives it away, because a focused editable field
        // that is still empty did not refuse the paste, it forwarded it.
        let f = editable_but_silent();
        let before = view(f, None, None);
        let after = view(f, None, None);
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "the message that was sent"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn an_empty_string_on_both_sides_is_not_a_miss() {
        // Same shape as above, for providers that answer with an empty string
        // rather than declining to answer.
        let f = editable_but_silent();
        let before = view(f, Some("   "), None);
        let after = view(f, Some("   "), None);
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "the message that was sent"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn a_tree_that_finished_building_can_still_report_a_miss() {
        // Chromium populates its accessibility tree lazily: the pre-flight read
        // saw a patternless container and correctly refused to guess, and this
        // one — of the same element — can see it is read-only. That is positive
        // evidence, arriving late, and it is the case post-flight exists for.
        let before = view(no_text_affordance(), None, None);
        let after = view(
            crate::context_detect::FocusFacts {
                value_read_only: Some(true),
                ..Default::default()
            },
            None,
            None,
        );
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "a transcript with nowhere to go"),
            PasteOutcome::Missed
        );
    }

    #[test]
    fn positive_evidence_about_a_different_element_proves_nothing() {
        // Focus moved to a button after the paste — which is what pressing
        // Enter in a chat app looks like. The button is positively not
        // editable, but it is not what the paste targeted.
        let before = view(editable_but_silent(), None, None);
        let after = other_view(
            crate::context_detect::FocusFacts {
                control: crate::context_detect::ControlClass::NonText,
                ..Default::default()
            },
            None,
        );
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "a transcript that did land"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn an_editable_but_unreadable_target_stays_unknown() {
        // It claims to accept text and told us nothing — a poor-UIA editor.
        // Holding here would interrupt a paste that probably worked.
        assert_eq!(
            verdict(&view(editable_but_silent(), None, None), "some transcript"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn a_text_pattern_alone_is_enough_to_stay_unknown() {
        let facts = crate::context_detect::FocusFacts {
            has_text_pattern: true,
            ..Default::default()
        };
        assert_eq!(
            verdict(&view(facts, None, None), "some transcript"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn a_thread_level_caret_does_not_rescue_an_inert_page_body() {
        // Chromium keeps a system caret around for IME positioning while the
        // focused element is a caret-less `Document` — a rendered page, the
        // single most common surface a dictation paste lands on by mistake.
        // Treating that thread-level signal as evidence about this element
        // silenced the catch in every browser and Electron app.
        let facts = crate::context_detect::FocusFacts {
            control: crate::context_detect::ControlClass::Document,
            has_native_caret: true,
            ..Default::default()
        };
        let before = view(facts, None, None);
        let after = view(facts, None, None);
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "a transcript with nowhere to go"),
            PasteOutcome::Missed
        );
    }

    #[test]
    fn a_failed_pre_flight_read_does_not_block_positive_evidence() {
        // `before` is None only because the pre-paste probe failed. That is no
        // reason to discard a post-paste read that positively says the element
        // cannot take text.
        let facts = crate::context_detect::FocusFacts {
            value_read_only: Some(true),
            ..Default::default()
        };
        assert_eq!(
            verdict(&view(facts, None, None), "a transcript with nowhere to go"),
            PasteOutcome::Missed
        );
    }

    #[test]
    fn a_caret_less_document_is_still_caught_after_the_fact() {
        // The counterpart: a PDF viewer or rendered reader, same element on
        // both reads, nothing readable. Positive evidence, so it holds.
        let facts = crate::context_detect::FocusFacts {
            control: crate::context_detect::ControlClass::Document,
            ..Default::default()
        };
        let before = view(facts, None, None);
        let after = view(facts, None, None);
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "a transcript with nowhere to go"),
            PasteOutcome::Missed
        );
    }

    #[test]
    fn identical_content_before_and_after_is_a_miss() {
        // The robust miss signal: the paste changed nothing at all. Needs no
        // tail match, so it holds even for text the application would rewrite.
        let f = editable_but_silent();
        let before = view(f, Some("unchanged content"), None);
        let after = view(f, Some("unchanged content"), None);
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "a transcript that never arrived"),
            PasteOutcome::Missed
        );
    }

    #[test]
    fn changed_content_counts_as_landed_even_if_the_app_rewrote_it() {
        // Smart quotes, autocorrect, newline stripping: the tail no longer
        // matches, but something demonstrably arrived. This is the false
        // positive that differencing exists to remove.
        let f = editable_but_silent();
        let before = view(f, Some("Dear Bob "), None);
        let after = view(
            f,
            Some("Dear Bob \u{201c}rewritten\u{201d} by the app"),
            None,
        );
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "\"rewritten\" by the app!!"),
            PasteOutcome::Landed
        );
    }

    #[test]
    fn differencing_is_skipped_when_focus_moved() {
        // Focus changed between the paste and the read, so the element in front
        // of us is not the one the paste targeted. Its content proves nothing.
        let f = editable_but_silent();
        let before = view(f, Some("same text"), None);
        let after = other_view(f, Some("same text"));
        assert_eq!(
            verdict_after_paste(Some(&before), &after, "a transcript"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn a_positive_tail_match_wins_over_everything() {
        // Even with a before-image showing identical content, seeing the
        // transcript is decisive.
        let f = editable_but_silent();
        let t = "the transcript that certainly arrived";
        let observed = format!("x{t}");
        let before = view(f, Some("x"), None);
        let after = view(f, Some(&observed), None);
        assert_eq!(
            verdict_after_paste(Some(&before), &after, t),
            PasteOutcome::Landed
        );
    }

    #[test]
    fn grains_own_window_is_never_judged() {
        // The pill or a panel took focus. Grain's surfaces expose no text
        // affordance, so without this guard every focus steal would read as a
        // missed paste and hold a transcript that landed perfectly well.
        let facts = crate::context_detect::FocusFacts {
            is_own_process: true,
            ..Default::default()
        };
        assert_eq!(
            verdict(&view(facts, None, None), "some transcript"),
            PasteOutcome::Unknown
        );
    }

    #[test]
    fn a_password_field_is_never_held() {
        let facts = crate::context_detect::FocusFacts {
            is_password: true,
            ..Default::default()
        };
        // No caret, no value, no affordance — every other rule would say
        // "missed". Privacy wins over the safety net.
        assert_eq!(
            verdict(&view(facts, None, None), "a spoken password"),
            PasteOutcome::Landed
        );
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
        assert!(!should_verify(
            true,
            false,
            false,
            true,
            MIN_VERIFY_CHARS - 1
        ));
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
