import React from "react";
import { useTranslation } from "react-i18next";
import { ShowOverlay } from "../ShowOverlay";
import { CustomWords } from "../CustomWords";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { StartHidden } from "../StartHidden";
import DefaultPanel from "../DefaultPanel";
import { AutostartToggle } from "../AutostartToggle";
import { ShowTrayIcon } from "../ShowTrayIcon";
import { PasteMethodSetting } from "../PasteMethod";
import { TypingToolSetting } from "../TypingTool";
import { ClipboardHandlingSetting } from "../ClipboardHandling";
import { AutoSubmit } from "../AutoSubmit";
import { PostProcessingToggle } from "../PostProcessingToggle";
import { AppendTrailingSpace } from "../AppendTrailingSpace";
import { HistoryLimit } from "../HistoryLimit";
import { RecordingRetentionPeriodSelector } from "../RecordingRetentionPeriod";
import { ExperimentalToggle } from "../ExperimentalToggle";
import { useSettings } from "../../../hooks/useSettings";
import { KeyboardImplementationSelector } from "../debug/KeyboardImplementationSelector";
import { AccelerationSelector } from "../AccelerationSelector";
import { LazyStreamClose } from "../LazyStreamClose";
import { AppearanceToggle } from "../AppearanceToggle";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

export const AdvancedSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const experimentalEnabled = getSetting("experimental_enabled") || false;
  const autoDictionary = getSetting("auto_dictionary_enabled") ?? false;
  const scrapThat = getSetting("scrap_that_enabled") ?? false;

  return (
    <div className="max-w-4xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.advanced.groups.app")}>
        <DefaultPanel grouped={true} />
        <ShowOverlay descriptionMode="tooltip" grouped={true} />
        <StartHidden descriptionMode="tooltip" grouped={true} />
        <AutostartToggle descriptionMode="tooltip" grouped={true} />
        <ShowTrayIcon descriptionMode="tooltip" grouped={true} />
        <ExperimentalToggle descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.advanced.groups.output")}>
        <PasteMethodSetting descriptionMode="tooltip" grouped={true} />
        <TypingToolSetting descriptionMode="tooltip" grouped={true} />
        <ClipboardHandlingSetting descriptionMode="tooltip" grouped={true} />
        <AutoSubmit descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.advanced.groups.transcription")}>
        <CustomWords descriptionMode="tooltip" grouped />
        <ToggleSwitch
          label="Auto-add to dictionary"
          description="After pasting, briefly watch for you re-spelling a word (e.g. a name Grain got wrong). If you make the same correction across a couple of pastes, the pill offers to add that spelling — click it to accept. Only proper nouns and identifiers are learned; off = zero overhead."
          descriptionMode="tooltip"
          grouped
          checked={autoDictionary}
          isUpdating={isUpdating("auto_dictionary_enabled")}
          onChange={(v) => updateSetting("auto_dictionary_enabled", v)}
        />
        <ToggleSwitch
          label='"Scrap that" voice reset'
          description='Say "scrap that" mid-dictation to discard everything before it and start the transcript fresh from that point. Works in every mode; in live-streaming the expanded pill collapses back to the compact capsule until you speak again. Off = zero overhead.'
          descriptionMode="tooltip"
          grouped
          checked={scrapThat}
          isUpdating={isUpdating("scrap_that_enabled")}
          onChange={(v) => updateSetting("scrap_that_enabled", v)}
        />
        <AppendTrailingSpace descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.advanced.groups.history")}>
        <HistoryLimit descriptionMode="tooltip" grouped={true} />
        <RecordingRetentionPeriodSelector
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>

      {experimentalEnabled && (
        <SettingsGroup title={t("settings.advanced.groups.experimental")}>
          <PostProcessingToggle descriptionMode="tooltip" grouped={true} />
          <KeyboardImplementationSelector
            descriptionMode="tooltip"
            grouped={true}
          />
          <AccelerationSelector descriptionMode="tooltip" grouped={true} />
          <LazyStreamClose descriptionMode="tooltip" grouped={true} />
          <AppearanceToggle />
        </SettingsGroup>
      )}
    </div>
  );
};
