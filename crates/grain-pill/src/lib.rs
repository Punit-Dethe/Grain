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
//! Keys (standalone preview): R recording · P processing · I idle · Esc quit.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use grain_core::settings::OverlayPosition;
use grain_core::{DaemonEvent, SessionMode};

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Transform};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, WindowEvent};
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

// Prompt riser: a second capsule that slides up from behind the pill carrying
// the active prompt name during a mid-speech prompt switch.
const RISER_RESERVE: f32 = 5.0; // grid-cells reserved ABOVE the pill for the riser
const RISER_PEEK: f32 = 4.2; // grid-cells of riser visible when fully shown
const RISER_HOLD: Duration = Duration::from_millis(1600);
// Present (and ease the riser/hover) at ~60 fps so motion is smooth; the dot
// field itself only re-rolls every ROLL_INTERVAL so it keeps its calm cadence
// instead of turning into 60 fps static.
const TICK: Duration = Duration::from_millis(16);
const ROLL_INTERVAL: Duration = Duration::from_millis(80);

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
/// `SessionMode::NativeAsr`. The window is resized + repositioned on the rare
/// transitions between the two (never per-frame).
#[derive(Clone, Copy, PartialEq, Eq)]
enum PillMode {
    Collapsed,
    Studio,
}

// ── Studio Window geometry ──────────────────────────────────────────────────
//
// [GRAIN] Modeled on Handy's live-transcription overlay (upstream
// `src/overlay/RecordingOverlay.css`): the live transcript flows in the TOP
// region (bottom-anchored, newest line lowest) and dissolves into the card's
// dark surface at the top edge, while a single control row sits pinned at the
// BOTTOM — recording dot (left) · reactive waveform (center) · elapsed timer +
// cancel glyph (right). Sizes are one modular scale off STUDIO_PAD + the line
// rhythm so nothing looks "off".
const STUDIO_W: f32 = 452.0;
const STUDIO_PAD: f32 = 18.0; // horizontal inset for text + control row
const STUDIO_CORNER_R: f32 = 16.0;
const STUDIO_TOP_PAD: f32 = 14.0; // breathing room above the first transcript line
// Transcript type scale — a comfortable italic caption body (Handy: 15px/1.35).
const STUDIO_TEXT_PX: f32 = 15.5;
const STUDIO_LINE_HEIGHT: f32 = 21.0;
// Handy caps the live text at ~4 lines; older lines scroll up and fade out.
const STUDIO_MAX_LINES: usize = 4;
// Height of the top dissolve band (older lines fade into the dark surface).
const STUDIO_FADE_PX: f32 = 20.0;
// Bottom control row (dot · waveform · timer + cancel). Handy `--ov-base-h`.
const STUDIO_CTRL_H: f32 = 40.0;
// Card height = top pad + N transcript lines + control row. No trailing pad: the
// control row already carries its own vertical centering.
const STUDIO_H: f32 =
    STUDIO_TOP_PAD + STUDIO_LINE_HEIGHT * STUDIO_MAX_LINES as f32 + STUDIO_CTRL_H;

// [GRAIN] Grain's brand accent (the pill's orange), reused for the live overlay's
// dot / waveform / timer so the Studio Window matches the collapsed capsule.
const ACCENT: [u8; 3] = [255, 93, 30];

// ── Bottom control-row waveform (replaces the old dot-matrix equalizer) ──────
// A simple reactive bar meter, centered in the control row — Handy's `.swave`.
const WAVE_BARS: usize = 9;
const WAVE_BAR_W: f32 = 4.0;
const WAVE_GAP: f32 = 3.0;
const WAVE_MAX_H: f32 = 18.0;
const WAVE_MIN_H: f32 = 3.0;

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

        self.clear_to([12, 12, 12, 255]); // only lit pixels appear; unlit stay near-black dark grey
        for (k, &idx) in eligible.iter().enumerate() {
            if k >= active_count {
                break;
            }
            if k < hot_count {
                self.dots[idx] = [189, 193, 201, 235];
            } else {
                let a = (0.34 + lit_base * 0.30 + self.rng.f32() * flicker).min(0.82);
                let g = self.rng.f32();
                let (rr, gg, bb) = if g < 0.33 {
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

/// Load a system monospace/UI font for the riser label (Windows paths).
fn load_font() -> Option<fontdue::Font> {
    for path in [
        "C:/Windows/Fonts/consola.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                return Some(font);
            }
        }
    }
    None
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
fn paint_studio_card(
    pixmap: &mut Pixmap,
    asr: &AsrDisplay,
    state: PillState,
    fade: f32,
    phase: f32,
    amp: f32,
    elapsed_secs: u64,
    font: Option<&fontdue::Font>,
) {
    let (w, h) = studio_pixel_size();
    let (wf, hf) = (w as f32, h as f32);
    pixmap.fill(Color::TRANSPARENT);

    // 1) Card background: a near-black panel with a 1px inner top highlight so
    // it reads as a raised premium surface, not a flat rectangle.
    let mut bg = Paint {
        anti_alias: true,
        ..Default::default()
    };
    bg.set_color(Color::from_rgba8(13, 13, 15, (244.0 * fade) as u8));
    if let Some(path) = rounded_rect_path(0.0, 0.0, wf, hf, STUDIO_CORNER_R) {
        pixmap.fill_path(&path, &bg, FillRule::Winding, Transform::identity(), None);
    }
    let mut hair = Paint {
        anti_alias: true,
        ..Default::default()
    };
    hair.set_color(Color::from_rgba8(255, 255, 255, (14.0 * fade) as u8));
    if let Some(rect) = Rect::from_ltrb(STUDIO_CORNER_R, 0.5, wf - STUDIO_CORNER_R, 1.5) {
        pixmap.fill_path(
            &PathBuilder::from_rect(rect),
            &hair,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // 2) Live transcript — top region, bottom-anchored, top edge dissolving.
    let ctrl_top = hf - STUDIO_CTRL_H;
    if let Some(font) = font {
        // Leave a hair of space so descenders never touch the control row.
        draw_transcript(pixmap, asr, font, STUDIO_TOP_PAD, ctrl_top - 2.0, fade);
    }

    // 3) Control row pinned to the bottom.
    draw_control_row(pixmap, state, amp, phase, elapsed_secs, ctrl_top, wf, fade, font);
}

/// Studio pixel size — the single source of truth used by both the free painter
/// and `App::win_size_for(Studio)`.
fn studio_pixel_size() -> (u32, u32) {
    (
        (STUDIO_W * SCALE).round() as u32,
        (STUDIO_H * SCALE).round() as u32,
    )
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
                // Committed text is stable/pasteable → solid warm white, crisp,
                // and it never grays as it scrolls up (the user must be able to
                // trust the solid text). The uncommitted tail is the "still being
                // decided" region → dimmer but crisp (Handy renders its tentative
                // tail plainly; blurred per-frame text hurt readability).
                let (color, alpha): ([u8; 3], f32) = match style {
                    RunStyle::Committed => ([238, 236, 232], 0.97),
                    RunStyle::Partial { stable: true } => ([200, 203, 210], 0.66),
                    RunStyle::Partial { stable: false } => ([186, 190, 198], 0.50),
                };
                pen += draw_word(
                    layer, font, word, STUDIO_TEXT_PX, pen, baseline, color, alpha, 0,
                );
            }
        }

        // Dissolve the top band so older lines melt into the dark surface.
        fade_top_band(layer, STUDIO_FADE_PX);

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
fn draw_control_row(
    pixmap: &mut Pixmap,
    state: PillState,
    amp: f32,
    phase: f32,
    elapsed_secs: u64,
    ctrl_top: f32,
    wf: f32,
    fade: f32,
    font: Option<&fontdue::Font>,
) {
    let cy = ctrl_top + STUDIO_CTRL_H / 2.0;
    let recording = state == PillState::Recording;

    // LEFT — pulsing recording dot, or a spinner while finalizing.
    let left_cx = STUDIO_PAD + 6.0;
    if recording {
        draw_rec_dot(pixmap, left_cx, cy, phase, fade);
    } else {
        draw_spinner(pixmap, left_cx, cy, phase, fade);
    }

    // RIGHT — cancel glyph, with the elapsed timer to its left while recording.
    let x_cx = wf - STUDIO_PAD - 11.0; // 22px circle, 11px from the inset edge
    draw_x_button(pixmap, x_cx, cy, fade);
    if recording {
        if let Some(font) = font {
            let label = fmt_elapsed(elapsed_secs);
            let tw = text_width(font, &label, 12.0);
            let tx = x_cx - 11.0 - 10.0 - tw; // button half-width + gap
            draw_word(
                pixmap,
                font,
                &label,
                12.0,
                tx,
                cy + 12.0 * 0.34,
                [154, 152, 148],
                fade,
                0,
            );
        }
    }

    // CENTER — reactive waveform while recording, else a status label.
    if recording {
        draw_waveform(pixmap, amp, phase, wf, cy, fade);
    } else if let Some(font) = font {
        let label = match state {
            PillState::Processing => "Processing",
            PillState::Fallback => "Error",
            _ => "",
        };
        if !label.is_empty() {
            let tw = text_width(font, label, 12.5);
            draw_word(
                pixmap,
                font,
                label,
                12.5,
                (wf - tw) / 2.0,
                cy + 12.5 * 0.34,
                [176, 178, 184],
                fade,
                0,
            );
        }
    }
}

/// Elapsed recording time as `m:ss` (Handy's overlay timer format).
fn fmt_elapsed(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
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
        p.set_color(Color::from_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], (fade * 255.0) as u8));
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

/// The center waveform — `WAVE_BARS` reactive bars whose heights track the mic
/// level with a symmetric center-tall envelope plus a traveling ripple so it
/// never reads as static (Handy's `.swave`, driven from our single RMS level).
fn draw_waveform(pixmap: &mut Pixmap, amp: f32, phase: f32, wf: f32, cy: f32, fade: f32) {
    let total_w = WAVE_BARS as f32 * WAVE_BAR_W + (WAVE_BARS as f32 - 1.0) * WAVE_GAP;
    let x0 = (wf - total_w) / 2.0;
    let mut p = Paint {
        anti_alias: true,
        ..Default::default()
    };
    p.set_color(Color::from_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], (fade * 255.0) as u8));
    let half = (WAVE_BARS as f32 - 1.0) / 2.0;
    for i in 0..WAVE_BARS {
        // Center-tall envelope (edges 0.55 → center 1.0).
        let d = (i as f32 - half).abs() / half;
        let env = 0.55 + 0.45 * (1.0 - d);
        // Per-bar traveling ripple keeps the meter alive at steady levels.
        let ripple = 0.5 + 0.5 * (phase * 0.28 + i as f32 * 0.9).sin();
        let v = (amp * env * (0.55 + 0.45 * ripple)).clamp(0.0, 1.0);
        let bh = WAVE_MIN_H + v.powf(0.7) * (WAVE_MAX_H - WAVE_MIN_H);
        let x = x0 + i as f32 * (WAVE_BAR_W + WAVE_GAP);
        let y = cy - bh / 2.0;
        if let Some(path) = rounded_rect_path(x, y, WAVE_BAR_W, bh, WAVE_BAR_W / 2.0) {
            pixmap.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
        }
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
        SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WS_EX_LAYERED,
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
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
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
    /// [GRAIN] Native ASR: which surface to present (Studio Window vs the
    /// classic collapsed capsule), set from `RecordingStarted`'s `SessionMode`.
    mode: PillMode,
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
            // [GRAIN] Native ASR → the Studio Window; every other mode is the
            // classic collapsed capsule. Fresh `asr` buffer per session so a
            // prior dictation's text never bleeds into the next.
            r.mode = if mode == SessionMode::NativeAsr {
                PillMode::Studio
            } else {
                PillMode::Collapsed
            };
            r.asr = AsrDisplay::default();
            eprintln!("event: RecordingStarted -> show (recording, mode {mode:?})");
        }
        DaemonEvent::RecordingStopped { .. } => {
            r.state = PillState::Processing;
            r.visible = can_show(&r);
            eprintln!("event: RecordingStopped -> processing");
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
}

/// Connect to the core's local event WS and drive `remote` from DaemonEvents.
/// Reconnects forever — the pill is always-on; the core may come and go.
/// Sends a `UserEvent::Wake` to the winit loop on every session state change so
/// the pill surfaces without waiting for the next HIDDEN_TICK (up to 80 ms).
fn spawn_event_client(remote: Arc<Mutex<Remote>>, proxy: EventLoopProxy<UserEvent>) {
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
            use futures_util::StreamExt;
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
                let (_w, mut read) = ws.split();
                while let Some(Ok(msg)) = read.next().await {
                    if let Message::Text(txt) = msg {
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
                            );
                            apply_event(&remote, ev);
                            if is_session_event {
                                let _ = proxy.send_event(UserEvent::Wake);
                            }
                        }
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

// ── App ─────────────────────────────────────────────────────────────────────

struct App {
    window: Option<Rc<Window>>,
    aura: Aura,
    state: PillState,
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
    riser_progress: f32,
    riser_hide_at: Option<Instant>,
    next_tick: Instant,
    next_roll: Instant,
    remote: Arc<Mutex<Remote>>,
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
    /// [GRAIN] Studio recording timer. `studio_since` is set when a session
    /// enters Recording; `studio_elapsed` is refreshed from it each frame while
    /// recording and then FROZEN once we leave Recording (the timer stops at the
    /// value it had, matching Handy's overlay, rather than resetting to 0).
    studio_since: Option<Instant>,
    studio_elapsed: u64,
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
        spawn_event_client(remote.clone(), proxy);
        App {
            window: None,
            aura: Aura::new(),
            state: PillState::Idle,
            amp,
            _mic: None,
            sim_target: 0.5,
            sim_amp: 0.0,
            font: load_font(),
            prompts: ["General", "Email Format", "Meeting Notes", "Translation"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            prompt_idx: 0,
            prompt_label: String::new(),
            cached_label: None,
            last_prompt_seq: 0,
            prompt_preview_until: None,
            riser_progress: 0.0,
            riser_hide_at: None,
            next_tick: Instant::now(),
            next_roll: Instant::now(),
            remote,
            visible: false,
            presenter: None,
            pixmap: None,
            mode: PillMode::Collapsed,
            asr: AsrDisplay::default(),
            studio_alpha: 0.0,
            closing: false,
            studio_phase: 0.0,
            studio_since: None,
            studio_elapsed: 0,
        }
    }

    fn win_size() -> (u32, u32) {
        (
            (COLS as f32 * CELL * SCALE).round() as u32,
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
        }
    }

    /// [GRAIN] Place the pill on the monitor under it (or primary) per the user's
    /// `overlay_position`: centered horizontally, near the top or bottom edge.
    /// Recomputed on each show so it follows the active monitor + setting changes.
    fn position_window(window: &Window, anchor: OverlayPosition, w: u32, h: u32) {
        let Some(mon) = window
            .current_monitor()
            .or_else(|| window.primary_monitor())
        else {
            return;
        };
        let ms = mon.size();
        let mp = mon.position();
        let margin = (16.0 * SCALE) as i32;
        let x = mp.x + ((ms.width.saturating_sub(w)) / 2) as i32;
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

    fn update_cached_label(&mut self) {
        if let Some(font) = &self.font {
            let cell_px = Self::cell_px();
            let peek = RISER_PEEK * cell_px;
            let font_px = peek * 0.6;
            let pad = peek * 0.4;
            let (w, _) = Self::win_size();
            let arrow_inset = peek * 0.85;
            let (px0, px1) = (cell_px, w as f32 - cell_px);

            let (lx, rx) = (px0 + arrow_inset, px1 - arrow_inset);
            let label_max = ((rx - lx) - 2.0 * pad).max(0.0);
            let truncated = truncate_to_width(font, &self.prompt_label, font_px, label_max);
            self.cached_label = Some(CachedText::new(font, &truncated, font_px));
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
        match self.mode {
            PillMode::Collapsed => self.render_collapsed(),
            PillMode::Studio => self.render_studio(),
        }
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
        let (x0, x1) = (cell_px, w as f32 - cell_px);

        // 1) Prompt riser (drawn first, so the pill body hides its lower half).
        if self.riser_progress > 0.01 {
            self.draw_riser(&mut pixmap, w as f32, cell_px, y_off);
        }

        // 2) Floating capsule body (offset below the riser reserve).
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

        // 3) Dots.
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

        if let Some(presenter) = &self.presenter {
            presenter.blit(&pixmap);
        }
        self.pixmap = Some(pixmap); // keep it for next frame
    }

    /// The Native ASR Studio Window — a compact, deliberately-proportioned
    /// caption card. Top-left: a small tracked status eyebrow. Top-right: a
    /// frameless dot-matrix equalizer (the pill's dot-pixel language, sleeker
    /// and without the capsule). Below: the live transcript, word-wrapped, with
    /// committed text solid and the volatile tentative tail dimmed but crisp
    /// (it keeps the preview moving between the engine's commit points).
    fn render_studio(&mut self) {
        if self.window.is_none() {
            return;
        }
        let (w, h) = Self::win_size_for(PillMode::Studio);
        let mut pixmap = self
            .pixmap
            .take()
            .unwrap_or_else(|| Pixmap::new(w, h).unwrap());

        // Advance the equalizer clock once per frame (smooth, cadence-free).
        self.studio_phase += 1.0;
        let fade = self.studio_alpha.clamp(0.0, 1.0);
        let amp = self.current_amp();

        // All drawing lives in the windowing-free `paint_studio_card` so it can
        // be rendered to a PNG in tests without a winit window/presenter.
        paint_studio_card(
            &mut pixmap,
            &self.asr,
            self.state,
            fade,
            self.studio_phase,
            amp,
            self.studio_elapsed,
            self.font.as_ref(),
        );

        if let Some(presenter) = &self.presenter {
            presenter.blit(&pixmap);
        }
        self.pixmap = Some(pixmap);
    }

    /// Draw the prompt riser bar (rounded top) and its label in the crescent
    /// visible above the pill, sliding up by `riser_progress`.
    ///
    /// Layout rules (per spec):
    /// - The bar's width matches the pill body's width EXACTLY (`px0..px1`).
    /// - The bar's bottom reaches 50% of the pill height (hidden behind the body),
    ///   giving the rounded-top crescent its full presence.
    /// - The `‹` / `›` arrows sit at FIXED inner positions; the label is centered
    ///   and truncated so it never moves the arrows or spills out.
    fn draw_riser(&self, pixmap: &mut Pixmap, w: f32, cell_px: f32, y_off: f32) {
        let pill_h = ROWS as f32 * cell_px;
        let peek = RISER_PEEK * cell_px;
        let bar_top = y_off - peek * self.riser_progress;

        // [GRAIN] Keep it fully opaque. By matching the corner radii perfectly,
        // it slides flawlessly behind the pill without any artifacts.
        let alpha = 1.0;

        let mut p = Paint {
            anti_alias: true,
            ..Default::default()
        };
        p.set_color(Color::from_rgba8(11, 11, 10, (235.0 * alpha) as u8));

        let (px0, px1) = (cell_px, w - cell_px);

        // Drop the bar bottom to 50% of the pill height (the body hides the rest).
        let bar_bottom = y_off + pill_h * 0.5;
        // [GRAIN] Match the pill's corner radius exactly! This ensures that when the
        // riser slides all the way down, its corners perfectly align with the pill's
        // corners, completely eliminating the "peeking" artifact without needing to fade!
        let rr = pill_h / 2.0;
        // Rounded-top bar = vertical rect + horizontal rect + two top circles.
        if let Some(rect) = Rect::from_ltrb(px0, bar_top + rr, px1, bar_bottom) {
            pixmap.fill_path(
                &PathBuilder::from_rect(rect),
                &p,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        if let Some(rect) = Rect::from_ltrb(px0 + rr, bar_top, px1 - rr, bar_bottom) {
            pixmap.fill_path(
                &PathBuilder::from_rect(rect),
                &p,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        for cx in [px0 + rr, px1 - rr] {
            if let Some(circle) = PathBuilder::from_circle(cx, bar_top + rr, rr) {
                pixmap.fill_path(&circle, &p, FillRule::Winding, Transform::identity(), None);
            }
        }

        if let Some(font) = &self.font {
            let font_px = peek * 0.6;
            // [GRAIN] Anchor the text rigidly to the top of the bar so it slides WITH the bar
            // exactly, instead of squishing/lagging as the visible crescent shrinks.
            let cy = bar_top + peek / 2.0;

            // Fixed arrow positions, anchored a constant inset from the bar edges.
            let arrow_inset = peek * 0.85;
            let lx = px0 + arrow_inset;
            let rx = px1 - arrow_inset;

            // Cache arrows and label on the fly if needed
            let cached_left = CachedText::new(font, "\u{2039}", font_px);
            let cached_right = CachedText::new(font, "\u{203a}", font_px);

            draw_cached_text_centered(
                pixmap,
                &cached_left,
                (lx, cy),
                font_px,
                [236, 229, 218],
                alpha,
            );
            draw_cached_text_centered(
                pixmap,
                &cached_right,
                (rx, cy),
                font_px,
                [236, 229, 218],
                alpha,
            );

            if let Some(cached_label) = &self.cached_label {
                draw_cached_text_centered(
                    pixmap,
                    cached_label,
                    (w / 2.0, cy),
                    font_px,
                    [236, 229, 218],
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
        Self::position_window(&window, anchor, w, h);

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
            // [GRAIN] Hover/click interactions removed — the pill is display-only.
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
            self.asr = r.asr.clone();

            // [GRAIN] Studio recording timer: advance while Recording, freeze the
            // instant we leave it (Processing keeps the final elapsed on screen).
            if self.state == PillState::Recording {
                let since = *self.studio_since.get_or_insert(now);
                self.studio_elapsed = now.saturating_duration_since(since).as_secs();
            } else {
                self.studio_since = None;
            }

            // [GRAIN] Resize/recreate the OS window the rare times the surface
            // actually changes (Collapsed <-> Studio) — never per frame. The
            // Presenter caches a fixed-size GDI bitmap, so it must be rebuilt
            // for the new size; the cached pixmap is invalidated too (wrong
            // dimensions otherwise).
            if r.mode != self.mode {
                self.mode = r.mode;
                if let Some(window) = &self.window {
                    let (w, h) = Self::win_size_for(self.mode);
                    // [GRAIN] winit 0.30 renamed this `request_inner_size` (the
                    // resize isn't always synchronous on every platform); on
                    // Windows it applies immediately, so the Presenter rebuilt
                    // right after is sized correctly for the very next frame.
                    let _ = window.request_inner_size(PhysicalSize::new(w, h));
                    self.presenter = present::Presenter::new(window, w as i32, h as i32);
                }
                self.pixmap = None;
                self.studio_alpha = 0.0;
                // Fresh session → reset the recording timer.
                self.studio_since = None;
                self.studio_elapsed = 0;
                // A mode change always means a brand-new session just started
                // (mode is only ever set from RecordingStarted) — never
                // mid-transition leftovers from whatever the previous surface
                // was doing, including an in-progress Studio close-fade, which
                // this intentionally cuts short rather than leaving stale.
                self.visible = false;
                self.closing = false;
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

            // Visible if the core says so OR we're inside a transient prompt preview.
            let preview_visible = self.prompt_preview_until.is_some_and(|t| now < t);
            let want_visible = r.visible || preview_visible;
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
                if self.mode == PillMode::Studio {
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
                    self.visible = false;
                    if let Some(window) = &self.window {
                        eprintln!("window: hide (studio fade complete)");
                        present::hide_window(window);
                    }
                }
            }

            if self.visible {
                // Re-roll the dot field on its own (slower) cadence so it stays
                // calm; everything else eases every frame for smoothness.
                if now >= self.next_roll {
                    let amp = self.current_amp();
                    self.aura.roll(self.state, amp);
                    self.next_roll = now + ROLL_INTERVAL;
                }
                // Ease the prompt riser, auto-hiding after RISER_HOLD.
                let riser_target = match self.riser_hide_at {
                    Some(t) if now < t => 1.0,
                    _ => 0.0,
                };
                self.riser_progress += (riser_target - self.riser_progress) * 0.12;
                if riser_target == 0.0 && self.riser_progress < 0.02 {
                    self.riser_progress = 0.0;
                    self.riser_hide_at = None;
                }
                // Push the layered content FIRST (a layered window shows nothing
                // until UpdateLayeredWindow runs) …
                self.render();
                // … then reveal it without stealing focus.
                if becoming_visible {
                    if let Some(window) = &self.window {
                        // Re-anchor each show so a changed setting / active monitor
                        // takes effect immediately.
                        let (w, h) = Self::win_size_for(self.mode);
                        Self::position_window(window, r.anchor, w, h);
                        eprintln!("window: show (content primed)");
                        present::show_window(window);
                    }
                }
            }
            // 60 fps only while visible; sleep forever when hidden (woken by UserEvent::Wake).
            if self.visible {
                self.next_tick = now + TICK;
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
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
            let _ =
                SetCurrentProcessExplicitAppUserModelID(windows::core::w!("com.punitdethe.grain"));
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
        // A minimal valid TTF (DejaVu-style placeholder isn't bundled with the
        // crate); fall back to a system font like `load_font()` does, but fail
        // loudly in CI rather than silently skipping — these tests only assert
        // properties that hold for any monospace/UI font.
        load_font().expect("a system font must be available to test text layout")
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
            paint_studio_card(&mut card, &asr, state, 1.0, 12.0, 0.6, 18, Some(&font));
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
