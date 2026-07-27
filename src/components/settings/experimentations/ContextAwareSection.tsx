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
  const caretText = getSetting("context_caret_text") ?? false;

  return (
    <>
      <ToggleSwitch
        label="Nearby-term hints (silent)"
        description="Read UNIQUE names and identifiers (e.g. Rita, useGrainStore, PyTorch) from the field you're dictating into and use them to improve accuracy — both as a spelling hint for the AI and to bias the recognizer itself. Never sends raw text, never stored, password fields skipped."
        descriptionMode="tooltip"
        grouped
        checked={nearbyTerms}
        isUpdating={isUpdating("context_nearby_terms")}
        onChange={(v) => updateSetting("context_nearby_terms", v)}
      />
      {/* Deliberately a second switch rather than part of the one above: that
          one promises to send only unique tokens and never raw text, and this
          one sends a short raw excerpt of what surrounds the cursor. Folding
          them together would quietly break the narrower promise. */}
      <ToggleSwitch
        label="Fit text to the cursor"
        description="Read a short span of text either side of your cursor so dictation inserted mid-sentence flows: correct spacing, no stray capital, no repeated words. Sends that excerpt (up to ~320 characters each side) to your AI provider along with the transcript. Never stored, password fields skipped."
        descriptionMode="tooltip"
        grouped
        checked={caretText}
        isUpdating={isUpdating("context_caret_text")}
        onChange={(v) => updateSetting("context_caret_text", v)}
      />
    </>
  );
};
