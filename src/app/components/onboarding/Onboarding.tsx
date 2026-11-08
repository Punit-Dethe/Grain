import { useTranslation } from "react-i18next";
import { ArrowRight, ChevronLeft } from "lucide-react";
import { useModelStore } from "@/stores/modelStore";
import { ModelPicker } from "@/components/settings/speech-to-text/ModelPicker";
import { OnboardingLayout } from "./OnboardingLayout";

interface OnboardingProps {
  onBack: () => void;
  onModelSelected: () => void;
}

export default function Onboarding({
  onBack,
  onModelSelected,
}: OnboardingProps) {
  const { t } = useTranslation();
  const { models, currentModel } = useModelStore();
  const ready = models.some((m) => m.id === currentModel && m.is_downloaded);
  return (
    <OnboardingLayout
      step={1}
      footer={
        <>
          <button type="button" className="onboarding-back" onClick={onBack}>
            <ChevronLeft aria-hidden="true" />
            {t("onboarding.setup.models.back")}
          </button>
          <button
            type="button"
            className="onboarding-primary"
            disabled={!ready}
            onClick={onModelSelected}
          >
            {t("onboarding.setup.continue")}
            <ArrowRight aria-hidden="true" />
          </button>
        </>
      }
    >
      <ModelPicker />
    </OnboardingLayout>
  );
}
