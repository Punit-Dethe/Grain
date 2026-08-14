import { useTranslation } from "react-i18next";
import { AccelerationSelector } from "@/components/settings/AccelerationSelector";
import { AppearanceMode } from "@/components/settings/AppearanceMode";
import { AutostartToggle } from "@/components/settings/AutostartToggle";
import DefaultPanel from "@/components/settings/DefaultPanel";
import { ExperimentalToggle } from "@/components/settings/ExperimentalToggle";
import { HistoryLimit } from "@/components/settings/HistoryLimit";
import { LazyStreamClose } from "@/components/settings/LazyStreamClose";
import { PillSkinSelector } from "@/components/settings/PillSkinSelector";
import { RecordingRetentionPeriodSelector } from "@/components/settings/RecordingRetentionPeriod";
import { ShowOverlay } from "@/components/settings/ShowOverlay";
import { ShowTrayIcon } from "@/components/settings/ShowTrayIcon";
import { StartHidden } from "@/components/settings/StartHidden";
import { UpdateChecksToggle } from "@/components/settings/UpdateChecksToggle";
import { UpdatesSection } from "@/components/settings/UpdatesSection";
import { KeyboardImplementationSelector } from "@/components/settings/debug/KeyboardImplementationSelector";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { useSettings } from "@/hooks/useSettings";

/**
 * [GRAIN] Application — the app itself: how it looks, when it runs, what it
 * keeps.
 *
 * This was most of "Advanced", which was never advanced — choosing a theme or
 * turning off the tray icon is the opposite of advanced, and filing them under
 * that name meant the ordinary settings were the hardest ones to find.
 */
export function ApplicationPane() {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const experimentalEnabled = getSetting("experimental_enabled") || false;

  return (
    <div className="max-w-4xl w-full mx-auto space-y-7">
      <SettingsGroup
        title={t("ui2.settings.groups.appearance")}
        info={t("ui2.settings.groups.appearanceInfo")}
      >
        <AppearanceMode />
        <DefaultPanel grouped />
        <ShowOverlay descriptionMode="tooltip" grouped />
        <PillSkinSelector descriptionMode="tooltip" grouped />
      </SettingsGroup>

      <SettingsGroup
        title={t("ui2.settings.groups.system")}
        info={t("ui2.settings.groups.systemInfo")}
      >
        <AutostartToggle descriptionMode="tooltip" grouped />
        <StartHidden descriptionMode="tooltip" grouped />
        <ShowTrayIcon descriptionMode="tooltip" grouped />
      </SettingsGroup>

      {/* [GRAIN] Updates were only ever reachable from Debug, which is not where
          anyone looks for them. */}
      <SettingsGroup title="Updates">
        <UpdatesSection />
        <UpdateChecksToggle descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      <SettingsGroup
        title={t("ui2.settings.groups.storage")}
        info={t("ui2.settings.groups.storageInfo")}
      >
        <HistoryLimit descriptionMode="tooltip" grouped />
        <RecordingRetentionPeriodSelector descriptionMode="tooltip" grouped />
      </SettingsGroup>

      {/* The switch that reveals the rest of the group leads it, so turning
          experiments off leaves one row rather than a heading over nothing. */}
      <SettingsGroup
        title={t("ui2.settings.groups.experimental")}
        info={t("ui2.settings.groups.experimentalInfo")}
      >
        <ExperimentalToggle descriptionMode="tooltip" grouped />
        {experimentalEnabled && (
          <>
            <KeyboardImplementationSelector descriptionMode="tooltip" grouped />
            <AccelerationSelector descriptionMode="tooltip" grouped />
            <LazyStreamClose descriptionMode="tooltip" grouped />
          </>
        )}
      </SettingsGroup>
    </div>
  );
}
