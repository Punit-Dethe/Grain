import { useTranslation } from "react-i18next";
import { type } from "@tauri-apps/plugin-os";
import { CaptureModes } from "@/components/settings/capture/CaptureModes";
import { ModelSettingsCard } from "@/components/settings/general/ModelSettingsCard";
import { PushToTalk } from "@/components/settings/PushToTalk";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
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
  const { getSetting } = useSettings();
  const pushToTalk = getSetting("push_to_talk");
  const isLinux = type() === "linux";

  return (
    <div className="max-w-4xl w-full mx-auto space-y-7">
      <CaptureModes />

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
