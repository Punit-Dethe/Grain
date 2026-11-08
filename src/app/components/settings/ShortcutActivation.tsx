import { useTranslation } from "react-i18next";
import type { ShortcutActivation as Activation } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";

export function ShortcutActivation() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const selected = getSetting("shortcut_activation") ?? "toggle";
  const title = t("shortcutActivation.title", "Recording behaviour");
  const descriptions: Record<Activation, string> = {
    toggle: t(
      "shortcutActivation.toggleDescription",
      "Press once to record, then press again to stop.",
    ),
    push_to_talk: t(
      "shortcutActivation.holdDescription",
      "Hold the shortcut to record. Release it to stop.",
    ),
    hold_or_toggle: t(
      "shortcutActivation.autoDescription",
      "Tap to keep recording until the next press. Hold for at least 300 ms to stop on release.",
    ),
  };
  return (
    <SettingContainer
      title={title}
      description={descriptions[selected]}
      descriptionMode="inline"
      grouped
      layout="horizontal"
    >
      <Dropdown
        ariaLabel={title}
        selectedValue={selected}
        disabled={isUpdating("shortcut_activation")}
        options={[
          {
            value: "toggle",
            label: t("shortcutActivation.toggle", "Press to toggle"),
          },
          {
            value: "push_to_talk",
            label: t("shortcutActivation.hold", "Hold to record"),
          },
          {
            value: "hold_or_toggle",
            label: t("shortcutActivation.auto", "Hold or toggle"),
          },
        ]}
        onSelect={(value) =>
          void updateSetting("shortcut_activation", value as Activation)
        }
      />
    </SettingContainer>
  );
}
