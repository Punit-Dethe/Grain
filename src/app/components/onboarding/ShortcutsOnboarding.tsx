import { useTranslation } from "react-i18next";
import { Check, ChevronLeft } from "lucide-react";
import type { OnboardingTestMode } from "@/bindings";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { OnboardingLayout } from "./OnboardingLayout";

interface ShortcutsOnboardingProps {
  onBack: () => void;
  onComplete: () => void;
  availableModes: OnboardingTestMode[];
}
const SHORTCUTS = {
  flow: "transcribe_realtime",
  standard: "transcribe",
  streaming: "transcribe_native_asr",
} as const;

export default function ShortcutsOnboarding({
  onBack,
  onComplete,
  availableModes,
}: ShortcutsOnboardingProps) {
  const { t } = useTranslation();
  return (
    <OnboardingLayout
      step={4}
      footer={
        <>
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
            <Check aria-hidden="true" />
          </button>
        </>
      }
    >
      <section className="onboarding-shortcuts-step">
        <div className="onboarding-heading">
          <h1>{t("onboarding.setup.shortcuts.title")}</h1>
          <p>{t("onboarding.setup.shortcuts.description")}</p>
        </div>
        <div className="onboarding-shortcut-editor">
          <div className="onboarding-shortcut-list">
            {availableModes.map((mode) => (
              <ShortcutInput
                key={mode}
                shortcutId={SHORTCUTS[mode]}
                descriptionMode="inline"
                grouped
              />
            ))}
          </div>
        </div>
        <p className="onboarding-support-note">
          {t("onboarding.setup.shortcuts.settingsNote")}
        </p>
      </section>
    </OnboardingLayout>
  );
}
