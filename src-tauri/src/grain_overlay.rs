//! Grain presentation supplements to Handy's shared overlay lifecycle.
use base64::Engine;
use grain_core::{DaemonEvent, SessionMode};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use tauri::{AppHandle, Emitter, Manager};

/// Grain's visual clearance above Handy's platform-specific bottom placement.
pub const BOTTOM_RAISE: f64 = 8.0;

#[derive(Clone, Default, Serialize, Deserialize, Type)]
pub struct OverlayPresentation {
    pub visible: bool,
    pub state: String,
    pub ready: bool,
    pub session_id: u64,
    pub agent: bool,
    pub owner: Option<String>,
    pub icon: Option<String>,
    pub notice: Option<String>,
    pub followup: Option<String>,
    pub committed: String,
    pub tentative: String,
    pub working: bool,
    pub work_kind: String,
}

impl OverlayPresentation {
    fn clear_capture(&mut self) {
        self.ready = false;
        self.committed = String::new();
        self.tentative = String::new();
        self.working = false;
    }

    fn complete(&mut self, session_id: u64) -> bool {
        if session_id != self.session_id {
            return false;
        }
        self.clear_capture();
        if self.notice.is_some() || self.followup.is_some() {
            self.state = if self.followup.is_some() {
                "followup"
            } else {
                "notice"
            }
            .into();
            return false;
        }
        self.visible = false;
        true
    }

    fn stream_text(&mut self, session_id: u64, committed: &str, tentative: &str) {
        if session_id == self.session_id && self.state == "streaming" && self.visible {
            self.committed = committed.into();
            self.tentative = tentative.into();
        }
    }

    fn expire_notice(&mut self, scheduled: u64, current: u64) -> Option<bool> {
        if scheduled != current || self.notice.is_none() {
            return None;
        }
        self.notice = None;
        let hide = self.state == "notice";
        if hide {
            self.visible = false;
        }
        Some(hide)
    }
}

#[derive(Default)]
pub struct OverlayContext {
    presentation: Mutex<OverlayPresentation>,
    notice_generation: AtomicU64,
    capture_generation: AtomicU64,
}

fn publish(app: &AppHandle, value: &OverlayPresentation) {
    let _ = app.emit_to("recording_overlay", "grain-overlay-context", value);
}

pub fn show_capture(app: &AppHandle, mode: SessionMode) -> u64 {
    let generation = app
        .state::<OverlayContext>()
        .capture_generation
        .fetch_add(1, Ordering::AcqRel)
        + 1;
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if handle
            .state::<OverlayContext>()
            .capture_generation
            .load(Ordering::Acquire)
            == generation
        {
            show_capture_main(&handle, mode);
        }
    });
    generation
}

/// Failed startup must retire after its queued show, and never dismiss a newer capture.
pub fn hide_failed_capture(app: &AppHandle, generation: u64) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let ctx = handle.state::<OverlayContext>();
        if ctx.capture_generation.load(Ordering::Acquire) != generation {
            return;
        }
        let mut value = ctx.presentation.lock().unwrap();
        value.visible = false;
        value.clear_capture();
        publish(&handle, &value);
        drop(value);
        crate::overlay::hide_recording_overlay(&handle);
    });
}

fn show_capture_main(app: &AppHandle, mode: SessionMode) {
    let settings = crate::settings::get_settings(app);
    let streaming = mode == SessionMode::NativeAsr
        && settings.overlay_style == crate::settings::OverlayStyle::Live;
    if let Some(ctx) = app.try_state::<OverlayContext>() {
        ctx.notice_generation.fetch_add(1, Ordering::AcqRel);
        let mut value = ctx.presentation.lock().unwrap();
        *value = OverlayPresentation {
            visible: settings.overlay_style != crate::settings::OverlayStyle::None,
            state: if streaming { "streaming" } else { "recording" }.into(),
            ..Default::default()
        };
        publish(app, &value);
    }
    if streaming {
        crate::overlay::show_streaming_overlay(app);
    } else {
        crate::overlay::show_recording_overlay(app);
    }
}

pub fn mark_ready(app: &AppHandle) {
    if let Some(ctx) = app.try_state::<OverlayContext>() {
        ctx.presentation.lock().unwrap().ready = true;
    }
}

pub fn show_processing(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || show_processing_main(&handle));
}

fn show_processing_main(app: &AppHandle) {
    let Some(ctx) = app.try_state::<OverlayContext>() else {
        return;
    };
    let mut value = ctx.presentation.lock().unwrap();
    if !value.visible || !matches!(value.state.as_str(), "streaming" | "transcribing") {
        return;
    }
    if value.state == "streaming" {
        value.working = true;
        value.work_kind = "polishing".into();
        publish(app, &value);
        drop(value);
        app.state::<std::sync::Arc<crate::managers::transcription::TranscriptionManager>>()
            .emit_stream_working(crate::managers::transcription::StreamWorkKind::Polishing);
    } else {
        value.state = "processing".into();
        publish(app, &value);
        drop(value);
        crate::overlay::show_processing_overlay(app);
    }
}

pub fn on_event(app: &AppHandle, event: &DaemonEvent) {
    // Keep presentation updates and Handy show/hide generations on one queue.
    // Public audio remains independent of the window and never enters this queue.
    if matches!(event, DaemonEvent::AudioLevel { .. }) {
        return;
    }
    if !matches!(
        event,
        DaemonEvent::RecordingStarted { .. }
            | DaemonEvent::RecordingStopped { .. }
            | DaemonEvent::ProcessingComplete { .. }
            | DaemonEvent::SessionCancelled { .. }
            | DaemonEvent::AgentInputShow { .. }
            | DaemonEvent::AgentInputHide
            | DaemonEvent::AgentFollowupOffer { .. }
            | DaemonEvent::AgentFollowupClear
            | DaemonEvent::PasteMissed { .. }
            | DaemonEvent::PasteCatchDisabled
            | DaemonEvent::PillIcon { .. }
            | DaemonEvent::PillSkin { .. }
            | DaemonEvent::OverlayConfig { .. }
            | DaemonEvent::PasteError { .. }
            | DaemonEvent::AsrStreamText { .. }
    ) {
        return;
    }
    let handle = app.clone();
    let event = event.clone();
    let _ = app.run_on_main_thread(move || on_event_main(&handle, &event));
}

fn on_event_main(app: &AppHandle, event: &DaemonEvent) {
    match event {
        DaemonEvent::PillSkin { skin } => {
            let _ = app.emit_to("recording_overlay", "grain-overlay-skin", skin);
        }
        DaemonEvent::OverlayConfig { position } => {
            let _ = app.emit_to("recording_overlay", "grain-overlay-position", position);
        }
        _ => {}
    }
    // No locking or serialization for the high-frequency public audio feed.
    if !matches!(
        event,
        DaemonEvent::RecordingStarted { .. }
            | DaemonEvent::RecordingStopped { .. }
            | DaemonEvent::ProcessingComplete { .. }
            | DaemonEvent::SessionCancelled { .. }
            | DaemonEvent::AgentInputShow { .. }
            | DaemonEvent::AgentInputHide
            | DaemonEvent::AgentFollowupOffer { .. }
            | DaemonEvent::AgentFollowupClear
            | DaemonEvent::PasteMissed { .. }
            | DaemonEvent::PasteCatchDisabled
            | DaemonEvent::PillIcon { .. }
            | DaemonEvent::PillSkin { .. }
            | DaemonEvent::OverlayConfig { .. }
            | DaemonEvent::PasteError { .. }
            | DaemonEvent::AsrStreamText { .. }
    ) {
        return;
    }
    let Some(ctx) = app.try_state::<OverlayContext>() else {
        return;
    };
    let mut show = None;
    let mut hide = false;
    let mut expiry = None;
    let mut transcribing = false;
    {
        let mut value = ctx.presentation.lock().unwrap();
        match event {
            DaemonEvent::RecordingStarted {
                session_id, owner, ..
            } => {
                value.session_id = *session_id;
                value.owner = owner.clone();
            }
            DaemonEvent::RecordingStopped { session_id } if *session_id == value.session_id => {
                value.ready = false;
                if value.state == "streaming" {
                    value.working = true;
                    value.work_kind = "transcribing".into();
                    transcribing = true;
                } else {
                    value.state = "transcribing".into();
                    show = Some("transcribing");
                }
            }
            DaemonEvent::ProcessingComplete { session_id, .. }
            | DaemonEvent::SessionCancelled { session_id }
                if *session_id == value.session_id =>
            {
                // A clipboard notice is independent of capture completion.
                hide = value.complete(*session_id);
            }
            DaemonEvent::AgentInputShow { .. } => {
                value.agent = true;
            }
            DaemonEvent::AgentInputHide => {
                if value.agent {
                    crate::surface_watch::stop(app);
                    value.agent = false;
                    let session_id = value.session_id;
                    hide = value.complete(session_id);
                }
            }
            DaemonEvent::AgentFollowupOffer { shortcut } => {
                ctx.notice_generation.fetch_add(1, Ordering::AcqRel);
                value.followup = Some(shortcut.clone());
                value.notice = None;
                value.state = "followup".into();
                value.visible = crate::settings::get_settings(app).overlay_style
                    != crate::settings::OverlayStyle::None;
                show = Some("followup");
            }
            DaemonEvent::AgentFollowupClear => {
                value.followup = None;
                if value.state == "followup" {
                    if value.notice.is_some() {
                        value.state = "notice".into();
                    } else {
                        value.visible = false;
                        hide = true;
                    }
                }
            }
            DaemonEvent::PasteMissed { .. } | DaemonEvent::PasteError { .. } => {
                value.notice = Some(
                    if matches!(event, DaemonEvent::PasteMissed { .. }) {
                        "Copied to clipboard"
                    } else {
                        "Could not paste"
                    }
                    .into(),
                );
                if !value.visible {
                    value.clear_capture();
                    value.state = "notice".into();
                    value.visible = crate::settings::get_settings(app).overlay_style
                        != crate::settings::OverlayStyle::None;
                    show = Some("notice");
                }
                expiry = Some(ctx.notice_generation.fetch_add(1, Ordering::AcqRel) + 1);
            }
            DaemonEvent::PasteCatchDisabled => {
                ctx.notice_generation.fetch_add(1, Ordering::AcqRel);
                value.notice = None;
                if value.state == "notice" {
                    value.visible = false;
                    hide = true;
                }
            }
            DaemonEvent::PillIcon { rgba } => {
                value.icon = rgba
                    .as_ref()
                    .and_then(|data| base64::engine::general_purpose::STANDARD.decode(data).ok())
                    .and_then(|rgba| crate::pill_icon::png_data_url(&rgba));
            }
            DaemonEvent::AsrStreamText {
                session_id,
                committed,
                tentative,
            } => {
                value.stream_text(*session_id, committed, tentative);
            }
            _ => {}
        }
        publish(app, &value);
    }
    if transcribing {
        app.state::<std::sync::Arc<crate::managers::transcription::TranscriptionManager>>()
            .emit_stream_working(crate::managers::transcription::StreamWorkKind::Transcribing);
    }
    if let Some(state) = show {
        crate::overlay::show_overlay_state(app, state);
    }
    if hide {
        crate::overlay::hide_recording_overlay(app);
    }
    if let Some(generation) = expiry {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(3));
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let Some(ctx) = handle.try_state::<OverlayContext>() else {
                    return;
                };
                let mut value = ctx.presentation.lock().unwrap();
                let Some(hide) =
                    value.expire_notice(generation, ctx.notice_generation.load(Ordering::Acquire))
                else {
                    return;
                };
                publish(&handle, &value);
                drop(value);
                if hide {
                    crate::overlay::hide_recording_overlay(&handle);
                }
            });
        });
    }
}

#[derive(Serialize, Deserialize, Type)]
pub struct OverlaySnapshot {
    pub presentation: OverlayPresentation,
    pub position: crate::settings::OverlayPosition,
    pub skin: grain_core::settings::PillSkin,
    pub theme: crate::grain_theme::ThemeState,
    pub streaming_width: f64,
    pub streaming_height: f64,
}

#[tauri::command]
#[specta::specta]
pub fn overlay_snapshot(app: AppHandle) -> OverlaySnapshot {
    let settings = crate::settings::get_settings(&app);
    OverlaySnapshot {
        presentation: app
            .state::<OverlayContext>()
            .presentation
            .lock()
            .unwrap()
            .clone(),
        position: settings.overlay_position,
        skin: settings.pill_skin,
        theme: crate::grain_theme::get_theme(app.clone()),
        streaming_width: crate::overlay::overlay_dimensions("streaming").0,
        streaming_height: crate::overlay::overlay_dimensions("streaming").1,
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_overlay_style_setting(app: AppHandle, style: crate::settings::OverlayStyle) {
    let mut settings = crate::settings::get_settings(&app);
    settings.overlay_style = style;
    crate::settings::write_settings(&app, settings);
    crate::overlay::update_overlay_enabled_cache(style != crate::settings::OverlayStyle::None);
    if style == crate::settings::OverlayStyle::None {
        app.state::<OverlayContext>()
            .presentation
            .lock()
            .unwrap()
            .visible = false;
        crate::overlay::hide_recording_overlay(&app);
    }
}

#[tauri::command]
#[specta::specta]
pub fn overlay_cancel(app: AppHandle) {
    if crate::agent::input_is_active(&app) {
        crate::agent::input_cancel(&app);
    } else {
        crate::utils::cancel_current_operation(&app);
        crate::grain_actions::cancel_session(&app);
    }
}

#[tauri::command]
#[specta::specta]
pub fn overlay_followup(app: AppHandle) {
    crate::agent::open_followup(&app);
}

static LAST_PUBLIC_LEVEL: AtomicU64 = AtomicU64::new(0);
pub fn emit_public_levels(app: &AppHandle, levels: &[f32]) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let last = LAST_PUBLIC_LEVEL.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 33 {
        return;
    }
    LAST_PUBLIC_LEVEL.store(now, Ordering::Relaxed);
    crate::bridge::emit(
        app,
        DaemonEvent::AudioLevel {
            levels: levels.to_vec(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::OverlayPresentation;

    #[test]
    fn stale_capture_updates_cannot_overwrite_or_hide_a_new_session() {
        let mut value = OverlayPresentation {
            visible: true,
            ready: true,
            state: "streaming".into(),
            session_id: 2,
            ..Default::default()
        };
        value.stream_text(2, "current", "tail");
        value.stream_text(1, "stale", "text");
        assert_eq!(value.committed, "current");
        assert!(!value.complete(1));
        assert!(value.visible && value.ready);
        value.working = true;
        assert_eq!(value.tentative, "tail");
        assert!(value.complete(2));
        assert!(!value.visible && !value.ready && !value.working);
        assert!(value.committed.is_empty() && value.tentative.is_empty());
        assert_eq!(value.committed.capacity(), 0);
    }

    #[test]
    fn clipboard_expiry_is_independent_and_cannot_hide_a_replacement() {
        let mut value = OverlayPresentation {
            visible: true,
            state: "notice".into(),
            notice: Some("Copied to clipboard".into()),
            session_id: 1,
            ..Default::default()
        };
        assert!(!value.complete(1));
        assert!(value.visible && value.notice.is_some());
        assert_eq!(value.expire_notice(1, 2), None);
        assert!(value.visible);
        assert_eq!(value.expire_notice(2, 2), Some(true));
        assert!(!value.visible && value.notice.is_none());
        value.visible = true;
        value.state = "recording".into();
        assert_eq!(value.expire_notice(2, 2), None);
        assert!(value.visible);
    }

    #[test]
    fn clipboard_notice_during_live_capture_restores_text_and_waveform() {
        let mut value = OverlayPresentation {
            visible: true,
            ready: true,
            state: "streaming".into(),
            session_id: 2,
            notice: Some("Copied to clipboard".into()),
            ..Default::default()
        };
        value.stream_text(2, "dictation continued", "while notice was visible");
        assert_eq!(value.expire_notice(1, 1), Some(false));
        assert!(value.visible && value.ready && value.notice.is_none());
        assert_eq!(value.committed, "dictation continued");
        value.followup = Some("Ctrl+Enter".into());
        value.state = "followup".into();
        value.notice = Some("Copied to clipboard".into());
        assert_eq!(value.expire_notice(2, 2), Some(false));
        assert!(value.visible && value.followup.is_some());
    }
}
