//! [GRAIN] Screen-image capture — a **platform capability, not a feature**.
//!
//! # Why nothing in core calls this
//!
//! An image of what someone is looking at is the most sensitive thing Grain can
//! produce. It cannot be filtered the way text can: a screenshot carries whatever
//! happened to be on the window — a password manager mid-reveal, someone else's
//! message, a medical record — and unlike the accessibility tree there is no
//! per-element flag to skip. Grain's own code therefore never captures one, never
//! decides to send one, and has no setting that turns that on.
//!
//! What exists instead is the *possibility*. An extension the user deliberately
//! installed and deliberately granted `capture:screen-image` can take a frame and
//! hand it to a model. That keeps the decision, the blast radius and the
//! responsibility with a component the user opted into, rather than with the
//! dictation app they leave running all day. If this ever becomes a built-in, it
//! should arrive through an experimentation flag with its own consent surface —
//! not by quietly gaining a caller in here.
//!
//! # What the capture is bounded to
//!
//! - **The foreground window only.** Never the desktop, never another
//!   application, never a region the user cannot see. `PrintWindow` targets one
//!   `HWND`; there is no code path here that can widen that.
//! - **Downscaled before it exists as an encoded image.** Vision models tile
//!   their input anyway, so a 4K PNG is bytes and latency spent for nothing.
//! - **Never written to disk, never logged.** The buffer is overwritten on drop
//!   (see [`CapturedImage`]), so it does not linger in freed heap for whatever
//!   allocates next.
//!
//! # Platform
//!
//! Windows via GDI `PrintWindow`, following the same `#[cfg(windows)]` +
//! `None`-elsewhere shape as `context_detect`. The whole platform surface is one
//! function returning one owned struct, so moving to Windows Graphics Capture —
//! or to a cross-platform crate — is an implementation swap behind an unchanged
//! boundary.

/// One captured frame, owned and self-clearing.
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    /// MIME type of [`Self::bytes`], for the `data:` URI.
    pub mime: &'static str,
    /// Encoded image bytes. Overwritten when this value drops.
    pub bytes: Vec<u8>,
}

impl Drop for CapturedImage {
    fn drop(&mut self) {
        // Zero before release. An encoded screenshot sitting in freed heap is
        // readable by whatever allocates next in-process; this costs one pass
        // over a few hundred KB and closes that window.
        //
        // `zeroize` rather than a plain loop: writing to a buffer that is about
        // to be freed is a dead store, and the compiler is entirely within its
        // rights to delete it. `Zeroize` guarantees the writes with volatile
        // semantics plus a barrier, which is the difference between this
        // actually clearing the frame and only appearing to.
        use zeroize::Zeroize;
        self.bytes.zeroize();
    }
}

impl std::fmt::Debug for CapturedImage {
    /// Deliberately shape-only. A derived `Debug` would put the frame into any
    /// log line that formatted it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CapturedImage {{ {}x{}, {}, {} bytes }}",
            self.width,
            self.height,
            self.mime,
            self.bytes.len()
        )
    }
}

impl CapturedImage {
    /// Base64 for the `data:` URI. Kept here so callers never hand-roll it.
    pub fn to_base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&self.bytes)
    }
}

/// Longest edge of the encoded frame, in pixels.
///
/// Vision models tile input into fixed-size patches, so detail past roughly this
/// point buys nothing and costs tokens, bandwidth and latency on every call.
const MAX_EDGE: u32 = 1280;

/// Refuse to capture a window larger than this in either direction. A sane upper
/// bound on the intermediate RGBA buffer, which is 4 bytes per pixel before any
/// downscaling happens.
const MAX_SOURCE_EDGE: i32 = 16_384;

/// Capture the foreground window.
///
/// `None` on unsupported platforms, when there is no foreground window, or on
/// any capture failure — this never panics and never partially succeeds.
pub fn capture_foreground_window() -> Option<CapturedImage> {
    #[cfg(windows)]
    {
        windows_impl::capture()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Downscale to [`MAX_EDGE`] and encode as PNG.
///
/// PNG rather than a lossy format on purpose: these frames are overwhelmingly
/// text and UI, where lossy compression puts ringing artifacts exactly on the
/// glyph edges a model needs to read.
#[cfg(windows)]
fn encode(rgba: image::RgbaImage) -> Option<CapturedImage> {
    use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
    use image::{imageops::FilterType, ImageEncoder};

    let (w, h) = (rgba.width(), rgba.height());
    let longest = w.max(h);
    let rgba = if longest > MAX_EDGE {
        let scale = MAX_EDGE as f32 / longest as f32;
        let (nw, nh) = (
            ((w as f32 * scale) as u32).max(1),
            ((h as f32 * scale) as u32).max(1),
        );
        // Triangle: cheap, and adequate once the image is only ever read by a
        // model rather than displayed.
        image::imageops::resize(&rgba, nw, nh, FilterType::Triangle)
    } else {
        rgba
    };

    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, PngFilter::Adaptive)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;

    Some(CapturedImage {
        width: rgba.width(),
        height: rgba.height(),
        mime: "image/png",
        bytes,
    })
}

#[cfg(windows)]
mod windows_impl {
    use super::{encode, CapturedImage, MAX_SOURCE_EDGE};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
    };
    // `PrintWindow` lives under Storage::Xps in the windows crate — that is
    // where the SDK's header association puts it, not where its purpose suggests.
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};

    /// `PW_RENDERFULLCONTENT`. Without it, hardware-composited windows — which
    /// today means every browser and every Electron app — come back blank.
    const PW_RENDERFULLCONTENT: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(2);

    /// RAII for the GDI objects.
    ///
    /// It owns each handle from the moment that handle exists, and the later
    /// ones are `Option` precisely so it can. An earlier version built the guard
    /// only after the bitmap was created, which meant a `CreateDIBSection`
    /// failure returned while the device contexts were still held — a GDI handle
    /// leak on the error path of a process that runs all day.
    struct Gdi {
        window: HWND,
        screen_dc: windows::Win32::Graphics::Gdi::HDC,
        mem_dc: windows::Win32::Graphics::Gdi::HDC,
        bitmap: Option<HBITMAP>,
        previous: Option<HGDIOBJ>,
    }

    impl Drop for Gdi {
        fn drop(&mut self) {
            unsafe {
                // Reverse order of acquisition: deselect the bitmap before
                // deleting it, delete it before the DC it was selected into.
                if let Some(previous) = self.previous.take() {
                    SelectObject(self.mem_dc, previous);
                }
                if let Some(bitmap) = self.bitmap.take() {
                    let _ = DeleteObject(bitmap.into());
                }
                if !self.mem_dc.is_invalid() {
                    let _ = DeleteDC(self.mem_dc);
                }
                if !self.screen_dc.is_invalid() {
                    ReleaseDC(Some(self.window), self.screen_dc);
                }
            }
        }
    }

    pub(super) fn capture() -> Option<CapturedImage> {
        unsafe {
            let window = GetForegroundWindow();
            if window.0.is_null() {
                return None;
            }

            let mut rect = RECT::default();
            GetWindowRect(window, &mut rect).ok()?;
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width <= 0 || height <= 0 || width > MAX_SOURCE_EDGE || height > MAX_SOURCE_EDGE {
                return None;
            }

            let screen_dc = GetDC(Some(window));
            if screen_dc.is_invalid() {
                return None;
            }
            let mem_dc = CreateCompatibleDC(Some(screen_dc));
            // Guard takes ownership NOW, before anything else can fail.
            let mut gdi = Gdi {
                window,
                screen_dc,
                mem_dc,
                bitmap: None,
                previous: None,
            };

            // Top-down 32bpp: a negative height flips the DIB so row 0 is the
            // top, which is the order image encoders expect. Reading a
            // bottom-up DIB straight into one is the classic upside-down bug.
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut pixels: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap =
                CreateDIBSection(Some(screen_dc), &info, DIB_RGB_COLORS, &mut pixels, None, 0)
                    .ok()?; // guard above releases both DCs on this path
            gdi.bitmap = Some(bitmap);
            gdi.previous = Some(SelectObject(mem_dc, bitmap.into()));

            if !PrintWindow(window, mem_dc, PW_RENDERFULLCONTENT).as_bool() {
                return None;
            }
            if pixels.is_null() {
                return None;
            }

            // The DIB is BGRA; encoders want RGBA. Alpha is forced opaque
            // because `PrintWindow` leaves it zero on many windows, which would
            // otherwise encode as a fully transparent — and to a model, blank —
            // image.
            let len = (width as usize) * (height as usize) * 4;
            let bgra = std::slice::from_raw_parts(pixels as *const u8, len);
            let mut rgba = Vec::with_capacity(len);
            for chunk in bgra.chunks_exact(4) {
                rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], 255]);
            }

            let buffer = image::RgbaImage::from_raw(width as u32, height as u32, rgba)?;
            encode(buffer)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_frame() {
        let image = CapturedImage {
            width: 8,
            height: 4,
            mime: "image/png",
            bytes: vec![0xAB; 64],
        };
        let rendered = format!("{image:?}");
        assert!(rendered.contains("8x4"));
        assert!(rendered.contains("64 bytes"));
        // The pixels themselves must never appear in a formatted value.
        assert!(!rendered.contains("171"));
        assert!(!rendered.contains("AB"));
    }

    #[test]
    fn base64_round_trips() {
        use base64::Engine;
        let image = CapturedImage {
            width: 1,
            height: 1,
            mime: "image/png",
            bytes: vec![1, 2, 3, 4],
        };
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(image.to_base64())
            .unwrap();
        assert_eq!(decoded, vec![1, 2, 3, 4]);
    }

    /// Capture must never panic, whatever the environment — including a test
    /// runner with no foreground window.
    #[test]
    fn capture_is_infallible_in_the_absence_of_a_window() {
        let _ = capture_foreground_window();
    }
}
