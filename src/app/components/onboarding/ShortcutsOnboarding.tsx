import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronLeft, Mic, Radio, Zap } from "lucide-react";
import { commands } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import "./onboarding.css";

interface ShortcutsOnboardingProps {
  onBack: () => void;
  onComplete: () => void;
}

const CAPTURE_MODES = [
  { id: "transcribe_realtime", key: "flow", icon: Zap },
  { id: "transcribe", key: "standard", icon: Mic },
  { id: "transcribe_native_asr", key: "streaming", icon: Radio },
] as const;

type CaptureModeId = (typeof CAPTURE_MODES)[number]["id"];

const ShortcutsOnboarding: React.FC<ShortcutsOnboardingProps> = ({
  onBack,
  onComplete,
}) => {
  const { t } = useTranslation();
  const { getSetting, refreshSettings, isLoading } = useSettings();
  const [error, setError] = useState("");
  const [changingMode, setChangingMode] = useState(false);
  const modeSet = getSetting("capture_mode_set") ?? "single";
  const storedPrimary = getSetting("capture_primary_mode") ?? "transcribe";
  const primaryMode = CAPTURE_MODES.some((mode) => mode.id === storedPrimary)
    ? (storedPrimary as CaptureModeId)
    : "transcribe";
  const chooseMode = async (mode: CaptureModeId) => {
    if (mode === primaryMode || changingMode) return;
    setError("");
    setChangingMode(true);
    try {
      const result = await commands.changeCapturePrimaryModeSetting(mode);
      if (result.status === "error") throw new Error(result.error);
      await refreshSettings();
    } catch (changeError) {
      console.error("Failed to choose onboarding capture mode:", changeError);
      setError(t("onboarding.setup.shortcuts.errors.mode"));
    } finally {
      setChangingMode(false);
    }
  };

  const shortcutRows =
    modeSet === "all"
      ? CAPTURE_MODES
      : CAPTURE_MODES.filter((mode) => mode.id === primaryMode);

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

          {modeSet === "single" ? (
            <div className="onboarding-shortcut-mode-section">
              <span className="onboarding-field-label">
                {t("onboarding.setup.shortcuts.modeLabel")}
              </span>
              <div
                className="onboarding-shortcut-modes"
                role="radiogroup"
                aria-label={t("onboarding.setup.shortcuts.modeLabel")}
              >
                {CAPTURE_MODES.map((mode) => {
                  const Icon = mode.icon;
                  const selected = mode.id === primaryMode;
                  return (
                    <button
                      key={mode.id}
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      className={selected ? "selected" : undefined}
                      disabled={isLoading || changingMode}
                      onClick={() => void chooseMode(mode.id)}
                    >
                      <span className="onboarding-shortcut-mode-icon">
                        <Icon aria-hidden="true" />
                      </span>
                      <span>
                        <strong>
                          {t(`onboarding.setup.modes.${mode.key}.title`)}
                        </strong>
                        <small>
                          {t(`onboarding.setup.shortcuts.modes.${mode.key}`)}
                        </small>
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          ) : (
            <p className="onboarding-shortcut-all-note">
              {t("onboarding.setup.shortcuts.allModes")}
            </p>
          )}

          <div className="onboarding-shortcut-editor">
            <div className="onboarding-shortcut-editor-head">
              <strong>{t("onboarding.setup.shortcuts.shortcutLabel")}</strong>
              <span>{t("onboarding.setup.shortcuts.editHint")}</span>
            </div>
            <div className="onboarding-shortcut-list">
              {shortcutRows.map((mode) => (
                <ShortcutInput
                  key={mode.id}
                  shortcutId={mode.id}
                  descriptionMode="inline"
                  grouped
                />
              ))}
            </div>
          </div>

          {error ? (
            <p className="onboarding-shortcut-error" role="alert">
              {error}
            </p>
          ) : null}
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
            disabled={changingMode}
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
