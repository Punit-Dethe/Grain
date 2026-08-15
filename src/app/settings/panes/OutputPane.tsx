import { useTranslation } from "react-i18next";
import { AppendTrailingSpace } from "@/components/settings/AppendTrailingSpace";
import { AutoSubmit } from "@/components/settings/AutoSubmit";
import { ClipboardHandlingSetting } from "@/components/settings/ClipboardHandling";
import { PasteMethodSetting } from "@/components/settings/PasteMethod";
import { TypingToolSetting } from "@/components/settings/TypingTool";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";

/**
 * [GRAIN] Output — what happens to the transcript once Grain has it.
 *
 * One group, not three. Paste method, typing tool and clipboard handling are
 * one decision seen from three angles: how the text gets into the app in front
 * of you. Splitting them into a group each made the page look busier than the
 * choice actually is.
 */
export function OutputPane() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const scrapThat = getSetting("scrap_that_enabled") ?? false;
  const pasteCatch = getSetting("paste_catch_enabled") ?? true;

  return (
    <div className="max-w-4xl w-full mx-auto space-y-7">
      <SettingsGroup
        title={t("ui2.settings.groups.insertion")}
        info={t("ui2.settings.groups.insertionInfo")}
      >
        <PasteMethodSetting descriptionMode="tooltip" grouped />
        <TypingToolSetting descriptionMode="tooltip" grouped />
        <ClipboardHandlingSetting descriptionMode="tooltip" grouped />
        <ToggleSwitch
          label={t("ui2.settings.pasteCatch.title")}
          description={t("ui2.settings.pasteCatch.description")}
          descriptionMode="tooltip"
          grouped
          checked={pasteCatch}
          isUpdating={isUpdating("paste_catch_enabled")}
          onChange={(value) => updateSetting("paste_catch_enabled", value)}
        />
        <AutoSubmit descriptionMode="tooltip" grouped />
        <AppendTrailingSpace descriptionMode="tooltip" grouped />
      </SettingsGroup>

      <SettingsGroup
        title={t("ui2.settings.groups.corrections")}
        info={t("ui2.settings.groups.correctionsInfo")}
      >
        <ToggleSwitch
          label={t("ui2.settings.scrapThat.title")}
          description={t("ui2.settings.scrapThat.description")}
          descriptionMode="tooltip"
          grouped
          checked={scrapThat}
          isUpdating={isUpdating("scrap_that_enabled")}
          onChange={(value) => updateSetting("scrap_that_enabled", value)}
        />
      </SettingsGroup>
    </div>
  );
}
