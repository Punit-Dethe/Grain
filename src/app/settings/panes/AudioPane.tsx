import { useTranslation } from "react-i18next";
import { AudioFeedback } from "@/components/settings/AudioFeedback";
import { MicrophoneSelector } from "@/components/settings/MicrophoneSelector";
import { MuteWhileRecording } from "@/components/settings/MuteWhileRecording";
import { OutputDeviceSelector } from "@/components/settings/OutputDeviceSelector";
import { VoiceProcessing } from "@/components/settings/VoiceProcessing";
import { VolumeSlider } from "@/components/settings/VolumeSlider";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { useSettings } from "@/hooks/useSettings";

/**
 * [GRAIN] Audio — what Grain listens to, and what you hear back.
 *
 * Both halves used to sit at the bottom of General, below the capture keys and
 * the AI switches, which is a long way to scroll for "which microphone is it
 * using?" — the most common question in the whole settings pane.
 */
export function AudioPane() {
  const { t } = useTranslation();
  const { audioFeedbackEnabled } = useSettings();

  return (
    <div className="max-w-4xl w-full mx-auto space-y-7">
      <SettingsGroup
        title={t("ui2.settings.groups.input")}
        info={t("ui2.settings.groups.inputInfo")}
      >
        <MicrophoneSelector descriptionMode="tooltip" grouped />
        <VoiceProcessing descriptionMode="tooltip" grouped />
        <MuteWhileRecording descriptionMode="tooltip" grouped />
      </SettingsGroup>

      <SettingsGroup
        title={t("ui2.settings.groups.feedback")}
        info={t("ui2.settings.groups.feedbackInfo")}
      >
        <AudioFeedback descriptionMode="tooltip" grouped />
        <OutputDeviceSelector
          descriptionMode="tooltip"
          grouped
          disabled={!audioFeedbackEnabled}
        />
        <VolumeSlider disabled={!audioFeedbackEnabled} />
      </SettingsGroup>
    </div>
  );
}
