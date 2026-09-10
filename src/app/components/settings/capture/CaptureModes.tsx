/**
 * [GRAIN] Capture modes — the three ways to start a capture, plus the AI key.
 *
 * Standard and Streaming are always live. Flow remains visible for
 * discoverability but is disabled unless the selected local model satisfies
 * its reviewed Parakeet TDT contract.
 *
 * The mode names and descriptions come from the Grain translation table
 * (`settings.general.shortcut.bindings.*`), the same source the Overview key
 * cards and the shortcut rows read, so a rename lands everywhere at once. The
 * backend binding is the fallback when a locale has no entry — never a
 * hardcoded list, which is how the old UI grew a dead row.
 */
import React, { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { ShortcutInput } from "../ShortcutInput";
import { PostProcessingToggle } from "../PostProcessingToggle";
import { useModelStore } from "@/stores/modelStore";
import { getFlowAvailability } from "@/lib/flowAvailability";

/** Mirrors `CAPTURE_MODE_IDS` in grain-core. Order = least to most machinery. */
const CAPTURE_MODE_IDS = [
  "transcribe",
  "transcribe_realtime",
  "transcribe_native_asr",
] as const;

type CaptureModeId = (typeof CAPTURE_MODE_IDS)[number];

interface CaptureMode {
  id: CaptureModeId;
  name: string;
  description: string;
}

export const CaptureModes: React.FC = () => {
  const { t } = useTranslation();
  const { settings, getSetting, updateSetting, isUpdating } = useSettings();
  const { models: allModels, currentModel } = useModelStore();

  const pushToTalk = getSetting("push_to_talk") ?? false;
  const postProcessEnabled = getSetting("post_process_enabled") ?? false;
  const alwaysAi = getSetting("capture_always_ai") ?? false;
  const endWithAi = getSetting("capture_end_with_ai") ?? true;
  const aiStartMode = getSetting("capture_ai_start_mode") ?? "transcribe";
  const flowAvailability = getFlowAvailability(
    allModels,
    currentModel,
    getSetting("translate_to_english") ?? false,
  );
  const flowUnavailableDescription = !flowAvailability.available
    ? t(`settings.speechToText.flowUnavailable.${flowAvailability.reason}`)
    : null;

  const modes = useMemo<CaptureMode[]>(
    () =>
      CAPTURE_MODE_IDS.map((id) => {
        const binding = settings?.bindings?.[id];
        return {
          id,
          name: t(
            `settings.general.shortcut.bindings.${id}.name`,
            binding?.name ?? id,
          ),
          description:
            id === "transcribe_realtime" && flowUnavailableDescription
              ? flowUnavailableDescription
              : t(
                  `settings.general.shortcut.bindings.${id}.description`,
                  binding?.description ?? "",
                ),
        };
      }),
    [flowUnavailableDescription, settings, t],
  );

  const availableModes = modes.filter(
    (mode) => mode.id !== "transcribe_realtime" || flowAvailability.available,
  );
  const effectiveAiStartMode = availableModes.some(
    (mode) => mode.id === aiStartMode,
  )
    ? aiStartMode
    : "transcribe";

  return (
    <>
      <SettingsGroup title={t("ui2.capture.group")}>
        <div className="capture-mode-picker">
          {modes.map((mode) => {
            const disabled =
              mode.id === "transcribe_realtime" && !flowAvailability.available;
            return (
              <div
                key={mode.id}
                className={`capture-mode-option${disabled ? " is-disabled" : ""}`}
                aria-disabled={disabled}
              >
                <span className="capture-mode-copy">
                  <strong>{mode.name}</strong>
                  <small>{mode.description}</small>
                </span>
                <span className="capture-mode-shortcut">
                  <ShortcutInput
                    shortcutId={mode.id}
                    bare
                    disabled={disabled}
                  />
                </span>
              </div>
            );
          })}
        </div>
      </SettingsGroup>

      {/* AI is deliberately its own group: it is a property of what happens
          *after* speech, not a fourth way to start speaking. Its master switch
          leads the group rather than owning a section elsewhere, so the rows it
          governs sit directly under the thing that turns them on — and turning
          it off leaves one row rather than an empty heading. */}
      <SettingsGroup title={t("ui2.capture.ai.group")}>
        <PostProcessingToggle descriptionMode="tooltip" grouped />

        {postProcessEnabled && (
          <ToggleSwitch
            label={t("ui2.capture.ai.always.title")}
            description={t("ui2.capture.ai.always.description")}
            descriptionMode="tooltip"
            grouped
            checked={alwaysAi}
            isUpdating={isUpdating("capture_always_ai")}
            onChange={(value) => updateSetting("capture_always_ai", value)}
          />
        )}

        {/* With every capture already going to AI there is nothing for the AI
            key to add mid-capture, so we do not offer a switch that changes
            nothing. */}
        {postProcessEnabled && !alwaysAi && (
          <ShortcutInput shortcutId="transcribe_send_to_ai" grouped />
        )}

        {/* Push-to-talk ends a capture by releasing the key, so there is no
            moment at which a second key could end it with AI. */}
        {postProcessEnabled && !alwaysAi && !pushToTalk && (
          <ToggleSwitch
            label={t("ui2.capture.ai.end.title")}
            description={t("ui2.capture.ai.end.description")}
            descriptionMode="tooltip"
            grouped
            checked={endWithAi}
            isUpdating={isUpdating("capture_end_with_ai")}
            onChange={(value) => updateSetting("capture_end_with_ai", value)}
          />
        )}

        {postProcessEnabled && !alwaysAi && (
          <SettingContainer
            title={t("ui2.capture.ai.startMode.title")}
            description={t("ui2.capture.ai.startMode.description")}
            descriptionMode="tooltip"
            grouped
          >
            <Dropdown
              options={availableModes.map((mode) => ({
                value: mode.id,
                label: mode.name,
              }))}
              selectedValue={effectiveAiStartMode}
              onSelect={(value) =>
                updateSetting("capture_ai_start_mode", value)
              }
            />
          </SettingContainer>
        )}

        {/* Prompt cycling only exists once there are prompts to cycle. */}
        {postProcessEnabled && (
          <>
            <ShortcutInput shortcutId="prompt_prev" grouped />
            <ShortcutInput shortcutId="prompt_next" grouped />
          </>
        )}
      </SettingsGroup>
    </>
  );
};
