import React from "react";
import { useTranslation } from "react-i18next";
import { type } from "@tauri-apps/plugin-os";
import { MicrophoneSelector } from "../MicrophoneSelector";
import { ShortcutInput } from "../ShortcutInput";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { OutputDeviceSelector } from "../OutputDeviceSelector";
import { PushToTalk } from "../PushToTalk";
import { AudioFeedback } from "../AudioFeedback";
import { useSettings } from "../../../hooks/useSettings";
import { VolumeSlider } from "../VolumeSlider";
import { MuteWhileRecording } from "../MuteWhileRecording";
import { VoiceProcessing } from "../VoiceProcessing";
import { PostProcessingToggle } from "../PostProcessingToggle";
import { ModelSettingsCard } from "./ModelSettingsCard";

export const GeneralSettings: React.FC = () => {
  const { t } = useTranslation();
  const { audioFeedbackEnabled, getSetting } = useSettings();
  const pushToTalk = getSetting("push_to_talk");
  const postProcessEnabled = getSetting("post_process_enabled");
  const isLinux = type() === "linux";

  return (
    <div className="max-w-4xl w-full mx-auto space-y-7">
      <div className="px-1">
        <h1 className="text-xl font-semibold mb-1">
          {t("settings.general.title")}
        </h1>
        <p className="text-sm text-ink-soft">{t("settings.general.description")}</p>
      </div>

      {/* Capture modes — one rebindable shortcut per recording mode. Grouping
          the three "start a recording" keys together makes them legible as a
          set, instead of a wall of look-alike rows. */}
      <SettingsGroup title={t("settings.general.groups.captureModes")}>
        <ShortcutInput shortcutId="transcribe" grouped={true} />
        <ShortcutInput shortcutId="transcribe_realtime" grouped={true} />
        <ShortcutInput shortcutId="transcribe_with_post_process" grouped={true} />
        {/* [GRAIN] Summon the voice-first AI agent on the current selection. */}
        <ShortcutInput shortcutId="summon_agent" grouped={true} />
      </SettingsGroup>

      {/* Recording behaviour — how a capture is held and cancelled. */}
      <SettingsGroup title={t("settings.general.groups.behaviour")}>
        <PushToTalk descriptionMode="tooltip" grouped={true} />
        {/* Cancel shortcut is hidden with push-to-talk (release key cancels) and
            on Linux (dynamic shortcut instability). */}
        {!isLinux && !pushToTalk && (
          <ShortcutInput shortcutId="cancel" grouped={true} />
        )}
      </SettingsGroup>

      {/* AI post-processing — the on/off toggle, plus the prompt-cycling
          shortcuts which only matter once post-processing is enabled. */}
      <SettingsGroup title={t("settings.general.groups.postProcessing")}>
        <PostProcessingToggle descriptionMode="tooltip" grouped={true} />
        {postProcessEnabled && (
          <>
            <ShortcutInput shortcutId="prompt_prev" grouped={true} />
            <ShortcutInput shortcutId="prompt_next" grouped={true} />
          </>
        )}
      </SettingsGroup>

      {/* Contextual: language / translation for the active model (self-hides
          when the current model doesn't support them). */}
      <ModelSettingsCard />

      {/* Microphone & input conditioning. */}
      <SettingsGroup title={t("settings.general.groups.input")}>
        <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
        {/* [GRAIN] high-pass + boost-only AGC for quiet/laptop mics. */}
        <VoiceProcessing descriptionMode="tooltip" grouped={true} />
        <MuteWhileRecording descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      {/* Audio feedback output chain. */}
      <SettingsGroup title={t("settings.general.groups.feedback")}>
        <AudioFeedback descriptionMode="tooltip" grouped={true} />
        <OutputDeviceSelector
          descriptionMode="tooltip"
          grouped={true}
          disabled={!audioFeedbackEnabled}
        />
        <VolumeSlider disabled={!audioFeedbackEnabled} />
      </SettingsGroup>
    </div>
  );
};
