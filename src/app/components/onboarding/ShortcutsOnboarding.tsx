import { useTranslation } from "react-i18next";
import { ChevronLeft } from "lucide-react";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import "./onboarding.css";

interface ShortcutsOnboardingProps {
  onBack: () => void;
  onComplete: () => void;
}

// All three capture modes are always live; onboarding just shows each key.
const CAPTURE_MODE_IDS = [
  "transcribe_realtime",
  "transcribe",
  "transcribe_native_asr",
] as const;

const ShortcutsOnboarding: React.FC<ShortcutsOnboardingProps> = ({
  onBack,
  onComplete,
}) => {
  const { t } = useTranslation();

  return (
    <div className="onboarding-shell">
      <header className="onboarding-topbar">
        <div
          className="onboarding-brand"
          aria-label={t("onboarding.setup.brand")}
        >
          <strong>{t("onboarding.setup.brand")}</strong>
          <span>{t("onboarding.setup.label")}</span>
        </div>

        <ol
          className="onboarding-stepper"
          aria-label={t("onboarding.setup.progress")}
        >
          {(["microphone", "modes", "models", "try", "shortcuts"] as const).map(
            (step, index) => (
              <li
                key={step}
                className={index === 4 ? "active" : "done"}
                aria-current={index === 4 ? "step" : undefined}
              >
                <span className="onboarding-stepper-line" aria-hidden="true" />
                <span>{t(`onboarding.setup.steps.${step}`)}</span>
              </li>
            ),
          )}
        </ol>

        <span aria-hidden="true" />
      </header>

      <main className="onboarding-stage">
        <section className="onboarding-shortcuts-step">
          <div className="onboarding-heading">
            <h1>{t("onboarding.setup.shortcuts.title")}</h1>
            <p>{t("onboarding.setup.shortcuts.description")}</p>
          </div>

          <div className="onboarding-shortcut-editor">
            <div className="onboarding-shortcut-editor-head">
              <strong>{t("onboarding.setup.shortcuts.shortcutLabel")}</strong>
              <span>{t("onboarding.setup.shortcuts.editHint")}</span>
            </div>
            <div className="onboarding-shortcut-list">
              {CAPTURE_MODE_IDS.map((id) => (
                <ShortcutInput
                  key={id}
                  shortcutId={id}
                  descriptionMode="inline"
                  grouped
                />
              ))}
            </div>
          </div>

          <p className="onboarding-shortcut-note">
            {t("onboarding.setup.shortcuts.settingsNote")}
          </p>
        </section>
      </main>

      <footer className="onboarding-footer">
        <div className="onboarding-footer-inner">
          <button type="button" className="onboarding-back" onClick={onBack}>
            <ChevronLeft aria-hidden="true" />
            {t("onboarding.setup.shortcuts.back")}
          </button>
          <button
            type="button"
            className="onboarding-primary"
            onClick={onComplete}
          >
            {t("onboarding.setup.shortcuts.finish")}
          </button>
        </div>
      </footer>
    </div>
  );
};

export default ShortcutsOnboarding;
