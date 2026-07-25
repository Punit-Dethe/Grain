import React from "react";
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

/** [GRAIN] Context awareness settings — the rows BELOW the feature's own switch,
 * rendered inside its panel (see [`FeaturePanel`]), not a section of their own.
 *
 * The automatic SOFT tone/vocabulary layer is the feature itself and has no
 * setting; what is configurable is how much it may read. HARD per-app formatting
 * is not here either — it is what the App Modes extension does, in its own
 * storage and its own transform hook, anchored directly below. */
export const ContextAwareSection: React.FC = () => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const nearbyTerms = getSetting("context_nearby_terms") ?? false;

  return (
    <ToggleSwitch
      label="Nearby-term hints (silent)"
      description="Read UNIQUE names and identifiers (e.g. Rita, useGrainStore, PyTorch) from the field you're dictating into and pass them as a spelling hint only. Never sends raw text, never stored, password fields skipped. Improves accuracy on names and jargon."
      descriptionMode="tooltip"
      grouped
      checked={nearbyTerms}
      isUpdating={isUpdating("context_nearby_terms")}
      onChange={(v) => updateSetting("context_nearby_terms", v)}
    />
  );
};
