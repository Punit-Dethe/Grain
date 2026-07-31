import { useEffect, type ComponentType } from "react";
import { useTranslation } from "react-i18next";
import { AdvancedSettings } from "@/components/settings/advanced/AdvancedSettings";
import { DebugSettings } from "@/components/settings/debug/DebugSettings";
import { GeneralSettings } from "@/components/settings/general/GeneralSettings";
import { PostProcessingSettings } from "@/components/settings/post-processing/PostProcessingSettings";
import { SpeechToTextSettings } from "@/components/settings/speech-to-text/SpeechToTextSettings";
import { useSettings } from "@/hooks/useSettings";
import { initPpPool } from "@/stores/ppPoolStore";
import { initSttPool } from "@/stores/sttPoolStore";
import { hashForRoute, type SettingsSectionId } from "../navigation";
import {
  isSettingsSectionEnabled,
  SETTINGS_SECTIONS,
} from "../settings/sections";

const sectionComponents: Record<SettingsSectionId, ComponentType> = {
  general: GeneralSettings,
  advanced: AdvancedSettings,
  "speech-to-text": SpeechToTextSettings,
  "post-processing": PostProcessingSettings,
  debug: DebugSettings,
};

interface SettingsPageProps {
  section: SettingsSectionId;
}

export function SettingsPage({ section }: SettingsPageProps) {
  const { t } = useTranslation();
  const { settings, isLoading } = useSettings();
  const enabled = isSettingsSectionEnabled(section, settings);
  const activeSection = isLoading ? section : enabled ? section : "general";
  const ActiveSection = sectionComponents[activeSection];

  useEffect(() => {
    if (!isLoading && !enabled) {
      window.history.replaceState(
        null,
        "",
        hashForRoute({ page: "settings", section: "general" }),
      );
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    }
  }, [enabled, isLoading, section]);

  useEffect(() => {
    if (isLoading || activeSection !== section) return;

    const initialize =
      activeSection === "speech-to-text"
        ? initSttPool
        : activeSection === "post-processing"
          ? initPpPool
          : null;

    if (initialize) {
      void initialize().catch((error) => {
        console.error(
          `Failed to initialize ${activeSection} provider pool:`,
          error,
        );
      });
    }
  }, [activeSection, isLoading, section]);

  const availableSections = SETTINGS_SECTIONS.filter((item) =>
    item.enabled(settings),
  );

  return (
    <div className="next-settings" aria-busy={isLoading || undefined}>
      <aside className="next-settings-index">
        <header>
          <p>{t("ui2.settings.indexEyebrow")}</p>
          <h1>{t("ui2.settings.title")}</h1>
        </header>
        <nav aria-label={t("ui2.settings.navigation")}>
          {availableSections.map(({ id, icon: Icon }, index) => (
            <a
              key={id}
              href={hashForRoute({ page: "settings", section: id })}
              className="next-settings-link"
              aria-current={id === activeSection ? "page" : undefined}
            >
              <span>{String(index + 1).padStart(2, "0")}</span>
              <Icon aria-hidden="true" size={15} strokeWidth={1.7} />
              <strong>{t(`ui2.settings.sections.${id}.label`)}</strong>
              <i aria-hidden="true" />
            </a>
          ))}
        </nav>
      </aside>

      <section
        className="next-settings-canvas"
        aria-labelledby="next-settings-section-title"
      >
        <header className="next-settings-head">
          <div>
            <p>{t("ui2.settings.eyebrow")}</p>
            <h2 id="next-settings-section-title">
              {t(`ui2.settings.sections.${activeSection}.label`)}
            </h2>
            <span>
              {t(`ui2.settings.sections.${activeSection}.description`)}
            </span>
          </div>
          <small>{t("ui2.settings.localState")}</small>
        </header>

        {isLoading ? (
          <div className="next-settings-loading" role="status">
            <i aria-hidden="true" />
            {t("ui2.settings.loading")}
          </div>
        ) : (
          <div key={activeSection} className="next-settings-content">
            <ActiveSection />
          </div>
        )}
      </section>
    </div>
  );
}
