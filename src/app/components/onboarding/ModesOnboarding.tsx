import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, ChevronLeft, FileText, Radio, Waves } from "lucide-react";
import { OnboardingLayout } from "./OnboardingLayout";

interface ModesOnboardingProps {
  onBack: () => void;
  onComplete: () => void;
}
const MODES = ["standard", "flow", "streaming"] as const;
const MODE_ICONS = { standard: FileText, flow: Waves, streaming: Radio };

export default function ModesOnboarding({
  onBack,
  onComplete,
}: ModesOnboardingProps) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<(typeof MODES)[number]>("flow");
  const ModeIcon = MODE_ICONS[mode];
  return (
    <OnboardingLayout
      step={1}
      topAction={
        <button type="button" className="onboarding-skip" onClick={onComplete}>
          {t("onboarding.setup.modes.skip")}
        </button>
      }
      footer={
        <>
          <button type="button" className="onboarding-back" onClick={onBack}>
            <ChevronLeft aria-hidden="true" />
            {t("onboarding.setup.modes.back")}
          </button>
          <button
            type="button"
            className="onboarding-primary"
            onClick={onComplete}
          >
            {t("onboarding.setup.modes.chooseModels")}
            <ArrowRight aria-hidden="true" />
          </button>
        </>
      }
    >
      <section className="onboarding-modes-step">
        <div className="onboarding-heading">
          <h1>{t("onboarding.setup.modes.title")}</h1>
          <p>{t("onboarding.setup.modes.description")}</p>
        </div>
        <div
          className="onboarding-mode-tabs"
          role="group"
          aria-label={t("onboarding.setup.try.modeLabel")}
        >
          {MODES.map((item) => {
            const ItemIcon = MODE_ICONS[item];
            return (
              <button
                key={item}
                type="button"
                aria-pressed={item === mode}
                className={item === mode ? "active" : undefined}
                onClick={() => setMode(item)}
              >
                <ItemIcon aria-hidden="true" />
                {t(`onboarding.setup.modes.${item}.title`)}
              </button>
            );
          })}
        </div>
        <div className="onboarding-mode-explainer" key={mode}>
          <div className="onboarding-mode-explainer-heading">
            <ModeIcon aria-hidden="true" />
            <div>
              <h2>{t(`onboarding.setup.modes.${mode}.title`)}</h2>
              <p>{t(`onboarding.setup.modes.${mode}.explanation`)}</p>
            </div>
          </div>
          <ol className="onboarding-mode-sequence">
            {[1, 2, 3].map((item) => (
              <li key={item}>
                <span aria-hidden="true">{item}</span>
                <div>
                  <strong>
                    {t(`onboarding.setup.modes.${mode}.sequence${item}`)}
                  </strong>
                  <p>
                    {t(`onboarding.setup.modes.${mode}.sequence${item}Detail`)}
                  </p>
                </div>
              </li>
            ))}
          </ol>
          <div className="onboarding-mode-example">
            <span>{t("onboarding.setup.modes.exampleLabel")}</span>
            <p>{t("onboarding.setup.modes.exampleText")}</p>
            <small>{t(`onboarding.setup.modes.${mode}.exampleHint`)}</small>
          </div>
        </div>
        <p className="onboarding-support-note">
          {t("onboarding.setup.modes.modelNote")}
        </p>
      </section>
    </OnboardingLayout>
  );
}
