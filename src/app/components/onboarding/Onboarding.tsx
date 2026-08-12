import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  ChevronDown,
  ChevronLeft,
  HardDriveDownload,
  Loader2,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { commands, type ModelInfo } from "@/bindings";
import { useModelStore } from "@/stores/modelStore";
import "./onboarding.css";

interface OnboardingProps {
  onBack: () => void;
  onModelSelected: () => void;
}

type ModelFamily = "standard" | "streaming";

const formatSize = (sizeMb: number) => {
  if (sizeMb >= 1024) return `${(sizeMb / 1024).toFixed(1)} GB`;
  return `${Math.round(sizeMb)} MB`;
};

const Onboarding: React.FC<OnboardingProps> = ({ onBack, onModelSelected }) => {
  const { t } = useTranslation();
  const {
    models,
    loading,
    downloadModel,
    selectModel,
    cancelDownload,
    downloadingModels,
    verifyingModels,
    extractingModels,
    downloadProgress,
  } = useModelStore();

  const [enabledFamilies, setEnabledFamilies] = useState({
    standard: true,
    streaming: true,
  });
  const [selectedModels, setSelectedModels] = useState<
    Record<ModelFamily, string>
  >({ standard: "", streaming: "" });
  const [openPicker, setOpenPicker] = useState<ModelFamily | null>(null);
  const [defaultsLoading, setDefaultsLoading] = useState(true);
  const [defaultsError, setDefaultsError] = useState<string | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);
  const [activeDownloadId, setActiveDownloadId] = useState<string | null>(null);
  const [completedDownloadIds, setCompletedDownloadIds] = useState<string[]>(
    [],
  );
  const cancellationRequestedRef = useRef(false);

  const standardModels = useMemo(
    () => models.filter((model) => !model.supports_streaming),
    [models],
  );
  const streamingModels = useMemo(
    () => models.filter((model) => model.supports_streaming),
    [models],
  );

  const resolveDefaults = useCallback(async () => {
    setDefaultsLoading(true);
    setDefaultsError(null);
    try {
      const result = await commands.getOnboardingModelDefaults();
      if (result.status === "error") throw new Error(result.error);
      setSelectedModels({
        standard: result.data.standard_model_id,
        streaming: result.data.asr_model_id,
      });
    } catch (error) {
      console.error("Failed to resolve onboarding model defaults:", error);
      setDefaultsError(t("onboarding.setup.models.errors.defaults"));
    } finally {
      setDefaultsLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void resolveDefaults();
  }, [resolveDefaults]);

  const selectedStandard = models.find(
    (model) => model.id === selectedModels.standard,
  );
  const selectedStreaming = models.find(
    (model) => model.id === selectedModels.streaming,
  );

  const selectedFamilyModels = [
    enabledFamilies.standard ? selectedStandard : undefined,
    enabledFamilies.streaming ? selectedStreaming : undefined,
  ].filter((model): model is ModelInfo => Boolean(model));

  const selectedCount = selectedFamilyModels.length;
  const totalSizeMb = selectedFamilyModels.reduce(
    (total, model) => total + (model.is_downloaded ? 0 : model.size_mb),
    0,
  );
  const canInstall =
    !loading &&
    !defaultsLoading &&
    !defaultsError &&
    selectedCount > 0 &&
    !isInstalling;

  const toggleFamily = (family: ModelFamily) => {
    if (isInstalling) return;
    setEnabledFamilies((current) => {
      const next = { ...current, [family]: !current[family] };
      if (!next.standard && !next.streaming) return current;
      return next;
    });
  };

  const chooseModel = (family: ModelFamily, modelId: string) => {
    setSelectedModels((current) => ({ ...current, [family]: modelId }));
    setOpenPicker(null);
  };

  const finishSelections = async () => {
    if (enabledFamilies.standard) {
      const selected = await selectModel(selectedModels.standard);
      if (!selected) throw new Error(t("onboarding.errors.selectModel"));
    }
    if (enabledFamilies.streaming) {
      const result = await commands.selectAsrModel(selectedModels.streaming);
      if (result.status === "error") throw new Error(result.error);
    }
  };

  const installSelected = async () => {
    if (!canInstall) return;
    setIsInstalling(true);
    setOpenPicker(null);
    setCompletedDownloadIds([]);
    cancellationRequestedRef.current = false;

    try {
      for (const model of selectedFamilyModels) {
        if (cancellationRequestedRef.current) return;
        if (model.is_downloaded) continue;
        setActiveDownloadId(model.id);
        const succeeded = await downloadModel(model.id);
        if (!succeeded) {
          if (cancellationRequestedRef.current) return;
          throw new Error(t("onboarding.setup.models.errors.download"));
        }
        setCompletedDownloadIds((current) => [...current, model.id]);
      }
      if (cancellationRequestedRef.current) return;
      setActiveDownloadId(null);
      await finishSelections();
      onModelSelected();
    } catch (error) {
      console.error("Failed to install onboarding models:", error);
      toast.error(
        error instanceof Error
          ? error.message
          : t("onboarding.setup.models.errors.download"),
      );
    } finally {
      setActiveDownloadId(null);
      setIsInstalling(false);
    }
  };

  const stopInstallation = async () => {
    cancellationRequestedRef.current = true;
    if (activeDownloadId) await cancelDownload(activeDownloadId);
  };

  const modelStatus = (model: ModelInfo) => {
    if (model.id in extractingModels)
      return t("onboarding.setup.models.extracting");
    if (model.id in verifyingModels)
      return t("onboarding.setup.models.verifying");
    if (model.id === activeDownloadId || model.id in downloadingModels)
      return t("onboarding.setup.models.downloading");
    if (model.is_downloaded || completedDownloadIds.includes(model.id))
      return t("onboarding.setup.models.installed");
    if (isInstalling) return t("onboarding.setup.models.waiting");
    return t("onboarding.setup.models.ready");
  };

  const renderFamily = (
    family: ModelFamily,
    selectedModel: ModelInfo | undefined,
    familyModels: ModelInfo[],
  ) => {
    const enabled = enabledFamilies[family];
    return (
      <div
        className={`onboarding-model-family${enabled ? " selected" : ""}`}
        data-family={family}
      >
        <div className="onboarding-model-family-main">
          <button
            type="button"
            className="onboarding-model-check"
            aria-label={t("onboarding.setup.models.toggle", {
              family: t(`onboarding.setup.models.${family}.title`),
            })}
            aria-pressed={enabled}
            disabled={isInstalling}
            onClick={() => toggleFamily(family)}
          >
            <Check aria-hidden="true" />
          </button>

          <div className="onboarding-model-family-copy">
            <strong>
              {t(`onboarding.setup.models.${family}.title`)}
              <span>{t("onboarding.recommended")}</span>
            </strong>
            <p>{t(`onboarding.setup.models.${family}.description`)}</p>
            <small>{t(`onboarding.setup.models.${family}.detail`)}</small>
          </div>

          <div className="onboarding-model-choice">
            {selectedModel ? (
              <div>
                <strong>{selectedModel.name}</strong>
                <span>
                  {formatSize(selectedModel.size_mb)} ·{" "}
                  {modelStatus(selectedModel)}
                </span>
              </div>
            ) : (
              <span>{t("onboarding.setup.models.resolving")}</span>
            )}
            <button
              type="button"
              disabled={!enabled || defaultsLoading || isInstalling}
              aria-expanded={openPicker === family}
              onClick={() =>
                setOpenPicker((current) => (current === family ? null : family))
              }
            >
              {t("onboarding.setup.models.change")}
              <ChevronDown aria-hidden="true" />
            </button>
          </div>
        </div>

        {openPicker === family ? (
          <div className="onboarding-model-picker" role="radiogroup">
            {familyModels.map((model) => {
              const chosen = model.id === selectedModels[family];
              return (
                <button
                  key={model.id}
                  type="button"
                  role="radio"
                  aria-checked={chosen}
                  className={chosen ? "selected" : undefined}
                  onClick={() => chooseModel(family, model.id)}
                >
                  <span>
                    <strong>{model.name}</strong>
                    <small>{model.description}</small>
                  </span>
                  <span>
                    {formatSize(model.size_mb)}
                    {chosen ? <Check aria-hidden="true" /> : null}
                  </span>
                </button>
              );
            })}
          </div>
        ) : null}
      </div>
    );
  };

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
          {(["microphone", "modes", "models", "try"] as const).map(
            (step, index) => (
              <li
                key={step}
                className={
                  index === 2 ? "active" : index < 2 ? "done" : undefined
                }
                aria-current={index === 2 ? "step" : undefined}
              >
                <span className="onboarding-stepper-line" aria-hidden="true" />
                <span>{t(`onboarding.setup.steps.${step}`)}</span>
              </li>
            ),
          )}
        </ol>

        <span className="onboarding-local-note">
          <HardDriveDownload aria-hidden="true" />
          {t("onboarding.setup.models.local")}
        </span>
      </header>

      <main className="onboarding-stage">
        <section className="onboarding-models-step">
          <div className="onboarding-heading">
            <h1>{t("onboarding.setup.models.title")}</h1>
            <p>{t("onboarding.setup.models.description")}</p>
          </div>

          {defaultsError ? (
            <div className="onboarding-model-error" role="alert">
              <span>{defaultsError}</span>
              <button type="button" onClick={resolveDefaults}>
                {t("onboarding.setup.models.retry")}
              </button>
            </div>
          ) : (
            <div className="onboarding-model-families">
              {renderFamily("standard", selectedStandard, standardModels)}
              {renderFamily("streaming", selectedStreaming, streamingModels)}
            </div>
          )}

          <div className="onboarding-model-summary">
            <span>
              <strong>{selectedCount}</strong>{" "}
              {t("onboarding.setup.models.selected", { count: selectedCount })}
            </span>
            <span>
              {totalSizeMb > 0
                ? t("onboarding.setup.models.downloadSize", {
                    size: formatSize(totalSizeMb),
                  })
                : t("onboarding.setup.models.noDownload")}
            </span>
          </div>

          {isInstalling ? (
            <div className="onboarding-download-panel" aria-live="polite">
              {selectedFamilyModels.map((model) => {
                const progress =
                  downloadProgress[model.id]?.percentage ??
                  (model.is_downloaded ||
                  completedDownloadIds.includes(model.id)
                    ? 100
                    : 0);
                return (
                  <div key={model.id} className="onboarding-download-row">
                    <div>
                      <strong>{model.name}</strong>
                      <span>{modelStatus(model)}</span>
                    </div>
                    <div
                      className="onboarding-download-track"
                      aria-hidden="true"
                    >
                      <i style={{ transform: `scaleX(${progress / 100})` }} />
                    </div>
                    <span>{Math.round(progress)}%</span>
                  </div>
                );
              })}
              <button type="button" onClick={stopInstallation}>
                <X aria-hidden="true" />
                {t("onboarding.setup.models.cancel")}
              </button>
            </div>
          ) : null}
        </section>
      </main>

      <footer className="onboarding-footer">
        <div className="onboarding-footer-inner">
          <button
            type="button"
            className="onboarding-back"
            disabled={isInstalling}
            onClick={onBack}
          >
            <ChevronLeft aria-hidden="true" />
            {t("onboarding.setup.models.back")}
          </button>
          <button
            type="button"
            className="onboarding-primary"
            disabled={!canInstall}
            onClick={installSelected}
          >
            {isInstalling ? (
              <Loader2 className="spin" aria-hidden="true" />
            ) : null}
            {isInstalling
              ? t("onboarding.setup.models.installing")
              : t("onboarding.setup.models.install")}
          </button>
        </div>
      </footer>
    </div>
  );
};

export default Onboarding;
