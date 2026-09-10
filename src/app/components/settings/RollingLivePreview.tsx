import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { getFlowAvailability } from "@/lib/flowAvailability";

interface RollingLivePreviewProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// [GRAIN] Toggle the rolling live preview — a growing caption in the Studio
// Window while dictating in the real-time (rolling) mode. OFF by default and
// OFF is genuinely zero-cost (the rolling worker never decodes the tail); ON
// adds an inter-chunk tail decode, so it trades compute for a live caption.
// Wired to `rolling_live_preview` via `changeRollingLivePreviewSetting`.
export const RollingLivePreview: React.FC<RollingLivePreviewProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const { models, currentModel } = useModelStore();

    const enabled = getSetting("rolling_live_preview") ?? false;
    const availability = getFlowAvailability(
      models,
      currentModel,
      getSetting("translate_to_english") ?? false,
    );
    const description = availability.available
      ? t("settings.speechToText.rollingLivePreview.description")
      : t(`settings.speechToText.flowUnavailable.${availability.reason}`);

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(v) => updateSetting("rolling_live_preview", v)}
        isUpdating={isUpdating("rolling_live_preview")}
        label={t("settings.speechToText.rollingLivePreview.label")}
        description={description}
        descriptionMode={descriptionMode}
        grouped={grouped}
        disabled={!availability.available}
      />
    );
  },
);
