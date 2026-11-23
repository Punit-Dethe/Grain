import { useEffect, useState } from "react";
import { platform } from "@tauri-apps/plugin-os";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands, type AgentVoicePreset, type SourceDevice } from "@/bindings";
import { deviceOptions } from "../deviceOptions";
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

/** Where talk-back plays. "System default" follows the default output, also
 * when it changes mid-reply. The list is re-read when the window regains
 * focus (devices come and go). */
function VoiceOutputPicker() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [devices, setDevices] = useState<SourceDevice[]>([]);
  useEffect(() => {
    let live = true;
    const load = () =>
      void commands.agentVoiceOutputs().then((result) => {
        if (live && result.status === "ok") setDevices(result.data);
      });
    load();
    window.addEventListener("focus", load);
    return () => {
      live = false;
      window.removeEventListener("focus", load);
    };
  }, []);
  const selected = getSetting("agent_voice_output") ?? "";
  return (
    <SettingContainer
      title={t("ui2.settings.agent.voiceOutput")}
      description={t("ui2.settings.agent.voiceOutputDescription")}
      descriptionMode="tooltip"
      grouped
    >
      <Dropdown
        options={deviceOptions(devices, selected, t)}
        selectedValue={selected}
        onSelect={(value) =>
          void updateSetting("agent_voice_output", value || null)
        }
        disabled={isUpdating("agent_voice_output")}
        ariaLabel={t("ui2.settings.agent.voiceOutput")}
      />
    </SettingContainer>
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
          {platform() === "linux" && (
            <SettingContainer
              title={t("agent.openChat")}
              description={t("agent.openChatDescription")}
              grouped
            >
              <button
                className="button"
                type="button"
                onClick={() => {
                  void commands
                    .agentOpenChat()
                    .then((result) => {
                      if (result.status === "error") toast.error(result.error);
                    })
                    .catch((error) => toast.error(String(error)));
                }}
              >
                {t("common.open")}
              </button>
            </SettingContainer>
          )}

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

          {talkBack && <VoiceOutputPicker />}

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
