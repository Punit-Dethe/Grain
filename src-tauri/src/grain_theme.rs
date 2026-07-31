//! [GRAIN] One answer to "light or dark", for every surface Grain paints.
//!
//! # Why this is not `localStorage`
//!
//! It used to be. `ThemeContext.tsx` kept the preference under
//! `grain-theme-settings`, and `extension-surface.ts` read *that same key* to
//! decide how to style a sandboxed extension frame — which worked only because
//! the two happen to share a browser origin, and told you nothing at all if the
//! surface you needed to style was the native pill.
//!
//! Grain paints more than one webview: the settings window, the agent panel, the
//! switcher capsule, extension surfaces, and a winit/Floem pill that has no
//! browser storage to read. A preference that only one of them can see is not a
//! preference, it is a local variable. So the preference is a settings field
//! ([`grain_core::settings::ThemeMode`]) and this module owns the two jobs that
//! go with it: resolving `System` against the OS, and telling everyone when the
//! answer changes.
//!
//! # Why the resolved value is what travels
//!
//! [`ThemeMode`] is what the user chose; [`ResolvedTheme`] is what to paint.
//! Only the second one is broadcast. Handing `System` to each surface would
//! make every one of them ask the OS independently — three toolkits, three
//! chances to disagree, and a visible mismatch between the pill and the window
//! behind it. The resolution happens once, here.
//!
//! # Both buses, because both audiences exist
//!
//! The change goes out as a Tauri event (Grain's webviews) *and* as
//! [`DaemonEvent::ThemeConfig`] (the pill and extension surfaces). That is the
//! same split `grain_events` documents — not a duplication, two transports for
//! two kinds of consumer.

use grain_core::settings::{ResolvedTheme, ThemeMode};
use grain_core::DaemonEvent;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

/// Broadcast when the effective colour scheme changes — either because the user
/// picked a different mode, or because the OS flipped while on `System`.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct ThemeChanged {
    pub mode: ThemeMode,
    pub resolved: ResolvedTheme,
}

/// The preference and its resolution together, so a surface can render
/// immediately *and* show the right radio button without a second round trip.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ThemeState {
    pub mode: ThemeMode,
    pub resolved: ResolvedTheme,
}

/// What the OS is currently doing, or `None` if it will not say.
///
/// Read from a real window because that is the only place Tauri exposes it.
/// Any window will do — the platform reports one system-wide scheme — so this
/// takes whichever exists rather than depending on a particular label; the
/// agent panel and the settings window come and go independently.
fn os_dark(app: &AppHandle) -> Option<bool> {
    let window = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())?;
    match window.theme().ok()? {
        tauri::Theme::Dark => Some(true),
        _ => Some(false),
    }
}

/// The current preference resolved against the OS.
pub fn current(app: &AppHandle) -> ThemeState {
    let mode = crate::settings::get_settings(app).theme;
    ThemeState {
        mode,
        resolved: mode.resolve(os_dark(app)),
    }
}

/// Tell every surface. Safe to call when nothing actually changed — a repaint
/// with identical values is cheaper than tracking who has heard what.
pub fn broadcast(app: &AppHandle, state: &ThemeState) {
    use tauri_specta::Event as _;

    if let Err(e) = (ThemeChanged {
        mode: state.mode,
        resolved: state.resolved,
    })
    .emit(app)
    {
        log::warn!("failed to emit theme-changed: {e}");
    }
    crate::bridge::emit(
        app,
        DaemonEvent::ThemeConfig {
            theme: state.resolved,
        },
    );
}

/// Re-resolve and broadcast. Called when the OS scheme flips underneath us;
/// a no-op in effect unless the user is on `System`, but cheap enough that
/// filtering would cost more than it saves.
pub fn refresh_from_os(app: &AppHandle) {
    broadcast(app, &current(app));
}

#[tauri::command]
#[specta::specta]
pub fn get_theme(app: AppHandle) -> ThemeState {
    current(&app)
}

#[tauri::command]
#[specta::specta]
pub fn set_theme_mode(app: AppHandle, mode: ThemeMode) -> Result<ThemeState, String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.theme = mode;
    crate::settings::write_settings(&app, settings);

    let state = current(&app);
    broadcast(&app, &state);
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_modes_ignore_the_os() {
        for os in [Some(true), Some(false), None] {
            assert_eq!(ThemeMode::Light.resolve(os), ResolvedTheme::Light);
            assert_eq!(ThemeMode::Dark.resolve(os), ResolvedTheme::Dark);
        }
    }

    #[test]
    fn system_follows_the_os() {
        assert_eq!(ThemeMode::System.resolve(Some(true)), ResolvedTheme::Dark);
        assert_eq!(ThemeMode::System.resolve(Some(false)), ResolvedTheme::Light);
    }

    /// A platform that will not report its scheme must land on light, not dark:
    /// that is what `localStorage.getItem(...) === "dark"` returned for a
    /// missing key, so nobody's app changes appearance on upgrade.
    #[test]
    fn unknown_os_scheme_is_light() {
        assert_eq!(ThemeMode::System.resolve(None), ResolvedTheme::Light);
    }

    #[test]
    fn default_is_follow_the_system() {
        assert_eq!(ThemeMode::default(), ThemeMode::System);
    }
}
