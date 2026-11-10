//! Grain presentation supplements to Handy's shared overlay lifecycle.
use base64::Engine;
use grain_core::DaemonEvent;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use tauri::{AppHandle, Emitter, Manager};

/// Grain's visual clearance above Handy's platform-specific bottom placement.
pub const BOTTOM_RAISE: f64 = 16.0;

#[cfg(windows)]
const DICTIONARY_NOTICE_DURATION: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone, Serialize, Deserialize, Type)]
pub struct DictionarySuggestion {
    pub id: u32,
    pub original: String,
    pub corrected: String,
    /// Wall-clock deadline for the renderer's countdown. Cleared on save failure.
    pub expires_at_ms: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize, Type)]
pub struct ClipboardOffer {
    pub id: u32,
    pub expires_at_ms: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DictionaryResolution {
    Save,
    Decline,
    Timeout,
}

#[derive(Clone, Default, Serialize, Deserialize, Type)]
pub struct OverlayPresentation {
    pub visible: bool,
    pub state: String,
    pub ready: bool,
    pub session_id: u64,
    pub prompt_recording: bool,
    pub app_icon_enabled: bool,
    pub icon: Option<String>,
    pub notice: Option<String>,
    pub dictionary: Option<DictionarySuggestion>,
    pub clipboard_offer: Option<ClipboardOffer>,
    pub committed: String,
    pub tentative: String,
    pub working: bool,
    pub work_kind: String,
}

impl OverlayPresentation {
    fn for_capture(
        live_preview: bool,
        style: crate::settings::OverlayStyle,
        app_icon_enabled: bool,
    ) -> Self {
        Self {
            app_icon_enabled,
            visible: style != crate::settings::OverlayStyle::None,
            // Live preview follows the selected model capability, as in Handy.
            // Minimal forces every action to use the compact recording card.
            state: if style == crate::settings::OverlayStyle::Live && live_preview {
                "streaming"
            } else {
                "recording"
            }
            .into(),
            ..Default::default()
        }
    }

    fn clear_capture(&mut self) {
        self.ready = false;
        self.prompt_recording = false;
        self.committed = String::new();
        self.tentative = String::new();
        self.working = false;
    }

    fn mark_prompt_record(&mut self, session_id: u64) -> bool {
        if session_id != self.session_id
            || self.working
            || !matches!(self.state.as_str(), "recording" | "streaming")
        {
            return false;
        }
        self.prompt_recording = true;
        true
    }

    fn complete(&mut self, session_id: u64) -> bool {
        if session_id != self.session_id {
            return false;
        }
        self.clear_capture();
        if self.dictionary.is_some() || self.clipboard_offer.is_some() {
            return false;
        }
        if self.notice.is_some() {
            self.state = "notice".into();
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

    /// A click and its timeout share the same pending offer. Only the first
    /// successful resolution consumes it. A failed save permits a manual retry.
    fn resolve_dictionary(
        &mut self,
        id: u32,
        resolution: DictionaryResolution,
        save: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<bool, String> {
        let Some(suggestion) = self.dictionary.as_mut().filter(|suggestion| {
            self.visible
                && suggestion.id == id
                && (resolution != DictionaryResolution::Timeout
                    || suggestion.expires_at_ms.is_some())
        }) else {
            return Ok(false);
        };
        if resolution != DictionaryResolution::Decline {
            if let Err(error) = save(&suggestion.corrected) {
                suggestion.expires_at_ms = None;
                return Err(error);
            }
        }
        self.dictionary = None;
        self.visible = false;
        Ok(true)
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

/// Called only after the recorder accepted the split; stale queued feedback
/// cannot tint another capture or a session that has already stopped.
pub fn mark_prompt_record(app: &AppHandle, session_id: u64) {
    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(ctx) = app_handle.try_state::<OverlayContext>() {
            let mut value = ctx.presentation.lock().unwrap();
            if value.mark_prompt_record(session_id) {
                publish(&app_handle, &value);
            }
        }
    });
}

pub fn set_app_icon_enabled(app: &AppHandle, enabled: bool) {
    if let Some(ctx) = app.try_state::<OverlayContext>() {
        let mut value = ctx.presentation.lock().unwrap();
        value.app_icon_enabled = enabled;
        publish(app, &value);
    }
}

pub fn show_capture(app: &AppHandle, live_preview: bool) -> u64 {
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
            show_capture_main(&handle, live_preview);
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

fn show_capture_main(app: &AppHandle, live_preview: bool) {
    let settings = crate::settings::get_settings(app);
    // Use the caller's capability snapshot, the same one Handy uses for VAD.
    // Do not re-read the selected model after this show was queued.
    let presentation = OverlayPresentation::for_capture(
        live_preview,
        settings.overlay_style,
        settings.pill_show_app_icon,
    );
    let streaming = presentation.state == "streaming";
    if let Some(ctx) = app.try_state::<OverlayContext>() {
        ctx.notice_generation.fetch_add(1, Ordering::AcqRel);
        let mut value = ctx.presentation.lock().unwrap();
        *value = presentation;
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
            | DaemonEvent::PasteMissed { .. }
            | DaemonEvent::PasteMissedClear { .. }
            | DaemonEvent::PillIcon { .. }
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
            | DaemonEvent::PasteMissed { .. }
            | DaemonEvent::PasteMissedClear { .. }
            | DaemonEvent::PillIcon { .. }
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
            DaemonEvent::RecordingStarted { session_id, .. } => {
                if value.dictionary.take().is_some() {
                    value.visible = false;
                    hide = true;
                }
                value.session_id = *session_id;
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
            DaemonEvent::PasteMissed {
                id, expires_at_ms, ..
            } => {
                value.clipboard_offer = Some(ClipboardOffer {
                    id: *id,
                    expires_at_ms: *expires_at_ms,
                });
                value.state = "copy".into();
                value.visible = crate::settings::get_settings(app).overlay_style
                    != crate::settings::OverlayStyle::None;
                show = Some("copy");
            }
            DaemonEvent::PasteMissedClear { id }
                if value
                    .clipboard_offer
                    .as_ref()
                    .is_some_and(|offer| offer.id == *id) =>
            {
                value.clipboard_offer = None;
                if value.state == "copy" {
                    value.visible = false;
                    hide = true;
                }
            }
            DaemonEvent::PasteError { .. } => {
                value.notice = Some("Could not paste".into());
                if !value.visible {
                    value.clear_capture();
                    value.state = "notice".into();
                    value.visible = crate::settings::get_settings(app).overlay_style
                        != crate::settings::OverlayStyle::None;
                    show = Some("notice");
                }
                expiry = Some(ctx.notice_generation.fetch_add(1, Ordering::AcqRel) + 1);
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
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
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

#[cfg(windows)]
pub(crate) fn show_dictionary_suggestion(
    app: &AppHandle,
    id: u32,
    original: String,
    corrected: String,
) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let settings = crate::settings::get_settings(&handle);
        let ctx = handle.state::<OverlayContext>();
        let mut value = ctx.presentation.lock().unwrap();
        if value.visible
            || !settings.learn_from_edits
            || settings.overlay_style == crate::settings::OverlayStyle::None
            || !crate::grain_dictionary::is_current(id)
            || dictionary_contains(&settings.custom_words, &corrected)
        {
            return;
        }
        let deadline = tokio::time::Instant::now() + DICTIONARY_NOTICE_DURATION;
        let expires_at_ms = (std::time::SystemTime::now() + DICTIONARY_NOTICE_DURATION)
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        value.state = "dictionary".into();
        value.visible = true;
        value.dictionary = Some(DictionarySuggestion {
            id,
            original,
            corrected,
            expires_at_ms: Some(expires_at_ms),
        });
        publish(&handle, &value);
        drop(value);
        crate::overlay::show_overlay_state(&handle, "dictionary");
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep_until(deadline).await;
            let app = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                if let Err(error) =
                    finish_dictionary_suggestion(&app, id, DictionaryResolution::Timeout)
                {
                    log::warn!("Could not save dictionary suggestion: {error}");
                }
            });
        });
    });
}

fn dictionary_contains(words: &[String], candidate: &str) -> bool {
    let candidate = candidate.to_lowercase();
    words
        .iter()
        .any(|word| word.trim().to_lowercase() == candidate)
}

pub(crate) fn dismiss_dictionary_suggestion(app: &AppHandle) {
    let ctx = app.state::<OverlayContext>();
    let id = ctx
        .presentation
        .lock()
        .unwrap()
        .dictionary
        .as_ref()
        .map(|suggestion| suggestion.id);
    if let Some(id) = id {
        let _ = finish_dictionary_suggestion(app, id, DictionaryResolution::Decline);
    }
}

fn finish_dictionary_suggestion(
    app: &AppHandle,
    id: u32,
    resolution: DictionaryResolution,
) -> Result<(), String> {
    let ctx = app.state::<OverlayContext>();
    let mut value = ctx.presentation.lock().unwrap();
    let mut saved = false;
    let resolved = value.resolve_dictionary(id, resolution, |corrected| {
        if !crate::grain_dictionary::is_current(id) {
            return Ok(());
        }
        app.state::<std::sync::Arc<grain_core::AppContext>>()
            .update_public_settings(|settings| {
                if settings.learn_from_edits
                    && !dictionary_contains(&settings.custom_words, corrected)
                {
                    settings.custom_words.push(corrected.to_owned());
                    saved = true;
                }
            })
            .map_err(|error| error.to_string())
    });
    if matches!(resolved, Ok(false)) {
        return Ok(());
    }
    publish(app, &value);
    drop(value);
    resolved?;
    crate::overlay::hide_recording_overlay(app);
    if saved {
        let _ = app.emit(
            "settings-changed",
            serde_json::json!({ "setting": "custom_words" }),
        );
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn resolve_dictionary_suggestion(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: u32,
    save: bool,
) -> Result<(), String> {
    if window.label() != "recording_overlay" {
        return Err("The suggestion belongs to the recording overlay.".into());
    }
    let resolution = if save {
        DictionaryResolution::Save
    } else {
        DictionaryResolution::Decline
    };
    finish_dictionary_suggestion(&app, id, resolution)
}

#[derive(Serialize, Deserialize, Type)]
pub struct OverlaySnapshot {
    pub presentation: OverlayPresentation,
    pub position: crate::settings::OverlayPosition,
    pub pill_hide_close_button: bool,
    pub theme: crate::grain_theme::ThemeState,
    pub streaming_width: f64,
    pub streaming_height: f64,
}

#[tauri::command]
#[specta::specta]
pub fn overlay_snapshot(app: AppHandle) -> OverlaySnapshot {
    let settings = crate::settings::get_settings(&app);
    let mut presentation = app
        .state::<OverlayContext>()
        .presentation
        .lock()
        .unwrap()
        .clone();
    presentation.app_icon_enabled = settings.pill_show_app_icon;
    OverlaySnapshot {
        presentation,
        position: settings.overlay_position,
        pill_hide_close_button: settings.pill_hide_close_button,
        theme: crate::grain_theme::get_theme(app.clone()),
        streaming_width: crate::overlay::overlay_dimensions("streaming").0,
        streaming_height: crate::overlay::overlay_dimensions("streaming").1,
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_pill_hide_close_button_setting(app: AppHandle, hidden: bool) -> Result<(), String> {
    app.state::<std::sync::Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.pill_hide_close_button = hidden)
        .map_err(|error| error.to_string())?;
    let _ = app.emit_to(
        "recording_overlay",
        "grain-overlay-compact-close-hidden",
        hidden,
    );
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_overlay_style_setting(
    app: AppHandle,
    style: crate::settings::OverlayStyle,
) -> Result<(), String> {
    app.state::<std::sync::Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.overlay_style = style)
        .map_err(|error| error.to_string())?;
    crate::overlay::update_overlay_enabled_cache(style != crate::settings::OverlayStyle::None);
    if style == crate::settings::OverlayStyle::None {
        crate::grain_dictionary::cancel_watch();
        crate::paste_catch::supersede(&app);
        let ctx = app.state::<OverlayContext>();
        let mut value = ctx.presentation.lock().unwrap();
        value.visible = false;
        value.dictionary = None;
        value.clipboard_offer = None;
        drop(value);
        crate::overlay::hide_recording_overlay(&app);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn overlay_cancel(app: AppHandle) {
    crate::utils::cancel_current_operation(&app);
    crate::grain_actions::cancel_session(&app);
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
    if crate::agent::blocks_dictation(app) {
        let _ = app.emit_to(crate::agent::PANEL_LABEL, "mic-level", levels);
    }
    crate::bridge::emit(
        app,
        DaemonEvent::AudioLevel {
            levels: levels.to_vec(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{DictionaryResolution, DictionarySuggestion, OverlayPresentation};
    use crate::settings::OverlayStyle;

    #[test]
    fn prompt_feedback_is_confirmed_for_its_capture_and_cleared_at_completion() {
        for streaming in [false, true] {
            let mut value = OverlayPresentation::for_capture(streaming, OverlayStyle::Live, false);
            value.session_id = 2;
            assert!(!value.mark_prompt_record(1));
            assert!(!value.prompt_recording);
            assert!(value.mark_prompt_record(2));
            assert!(value.prompt_recording);
            assert!(!value.complete(1));
            assert!(value.prompt_recording);
            assert!(value.complete(2));
            assert!(!value.prompt_recording);
            assert!(!value.app_icon_enabled);
        }
        for (state, working) in [
            ("transcribing", false),
            ("processing", false),
            ("streaming", true),
        ] {
            let mut value = OverlayPresentation {
                session_id: 2,
                state: state.into(),
                working,
                ..Default::default()
            };
            assert!(!value.mark_prompt_record(2));
            assert!(!value.prompt_recording);
        }
    }

    #[test]
    fn missing_icon_is_distinct_from_the_disabled_icon_preference() {
        for enabled in [true, false] {
            let value = OverlayPresentation::for_capture(false, OverlayStyle::Minimal, enabled);
            assert_eq!(value.app_icon_enabled, enabled);
            assert!(value.icon.is_none());
            assert!(!value.prompt_recording);
        }
    }

    #[test]
    fn pill_preference_and_capture_mode_select_presentation() {
        for (style, visible, expected_states) in [
            (OverlayStyle::None, false, ["recording"; 2]),
            (OverlayStyle::Minimal, true, ["recording"; 2]),
            (OverlayStyle::Live, true, ["recording", "streaming"]),
        ] {
            for (streaming, expected_state) in [false, true].into_iter().zip(expected_states) {
                let mut value = OverlayPresentation::for_capture(streaming, style, true);
                value.session_id = 7;
                assert_eq!(value.visible, visible, "{style:?} / {streaming:?}");
                assert_eq!(value.state, expected_state, "{style:?} / {streaming:?}");
                if expected_state == "streaming" {
                    value.stream_text(7, "live words", "tail");
                    assert_eq!(value.committed, "live words");
                    // Working phases keep the live card and its text.
                    value.working = true;
                    assert_eq!(value.state, "streaming");
                    assert_eq!(value.tentative, "tail");
                } else {
                    // Compact and None never open from streamed text; Standard,
                    // Flow and Agent stay compact even with Live selected.
                    value.stream_text(7, "unrelated stream", "tail");
                    assert!(value.committed.is_empty() && value.tentative.is_empty());
                }
                assert!(value.complete(7));
                assert!(!value.visible && value.committed.is_empty());
            }
        }
    }

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

    fn dictionary_offer() -> OverlayPresentation {
        OverlayPresentation {
            visible: true,
            dictionary: Some(DictionarySuggestion {
                id: 7,
                original: "Puneet".into(),
                corrected: "Punit".into(),
                expires_at_ms: Some(10_000),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn declining_a_dictionary_offer_cancels_its_timeout() {
        let mut value = dictionary_offer();
        assert!(!value
            .resolve_dictionary(6, DictionaryResolution::Timeout, |_| panic!(
                "stale timeout"
            ))
            .unwrap());
        assert!(value
            .resolve_dictionary(7, DictionaryResolution::Decline, |_| panic!("declined"))
            .unwrap());
        assert!(!value.visible);
        assert!(!value
            .resolve_dictionary(7, DictionaryResolution::Timeout, |_| panic!(
                "dismissed offer"
            ))
            .unwrap());
    }

    #[test]
    fn a_dictionary_timeout_and_an_explicit_save_each_save_once() {
        for resolution in [DictionaryResolution::Timeout, DictionaryResolution::Save] {
            let mut value = dictionary_offer();
            assert!(value
                .resolve_dictionary(7, resolution, |word| {
                    assert_eq!(word, "Punit");
                    Ok(())
                })
                .unwrap());
            assert!(!value.visible && value.dictionary.is_none());
            assert!(!value
                .resolve_dictionary(7, resolution, |_| panic!("already saved"))
                .unwrap());
        }
    }

    #[test]
    fn a_failed_dictionary_save_stops_the_countdown_and_allows_retry() {
        let mut value = dictionary_offer();
        assert!(value
            .resolve_dictionary(7, DictionaryResolution::Timeout, |_| Err(
                "disk error".into()
            ))
            .is_err());
        assert!(value.visible);
        assert!(value.dictionary.as_ref().unwrap().expires_at_ms.is_none());
        assert!(!value
            .resolve_dictionary(7, DictionaryResolution::Timeout, |_| panic!(
                "countdown stopped"
            ))
            .unwrap());
        assert!(value
            .resolve_dictionary(7, DictionaryResolution::Save, |_| Ok(()))
            .unwrap());
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
    }
}
