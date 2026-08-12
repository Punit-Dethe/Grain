import { useEffect, type ComponentType } from "react";
import { useTranslation } from "react-i18next";
import { AboutSettings } from "@/components/settings/about/AboutSettings";
import { DebugSettings } from "@/components/settings/debug/DebugSettings";
import { PostProcessingSettings } from "@/components/settings/post-processing/PostProcessingSettings";
import { SpeechToTextSettings } from "@/components/settings/speech-to-text/SpeechToTextSettings";
import { ApplicationPane } from "../settings/panes/ApplicationPane";
import { AudioPane } from "../settings/panes/AudioPane";
import { CapturePane } from "../settings/panes/CapturePane";
import { OutputPane } from "../settings/panes/OutputPane";
import { useSettings } from "@/hooks/useSettings";
import { initPpPool } from "@/stores/ppPoolStore";
import { initSttPool } from "@/stores/sttPoolStore";
import { hashForRoute, type SettingsSectionId } from "../navigation";
import {
  isSettingsSectionEnabled,
  SETTINGS_SECTIONS,
} from "../settings/sections";
import { consumeSettingFocus, focusSettingByTitle } from "../quick-panel/focus";

const sectionComponents: Record<SettingsSectionId, ComponentType> = {
  capture: CapturePane,
  audio: AudioPane,
  output: OutputPane,
  application: ApplicationPane,
  "speech-to-text": SpeechToTextSettings,
  "post-processing": PostProcessingSettings,
  debug: DebugSettings,
  about: AboutSettings,
};

interface SettingsPageProps {
  section: SettingsSectionId;
}

export function SettingsPage({ section }: SettingsPageProps) {
  const { t } = useTranslation();
  const { settings, isLoading } = useSettings();
  const enabled = isSettingsSectionEnabled(section, settings);
  const activeSection = isLoading ? section : enabled ? section : "capture";
  const ActiveSection = sectionComponents[activeSection];

  useEffect(() => {
    if (!isLoading && !enabled) {
      window.history.replaceState(
        null,
        "",
        hashForRoute({ page: "settings", section: "capture" }),
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

  // A Quick Panel jump lands us on the section, then asks us to scroll to the
  // exact row. Panes settle asynchronously (provider pools, device lists), so
  // retry a few frames before giving up.
  useEffect(() => {
    if (isLoading || activeSection !== section) return;
    const request = consumeSettingFocus(activeSection);
    if (!request) return;

    let cancelled = false;
    const attempt = (remaining: number) => {
      if (cancelled) return;
      if (focusSettingByTitle(request.title) || remaining <= 0) return;
      window.setTimeout(() => attempt(remaining - 1), 90);
    };
    const frame = requestAnimationFrame(() => attempt(6));
    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
    };
  }, [activeSection, isLoading, section]);

  const availableSections = SETTINGS_SECTIONS.filter((item) =>
    item.enabled(settings),
  );

  return (
    <section
      className="page settings-workspace-page active"
      data-page-panel="settings"
      aria-busy={isLoading || undefined}
    >
      <div className="page-wrap settings-page-wrap">
        <div className="settings-shell">
          <aside className="settings-sidebar-pane">
            <div className="settings-pane-header">
              <div>
                <strong>{t("ui2.settings.title")}</strong>
              </div>
            </div>
            <nav
              className="settings-nav"
              aria-label={t("ui2.settings.navigation")}
            >
              {availableSections.map(({ id }) => (
                <button
                  key={id}
                  type="button"
                  className={id === activeSection ? "active" : ""}
                  aria-current={id === activeSection ? "page" : undefined}
                  onClick={() => {
                    window.location.hash = hashForRoute({
                      page: "settings",
                      section: id,
                    }).slice(1);
                  }}
                >
                  {t(`ui2.settings.sections.${id}.label`)}
                </button>
              ))}
            </nav>
          </aside>

          <section
            className="settings-canvas"
            aria-labelledby="next-settings-title"
          >
            <div className="settings-scroll">
              <div className="settings-content">
                <header className="settings-main-heading">
                  <h1 id="next-settings-title">{t("ui2.settings.title")}</h1>
                  <div className="settings-current-copy">
                    <strong>
                      {t(`ui2.settings.sections.${activeSection}.label`)}
                    </strong>
                    <span>
                      {t(`ui2.settings.sections.${activeSection}.description`)}
                    </span>
                  </div>
                </header>

                {isLoading ? (
                  <div className="next-settings-loading" role="status">
                    <i aria-hidden="true" />
                    {t("ui2.settings.loading")}
                  </div>
                ) : (
                  <div
                    key={activeSection}
                    className="settings-pane next-settings-content"
                  >
                    {activeSection === "about" ? (
                      <AboutSettings variant="next" />
                    ) : (
                      <ActiveSection />
                    )}
                  </div>
                )}
              </div>
            </div>
          </section>
        </div>
      </div>
    </section>
  );
}
