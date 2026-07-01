import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import type { ModelCardStatus } from "@/components/onboarding";
import { ModelCard } from "@/components/onboarding";
import { useAsrModelStore } from "@/stores/asrModelStore";
import { useSettings } from "@/hooks/useSettings";
import { type AsrModelInfo, type ModelInfo } from "@/bindings";

// [GRAIN] The Streaming / Native-ASR model browser — the twin of `ModelLibrary`
// (the Batch/Rolling browser) against the SEPARATE ASR registry. It reuses the
// exact same `ModelCard` so both categories share one beautiful interface, via
// a small adapter that maps the ASR registry's shape onto the card's
// `ModelInfo` props. ASR bundles have no accuracy/speed scores or translation,
// so those fields are zeroed/false — `ModelCard` self-hides them, leaving the
// card showing name, description, languages, and size exactly like a Batch one.

/** Adapt an `AsrModelInfo` to the `ModelInfo` shape `ModelCard` renders.
 *  Fields the ASR registry doesn't carry are given neutral values that make
 *  `ModelCard` hide the corresponding UI (score bars, translation tag, custom
 *  badge), never fabricate it. */
const asrToModelInfo = (m: AsrModelInfo): ModelInfo => ({
  id: m.id,
  name: m.name,
  description: `Streaming · ${m.backend} · ~${m.memory_mb} MB RAM`,
  filename: "",
  url: null,
  sha256: null,
  size_mb: m.size_mb,
  is_downloaded: m.is_downloaded,
  is_downloading: m.is_downloading,
  partial_size: 0,
  is_directory: true,
  // Not used by ModelCard's rendering; a valid enum value keeps the type honest.
  engine_type: "Parakeet",
  accuracy_score: 0,
  speed_score: 0,
  supports_translation: false,
  is_recommended: false,
  supported_languages: m.languages,
  supports_language_selection: m.languages.length > 1,
  is_custom: false,
});

export const AsrModelLibrary: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const selectedAsrModel = getSetting("selected_asr_model") ?? "";
  const [switchingModelId, setSwitchingModelId] = useState<string | null>(null);

  const {
    models,
    downloadingModels,
    extractingModels,
    downloadModel,
    cancelDownload,
    deleteModel,
    getDownloadProgress,
    getDownloadSpeed,
  } = useAsrModelStore();

  const getModelStatus = (modelId: string): ModelCardStatus => {
    // Extraction is checked before download so the large streaming models show
    // the "extracting" phase once bytes finish transferring.
    if (modelId in extractingModels) return "extracting";
    if (modelId in downloadingModels) return "downloading";
    if (switchingModelId === modelId) return "switching";
    if (modelId === selectedAsrModel) return "active";
    const model = models.find((m) => m.id === modelId);
    if (model?.is_downloaded) return "available";
    return "downloadable";
  };

  const handleSelect = async (modelId: string) => {
    setSwitchingModelId(modelId);
    try {
      await updateSetting("selected_asr_model", modelId);
    } finally {
      setSwitchingModelId(null);
    }
  };

  const handleDownload = async (modelId: string) => {
    await downloadModel(modelId);
  };

  const handleDelete = async (modelId: string) => {
    const model = models.find((m) => m.id === modelId);
    const modelName = model?.name || modelId;
    const isActive = modelId === selectedAsrModel;
    const confirmed = await ask(
      isActive
        ? t("settings.models.deleteActiveConfirm", { modelName })
        : t("settings.models.deleteConfirm", { modelName }),
      { title: t("settings.models.deleteTitle"), kind: "warning" },
    );
    if (confirmed) {
      // Deleting the active ASR model clears the selection backend-side; mirror
      // that here so the picker doesn't keep showing it as active.
      if (isActive) await updateSetting("selected_asr_model", "");
      try {
        await deleteModel(modelId);
      } catch (err) {
        console.error(`Failed to delete ASR model ${modelId}:`, err);
      }
    }
  };

  const handleCancel = async (modelId: string) => {
    try {
      await cancelDownload(modelId);
    } catch (err) {
      console.error(`Failed to cancel ASR download for ${modelId}:`, err);
    }
  };

  const { downloadedModels, availableModels } = useMemo(() => {
    const downloaded: AsrModelInfo[] = [];
    const available: AsrModelInfo[] = [];
    for (const model of models) {
      if (
        model.is_downloaded ||
        model.id in downloadingModels ||
        model.id in extractingModels
      ) {
        downloaded.push(model);
      } else {
        available.push(model);
      }
    }
    downloaded.sort((a, b) => {
      if (a.id === selectedAsrModel) return -1;
      if (b.id === selectedAsrModel) return 1;
      return 0;
    });
    return { downloadedModels: downloaded, availableModels: available };
  }, [models, downloadingModels, extractingModels, selectedAsrModel]);

  if (models.length === 0) {
    return (
      <div className="text-center py-8 text-ink-soft">
        {t("settings.models.noModelsMatch")}
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {downloadedModels.length > 0 && (
        <div className="space-y-3">
          <h2 className="text-sm font-medium text-ink-soft">
            {t("settings.models.yourModels")}
          </h2>
          {downloadedModels.map((model) => (
            <ModelCard
              key={model.id}
              model={asrToModelInfo(model)}
              status={getModelStatus(model.id)}
              onSelect={handleSelect}
              onDownload={handleDownload}
              onDelete={handleDelete}
              onCancel={handleCancel}
              downloadProgress={getDownloadProgress(model.id)?.percentage}
              downloadSpeed={getDownloadSpeed(model.id)}
              showRecommended={false}
            />
          ))}
        </div>
      )}

      {availableModels.length > 0 && (
        <div className="space-y-3">
          <h2 className="text-sm font-medium text-ink-soft">
            {t("settings.models.availableModels")}
          </h2>
          {availableModels.map((model) => (
            <ModelCard
              key={model.id}
              model={asrToModelInfo(model)}
              status={getModelStatus(model.id)}
              onSelect={handleSelect}
              onDownload={handleDownload}
              onDelete={handleDelete}
              onCancel={handleCancel}
              downloadProgress={getDownloadProgress(model.id)?.percentage}
              downloadSpeed={getDownloadSpeed(model.id)}
              showRecommended={false}
            />
          ))}
        </div>
      )}
    </div>
  );
};
