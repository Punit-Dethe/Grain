import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { ShortcutInput } from "../ShortcutInput";
import { ModelUnloadTimeoutSetting } from "../ModelUnloadTimeout";

export const CaptureModes: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const pushToTalk = getSetting("shortcut_activation") === "push_to_talk";
  const alwaysAi = getSetting("capture_always_ai") ?? false;
  const endWithAi = getSetting("capture_end_with_ai") ?? true;
  return (
    <>
      <SettingsGroup title={t("ui2.capture.group")}>
        <ShortcutInput shortcutId="transcribe" grouped />
        <ModelUnloadTimeoutSetting descriptionMode="tooltip" grouped />
      </SettingsGroup>

      <SettingsGroup title={t("ui2.capture.ai.group")}>
        <ToggleSwitch
          label={t("ui2.capture.ai.always.title")}
          description={t("ui2.capture.ai.always.description")}
          descriptionMode="tooltip"
          grouped
          checked={alwaysAi}
          isUpdating={isUpdating("capture_always_ai")}
          onChange={(value) => updateSetting("capture_always_ai", value)}
        />

        {/* With every capture already going to AI there is nothing for the AI
            key to add mid-capture, so we do not offer a switch that changes
            nothing. */}
        {!alwaysAi && (
          <ShortcutInput shortcutId="transcribe_send_to_ai" grouped />
        )}

        {/* Push-to-talk ends a capture by releasing the key, so there is no
            moment at which a second key could end it with AI. */}
        {!alwaysAi && !pushToTalk && (
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
        <ShortcutInput
          shortcutId="prompt_record"
          descriptionMode="tooltip"
          grouped
        />
      </SettingsGroup>
    </>
  );
};
