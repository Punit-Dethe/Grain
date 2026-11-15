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
  const compactVoice = getSetting("agent_compact_voice") ?? true;
  const spokenAck = getSetting("agent_spoken_ack") ?? true;
  const dynamicUi = getSetting("agent_dynamic_ui") ?? true;
  const polishedUi = getSetting("agent_polished_ui") ?? true;

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

          {talkBack && (
            <ToggleSwitch
              label={t("ui2.settings.agent.compactVoice")}
              description={t("ui2.settings.agent.compactVoiceDescription")}
              descriptionMode="tooltip"
              grouped
              checked={compactVoice}
              isUpdating={isUpdating("agent_compact_voice")}
              onChange={(v) => updateSetting("agent_compact_voice", v)}
            />
          )}

          {talkBack && (
            <ToggleSwitch
              label={t("ui2.settings.agent.spokenAck")}
              description={t("ui2.settings.agent.spokenAckDescription")}
              descriptionMode="tooltip"
              grouped
              checked={spokenAck}
              isUpdating={isUpdating("agent_spoken_ack")}
              onChange={(v) => updateSetting("agent_spoken_ack", v)}
            />
          )}

          <ToggleSwitch
            label={t("ui2.settings.agent.dynamicUi")}
            description={t("ui2.settings.agent.dynamicUiDescription")}
            descriptionMode="tooltip"
            grouped
            checked={dynamicUi}
            isUpdating={isUpdating("agent_dynamic_ui")}
            onChange={(v) => updateSetting("agent_dynamic_ui", v)}
          />

          {dynamicUi && (
            <ToggleSwitch
              label={t("ui2.settings.agent.polishedUi")}
              description={t("ui2.settings.agent.polishedUiDescription")}
              descriptionMode="tooltip"
              grouped
              checked={polishedUi}
              isUpdating={isUpdating("agent_polished_ui")}
              onChange={(v) => updateSetting("agent_polished_ui", v)}
            />
          )}
        </>
      )}
    </SettingsGroup>
  );
}
