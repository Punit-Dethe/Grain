import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "../ui/Dropdown";
import { useSettingsStore } from "../../stores/settingsStore";
import { DefaultPanel as DefaultPanelType } from "@/bindings";

export default function DefaultPanel({ grouped = false }: { grouped?: boolean }) {
  const { t } = useTranslation();
  const getSetting = useSettingsStore((state) => state.getSetting);
  const updateSetting = useSettingsStore((state) => state.updateSetting);

  const defaultPanel = getSetting("default_panel") ?? "settings";

  return (
    <SettingContainer
      title={t("settings.advanced.defaultPanel.title")}
      description={t("settings.advanced.defaultPanel.description")}
      grouped={grouped}
    >
      <Dropdown
        selectedValue={defaultPanel as string}
        onSelect={(value: string) => updateSetting("default_panel", value as DefaultPanelType)}
        options={[
          {
            value: "settings",
            label: t("settings.advanced.defaultPanel.options.settings"),
          },
          {
            value: "quick_panel",
            label: t("settings.advanced.defaultPanel.options.quick_panel"),
          },
        ]}
      />
    </SettingContainer>
  );
}
