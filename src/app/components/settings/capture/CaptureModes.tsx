/**
 * [GRAIN] Capture modes — the three ways to start a capture, plus the AI key.
 *
 * All three capture modes (Standard, Flow, Live) are always live: each is a
 * static row showing its name, description and its own shortcut on the right
 * edge. There is no "one mode vs all three" choice — you simply use whichever
 * shortcut you press. The AI key is a separate group of toggles below.
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

  const pushToTalk = getSetting("push_to_talk") ?? false;
  const postProcessEnabled = getSetting("post_process_enabled") ?? false;
  const alwaysAi = getSetting("capture_always_ai") ?? false;
  const endWithAi = getSetting("capture_end_with_ai") ?? true;
  const aiStartMode = getSetting("capture_ai_start_mode") ?? "transcribe";

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
          description: t(
            `settings.general.shortcut.bindings.${id}.description`,
            binding?.description ?? "",
          ),
        };
      }),
    [settings, t],
  );

  return (
    <>
      <SettingsGroup title={t("ui2.capture.group")}>
        {/* Every mode is always live: name + description on the left, its own
            shortcut on the right edge. No picker, no toggle — just the keys. */}
        <div className="capture-mode-picker">
          {modes.map((mode) => (
            <div key={mode.id} className="capture-mode-option">
              <span className="capture-mode-copy">
                <strong>{mode.name}</strong>
                <small>{mode.description}</small>
              </span>
              <span className="capture-mode-shortcut">
                <ShortcutInput shortcutId={mode.id} bare />
              </span>
            </div>
          ))}
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

        {/* Every mode is live, so the AI key's idle start mode is a free choice. */}
        {postProcessEnabled && !alwaysAi && (
          <SettingContainer
            title={t("ui2.capture.ai.startMode.title")}
            description={t("ui2.capture.ai.startMode.description")}
            descriptionMode="tooltip"
            grouped
          >
            <Dropdown
              options={modes.map((mode) => ({
                value: mode.id,
                label: mode.name,
              }))}
              selectedValue={aiStartMode}
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
