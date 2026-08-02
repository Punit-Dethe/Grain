import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface VoiceProcessingToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// [GRAIN] Toggles `audio_conditioning` (85 Hz high-pass + boost-only AGC). Helps
// quiet/laptop mics; defaults on. The backend live-updates the open recorder.
export const VoiceProcessing: React.FC<VoiceProcessingToggleProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("audio_conditioning") ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("audio_conditioning", value)}
        isUpdating={isUpdating("audio_conditioning")}
        label={t("settings.debug.voiceProcessing.label")}
        description={t("settings.debug.voiceProcessing.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
