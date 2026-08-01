import { useTranslation } from "react-i18next";
import { AboutSettings } from "@/components/settings/about/AboutSettings";

/**
 * [GRAIN] About is its own tab in UI 2.0, not a settings section — it carries
 * app identity, the language preference and the on-disk locations, none of
 * which are per-feature settings. It re-skins the existing About surface; it
 * does not redesign it.
 */
export function AboutPage() {
  const { t } = useTranslation();

  return (
    <section
      className="page settings-workspace-page active"
      data-page-panel="about"
    >
      <div className="page-wrap settings-page-wrap">
        <div className="settings-shell settings-shell-single">
          <section
            className="settings-canvas"
            aria-labelledby="next-about-title"
          >
            <div className="settings-scroll">
              <div className="settings-content">
                <header className="settings-main-heading">
                  <h1 id="next-about-title">{t("ui2.nav.about")}</h1>
                  <div className="settings-current-copy">
                    <strong>{t("ui2.about.label")}</strong>
                    <span>{t("ui2.about.description")}</span>
                  </div>
                </header>
                <div className="settings-pane next-settings-content">
                  <AboutSettings variant="next" />
                </div>
              </div>
            </div>
          </section>
        </div>
      </div>
    </section>
  );
}
