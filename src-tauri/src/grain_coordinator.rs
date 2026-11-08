// Included in Handy's coordinator so its production transition policy stays shared.
impl CoordinatorState {
    fn grain_on_input(
        &mut self,
        app: &AppHandle,
        input: InputEvent,
        now: Instant,
    ) -> Option<Effect> {
        let end_with_ai = input.is_pressed
            && input.binding_id == "transcribe_send_to_ai"
            && grain_core::capture::ends_with_ai(&crate::settings::get_settings(app));
        self.grain_input_with_ai(input, now, end_with_ai)
    }

    fn grain_input_with_ai(
        &mut self,
        mut input: InputEvent,
        now: Instant,
        end_with_ai: bool,
    ) -> Option<Effect> {
        let finish_with_ai = end_with_ai
            && input.is_pressed
            && input.binding_id == "transcribe_send_to_ai"
            && (self.is_locked() || input.mode == ShortcutActivation::Toggle);
        let mut remapped = false;
        if finish_with_ai {
            if let Stage::Recording(binding) = &self.stage {
                if binding != &input.binding_id {
                    input.binding_id = binding.clone();
                    remapped = true;
                }
            }
        }
        let effect = self.on_input(input, now);
        if remapped && matches!(effect, Some(Effect::Stop { .. })) {
            self.grain_finish_with_ai = true;
        }
        effect
    }

    fn grain_start(&mut self, app: &AppHandle, binding: &str, hotkey: &str) -> bool {
        let _start = crate::grain_actions::capture_start_guard();
        self.grain_action = None;
        self.grain_finish_with_ai = false;
        if crate::grain_actions::dictation_start_blocked(app) {
            return false;
        }
        let action_id = crate::grain_dictation_routing::action_id_for(app, binding);
        let Some(action) = crate::actions::ACTION_MAP.get(&action_id) else {
            return false;
        };
        action.start(app, &action_id, hotkey);
        let started = app
            .try_state::<std::sync::Arc<crate::managers::audio::AudioRecordingManager>>()
            .is_some_and(|audio| audio.is_recording());
        if started {
            self.grain_action = Some(action_id);
        }
        started
    }

    fn grain_take_action(&mut self) -> Option<String> {
        self.grain_action.take()
    }

    fn grain_stop(&mut self, app: &AppHandle, binding: &str, hotkey: &str) {
        let Some(action_id) = self.grain_take_action() else {
            warn!("No retained recording action for '{binding}'");
            return;
        };
        let send_to_ai = std::mem::take(&mut self.grain_finish_with_ai)
            || grain_core::capture::should_route_to_ai(
                &crate::settings::get_settings(app),
                binding,
            );
        let Some(action) = crate::actions::ACTION_MAP.get(&action_id) else {
            return;
        };
        if send_to_ai {
            action.set_post_process_override(true);
        }
        action.stop(app, &action_id, hotkey);
        if send_to_ai {
            action.set_post_process_override(false);
        }
    }
}

#[cfg(test)]
mod grain_routing_tests {
    use super::*;

    fn input(binding: &str, mode: ShortcutActivation, pressed: bool) -> InputEvent {
        InputEvent {
            binding_id: binding.into(),
            hotkey_string: "test".into(),
            is_pressed: pressed,
            mode,
            hold_threshold: crate::grain_shortcuts::HOLD_THRESHOLD,
            external: false,
        }
    }

    #[test]
    fn alternate_ai_finish_retains_the_started_engine_and_original_binding() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();
        state.on_input(input("transcribe", ShortcutActivation::Toggle, true), now);
        state.grain_action = Some("transcribe_realtime".into());
        let effect = state.grain_input_with_ai(
            input("transcribe_send_to_ai", ShortcutActivation::Toggle, true),
            now + Duration::from_millis(40),
            true,
        );
        assert!(
            matches!(effect, Some(Effect::Stop { binding_id, .. }) if binding_id == "transcribe")
        );
        assert!(state.grain_finish_with_ai);
        assert_eq!(
            state.grain_take_action().as_deref(),
            Some("transcribe_realtime")
        );
        assert!(state.grain_take_action().is_none());
    }

    #[test]
    fn disabled_ai_finish_and_held_ptt_do_not_stop_another_binding() {
        for (mode, enabled) in [
            (ShortcutActivation::Toggle, false),
            (ShortcutActivation::PushToTalk, true),
            (ShortcutActivation::HoldOrToggle, true),
        ] {
            let mut state = CoordinatorState::new();
            let now = Instant::now();
            state.on_input(input("transcribe_native_asr", mode, true), now);
            assert!(state
                .grain_input_with_ai(
                    input("transcribe_send_to_ai", mode, true),
                    now + Duration::from_millis(100),
                    enabled
                )
                .is_none());
            assert!(matches!(state.stage, Stage::Recording(_)));
            assert!(!state.grain_finish_with_ai);
        }
    }

    #[test]
    fn tapped_hold_or_toggle_can_finish_with_ai_and_cancel_clears_routing() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();
        state.on_input(
            input("transcribe", ShortcutActivation::HoldOrToggle, true),
            now,
        );
        state.on_input(
            input("transcribe", ShortcutActivation::HoldOrToggle, false),
            now + Duration::from_millis(100),
        );
        state.on_grace_expired();
        assert!(state.is_locked());
        assert!(matches!(
            state.grain_input_with_ai(
                input(
                    "transcribe_send_to_ai",
                    ShortcutActivation::HoldOrToggle,
                    true
                ),
                now + Duration::from_millis(200),
                true
            ),
            Some(Effect::Stop { .. })
        ));
        state.grain_action = Some("transcribe".into());
        state.on_cancel(false);
        assert!(state.grain_take_action().is_none());
        assert!(!state.grain_finish_with_ai);
    }
}
