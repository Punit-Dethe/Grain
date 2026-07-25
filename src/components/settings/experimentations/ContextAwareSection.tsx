import React from "react";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

/** [GRAIN] Context awareness: the automatic SOFT tone/vocabulary layer applied on
 * top of the selected post-processing prompt.
 *
 * HARD per-app formatting is NOT here. It used to be a built-in "Modes" editor,
 * and it is now what the App Modes extension does — its own storage, its own
 * transform hook, anchored at `context.after` so it renders directly below this.
 * Two editors for one idea is worse than either, and the extension is the one
 * that can be improved without shipping a new Grain. */
export const ContextAwareSection: React.FC = () => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("context_awareness_enabled") ?? false;
  const nearbyTerms = getSetting("context_nearby_terms") ?? false;

  return (
    <SettingsGroup
      title="Context awareness"
      info="Adapts post-processing to the app you're dictating into. It softly nudges tone and vocabulary (an IDE keeps technical terms; chat stays casual; email gets slightly more polished) without hard-reformatting. Applied on top of your selected post-processing prompt. Requires post-processing to be on."
    >
      {/* NOTE: the master "enable context awareness" switch is NOT here — it is
          the tab header (FeatureToggle in ExperimentationsSettings), so all
          three core features turn on and off in the same place. */}
      <ToggleSwitch
        label="Nearby-term hints (silent)"
        description="Read UNIQUE names and identifiers (e.g. Rita, useGrainStore, PyTorch) from the field you're dictating into and pass them as a spelling hint only. Never sends raw text, never stored, password fields skipped. Improves accuracy on names and jargon."
        descriptionMode="tooltip"
        grouped
        disabled={!enabled}
        checked={nearbyTerms}
        isUpdating={isUpdating("context_nearby_terms")}
        onChange={(v) => updateSetting("context_nearby_terms", v)}
      />
    </SettingsGroup>
  );
};
