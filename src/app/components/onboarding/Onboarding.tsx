import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowRight,
  Check,
  ChevronLeft,
  Download,
  Loader2,
  X,
} from "lucide-react";
import { commands, type ModelInfo } from "@/bindings";
import { useModelStore } from "@/stores/modelStore";
import { useSettings } from "@/hooks/useSettings";
import { isReviewedFlowModelId } from "@/lib/flowAvailability";
import { OnboardingLayout } from "./OnboardingLayout";
import type { ModelFamily, OnboardingModelDraft } from "./onboardingState";

interface OnboardingProps {
  onBack: () => void;
  onModelSelected: () => void;
  draft: OnboardingModelDraft;
  onDraftChange: Dispatch<SetStateAction<OnboardingModelDraft>>;
}
const formatSize = (sizeMb: number) =>
  sizeMb >= 1024
    ? `${(sizeMb / 1024).toFixed(1)} GB`
    : `${Math.round(sizeMb)} MB`;

export default function Onboarding({
  onBack,
  onModelSelected,
  draft,
  onDraftChange,
}: OnboardingProps) {
  const { t } = useTranslation();
  const { refreshSettings } = useSettings();
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
  const [defaultsLoading, setDefaultsLoading] = useState(true);
  const [defaultsError, setDefaultsError] = useState(false);
  const [installError, setInstallError] = useState("");
  const [isInstalling, setIsInstalling] = useState(false);
  const [activeDownloadId, setActiveDownloadId] = useState<string | null>(null);
  const [completedDownloadIds, setCompletedDownloadIds] = useState<string[]>(
    [],
  );
  const cancellationRequestedRef = useRef(false);
  const mountedRef = useRef(true);
  const activeDownloadRef = useRef<string | null>(null);
  const cancellingRef = useRef(false);
  const [isCancelling, setIsCancelling] = useState(false);
  const { enabledFamilies, selectedModels } = draft;
  const familyModels = useMemo(
    () => ({
      standard: models.filter((model) => !model.supports_streaming),
      streaming: models.filter((model) => model.supports_streaming),
    }),
    [models],
  );

  const resolveDefaults = useCallback(async () => {
    setDefaultsLoading(true);
    setDefaultsError(false);
    try {
      const result = await commands.getOnboardingModelDefaults();
      if (!mountedRef.current) return;
      if (result.status === "error") throw new Error(result.error);
      onDraftChange((current) => ({
        ...current,
        selectedModels: {
          standard:
            current.selectedModels.standard || result.data.standard_model_id,
          streaming:
            current.selectedModels.streaming || result.data.asr_model_id,
        },
      }));
    } catch (error) {
      if (mountedRef.current) setDefaultsError(true);
      console.warn("Failed to resolve onboarding model defaults:", error);
    } finally {
      if (mountedRef.current) setDefaultsLoading(false);
    }
  }, [onDraftChange]);

  useEffect(() => {
    mountedRef.current = true;
    void resolveDefaults();
    return () => {
      mountedRef.current = false;
      cancellationRequestedRef.current = true;
      if (activeDownloadRef.current)
        void cancelDownload(activeDownloadRef.current);
    };
  }, [cancelDownload, resolveDefaults]);
  useEffect(() => {
    if (loading || defaultsLoading) return;
    onDraftChange((current) => {
      const next = { ...current.selectedModels };
      for (const family of ["standard", "streaming"] as const) {
        if (familyModels[family].some((model) => model.id === next[family]))
          continue;
        const fallback =
          familyModels[family].find((model) => model.is_downloaded) ||
          familyModels[family].find(
            (model) => family === "standard" && isReviewedFlowModelId(model.id),
          ) ||
          familyModels[family][0];
        next[family] = fallback?.id || "";
      }
      if (
        next.standard === current.selectedModels.standard &&
        next.streaming === current.selectedModels.streaming
      )
        return current;
      return { ...current, selectedModels: next };
    });
  }, [defaultsLoading, familyModels, loading, onDraftChange]);

  const selectedFamilyModels = (["standard", "streaming"] as ModelFamily[])
    .filter((family) => enabledFamilies[family])
    .map((family) =>
      familyModels[family].find((model) => model.id === selectedModels[family]),
    );
  const selectedCount =
    Number(enabledFamilies.standard) + Number(enabledFamilies.streaming);
  const validModels = selectedFamilyModels.filter((model): model is ModelInfo =>
    Boolean(model),
  );
  const totalSizeMb = validModels.reduce(
    (total, model) => total + (model.is_downloaded ? 0 : model.size_mb),
    0,
  );
  const canInstall =
    !loading &&
    !defaultsLoading &&
    selectedCount > 0 &&
    validModels.length === selectedCount &&
    !isInstalling;

  const installSelected = async () => {
    if (!canInstall) return;
    setIsInstalling(true);
    setInstallError("");
    setCompletedDownloadIds([]);
    cancellationRequestedRef.current = false;
    try {
      for (const model of validModels) {
        if (cancellationRequestedRef.current) return;
        if (model.is_downloaded) continue;
        activeDownloadRef.current = model.id;
        setActiveDownloadId(model.id);
        const succeeded = await downloadModel(model.id);
        activeDownloadRef.current = null;
        if (cancellationRequestedRef.current) return;
        if (!succeeded)
          throw new Error(t("onboarding.setup.models.errors.download"));
        setCompletedDownloadIds((current) => [...current, model.id]);
      }
      if (cancellationRequestedRef.current) return;
      setActiveDownloadId(null);
      if (
        enabledFamilies.standard &&
        !(await selectModel(selectedModels.standard))
      )
        throw new Error(t("onboarding.errors.selectModel"));
      if (cancellationRequestedRef.current) return;
      if (enabledFamilies.streaming) {
        const result = await commands.selectAsrModel(selectedModels.streaming);
        if (result.status === "error") throw new Error(result.error);
      }
      await refreshSettings();
      if (mountedRef.current && !cancellationRequestedRef.current)
        onModelSelected();
    } catch (error) {
      if (mountedRef.current)
        setInstallError(
          error instanceof Error
            ? error.message
            : t("onboarding.setup.models.errors.download"),
        );
    } finally {
      activeDownloadRef.current = null;
      if (mountedRef.current) {
        setActiveDownloadId(null);
        setIsInstalling(false);
      }
    }
  };
  const stopInstallation = async () => {
    if (cancellingRef.current) return;
    cancellingRef.current = true;
    setIsCancelling(true);
    cancellationRequestedRef.current = true;
    try {
      if (activeDownloadRef.current)
        await cancelDownload(activeDownloadRef.current);
    } finally {
      cancellingRef.current = false;
      if (mountedRef.current) setIsCancelling(false);
    }
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
    return t(
      isInstalling
        ? "onboarding.setup.models.waiting"
        : "onboarding.setup.models.ready",
    );
  };
  return (
    <OnboardingLayout
      step={2}
      footer={
        <>
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
            ) : totalSizeMb > 0 ? (
              <Download aria-hidden="true" />
            ) : (
              <ArrowRight aria-hidden="true" />
            )}
            {isInstalling
              ? t("onboarding.setup.models.installing")
              : installError
                ? t("onboarding.setup.models.retryInstall")
                : totalSizeMb > 0
                  ? t("onboarding.setup.models.install")
                  : t("onboarding.setup.continue")}
          </button>
        </>
      }
    >
      <section className="onboarding-models-step">
        <div className="onboarding-heading">
          <h1>{t("onboarding.setup.models.title")}</h1>
          <p>{t("onboarding.setup.models.description")}</p>
        </div>
        {defaultsError && (
          <div className="onboarding-model-error" role="status">
            <span>{t("onboarding.setup.models.defaultsFallback")}</span>
            <button type="button" onClick={resolveDefaults}>
              {t("onboarding.setup.models.retry")}
            </button>
          </div>
        )}
        <fieldset className="onboarding-model-families" disabled={isInstalling}>
          <legend className="sr-only">
            {t("onboarding.setup.models.familyLabel")}
          </legend>
          {(["standard", "streaming"] as const).map((family) => {
            const model = familyModels[family].find(
              (item) => item.id === selectedModels[family],
            );
            return (
              <div
                key={family}
                className={`onboarding-model-family${enabledFamilies[family] ? " selected" : ""}`}
              >
                <label className="onboarding-model-family-label">
                  <input
                    type="checkbox"
                    checked={enabledFamilies[family]}
                    onChange={(event) => {
                      const checked = event.target.checked;
                      onDraftChange((current) => ({
                        ...current,
                        enabledFamilies: {
                          ...current.enabledFamilies,
                          [family]: checked,
                        },
                      }));
                    }}
                  />
                  <span>
                    <strong>
                      {t(`onboarding.setup.models.${family}.title`)}
                    </strong>
                    <p>{t(`onboarding.setup.models.${family}.description`)}</p>
                  </span>
                </label>
                <div className="onboarding-model-choice">
                  <label htmlFor={`onboarding-model-${family}`}>
                    {t("onboarding.setup.models.modelLabel")}
                  </label>
                  <select
                    id={`onboarding-model-${family}`}
                    className="onboarding-select"
                    value={selectedModels[family]}
                    disabled={
                      !enabledFamilies[family] || loading || defaultsLoading
                    }
                    onChange={(event) => {
                      const value = event.target.value;
                      onDraftChange((current) => ({
                        ...current,
                        selectedModels: {
                          ...current.selectedModels,
                          [family]: value,
                        },
                      }));
                    }}
                  >
                    {!model && (
                      <option value="">
                        {t(
                          defaultsLoading || loading
                            ? "onboarding.setup.models.resolving"
                            : "onboarding.setup.models.unavailable",
                        )}
                      </option>
                    )}
                    {familyModels[family].map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.name} — {formatSize(item.size_mb)}
                      </option>
                    ))}
                  </select>
                  <small>
                    {model ? (
                      <>
                        {model.is_downloaded ? (
                          <Check aria-hidden="true" />
                        ) : null}
                        {modelStatus(model)}
                        {family === "standard" &&
                        !isReviewedFlowModelId(model.id)
                          ? ` · ${t("onboarding.setup.models.standardOnly")}`
                          : ""}
                      </>
                    ) : (
                      t("onboarding.setup.models.chooseAlternative")
                    )}
                  </small>
                </div>
              </div>
            );
          })}
        </fieldset>
        <div className="onboarding-model-summary" aria-live="polite">
          <span>
            {selectedCount === 0
              ? t("onboarding.setup.models.noneSelected")
              : t("onboarding.setup.models.selected", { count: selectedCount })}
          </span>
          <strong>
            {totalSizeMb > 0
              ? t("onboarding.setup.models.downloadSize", {
                  size: formatSize(totalSizeMb),
                })
              : selectedCount > 0
                ? t("onboarding.setup.models.noDownload")
                : ""}
          </strong>
        </div>
        {installError && (
          <div className="onboarding-model-error" role="alert">
            {installError}
          </div>
        )}
        {isInstalling && (
          <div className="onboarding-download-panel" aria-live="polite">
            {validModels.map((model) => {
              const progress =
                downloadProgress[model.id]?.percentage ??
                (model.is_downloaded || completedDownloadIds.includes(model.id)
                  ? 100
                  : 0);
              return (
                <div key={model.id} className="onboarding-download-row">
                  <div>
                    <strong>{model.name}</strong>
                    <span>{modelStatus(model)}</span>
                  </div>
                  <progress
                    max={100}
                    value={progress}
                    aria-label={model.name}
                  />
                  <span>{Math.round(progress)}%</span>
                </div>
              );
            })}
            <button
              type="button"
              className="onboarding-skip"
              disabled={isCancelling}
              onClick={stopInstallation}
            >
              <X aria-hidden="true" />
              {t(
                isCancelling
                  ? "onboarding.setup.models.cancelling"
                  : "onboarding.setup.models.cancel",
              )}
            </button>
          </div>
        )}
      </section>
    </OnboardingLayout>
  );
}
