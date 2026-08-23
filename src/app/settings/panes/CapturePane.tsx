import { useTranslation } from "react-i18next";
import { type } from "@tauri-apps/plugin-os";
import { CaptureModes } from "@/components/settings/capture/CaptureModes";
import { ModelSettingsCard } from "@/components/settings/general/ModelSettingsCard";
import { PushToTalk } from "@/components/settings/PushToTalk";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";

/**
 * [GRAIN] Capture — everything about starting, holding and ending a recording.
 *
 * This is the first of four panes that replaced two ("General" and "Advanced").
 * Those two had become storage rather than subjects: General held capture keys,
 * microphones, speakers and AI; Advanced held appearance, paste behaviour,
 * history retention and startup. Nothing in either name told you which one to
 * open, so every question meant reading both.
 */
export function CapturePane() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const pushToTalk = getSetting("push_to_talk");
  const autoSendEnabled = getSetting("auto_send_enabled") ?? false;
  const experimentalEnabled = getSetting("experimental_enabled") ?? false;
  const isLinux = type() === "linux";

  return (
    <div className="max-w-4xl w-full mx-auto space-y-7">
      <CaptureModes />

      <SettingsGroup
        title={t("ui2.capture.extensionMode.group")}
        info={t("ui2.capture.extensionMode.info")}
      >
        <ShortcutInput shortcutId="extension_mode" grouped />
        <ToggleSwitch
          checked={autoSendEnabled}
          disabled={!experimentalEnabled && !autoSendEnabled}
          isUpdating={isUpdating("auto_send_enabled")}
          onChange={(enabled) => updateSetting("auto_send_enabled", enabled)}
          label={t("ui2.capture.extensionMode.autoSend.title")}
          description={t(
            experimentalEnabled
              ? "ui2.capture.extensionMode.autoSend.description"
              : "ui2.capture.extensionMode.autoSend.paused",
          )}
          descriptionMode="inline"
          grouped
        />
      </SettingsGroup>

      <SettingsGroup
        title={t("ui2.settings.groups.recording")}
        info={t("ui2.settings.groups.recordingInfo")}
      >
        <PushToTalk descriptionMode="tooltip" grouped />
        {/* Push-to-talk cancels by releasing the key, so a cancel shortcut would
            be a key that can never fire. Linux keeps it hidden regardless:
            dynamic re-registration there is unstable. */}
        {!isLinux && !pushToTalk && (
          <ShortcutInput shortcutId="cancel" grouped />
        )}
      </SettingsGroup>

      {/* Language and translation for the loaded model. Self-hides when the
          current model supports neither, so it is not a permanently empty box. */}
      <ModelSettingsCard />
    </div>
  );
}
