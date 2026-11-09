import { useTranslation } from "react-i18next";
import { AgentSection } from "@/components/settings/experimentations/AgentSection";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";

export function AiPane() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const pushToTalk = getSetting("shortcut_activation") === "push_to_talk";
  const alwaysAi = getSetting("capture_always_ai") ?? false;
  const endWithAi = getSetting("capture_end_with_ai") ?? true;

  return (
    <div className="max-w-4xl w-full mx-auto space-y-7">
      <SettingsGroup title={t("ui2.settings.ai.dictationCleanup")}>
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
      <AgentSection />
    </div>
  );
}
