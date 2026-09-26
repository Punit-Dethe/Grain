//! Grain's initial window bounds, in logical pixels. Keep setup within the
//! desktop work area even when display scaling makes it smaller than our
//! normal settings-window minimum.

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MainWindowSize {
    pub width: f64,
    pub height: f64,
    pub min_width: f64,
    pub min_height: f64,
}

impl Default for MainWindowSize {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 800.0,
            min_width: 1024.0,
            min_height: 680.0,
        }
    }
}

pub(crate) fn main_window_size(
    work_width: u32,
    work_height: u32,
    scale_factor: f64,
    native_frame: bool,
) -> MainWindowSize {
    let preferred = MainWindowSize::default();
    if work_width == 0 || work_height == 0 {
        return preferred;
    }
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    // Leave breathing room around the window; macOS also needs room for the
    // native frame, which is outside the webview's inner size.
    let available_width = (f64::from(work_width) / scale - 32.0).max(1.0);
    let available_height =
        (f64::from(work_height) / scale - 32.0 - if native_frame { 32.0 } else { 0.0 }).max(1.0);

    MainWindowSize {
        width: preferred.width.min(available_width),
        height: preferred.height.min(available_height),
        min_width: preferred.min_width.min(available_width),
        min_height: preferred.min_height.min(available_height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_desktops_keep_the_normal_size() {
        assert_eq!(
            main_window_size(1920, 1040, 1.0, false),
            MainWindowSize::default()
        );
    }

    #[test]
    fn scaled_laptops_fit_above_the_taskbar() {
        let size = main_window_size(1920, 1040, 1.5, false);
        assert_eq!(size.width, 1248.0);
        assert!((size.height - (1040.0 / 1.5 - 32.0)).abs() < 0.01);
        assert_eq!(size.min_width, 1024.0);
        assert_eq!(size.min_height, size.height);
    }

    #[test]
    fn small_displays_do_not_enforce_an_offscreen_minimum() {
        let size = main_window_size(800, 560, 1.0, false);
        assert_eq!(size.width, 768.0);
        assert_eq!(size.height, 528.0);
        assert_eq!(size.min_width, size.width);
        assert_eq!(size.min_height, size.height);
    }

    #[test]
    fn native_frames_have_room_and_invalid_monitor_data_is_safe() {
        assert_eq!(main_window_size(1280, 720, 1.0, true).height, 656.0);
        assert_eq!(
            main_window_size(0, 0, 1.0, false),
            MainWindowSize::default()
        );
        assert_eq!(
            main_window_size(1280, 720, f64::NAN, false),
            main_window_size(1280, 720, 1.0, false),
        );
    }
}
