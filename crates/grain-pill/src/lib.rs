#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Grain pill — the always-on dot-matrix "Aura Core" surface.
//!
//! winit + tiny-skia. A 25×8 dot grid presented as a true per-pixel-transparent
//! floating capsule (Win32 layered window). Mic levels are captured directly
//! (cpal) for lowest latency.
//!
//! Visual language:
//! - RECORDING: lit-dot *density* tracks mic amplitude (random placement,
//!   grey/near-white tiers, flicker) across the WHOLE grid.
//! - PROCESSING: orange "static" sparkle across the whole grid.
//! - IDLE: a calm soft sweep/breathing shimmer (fills the grid, no gap).
//! - HOVER: the right (non-voice-reactive) section smoothly turns off and
//!   expands leftward to reveal lit-pixel glyphs — ✓ + ✗ while recording, ✓
//!   while processing. Click ✓ to confirm, ✗ to cancel.
//!
//! Keys (standalone preview): R recording · P processing · I idle · B prompt-record
//! (blue tint, press after R) · A agent-input card · Esc quit.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use grain_core::settings::OverlayPosition;
use grain_core::{AgentInputKind, DaemonEvent, PillAction, SessionMode};

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Transform};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId, WindowLevel};

/// Sent from the WS thread to wake the winit event loop immediately when a
/// session-relevant event arrives (instead of waiting the full HIDDEN_TICK).
#[derive(Debug)]
enum UserEvent {
    Wake,
}

// ── Grid geometry ───────────────────────────────────────────────────────────
const COLS: usize = 25;
const ROWS: usize = 8;
const DOT_D: f32 = 3.0;
const CELL: f32 = 5.0;
const SCALE: f32 = 1.0; // px per grid unit — small pill (native QML size)

// The 4×4 confirm/recording zone (cols 18–21, rows 2–5) minus its 4 corners.
const BTN_COL: usize = 18;
const BTN_ROW: usize = 2;
const BTN_SPAN: usize = 4;

// [GRAIN] Prompt switcher — a SECOND capsule carrying the active post-processing
// prompt during a mid-speech switch (cycled by the same global shortcut). It is
// rendered in the SAME OS window as the pill (a single tiny-skia pixmap → NO
// extra window/webview/surface, zero incremental RAM):
//   · Collapsed: a sibling capsule that slides in to the RIGHT of the pill.
//   · Studio:    a full-width capsule that slides in ABOVE the transcript card.
// The unused canvas the two reserve is fully transparent → click-through on the
// layered window, so it costs nothing at rest.
const RISER_RESERVE: f32 = 5.0; // grid-cells kept transparent ABOVE the collapsed pill
const RISER_HOLD: Duration = Duration::from_millis(1600);
/// Shared label/arrow type size for both prompt capsules.
const PROMPT_LABEL_PX: f32 = 12.5;

// Collapsed sibling capsule (to the right of the pill). Fixed width — the label
// truncates with an ellipsis rather than resizing the capsule.
const SIB_GAP: f32 = 12.0; // px between the pill's right edge and the sibling
const SIB_W: f32 = 152.0; // prompt-switcher capsule width (fixed, arrows on both ends)
const SIB_MAX_W: f32 = 250.0; // widest the sibling ever gets (the agent follow-up offer)
const SIB_SLIDE: f32 = 16.0; // px the sibling travels in from the right
const SIB_ARROW_INSET: f32 = 17.0; // ‹ › inset from each capsule end
const SIB_TEXT_PAD: f32 = 6.0; // gap kept between the arrows and the label

// Studio top capsule (above the transcript card, spanning the full card width).
const STUDIO_TOP_PILL_H: f32 = 30.0;
const STUDIO_TOP_GAP: f32 = 9.0; // gap between the top capsule and the card
const STUDIO_TOP_RESERVE: f32 = STUDIO_TOP_PILL_H + STUDIO_TOP_GAP + 6.0; // canvas above the card
const STUDIO_TOP_ARROW_INSET: f32 = 22.0; // ‹ › inset from each capsule end
// Present (and ease the riser/hover) at ~60 fps so motion is smooth; the dot
// field itself only re-rolls every ROLL_INTERVAL so it keeps its calm cadence
// instead of turning into 60 fps static.
const TICK: Duration = Duration::from_millis(16);
const ROLL_INTERVAL: Duration = Duration::from_millis(80);
/// [GRAIN] After the pill has been continuously hidden this long, its parsed
/// font and frame buffer are dropped so the always-on process idles at its
/// floor (winit + a mic-less loop). Both reload lazily on the next show — the
/// first shown frame re-parses the font (~a few ms), invisible to the user.
const IDLE_FREE_AFTER: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, PartialEq, Eq)]
enum PillState {
    Idle,
    Recording,
    Processing,
    /// [GRAIN] B4: something went wrong (model load / paste). Placeholder visual
    /// — a dim static grid; dismissed by click or the next session's events.
    Fallback,
}

/// [GRAIN] Native ASR Studio Window: which surface the single OS window is
/// currently presenting. `Collapsed` is the classic small capsule (Batch/
/// Rolling); `Studio` is the larger streaming-text window, driven by
/// `SessionMode::NativeAsr`; `AgentInput` is the native Agent summon card
/// (recording → type-to-expand). The window is resized + repositioned on the
/// rare transitions between the surfaces (never per-frame).
#[derive(Clone, Copy, PartialEq, Eq)]
enum PillMode {
    Collapsed,
    Studio,
    AgentInput,
}

// ── Agent input geometry (the native summon card, per the reference design) ──
//
// The OS window is a fixed canvas sized for the EXPANDED card; the card itself
// is drawn bottom-anchored and animates its width/height INSIDE the canvas, so
// expansion never resizes the OS window (a layered window's fully transparent
// pixels are click-through, so the unused canvas area doesn't eat clicks).
const AIN_WIN_W: u32 = 580;
const AIN_WIN_H: u32 = 170;
/// Margin between the card and the work-area edge it anchors to.
const AIN_EDGE_MARGIN: i32 = 40;
/// Expanded card: 520px content + 2×10px horizontal padding (the reference).
const AIN_EXPANDED_W: f32 = 540.0;
const AIN_EXPANDED_H: f32 = 136.0;
/// Compact card paddings (12px vertical, 10px horizontal in the reference).
const AIN_PAD_X: f32 = 10.0;
const AIN_PAD_Y_COMPACT: f32 = 12.0;
const AIN_PAD_Y_EXPANDED: f32 = 16.0;
const AIN_RADIUS: f32 = 16.0;
/// Wave grid: 12×4 dots of 3.5px with 3px gaps.
const AIN_WAVE_COLS: usize = 12;
const AIN_WAVE_ROWS: usize = 4;
const AIN_WAVE_DOT: f32 = 3.5;
const AIN_WAVE_GAP: f32 = 3.0;

// ── Studio Window geometry ──────────────────────────────────────────────────
//
// [GRAIN] Modeled on Handy's live-transcription overlay (upstream
// `src/overlay/RecordingOverlay.css`): the live transcript flows in the TOP
// region (bottom-anchored, newest line lowest) and dissolves into the card's
// dark surface at the top edge, while a single control row sits pinned at the
// BOTTOM — recording dot (left) · reactive waveform (center) · elapsed timer +
// cancel glyph (right). Sizes are one modular scale off STUDIO_PAD + the line
// rhythm so nothing looks "off".
const STUDIO_W: f32 = 420.0; // a touch narrower than Handy's 452 — tighter caption box
const STUDIO_PAD: f32 = 18.0; // horizontal inset for text + control row
const STUDIO_CORNER_R: f32 = 16.0;
// [GRAIN] While the card is still GROWING (1–3 lines) it carries a small
// breathing gap above the first line. At the 4-line cap the gap closes to 0 so
// the text reaches the very top edge and the top dissolve takes over.
const STUDIO_GROW_TOP_GAP: f32 = 9.0;
// Transcript type scale — a comfortable italic caption body (Handy: 15px/1.35).
const STUDIO_TEXT_PX: f32 = 15.5;
const STUDIO_LINE_HEIGHT: f32 = 21.0;
// Live text caps at 4 lines; older lines scroll up and dissolve at the top edge.
const STUDIO_MAX_LINES: usize = 4;
// Height of the top dissolve band (only active once the card hits the 4-line
// cap; while growing the text is crisp to the top gap, no fade).
const STUDIO_FADE_PX: f32 = 22.0;
// Bottom control row: recording dot (left) · dot-matrix "waveform" (center) ·
// cancel X (right). Its height matches the small pill's dot field (ROWS·CELL) so
// the collapsed capsule can grow into it without the matrix changing size.
const STUDIO_CTRL_H: f32 = 26.0; // holds the 22px cancel disc + the trimmed matrix
                                 // [GRAIN] A breathing gap BELOW the control row so the dot-matrix never touches
                                 // the capsule's bottom edge (per the unified-pill spec — the pill "grows a little
                                 // bit down" past the matrix).
const STUDIO_BOTTOM_PAD: f32 = 5.0;
// [GRAIN] The card GROWS with the transcript: 1 line → 4 lines, then it caps and
// scrolls. The OS window is sized to the TALLEST the card ever gets (the capped
// 4-line height) and shorter cards are drawn bottom-anchored inside it, the
// space above left transparent — so growth is pure compositing (no per-frame
// window resize). At the cap the top gap is 0, so max height omits it.
const STUDIO_MAX_CARD_H: f32 =
    STUDIO_LINE_HEIGHT * STUDIO_MAX_LINES as f32 + STUDIO_CTRL_H + STUDIO_BOTTOM_PAD;

// [GRAIN] Unified pill: the streaming ("Studio") surface EXPANDS from the small
// pill. The dot-matrix aura (the small pill's entire body) becomes the bottom-
// center "waveform"; the black capsule eases outward from the collapsed width to
// the full card width so the recording dot (left) and cancel X (right) ride the
// animated edges, and the transcript + controls fade in with `expand`.
const STUDIO_MIN_W: f32 = COLS as f32 * CELL; // collapsed pill width (the dot field)
const STUDIO_EXPAND_EASE: f32 = 0.14; // per-frame ease of the grow — gentle/smooth
                                      // [GRAIN] The transcript is not painted until the pill has (all but) finished
                                      // expanding — the first word appears only once the capsule is fully open, then
                                      // fades in via the normal per-word reveal.
const STUDIO_TEXT_GATE: f32 = 0.98;

// [GRAIN] The REDUCED dot-matrix shown INSIDE the expanded pill: TWO centered
// ROWS (3–4) spanning the FULL pill width (cols 0–24, edge-silhouette respected
// by the roll functions). Volume is visualized by lighting columns from the
// horizontal center outward — silence = nothing lit, loud = all columns lit.
// Unlit pixels are fully black (NONE).
const STUDIO_MTX_R0: usize = 3;
const STUDIO_MTX_R1: usize = 4;
const STUDIO_MTX_C0: usize = 0;
const STUDIO_MTX_C1: usize = COLS - 1; // 24

// [GRAIN] Per-frame easing for the card's grow toward its target height. Lower =
// slower / smoother. 0.07 ≈ a soft ~500ms settle at 60fps — deliberately gentle
// so a new line rises in without the quick "snap" that read as a glitch.
const STUDIO_GROW_EASE: f32 = 0.07;
// New transcript words ramp their alpha in over this long instead of popping.
// Slower = smoother; long enough that fast streaming words dissolve in rather
// than flicking on.
const STUDIO_WORD_REVEAL: Duration = Duration::from_millis(260);

// [GRAIN] Grain's brand accent (the pill's orange), reused for the live overlay's
// dot / waveform / timer so the Studio Window matches the collapsed capsule.
const ACCENT: [u8; 3] = [255, 93, 30];

/// A rounded-rect path (quadratic corners — plenty smooth at this size; tiny-skia
/// has no built-in rounded-rect constructor). Used for the Studio Window background.
fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Whether a word in the Studio Window's live transcript is settled (solid) or
/// still volatile (rendered with a blur whose intensity reflects how unsettled
/// the stabilizer thinks it is).
#[derive(Clone, Copy, PartialEq)]
enum RunStyle {
    Committed,
    Partial { stable: bool },
}

/// [GRAIN] Accumulated Native ASR transcript for the Studio Window, rebuilt
/// from `Asr*` `DaemonEvent`s. `committed`/`partial` track the CURRENTLY OPEN
/// segment (each event replaces the prior value — they carry the full prefix/
/// tail, not a delta); `finished` holds segments already closed by
/// `AsrSegmentFinal`. Cleared at the start of every new Native ASR session.
#[derive(Clone, Default)]
struct AsrDisplay {
    finished: Vec<String>,
    committed: String,
    partial: String,
    partial_stable: bool,
}

impl AsrDisplay {
    /// Append a commit *delta* to the current segment's committed prefix.
    /// `AsrCommit` events carry only the newly-committed words (see the
    /// stabilizer), so they must be appended — never assigned — or committed
    /// text collapses to just the last delta.
    fn append_commit(&mut self, delta: &str) {
        let delta = delta.trim();
        if delta.is_empty() {
            return;
        }
        if self.committed.is_empty() {
            self.committed.push_str(delta);
        } else {
            self.committed.push(' ');
            self.committed.push_str(delta);
        }
    }

    fn runs(&self) -> Vec<(String, RunStyle)> {
        let mut runs = Vec::new();
        for seg in &self.finished {
            runs.extend(
                seg.split_whitespace()
                    .map(|w| (w.to_string(), RunStyle::Committed)),
            );
        }
        runs.extend(
            self.committed
                .split_whitespace()
                .map(|w| (w.to_string(), RunStyle::Committed)),
        );
        runs.extend(self.partial.split_whitespace().map(|w| {
            (
                w.to_string(),
                RunStyle::Partial {
                    stable: self.partial_stable,
                },
            )
        }));
        runs
    }

    /// [GRAIN] What the Studio Window actually draws: committed words PLUS the
    /// volatile tentative tail, exactly like Handy's live overlay. The tail is
    /// essential — transcribe-cpp's auto-commit can go long stretches without
    /// committing (typically pausing right after a sentence boundary), and a
    /// committed-only view visibly freezes during those stretches even though
    /// decoding is healthy. The renderer styles the tail dimmer (but crisp, no
    /// blur) so the stable/volatile distinction stays readable.
    fn display_runs(&self) -> Vec<(String, RunStyle)> {
        self.runs()
    }

    /// A cheap concatenation of all visible transcript text — used only to test
    /// whether the caption needs the fallback face (non-Latin dictation).
    fn probe_text(&self) -> String {
        let mut s = self.committed.clone();
        s.push_str(&self.partial);
        for f in &self.finished {
            s.push_str(f);
        }
        s
    }
}

struct LaidLine {
    words: Vec<(String, RunStyle)>,
}

/// Greedy word-wrap of styled runs into lines no wider than `max_w` at `px`.
fn wrap_runs(
    font: &fontdue::Font,
    runs: &[(String, RunStyle)],
    px: f32,
    max_w: f32,
) -> Vec<LaidLine> {
    let space_w = font.metrics(' ', px).advance_width;
    let mut lines: Vec<LaidLine> = Vec::new();
    let mut cur: Vec<(String, RunStyle)> = Vec::new();
    let mut cur_w = 0.0f32;
    for (word, style) in runs {
        let word_w: f32 = word
            .chars()
            .map(|c| font.metrics(c, px).advance_width)
            .sum();
        let added = if cur.is_empty() {
            word_w
        } else {
            word_w + space_w
        };
        if !cur.is_empty() && cur_w + added > max_w {
            lines.push(LaidLine {
                words: std::mem::take(&mut cur),
            });
            cur_w = 0.0;
        }
        cur_w += if cur.is_empty() {
            word_w
        } else {
            word_w + space_w
        };
        cur.push((word.clone(), *style));
    }
    if !cur.is_empty() {
        lines.push(LaidLine { words: cur });
    }
    lines
}

/// Separable box blur applied in place to a single-channel coverage bitmap (a
/// fontdue glyph raster). Used to render volatile partial-transcript words as
/// visibly "not settled yet" — `radius` 0 is a no-op.
fn box_blur(bmp: &mut [u8], w: usize, h: usize, radius: usize) {
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    let mut tmp = vec![0u8; w * h];
    // Horizontal pass.
    for y in 0..h {
        let row = &bmp[y * w..y * w + w];
        for x in 0..w {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius).min(w - 1);
            let sum: u32 = row[lo..=hi].iter().map(|&v| v as u32).sum();
            tmp[y * w + x] = (sum / (hi - lo + 1) as u32) as u8;
        }
    }
    // Vertical pass.
    for x in 0..w {
        for y in 0..h {
            let lo = y.saturating_sub(radius);
            let hi = (y + radius).min(h - 1);
            let sum: u32 = (lo..=hi).map(|yy| tmp[yy * w + x] as u32).sum();
            bmp[y * w + x] = (sum / (hi - lo + 1) as u32) as u8;
        }
    }
}

/// Curved silhouette: cols 0/24 hidden; 1/23 keep rows 2–5; 2/22 keep rows 1–6.
fn is_edge(c: usize, r: usize) -> bool {
    if c == 0 || c == COLS - 1 {
        return true;
    }
    if c == 1 || c == COLS - 2 {
        return !(2..=5).contains(&r);
    }
    if c == 2 || c == COLS - 3 {
        return !(1..=6).contains(&r);
    }
    false
}

/// The orange button/recording zone (excluded from the voice-density field;
/// it runs the constant radial ripple instead).
fn is_button(c: usize, r: usize) -> bool {
    if !((BTN_COL..BTN_COL + BTN_SPAN).contains(&c) && (BTN_ROW..BTN_ROW + BTN_SPAN).contains(&r)) {
        return false;
    }
    let (lc, lr) = (c - BTN_COL, r - BTN_ROW);
    !((lc == 0 || lc == 3) && (lr == 0 || lr == 3))
}

/// [GRAIN] Bottom edge (physical px) of the WORK AREA of the monitor containing
/// the given point — i.e. excluding the taskbar. `None` if it can't be resolved.
#[cfg(windows)]
fn work_area_bottom(center_x: i32, center_y: i32) -> Option<i32> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    unsafe {
        let hmon = MonitorFromPoint(
            POINT {
                x: center_x,
                y: center_y,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            Some(mi.rcWork.bottom)
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
fn work_area_bottom(_center_x: i32, _center_y: i32) -> Option<i32> {
    None
}

/// [GRAIN] Top edge (physical px) of the monitor's WORK AREA — for the agent
/// input's optional top anchor (clears a top-docked taskbar).
#[cfg(windows)]
fn work_area_top(center_x: i32, center_y: i32) -> Option<i32> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    unsafe {
        let hmon = MonitorFromPoint(
            POINT {
                x: center_x,
                y: center_y,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            Some(mi.rcWork.top)
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
fn work_area_top(_center_x: i32, _center_y: i32) -> Option<i32> {
    None
}

struct Rng(u64);
impl Rng {
    fn f32(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }
}

type Rgba = [u8; 4];
const NONE: Rgba = [0, 0, 0, 0];

struct Aura {
    energy: f32,
    phase: f32,
    btn_angle: f32,
    eligible: Vec<usize>, // non-edge, non-button cells (the voice-density field)
    dots: Vec<Rgba>,
    rng: Rng,
    /// [GRAIN] Prompt Record: when true, the RECORDING dot field is tinted a
    /// grey/light-blue mix (instead of grey/white) to signal the user is now
    /// dictating an AI instruction. The density-tracks-volume behavior is
    /// unchanged — only the palette shifts.
    prompt_recording: bool,
}

impl Aura {
    fn new() -> Self {
        let mut eligible = Vec::new();
        for r in 0..ROWS {
            for c in 0..COLS {
                if !is_edge(c, r) && !is_button(c, r) {
                    eligible.push(r * COLS + c);
                }
            }
        }
        Aura {
            energy: 0.0,
            phase: 0.0,
            btn_angle: 0.0,
            eligible,
            dots: vec![NONE; ROWS * COLS],
            rng: Rng(0x9E3779B97F4A7C15),
            prompt_recording: false,
        }
    }

    /// [GRAIN] Roll the REDUCED studio field for IDLE/FALLBACK: a calm, dim
    /// breathing presence in the 2-row strip. Center columns glow faintly and
    /// pulse gently so the user knows the pill is alive. Unlit = fully black.
    fn roll_studio(&mut self, _amp: f32) {
        self.phase += 1.0;

        // Everything OFF.
        for d in self.dots.iter_mut() {
            *d = NONE;
        }

        // Gentle breathing: center columns pulse softly, fading toward edges.
        let center = (STUDIO_MTX_C0 + STUDIO_MTX_C1) as f32 / 2.0; // 12.0
        let max_dist = center - STUDIO_MTX_C0 as f32; // 12.0
        let breath = 0.15 + 0.10 * (self.phase * 0.04).sin();
        for r in STUDIO_MTX_R0..=STUDIO_MTX_R1 {
            for c in STUDIO_MTX_C0..=STUDIO_MTX_C1 {
                if is_edge(c, r) {
                    continue;
                }
                let dist = (c as f32 - center).abs();
                // Only the inner ~6 columns glow; outer columns stay black.
                let a = ((1.0 - dist / (max_dist * 0.5)).max(0.0) * breath).clamp(0.0, 0.5);
                if a > 0.02 {
                    self.dots[r * COLS + c] = [150, 160, 180, (a * 255.0) as u8];
                }
            }
        }
    }

    /// [GRAIN] Roll the PROCESSING state's studio field: orange sparkle across
    /// the 2-row strip, matching the collapsed pill's orange processing but
    /// constrained to the studio rows only. Unlit = fully black.
    fn roll_studio_processing(&mut self) {
        self.phase += 1.0;

        for d in self.dots.iter_mut() {
            *d = NONE;
        }

        for r in STUDIO_MTX_R0..=STUDIO_MTX_R1 {
            for c in STUDIO_MTX_C0..=STUDIO_MTX_C1 {
                if is_edge(c, r) {
                    continue;
                }
                let shade = self.rng.f32();
                let (rr, gg, bb) = if shade < 0.40 {
                    (255, 93, 30)   // deep orange
                } else if shade < 0.72 {
                    (255, 145, 70)  // mid orange
                } else {
                    (255, 185, 110) // light orange
                };
                let a = if self.rng.f32() < 0.30 {
                    0.60 + self.rng.f32() * 0.40
                } else {
                    0.10 + self.rng.f32() * 0.25
                };
                self.dots[r * COLS + c] = [rr, gg, bb, (a * 255.0) as u8];
            }
        }
    }

    /// [GRAIN] The RECORDING state's studio field: 2 rows (3–4), full pill width.
    /// Volume (mic amplitude) determines how many columns light up symmetrically
    /// from the horizontal center outward — silence = nothing lit, louder = more
    /// columns bloom toward the edges. Unlit = fully black (NONE).
    fn roll_studio_waveform(&mut self, amp: f32) {
        self.phase += 1.0;
        // Asymmetric smoothing: fast attack (0.65) so quiet speech registers
        // immediately, slower decay (0.35) so the glow lingers naturally.
        if amp > self.energy {
            self.energy = self.energy * 0.35 + amp * 0.65;
        } else {
            self.energy = self.energy * 0.7 + amp * 0.3;
        }
        let level = self.energy.clamp(0.0, 1.0);

        for d in self.dots.iter_mut() {
            *d = NONE;
        }

        // Center column is 12.0 (between cols 12 and 13 for a 25-wide grid).
        let center = (STUDIO_MTX_C0 + STUDIO_MTX_C1) as f32 / 2.0; // 12.0
        // Maximum distance from center to an edge column.
        let max_dist = center - STUDIO_MTX_C0 as f32; // 12.0
        // Square-root curve: amplifies low volumes (0.1 → 0.32, 0.25 → 0.50)
        // while keeping medium/high volumes natural (0.5 → 0.71, 1.0 → 1.0).
        let shaped = level.sqrt();
        let reach = shaped * max_dist;

        for r in STUDIO_MTX_R0..=STUDIO_MTX_R1 {
            for c in STUDIO_MTX_C0..=STUDIO_MTX_C1 {
                if is_edge(c, r) {
                    continue;
                }
                let dist = (c as f32 - center).abs(); // 0.0 to 12.0
                if dist > reach {
                    continue; // outside the volume reach → stays black
                }
                // Brightness: brighter near center, dimmer at the edges of reach.
                let proximity = 1.0 - (dist / reach.max(0.01));
                let a = (0.30 + proximity * 0.55 + self.rng.f32() * 0.08).clamp(0.0, 0.92);
                // Slight color variation: brighter at center. [GRAIN] Prompt Record
                // tints the whole waveform sky blue (same volume-reactive shape).
                let g = self.rng.f32();
                let (rr, gg, bb) = if self.prompt_recording {
                    if proximity > 0.7 {
                        (140, 200, 255) // bright sky-blue center
                    } else if g < 0.5 {
                        (100, 168, 236) // mid sky blue
                    } else {
                        (82, 144, 212) // dim sky-blue edge
                    }
                } else if proximity > 0.7 {
                    (200, 204, 212) // bright center
                } else if g < 0.5 {
                    (168, 174, 184) // mid
                } else {
                    (140, 148, 160) // dim edge
                };
                self.dots[r * COLS + c] = [rr, gg, bb, (a * 255.0) as u8];
            }
        }
    }

    /// Orange radial ripple in the 4×4 button zone — the constant "recording"
    /// pulse (inner cells lead, perimeter lags). Runs in recording + processing.
    fn roll_button(&mut self) {
        self.btn_angle = (self.btn_angle + 0.26) % (std::f32::consts::PI * 2.0);
        for lr in 0..4 {
            for lc in 0..4 {
                if (lr == 0 || lr == 3) && (lc == 0 || lc == 3) {
                    continue;
                }
                let dr = lr as f32 - 1.5;
                let dc = lc as f32 - 1.5;
                let rdist = (dr * dr + dc * dc).sqrt();
                let brightness = 0.5 + 0.5 * (self.btn_angle - rdist * 1.4).sin();
                let a = 0.04 + brightness * 0.96;
                let idx = (BTN_ROW + lr) * COLS + (BTN_COL + lc);
                self.dots[idx] = [255, 93, 30, (a * 255.0) as u8];
            }
        }
    }

    fn clear_to(&mut self, fill: Rgba) {
        for r in 0..ROWS {
            for c in 0..COLS {
                self.dots[r * COLS + c] = if is_edge(c, r) { NONE } else { fill };
            }
        }
    }

    fn roll(&mut self, state: PillState, amp: f32) {
        self.phase += 1.0;
        match state {
            PillState::Idle => self.roll_idle(),
            PillState::Processing => {
                self.roll_processing();
            }
            PillState::Recording => {
                self.roll_recording(amp);
                self.roll_button();
            }
            // [GRAIN] B4: placeholder — calm dim grid (reuse idle until designed).
            PillState::Fallback => self.roll_idle(),
        }
    }

    /// Calm idle: a soft band sweeps slowly across (4 s/pass) over a gentle
    /// breathing base — alive but quiet. Fills the whole grid (no gap).
    fn roll_idle(&mut self) {
        self.energy = 0.0;
        let sweep = (self.phase * 0.02).rem_euclid(1.0);
        let breath = 0.07 + 0.04 * (self.phase * 0.06).sin();
        for r in 0..ROWS {
            for c in 0..COLS {
                let idx = r * COLS + c;
                if is_edge(c, r) {
                    self.dots[idx] = NONE;
                    continue;
                }
                let p = c as f32 / COLS as f32;
                let mut dist = (p - sweep).abs();
                if dist > 0.5 {
                    dist = 1.0 - dist;
                }
                let bump = (-(dist * 8.0).powi(2)).exp();
                let a = (breath + 0.34 * bump).clamp(0.0, 0.55);
                self.dots[idx] = [150, 160, 180, (a * 255.0) as u8];
            }
        }
    }

    fn roll_processing(&mut self) {
        for r in 0..ROWS {
            for c in 0..COLS {
                let idx = r * COLS + c;
                if is_edge(c, r) {
                    self.dots[idx] = NONE;
                    continue;
                }
                let shade = self.rng.f32();
                let (rr, gg, bb) = if shade < 0.40 {
                    (255, 93, 30)
                } else if shade < 0.72 {
                    (255, 145, 70)
                } else {
                    (255, 185, 110)
                };
                let a = if self.rng.f32() < 0.25 {
                    0.60 + self.rng.f32() * 0.40
                } else {
                    0.08 + self.rng.f32() * 0.22
                };
                self.dots[idx] = [rr, gg, bb, (a * 255.0) as u8];
            }
        }
        self.energy = self.energy.max(0.42);
    }

    fn roll_recording(&mut self, amp: f32) {
        self.energy = self.energy * 0.35 + amp * 0.65;
        let lit_base = self.energy;
        let flicker = 0.10;
        let jitter = if lit_base > 0.001 {
            (self.rng.f32() - 0.5) * flicker
        } else {
            0.0
        };
        let lit_ratio = (lit_base + jitter).clamp(0.0, 0.94);

        let mut eligible = self.eligible.clone();
        let active_count = (eligible.len() as f32 * lit_ratio).round() as usize;
        let hot_count = (active_count as f32 * 0.08).round() as usize;
        for i in (1..eligible.len()).rev() {
            let j = (self.rng.f32() * (i as f32 + 1.0)) as usize;
            eligible.swap(i, j.min(i));
        }

        // [GRAIN] Prompt Record tints the same density field a grey/light-blue mix
        // so the user can see, at a glance, that they're now dictating an AI
        // instruction. Only the colors differ — placement/density are identical.
        let blue = self.prompt_recording;
        self.clear_to([12, 12, 12, 255]); // only lit pixels appear; unlit stay near-black dark grey
        for (k, &idx) in eligible.iter().enumerate() {
            if k >= active_count {
                break;
            }
            if k < hot_count {
                self.dots[idx] = if blue {
                    [150, 196, 255, 235] // light-blue hot dot
                } else {
                    [189, 193, 201, 235]
                };
            } else {
                let a = (0.34 + lit_base * 0.30 + self.rng.f32() * flicker).min(0.82);
                let g = self.rng.f32();
                let (rr, gg, bb) = if blue {
                    // Grey + light-blue mix: two blue tiers and one neutral grey.
                    if g < 0.33 {
                        (110, 162, 224) // blue
                    } else if g < 0.66 {
                        (140, 148, 160) // grey (the mix)
                    } else {
                        (172, 206, 246) // light blue
                    }
                } else if g < 0.33 {
                    (168, 174, 184)
                } else if g < 0.66 {
                    (140, 148, 160)
                } else {
                    (200, 204, 212)
                };
                self.dots[idx] = [rr, gg, bb, (a * 255.0) as u8];
            }
        }
        self.energy = (self.energy * 0.74).max(0.0);
    }
}

// [GRAIN] The pill's hover interactions (the ✓/✗ reveal panel shown on
// mouse-over in Recording/Processing, and the click-to-confirm/cancel that went
// with it) were removed — the pill is now a display-only surface in every state.

// ── Text (prompt riser) ─────────────────────────────────────────────────────

/// [GRAIN] The bundled primary face: Space Grotesk (Medium 500), subset to
/// European Latin + the exact punctuation/symbols the pill draws (~388 glyphs,
/// ~21 KB). Chosen for the "Apple × Teenage Engineering" aesthetic — a
/// proportional descendant of Space Mono, stylized yet readable next to the
/// dot-matrix field. SIL OFL (see `assets/SpaceGrotesk-OFL.txt`).
///
/// Bundling + subsetting is the pill's RAM lever: fontdue parses a font's WHOLE
/// glyph set eagerly (a full system sans is ~15-20 MB resident), so a ~388-glyph
/// face parses to well under 1 MB. It also makes the pill render identically on
/// every machine instead of depending on which system font happens to exist.
/// (Font *compression* — WOFF2/Brotli — would NOT help: rasterizers rebuild the
/// full uncompressed sfnt in RAM, so only subsetting moves the footprint.)
const PRIMARY_FONT_TTF: &[u8] = include_bytes!("../assets/SpaceGrotesk-pill.ttf");

/// Parse the bundled primary face. Called lazily (and re-called after the
/// idle-free drops it).
fn load_font() -> Option<fontdue::Font> {
    fontdue::Font::from_bytes(PRIMARY_FONT_TTF, fontdue::FontSettings::default()).ok()
}

/// [GRAIN] Load a broad-coverage system face for glyphs the subset primary
/// lacks (Cyrillic, Greek, CJK, …). Loaded ONLY when such a glyph actually needs
/// drawing (a non-Latin prompt name / caption / typed instruction), and dropped
/// again on idle — so the common all-Latin case never pays for it. Segoe UI is
/// preferred for its wide coverage; Arial/Tahoma are fallbacks.
fn load_fallback_font() -> Option<fontdue::Font> {
    for path in [
        "C:/Windows/Fonts/segoeui.ttf",
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/tahoma.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                return Some(font);
            }
        }
    }
    None
}

/// Pick the face to draw `text` with: the bundled primary unless `text` contains
/// a glyph the primary lacks but the fallback has (then the whole run uses the
/// fallback, keeping one consistent face per string). The all-Latin path always
/// returns the primary — the fallback is pure insurance for non-Latin text.
fn font_for<'a>(
    primary: &'a fontdue::Font,
    fallback: Option<&'a fontdue::Font>,
    text: &str,
) -> &'a fontdue::Font {
    if let Some(fb) = fallback {
        let needs_fallback = text.chars().any(|c| {
            !c.is_whitespace()
                && primary.lookup_glyph_index(c) == 0
                && fb.lookup_glyph_index(c) != 0
        });
        if needs_fallback {
            return fb;
        }
    }
    primary
}

/// True if the primary face is missing a glyph for any non-space char in `text`
/// (⇒ the fallback should be loaded before drawing it).
fn primary_missing_glyph(primary: &fontdue::Font, text: &str) -> bool {
    text.chars()
        .any(|c| !c.is_whitespace() && primary.lookup_glyph_index(c) == 0)
}

/// Draw `text` LEFT-aligned at `(x, cy_center)` (vertical center) into the
/// pixmap. Returns the advance width. Shared by the agent input card.
fn draw_text_left(
    pixmap: &mut Pixmap,
    font: &fontdue::Font,
    text: &str,
    x: f32,
    cy_center: f32,
    px: f32,
    color: [u8; 3],
    alpha: f32,
) -> f32 {
    let baseline = cy_center + px * 0.34;
    let mut pen = x;
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let data = pixmap.data_mut();
    for ch in text.chars() {
        let (m, bmp) = font.rasterize(ch, px);
        let gx = pen + m.xmin as f32;
        let gy = baseline - (m.height as f32 + m.ymin as f32);
        for yy in 0..m.height {
            for xx in 0..m.width {
                let ga = bmp[yy * m.width + xx] as f32 / 255.0 * alpha;
                if ga <= 0.003 {
                    continue;
                }
                let xi = (gx + xx as f32) as i32;
                let yi = (gy + yy as f32) as i32;
                if xi < 0 || yi < 0 || xi >= w || yi >= h {
                    continue;
                }
                let o = ((yi * w + xi) as usize) * 4;
                let inv = 1.0 - ga;
                let blend =
                    |s: u8, d: u8| -> u8 { ((s as f32 * ga) + (d as f32 * inv)).min(255.0) as u8 };
                data[o] = blend(color[0], data[o]);
                data[o + 1] = blend(color[1], data[o + 1]);
                data[o + 2] = blend(color[2], data[o + 2]);
                data[o + 3] = ((255.0 * ga) + (data[o + 3] as f32 * inv)).min(255.0) as u8;
            }
        }
        pen += m.advance_width;
    }
    pen - x
}

/// Measure a string's advance width at `px` (no rasterization).
fn text_width(font: &fontdue::Font, text: &str, px: f32) -> f32 {
    text.chars()
        .map(|ch| font.metrics(ch, px).advance_width)
        .sum()
}

/// Trim `text` so it fits within `max_w`, appending an ellipsis when cut. Keeps
/// the label inside its reserved zone so it never collides with the arrows.
fn truncate_to_width(font: &fontdue::Font, text: &str, px: f32, max_w: f32) -> String {
    if text_width(font, text, px) <= max_w {
        return text.to_string();
    }
    let ell = '\u{2026}'; // …
    let ell_w = font.metrics(ell, px).advance_width;
    let mut out = String::new();
    let mut acc = 0.0;
    for ch in text.chars() {
        let cw = font.metrics(ch, px).advance_width;
        if acc + cw + ell_w > max_w {
            break;
        }
        acc += cw;
        out.push(ch);
    }
    out.push(ell);
    out
}

struct CachedText {
    total_width: f32,
    glyphs: Vec<(fontdue::Metrics, Vec<u8>)>,
}

impl CachedText {
    fn new(font: &fontdue::Font, text: &str, px: f32) -> Self {
        let glyphs: Vec<_> = text.chars().map(|ch| font.rasterize(ch, px)).collect();
        let total_width = glyphs.iter().map(|(m, _)| m.advance_width).sum();
        CachedText {
            total_width,
            glyphs,
        }
    }
}

/// Blend a pre-rasterized CachedText centered at `(cx, cy_center)` into the pixmap.
/// `alpha` fades the whole label (for the slide-in).
fn draw_cached_text_centered(
    pixmap: &mut Pixmap,
    cached: &CachedText,
    center: (f32, f32),
    px: f32,
    color: [u8; 3],
    alpha: f32,
) {
    let (cx, cy_center) = center;
    let baseline = cy_center + px * 0.34;
    let mut pen = cx - cached.total_width / 2.0;
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let data = pixmap.data_mut();
    for (m, bmp) in &cached.glyphs {
        let gx = pen + m.xmin as f32;
        let gy = baseline - (m.height as f32 + m.ymin as f32);
        for yy in 0..m.height {
            for xx in 0..m.width {
                let ga = bmp[yy * m.width + xx] as f32 / 255.0 * alpha;
                if ga <= 0.003 {
                    continue;
                }
                let x = (gx + xx as f32) as i32;
                let y = (gy + yy as f32) as i32;
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                let o = ((y * w + x) as usize) * 4;
                let inv = 1.0 - ga;
                let blend =
                    |s: u8, d: u8| -> u8 { ((s as f32 * ga) + (d as f32 * inv)).min(255.0) as u8 };
                data[o] = blend(color[0], data[o]);
                data[o + 1] = blend(color[1], data[o + 1]);
                data[o + 2] = blend(color[2], data[o + 2]);
                data[o + 3] = ((255.0 * ga) + (data[o + 3] as f32 * inv)).min(255.0) as u8;
            }
        }
        pen += m.advance_width;
    }
}

/// [GRAIN] Draw a small "return / enter" arrow (↵) as a vector, centered
/// vertically at `cy`, spanning `w` px wide. Used on the agent card's Confirm
/// button because the subset primary font has no U+21B5 glyph. A short right→
/// left shaft with an up-hook on the left and a small arrowhead.
fn draw_return_arrow(pixmap: &mut Pixmap, x: f32, cy: f32, w: f32, color: [u8; 3], alpha: f32) {
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    if a == 0 {
        return;
    }
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(Color::from_rgba8(color[0], color[1], color[2], a));
    let stroke = tiny_skia::Stroke {
        width: 1.5,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };
    let h = w * 0.72;
    let (top, bot) = (cy - h * 0.5, cy + h * 0.5);
    let right = x + w;
    let left = x + w * 0.18;
    // Vertical hook down the right side, then the shaft running left along the
    // bottom to the arrowhead.
    let mut pb = PathBuilder::new();
    pb.move_to(right, top);
    pb.line_to(right, bot);
    pb.line_to(left, bot);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    // Arrowhead at the left end of the shaft.
    let mut head = PathBuilder::new();
    let hd = w * 0.28;
    head.move_to(left + hd, bot - hd);
    head.line_to(left, bot);
    head.line_to(left + hd, bot + hd);
    if let Some(path) = head.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// Blend one freshly-rasterized glyph bitmap into the pixmap at absolute
/// top-left `(gx, gy)`. Shared by [`draw_word`] (Studio Window transcript).
fn blend_glyph(
    pixmap: &mut Pixmap,
    m: &fontdue::Metrics,
    bmp: &[u8],
    gx: f32,
    gy: f32,
    color: [u8; 3],
    alpha: f32,
) {
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let data = pixmap.data_mut();
    for yy in 0..m.height {
        for xx in 0..m.width {
            let ga = bmp[yy * m.width + xx] as f32 / 255.0 * alpha;
            if ga <= 0.003 {
                continue;
            }
            let x = (gx + xx as f32) as i32;
            let y = (gy + yy as f32) as i32;
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let o = ((y * w + x) as usize) * 4;
            let inv = 1.0 - ga;
            let blend =
                |s: u8, d: u8| -> u8 { ((s as f32 * ga) + (d as f32 * inv)).min(255.0) as u8 };
            data[o] = blend(color[0], data[o]);
            data[o + 1] = blend(color[1], data[o + 1]);
            data[o + 2] = blend(color[2], data[o + 2]);
            data[o + 3] = ((255.0 * ga) + (data[o + 3] as f32 * inv)).min(255.0) as u8;
        }
    }
}

/// Left-aligned word draw with optional blur (`blur_radius` 0 = crisp,
/// committed text; >0 = the Studio Window's volatile partial-text look, the
/// stabilizer's "not settled yet" signal made visible). Rasterizes per frame
/// (the transcript changes every frame during dictation, so caching buys
/// nothing) and returns the advance width consumed.
fn draw_word(
    pixmap: &mut Pixmap,
    font: &fontdue::Font,
    word: &str,
    px: f32,
    pen_x: f32,
    baseline: f32,
    color: [u8; 3],
    alpha: f32,
    blur_radius: usize,
) -> f32 {
    let mut pen = pen_x;
    for ch in word.chars() {
        let (m, mut bmp) = font.rasterize(ch, px);
        if blur_radius > 0 {
            box_blur(&mut bmp, m.width, m.height, blur_radius);
        }
        let gx = pen + m.xmin as f32;
        let gy = baseline - (m.height as f32 + m.ymin as f32);
        blend_glyph(pixmap, &m, &bmp, gx, gy, color, alpha);
        pen += m.advance_width;
    }
    pen - pen_x
}

// [GRAIN] Reusable off-screen layer for the live transcript. Rendering the text
// into its own transparent pixmap lets us apply the top dissolve gradient to the
// GLYPHS ONLY (fading them into the card's dark surface) without punching a hole
// in the card background. Kept thread-local + reused across frames so the Studio
// Window's per-frame draw stays allocation-free ("destroy if not in use").
thread_local! {
    static STUDIO_TEXT_LAYER: RefCell<Option<Pixmap>> = const { RefCell::new(None) };
}

/// Paint the whole Studio Window card into `pixmap` (which must be the Studio
/// size). Windowing-free so it renders identically in the app and in a PNG test.
///
/// [GRAIN] Layout mirrors Handy's live-transcription overlay
/// (`src/overlay/RecordingOverlay.*`): the live transcript fills the TOP region
/// (bottom-anchored — newest line lowest — dissolving into the surface at the
/// top edge) and a single control row is pinned to the BOTTOM: recording dot
/// (left) · reactive waveform (center) · elapsed timer + cancel glyph (right).
/// `fade` is the whole-card opacity (0..1), `phase` the animation clock, `amp`
/// the current mic level (0..1), `elapsed_secs` the recording timer.
/// [GRAIN] Paint the unified pill's EXPANDED (streaming) surface: a black capsule
/// that eases outward from the collapsed pill width to the full card, carrying the
/// live transcript on top and — pinned to the bottom — the recording dot (left),
/// the dot-matrix aura as the "waveform" (center), and the cancel X (right). The
/// dot-matrix is the very same COLS×ROWS field the collapsed pill shows, so the
/// two states are one continuous design. Windowing-free (rendered to a PNG in
/// tests). `expand` (0..1) grows the width and fades in the transcript + side
/// controls; the dot-matrix stays full-opacity throughout (it IS the small pill).
#[allow(clippy::too_many_arguments)]
fn paint_studio_card(
    pixmap: &mut Pixmap,
    asr: &AsrDisplay,
    state: PillState,
    fade: f32,
    phase: f32,
    font: Option<&fontdue::Font>,
    // [GRAIN] The (eased) drawn card height and the transcript's line count. The
    // card is bottom-anchored inside the fixed-size window; `n_lines` decides the
    // top gap + whether the top dissolve is active (only at the 4-line cap).
    card_h: f32,
    n_lines: usize,
    // Per-word reveal multipliers (see `draw_transcript`); `&[]` = fully revealed.
    reveal_alpha: &[f32],
    // The pre-rolled dot-matrix aura (COLS×ROWS) — drawn as the center waveform.
    dots: &[Rgba],
    // Width-grow 0..1 (collapsed pill width → full card width).
    expand: f32,
) {
    let (w, h) = studio_pixel_size();
    let (wf, hf) = (w as f32, h as f32);
    pixmap.fill(Color::TRANSPARENT);

    // The card hugs the window's BOTTOM edge and grows upward; the region above
    // it stays fully transparent so shorter cards simply appear smaller.
    let card_h = card_h.clamp(studio_card_height(0), hf);
    let card_top = hf - card_h;
    let at_cap = n_lines >= STUDIO_MAX_LINES;

    // Capsule spans an eased width, centered; bottom-anchored, grows upward.
    let expand = expand.clamp(0.0, 1.0);
    let cap_w = STUDIO_MIN_W + (wf - STUDIO_MIN_W) * expand;
    let cap_left = (wf - cap_w) / 2.0;
    let cap_right = cap_left + cap_w;

    // 1) Capsule background: a near-black panel with a 1px inner top highlight so
    // it reads as a raised premium surface, not a flat rectangle.
    let mut bg = Paint {
        anti_alias: true,
        ..Default::default()
    };
    bg.set_color(Color::from_rgba8(13, 13, 15, (244.0 * fade) as u8));
    if let Some(path) = rounded_rect_path(cap_left, card_top, cap_w, card_h, STUDIO_CORNER_R) {
        pixmap.fill_path(&path, &bg, FillRule::Winding, Transform::identity(), None);
    }
    let mut hair = Paint {
        anti_alias: true,
        ..Default::default()
    };
    hair.set_color(Color::from_rgba8(
        255,
        255,
        255,
        (14.0 * fade * expand) as u8,
    ));
    if let Some(rect) = Rect::from_ltrb(
        cap_left + STUDIO_CORNER_R,
        card_top + 0.5,
        cap_right - STUDIO_CORNER_R,
        card_top + 1.5,
    ) {
        pixmap.fill_path(
            &PathBuilder::from_rect(rect),
            &hair,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // 2) Live transcript, fading in with the expansion. While growing (1–3 lines)
    // it sits below a small top gap and stays crisp to the top; at the cap the gap
    // closes and the top dissolve (`at_cap`) melts the oldest line into the surface.
    let top_gap = if at_cap { 0.0 } else { STUDIO_GROW_TOP_GAP };
    let text_top = card_top + top_gap;
    let ctrl_top = card_top + card_h - STUDIO_CTRL_H - STUDIO_BOTTOM_PAD;
    if let Some(font) = font {
        // [GRAIN] Hold the transcript until the pill is fully expanded — the first
        // word must not appear mid-grow. Once open, it's drawn at full opacity and
        // the per-word reveal fades it in.
        if expand >= STUDIO_TEXT_GATE && n_lines > 0 {
            // Leave a hair of space so descenders never touch the control row.
            draw_transcript(
                pixmap,
                asr,
                font,
                text_top,
                ctrl_top - 2.0,
                fade,
                at_cap,
                reveal_alpha,
            );
        }
    }

    // 3) Control row pinned to the card's bottom.
    draw_control_row(
        pixmap, state, phase, dots, cap_left, cap_right, ctrl_top, fade, expand,
    );
}

/// Studio pixel size — the single source of truth used by both the free painter
/// and `App::win_size_for(Studio)`. Height is the TALLEST the card ever gets
/// (the 4-line cap); shorter cards draw bottom-anchored inside this window.
fn studio_pixel_size() -> (u32, u32) {
    (
        (STUDIO_W * SCALE).round() as u32,
        // [GRAIN] Reserve a transparent band ABOVE the (bottom-anchored) card for
        // the prompt-switcher top capsule. The card itself is unchanged; the band
        // stays empty (click-through) until a prompt switch reveals the capsule.
        ((STUDIO_MAX_CARD_H + STUDIO_TOP_RESERVE) * SCALE).round() as u32,
    )
}

/// The card's drawn height for `n` visible transcript lines (clamped 1..=cap).
/// Below the cap it carries the small top gap; at the cap the gap closes so the
/// text meets the top edge.
fn studio_card_height(n: usize) -> f32 {
    let n = n.clamp(1, STUDIO_MAX_LINES);
    let top_gap = if n >= STUDIO_MAX_LINES {
        0.0
    } else {
        STUDIO_GROW_TOP_GAP
    };
    top_gap + n as f32 * STUDIO_LINE_HEIGHT + STUDIO_CTRL_H + STUDIO_BOTTOM_PAD
}

/// How many lines the current transcript wraps to at the Studio width (before
/// the cap) — this drives the card's grow height. `1` when empty so a fresh
/// session opens at its smallest size rather than flashing tall.
fn studio_line_count(asr: &AsrDisplay, font: Option<&fontdue::Font>) -> usize {
    let Some(font) = font else { return 0 };
    let runs = asr.display_runs();
    if runs.is_empty() {
        // No transcript yet → 0 lines: the card opens at the bare dot-matrix
        // height (like the small pill) and grows as words stream in.
        return 0;
    }
    let (cw, _) = studio_pixel_size();
    let max_w = cw as f32 - 2.0 * STUDIO_PAD;
    wrap_runs(font, &runs, STUDIO_TEXT_PX, max_w).len().max(1)
}

/// Render the live transcript into the top region `[text_top, text_bottom]`,
/// bottom-anchored (newest line lowest), with the top `STUDIO_FADE_PX` dissolving
/// into the card surface — Handy's `mask-image` top fade, done here by rendering
/// the glyphs into their own layer and ramping that layer's alpha at the top.
fn draw_transcript(
    card: &mut Pixmap,
    asr: &AsrDisplay,
    font: &fontdue::Font,
    text_top: f32,
    text_bottom: f32,
    fade: f32,
    // [GRAIN] Only dissolve the top edge once the card has hit the 4-line cap and
    // is scrolling. While it's still growing (1–3 lines) the text is crisp all
    // the way up to the small top gap — no fade.
    fade_top: bool,
    // [GRAIN] Per-word reveal multipliers (0..1), indexed by global word order in
    // `asr.display_runs()`. Freshly-appeared words ramp 0→1 so text fades in
    // smoothly. Empty slice (or missing index) means fully revealed.
    reveal_alpha: &[f32],
) {
    let runs = asr.display_runs();
    if runs.is_empty() {
        return;
    }
    let (cw, _) = studio_pixel_size();
    let region_h = text_bottom - text_top;
    if region_h <= 1.0 {
        return;
    }
    let rh = region_h.ceil() as u32;
    let max_w = cw as f32 - 2.0 * STUDIO_PAD;
    let lines = wrap_runs(font, &runs, STUDIO_TEXT_PX, max_w);
    // Keep only the last N lines; older ones have scrolled off the top.
    let shown = &lines[lines.len().saturating_sub(STUDIO_MAX_LINES)..];
    let n = shown.len();
    let space_w = font.metrics(' ', STUDIO_TEXT_PX).advance_width;

    STUDIO_TEXT_LAYER.with(|cell| {
        let mut slot = cell.borrow_mut();
        // Reuse the scratch layer; only reallocate when the region size changes.
        let need_new = !matches!(slot.as_ref(), Some(p) if p.width() == cw && p.height() == rh);
        if need_new {
            *slot = Pixmap::new(cw, rh);
        }
        let Some(layer) = slot.as_mut() else {
            return;
        };
        layer.fill(Color::TRANSPARENT);

        // Map the shown words back to their global index in `runs` (the shown
        // words are the contiguous TAIL of the full run list) so each word picks
        // up the right per-word reveal alpha.
        let shown_word_count: usize = shown.iter().map(|l| l.words.len()).sum();
        let mut gidx = runs.len().saturating_sub(shown_word_count);

        // Bottom-anchor: newest line (i = n-1) sits flush at the region bottom;
        // earlier lines stack upward. Local coords (y down from the region top).
        for (i, line) in shown.iter().enumerate() {
            let box_bottom = region_h - (n - 1 - i) as f32 * STUDIO_LINE_HEIGHT;
            let baseline = box_bottom - (STUDIO_LINE_HEIGHT - STUDIO_TEXT_PX);
            let mut pen = STUDIO_PAD;
            for (word, style) in &line.words {
                if pen > STUDIO_PAD {
                    pen += space_w;
                }
                let reveal = reveal_alpha.get(gidx).copied().unwrap_or(1.0);
                gidx += 1;
                // Committed text is stable/pasteable → solid warm white, crisp.
                // [GRAIN] transcribe-cpp's auto-commit is CONSERVATIVE: it can
                // leave most of an utterance in the "tentative" tail for a long
                // time even though that text is already stable and flicker-free.
                // Styling that whole tail as dim grey made the preview read as
                // "nothing is committing" (only the first line or two ever looked
                // final). So a STABLE tail now renders near-committed white — it
                // is trustworthy text — and only the genuinely volatile
                // (unstable) tail is dimmed, marking the live decoding edge.
                let (color, alpha): ([u8; 3], f32) = match style {
                    RunStyle::Committed => ([238, 236, 232], 0.97),
                    RunStyle::Partial { stable: true } => ([232, 230, 226], 0.93),
                    RunStyle::Partial { stable: false } => ([196, 200, 208], 0.60),
                };
                pen += draw_word(
                    layer,
                    font,
                    word,
                    STUDIO_TEXT_PX,
                    pen,
                    baseline,
                    color,
                    alpha * reveal,
                    0,
                );
            }
        }

        // Dissolve the top band so older lines melt into the dark surface —
        // only once the card has capped and is scrolling (while growing, the
        // text stays crisp up to the top gap).
        if fade_top {
            fade_top_band(layer, STUDIO_FADE_PX);
        }

        // Composite over the card with the whole-card opacity.
        card.draw_pixmap(
            0,
            text_top.round() as i32,
            layer.as_ref(),
            &PixmapPaint {
                opacity: fade,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    });
}

/// Ramp the alpha of the top `fade_px` rows of a (premultiplied) layer from 0 at
/// the very top to 1 at the band's bottom, so text there fades to nothing.
fn fade_top_band(layer: &mut Pixmap, fade_px: f32) {
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    let band = (fade_px.ceil() as usize).min(h);
    if band == 0 || w == 0 {
        return;
    }
    let data = layer.data_mut();
    for y in 0..band {
        let f = (((y as f32 + 0.5) / fade_px).clamp(0.0, 1.0)) * 255.0;
        for x in 0..w {
            let o = (y * w + x) * 4;
            // Premultiplied RGBA → scale every channel to keep it valid.
            for k in 0..4 {
                data[o + k] = (data[o + k] as u32 * f as u32 / 255) as u8;
            }
        }
    }
}

/// The bottom control row: recording dot (left) · reactive waveform (center) ·
/// elapsed timer + cancel glyph (right), vertically centered in `STUDIO_CTRL_H`.
/// Processing/Fallback swap the dot for a spinner and the waveform for a status
/// label. The cancel glyph is display-only for now (the pill has no back-channel
/// to the core); it mirrors Handy's `.sx` purely for visual parity.
/// The unified pill's bottom control row: recording dot (left) · the dot-matrix
/// aura as the "waveform" (center) · cancel X (right). The dot/X ride the capsule's
/// animated edges (`cap_left`/`cap_right`) and fade in with `expand`; the matrix
/// is drawn at full opacity and fixed center (it is the collapsed pill, always
/// present). Processing/Fallback keep the same layout — the matrix itself carries
/// the state animation (processing shimmer), and the left dot becomes a spinner.
#[allow(clippy::too_many_arguments)]
fn draw_control_row(
    pixmap: &mut Pixmap,
    state: PillState,
    phase: f32,
    dots: &[Rgba],
    cap_left: f32,
    cap_right: f32,
    ctrl_top: f32,
    fade: f32,
    expand: f32,
) {
    let cy = ctrl_top + STUDIO_CTRL_H / 2.0;
    let recording = state == PillState::Recording;
    // Side controls belong to the EXPANDED form only — fade them with the width so
    // the collapsed pill (expand≈0) is just the bare dot-matrix, as before.
    let side = fade * expand.clamp(0.0, 1.0);

    // LEFT — pulsing recording dot, or a spinner while finalizing.
    let left_cx = cap_left + STUDIO_PAD + 6.0;
    if recording {
        draw_rec_dot(pixmap, left_cx, cy, phase, side);
    } else {
        draw_spinner(pixmap, left_cx, cy, phase, side);
    }

    // RIGHT — cancel glyph (display-only), 22px circle inset from the edge.
    let x_cx = cap_right - STUDIO_PAD - 11.0;
    draw_x_button(pixmap, x_cx, cy, side);

    // CENTER — the reduced 2-row dot-matrix, centered on the control row.
    // Rows 3–4 are the studio strip; their vertical center sits on `cy`.
    let (w, _) = studio_pixel_size();
    let field_w = COLS as f32 * CELL;
    let mtx_left = (w as f32 - field_w) / 2.0;
    let visible_mid = (STUDIO_MTX_R0 as f32 + STUDIO_MTX_R1 as f32 + 1.0) * 0.5 * CELL;
    draw_dot_matrix(pixmap, dots, mtx_left, cy - visible_mid, fade);
}

/// Draw the COLS×ROWS aura dot-field at `(left, top)` (cell = `CELL·SCALE`),
/// skipping the rounded-corner edge cells and scaling every dot's alpha by `fade`.
/// This is the exact field the collapsed pill renders, reused as the expanded
/// pill's center "waveform" so the two states share one dot language.
fn draw_dot_matrix(pixmap: &mut Pixmap, dots: &[Rgba], left: f32, top: f32, fade: f32) {
    let cell_px = CELL * SCALE;
    let radius = DOT_D * SCALE / 2.0;
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    for row in 0..ROWS {
        for col in 0..COLS {
            if is_edge(col, row) {
                continue;
            }
            let c = dots[row * COLS + col];
            let a = (c[3] as f32 * fade) as u8;
            if a == 0 {
                continue;
            }
            let dx = left + col as f32 * cell_px + cell_px / 2.0;
            let dy = top + row as f32 * cell_px + cell_px / 2.0;
            if let Some(circle) = PathBuilder::from_circle(dx, dy, radius) {
                paint.set_color(Color::from_rgba8(c[0], c[1], c[2], a));
                pixmap.fill_path(
                    &circle,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

/// A solid accent dot with an expanding, fading pulse ring (Handy's `.sdot`).
fn draw_rec_dot(pixmap: &mut Pixmap, cx: f32, cy: f32, phase: f32, fade: f32) {
    let mut p = Paint {
        anti_alias: true,
        ..Default::default()
    };
    // One pulse every ~1.9s (phase advances one step per rendered frame ≈ TICK).
    let t = ((phase * TICK.as_secs_f32()) / 1.9).fract();
    let ring_r = 3.5 + t * 7.0;
    let ring_a = ((1.0 - t) * 0.32 * fade * 255.0) as u8;
    if ring_a > 0 {
        if let Some(c) = PathBuilder::from_circle(cx, cy, ring_r) {
            p.set_color(Color::from_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], ring_a));
            pixmap.fill_path(&c, &p, FillRule::Winding, Transform::identity(), None);
        }
    }
    if let Some(c) = PathBuilder::from_circle(cx, cy, 3.5) {
        p.set_color(Color::from_rgba8(
            ACCENT[0],
            ACCENT[1],
            ACCENT[2],
            (fade * 255.0) as u8,
        ));
        pixmap.fill_path(&c, &p, FillRule::Winding, Transform::identity(), None);
    }
}

/// A rotating dot-ring spinner (the finalizing indicator, Handy's `.sspinner`).
fn draw_spinner(pixmap: &mut Pixmap, cx: f32, cy: f32, phase: f32, fade: f32) {
    let mut p = Paint {
        anti_alias: true,
        ..Default::default()
    };
    let r = 6.0;
    let head = phase * 0.18;
    const N: usize = 8;
    for i in 0..N {
        let ang = head + i as f32 * std::f32::consts::TAU / N as f32;
        let bright = i as f32 / (N - 1) as f32; // comet trail
        let a = ((0.12 + bright * 0.8) * fade * 255.0) as u8;
        let dx = cx + ang.cos() * r;
        let dy = cy + ang.sin() * r;
        if let Some(c) = PathBuilder::from_circle(dx, dy, 1.4) {
            p.set_color(Color::from_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], a));
            pixmap.fill_path(&c, &p, FillRule::Winding, Transform::identity(), None);
        }
    }
}

/// The circular cancel affordance — a subtle hairline disc with a muted ✗
/// (Handy's `.sx`). Display-only: it is drawn for visual parity but the pill has
/// no back-channel to trigger a cancel yet.
fn draw_x_button(pixmap: &mut Pixmap, cx: f32, cy: f32, fade: f32) {
    let mut bg = Paint {
        anti_alias: true,
        ..Default::default()
    };
    bg.set_color(Color::from_rgba8(255, 255, 255, (16.0 * fade) as u8));
    if let Some(c) = PathBuilder::from_circle(cx, cy, 11.0) {
        pixmap.fill_path(&c, &bg, FillRule::Winding, Transform::identity(), None);
    }
    let mut stroke_paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    stroke_paint.set_color(Color::from_rgba8(168, 168, 168, (fade * 235.0) as u8));
    let d = 3.8;
    let mut pb = PathBuilder::new();
    pb.move_to(cx - d, cy - d);
    pb.line_to(cx + d, cy + d);
    pb.move_to(cx + d, cy - d);
    pb.line_to(cx - d, cy + d);
    if let Some(path) = pb.finish() {
        let stroke = tiny_skia::Stroke {
            width: 1.6,
            line_cap: tiny_skia::LineCap::Round,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }
}

// ── Mic capture (direct, low-latency) ───────────────────────────────────────

fn start_mic(amp: Arc<AtomicU32>) -> Option<cpal::Stream> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let device = cpal::default_host().default_input_device()?;
    let supported = device.default_input_config().ok()?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let err_fn = |e| eprintln!("mic stream error: {e}");

    let shape = move |rms: f32| {
        let floor = 0.008_f32;
        let gated = if rms < floor {
            0.0
        } else {
            (rms - floor) / (1.0 - floor)
        };
        let reference = 0.15_f32;
        let shaped = (gated / reference).min(1.0).sqrt();
        amp.store(shaped.to_bits(), Ordering::Relaxed);
    };

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &_| {
                    let n = data.len().max(1) as f32;
                    let sum: f32 = data.iter().map(|s| s * s).sum();
                    shape((sum / n).sqrt());
                },
                err_fn,
                None,
            )
            .ok()?,
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                &config,
                move |data: &[i16], _: &_| {
                    let n = data.len().max(1) as f32;
                    let sum: f32 = data
                        .iter()
                        .map(|&s| {
                            let f = s as f32 / 32768.0;
                            f * f
                        })
                        .sum();
                    shape((sum / n).sqrt());
                },
                err_fn,
                None,
            )
            .ok()?,
        _ => return None,
    };
    stream.play().ok()?;
    Some(stream)
}

// ── Win32 layered-window presentation (true per-pixel transparency) ─────────

#[cfg(windows)]
mod present {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use tiny_skia::Pixmap;
    use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA,
        AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP,
        HDC, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, ShowWindow, UpdateLayeredWindow, GWL_EXSTYLE,
        SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    };

    fn hwnd_of(window: &winit::window::Window) -> Option<HWND> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut core::ffi::c_void)),
            _ => None,
        }
    }

    pub fn make_layered(window: &winit::window::Window) {
        if let Some(hwnd) = hwnd_of(window) {
            unsafe {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                // [GRAIN] WS_EX_LAYERED for per-pixel alpha; WS_EX_NOACTIVATE so a
                // Prompt Record click on the pill delivers WM_LBUTTONDOWN (→ winit
                // MouseInput) WITHOUT activating the window — the dictation target
                // keeps focus, so the paste still lands where the user is typing.
                SetWindowLongPtrW(
                    hwnd,
                    GWL_EXSTYLE,
                    ex | WS_EX_LAYERED.0 as isize | WS_EX_NOACTIVATE.0 as isize,
                );
            }
        }
    }

    /// Reveal the window WITHOUT activating it — an overlay must never steal
    /// foreground focus from the text field being dictated into. Call only after
    /// the presenter has blitted content, or the window appears empty.
    pub fn show_window(window: &winit::window::Window) {
        if let Some(hwnd) = hwnd_of(window) {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
    }

    pub fn hide_window(window: &winit::window::Window) {
        if let Some(hwnd) = hwnd_of(window) {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    /// [GRAIN] Toggle keyboard focusability for the agent input. The pill is
    /// normally WS_EX_NOACTIVATE (an overlay must never steal focus); the agent
    /// INPUT is the opposite — it exists to be typed into.
    pub fn set_focusable(window: &winit::window::Window, focusable: bool) {
        if let Some(hwnd) = hwnd_of(window) {
            unsafe {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let flag = WS_EX_NOACTIVATE.0 as isize;
                let next = if focusable { ex & !flag } else { ex | flag };
                if next != ex {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
                }
            }
        }
    }

    /// [GRAIN] Pull the (shown) window to the foreground and grab keyboard focus.
    /// A window summoned by a hotkey in ANOTHER process is subject to Windows'
    /// foreground lock, so we bridge the foreground thread's input queue to ours
    /// first — the same dance the core app uses for its own summoned surfaces.
    pub fn force_foreground(window: &winit::window::Window) {
        use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
        use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
        use windows::Win32::UI::WindowsAndMessaging::{
            BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
            SW_SHOW,
        };
        let Some(hwnd) = hwnd_of(window) else { return };
        unsafe {
            let fg = GetForegroundWindow();
            let our_tid = GetCurrentThreadId();
            let fg_tid = GetWindowThreadProcessId(fg, None);
            let attached = fg_tid != 0
                && fg_tid != our_tid
                && AttachThreadInput(fg_tid, our_tid, true).as_bool();
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(hwnd);
            if attached {
                let _ = AttachThreadInput(fg_tid, our_tid, false);
            }
        }
    }

    /// [GRAIN] Read CF_UNICODETEXT off the clipboard (Ctrl+V in the agent input)
    /// without pulling in a clipboard crate. Best-effort: `None` on any failure.
    pub fn read_clipboard_text(window: &winit::window::Window) -> Option<String> {
        use windows::Win32::Foundation::HGLOBAL;
        use windows::Win32::System::DataExchange::{
            CloseClipboard, GetClipboardData, OpenClipboard,
        };
        use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
        const CF_UNICODETEXT: u32 = 13;
        let hwnd = hwnd_of(window)?;
        unsafe {
            if OpenClipboard(hwnd).is_err() {
                return None;
            }
            let out = (|| {
                let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
                let hglobal = HGLOBAL(handle.0 as _);
                let ptr = GlobalLock(hglobal) as *const u16;
                if ptr.is_null() {
                    return None;
                }
                let mut len = 0usize;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                let _ = GlobalUnlock(hglobal);
                Some(text)
            })();
            let _ = CloseClipboard();
            out
        }
    }

    /// Caches the GDI memory DC + DIB section for the window's fixed size so each
    /// frame is just a memcpy + `UpdateLayeredWindow` — no per-frame allocation or
    /// GDI handle churn (that churn was the rolling-mode CPU spike).
    pub struct Presenter {
        hwnd: HWND,
        mem: HDC,
        dib: HBITMAP,
        old: HGDIOBJ,
        bits: *mut u8,
        w: i32,
        h: i32,
    }

    impl Presenter {
        pub fn new(window: &winit::window::Window, w: i32, h: i32) -> Option<Self> {
            let hwnd = hwnd_of(window)?;
            unsafe {
                let mem = CreateCompatibleDC(None);
                let bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: w,
                        biHeight: -h, // top-down
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
                let dib = match CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                    Ok(d) => d,
                    Err(_) => {
                        let _ = DeleteDC(mem);
                        return None;
                    }
                };
                let old = SelectObject(mem, HGDIOBJ(dib.0));
                Some(Presenter {
                    hwnd,
                    mem,
                    dib,
                    old,
                    bits: bits as *mut u8,
                    w,
                    h,
                })
            }
        }

        /// Copy the pixmap (premultiplied RGBA) into the cached DIB (BGRA) and
        /// composite it onto the desktop. Reuses all GDI objects across frames.
        pub fn blit(&self, pixmap: &Pixmap) {
            unsafe {
                let src = pixmap.data();
                let n = (self.w * self.h) as usize;
                let dst = std::slice::from_raw_parts_mut(self.bits, n * 4);
                for i in 0..n {
                    let o = i * 4;
                    dst[o] = src[o + 2]; // B
                    dst[o + 1] = src[o + 1]; // G
                    dst[o + 2] = src[o]; // R
                    dst[o + 3] = src[o + 3]; // A
                }
                let size = SIZE {
                    cx: self.w,
                    cy: self.h,
                };
                let src_pt = POINT { x: 0, y: 0 };
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                // hdcDst = None (no position change) avoids touching the screen DC.
                let _ = UpdateLayeredWindow(
                    self.hwnd,
                    None,
                    None,
                    Some(&size),
                    self.mem,
                    Some(&src_pt),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
            }
        }
    }

    impl Drop for Presenter {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.mem, self.old);
                let _ = DeleteObject(HGDIOBJ(self.dib.0));
                let _ = DeleteDC(self.mem);
            }
        }
    }
}

#[cfg(not(windows))]
mod present {
    use tiny_skia::Pixmap;
    pub fn make_layered(_w: &winit::window::Window) {}
    pub fn show_window(_w: &winit::window::Window) {}
    pub fn hide_window(_w: &winit::window::Window) {}
    pub fn set_focusable(_w: &winit::window::Window, _f: bool) {}
    pub fn force_foreground(_w: &winit::window::Window) {}
    pub fn read_clipboard_text(_w: &winit::window::Window) -> Option<String> {
        None
    }
    pub struct Presenter;
    impl Presenter {
        pub fn new(_w: &winit::window::Window, _w2: i32, _h: i32) -> Option<Self> {
            Some(Presenter)
        }
        pub fn blit(&self, _p: &Pixmap) {}
    }
}

// ── Core event link (WebSocket → pill state) ────────────────────────────────

/// State driven by the core over the event WS.
#[derive(Clone)]
struct Remote {
    state: PillState,
    visible: bool,
    /// Where the pill anchors; `None` means the user disabled the overlay, so the
    /// pill never shows regardless of session events.
    anchor: OverlayPosition,
    /// [GRAIN] Active post-processing prompt title (from `PromptChanged`), shown
    /// in the riser. `prompt_seq` bumps on each switch so the App can detect it
    /// and trigger the riser (+ a brief reveal when idle).
    prompt_name: String,
    prompt_seq: u64,
    /// [GRAIN] Which surface to present. Starts `Collapsed` for EVERY session; a
    /// streaming session (see `streaming`) flips it to `Studio` only when the first
    /// transcript word arrives, so the pill visibly expands from the small capsule.
    mode: PillMode,
    /// [GRAIN] True while this session is a live-streaming one (Native ASR / rolling
    /// live preview). Only a streaming session is allowed to expand into `Studio`,
    /// and only once it has text.
    streaming: bool,
    /// [GRAIN] Prompt Record: the user clicked the pill mid-recording and is now
    /// dictating an AI instruction. Tints the recording dots / Studio waveform blue
    /// (the sole visual indicator). Set by `PromptRecordingChanged`; reset per session.
    prompt_recording: bool,
    /// [GRAIN] Quick-Agent follow-up offer: the configured shortcut label while
    /// an offer is live (`AgentFollowupOffer`), else `None`. Reveals the pill
    /// with an "ASK FOLLOW-UP" riser; a click sends `PillAction::AgentFollowup`.
    agent_offer: Option<String>,
    /// Bumped on every offer/clear so the App detects the transition.
    agent_offer_seq: u64,
    /// [GRAIN] Native agent input: `Some((selection_chars, type_to_expand,
    /// kind))` while the summon card should be on screen. `kind` picks the card
    /// variant (Assist vs the top-anchored Grain Space Capture/Recall).
    /// Overrides every other surface.
    agent_input: Option<(u32, bool, AgentInputKind)>,
    /// Bumped on every show/hide so the App detects the transition.
    agent_input_seq: u64,
    /// Bumped when the core's global Enter asks the pill to submit (the pill
    /// answers with SubmitText or SubmitVoice depending on its state).
    agent_submit_req_seq: u64,
    /// [GRAIN] Bumped when a Grain Space capture saved — the card plays a brief
    /// in-place "Saved" confirmation before the core hides it.
    agent_input_saved_seq: u64,
    /// [GRAIN] Live Studio Window transcript. Frozen the instant `state` leaves
    /// `Recording` (see `apply_event`) so the preview never changes once the
    /// user releases the shortcut, even though the worker's drain can still
    /// emit a few trailing `Asr*` events while finalizing.
    asr: AsrDisplay,
}

impl Default for Remote {
    fn default() -> Self {
        Remote {
            state: PillState::Idle,
            visible: false,
            anchor: OverlayPosition::Bottom,
            prompt_name: String::new(),
            prompt_seq: 0,
            mode: PillMode::Collapsed,
            streaming: false,
            prompt_recording: false,
            agent_offer: None,
            agent_offer_seq: 0,
            agent_input: None,
            agent_input_seq: 0,
            agent_submit_req_seq: 0,
            agent_input_saved_seq: 0,
            asr: AsrDisplay::default(),
        }
    }
}

fn apply_event(remote: &Mutex<Remote>, ev: DaemonEvent) {
    let mut r = remote.lock().unwrap();
    // When the overlay is disabled (None), no session event may reveal the pill.
    let can_show = |r: &Remote| r.anchor != OverlayPosition::None;
    match ev {
        // [GRAIN] anchor (and enable/disable) for the single pill.
        DaemonEvent::OverlayConfig { position } => {
            r.anchor = position;
            if position == OverlayPosition::None {
                r.visible = false;
            }
            eprintln!("event: OverlayConfig -> anchor {position:?}");
        }
        // Recording overrides processing: while recording the pill shows
        // recording; processing only appears after the stop signal.
        DaemonEvent::RecordingStarted { mode, .. } => {
            r.state = PillState::Recording;
            r.visible = can_show(&r);
            // [GRAIN] Every session opens as the small collapsed capsule. A live
            // STREAMING session (Native ASR / rolling live preview) is allowed to
            // expand into the Studio surface, but only once its first word lands
            // (handled after the match) — so the pill grows FROM the small pill.
            // Fresh `asr` buffer per session so prior text never bleeds in.
            r.mode = PillMode::Collapsed;
            r.streaming = mode == SessionMode::NativeAsr;
            r.prompt_recording = false; // fresh session — never carry a prior mark's tint.
            // A new session supersedes any lingering Quick-Agent follow-up offer.
            if r.agent_offer.take().is_some() {
                r.agent_offer_seq = r.agent_offer_seq.wrapping_add(1);
            }
            r.asr = AsrDisplay::default();
            eprintln!("event: RecordingStarted -> show (recording, mode {mode:?})");
        }
        DaemonEvent::RecordingStopped { .. } => {
            r.state = PillState::Processing;
            r.visible = can_show(&r);
            r.prompt_recording = false; // recording over → drop the blue tint.
            eprintln!("event: RecordingStopped -> processing");
        }
        // [GRAIN] Prompt Record: the core registered the pill-click split mark.
        // Flips the dot field / Studio waveform to the blue tint — the sole visual
        // indicator that the user is now dictating an AI instruction.
        DaemonEvent::PromptRecordingChanged { active, .. } => {
            r.prompt_recording = active && r.state == PillState::Recording;
            eprintln!("event: PromptRecordingChanged -> {}", r.prompt_recording);
        }
        DaemonEvent::ProcessingComplete { .. } | DaemonEvent::SessionCancelled { .. } => {
            r.visible = false;
            eprintln!("event: ProcessingComplete/Cancelled -> hide");
        }
        // [GRAIN] B4: surface failures in the placeholder fallback state.
        DaemonEvent::ModelError { .. } | DaemonEvent::PasteError { .. } => {
            r.state = PillState::Fallback;
            r.visible = can_show(&r);
            eprintln!("event: ModelError/PasteError -> fallback");
        }
        // [GRAIN] Prompt switcher: carry the new title + bump the sequence so the
        // App shows the riser (and briefly reveals the pill if idle).
        DaemonEvent::PromptChanged { name } => {
            r.prompt_name = name;
            r.prompt_seq = r.prompt_seq.wrapping_add(1);
            eprintln!("event: PromptChanged -> riser");
        }
        // [GRAIN] Quick Agent: reveal the pill with the "ask follow-up" riser
        // until the core withdraws the offer (panel opened / expired / new
        // session). A click on the pill in this window sends `AgentFollowup`.
        DaemonEvent::AgentFollowupOffer { shortcut } => {
            r.agent_offer = Some(shortcut);
            r.agent_offer_seq = r.agent_offer_seq.wrapping_add(1);
            eprintln!("event: AgentFollowupOffer -> reveal");
        }
        DaemonEvent::AgentFollowupClear => {
            if r.agent_offer.take().is_some() {
                r.agent_offer_seq = r.agent_offer_seq.wrapping_add(1);
            }
            eprintln!("event: AgentFollowupClear -> withdraw");
        }
        // [GRAIN] Native agent input: the summon card. Shown/hidden by the core;
        // the pill owns the typing state and answers submit requests itself.
        DaemonEvent::AgentInputShow {
            selection_chars,
            type_to_expand,
            kind,
        } => {
            r.agent_input = Some((selection_chars, type_to_expand, kind));
            r.agent_input_seq = r.agent_input_seq.wrapping_add(1);
            eprintln!(
                "event: AgentInputShow ({selection_chars} sel chars, tte {type_to_expand}, kind {kind:?})"
            );
        }
        DaemonEvent::AgentInputHide => {
            if r.agent_input.take().is_some() {
                r.agent_input_seq = r.agent_input_seq.wrapping_add(1);
            }
            eprintln!("event: AgentInputHide");
        }
        DaemonEvent::AgentInputSaved => {
            r.agent_input_saved_seq = r.agent_input_saved_seq.wrapping_add(1);
            eprintln!("event: AgentInputSaved");
        }
        DaemonEvent::AgentInputSubmitRequest => {
            r.agent_submit_req_seq = r.agent_submit_req_seq.wrapping_add(1);
            eprintln!("event: AgentInputSubmitRequest");
        }
        // [GRAIN] transcribe-cpp streaming: both parts are cumulative snapshots
        // (the full flicker-free prefix + the volatile tail), so set them
        // directly. The tail is what keeps the preview moving while the engine's
        // auto-commit sits between commit points — without it the preview
        // appears frozen (classically right after a full stop) even though
        // decoding continues. Only while still `Recording` — once the shortcut
        // is released `state` flips to `Processing` and the preview freezes
        // where it was.
        DaemonEvent::AsrStreamText {
            committed,
            tentative,
            ..
        } if r.state == PillState::Recording => {
            r.asr.committed = committed;
            r.asr.partial = tentative;
            r.asr.partial_stable = true;
        }
        // Legacy sherpa path (stabilized deltas). Kept until sherpa is deleted.
        DaemonEvent::AsrCommit { text, .. } if r.state == PillState::Recording => {
            r.asr.append_commit(&text);
        }
        DaemonEvent::AsrPartial { text, stable, .. } if r.state == PillState::Recording => {
            r.asr.partial = text;
            r.asr.partial_stable = stable;
        }
        DaemonEvent::AsrSegmentFinal { text, .. } if r.state == PillState::Recording => {
            r.asr.finished.push(text);
            r.asr.committed.clear();
            r.asr.partial.clear();
        }
        _ => {} // AudioLevel / Asr* after the freeze / etc. — not a state change
    }

    // [GRAIN] First-word expand / "scrap that" collapse. A streaming session flips
    // to the Studio surface the instant it has any transcript to show, and flips
    // BACK to the small capsule when the preview empties again — which is exactly
    // what a "scrap that" reset produces (the scrubbed committed/tentative go
    // empty). The App sees the mode change and grows/shrinks the pill smoothly.
    if r.streaming {
        let empty = r.asr.display_runs().is_empty();
        if r.mode == PillMode::Collapsed && !empty {
            r.mode = PillMode::Studio;
            eprintln!("event: first transcript word -> expand to Studio");
        } else if r.mode == PillMode::Studio && empty {
            r.mode = PillMode::Collapsed;
            eprintln!("event: scrap that -> collapse to compact capsule");
        }
    }
}

/// Connect to the core's local event WS and drive `remote` from DaemonEvents.
/// Reconnects forever — the pill is always-on; the core may come and go.
/// Sends a `UserEvent::Wake` to the winit loop on every session state change so
/// the pill surfaces without waiting for the next HIDDEN_TICK (up to 80 ms).
fn spawn_event_client(
    remote: Arc<Mutex<Remote>>,
    proxy: EventLoopProxy<UserEvent>,
    // [GRAIN] Outbound pill actions (Prompt Record clicks) from the winit thread,
    // forwarded over the same WebSocket's write half.
    mut action_rx: tokio::sync::mpsc::UnboundedReceiver<PillAction>,
) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("pill: tokio runtime failed: {e}");
                return;
            }
        };
        rt.block_on(async move {
            use futures_util::{SinkExt, StreamExt};
            use tokio_tungstenite::tungstenite::Message;
            // Try to connect initially for a few seconds
            let mut attempts = 0;
            let ws_stream = loop {
                match tokio_tungstenite::connect_async("ws://127.0.0.1:7124").await {
                    Ok((ws, _)) => {
                        eprintln!("ws: connected to ws://127.0.0.1:7124");
                        break Some(ws);
                    }
                    Err(e) => {
                        eprintln!("ws: connect failed ({e}) — retrying");
                        attempts += 1;
                        if attempts > 50 {
                            break None;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            };

            if let Some(ws) = ws_stream {
                // [GRAIN] Duplex: read DaemonEvents in, write PillActions out
                // (the reverse channel — e.g. a Prompt Record click).
                let (mut write, mut read) = ws.split();
                loop {
                    tokio::select! {
                        msg = read.next() => match msg {
                            Some(Ok(Message::Text(txt))) => {
                                if let Ok(ev) = serde_json::from_str::<DaemonEvent>(txt.as_str()) {
                                    // Wake the event loop for session-relevant events so the
                                    // pill surfaces immediately, not after HIDDEN_TICK ms.
                                    let is_session_event = matches!(
                                        ev,
                                        DaemonEvent::RecordingStarted { .. }
                                            | DaemonEvent::RecordingStopped { .. }
                                            | DaemonEvent::ProcessingComplete { .. }
                                            | DaemonEvent::SessionCancelled { .. }
                                            | DaemonEvent::ModelError { .. }
                                            | DaemonEvent::PasteError { .. }
                                            | DaemonEvent::OverlayConfig { .. }
                                            | DaemonEvent::PromptRecordingChanged { .. }
                                            // The follow-up offer arrives while the pill is
                                            // HIDDEN (no session) — without a wake the loop
                                            // sits in ControlFlow::Wait and the offer never
                                            // paints. Same for its withdrawal.
                                            | DaemonEvent::AgentFollowupOffer { .. }
                                            | DaemonEvent::AgentFollowupClear
                                            | DaemonEvent::PromptChanged { .. }
                                            | DaemonEvent::AgentInputShow { .. }
                                            | DaemonEvent::AgentInputHide
                                            | DaemonEvent::AgentInputSaved
                                            | DaemonEvent::AgentInputSubmitRequest
                                    );
                                    apply_event(&remote, ev);
                                    if is_session_event {
                                        let _ = proxy.send_event(UserEvent::Wake);
                                    }
                                }
                            }
                            Some(Ok(_)) => {} // ping/pong/binary — ignore
                            _ => break,       // closed/errored — core gone
                        },
                        action = action_rx.recv() => match action {
                            Some(a) => {
                                if let Ok(json) = serde_json::to_string(&a) {
                                    if write.send(Message::Text(json.into())).await.is_err() {
                                        break; // write failed — core gone
                                    }
                                }
                            }
                            // All senders dropped — the App (and thus the process)
                            // is going away; end the connection.
                            None => break,
                        },
                    }
                }
                eprintln!("ws: disconnected — core app died, exiting.");
            } else {
                eprintln!("ws: failed to connect to core app after retries, exiting.");
            }
            std::process::exit(0);
        });
    });
}

// ── Agent input surface state ───────────────────────────────────────────────

/// [GRAIN] The native agent summon card (per the reference design): records by
/// default (dot-matrix wave + "Listening..."), expands into a 520px typing card
/// on the first printable keystroke. Owned entirely by the pill; the core only
/// shows/hides it and asks it to submit.
struct AgentInputUi {
    selection_chars: u32,
    /// Mirrors the setting: when false, printable keystrokes while listening are
    /// ignored (the user must Tab / click to reach the typing card).
    type_to_expand: bool,
    /// [GRAIN] Which brain this card serves — drives anchor + labels/placeholder
    /// ("Noting…"/"Save Note" for Capture, "Listening…"/"Confirm" otherwise).
    kind: AgentInputKind,
    /// Typing (expanded) vs recording (compact).
    expanded: bool,
    /// Eased 0..1 expansion progress (drives width/height/content cross-fade).
    expand_t: f32,
    text: String,
    /// Free-running clock: wave animation + caret blink.
    phase: f32,
    /// [GRAIN] Grain Space capture confirmation: once set, the card paints a
    /// green "Saved" state (in place, no new surface) until the core hides it.
    saved: bool,
    /// Confirm-button hit rect from the last rendered frame (x0, y0, x1, y1).
    confirm_rect: (f32, f32, f32, f32),
    /// Card hit rect from the last rendered frame.
    card_rect: (f32, f32, f32, f32),
    hover_confirm: bool,
}

impl AgentInputUi {
    fn new(selection_chars: u32, type_to_expand: bool, kind: AgentInputKind) -> Self {
        AgentInputUi {
            selection_chars,
            type_to_expand,
            kind,
            expanded: false,
            expand_t: 0.0,
            text: String::new(),
            phase: 0.0,
            saved: false,
            confirm_rect: (0.0, 0.0, 0.0, 0.0),
            card_rect: (0.0, 0.0, 0.0, 0.0),
            hover_confirm: false,
        }
    }

    /// True for the Grain Space memory surfaces (top-anchored variants).
    fn is_grain_space(&self) -> bool {
        matches!(self.kind, AgentInputKind::Capture | AgentInputKind::Recall)
    }
}

// ── App ─────────────────────────────────────────────────────────────────────

struct App {
    window: Option<Rc<Window>>,
    aura: Aura,
    state: PillState,
    /// [GRAIN] Prompt Record active for this session (mirrors `Remote`). Drives the
    /// collapsed pill's blue dot tint and the Studio waveform's sky-blue tint — the
    /// sole visual indicator of Prompt Record.
    prompt_recording: bool,
    amp: Arc<AtomicU32>,
    _mic: Option<cpal::Stream>,
    sim_target: f32,
    sim_amp: f32,
    font: Option<fontdue::Font>,
    prompts: Vec<String>,
    prompt_idx: usize,
    // [GRAIN] WS-driven riser label + change detection + transient idle reveal.
    prompt_label: String,
    cached_label: Option<CachedText>,
    last_prompt_seq: u64,
    prompt_preview_until: Option<Instant>,
    // [GRAIN] Quick-Agent follow-up offer (mirrors `Remote::agent_offer`): while
    // set, the pill stays revealed with an "ASK FOLLOW-UP · <shortcut>" riser
    // and a click sends `PillAction::AgentFollowup` instead of Prompt Record.
    agent_offer: Option<String>,
    last_agent_offer_seq: u64,
    /// True when the pill's next hide is an offer withdrawal — those fade out
    /// smoothly (reusing `studio_alpha`) instead of vanishing like session ends.
    offer_fade_close: bool,
    // [GRAIN] Native agent input (mirrors `Remote::agent_input`). While `Some`,
    // the window presents the summon card and accepts keyboard focus.
    agent_input: Option<AgentInputUi>,
    last_agent_input_seq: u64,
    last_agent_submit_req_seq: u64,
    last_agent_input_saved_seq: u64,
    /// Last cursor position (physical px) for card/button hit-testing.
    cursor_pos: (f32, f32),
    /// Live keyboard modifiers (Ctrl+Backspace word delete, Ctrl+V paste).
    ctrl_down: bool,
    /// [GRAIN] Broad-coverage fallback face for glyphs the bundled subset lacks
    /// (Cyrillic/Greek/CJK/…). Lazily loaded only when non-Latin text needs
    /// drawing, and dropped on idle. `fallback_tried` avoids re-probing a
    /// missing system font every frame.
    fallback_font: Option<fontdue::Font>,
    fallback_tried: bool,
    /// [GRAIN] When the pill has been continuously hidden this long, the fonts +
    /// pixmap are freed so the always-on process idles near its floor (they
    /// reload lazily on the next show). `None` = nothing scheduled.
    free_idle_at: Option<Instant>,
    riser_progress: f32,
    riser_hide_at: Option<Instant>,
    /// [GRAIN] The prompt-SWITCHER capsule's rect (physical px) as last drawn, or
    /// `None` when it isn't showing (or it's the clickable agent follow-up offer).
    /// A click inside it must NOT be read as a pill action (Prompt Record).
    prompt_switch_rect: Option<(f32, f32, f32, f32)>,
    next_tick: Instant,
    next_roll: Instant,
    remote: Arc<Mutex<Remote>>,
    /// [GRAIN] Reverse channel to the core: a pill click (Prompt Record) is sent as
    /// a `PillAction` over the same WebSocket. The winit thread hands the action to
    /// the WS task through this unbounded sender.
    action_tx: tokio::sync::mpsc::UnboundedSender<PillAction>,
    visible: bool,
    presenter: Option<present::Presenter>,
    pixmap: Option<Pixmap>,
    // [GRAIN] Native ASR Studio Window: current surface, its accumulated
    // transcript, and the fade in/out that lets it disappear smoothly instead
    // of vanishing (the collapsed capsule still hides instantly, unchanged).
    mode: PillMode,
    asr: AsrDisplay,
    studio_alpha: f32,
    closing: bool,
    /// Free-running clock for the Studio control row's animation (waveform
    /// ripple, dot pulse, spinner) — advances one step per rendered frame,
    /// independent of the dot-field roll cadence so the motion stays smooth.
    studio_phase: f32,
    /// [GRAIN] Eased Studio card height (px). Grows from a 1-line card toward the
    /// 4-line cap as the transcript wraps onto more lines; `0.0` means "unset —
    /// snap to the target on the next frame" (start of each session).
    studio_grown_h: f32,
    /// [GRAIN] Eased Studio card WIDTH grow, 0..1 (collapsed pill width → full
    /// card). Reset to 0 at each new session so the streaming pill visibly expands
    /// from the small capsule; eases to 1 over ~200ms.
    studio_expand: f32,
    /// [GRAIN] First-seen time per transcript word (global order) — drives the
    /// per-word fade-in. Grows as words are decoded; cleared each new session.
    reveal_since: Vec<Instant>,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let amp = Arc::new(AtomicU32::new(0));
        // [GRAIN] The mic is NOT opened at startup. The pill is always-on but
        // hidden between dictations; holding a capture stream open 24/7 wastes RAM,
        // wakes the audio callback for nothing, and keeps the mic device busy (OS
        // "mic in use" indicator). We open it only while the pill is visible and
        // close it on hide — see `about_to_wait`.
        let remote = Arc::new(Mutex::new(Remote::default()));
        // [GRAIN] Reverse channel for pill clicks (Prompt Record). Unbounded so the
        // winit thread never blocks handing an action to the async WS task.
        let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<PillAction>();
        spawn_event_client(remote.clone(), proxy, action_rx);
        App {
            window: None,
            aura: Aura::new(),
            state: PillState::Idle,
            prompt_recording: false,
            amp,
            _mic: None,
            sim_target: 0.5,
            sim_amp: 0.0,
            // [GRAIN] The single shared font loads LAZILY (on the first text
            // render) and is freed after a long idle — so the always-on pill
            // doesn't hold a parsed font while it sits hidden.
            font: None,
            prompts: ["General", "Email Format", "Meeting Notes", "Translation"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            prompt_idx: 0,
            prompt_label: String::new(),
            cached_label: None,
            last_prompt_seq: 0,
            prompt_preview_until: None,
            agent_offer: None,
            last_agent_offer_seq: 0,
            offer_fade_close: false,
            agent_input: None,
            last_agent_input_seq: 0,
            last_agent_submit_req_seq: 0,
            last_agent_input_saved_seq: 0,
            cursor_pos: (0.0, 0.0),
            ctrl_down: false,
            fallback_font: None,
            fallback_tried: false,
            free_idle_at: None,
            riser_progress: 0.0,
            riser_hide_at: None,
            prompt_switch_rect: None,
            next_tick: Instant::now(),
            next_roll: Instant::now(),
            remote,
            action_tx,
            visible: false,
            presenter: None,
            pixmap: None,
            mode: PillMode::Collapsed,
            asr: AsrDisplay::default(),
            studio_alpha: 0.0,
            closing: false,
            studio_phase: 0.0,
            studio_grown_h: 0.0,
            studio_expand: 0.0,
            reveal_since: Vec::new(),
        }
    }

    /// The COLLAPSED pill's own footprint width (px) — the dot field + its cell
    /// insets. The OS window is wider (see `win_size`) to make room for the
    /// sibling prompt capsule, but the pill body + horizontal centering are keyed
    /// to this so the pill never moves when the sibling appears.
    fn collapsed_core_w() -> f32 {
        COLS as f32 * CELL * SCALE
    }

    fn win_size() -> (u32, u32) {
        let cell = CELL * SCALE;
        // Extra width to the RIGHT for the sibling prompt capsule (its widest
        // form — the agent follow-up offer) plus a matching right inset. Kept
        // transparent until a switch, so it is click-through at rest.
        let extra = SIB_GAP + SIB_MAX_W + cell;
        (
            (Self::collapsed_core_w() + extra).round() as u32,
            ((ROWS as f32 + RISER_RESERVE) * CELL * SCALE).round() as u32,
        )
    }

    /// Window size for the given surface — the classic collapsed capsule or
    /// the Native ASR Studio Window. Switching between them resizes the single
    /// OS window (see `about_to_wait`'s mode-change handling); both are fixed
    /// sizes, so the Studio Window scrolls its transcript rather than growing.
    fn win_size_for(mode: PillMode) -> (u32, u32) {
        match mode {
            PillMode::Collapsed => Self::win_size(),
            PillMode::Studio => studio_pixel_size(),
            PillMode::AgentInput => (AIN_WIN_W, AIN_WIN_H),
        }
    }

    /// Width to horizontally center the OS window on (see `position_window`). The
    /// collapsed window is wider than the pill for the sibling capsule, so it is
    /// centered on the pill's own footprint; the others center on the full window.
    fn center_w_for(mode: PillMode, w: u32) -> f32 {
        match mode {
            PillMode::Collapsed => Self::collapsed_core_w(),
            _ => w as f32,
        }
    }

    /// [GRAIN] Place the pill on the monitor under it (or primary) per the user's
    /// `overlay_position`: centered horizontally, near the top or bottom edge.
    /// Recomputed on each show so it follows the active monitor + setting changes.
    fn position_window(window: &Window, anchor: OverlayPosition, h: u32, center_w: f32) {
        let Some(mon) = window
            .current_monitor()
            .or_else(|| window.primary_monitor())
        else {
            return;
        };
        let ms = mon.size();
        let mp = mon.position();
        let margin = (16.0 * SCALE) as i32;
        // [GRAIN] Horizontally center the CONTENT (`center_w`), not the full window.
        // The collapsed window is wider than the pill (transparent reserve to the
        // right for the sibling capsule); centering on the pill's own width keeps
        // it screen-centered while the reserve simply extends off to the right.
        let x = mp.x + ((ms.width as f32 - center_w) / 2.0).round() as i32;
        // Full-monitor bottom edge (fallback when the work area is unavailable).
        let screen_bottom = mp.y + ms.height as i32;
        let y = match anchor {
            OverlayPosition::Top => mp.y + margin,
            // Bottom is the default; None never reaches here (pill stays hidden).
            // [GRAIN] Anchor to the monitor's WORK AREA bottom (above the taskbar),
            // not the full screen height — otherwise the pill renders behind the
            // taskbar. The work area already excludes the taskbar, so this lifts the
            // pill clear of it (≈ a taskbar height up) on any taskbar size/edge.
            OverlayPosition::Bottom | OverlayPosition::None => {
                let bottom =
                    work_area_bottom(mp.x + (ms.width / 2) as i32, mp.y + (ms.height / 2) as i32)
                        .unwrap_or(screen_bottom);
                bottom - h as i32 - margin
            }
            // [GRAIN] Vertically centered on the monitor — the Studio Window's
            // natural home (a tall content box reads poorly hugging an edge).
            OverlayPosition::Center => mp.y + ((ms.height.saturating_sub(h)) / 2) as i32,
        };
        window.set_outer_position(PhysicalPosition::new(x, y));
    }

    /// [GRAIN] Place the agent input canvas: horizontally centered; anchored to
    /// the BOTTOM work-area edge by default (the card expands upward inside the
    /// canvas), or to the TOP edge when the user's overlay anchor is Top (the
    /// card then expands downward).
    fn position_agent_input(window: &Window, anchor: OverlayPosition) {
        let Some(mon) = window
            .current_monitor()
            .or_else(|| window.primary_monitor())
        else {
            return;
        };
        let ms = mon.size();
        let mp = mon.position();
        let (w, h) = (AIN_WIN_W, AIN_WIN_H);
        let x = mp.x + ((ms.width.saturating_sub(w)) / 2) as i32;
        let (cx, cy) = (mp.x + (ms.width / 2) as i32, mp.y + (ms.height / 2) as i32);
        let y = match anchor {
            OverlayPosition::Top => work_area_top(cx, cy).unwrap_or(mp.y) + AIN_EDGE_MARGIN,
            _ => {
                let bottom = work_area_bottom(cx, cy).unwrap_or(mp.y + ms.height as i32);
                bottom - h as i32 - AIN_EDGE_MARGIN
            }
        };
        window.set_outer_position(PhysicalPosition::new(x, y));
    }

    /// The EFFECTIVE anchor for the current agent-input card: the Grain Space
    /// memory kinds (Capture/Recall) always hug the TOP (the prototype's
    /// placement), while Assist follows the user's overlay setting (`base`).
    fn agent_input_anchor(&self, base: OverlayPosition) -> OverlayPosition {
        match self.agent_input.as_ref() {
            Some(ui) if ui.is_grain_space() => OverlayPosition::Top,
            _ => base,
        }
    }

    /// True when the agent input card should hug the TOP of its canvas (top
    /// anchor → expands downward). Mirrors `position_agent_input`.
    fn agent_input_anchored_top(&self) -> bool {
        let base = self.remote.lock().unwrap().anchor;
        self.agent_input_anchor(base) == OverlayPosition::Top
    }

    /// [GRAIN] Keystroke routing for the agent input card (the window has real
    /// focus while the input is up). Enter/Escape are fallbacks — they normally
    /// arrive via the core's transient global shortcuts instead.
    fn agent_input_key(&mut self, ev: KeyEvent) {
        let Some(ui) = &mut self.agent_input else {
            return;
        };
        match ev.logical_key.as_ref() {
            Key::Named(NamedKey::Escape) => {
                let _ = self.action_tx.send(PillAction::AgentInputCancel);
            }
            Key::Named(NamedKey::Enter) => {
                let text = ui.text.trim().to_string();
                let action = if ui.expanded && !text.is_empty() {
                    PillAction::AgentInputSubmitText { text }
                } else {
                    PillAction::AgentInputSubmitVoice
                };
                let _ = self.action_tx.send(action);
            }
            // Tab shrinks back to the recording state (the reference behavior);
            // the core restarts dictation on `active: false`.
            Key::Named(NamedKey::Tab) => {
                if ui.expanded {
                    ui.expanded = false;
                    ui.text.clear();
                    let _ = self
                        .action_tx
                        .send(PillAction::AgentInputTyping { active: false });
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if ui.expanded {
                    if self.ctrl_down {
                        // Ctrl+Backspace: drop the trailing word.
                        let trimmed = ui.text.trim_end();
                        let cut = trimmed
                            .rfind(char::is_whitespace)
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        ui.text.truncate(cut);
                    } else {
                        ui.text.pop();
                    }
                }
            }
            Key::Character("v") | Key::Character("V") if self.ctrl_down => {
                if let Some(window) = &self.window {
                    if let Some(pasted) = present::read_clipboard_text(window) {
                        let clean: String =
                            pasted.chars().filter(|c| !c.is_control()).collect();
                        if !clean.is_empty() {
                            if !ui.expanded {
                                ui.expanded = true;
                                let _ = self
                                    .action_tx
                                    .send(PillAction::AgentInputTyping { active: true });
                            }
                            ui.text.push_str(&clean);
                            // Char-safe cap (byte-index truncate can split UTF-8).
                            if ui.text.chars().count() > 2000 {
                                ui.text = ui.text.chars().take(2000).collect();
                            }
                        }
                    }
                }
            }
            _ => {
                // Ordinary text entry. The first printable character expands the
                // card (typing overrides the voice capture, per the reference) —
                // but ONLY when type-to-expand is on. When off, typing while
                // listening is ignored until the user expands via Tab / click.
                if self.ctrl_down {
                    return; // other Ctrl chords are not text
                }
                if !ui.expanded && !ui.type_to_expand {
                    return;
                }
                let Some(t) = ev.text.as_ref() else { return };
                let printable: String = t.chars().filter(|c| !c.is_control()).collect();
                if printable.is_empty() {
                    return;
                }
                if !ui.expanded {
                    ui.expanded = true;
                    let _ = self
                        .action_tx
                        .send(PillAction::AgentInputTyping { active: true });
                }
                if ui.text.chars().count() < 2000 {
                    ui.text.push_str(&printable);
                }
            }
        }
    }

    fn update_cached_label(&mut self) {
        // Runs (from `about_to_wait`) BEFORE the frame's `render`, so make sure
        // the shared font is resident — it may have been dropped by the idle-free.
        self.ensure_font();
        // A non-Latin prompt name (e.g. a Cyrillic custom prompt) needs the
        // fallback face; the all-Latin case never loads it.
        let label = self.prompt_label.clone();
        self.ensure_fallback_for(&label);
        let label_max = self.prompt_label_max();
        if let Some(font) = self.font.as_ref() {
            let font = font_for(font, self.fallback_font.as_ref(), &label);
            let truncated = truncate_to_width(font, &label, PROMPT_LABEL_PX, label_max);
            self.cached_label = Some(CachedText::new(font, &truncated, PROMPT_LABEL_PX));
        }
    }

    /// The max prompt-label width (px) for the capsule on the CURRENT surface, so
    /// the label ellipsis-truncates to fit between the arrows (or, for the agent
    /// follow-up offer, within the arrow-less capsule).
    fn prompt_label_max(&self) -> f32 {
        match self.mode {
            // Studio top capsule: full card width, arrows at a fixed inset.
            PillMode::Studio => {
                (STUDIO_W - 2.0 * STUDIO_TOP_ARROW_INSET - 2.0 * SIB_TEXT_PAD).max(0.0)
            }
            // Collapsed sibling: the arrow-less offer fills its max width; the
            // switcher fits between its arrows in the fixed-width capsule.
            _ if self.agent_offer.is_some() => (SIB_MAX_W - 4.0 * SIB_TEXT_PAD).max(0.0),
            _ => (SIB_W - 2.0 * SIB_ARROW_INSET - 2.0 * SIB_TEXT_PAD).max(0.0),
        }
    }

    fn cell_px() -> f32 {
        CELL * SCALE
    }

    /// Vertical offset of the pill body (the riser reserve sits above it).
    fn y_offset() -> f32 {
        RISER_RESERVE * CELL * SCALE
    }

    fn current_amp(&mut self) -> f32 {
        if self._mic.is_some() {
            f32::from_bits(self.amp.load(Ordering::Relaxed))
        } else {
            if self.aura.rng.f32() < 0.15 {
                self.sim_target = 0.15 + self.aura.rng.f32() * 0.8;
            }
            self.sim_amp += (self.sim_target - self.sim_amp) * 0.35;
            self.sim_amp
        }
    }

    /// Dispatch to whichever surface is current. The two are independent,
    /// fixed-size renderers (see `win_size_for`) — never mixed in one frame.
    fn render(&mut self) {
        // Every surface draws text (riser label / transcript / card), so make
        // sure the single shared font is resident before we render a frame. It
        // loads once here (lazy) and is dropped again after a long idle.
        self.ensure_font();
        match self.mode {
            PillMode::Collapsed => self.render_collapsed(),
            PillMode::Studio => self.render_studio(),
            PillMode::AgentInput => self.render_agent_input(),
        }
    }

    /// Lazily load the bundled primary font (idempotent). Called before each
    /// render; paired with the idle-free in `about_to_wait`.
    fn ensure_font(&mut self) {
        if self.font.is_none() {
            self.font = load_font();
        }
    }

    /// Ensure the broad-coverage fallback is loaded IF `text` has a char the
    /// primary can't draw (else a no-op — the all-Latin path never loads it).
    /// `fallback_tried` means "don't keep re-probing a system font we couldn't
    /// find"; it resets on idle-free so a later show can retry.
    fn ensure_fallback_for(&mut self, text: &str) {
        if self.fallback_font.is_some() || self.fallback_tried {
            return;
        }
        let Some(primary) = self.font.as_ref() else {
            return;
        };
        if primary_missing_glyph(primary, text) {
            self.fallback_tried = true;
            self.fallback_font = load_fallback_font();
        }
    }

    /// [GRAIN] The native agent summon card, pixel-matched to the reference:
    /// COMPACT — a content-hugging dark capsule with the 12×4 white→orange
    /// gradient wave (right-to-left "quantum audio stream") and "Listening...";
    /// EXPANDED (first printable keystroke) — a 520px card with the GRAIN
    /// header + selection chip, an 18px input with an orange caret, and a
    /// footer: "Tab to record" · "Esc to close" · a white "Confirm ↵" button.
    /// The card is drawn INSIDE the fixed canvas (anchored to the work-area
    /// edge) so expansion never resizes the OS window.
    fn render_agent_input(&mut self) {
        if self.window.is_none() {
            return;
        }
        // The user can type a non-Latin instruction; make sure the fallback is
        // ready before drawing it (no-op for Latin — the common case).
        let typed_probe = self
            .agent_input
            .as_ref()
            .map(|u| u.text.clone())
            .unwrap_or_default();
        self.ensure_fallback_for(&typed_probe);

        let (w, h) = (AIN_WIN_W, AIN_WIN_H);
        let mut pixmap = self
            .pixmap
            .take()
            .unwrap_or_else(|| Pixmap::new(w, h).unwrap());
        pixmap.fill(Color::TRANSPARENT);

        let anchored_top = self.agent_input_anchored_top();
        let fallback_ref = self.fallback_font.as_ref();
        let Some(ui) = &mut self.agent_input else {
            self.pixmap = Some(pixmap);
            return;
        };
        let t = ui.expand_t.clamp(0.0, 1.0);
        let phase = ui.phase;

        // [GRAIN] Card variant. Capture (Grain Space note) relabels the surface —
        // "Noting…"/"Write down your thoughts…"/"Save Note" — while Recall and
        // Assist keep "Listening…"/"Ask anything…"/"Confirm". `saved` flips the
        // whole card to the green in-place confirmation (Capture only). Same
        // window/pixmap — purely string + colour differences, zero extra RAM.
        let capture = matches!(ui.kind, AgentInputKind::Capture);
        let saved = ui.saved;
        let cue = if capture { "Noting..." } else { "Listening..." };
        let placeholder = if capture {
            "Write down your thoughts..."
        } else {
            "Ask anything..."
        };
        let btn_label = if capture { "Save Note" } else { "Confirm" };

        // ── Card geometry (width/height lerp between the two states) ──────────
        // One shared font for the whole card's FIXED strings (the reference's
        // semibold header / button render in the same face — a separate bold
        // face would be another ~15-20 MB fontdue parse for no real gain). Only
        // the user's TYPED text may need the fallback face for non-Latin input.
        let ui_font = self.font.as_ref();
        let sb_font = ui_font;
        let text_font = ui_font.map(|f| font_for(f, fallback_ref, &ui.text));

        // Compact width hugs its content: pad + 2 + wave + 14 + label + pad.
        let wave_w = AIN_WAVE_COLS as f32 * AIN_WAVE_DOT + (AIN_WAVE_COLS - 1) as f32 * AIN_WAVE_GAP;
        let wave_h = AIN_WAVE_ROWS as f32 * AIN_WAVE_DOT + (AIN_WAVE_ROWS - 1) as f32 * AIN_WAVE_GAP;
        let listen_w = ui_font.map(|f| text_width(f, cue, 11.5)).unwrap_or(64.0);
        let compact_w = AIN_PAD_X + 2.0 + wave_w + 14.0 + listen_w + AIN_PAD_X;
        let compact_h = AIN_PAD_Y_COMPACT * 2.0 + wave_h.max(16.0) + 2.0;

        let card_w = compact_w + (AIN_EXPANDED_W - compact_w) * t;
        let card_h = compact_h + (AIN_EXPANDED_H - compact_h) * t;
        let card_x = (w as f32 - card_w) / 2.0;
        // Anchored to the canvas edge nearest the screen edge, so the card
        // grows AWAY from it (upward at the bottom, downward at the top).
        let card_y = if anchored_top {
            1.0
        } else {
            h as f32 - card_h - 1.0
        };
        ui.card_rect = (card_x, card_y, card_x + card_w, card_y + card_h);

        // ── Card body: #1a1a1a fill, #333 border, r16, faint layered shadow ──
        let rounded = |x: f32, y: f32, ww: f32, hh: f32, r: f32| -> Option<tiny_skia::Path> {
            let mut pb = PathBuilder::new();
            let r = r.min(ww / 2.0).min(hh / 2.0);
            pb.move_to(x + r, y);
            pb.line_to(x + ww - r, y);
            pb.quad_to(x + ww, y, x + ww, y + r);
            pb.line_to(x + ww, y + hh - r);
            pb.quad_to(x + ww, y + hh, x + ww - r, y + hh);
            pb.line_to(x + r, y + hh);
            pb.quad_to(x, y + hh, x, y + hh - r);
            pb.line_to(x, y + r);
            pb.quad_to(x, y, x + r, y);
            pb.close();
            pb.finish()
        };
        let mut paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        // Faint expanding shadow layers (approximates the reference's soft drop).
        for (grow, alpha) in [(3.0_f32, 26_u8), (6.0, 14), (9.0, 7)] {
            paint.set_color(Color::from_rgba8(0, 0, 0, alpha));
            if let Some(p) = rounded(
                card_x - grow,
                card_y - grow * 0.4,
                card_w + grow * 2.0,
                card_h + grow * 1.4,
                AIN_RADIUS + grow,
            ) {
                pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
            }
        }
        paint.set_color(Color::from_rgba8(0x33, 0x33, 0x33, 255)); // border
        if let Some(p) = rounded(card_x, card_y, card_w, card_h, AIN_RADIUS) {
            pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
        paint.set_color(Color::from_rgba8(0x1a, 0x1a, 0x1a, 255)); // surface
        if let Some(p) = rounded(
            card_x + 1.0,
            card_y + 1.0,
            card_w - 2.0,
            card_h - 2.0,
            AIN_RADIUS - 1.0,
        ) {
            pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }

        // ── Saved confirmation (Grain Space capture) ──────────────────────────
        // A green dot + "Saved", centered — the SAME card confirms the headless
        // save in place (no new surface). Held briefly by the core, then hidden.
        if saved {
            let green = [0x10u8, 0xb9, 0x81];
            let label = "Saved";
            let label_w = ui_font.map(|f| text_width(f, label, 14.0)).unwrap_or(40.0);
            let dot_r = 4.0;
            let gap = 9.0;
            let total = dot_r * 2.0 + gap + label_w;
            let sx = card_x + (card_w - total) / 2.0;
            let cy = card_y + card_h / 2.0;
            paint.set_color(Color::from_rgba8(green[0], green[1], green[2], 255));
            if let Some(circ) = PathBuilder::from_circle(sx + dot_r, cy, dot_r) {
                pixmap.fill_path(&circ, &paint, FillRule::Winding, Transform::identity(), None);
            }
            if let Some(f) = ui_font {
                draw_text_left(
                    &mut pixmap,
                    f,
                    label,
                    sx + dot_r * 2.0 + gap,
                    cy,
                    14.0,
                    green,
                    1.0,
                );
            }
            ui.confirm_rect = (0.0, 0.0, 0.0, 0.0);
            if let Some(presenter) = &self.presenter {
                presenter.blit(&pixmap);
            }
            self.pixmap = Some(pixmap);
            return;
        }

        // Content cross-fade: recording fades out quickly as the card expands,
        // typing fades in on the back half (mirrors the reference's fadeIn).
        let rec_alpha = (1.0 - t * 2.2).clamp(0.0, 1.0);
        let typ_alpha = ((t - 0.45) / 0.55).clamp(0.0, 1.0);

        // ── Recording state (compact) ─────────────────────────────────────────
        if rec_alpha > 0.01 {
            let cy = card_y + card_h / 2.0;
            let wx = card_x + AIN_PAD_X + 2.0;
            let wy = cy - wave_h / 2.0;
            for i in 0..(AIN_WAVE_ROWS * AIN_WAVE_COLS) {
                let r = i / AIN_WAVE_COLS;
                let c = i % AIN_WAVE_COLS;
                let corner =
                    (r == 0 || r == AIN_WAVE_ROWS - 1) && (c == 0 || c == AIN_WAVE_COLS - 1);
                if corner {
                    continue;
                }
                // White→orange gradient, left to right (the reference's dot color).
                let ratio = c as f32 / (AIN_WAVE_COLS - 1) as f32;
                let (rr, gg, bb) = (255.0, 255.0 - 170.0 * ratio, 255.0 - 255.0 * ratio);
                // "Quantum audio stream": three additive waves travelling left.
                let (cf, rf) = (c as f32, r as f32);
                let w1 = (cf * 0.6 + phase * 6.0 + rf * 0.5).sin();
                let w2 = (cf * 0.3 + phase * 3.0 - rf * 1.2).sin();
                let w3 = (cf * 0.8 + phase * 4.5 + rf * 2.0).cos();
                let combined = w1 * 0.5 + w2 * w1 * 0.5 + w3 * 0.3;
                let normalized = (combined + 1.3) / 2.6;
                let opacity = (0.05 + normalized * 0.95).clamp(0.05, 1.0) * rec_alpha;
                let dx = wx + c as f32 * (AIN_WAVE_DOT + AIN_WAVE_GAP) + AIN_WAVE_DOT / 2.0;
                let dy = wy + r as f32 * (AIN_WAVE_DOT + AIN_WAVE_GAP) + AIN_WAVE_DOT / 2.0;
                if let Some(circle) = PathBuilder::from_circle(dx, dy, AIN_WAVE_DOT / 2.0) {
                    paint.set_color(Color::from_rgba8(
                        rr as u8,
                        gg as u8,
                        bb as u8,
                        (opacity * 255.0) as u8,
                    ));
                    pixmap.fill_path(&circle, &paint, FillRule::Winding, Transform::identity(), None);
                }
            }
            if let Some(f) = ui_font {
                draw_text_left(
                    &mut pixmap,
                    f,
                    cue,
                    wx + wave_w + 14.0,
                    cy,
                    11.5,
                    [0x8a, 0x8a, 0x8a],
                    rec_alpha,
                );
            }
        }

        // ── Typing state (expanded) ───────────────────────────────────────────
        let mut confirm_rect = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        if typ_alpha > 0.01 {
            let inner_x = card_x + AIN_PAD_X + 8.0; // content inset (reference keeps text off the rounding)
            let inner_w = card_w - (AIN_PAD_X + 8.0) * 2.0;
            let head_cy = card_y + AIN_PAD_Y_EXPANDED + 9.0;
            let input_cy = head_cy + 9.0 + 16.0 + 12.0;
            let foot_cy = input_cy + 12.0 + 16.0 + 15.0;

            // Header: GRAIN (semibold, letter-spaced) + the selection chip.
            if let Some(f) = sb_font {
                let mut x = inner_x;
                for ch in "GRAIN".chars() {
                    let s = ch.to_string();
                    x += draw_text_left(
                        &mut pixmap,
                        f,
                        &s,
                        x,
                        head_cy,
                        13.0,
                        [0xa0, 0xa0, 0xa0],
                        typ_alpha,
                    ) + 0.5;
                }
            }
            // Selection chip (top-right). Shown ONLY when there is actually a
            // selection; an empty state shows nothing (Recall never selects;
            // Capture/Assist with nothing highlighted stay clean).
            if let (Some(f), true) = (ui_font, ui.selection_chars > 0) {
                let chip_text = format!("{} chars", ui.selection_chars);
                let tw = text_width(f, &chip_text, 11.0);
                let chip_w = tw + 20.0;
                let chip_h = 21.0;
                let chip_x = inner_x + inner_w - chip_w;
                let chip_y = head_cy - chip_h / 2.0;
                paint.set_color(Color::from_rgba8(
                    0x33,
                    0x33,
                    0x33,
                    (typ_alpha * 255.0) as u8,
                ));
                if let Some(p) = rounded(chip_x, chip_y, chip_w, chip_h, 6.0) {
                    pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
                }
                paint.set_color(Color::from_rgba8(
                    0x2a,
                    0x2a,
                    0x2a,
                    (typ_alpha * 255.0) as u8,
                ));
                if let Some(p) = rounded(chip_x + 1.0, chip_y + 1.0, chip_w - 2.0, chip_h - 2.0, 5.0)
                {
                    pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
                }
                draw_text_left(
                    &mut pixmap,
                    f,
                    &chip_text,
                    chip_x + 10.0,
                    head_cy,
                    11.0,
                    [0xa0, 0xa0, 0xa0],
                    typ_alpha,
                );
            }

            // Input line: typed text (18px white) or the placeholder; caret.
            if let Some(f) = ui_font {
                let max_text_w = inner_w - 6.0;
                let caret_on = (phase % 1.0) < 0.6;
                if ui.text.is_empty() {
                    draw_text_left(
                        &mut pixmap,
                        f,
                        placeholder,
                        inner_x,
                        input_cy,
                        18.0,
                        [0x66, 0x66, 0x66],
                        typ_alpha,
                    );
                    if caret_on {
                        paint.set_color(Color::from_rgba8(
                            0xff,
                            0x55,
                            0x00,
                            (typ_alpha * 255.0) as u8,
                        ));
                        if let Some(rect) = Rect::from_xywh(inner_x - 1.0, input_cy - 10.0, 1.6, 20.0)
                        {
                            pixmap.fill_path(
                                &PathBuilder::from_rect(rect),
                                &paint,
                                FillRule::Winding,
                                Transform::identity(),
                                None,
                            );
                        }
                    }
                } else {
                    // Typed text may be non-Latin → use the fallback-aware face.
                    let f = text_font.unwrap_or(f);
                    // Show the TAIL when the text overflows (caret always visible).
                    let mut shown: &str = &ui.text;
                    while text_width(f, shown, 18.0) > max_text_w && !shown.is_empty() {
                        let mut it = shown.char_indices();
                        it.next();
                        shown = &shown[it.next().map(|(i, _)| i).unwrap_or(shown.len())..];
                    }
                    let tw = draw_text_left(
                        &mut pixmap,
                        f,
                        shown,
                        inner_x,
                        input_cy,
                        18.0,
                        [0xff, 0xff, 0xff],
                        typ_alpha,
                    );
                    if caret_on {
                        paint.set_color(Color::from_rgba8(
                            0xff,
                            0x55,
                            0x00,
                            (typ_alpha * 255.0) as u8,
                        ));
                        if let Some(rect) =
                            Rect::from_xywh(inner_x + tw + 1.0, input_cy - 10.0, 1.6, 20.0)
                        {
                            pixmap.fill_path(
                                &PathBuilder::from_rect(rect),
                                &paint,
                                FillRule::Winding,
                                Transform::identity(),
                                None,
                            );
                        }
                    }
                }
            }

            // Footer: "Tab to record" · "Esc to close" + the Confirm button.
            if let Some(f) = ui_font {
                draw_text_left(
                    &mut pixmap,
                    f,
                    "Tab to record",
                    inner_x,
                    foot_cy,
                    12.0,
                    [0x66, 0x66, 0x66],
                    typ_alpha,
                );
            }
            if let (Some(f), Some(fb)) = (ui_font, sb_font) {
                // Label ("Confirm" / "Save Note") + a hand-drawn return arrow
                // (the subset font has no U+21B5, and drawing it keeps the glyph
                // crisp and font-agnostic).
                let btn_tw = text_width(fb, btn_label, 13.0);
                let arrow_w = 11.0;
                let btn_w = btn_tw + arrow_w + 8.0 + 28.0;
                let btn_h = 29.0;
                let btn_x = inner_x + inner_w - btn_w;
                let btn_y = foot_cy - btn_h / 2.0;
                confirm_rect = (btn_x, btn_y, btn_x + btn_w, btn_y + btn_h);
                let bg = if ui.hover_confirm { 0xf0 } else { 0xff };
                paint.set_color(Color::from_rgba8(bg, bg, bg, (typ_alpha * 255.0) as u8));
                if let Some(p) = rounded(btn_x, btn_y, btn_w, btn_h, 8.0) {
                    pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
                }
                draw_text_left(
                    &mut pixmap,
                    fb,
                    btn_label,
                    btn_x + 14.0,
                    foot_cy,
                    13.0,
                    [0x00, 0x00, 0x00],
                    typ_alpha,
                );
                draw_return_arrow(
                    &mut pixmap,
                    btn_x + 14.0 + btn_tw + 8.0,
                    foot_cy,
                    arrow_w,
                    [0x00, 0x00, 0x00],
                    typ_alpha,
                );

                let esc = "Esc to close";
                let esc_w = text_width(f, esc, 12.0);
                draw_text_left(
                    &mut pixmap,
                    f,
                    esc,
                    btn_x - 12.0 - esc_w,
                    foot_cy,
                    12.0,
                    [0x66, 0x66, 0x66],
                    typ_alpha,
                );
            }
        }
        ui.confirm_rect = confirm_rect;

        if let Some(presenter) = &self.presenter {
            presenter.blit(&pixmap);
        }
        self.pixmap = Some(pixmap);
    }

    /// [GRAIN] True when the overlay is visible ONLY for a transient idle
    /// prompt-switch preview: no recording/processing session is active, the
    /// riser is up, and it is the prompt switcher (not the agent follow-up
    /// offer). In this state the body + dot aura are suppressed and the capsule
    /// is drawn alone, centered on screen.
    fn is_idle_prompt_preview(&self) -> bool {
        matches!(self.state, PillState::Idle | PillState::Fallback)
            && self.agent_offer.is_none()
            && self
                .prompt_preview_until
                .is_some_and(|t| t > Instant::now())
    }

    fn render_collapsed(&mut self) {
        if self.window.is_none() {
            return;
        }
        let (w, h) = Self::win_size();
        // Reuse one pixmap across frames (no per-frame allocation); clear it.
        let mut pixmap = self
            .pixmap
            .take()
            .unwrap_or_else(|| Pixmap::new(w, h).unwrap());
        pixmap.fill(Color::TRANSPARENT);

        let cell_px = Self::cell_px();
        let y_off = Self::y_offset();
        let pill_h = ROWS as f32 * cell_px;
        let r = pill_h / 2.0;
        // [GRAIN] The pill body is keyed to its OWN footprint (`core_w`), not the
        // window width — the window is wider to the right to host the sibling
        // prompt capsule, and the pill must stay put/centered regardless.
        let core_w = Self::collapsed_core_w();
        let (x0, x1) = (cell_px, core_w - cell_px);

        // [GRAIN] Idle prompt-switch preview: the pill is visible ONLY to show
        // the transient riser (no session, no agent offer). Drop the body + dot
        // aura and render the capsule alone, centered. Mid-speech switches keep
        // the full pill beside the capsule as before.
        let idle_preview = self.is_idle_prompt_preview();

        // 1) Floating capsule body (offset below the transparent top reserve).
        if !idle_preview {
            let mut body = Paint {
                anti_alias: true,
                ..Default::default()
            };
            body.set_color(Color::from_rgba8(0, 0, 0, 240));
            if let Some(rect) = Rect::from_ltrb(x0 + r, y_off, x1 - r, y_off + pill_h) {
                pixmap.fill_path(
                    &PathBuilder::from_rect(rect),
                    &body,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
            for cx in [x0 + r, x1 - r] {
                if let Some(circle) = PathBuilder::from_circle(cx, y_off + r, r) {
                    pixmap.fill_path(
                        &circle,
                        &body,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }

            // 2) Dots.
            let dots = &self.aura.dots;
            let radius = DOT_D * SCALE / 2.0;
            let mut paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            for row in 0..ROWS {
                for col in 0..COLS {
                    if is_edge(col, row) {
                        continue;
                    }
                    let c = dots[row * COLS + col];
                    if c[3] == 0 {
                        continue;
                    }
                    let dx = col as f32 * cell_px + cell_px / 2.0;
                    let dy = y_off + row as f32 * cell_px + cell_px / 2.0;
                    if let Some(circle) = PathBuilder::from_circle(dx, dy, radius) {
                        paint.set_color(Color::from_rgba8(c[0], c[1], c[2], c[3]));
                        pixmap.fill_path(
                            &circle,
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }

        // 3) Prompt capsule — slides in to the RIGHT of the pill during a
        // mid-speech prompt switch (or as the agent follow-up offer). Drawn after
        // the pill so its slide-in never clips under the body. When the pill is
        // visible ONLY for an idle prompt-switch preview, the capsule is drawn
        // alone and centered (see `draw_centered_prompt_capsule`).
        if self.riser_progress > 0.01 {
            if idle_preview {
                self.draw_centered_prompt_capsule(&mut pixmap, y_off, pill_h);
            } else {
                self.draw_sibling_pill(&mut pixmap, x1, y_off, pill_h);
            }
        } else {
            self.prompt_switch_rect = None;
        }

        // [GRAIN] Whole-surface fade for the collapsed capsule (offer reveal /
        // withdrawal). The pixmap is premultiplied RGBA, so scaling every byte
        // uniformly preserves the premultiplication invariant. No-op at full
        // opacity; the pixmap is tiny, so the pass is negligible.
        if self.studio_alpha < 0.995 {
            let f = self.studio_alpha.clamp(0.0, 1.0);
            for b in pixmap.data_mut() {
                *b = (*b as f32 * f) as u8;
            }
        }

        if let Some(presenter) = &self.presenter {
            presenter.blit(&pixmap);
        }
        self.pixmap = Some(pixmap); // keep it for next frame
    }

    /// The unified pill's EXPANDED (streaming) surface — the collapsed capsule
    /// grown into a caption card: live transcript up top, and pinned to the bottom
    /// the recording dot (left) · the dot-matrix aura as the "waveform" (center) ·
    /// cancel X (right). The prompt riser still slides up from behind the top edge.
    fn render_studio(&mut self) {
        if self.window.is_none() {
            return;
        }
        // A caption dictated in a non-Latin script needs the fallback face; the
        // choice is per-transcript so the whole caption stays one consistent
        // face (the common Latin case never loads the fallback).
        let transcript_probe = self.asr.probe_text();
        self.ensure_fallback_for(&transcript_probe);
        let caption_font = self
            .font
            .as_ref()
            .map(|f| font_for(f, self.fallback_font.as_ref(), &transcript_probe));

        let (w, h) = Self::win_size_for(PillMode::Studio);
        let mut pixmap = self
            .pixmap
            .take()
            .unwrap_or_else(|| Pixmap::new(w, h).unwrap());

        // Advance the equalizer clock once per frame (smooth, cadence-free).
        self.studio_phase += 1.0;
        let fade = self.studio_alpha.clamp(0.0, 1.0);

        // [GRAIN] Grow the card smoothly toward the height its current line count
        // needs (0 lines = bare dot-matrix → 4-line cap). First frame snaps;
        // afterwards it eases so each new line rises in rather than jumping. Pure
        // compositing inside the fixed max-size window — no OS resize.
        let n_lines = studio_line_count(&self.asr, caption_font);
        let target_h = studio_card_height(n_lines);
        if self.studio_grown_h <= 0.0 {
            self.studio_grown_h = target_h;
        } else {
            self.studio_grown_h += (target_h - self.studio_grown_h) * STUDIO_GROW_EASE;
        }

        // [GRAIN] Width grow: the capsule eases open from the collapsed pill width
        // to the full card so it reads as the small pill EXPANDING. Reset to 0 at
        // each new session (mode change), so every stream opens with the grow.
        self.studio_expand += (1.0 - self.studio_expand) * STUDIO_EXPAND_EASE;
        if self.studio_expand > 0.995 {
            self.studio_expand = 1.0; // settle exactly so the text gate opens cleanly
        }
        let fully_open = self.studio_expand >= STUDIO_TEXT_GATE;

        // [GRAIN] Per-word reveal: stamp the first-seen time of each word (indexed
        // by global order) so freshly-decoded words fade in instead of popping.
        // Words are only ever appended at the tail (committed grows at the front,
        // tentative at the end) so index-based tracking is stable — a word that is
        // merely revised in place keeps its stamp and never re-fades.
        let now = Instant::now();
        let word_count = self.asr.display_runs().len();
        if self.reveal_since.len() > word_count {
            self.reveal_since.truncate(word_count);
        }
        while self.reveal_since.len() < word_count {
            self.reveal_since.push(now);
        }
        // Until the pill is fully open the transcript is hidden (see
        // `paint_studio_card`); hold every word's reveal clock at `now` so the
        // first words fade in cleanly AFTER the expansion instead of appearing
        // already-revealed the instant the gate opens.
        if !fully_open {
            for t in self.reveal_since.iter_mut() {
                *t = now;
            }
        }
        let reveal_ms = STUDIO_WORD_REVEAL.as_secs_f32();
        let reveal_alpha: Vec<f32> = self
            .reveal_since
            .iter()
            .map(|t| {
                let x =
                    (now.saturating_duration_since(*t).as_secs_f32() / reveal_ms).clamp(0.0, 1.0);
                x * x * (3.0 - 2.0 * x) // smoothstep — ease-in-out
            })
            .collect();

        // Most drawing lives in the windowing-free `paint_studio_card` so it can
        // be rendered to a PNG in tests without a winit window/presenter.
        paint_studio_card(
            &mut pixmap,
            &self.asr,
            self.state,
            fade,
            self.studio_phase,
            caption_font,
            self.studio_grown_h,
            n_lines,
            &reveal_alpha,
            &self.aura.dots,
            self.studio_expand,
        );

        // [GRAIN] Prompt switcher — the same mid-speech switch as the collapsed
        // pill, here a full-width capsule that slides UP into the reserved band
        // above the transcript card. Drawn AFTER the card; the two share the same
        // near-black fill, so the capsule's lower edge tucking behind the card
        // during the slide is seamless (identical color over identical color).
        if self.riser_progress > 0.01 {
            let hf = h as f32;
            let card_h = self.studio_grown_h.clamp(studio_card_height(0), hf);
            let card_top = hf - card_h;
            self.draw_studio_top_pill(&mut pixmap, w as f32, card_top, fade);
        } else {
            self.prompt_switch_rect = None;
        }

        if let Some(presenter) = &self.presenter {
            presenter.blit(&pixmap);
        }
        self.pixmap = Some(pixmap);
    }

    /// [GRAIN] The collapsed pill's SIBLING prompt capsule — a fixed-width pill
    /// that slides in to the RIGHT of the pill (`pill_right` = the body's right
    /// edge), carrying the active prompt name between `‹`/`›` arrows. The agent
    /// follow-up offer reuses it (no arrows, sized to its text up to `SIB_MAX_W`).
    /// `riser_progress` drives both the slide-in travel and the fade.
    fn draw_sibling_pill(&mut self, pixmap: &mut Pixmap, pill_right: f32, y_off: f32, pill_h: f32) {
        let p = self.riser_progress.clamp(0.0, 1.0);
        let is_offer = self.agent_offer.is_some();
        // Fixed width for the switcher; the offer hugs its label up to the max.
        let cap_w = if is_offer {
            let label_w = self.cached_label.as_ref().map_or(0.0, |c| c.total_width);
            (label_w + 4.0 * SIB_TEXT_PAD).clamp(SIB_W, SIB_MAX_W)
        } else {
            SIB_W
        };
        let slide = SIB_SLIDE * (1.0 - p);
        let left = pill_right + SIB_GAP + slide;
        let right = left + cap_w;
        self.draw_prompt_capsule(pixmap, left, right, y_off, pill_h, SIB_ARROW_INSET, p);
        // Remember the switcher's rect so a click on it is NOT read as a pill
        // action (Prompt Record). The offer capsule stays clickable (it IS the
        // follow-up affordance), so only record the rect for the switcher.
        self.prompt_switch_rect = if is_offer {
            None
        } else {
            Some((left, y_off, right, y_off + pill_h))
        };
    }

    /// [GRAIN] Centered prompt-switch capsule — used when the overlay is visible
    /// ONLY for a transient idle prompt-switch preview (no session, no agent
    /// offer). Draws the capsule alone at the LEFT of the window (`0..SIB_W`);
    /// the window is positioned (see the `becoming_visible` show path) centered
    /// on `SIB_W`, so the capsule lands screen-centered. Fades in place rather
    /// than sliding. Reuses `draw_prompt_capsule` for the look.
    fn draw_centered_prompt_capsule(&mut self, pixmap: &mut Pixmap, y_off: f32, pill_h: f32) {
        let p = self.riser_progress.clamp(0.0, 1.0);
        let left = 0.0;
        let right = SIB_W;
        self.draw_prompt_capsule(pixmap, left, right, y_off, pill_h, SIB_ARROW_INSET, p);
        self.prompt_switch_rect = Some((left, y_off, right, y_off + pill_h));
    }

    /// [GRAIN] The Studio surface's prompt capsule — a full-width pill (matching
    /// the transcript card's width) that slides UP into the reserved band above
    /// the card (`card_top`). Fixed size; the label truncates rather than resizing
    /// it. `riser_progress` drives the slide; `fade` is the whole-window opacity.
    fn draw_studio_top_pill(&mut self, pixmap: &mut Pixmap, wf: f32, card_top: f32, fade: f32) {
        let p = self.riser_progress.clamp(0.0, 1.0);
        let travel = STUDIO_TOP_GAP + STUDIO_TOP_PILL_H;
        let top = card_top - travel * p;
        let bottom = top + STUDIO_TOP_PILL_H;
        self.draw_prompt_capsule(
            pixmap,
            0.0,
            wf,
            top,
            STUDIO_TOP_PILL_H,
            STUDIO_TOP_ARROW_INSET,
            p * fade.clamp(0.0, 1.0),
        );
        self.prompt_switch_rect = if self.agent_offer.is_some() {
            None
        } else {
            Some((0.0, top, wf, bottom))
        };
    }

    /// Draw a prompt capsule into `[px0,px1] × [top, top+ph]`: a near-black
    /// rounded pill (matching the Studio card's fill), then — unless this is the
    /// agent follow-up offer — `‹`/`›` arrows at `arrow_inset` from each end and
    /// the prompt label centered (already ellipsis-truncated in `cached_label`).
    /// `alpha` (0..1) fades the whole capsule for the slide-in.
    fn draw_prompt_capsule(
        &self,
        pixmap: &mut Pixmap,
        px0: f32,
        px1: f32,
        top: f32,
        ph: f32,
        arrow_inset: f32,
        alpha: f32,
    ) {
        let alpha = alpha.clamp(0.0, 1.0);
        let fill_a = (alpha * 244.0) as u8;
        if fill_a == 0 || px1 <= px0 {
            return;
        }
        let rr = (ph / 2.0).min((px1 - px0) / 2.0).max(0.0);
        let mut fill = Paint {
            anti_alias: true,
            ..Default::default()
        };
        fill.set_color(Color::from_rgba8(13, 13, 15, fill_a));
        if let Some(path) = rounded_rect_path(px0, top, px1 - px0, ph, rr) {
            pixmap.fill_path(&path, &fill, FillRule::Winding, Transform::identity(), None);
        }

        if let Some(font) = &self.font {
            let cy = top + ph / 2.0;
            let col = [236, 229, 218];
            // The `‹ ›` arrows belong to the SWITCHER; the follow-up offer is a
            // single clickable affordance, so it hides them.
            if self.agent_offer.is_none() {
                let l = CachedText::new(font, "\u{2039}", PROMPT_LABEL_PX);
                let r = CachedText::new(font, "\u{203a}", PROMPT_LABEL_PX);
                draw_cached_text_centered(
                    pixmap,
                    &l,
                    (px0 + arrow_inset, cy),
                    PROMPT_LABEL_PX,
                    col,
                    alpha,
                );
                draw_cached_text_centered(
                    pixmap,
                    &r,
                    (px1 - arrow_inset, cy),
                    PROMPT_LABEL_PX,
                    col,
                    alpha,
                );
            }
            if let Some(cached_label) = &self.cached_label {
                draw_cached_text_centered(
                    pixmap,
                    cached_label,
                    ((px0 + px1) / 2.0, cy),
                    PROMPT_LABEL_PX,
                    col,
                    alpha,
                );
            }
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (w, h) = Self::win_size();
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes()
            .with_title("")
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_visible(false) // hidden until the core signals a session
            .with_inner_size(PhysicalSize::new(w, h));
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attrs = attrs.with_skip_taskbar(true);
        }
        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        present::make_layered(&window);
        self.presenter = present::Presenter::new(&window, w as i32, h as i32);

        // Initial placement using the current anchor; repositioned on each show.
        let anchor = self.remote.lock().unwrap().anchor;
        Self::position_window(&window, anchor, h, Self::collapsed_core_w());

        eprintln!("window: created {w}x{h} (hidden until a session starts)");
        self.window = Some(window);
        self.next_tick = Instant::now();
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    // Wake from HIDDEN_TICK sleep instantly when the WS thread signals a session event.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: UserEvent) {
        // Force about_to_wait to fire on the very next loop tick.
        self.next_tick = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::ModifiersChanged(m) => {
                self.ctrl_down = m.state().control_key();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = (position.x as f32, position.y as f32);
                if let Some(ui) = &mut self.agent_input {
                    let (x0, y0, x1, y1) = ui.confirm_rect;
                    ui.hover_confirm = ui.expanded
                        && self.cursor_pos.0 >= x0
                        && self.cursor_pos.0 <= x1
                        && self.cursor_pos.1 >= y0
                        && self.cursor_pos.1 <= y1;
                }
            }
            // [GRAIN] Prompt Record: a left-click on the pill while recording enters
            // AI-instruction mode — everything spoken after this is a prompt for the
            // LLM, not content. Works on the collapsed capsule AND the expanded
            // Studio card (its center waveform). One-way (no un-toggle, to keep it
            // dead simple). We send the action to the core and let its
            // `PromptRecordingChanged` echo flip the visuals, so the tint only
            // changes once the mark is actually registered.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                // [GRAIN] Agent input: click-to-expand (compact) / Confirm (expanded).
                if let Some(ui) = &mut self.agent_input {
                    let (cx, cy) = self.cursor_pos;
                    let (bx0, by0, bx1, by1) = ui.confirm_rect;
                    if ui.expanded && cx >= bx0 && cx <= bx1 && cy >= by0 && cy <= by1 {
                        let text = ui.text.trim().to_string();
                        if !text.is_empty() {
                            let _ = self
                                .action_tx
                                .send(PillAction::AgentInputSubmitText { text });
                        } else {
                            let _ = self.action_tx.send(PillAction::AgentInputSubmitVoice);
                        }
                    } else if !ui.expanded {
                        let (kx0, ky0, kx1, ky1) = ui.card_rect;
                        if cx >= kx0 && cx <= kx1 && cy >= ky0 && cy <= ky1 {
                            ui.expanded = true;
                            let _ = self
                                .action_tx
                                .send(PillAction::AgentInputTyping { active: true });
                        }
                    }
                    return;
                }
                // [GRAIN] Ignore clicks that land on the prompt-SWITCHER capsule —
                // it is a display-only indicator (cycled by the shortcut, not the
                // mouse), so a click there must not fire a pill action.
                if let Some((rx0, ry0, rx1, ry1)) = self.prompt_switch_rect {
                    let (cx, cy) = self.cursor_pos;
                    if cx >= rx0 && cx <= rx1 && cy >= ry0 && cy <= ry1 {
                        return;
                    }
                }
                // Works on BOTH surfaces: the collapsed capsule and the expanded
                // Studio card (whose center waveform is the click affordance).
                if self.state == PillState::Recording && !self.prompt_recording {
                    let _ = self.action_tx.send(PillAction::PromptRecord);
                } else if self.agent_offer.is_some() && self.state != PillState::Recording {
                    // [GRAIN] Quick Agent: the pill is up as a follow-up offer —
                    // a click reopens the Agent expanded with the conversation.
                    let _ = self.action_tx.send(PillAction::AgentFollowup);
                }
            }
            // [GRAIN] Agent input keyboard: the window has real focus while the
            // input is up, so keystrokes land here as ordinary window events.
            // (Enter/Escape usually arrive via the core's transient GLOBAL
            // shortcuts instead — these are the fallback when those failed to
            // register.) Handled BEFORE the dev-preview keys below.
            WindowEvent::KeyboardInput {
                event: ref key_event @ KeyEvent {
                    state: ElementState::Pressed,
                    ..
                },
                ..
            } if self.agent_input.is_some() => {
                self.agent_input_key(key_event.clone());
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match logical_key.as_ref() {
                Key::Named(NamedKey::Escape) => event_loop.exit(),
                // Dev preview overrides (write through the same remote the WS drives).
                Key::Character("r") => {
                    let mut r = self.remote.lock().unwrap();
                    r.state = PillState::Recording;
                    r.visible = true;
                }
                Key::Character("p") => {
                    let mut r = self.remote.lock().unwrap();
                    r.state = PillState::Processing;
                    r.visible = true;
                }
                Key::Character("i") => self.remote.lock().unwrap().visible = false,
                // [GRAIN] Dev preview: toggle the Prompt Record blue tint (press R
                // first, then B). Mirrors the real PromptRecordingChanged event.
                Key::Character("b") => {
                    let mut r = self.remote.lock().unwrap();
                    r.prompt_recording = !r.prompt_recording;
                }
                // [GRAIN] Dev preview: toggle the native agent input card (A).
                // Mirrors the real AgentInputShow/Hide events.
                Key::Character("a") => {
                    let mut r = self.remote.lock().unwrap();
                    if r.agent_input.take().is_some() {
                        r.agent_input_seq = r.agent_input_seq.wrapping_add(1);
                    } else {
                        r.agent_input = Some((128, true, AgentInputKind::Assist));
                        r.agent_input_seq = r.agent_input_seq.wrapping_add(1);
                    }
                }
                // Preview the prompt riser (← / →). Real trigger = DaemonEvent::PromptChanged.
                Key::Named(NamedKey::ArrowRight) => {
                    self.prompt_idx = (self.prompt_idx + 1) % self.prompts.len();
                    self.prompt_label = self.prompts[self.prompt_idx].clone();
                    self.riser_hide_at = Some(Instant::now() + RISER_HOLD);
                    self.prompt_preview_until = Some(Instant::now() + RISER_HOLD);
                    self.update_cached_label();
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.prompt_idx =
                        (self.prompt_idx + self.prompts.len() - 1) % self.prompts.len();
                    self.prompt_label = self.prompts[self.prompt_idx].clone();
                    self.riser_hide_at = Some(Instant::now() + RISER_HOLD);
                    self.prompt_preview_until = Some(Instant::now() + RISER_HOLD);
                    self.update_cached_label();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_tick {
            // Pull authoritative state/visibility from the core (or dev keys).
            let r = self.remote.lock().unwrap().clone();
            self.state = r.state;
            self.prompt_recording = r.prompt_recording;
            self.asr = r.asr.clone();

            // [GRAIN] Native agent input shown/hidden by the core. Adopt it
            // BEFORE the mode computation below so the surface swap happens in
            // the same tick. The window becomes focusable only while the input
            // is up (typing needs real keyboard focus); it reverts to the
            // never-activate overlay the moment the input goes away.
            if r.agent_input_seq != self.last_agent_input_seq {
                self.last_agent_input_seq = r.agent_input_seq;
                match r.agent_input {
                    Some((chars, tte, kind)) => {
                        // A re-show while already up just refreshes the chip and
                        // re-grabs focus (the core re-emits on a double summon).
                        let already = self.agent_input.is_some();
                        if let Some(ui) = &mut self.agent_input {
                            ui.selection_chars = chars;
                            ui.type_to_expand = tte;
                            ui.kind = kind;
                        } else {
                            self.agent_input = Some(AgentInputUi::new(chars, tte, kind));
                        }
                        if let Some(window) = &self.window {
                            present::set_focusable(window, true);
                            if already {
                                present::force_foreground(window);
                            }
                        }
                    }
                    None => {
                        self.agent_input = None;
                        if let Some(window) = &self.window {
                            present::set_focusable(window, false);
                        }
                    }
                }
            }

            // [GRAIN] Grain Space capture confirmed — flip the still-open card to
            // its green "Saved" state (the core hides it after a brief hold).
            if r.agent_input_saved_seq != self.last_agent_input_saved_seq {
                self.last_agent_input_saved_seq = r.agent_input_saved_seq;
                if let Some(ui) = &mut self.agent_input {
                    ui.saved = true;
                }
            }

            // [GRAIN] The core's global Enter fired — answer with the submit
            // matching our state (typed text wins; otherwise submit the voice
            // capture). The core clears the input on receipt.
            if r.agent_submit_req_seq != self.last_agent_submit_req_seq {
                self.last_agent_submit_req_seq = r.agent_submit_req_seq;
                if let Some(ui) = &self.agent_input {
                    let action = if ui.expanded && !ui.text.trim().is_empty() {
                        PillAction::AgentInputSubmitText {
                            text: ui.text.trim().to_string(),
                        }
                    } else {
                        PillAction::AgentInputSubmitVoice
                    };
                    let _ = self.action_tx.send(action);
                }
            }

            // The agent input overrides every other surface while it is up.
            let desired_mode = if self.agent_input.is_some() {
                PillMode::AgentInput
            } else {
                r.mode
            };

            // [GRAIN] Resize/recreate the OS window the rare times the surface
            // actually changes (Collapsed <-> Studio <-> AgentInput) — never per
            // frame. The Presenter caches a fixed-size GDI bitmap, so it must be
            // rebuilt for the new size; the cached pixmap is invalidated too
            // (wrong dimensions otherwise).
            if desired_mode != self.mode {
                // [GRAIN] Collapsed→Studio while already on screen is the FIRST-WORD
                // expand (mid-session): keep it visible at full opacity so it grows
                // seamlessly instead of fading/flashing. Every other transition
                // (fresh session, Studio→Collapsed) resets the fade as before.
                let seamless_expand = self.mode == PillMode::Collapsed
                    && desired_mode == PillMode::Studio
                    && (self.visible || self.closing);
                self.mode = desired_mode;
                if let Some(window) = &self.window {
                    let (w, h) = Self::win_size_for(self.mode);
                    // [GRAIN] winit 0.30 renamed this `request_inner_size` (the
                    // resize isn't always synchronous on every platform); on
                    // Windows it applies immediately, so the Presenter rebuilt
                    // right after is sized correctly for the very next frame.
                    let _ = window.request_inner_size(PhysicalSize::new(w, h));
                    self.presenter = present::Presenter::new(window, w as i32, h as i32);
                    // Re-anchor the resized window so the capsule stays put across
                    // the swap. The agent input has its own placement (centered on
                    // the work-area edge, expanding away from it).
                    if self.mode == PillMode::AgentInput {
                        let ain_anchor = self.agent_input_anchor(r.anchor);
                        Self::position_agent_input(window, ain_anchor);
                    } else {
                        Self::position_window(
                            window,
                            r.anchor,
                            h,
                            Self::center_w_for(self.mode, w),
                        );
                    }
                }
                self.pixmap = None;
                // Grow the card from the COLLAPSED (matrix-only) height and the
                // collapsed width, so a streaming session opens as the small pill
                // and expands out when the first word lands.
                self.studio_grown_h = studio_card_height(0);
                self.studio_expand = 0.0;
                self.reveal_since.clear();
                // Re-roll immediately so the first Studio frame already shows the
                // reduced voice-reactive field (not a stale full-pill frame).
                self.next_roll = now;
                if seamless_expand {
                    self.studio_alpha = 1.0;
                } else {
                    self.studio_alpha = 0.0;
                    self.visible = false;
                    self.closing = false;
                }
            }

            // [GRAIN] Prompt switched → show the riser, and briefly reveal the
            // pill if no session is active (so the user sees the new title).
            if r.prompt_seq != self.last_prompt_seq {
                self.last_prompt_seq = r.prompt_seq;
                self.prompt_label = r.prompt_name.clone();
                self.riser_hide_at = Some(now + RISER_HOLD);
                self.prompt_preview_until = Some(now + RISER_HOLD);
                self.update_cached_label();
            }

            // [GRAIN] Quick-Agent follow-up offer changed → adopt it. While live
            // the riser carries "ASK FOLLOW-UP · <shortcut>" and stays up (the
            // hold below pins the riser target at 1); on clear it eases away.
            if r.agent_offer_seq != self.last_agent_offer_seq {
                self.last_agent_offer_seq = r.agent_offer_seq;
                self.agent_offer = r.agent_offer.clone();
                match &self.agent_offer {
                    Some(shortcut) => {
                        self.prompt_label =
                            format!("ASK FOLLOW-UP \u{b7} {}", shortcut.to_uppercase());
                        self.update_cached_label();
                        self.offer_fade_close = false;
                        // Fade the offer IN when it reveals a hidden pill.
                        if !self.visible && !self.closing {
                            self.studio_alpha = 0.0;
                        }
                    }
                    None => {
                        self.riser_hide_at = None;
                        // The withdrawal fades out instead of vanishing.
                        self.offer_fade_close = true;
                    }
                }
            }

            // Visible if the core says so, we're inside a transient prompt
            // preview, a Quick-Agent follow-up offer is live, or the native
            // agent input is up (the input ignores the overlay's None anchor —
            // it is a functional surface, not a status overlay).
            let offer_live = self.agent_offer.is_some();
            let input_live = self.agent_input.is_some();
            let preview_visible = self.prompt_preview_until.is_some_and(|t| now < t);
            let want_visible = r.visible || preview_visible || offer_live || input_live;
            // [GRAIN] The Studio Window fades out instead of vanishing: while
            // `closing` is true we keep `self.visible` true (so rendering/mic
            // gating below behave as if still showing) and just ease
            // `studio_alpha` toward 0, only actually hiding once it settles
            // (below). The collapsed pill is unchanged — it still hides the
            // instant the core says to.
            let was_showing = self.visible || self.closing;
            let becoming_visible = want_visible && !was_showing;
            let starting_close = !want_visible && was_showing && !self.closing;

            if becoming_visible {
                self.visible = true;
                self.closing = false;
            } else if starting_close {
                // Studio always fades; the collapsed capsule fades only when the
                // hide is an offer withdrawal — session ends keep the instant hide.
                if self.mode == PillMode::Studio || self.offer_fade_close {
                    self.closing = true;
                } else {
                    self.visible = false;
                    if let Some(window) = &self.window {
                        eprintln!("window: hide");
                        present::hide_window(window);
                    }
                }
            }

            // Snap the tick deadline to now so becoming_visible renders immediately
            // (UserEvent::Wake already shortened the sleep; this is the safety net).
            if becoming_visible {
                self.next_tick = now;
            }

            // [GRAIN] Mic lifecycle is gated on RECORDING, not mere visibility:
            // only `roll_recording` consumes live amplitude, so opening the
            // capture device for the Processing phase or a prompt-riser preview
            // would light the OS "mic in use" indicator and wake the audio
            // callback for nothing ("destroy if not in use"). Open it just-in-time
            // when recording starts; release it the instant we leave Recording
            // (stop → Processing) or the pill hides.
            let needs_mic = self.visible && self.state == PillState::Recording;
            if needs_mic && self._mic.is_none() {
                self._mic = start_mic(self.amp.clone());
                if self._mic.is_none() {
                    eprintln!("no microphone — falling back to a simulated signal");
                }
            } else if !needs_mic && self._mic.is_some() {
                // Recording ended (or the pill hid) — free the device immediately.
                self._mic = None;
            }

            if self.visible {
                // Ease the Studio Window's whole-window fade. A no-op for the
                // collapsed pill (which never sets `closing` and always
                // targets full opacity, so `studio_alpha` just sits at 1.0).
                let target_alpha = if self.closing { 0.0 } else { 1.0 };
                self.studio_alpha += (target_alpha - self.studio_alpha) * 0.18;
                if self.closing && self.studio_alpha < 0.02 {
                    self.studio_alpha = 0.0;
                    self.closing = false;
                    self.offer_fade_close = false;
                    self.visible = false;
                    if let Some(window) = &self.window {
                        eprintln!("window: hide (fade complete)");
                        present::hide_window(window);
                    }
                }
            }

            if self.visible {
                // Re-roll the dot field on its own (slower) cadence so it stays
                // calm; everything else eases every frame for smoothness.
                if now >= self.next_roll {
                    let amp = self.current_amp();
                    // The expanded pill uses the 2-column field: center-outward
                    // waveform while recording, orange sparkle while processing,
                    // calm breathing for idle/fallback. Collapsed pill unchanged.
                    // [GRAIN] Feed the Prompt Record tint into the dot field: the
                    // collapsed recording field turns grey/light-blue, the Studio
                    // waveform turns sky blue. Density/shape are unchanged.
                    self.aura.prompt_recording = self.prompt_recording;
                    if self.mode == PillMode::Studio {
                        match self.state {
                            PillState::Recording => self.aura.roll_studio_waveform(amp),
                            PillState::Processing => self.aura.roll_studio_processing(),
                            _ => self.aura.roll_studio(amp),
                        }
                    } else {
                        self.aura.roll(self.state, amp);
                    }
                    self.next_roll = now + ROLL_INTERVAL;
                }
                // Ease the prompt riser, auto-hiding after RISER_HOLD. A live
                // follow-up offer pins it up until the core withdraws the offer.
                let riser_target = if offer_live {
                    1.0
                } else {
                    match self.riser_hide_at {
                        Some(t) if now < t => 1.0,
                        _ => 0.0,
                    }
                };
                self.riser_progress += (riser_target - self.riser_progress) * 0.12;
                if riser_target == 0.0 && self.riser_progress < 0.02 {
                    self.riser_progress = 0.0;
                    self.riser_hide_at = None;
                }
                // [GRAIN] Agent input per-frame motion: the wave/caret clock and
                // the expand ease (≈ the reference's 300ms ease-out curve).
                if let Some(ui) = &mut self.agent_input {
                    ui.phase += TICK.as_secs_f32();
                    let target = if ui.expanded { 1.0 } else { 0.0 };
                    ui.expand_t += (target - ui.expand_t) * 0.16;
                }
                // Push the layered content FIRST (a layered window shows nothing
                // until UpdateLayeredWindow runs) …
                self.render();
                // … then reveal it. Overlay surfaces must never steal focus; the
                // agent INPUT is the one exception — it exists to be typed into,
                // so it takes the foreground (bridging Windows' foreground lock).
                if becoming_visible {
                    if let Some(window) = &self.window {
                        // Re-anchor each show so a changed setting / active monitor
                        // takes effect immediately.
                        if self.mode == PillMode::AgentInput {
                            let ain_anchor = self.agent_input_anchor(r.anchor);
                            Self::position_agent_input(window, ain_anchor);
                            eprintln!("window: show agent input (focused)");
                            present::show_window(window);
                            present::force_foreground(window);
                        } else {
                            let (w, h) = Self::win_size_for(self.mode);
                            // [GRAIN] An idle prompt-switch preview shows ONLY the
                            // centered capsule (no body), so center the window on
                            // the capsule's width (`SIB_W`) — not the pill body —
                            // so the capsule lands screen-centered.
                            let center_w = if self.mode == PillMode::Collapsed
                                && self.is_idle_prompt_preview()
                            {
                                SIB_W
                            } else {
                                Self::center_w_for(self.mode, w)
                            };
                            Self::position_window(window, r.anchor, h, center_w);
                            eprintln!("window: show (content primed)");
                            present::show_window(window);
                        }
                    }
                }
            }
            // 60 fps only while visible; when hidden, sleep until the idle-free
            // deadline (then forever), woken early by UserEvent::Wake.
            if self.visible {
                self.free_idle_at = None; // shown → cancel any pending free
                self.next_tick = now + TICK;
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
            } else {
                // [GRAIN] Idle-free: the first hidden tick arms the deadline; a
                // later hidden tick at/after it drops the parsed font + frame
                // buffer so the always-on pill idles at its floor. Both reload
                // lazily on the next show (ensure_font + pixmap re-alloc).
                match self.free_idle_at {
                    None if self.font.is_some()
                        || self.fallback_font.is_some()
                        || self.pixmap.is_some() =>
                    {
                        self.free_idle_at = Some(now + IDLE_FREE_AFTER);
                        event_loop
                            .set_control_flow(ControlFlow::WaitUntil(now + IDLE_FREE_AFTER));
                    }
                    Some(deadline) if now >= deadline => {
                        self.font = None;
                        self.fallback_font = None;
                        self.fallback_tried = false;
                        self.pixmap = None;
                        self.cached_label = None;
                        self.free_idle_at = None;
                        event_loop.set_control_flow(ControlFlow::Wait);
                    }
                    Some(deadline) => {
                        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                    }
                    None => event_loop.set_control_flow(ControlFlow::Wait),
                }
            }
        } else {
            // Wait until next tick if we haven't reached it yet
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
        }
    }
}

pub fn run_pill() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, SetPriorityClass, HIGH_PRIORITY_CLASS,
        };
        use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
        unsafe {
            let _ = SetCurrentProcessExplicitAppUserModelID(windows::core::w!("com.grain.app"));
            let _ = SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS);
        }
    }
    eprintln!("pill: starting (pid {})", std::process::id());
    let event_loop: EventLoop<UserEvent> = EventLoop::with_user_event()
        .build()
        .expect("create event loop");
    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    event_loop.run_app(&mut app).expect("run pill");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> fontdue::Font {
        // The primary face is now bundled (subset Space Grotesk), so this always
        // succeeds — no system-font dependency in tests or at runtime.
        load_font().expect("bundled primary font must parse")
    }

    /// The bundled subset parses and covers everything the pill draws: ASCII,
    /// European accents, and the riser/card punctuation symbols. (U+21B5 is
    /// intentionally absent — the Confirm arrow is hand-drawn.)
    #[test]
    fn bundled_font_has_pill_glyphs() {
        let f = load_font().expect("bundled primary font must parse");
        for ch in ['A', 'z', '0', '9', 'é', 'ñ', 'ł', '\u{2039}', '\u{203a}', '\u{b7}',
            '\u{2026}', '\u{201c}', '\u{2022}', ' ', '.', '·']
        {
            assert_ne!(
                f.lookup_glyph_index(ch),
                0,
                "bundled font missing glyph for {ch:?}"
            );
        }
    }

    /// `font_for` keeps the all-Latin path on the primary and only diverts a
    /// string with a truly-missing glyph to the fallback.
    #[test]
    fn font_for_prefers_primary_for_latin() {
        let primary = load_font().unwrap();
        // Use the same face as a stand-in fallback: an all-Latin string must
        // still resolve to the primary (no missing glyphs).
        let fb = load_font().unwrap();
        let chosen = font_for(&primary, Some(&fb), "Hello, world — café");
        assert!(std::ptr::eq(chosen, &primary));
        // A char the primary lacks (e.g. Cyrillic Д) has no glyph in this
        // Latin-only stand-in fallback either, so it still stays on primary
        // (font_for only diverts when the fallback actually has the glyph).
        assert!(primary.lookup_glyph_index('\u{0414}') == 0);
    }

    /// Render the Studio Window to a PNG for visual inspection. Not an
    /// assertion test — it just proves `paint_studio_card` runs windowing-free
    /// and produces the expected pixel size, and leaves an artifact to eyeball.
    #[test]
    fn studio_card_renders_to_png() {
        use tiny_skia::PixmapPaint;

        let font = font();
        let (cw, ch) = studio_pixel_size();

        // A realistic mid-dictation frame that overflows the 4-line cap so the
        // top-edge dissolve is visible: a long committed (crisp) prefix + a short
        // volatile (dimmed) tail.
        let mut asr = AsrDisplay::default();
        asr.append_commit(
            "Let's test the streaming transcription with the new Studio Window layout and make sure it fills more than four lines so the oldest visible line dissolves into the dark surface at the very top edge of the card",
        );
        asr.partial = "while the tail stays dimmed".into();
        asr.partial_stable = false;

        // Stack two states (Recording, Processing) on a desktop-like backdrop so
        // proportions and the equalizer are easy to judge.
        let margin = 24i32;
        let gap = 20i32;
        let bw = cw + margin as u32 * 2;
        let bh = ch * 2 + margin as u32 * 2 + gap as u32;
        let mut bg = Pixmap::new(bw, bh).unwrap();
        bg.fill(Color::from_rgba8(205, 203, 198, 255));

        for (i, state) in [PillState::Recording, PillState::Processing]
            .into_iter()
            .enumerate()
        {
            let mut card = Pixmap::new(cw, ch).unwrap();
            // A rolled aura supplies the dot-matrix field: a waveform while
            // recording, the reactive scatter otherwise.
            let mut aura = Aura::new();
            for _ in 0..3 {
                match state {
                    PillState::Recording => aura.roll_studio_waveform(0.6),
                    PillState::Processing => aura.roll_studio_processing(),
                    _ => aura.roll_studio(0.6),
                }
            }
            // The sample text overflows 4 lines → render the capped (max-height)
            // card so the top dissolve is exercised. `expand = 1.0` = fully open.
            paint_studio_card(
                &mut card,
                &asr,
                state,
                1.0,
                12.0,
                Some(&font),
                studio_card_height(STUDIO_MAX_LINES),
                STUDIO_MAX_LINES,
                &[],
                &aura.dots,
                1.0,
            );
            let y = margin + i as i32 * (ch as i32 + gap);
            bg.draw_pixmap(
                margin,
                y,
                card.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }

        let path = std::env::temp_dir().join("grain_studio_preview.png");
        bg.save_png(&path).expect("save png");
        eprintln!("STUDIO_PREVIEW_PNG={}", path.display());
    }

    /// [GRAIN] Render the card at 1 → 4 wrapped lines so the GROW behavior is
    /// eyeball-able: 1–3 lines carry a small top gap and stay crisp to the top
    /// (no dissolve); the 4-line card closes the gap to the edge and turns the
    /// top dissolve on. Not an assertion test — leaves a PNG artifact.
    #[test]
    fn studio_card_growth_strip_renders_to_png() {
        use tiny_skia::PixmapPaint;

        let font = font();
        let (cw, ch) = studio_pixel_size();

        // Increasingly long committed text so it wraps to 1, 2, 3, then 4+ lines.
        let samples = [
            "Hey there,",
            "Hey there, so I just want to know how the",
            "Hey there, so I just want to know how the light transcription is working",
            "Hey there, so I just want to know how the light transcription is working and yeah, that is basically what I'm gonna do next",
        ];

        let margin = 24i32;
        let gap = 16i32;
        let bw = cw + margin as u32 * 2;
        let bh =
            ch * samples.len() as u32 + margin as u32 * 2 + gap as u32 * (samples.len() as u32 - 1);
        let mut bg = Pixmap::new(bw, bh).unwrap();
        bg.fill(Color::from_rgba8(205, 203, 198, 255));

        for (i, text) in samples.iter().enumerate() {
            let mut asr = AsrDisplay::default();
            asr.append_commit(text);
            let n = studio_line_count(&asr, Some(&font));
            let card_h = studio_card_height(n);
            let mut card = Pixmap::new(cw, ch).unwrap();
            let mut aura = Aura::new();
            for _ in 0..3 {
                aura.roll_studio_waveform(0.6);
            }
            paint_studio_card(
                &mut card,
                &asr,
                PillState::Recording,
                1.0,
                12.0,
                Some(&font),
                card_h,
                n,
                &[],
                &aura.dots,
                1.0,
            );
            let y = margin + i as i32 * (ch as i32 + gap);
            bg.draw_pixmap(
                margin,
                y,
                card.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }

        let path = std::env::temp_dir().join("grain_studio_growth.png");
        bg.save_png(&path).expect("save png");
        eprintln!("STUDIO_GROWTH_PNG={}", path.display());
    }

    /// [GRAIN] Render the WIDTH-expand animation (`expand` 0 → 1) so the "small
    /// pill grows into the streaming card" motion is eyeball-able: at 0 the capsule
    /// is just the dot-matrix width (the collapsed pill) with the dot/X hidden; as
    /// it opens, the capsule widens and the side controls fade in on the edges.
    /// Not an assertion test — leaves a PNG artifact.
    #[test]
    fn studio_expand_strip_renders_to_png() {
        use tiny_skia::PixmapPaint;

        let font = font();
        let (cw, ch) = studio_pixel_size();

        let mut asr = AsrDisplay::default();
        asr.append_commit(
            "Hey there, so I just want to know how the light transcription is working",
        );

        let steps = [0.0_f32, 0.35, 0.7, 1.0];
        let margin = 24i32;
        let gap = 16i32;
        let bw = cw + margin as u32 * 2;
        let bh =
            ch * steps.len() as u32 + margin as u32 * 2 + gap as u32 * (steps.len() as u32 - 1);
        let mut bg = Pixmap::new(bw, bh).unwrap();
        bg.fill(Color::from_rgba8(205, 203, 198, 255));

        let mut aura = Aura::new();
        for _ in 0..3 {
            aura.roll_studio_waveform(0.6);
        }
        for (i, &expand) in steps.iter().enumerate() {
            // The text only exists once expanded; while opening, height is minimal.
            let n = if expand > 0.5 {
                studio_line_count(&asr, Some(&font))
            } else {
                0
            };
            let card_h = studio_card_height(n);
            let mut card = Pixmap::new(cw, ch).unwrap();
            paint_studio_card(
                &mut card,
                &asr,
                PillState::Recording,
                1.0,
                12.0,
                Some(&font),
                card_h,
                n,
                &[],
                &aura.dots,
                expand,
            );
            let y = margin + i as i32 * (ch as i32 + gap);
            bg.draw_pixmap(
                margin,
                y,
                card.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }

        let path = std::env::temp_dir().join("grain_studio_expand.png");
        bg.save_png(&path).expect("save png");
        eprintln!("STUDIO_EXPAND_PNG={}", path.display());
    }

    /// [GRAIN] Render the expanded Studio card in PROMPT RECORD mode: the center
    /// waveform turns sky blue (the mode indicator); the transcript text is left
    /// plain white (no per-word tint). Not an assertion test — leaves a PNG artifact.
    #[test]
    fn studio_prompt_record_renders_to_png() {
        use tiny_skia::PixmapPaint;

        let font = font();
        let (cw, ch) = studio_pixel_size();

        let mut asr = AsrDisplay::default();
        asr.append_commit("Team sync notes from today Monday rewrite this as a formal email");
        asr.partial = "and keep it short".into();
        asr.partial_stable = false;
        let n = studio_line_count(&asr, Some(&font));
        let card_h = studio_card_height(n);

        let margin = 24i32;
        let bw = cw + margin as u32 * 2;
        let bh = ch + margin as u32 * 2;
        let mut bg = Pixmap::new(bw, bh).unwrap();
        bg.fill(Color::from_rgba8(205, 203, 198, 255));

        let mut aura = Aura::new();
        aura.prompt_recording = true;
        for _ in 0..3 {
            aura.roll_studio_waveform(0.6);
        }
        let mut card = Pixmap::new(cw, ch).unwrap();
        paint_studio_card(
            &mut card,
            &asr,
            PillState::Recording,
            1.0,
            12.0,
            Some(&font),
            card_h,
            n,
            &[],
            &aura.dots,
            1.0,
        );
        bg.draw_pixmap(
            margin,
            margin,
            card.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );

        let path = std::env::temp_dir().join("grain_studio_prompt_record.png");
        bg.save_png(&path).expect("save png");
        eprintln!("STUDIO_PROMPT_RECORD_PNG={}", path.display());
    }

    #[test]
    fn append_commit_accumulates_deltas_not_replaces() {
        // AsrCommit events carry only the newly-committed words (deltas). The
        // display must accumulate them into the running committed prefix — the
        // old `committed = text` collapsed it to just the last delta.
        let mut d = AsrDisplay::default();
        d.append_commit("hello");
        d.append_commit("there");
        d.append_commit("friend");
        assert_eq!(d.committed, "hello there friend");

        // Empty/whitespace deltas are ignored and never inject stray spaces.
        d.append_commit("   ");
        assert_eq!(d.committed, "hello there friend");

        // Committed words all render crisp (no blur / no gray).
        let runs = d.runs();
        assert!(runs.iter().all(|(_, s)| matches!(s, RunStyle::Committed)));
        assert_eq!(runs.len(), 3);
    }

    #[test]
    fn asr_display_runs_orders_finished_then_committed_then_partial() {
        let mut d = AsrDisplay::default();
        d.finished.push("hello world".to_string());
        d.committed = "this is".to_string();
        d.partial = "a test".to_string();
        d.partial_stable = true;

        let runs = d.runs();
        let words: Vec<&str> = runs.iter().map(|(w, _)| w.as_str()).collect();
        assert_eq!(words, vec!["hello", "world", "this", "is", "a", "test"]);

        // Finished + committed are solid; partial carries the stability flag.
        assert!(matches!(runs[0].1, RunStyle::Committed));
        assert!(matches!(runs[3].1, RunStyle::Committed));
        assert!(matches!(runs[4].1, RunStyle::Partial { stable: true }));
    }

    #[test]
    fn wrap_runs_breaks_at_max_width_not_mid_word() {
        let font = font();
        let px = 16.0;
        let runs: Vec<(String, RunStyle)> = "one two three four five six seven"
            .split_whitespace()
            .map(|w| (w.to_string(), RunStyle::Committed))
            .collect();

        // Narrow enough that every line must hold only a couple of words.
        let max_w = 80.0;
        let lines = wrap_runs(&font, &runs, px, max_w);

        assert!(
            lines.len() > 1,
            "narrow width must wrap onto multiple lines"
        );
        // No word is split: every original word appears exactly once, in order.
        let rebuilt: Vec<&str> = lines
            .iter()
            .flat_map(|l| l.words.iter().map(|(w, _)| w.as_str()))
            .collect();
        let expected: Vec<&str> = "one two three four five six seven"
            .split_whitespace()
            .collect();
        assert_eq!(rebuilt, expected);
    }

    #[test]
    fn wrap_runs_single_line_when_width_is_generous() {
        let font = font();
        let runs: Vec<(String, RunStyle)> = vec![
            ("hello".to_string(), RunStyle::Committed),
            ("world".to_string(), RunStyle::Committed),
        ];
        let lines = wrap_runs(&font, &runs, 16.0, 10_000.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].words.len(), 2);
    }

    #[test]
    fn box_blur_radius_zero_is_noop() {
        let mut bmp = vec![0u8, 255, 0, 255, 0, 255, 0, 255, 0];
        let before = bmp.clone();
        box_blur(&mut bmp, 3, 3, 0);
        assert_eq!(bmp, before);
    }

    #[test]
    fn box_blur_smooths_a_sharp_edge() {
        // A 1-pixel-wide bright column in a dark 5x1 row.
        let mut bmp = vec![0u8, 0, 255, 0, 0];
        box_blur(&mut bmp, 5, 1, 1);
        // The center is averaged down (no longer fully bright) and its
        // neighbors picked up some of its brightness (no longer fully dark) —
        // i.e. the edge actually got softer, not just shuffled.
        assert!(bmp[2] < 255, "center should have softened: {bmp:?}");
        assert!(
            bmp[1] > 0,
            "left neighbor should pick up some brightness: {bmp:?}"
        );
        assert!(
            bmp[3] > 0,
            "right neighbor should pick up some brightness: {bmp:?}"
        );
    }

    #[test]
    fn rounded_rect_path_is_some_for_sane_dimensions() {
        assert!(rounded_rect_path(0.0, 0.0, 100.0, 50.0, 12.0).is_some());
        // Radius larger than half the smallest dimension is clamped, not rejected.
        assert!(rounded_rect_path(0.0, 0.0, 20.0, 10.0, 999.0).is_some());
    }
}
