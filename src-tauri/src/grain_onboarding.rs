//! [GRAIN] Where a launching app should land: onboarding, permissions, or the app.
//!
//! This decision used to live in `App.tsx`, spread across an async function, two
//! pieces of React state and a `platform()` string compared in TypeScript. Three
//! things were wrong with that:
//!
//! 1. **It is not a rendering decision.** "Does this machine have a usable model
//!    and the permissions to record" is a fact about the machine. The UI's job
//!    is to draw the answer, not to derive it.
//! 2. **It is per-platform, and upstream fixes it in Rust.** Handy's macOS and
//!    Windows permission work lands in `commands/audio.rs` and the permissions
//!    plugin. A copy of the gating logic in TSX means every such fix has to be
//!    re-read and re-applied by hand on our side.
//! 3. **The UI 2.0 rewrite would have had to reimplement it from scratch**, from
//!    a shape that mixed policy with `useState` calls — which is exactly how the
//!    silent regressions get introduced.
//!
//! The permission checks themselves are the plugin's own Rust functions, not a
//! reimplementation: `tauri_plugin_macos_permissions` exports them directly and
//! already returns `true` off-macOS, so the frontend was paying an IPC hop to
//! reach code we can call in-process.
//!
//! # A query with one side effect, on purpose
//!
//! When permissions are what is blocking, the main window has to be revealed —
//! Grain can start hidden, and a permission prompt behind an invisible window is
//! a hang from the user's point of view. That reveal happens here rather than
//! being returned as a flag for the UI to honour, because it must happen
//! identically in the old shell and the new one, and a flag is something a
//! rewrite can forget to read.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, State};

use crate::managers::model::ModelManager;

/// Which screen the app should open on.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    /// Permissions screen — either first-run, or a returning user who has since
    /// had a permission revoked.
    Accessibility,
    /// Model picker. Only ever reached by a genuinely new user, and only after
    /// permissions are settled.
    Model,
    /// Nothing in the way; show the app.
    Done,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
pub struct OnboardingState {
    pub step: OnboardingStep,
    /// Has models already. Decides where "permissions granted" goes next: a
    /// returning user goes straight to the app, a new user picks a model first.
    pub is_returning_user: bool,
    /// True when `step` is `Accessibility` *because a permission is missing*
    /// rather than because this is a first run. Lets the UI say "Grain lost
    /// microphone access" instead of "welcome".
    pub blocked_on_permissions: bool,
}

/// Are the permissions Grain needs in place?
///
/// `None` means "could not tell" — a probe failed, or the platform does not
/// expose the answer. Unknown is deliberately NOT treated as denied: the old
/// TSX swallowed probe errors and continued into the app so the user could fix
/// it there, and blocking a working install behind an unanswerable question is
/// the worse failure.
async fn permissions_ok(app: &AppHandle) -> Option<bool> {
    let _ = app;

    #[cfg(target_os = "macos")]
    {
        let accessibility = tauri_plugin_macos_permissions::check_accessibility_permission().await;
        let microphone = tauri_plugin_macos_permissions::check_microphone_permission().await;
        return Some(accessibility && microphone);
    }

    #[cfg(target_os = "windows")]
    {
        use crate::commands::audio::PermissionAccess;
        let status = crate::commands::audio::get_windows_microphone_permission_status();
        if !status.supported {
            return None;
        }
        // Only an explicit "deny" blocks. `Unknown` is the common case on a
        // machine that has simply never been asked.
        return Some(status.overall_access != PermissionAccess::Denied);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    None
}

/// Decide where the app should open. See the module docs for the window reveal.
#[tauri::command]
#[specta::specta]
pub async fn resolve_onboarding_state(
    app: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<OnboardingState, String> {
    let has_models = model_manager
        .get_available_models()
        .iter()
        .any(|m| m.is_downloaded);

    if !has_models {
        // A new user meets permissions first, then the model picker. Nothing to
        // reveal: onboarding shows the window itself.
        return Ok(OnboardingState {
            step: OnboardingStep::Accessibility,
            is_returning_user: false,
            blocked_on_permissions: false,
        });
    }

    if permissions_ok(&app).await == Some(false) {
        if let Err(e) = crate::show_main_window_command(app.clone()) {
            log::warn!("failed to reveal main window for permission onboarding: {e}");
        }
        return Ok(OnboardingState {
            step: OnboardingStep::Accessibility,
            is_returning_user: true,
            blocked_on_permissions: true,
        });
    }

    Ok(OnboardingState {
        step: OnboardingStep::Done,
        is_returning_user: true,
        blocked_on_permissions: false,
    })
}

/// Where "permissions granted" leads. A returning user already has a model, so
/// the picker would be a dead screen; a new user needs it.
#[tauri::command]
#[specta::specta]
pub fn onboarding_step_after_permissions(is_returning_user: bool) -> OnboardingStep {
    if is_returning_user {
        OnboardingStep::Done
    } else {
        OnboardingStep::Model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returning_users_skip_the_model_picker() {
        assert_eq!(
            onboarding_step_after_permissions(true),
            OnboardingStep::Done
        );
        assert_eq!(
            onboarding_step_after_permissions(false),
            OnboardingStep::Model
        );
    }
}
