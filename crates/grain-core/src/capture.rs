//! [GRAIN] Capture-mode policy.
//!
//! Grain has three ways to start a capture — Standard, Flow and Live — and a
//! fourth shortcut that routes a transcript to AI. Registering all four
//! globally means four keys to remember before the user has said a word, and
//! in practice almost everyone lives in one mode.
//!
//! This module is the single place that answers "which capture shortcuts are
//! live, and what does the AI key do?". The Handy-derived shortcut backends
//! each call `capture_binding_is_active` from their existing skip list; the
//! coordinator calls the rest. Keeping the decision here means the two
//! keyboard implementations cannot drift apart on it.

use crate::settings::{AppSettings, CaptureModeSet, CAPTURE_MODE_IDS};

/// Is `id` one of the three capture-starting bindings?
pub fn is_capture_mode(id: &str) -> bool {
    CAPTURE_MODE_IDS.contains(&id)
}

/// The capture mode the user has nominated as their one mode. Falls back to
/// Standard if the stored value is not a mode we ship — settings files outlive
/// the code that wrote them, and an unknown id must not leave the user with no
/// capture shortcut at all.
pub fn primary_mode(settings: &AppSettings) -> &str {
    if is_capture_mode(&settings.capture_primary_mode) {
        &settings.capture_primary_mode
    } else {
        CAPTURE_MODE_IDS[0]
    }
}

/// Should this binding hold a global shortcut, given the capture-mode setting?
///
/// Only ever narrows capture bindings — every other shortcut in the app is
/// none of this module's business and passes straight through.
pub fn capture_binding_is_active(settings: &AppSettings, id: &str) -> bool {
    if !is_capture_mode(id) {
        return true;
    }
    match settings.capture_mode_set {
        CaptureModeSet::All => true,
        CaptureModeSet::Single => id == primary_mode(settings),
    }
}

/// Which mode the AI shortcut starts when pressed from idle.
///
/// Under `Single` this is necessarily the primary mode — offering a choice
/// there would let the AI key start a mode whose own shortcut the user just
/// turned off, which is the opposite of simplifying.
pub fn ai_start_mode(settings: &AppSettings) -> &str {
    match settings.capture_mode_set {
        CaptureModeSet::Single => primary_mode(settings),
        CaptureModeSet::All => {
            if is_capture_mode(&settings.capture_ai_start_mode) {
                &settings.capture_ai_start_mode
            } else {
                CAPTURE_MODE_IDS[0]
            }
        }
    }
}

/// The capture action a trigger key actually runs.
///
/// Every key is its own action except the AI key, which has no capture engine
/// of its own — it borrows whichever mode the user nominated. The session is
/// still *staged* under the trigger key, so push-to-talk release matching and
/// the tap-to-stop path keep working against the key the user is holding.
pub fn action_id_for<'a>(settings: &'a AppSettings, binding_id: &'a str) -> &'a str {
    if binding_id == "transcribe_send_to_ai" {
        ai_start_mode(settings)
    } else {
        binding_id
    }
}

/// Should the finished transcript go to AI, for a capture started by `id`?
///
/// `capture_always_ai` makes every capture an AI capture, which collapses the
/// product to a single key. Otherwise only the two shortcuts that mean AI by
/// construction route there.
///
/// Gated on `post_process_enabled` throughout: routing to AI with no
/// post-processing configured would drop the transcript into a pipeline that
/// cannot run.
pub fn should_route_to_ai(settings: &AppSettings, id: &str) -> bool {
    if !settings.post_process_enabled {
        return false;
    }
    settings.capture_always_ai
        || id == "transcribe_send_to_ai"
        || id == "transcribe_with_post_process"
}

/// Does the AI shortcut end an in-progress capture and send it to AI?
pub fn ends_with_ai(settings: &AppSettings) -> bool {
    settings.post_process_enabled && settings.capture_end_with_ai
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn settings_with(set: CaptureModeSet, primary: &str) -> AppSettings {
        let mut s = get_default_settings();
        s.capture_mode_set = set;
        s.capture_primary_mode = primary.to_string();
        s
    }

    #[test]
    fn single_registers_only_the_primary_capture_mode() {
        let s = settings_with(CaptureModeSet::Single, "transcribe_realtime");
        assert!(capture_binding_is_active(&s, "transcribe_realtime"));
        assert!(!capture_binding_is_active(&s, "transcribe"));
        assert!(!capture_binding_is_active(&s, "transcribe_native_asr"));
    }

    #[test]
    fn all_registers_every_capture_mode() {
        let s = settings_with(CaptureModeSet::All, "transcribe");
        for id in CAPTURE_MODE_IDS {
            assert!(capture_binding_is_active(&s, id));
        }
    }

    #[test]
    fn non_capture_bindings_are_never_narrowed() {
        let s = settings_with(CaptureModeSet::Single, "transcribe");
        for id in ["cancel", "summon_agent", "transcribe_send_to_ai", "prompt_next"] {
            assert!(capture_binding_is_active(&s, id));
        }
    }

    #[test]
    fn an_unknown_stored_mode_still_leaves_one_capture_shortcut() {
        // A settings file from a future or hand-edited build must not be able
        // to leave the user unable to start a capture at all.
        let s = settings_with(CaptureModeSet::Single, "transcribe_teleport");
        assert!(capture_binding_is_active(&s, "transcribe"));
        assert_eq!(primary_mode(&s), "transcribe");
    }

    #[test]
    fn ai_start_mode_follows_the_primary_when_only_one_mode_is_live() {
        let s = settings_with(CaptureModeSet::Single, "transcribe_native_asr");
        // Even though the stored AI mode says otherwise: under Single it would
        // start a mode the user has no shortcut for.
        assert_eq!(ai_start_mode(&s), "transcribe_native_asr");
    }

    #[test]
    fn ai_start_mode_is_independent_when_all_modes_are_live() {
        let mut s = settings_with(CaptureModeSet::All, "transcribe");
        s.capture_ai_start_mode = "transcribe_realtime".to_string();
        assert_eq!(ai_start_mode(&s), "transcribe_realtime");
    }

    #[test]
    fn the_ai_key_borrows_a_capture_engine_but_others_run_their_own() {
        let mut s = settings_with(CaptureModeSet::All, "transcribe");
        s.capture_ai_start_mode = "transcribe_realtime".to_string();
        assert_eq!(action_id_for(&s, "transcribe_send_to_ai"), "transcribe_realtime");
        assert_eq!(action_id_for(&s, "transcribe"), "transcribe");
        assert_eq!(action_id_for(&s, "summon_agent"), "summon_agent");
    }

    #[test]
    fn always_ai_routes_every_mode_but_still_needs_post_processing() {
        let mut s = settings_with(CaptureModeSet::Single, "transcribe");
        s.post_process_enabled = true;
        s.capture_always_ai = true;
        assert!(should_route_to_ai(&s, "transcribe"));

        s.post_process_enabled = false;
        assert!(!should_route_to_ai(&s, "transcribe"));
        assert!(!should_route_to_ai(&s, "transcribe_send_to_ai"));
        assert!(!ends_with_ai(&s));
    }

    #[test]
    fn without_always_ai_only_the_ai_shortcuts_route_there() {
        let mut s = settings_with(CaptureModeSet::All, "transcribe");
        s.post_process_enabled = true;
        assert!(!should_route_to_ai(&s, "transcribe"));
        assert!(!should_route_to_ai(&s, "transcribe_realtime"));
        assert!(should_route_to_ai(&s, "transcribe_send_to_ai"));
        assert!(should_route_to_ai(&s, "transcribe_with_post_process"));
    }
}
