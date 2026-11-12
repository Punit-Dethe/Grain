import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AgentNotchAutohideProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** [GRAIN] Hide the resting Agent notch until the pointer reaches it. */
export const AgentNotchAutohide: React.FC<AgentNotchAutohideProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    // The notch only exists while the Agent is on.
    if (!(getSetting("agent_enabled") ?? true)) return null;

    return (
      <ToggleSwitch
        checked={getSetting("agent_notch_autohide") ?? false}
        onChange={(enabled) => updateSetting("agent_notch_autohide", enabled)}
        isUpdating={isUpdating("agent_notch_autohide")}
        label={t("settings.advanced.agentNotchAutohide.label")}
        description={t("settings.advanced.agentNotchAutohide.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        tooltipPosition="bottom"
      />
    );
  },
);

AgentNotchAutohide.displayName = "AgentNotchAutohide";
