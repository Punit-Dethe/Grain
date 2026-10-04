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
  flow: "transcribe",
  standard: "transcribe",
  streaming: "transcribe_native_asr",
} as const;

export default function ShortcutsOnboarding({
  onBack,
  onComplete,
  availableModes,
}: ShortcutsOnboardingProps) {
  const { t } = useTranslation();
  const shortcutIds = [
    ...new Set(availableModes.map((mode) => SHORTCUTS[mode])),
  ];
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
            {shortcutIds.map((id) => (
              <ShortcutInput
                key={id}
                shortcutId={id}
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
