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

use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use grain_core::settings::OverlayPosition;
use grain_core::DaemonEvent;

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};
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
    for path in ["C:/Windows/Fonts/consola.ttf", "C:/Windows/Fonts/segoeui.ttf"] {
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
    text.chars().map(|ch| font.metrics(ch, px).advance_width).sum()
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
        CachedText { total_width, glyphs }
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
                let blend = |s: u8, d: u8| -> u8 {
                    ((s as f32 * ga) + (d as f32 * inv)).min(255.0) as u8
                };
                data[o] = blend(color[0], data[o]);
                data[o + 1] = blend(color[1], data[o + 1]);
                data[o + 2] = blend(color[2], data[o + 2]);
                data[o + 3] = ((255.0 * ga) + (data[o + 3] as f32 * inv)).min(255.0) as u8;
            }
        }
        pen += m.advance_width;
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
                Some(Presenter { hwnd, mem, dib, old, bits: bits as *mut u8, w, h })
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
                let size = SIZE { cx: self.w, cy: self.h };
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
}

impl Default for Remote {
    fn default() -> Self {
        Remote {
            state: PillState::Idle,
            visible: false,
            anchor: OverlayPosition::Bottom,
            prompt_name: String::new(),
            prompt_seq: 0,
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
        DaemonEvent::RecordingStarted { .. } => {
            r.state = PillState::Recording;
            r.visible = can_show(&r);
            eprintln!("event: RecordingStarted -> show (recording)");
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
        _ => {} // AudioLevel / etc. — not a state change
    }
}

/// Connect to the core's local event WS and drive `remote` from DaemonEvents.
/// Reconnects forever — the pill is always-on; the core may come and go.
/// Sends a `UserEvent::Wake` to the winit loop on every session state change so
/// the pill surfaces without waiting for the next HIDDEN_TICK (up to 80 ms).
fn spawn_event_client(remote: Arc<Mutex<Remote>>, proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
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
        }
    }

    fn win_size() -> (u32, u32) {
        (
            (COLS as f32 * CELL * SCALE).round() as u32,
            ((ROWS as f32 + RISER_RESERVE) * CELL * SCALE).round() as u32,
        )
    }

    /// [GRAIN] Place the pill on the monitor under it (or primary) per the user's
    /// `overlay_position`: centered horizontally, near the top or bottom edge.
    /// Recomputed on each show so it follows the active monitor + setting changes.
    fn position_window(window: &Window, anchor: OverlayPosition) {
        let (w, h) = Self::win_size();
        let Some(mon) = window.current_monitor().or_else(|| window.primary_monitor()) else {
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
                let bottom = work_area_bottom(mp.x + (ms.width / 2) as i32, mp.y + (ms.height / 2) as i32)
                    .unwrap_or(screen_bottom);
                bottom - h as i32 - margin
            }
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

    fn render(&mut self) {
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
            pixmap.fill_path(&PathBuilder::from_rect(rect), &body, FillRule::Winding, Transform::identity(), None);
        }
        for cx in [x0 + r, x1 - r] {
            if let Some(circle) = PathBuilder::from_circle(cx, y_off + r, r) {
                pixmap.fill_path(&circle, &body, FillRule::Winding, Transform::identity(), None);
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
                    pixmap.fill_path(&circle, &paint, FillRule::Winding, Transform::identity(), None);
                }
            }
        }

        if let Some(presenter) = &self.presenter {
            presenter.blit(&pixmap);
        }
        self.pixmap = Some(pixmap); // keep it for next frame
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
            pixmap.fill_path(&PathBuilder::from_rect(rect), &p, FillRule::Winding, Transform::identity(), None);
        }
        if let Some(rect) = Rect::from_ltrb(px0 + rr, bar_top, px1 - rr, bar_bottom) {
            pixmap.fill_path(&PathBuilder::from_rect(rect), &p, FillRule::Winding, Transform::identity(), None);
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
            
            draw_cached_text_centered(pixmap, &cached_left, (lx, cy), font_px, [236, 229, 218], alpha);
            draw_cached_text_centered(pixmap, &cached_right, (rx, cy), font_px, [236, 229, 218], alpha);
            
            if let Some(cached_label) = &self.cached_label {
                draw_cached_text_centered(pixmap, cached_label, (w / 2.0, cy), font_px, [236, 229, 218], alpha);
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
        Self::position_window(&window, anchor);

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
                    self.prompt_idx = (self.prompt_idx + self.prompts.len() - 1) % self.prompts.len();
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
            let becoming_visible = want_visible && !self.visible;
            let becoming_hidden = !want_visible && self.visible;
            self.visible = want_visible;

            // Snap the tick deadline to now so becoming_visible renders immediately
            // (UserEvent::Wake already shortened the sleep; this is the safety net).
            if becoming_visible {
                self.next_tick = now;
            }

            if becoming_hidden {
                // Release the mic device the moment the pill goes away.
                self._mic = None;
                if let Some(window) = &self.window {
                    eprintln!("window: hide");
                    present::hide_window(window);
                }
            }

            // Open the mic only while visible (just-in-time), so the first frame
            // can already react to the speaker.
            if becoming_visible && self._mic.is_none() {
                self._mic = start_mic(self.amp.clone());
                if self._mic.is_none() {
                    eprintln!("no microphone — falling back to a simulated signal");
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
                        Self::position_window(window, r.anchor);
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
        use windows::Win32::System::Threading::{GetCurrentProcess, SetPriorityClass, HIGH_PRIORITY_CLASS};
        use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
        unsafe {
            let _ = SetCurrentProcessExplicitAppUserModelID(windows::core::w!("com.punitdethe.grain"));
            let _ = SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS);
        }
    }
    eprintln!("pill: starting (pid {})", std::process::id());
    let event_loop: EventLoop<UserEvent> = EventLoop::with_user_event().build().expect("create event loop");
    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    event_loop.run_app(&mut app).expect("run pill");
}
