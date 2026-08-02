import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import type { ModelCardStatus } from "@/components/onboarding";
import { ModelCard } from "@/components/onboarding";
import { useModelStore } from "@/stores/modelStore";
import { useSettings } from "@/hooks/useSettings";
import { type ModelInfo } from "@/bindings";

// [GRAIN] The Streaming model browser — the twin of `ModelLibrary` (the
// Standard/Batch list). Since the transcribe-cpp unification both sections read
// the SAME unified model registry; this one shows the `supports_streaming`
// slice and drives the separate `selected_asr_model` setting (what the
// streaming shortcut loads). Downloads/deletes go through the shared
// `modelStore`, so progress state is identical to the batch section.

export const AsrModelLibrary: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const selectedAsrModel = getSetting("selected_asr_model") ?? "";
  const [switchingModelId, setSwitchingModelId] = useState<string | null>(null);

  const {
    models,
    downloadingModels,
    verifyingModels,
    extractingModels,
    downloadProgress,
    downloadStats,
    downloadModel,
    cancelDownload,
    deleteModel,
  } = useModelStore();

  const streamingModels = useMemo(
    () => models.filter((m: ModelInfo) => m.supports_streaming),
    [models],
  );

  const getModelStatus = (modelId: string): ModelCardStatus => {
    if (modelId in extractingModels) return "extracting";
    if (modelId in verifyingModels) return "verifying";
    if (modelId in downloadingModels) return "downloading";
    if (switchingModelId === modelId) return "switching";
    if (modelId === selectedAsrModel) return "active";
    const model = streamingModels.find((m) => m.id === modelId);
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
    const model = streamingModels.find((m) => m.id === modelId);
    const modelName = model?.name || modelId;
    const isActive = modelId === selectedAsrModel;
    const confirmed = await ask(
      isActive
        ? t("settings.models.deleteActiveConfirm", { modelName })
        : t("settings.models.deleteConfirm", { modelName }),
      { title: t("settings.models.deleteTitle"), kind: "warning" },
    );
    if (confirmed) {
      // Deleting the active streaming model clears the selection so the picker
      // doesn't keep showing it as active.
      if (isActive) await updateSetting("selected_asr_model", "");
      try {
        await deleteModel(modelId);
      } catch (err) {
        console.error(`Failed to delete streaming model ${modelId}:`, err);
      }
    }
  };

  const handleCancel = async (modelId: string) => {
    try {
      await cancelDownload(modelId);
    } catch (err) {
      console.error(`Failed to cancel streaming download for ${modelId}:`, err);
    }
  };

  const { downloadedModels, availableModels } = useMemo(() => {
    const downloaded: ModelInfo[] = [];
    const available: ModelInfo[] = [];
    for (const model of streamingModels) {
      if (
        model.is_custom ||
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
  }, [streamingModels, downloadingModels, extractingModels, selectedAsrModel]);

  if (streamingModels.length === 0) {
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
              model={model}
              status={getModelStatus(model.id)}
              onSelect={handleSelect}
              onDownload={handleDownload}
              onDelete={handleDelete}
              onCancel={handleCancel}
              downloadProgress={downloadProgress[model.id]?.percentage}
              downloadSpeed={downloadStats[model.id]?.speed}
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
              model={model}
              status={getModelStatus(model.id)}
              onSelect={handleSelect}
              onDownload={handleDownload}
              onDelete={handleDelete}
              onCancel={handleCancel}
              downloadProgress={downloadProgress[model.id]?.percentage}
              downloadSpeed={downloadStats[model.id]?.speed}
              showRecommended={false}
            />
          ))}
        </div>
      )}
    </div>
  );
};
