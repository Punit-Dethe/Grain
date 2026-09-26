import { useEffect, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Check, ShieldCheck } from "lucide-react";
import { useTheme } from "@/contexts/ThemeContext";
import { WindowChrome } from "../WindowChrome";
import grainMark from "../../branding/grain-mark.png";
import "./onboarding.css";

const STEPS = ["microphone", "modes", "models", "try", "shortcuts"] as const;

export function OnboardingLayout({
  step,
  children,
  footer,
  topAction,
}: {
  step: number;
  children: ReactNode;
  footer?: ReactNode;
  topAction?: ReactNode;
}) {
  const { t } = useTranslation();
  const { isDark } = useTheme();
  const stageRef = useRef<HTMLElement>(null);
  useEffect(() => {
    const stage = stageRef.current;
    const focusHeading = () => {
      const heading = stage?.querySelector<HTMLElement>("h1");
      if (!heading) return false;
      heading.tabIndex = -1;
      heading.focus({ preventScroll: true });
      return true;
    };
    if (focusHeading() || !stage) return;
    // Permission loading can precede the heading. Focus once when it appears.
    const observer = new MutationObserver(() => {
      if (focusHeading()) observer.disconnect();
    });
    observer.observe(stage, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [step]);

  return (
    <div
      className="onboarding-shell grain-root"
      data-theme={isDark ? "dark" : "light"}
    >
      <WindowChrome />
      <aside
        className="onboarding-journey"
        aria-label={t("onboarding.setup.progress")}
      >
        <div className="onboarding-brand">
          <img className="grain-mark" src={grainMark} alt="" />
          <strong>{t("onboarding.setup.brand")}</strong>
        </div>
        <ol className="onboarding-stepper">
          {STEPS.map((item, index) => (
            <li
              key={item}
              className={
                index === step ? "active" : index < step ? "done" : undefined
              }
              aria-current={index === step ? "step" : undefined}
            >
              <span className="onboarding-step-number" aria-hidden="true">
                {index < step ? <Check /> : index + 1}
              </span>
              <span>{t(`onboarding.setup.steps.${item}`)}</span>
            </li>
          ))}
        </ol>
        <p className="onboarding-local-note">
          <ShieldCheck aria-hidden="true" />
          {t("onboarding.setup.localNote")}
        </p>
      </aside>
      <div className="onboarding-workbench">
        <header className="onboarding-workbench-header">
          <span>
            {t("onboarding.setup.stepCount", {
              current: step + 1,
              total: STEPS.length,
            })}
          </span>
          {topAction}
        </header>
        <main className="onboarding-stage" ref={stageRef} tabIndex={0}>
          {children}
        </main>
        {footer && (
          <footer className="onboarding-footer">
            <div className="onboarding-footer-inner">{footer}</div>
          </footer>
        )}
      </div>
    </div>
  );
}
