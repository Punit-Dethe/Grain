//! [GRAIN] The resting Agent notch: a thin sliver fused to the top-centre of the
//! primary monitor, always on screen while the Agent is enabled.
//!
//! No WebView: it is a layered Win32 popup painted from a few kilobytes of
//! premultiplied pixels, created on the main thread so tao's message loop
//! drives it. It costs nothing at idle (no timer, no hook, no polling). While
//! the pointer rests on it a light bar fills left to right; when the bar is
//! full (or on a click) the Agent notch opens from this same spot. The WebView
//! is pre-created when the hover starts and destroyed again if it is abandoned.
//!
//! Windows only for now; elsewhere every call is a no-op.

use tauri::AppHandle;

/// Create or destroy the sliver to match `agent_enabled`.
pub fn sync(app: &AppHandle) {
    #[cfg(windows)]
    imp::sync(app);
    #[cfg(not(windows))]
    let _ = app;
}

/// [GRAIN] A chat request runs in the background: the sliver's line animates
/// an orange-to-yellow gradient until it ends.
pub fn set_working(app: &AppHandle, working: bool) {
    #[cfg(windows)]
    imp::set_working(app, working);
    #[cfg(not(windows))]
    let _ = (app, working);
}

/// Hide the sliver while the notch itself is on screen; show it again after.
pub fn set_hidden(app: &AppHandle, hidden: bool) {
    #[cfg(windows)]
    imp::set_hidden(app, hidden);
    #[cfg(not(windows))]
    let _ = (app, hidden);
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use tauri::AppHandle;
    use windows::core::w;
    use windows::Win32::Foundation::{
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        BLENDFUNCTION, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, KillTimer, LoadCursorW, RegisterClassW,
        SetTimer, ShowWindow, UpdateLayeredWindow, IDC_HAND, MA_NOACTIVATE, SW_HIDE,
        SW_SHOWNOACTIVATE, ULW_ALPHA, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_LBUTTONDOWN,
        WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    /// The sliver, in CSS px. It never changes size: the bar fills inside it.
    /// `.agent-notch.is-entering` in agent.css starts the WebView shell at
    /// this size and colour so the hand-off is seamless.
    const REST: (f64, f64) = (120.0, 6.0);
    /// The hover target: the window, hit-testable but invisible around the
    /// shape so the pointer need not land on the few visible pixels.
    const HIT: (f64, f64) = (180.0, 12.0);
    /// How long the pointer must rest before the notch opens.
    const FILL: Duration = Duration::from_millis(650);
    const TIMER_ID: usize = 1;
    /// Animates the working line (~30 fps) only while a request runs.
    const WORK_TIMER_ID: usize = 2;
    /// One sweep of the working gradient; `agent-notch-work` in agent.css.
    const WORK_PERIOD: f64 = 1.4;
    /// `WM_MOUSELEAVE` (winuser.h); its binding lives in the unused Controls feature.
    const WM_MOUSELEAVE: u32 = 0x02A3;
    const CLASS: windows::core::PCWSTR = w!("GrainAgentNotchSliver");

    /// `--notch-sliver` in agent.css.
    const BASE: [f64; 3] = [3.0, 3.0, 5.0];
    const TRACK: [f64; 3] = [30.0, 30.0, 35.0];
    const BAR: [f64; 3] = [206.0, 206.0, 214.0];
    /// The working gradient, cycled: orange, amber, yellow.
    const WORK: [[f64; 3]; 3] = [
        [255.0, 122.0, 24.0],
        [255.0, 179.0, 71.0],
        [255.0, 214.0, 102.0],
    ];

    static WORKING: AtomicBool = AtomicBool::new(false);

    struct Sliver {
        hwnd: HWND,
        app: AppHandle,
        hover_since: Option<Instant>,
        /// The open fired for this hover; leaving must not cancel the pre-warm.
        fired: bool,
        hidden: bool,
        /// Cached on the main loop: the window procedure never calls Tauri.
        geometry: Option<Geometry>,
        /// Phase origin of the working gradient.
        epoch: Instant,
    }

    #[derive(Clone, Copy)]
    struct Geometry {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        zoom: f64,
    }

    fn measure(app: &AppHandle) -> Option<Geometry> {
        let monitor = app.primary_monitor().ok().flatten()?;
        let zoom = monitor.scale_factor() * crate::agent::windows_notch_text_scale();
        let area = monitor.work_area();
        let width = (HIT.0 * zoom).round().max(1.0) as i32;
        let height = (HIT.1 * zoom).round().max(1.0) as i32;
        Some(Geometry {
            x: area.position.x + (area.size.width as i32 - width) / 2,
            y: area.position.y,
            width,
            height,
            zoom,
        })
    }

    /// Re-measure the monitor and repaint. Main loop only (calls Tauri).
    fn reposition() {
        let Some(app) = SLIVER.with(|cell| Some(cell.try_borrow().ok()?.as_ref()?.app.clone()))
        else {
            return;
        };
        let geometry = measure(&app);
        SLIVER.with(|cell| {
            if let Some(sliver) = cell.borrow_mut().as_mut() {
                sliver.geometry = geometry;
            }
        });
        repaint();
    }

    /// Run `work` on the main loop, outside this window's message dispatch:
    /// Tauri window work from inside a raw window procedure can deadlock.
    fn post(app: &AppHandle, work: fn(&AppHandle)) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || work(&handle));
    }

    thread_local! {
        // Only ever touched on the main thread, which also pumps its messages.
        static SLIVER: RefCell<Option<Sliver>> = const { RefCell::new(None) };
    }

    static REGISTERED: AtomicBool = AtomicBool::new(false);

    pub fn sync(app: &AppHandle) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let enabled = crate::settings::get_settings(&handle).agent_enabled;
            let exists = SLIVER.with(|cell| cell.borrow().is_some());
            if enabled && !exists {
                create(&handle);
            } else if !enabled && exists {
                destroy();
            }
        });
    }

    pub fn set_hidden(app: &AppHandle, hidden: bool) {
        let _ = app.run_on_main_thread(move || {
            let Some(hwnd) = SLIVER.with(|cell| {
                let mut slot = cell.try_borrow_mut().ok()?;
                let sliver = slot.as_mut()?;
                if sliver.hidden == hidden {
                    return None;
                }
                sliver.hidden = hidden;
                sliver.hover_since = None;
                sliver.fired = false;
                Some(sliver.hwnd)
            }) else {
                return;
            };
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_ID);
                if hidden {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                } else {
                    reposition();
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
            }
            sync_work_timer();
        });
    }

    pub fn set_working(app: &AppHandle, working: bool) {
        if WORKING.swap(working, Ordering::SeqCst) == working {
            return;
        }
        let _ = app.run_on_main_thread(|| {
            sync_work_timer();
            repaint();
        });
    }

    /// The working line ticks only while a request runs and the sliver shows.
    fn sync_work_timer() {
        let Some((hwnd, hidden)) = SLIVER.with(|cell| {
            let slot = cell.try_borrow().ok()?;
            let sliver = slot.as_ref()?;
            Some((sliver.hwnd, sliver.hidden))
        }) else {
            return;
        };
        unsafe {
            if WORKING.load(Ordering::SeqCst) && !hidden {
                SetTimer(Some(hwnd), WORK_TIMER_ID, 33, None);
            } else {
                let _ = KillTimer(Some(hwnd), WORK_TIMER_ID);
            }
        }
    }

    fn create(app: &AppHandle) {
        unsafe {
            let Ok(module) = GetModuleHandleW(None) else {
                return;
            };
            let instance = HINSTANCE(module.0);
            if !REGISTERED.swap(true, Ordering::SeqCst) {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: instance,
                    hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
                    lpszClassName: CLASS,
                    ..Default::default()
                };
                if RegisterClassW(&class) == 0 {
                    REGISTERED.store(false, Ordering::SeqCst);
                    log::warn!("[GRAIN] notch sliver: class registration failed");
                    return;
                }
            }
            let hwnd = match CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                CLASS,
                w!("Grain Agent"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance),
                None,
            ) {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    log::warn!("[GRAIN] notch sliver: window creation failed: {error}");
                    return;
                }
            };
            // The notch may already be on screen (an Agent summoned before the
            // sliver existed); it shows the sliver again when it closes.
            let hidden = tauri::Manager::get_webview_window(app, crate::agent::PANEL_LABEL)
                .is_some_and(|window| window.is_visible().unwrap_or(false));
            SLIVER.with(|cell| {
                *cell.borrow_mut() = Some(Sliver {
                    hwnd,
                    app: app.clone(),
                    hover_since: None,
                    fired: false,
                    hidden,
                    geometry: measure(app),
                    epoch: Instant::now(),
                });
            });
            repaint();
            if !hidden {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
        sync_work_timer();
    }

    fn destroy() {
        let Some(sliver) = SLIVER.with(|cell| cell.borrow_mut().take()) else {
            return;
        };
        unsafe {
            let _ = KillTimer(Some(sliver.hwnd), TIMER_ID);
            let _ = KillTimer(Some(sliver.hwnd), WORK_TIMER_ID);
            let _ = DestroyWindow(sliver.hwnd);
        }
    }

    struct Frame {
        hwnd: HWND,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        pixels: Vec<u32>,
    }

    /// Build the current frame without holding the cell across Win32 calls.
    fn frame() -> Option<Frame> {
        SLIVER.with(|cell| {
            let slot = cell.try_borrow().ok()?;
            let sliver = slot.as_ref()?;
            let Geometry {
                x,
                y,
                width,
                height,
                zoom,
            } = sliver.geometry?;
            let progress = sliver
                .hover_since
                .map(|since| (since.elapsed().as_secs_f64() / FILL.as_secs_f64()).min(1.0));
            Some(Frame {
                hwnd: sliver.hwnd,
                x,
                y,
                width,
                height,
                pixels: render(
                    width,
                    height,
                    zoom,
                    progress,
                    WORKING
                        .load(Ordering::SeqCst)
                        .then(|| (sliver.epoch.elapsed().as_secs_f64() / WORK_PERIOD).fract()),
                ),
            })
        })
    }

    fn repaint() {
        if let Some(frame) = frame() {
            unsafe { present(&frame) };
        }
    }

    /// Fraction of the pixel span `[p - 0.5, p + 0.5]` inside `[lo, hi]`.
    fn span(p: f64, lo: f64, hi: f64) -> f64 {
        ((p + 0.5).min(hi) - (p - 0.5).max(lo)).clamp(0.0, 1.0)
    }

    /// The cycled working gradient at `t` in [0, 1).
    fn work_color(t: f64) -> [f64; 3] {
        let scaled = t * WORK.len() as f64;
        let index = (scaled.floor() as usize) % WORK.len();
        let next = (index + 1) % WORK.len();
        let f = scaled.fract();
        // Smoothstep between stops: no visible seams in the sweep.
        let f = f * f * (3.0 - 2.0 * f);
        let mut color = [0.0; 3];
        for i in 0..3 {
            color[i] = WORK[index][i] + (WORK[next][i] - WORK[index][i]) * f;
        }
        color
    }

    /// Premultiplied BGRA pixels: the shape flush with the top edge, bottom
    /// corners rounded and anti-aliased, the working line (while a request
    /// runs, `work` is its phase) and the hover fill bar. Everything else is
    /// alpha 1/255: invisible, but it still takes the hover.
    fn render(
        width: i32,
        height: i32,
        zoom: f64,
        progress: Option<f64>,
        work: Option<f64>,
    ) -> Vec<u32> {
        let (sw, sh) = (REST.0 * zoom, REST.1 * zoom);
        // Fully rounded bottom corners: the radius is the whole height.
        let radius = sh;
        let half = sw / 2.0;
        let cx = width as f64 / 2.0;
        let pad = 9.0 * zoom;
        let (bar_top, bar_bottom) = (1.75 * zoom, 4.0 * zoom);
        let (bar_left, bar_right) = (cx - half + pad, cx + half - pad);
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            let py = y as f64 + 0.5;
            for x in 0..width {
                let px = x as f64 + 0.5;
                let dx = (px - cx).abs();
                let distance = if py <= sh - radius {
                    dx - half
                } else {
                    let qx = dx - (half - radius);
                    let qy = py - (sh - radius);
                    if qx <= 0.0 {
                        py - sh
                    } else {
                        (qx * qx + qy * qy).sqrt() - radius
                    }
                };
                let coverage = (0.5 - distance).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    pixels.push(1 << 24);
                    continue;
                }
                let mut color = BASE;
                if let Some(phase) = work {
                    let line = span(py, bar_top, bar_bottom) * span(px, bar_left, bar_right);
                    if line > 0.0 {
                        let along = (px - bar_left) / (bar_right - bar_left);
                        let hue = work_color((along - phase).rem_euclid(1.0));
                        for i in 0..3 {
                            color[i] += (hue[i] - color[i]) * line;
                        }
                    }
                }
                if let Some(progress) = progress {
                    let row = span(py, bar_top, bar_bottom);
                    if row > 0.0 {
                        let eased = 1.0 - (1.0 - progress).powi(2);
                        let filled = bar_left + (bar_right - bar_left) * eased;
                        // Over the working line the hover fill alone shows.
                        let track = if work.is_some() {
                            0.0
                        } else {
                            row * span(px, bar_left, bar_right)
                        };
                        let fill = row * span(px, bar_left, filled);
                        for i in 0..3 {
                            color[i] += (TRACK[i] - color[i]) * track;
                            color[i] += (BAR[i] - color[i]) * fill;
                        }
                    }
                }
                let alpha = (coverage * 255.0).round().max(1.0);
                let channel = |c: f64| ((c * coverage).round() as u32).min(alpha as u32);
                pixels.push(
                    ((alpha as u32) << 24)
                        | (channel(color[0]) << 16)
                        | (channel(color[1]) << 8)
                        | channel(color[2]),
                );
            }
        }
        pixels
    }

    unsafe fn present(frame: &Frame) {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: frame.width,
                biHeight: -frame.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        if let Ok(bitmap) =
            CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
        {
            if !bits.is_null() {
                std::ptr::copy_nonoverlapping(
                    frame.pixels.as_ptr(),
                    bits.cast::<u32>(),
                    frame.pixels.len(),
                );
                let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let _ = UpdateLayeredWindow(
                    frame.hwnd,
                    Some(screen),
                    Some(&POINT {
                        x: frame.x,
                        y: frame.y,
                    }),
                    Some(&SIZE {
                        cx: frame.width,
                        cy: frame.height,
                    }),
                    Some(memory),
                    Some(&POINT { x: 0, y: 0 }),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                SelectObject(memory, previous);
            }
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
    }

    /// Start a hover: track the leave, tick the fill, pre-warm the notch.
    fn hover(hwnd: HWND) {
        let Some(app) = SLIVER.with(|cell| {
            let mut slot = cell.try_borrow_mut().ok()?;
            let sliver = slot.as_mut()?;
            if sliver.hidden || sliver.hover_since.is_some() {
                return None;
            }
            sliver.hover_since = Some(Instant::now());
            sliver.fired = false;
            Some(sliver.app.clone())
        }) else {
            return;
        };
        unsafe {
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut track);
            SetTimer(Some(hwnd), TIMER_ID, 15, None);
        }
        repaint();
        post(&app, crate::agent::prewarm_open);
    }

    fn leave(hwnd: HWND) {
        let Some((app, fired)) = SLIVER.with(|cell| {
            let mut slot = cell.try_borrow_mut().ok()?;
            let sliver = slot.as_mut()?;
            sliver.hover_since.take()?;
            Some((sliver.app.clone(), std::mem::take(&mut sliver.fired)))
        }) else {
            return;
        };
        unsafe {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
        }
        repaint();
        if !fired {
            post(&app, crate::agent::cancel_prewarm);
        }
    }

    /// Open the notch once per hover: the bar filled, or the sliver was clicked.
    fn fire(hwnd: HWND, require_full: bool) {
        let Some(app) = SLIVER.with(|cell| {
            let mut slot = cell.try_borrow_mut().ok()?;
            let sliver = slot.as_mut()?;
            if sliver.hidden || sliver.fired {
                return None;
            }
            if require_full
                && !sliver
                    .hover_since
                    .is_some_and(|since| since.elapsed() >= FILL)
            {
                return None;
            }
            sliver.fired = true;
            Some(sliver.app.clone())
        }) else {
            return;
        };
        unsafe {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
        }
        post(&app, crate::agent::open_notch);
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_MOUSEMOVE => {
                hover(hwnd);
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                leave(hwnd);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == TIMER_ID => {
                repaint();
                fire(hwnd, true);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == WORK_TIMER_ID => {
                // While hovering, the hover timer already repaints.
                let hovering = SLIVER.with(|cell| {
                    cell.try_borrow()
                        .ok()
                        .and_then(|slot| slot.as_ref().map(|s| s.hover_since.is_some()))
                        .unwrap_or(false)
                });
                if !hovering {
                    repaint();
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                fire(hwnd, false);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                let app = SLIVER.with(|cell| Some(cell.try_borrow().ok()?.as_ref()?.app.clone()));
                if let Some(app) = app {
                    post(&app, |_| reposition());
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
