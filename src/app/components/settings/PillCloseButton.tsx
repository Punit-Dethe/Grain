import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

export function PillCloseButton() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  return (
    <ToggleSwitch
      checked={getSetting("pill_hide_close_button") ?? false}
      onChange={(hidden) => updateSetting("pill_hide_close_button", hidden)}
      isUpdating={isUpdating("pill_hide_close_button")}
      label={t("settings.advanced.pillCloseButton.label")}
      description={t("settings.advanced.pillCloseButton.description")}
      descriptionMode="tooltip"
      grouped
      tooltipPosition="bottom"
    />
  );
}
