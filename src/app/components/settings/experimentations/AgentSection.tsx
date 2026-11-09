import { useTranslation } from "react-i18next";
import type { AgentPanelPosition } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ShortcutInput } from "../ShortcutInput";

export function AgentSection() {
  const { t } = useTranslation();
  const lookOptions = [
    { value: "side", label: t("ui2.settings.agent.sideCard") },
    { value: "center", label: t("ui2.settings.agent.centerPanel") },
  ];
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("agent_enabled") ?? true;
  const screenImage = getSetting("agent_screen_image") ?? false;
  const panelPosition = getSetting("agent_panel_position") ?? "side";

  return (
    <SettingsGroup title={t("ui2.settings.agent.group")}>
      <ToggleSwitch
        label={t("ui2.settings.agent.enabled")}
        description={t("ui2.settings.agent.enabledDescription")}
        grouped
        checked={enabled}
        isUpdating={isUpdating("agent_enabled")}
        onChange={(value) => updateSetting("agent_enabled", value)}
      />
      {enabled && (
        <>
          <ShortcutInput
            shortcutId="summon_agent"
            grouped
            descriptionMode="tooltip"
          />

          <SettingContainer
            title={t("ui2.settings.agent.replyLayout")}
            description={t("ui2.settings.agent.replyLayoutDescription")}
            descriptionMode="tooltip"
            grouped
          >
            <Dropdown
              options={lookOptions}
              selectedValue={panelPosition}
              disabled={isUpdating("agent_panel_position")}
              onSelect={(v) =>
                updateSetting("agent_panel_position", v as AgentPanelPosition)
              }
            />
          </SettingContainer>

          <ShortcutInput
            shortcutId="agent_followup"
            grouped
            descriptionMode="tooltip"
          />

          <ToggleSwitch
            label={t("ui2.settings.agent.screenImage")}
            description={t("ui2.settings.agent.screenImageDescription")}
            descriptionMode="tooltip"
            grouped
            checked={screenImage}
            isUpdating={isUpdating("agent_screen_image")}
            onChange={(v) => updateSetting("agent_screen_image", v)}
          />
        </>
      )}
    </SettingsGroup>
  );
}
