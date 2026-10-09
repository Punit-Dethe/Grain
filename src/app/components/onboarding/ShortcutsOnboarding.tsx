import { useCallback, useState } from "react";
import { useSettings } from "@/hooks/useSettings";
import { useTranslation } from "react-i18next";
import { Check, ChevronLeft } from "lucide-react";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { OnboardingLayout } from "./OnboardingLayout";

interface ShortcutsOnboardingProps {
  onBack: () => void;
  onComplete: () => void;
  shortcutIds: string[];
}
export default function ShortcutsOnboarding({
  onBack,
  onComplete,
  shortcutIds,
}: ShortcutsOnboardingProps) {
  const { t } = useTranslation();
  const { isUpdating, isLoading } = useSettings();
  const [recordingId, setRecordingId] = useState<string | null>(null);
  const handleRecordingChange = useCallback(
    (id: string, recording: boolean) => {
      setRecordingId((current) =>
        recording ? id : current === id ? null : current,
      );
    },
    [],
  );
  const busy =
    isLoading ||
    recordingId !== null ||
    shortcutIds.some((id) => isUpdating(`binding_${id}`));
  return (
    <OnboardingLayout
      step={2}
      footer={
        <>
          <button
            type="button"
            className="onboarding-back"
            onClick={onBack}
            disabled={busy}
          >
            <ChevronLeft aria-hidden="true" />
            {t("onboarding.setup.shortcuts.back")}
          </button>
          <button
            type="button"
            className="onboarding-primary"
            onClick={onComplete}
            disabled={busy}
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
                onRecordingChange={handleRecordingChange}
                disabled={recordingId !== null && recordingId !== id}
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
