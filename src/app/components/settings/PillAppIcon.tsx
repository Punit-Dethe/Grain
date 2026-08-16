import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface PillAppIconProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * [GRAIN] Pill identity — show the icon of the app being dictated into in place
 * of the pill's state dot. On while the behaviour is being developed; it will
 * later fold into Context Awareness and appear only for surfaces Grain actually
 * treats differently.
 */
export const PillAppIcon: React.FC<PillAppIconProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("pill_show_app_icon") ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("pill_show_app_icon", value)}
        isUpdating={isUpdating("pill_show_app_icon")}
        label={t("settings.advanced.pillAppIcon.label")}
        description={t("settings.advanced.pillAppIcon.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        tooltipPosition="bottom"
      />
    );
  },
);
