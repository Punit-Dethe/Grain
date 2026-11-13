import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ShortcutInput } from "../ShortcutInput";

export function AgentSection() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("agent_enabled") ?? true;
  const screenImage = getSetting("agent_screen_image") ?? false;
  const talkBack = getSetting("agent_talk_back") ?? false;

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

          <ToggleSwitch
            label={t("ui2.settings.agent.screenImage")}
            description={t("ui2.settings.agent.screenImageDescription")}
            descriptionMode="tooltip"
            grouped
            checked={screenImage}
            isUpdating={isUpdating("agent_screen_image")}
            onChange={(v) => updateSetting("agent_screen_image", v)}
          />

          <ToggleSwitch
            label={t("ui2.settings.agent.talkBack")}
            description={t("ui2.settings.agent.talkBackDescription")}
            descriptionMode="tooltip"
            grouped
            checked={talkBack}
            isUpdating={isUpdating("agent_talk_back")}
            onChange={(v) => updateSetting("agent_talk_back", v)}
          />
        </>
      )}
    </SettingsGroup>
  );
}
