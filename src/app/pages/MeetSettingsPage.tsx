import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../components/ui/SettingsGroup";
import { ToggleSwitch } from "../components/ui/ToggleSwitch";
import { useSettings } from "../hooks/useSettings";
import { useMeetAiAvailable } from "../meet/useMeetController";
import { hashForRoute } from "../navigation";

function Glyph({ name }: { name: string }) {
  return (
    <svg className="icon sm" aria-hidden="true">
      <use href={`#i-${name}`} />
    </svg>
  );
}

/** Recall's own settings page. It is a page, not a menu, because the list
 * grows with the feature: everything about what Recall does on its own lives
 * here. */
export function MeetSettingsPage() {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const ai = useMeetAiAvailable();
  const autoSummary = getSetting("meet_auto_summary") ?? false;
  const autoTitle = getSetting("meet_auto_title") ?? false;
  const autoTranscribe = getSetting("meet_auto_transcribe") ?? false;

  return (
    <section className="page active" data-page-panel="meet">
      <div className="page-wrap meet-wrap meet-settings">
        <header className="meet-view-head">
          <div className="meet-view-nav">
            <a className="meet-back" href={hashForRoute({ page: "meet" })}>
              <Glyph name="chev-left" />
              {t("ui2.meet.name")}
            </a>
          </div>
          <h1>{t("ui2.meet.settings.title")}</h1>
          <p className="page-subtitle">{t("ui2.meet.settings.subtitle")}</p>
        </header>

        {ai === false && (
          <div className="meet-callout">
            <p>{t("ui2.meet.settings.aiMissing")}</p>
            <a
              className="text-button meet-callout-action"
              href={hashForRoute({ page: "settings", section: "ai" })}
            >
              {t("ui2.meet.ai.openSettings")}
            </a>
          </div>
        )}

        <div className="meet-settings-groups">
          <SettingsGroup title={t("ui2.meet.settings.afterRecording")}>
            <ToggleSwitch
              label={t("ui2.meet.settings.autoTranscribe")}
              description={t("ui2.meet.settings.autoTranscribeDescription")}
              grouped
              checked={autoTranscribe}
              isUpdating={isUpdating("meet_auto_transcribe")}
              onChange={(value) => updateSetting("meet_auto_transcribe", value)}
            />
          </SettingsGroup>
          <SettingsGroup title={t("ui2.meet.settings.automatic")}>
            <ToggleSwitch
              label={t("ui2.meet.settings.autoSummary")}
              description={t("ui2.meet.settings.autoSummaryDescription")}
              grouped
              checked={autoSummary}
              isUpdating={isUpdating("meet_auto_summary")}
              onChange={(value) => updateSetting("meet_auto_summary", value)}
            />
            <ToggleSwitch
              label={t("ui2.meet.settings.autoTitle")}
              description={t("ui2.meet.settings.autoTitleDescription")}
              grouped
              checked={autoTitle}
              isUpdating={isUpdating("meet_auto_title")}
              onChange={(value) => updateSetting("meet_auto_title", value)}
            />
          </SettingsGroup>
        </div>
      </div>
    </section>
  );
}
