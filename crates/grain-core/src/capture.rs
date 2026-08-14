//! [GRAIN] Capture-mode policy.
//!
//! Grain has three ways to start a capture — Standard, Flow and Live — and a
//! fourth shortcut that routes a transcript to AI. All three capture modes are
//! always live; the only per-mode question left is what the AI key starts.
//!
//! This module is the single place that answers "what does the AI key do, and
//! which shortcuts hold a global hotkey?". The Handy-derived shortcut backends
//! and the coordinator both call `shortcut_holds_hotkey`, so the two keyboard
//! implementations cannot drift apart on it.

use crate::settings::{AppSettings, CAPTURE_MODE_IDS};

/// Is `id` one of the three capture-starting bindings?
pub fn is_capture_mode(id: &str) -> bool {
    CAPTURE_MODE_IDS.contains(&id)
}

/// Bindings that are registered **dynamically** — held only while the surface
/// that owns them is live, never at init:
///
/// - `cancel` — while a recording is running.
/// - `agent_followup` — while an Agent surface (panel / pill offer) is open.
/// - `paste_catch_deliver` — while Grain is holding a transcript whose paste
///   missed the text field.
///
/// This is the list every registration path consults, so adding a dynamic
/// binding is a change here and nowhere else. Registering one of these globally
/// would squat on the user's keys for a surface that is not on screen.
pub fn is_dynamic_binding(id: &str) -> bool {
    matches!(id, "cancel" | "agent_followup" | "paste_catch_deliver")
}

/// Whether a shortcut id should hold a global hotkey at registration time,
/// given the current settings.
///
/// This is the **single source of truth** for shortcut gating, called from every
/// registration path (both keyboard-implementation inits AND the impl-switch
/// re-register). It used to be inline in all three; the impl-switch copy drifted
/// and left disabled features holding global hotkeys after a Tauri↔HandyKeys
/// switch. One function means they cannot disagree again.
///
/// Dynamic bindings are never held here — see [`is_dynamic_binding`].
pub fn shortcut_holds_hotkey(settings: &AppSettings, id: &str) -> bool {
    if is_dynamic_binding(id) {
        return false;
    }
    // The AI key must not hold a hotkey with no post-processing behind it.
    // `transcribe_with_post_process` no longer ships a binding (it survives only
    // as a CLI/SIGUSR1 action id), but a settings file written before it retired
    // can still name it, so the gate keeps covering it.
    if (id == "transcribe_with_post_process" || id == "transcribe_send_to_ai")
        && !settings.post_process_enabled
    {
        return false;
    }
    // Feature-gated shortcuts vanish when their feature is off — OFF must be
    // truly zero-overhead (no global hooks for a disabled feature).
    if id == "summon_agent" && !settings.agent_enabled {
        return false;
    }
    if id.starts_with("grain_space_") && !settings.grain_space_enabled {
        return false;
    }
    true
}

/// Which mode the AI shortcut starts when pressed from idle.
///
/// All three capture modes are always live, so this is a free choice. Falls
/// back to Standard if the stored value is not a mode we ship — settings files
/// outlive the code that wrote them.
pub fn ai_start_mode(settings: &AppSettings) -> &str {
    if is_capture_mode(&settings.capture_ai_start_mode) {
        &settings.capture_ai_start_mode
    } else {
        CAPTURE_MODE_IDS[0]
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

    #[test]
    fn every_capture_mode_holds_a_key() {
        // All three capture modes are always live — none is ever narrowed away.
        let s = get_default_settings();
        for id in CAPTURE_MODE_IDS {
            assert!(shortcut_holds_hotkey(&s, id));
        }
    }

    #[test]
    fn ai_start_mode_is_a_free_choice() {
        let mut s = get_default_settings();
        s.capture_ai_start_mode = "transcribe_realtime".to_string();
        assert_eq!(ai_start_mode(&s), "transcribe_realtime");
    }

    #[test]
    fn an_unknown_stored_ai_start_mode_falls_back_to_standard() {
        // A settings file from a future or hand-edited build must not name a
        // mode we do not ship as the AI start mode.
        let mut s = get_default_settings();
        s.capture_ai_start_mode = "transcribe_teleport".to_string();
        assert_eq!(ai_start_mode(&s), "transcribe");
    }

    #[test]
    fn the_ai_key_borrows_a_capture_engine_but_others_run_their_own() {
        let mut s = get_default_settings();
        s.capture_ai_start_mode = "transcribe_realtime".to_string();
        assert_eq!(action_id_for(&s, "transcribe_send_to_ai"), "transcribe_realtime");
        assert_eq!(action_id_for(&s, "transcribe"), "transcribe");
        assert_eq!(action_id_for(&s, "summon_agent"), "summon_agent");
    }

    #[test]
    fn always_ai_routes_every_mode_but_still_needs_post_processing() {
        let mut s = get_default_settings();
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
        let mut s = get_default_settings();
        s.post_process_enabled = true;
        assert!(!should_route_to_ai(&s, "transcribe"));
        assert!(!should_route_to_ai(&s, "transcribe_realtime"));
        assert!(should_route_to_ai(&s, "transcribe_send_to_ai"));
        assert!(should_route_to_ai(&s, "transcribe_with_post_process"));
    }

    #[test]
    fn gate_hides_disabled_features_but_keeps_every_capture_mode() {
        // Every feature off: the capture modes still hold keys (always live);
        // the feature-gated keys do not.
        let mut s = get_default_settings();
        s.post_process_enabled = false;
        s.agent_enabled = false;
        s.grain_space_enabled = false;

        // All three capture modes hold a key regardless of feature toggles.
        for id in CAPTURE_MODE_IDS {
            assert!(shortcut_holds_hotkey(&s, id));
        }
        // AI entry points need post-processing behind them.
        assert!(!shortcut_holds_hotkey(&s, "transcribe_send_to_ai"));
        assert!(!shortcut_holds_hotkey(&s, "transcribe_with_post_process"));
        // Feature-gated keys vanish with their feature.
        assert!(!shortcut_holds_hotkey(&s, "summon_agent"));
        assert!(!shortcut_holds_hotkey(&s, "grain_space_capture"));
        // Dynamic keys are never held at registration time.
        assert!(!shortcut_holds_hotkey(&s, "cancel"));
        assert!(!shortcut_holds_hotkey(&s, "agent_followup"));
        assert!(!shortcut_holds_hotkey(&s, "paste_catch_deliver"));
        // An unrelated shortcut is untouched.
        assert!(shortcut_holds_hotkey(&s, "prompt_next"));
    }

    #[test]
    fn dynamic_bindings_stay_dynamic_regardless_of_settings() {
        // The gate must not let a feature toggle promote a dynamic binding to a
        // globally-held hotkey — that would squat on the keys while the surface
        // that owns them is off screen.
        let mut s = get_default_settings();
        s.post_process_enabled = true;
        s.agent_enabled = true;
        s.paste_catch_enabled = true;
        for id in ["cancel", "agent_followup", "paste_catch_deliver"] {
            assert!(is_dynamic_binding(id));
            assert!(!shortcut_holds_hotkey(&s, id));
        }
        assert!(!is_dynamic_binding("prompt_next"));
    }

    #[test]
    fn gate_registers_everything_its_features_are_on() {
        let mut s = get_default_settings();
        s.post_process_enabled = true;
        s.agent_enabled = true;
        s.grain_space_enabled = true;

        for id in CAPTURE_MODE_IDS {
            assert!(shortcut_holds_hotkey(&s, id));
        }
        assert!(shortcut_holds_hotkey(&s, "transcribe_send_to_ai"));
        assert!(shortcut_holds_hotkey(&s, "summon_agent"));
        assert!(shortcut_holds_hotkey(&s, "grain_space_capture"));
    }
}
