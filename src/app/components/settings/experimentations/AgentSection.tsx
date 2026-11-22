import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type AgentVoicePreset } from "@/bindings";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ShortcutInput } from "../ShortcutInput";
import { GrainModelRow, useGrainModel } from "../GrainModelRow";

/** The talk-back voice model: shown while talk-back is on, and whenever it is
 * downloaded or downloading (so it can be deleted with talk-back off).
 * Deleting it turns talk-back off. */
function VoiceModelRow({ talkBack }: { talkBack: boolean }) {
  const { t } = useTranslation();
  const { updateSetting } = useSettings();
  const state = useGrainModel("TalkBackVoice");
  const status = state.status;
  if (!status) return null;
  if (!talkBack && !status.installed && !status.downloading) return null;
  return (
    <GrainModelRow
      title={t("ui2.settings.agent.voiceModel")}
      state={state}
      onDeleted={() => {
        if (talkBack) void updateSetting("agent_talk_back", false);
      }}
    />
  );
}

/** The talk-back voice: presets come from the backend (`grain_tts::VOICES`). */
function VoicePicker() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [presets, setPresets] = useState<AgentVoicePreset[]>([]);
  useEffect(() => {
    let live = true;
    void commands.agentVoicePresets().then((list) => {
      if (live) setPresets(list);
    });
    return () => {
      live = false;
    };
  }, []);
  const voice = getSetting("agent_voice") ?? presets[0]?.name ?? null;
  const options = presets.map((preset) => ({
    value: preset.name,
    label: `${preset.name} · ${t(`ui2.settings.agent.voiceAccent.${preset.accent}`)}`,
    group: t(`ui2.settings.agent.voiceGender.${preset.gender}`),
  }));
  return (
    <SettingContainer
      title={t("ui2.settings.agent.voice")}
      description={t("ui2.settings.agent.voiceDescription")}
      descriptionMode="tooltip"
      grouped
    >
      <Dropdown
        options={options}
        selectedValue={voice}
        onSelect={(value) => void updateSetting("agent_voice", value)}
        disabled={!presets.length || isUpdating("agent_voice")}
        ariaLabel={t("ui2.settings.agent.voice")}
      />
    </SettingContainer>
  );
}

export function AgentSection() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("agent_enabled") ?? true;
  const screenImage = getSetting("agent_screen_image") ?? false;
  const talkBack = getSetting("agent_talk_back") ?? false;
  const compactVoice = getSetting("agent_compact_voice") ?? true;
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

          <VoiceModelRow talkBack={talkBack} />

          {talkBack && <VoicePicker />}

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
