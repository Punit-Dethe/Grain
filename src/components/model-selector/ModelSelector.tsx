import React, { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import { getTranslatedModelName } from "../../lib/utils/modelTranslation";
import { useModelStore } from "../../stores/modelStore";
import { useSettings } from "../../hooks/useSettings";
import DownloadProgressDisplay from "./DownloadProgressDisplay";

import { ModelStateEvent } from "@/lib/types/events";

// [GRAIN] READ-ONLY engine status for the footer. With per-category model
// selections (standard batch model vs streaming model) a single global picker
// here was wrong — selection lives in Settings → Speech to Text. This shows
// whichever model currently occupies the ONE shared engine slot (batch OR
// streaming, they replace each other) and whether it is loaded, plus transient
// download/verify/extract activity from the shared registry.

type ModelStatus =
  | "ready"
  | "loading"
  | "downloading"
  | "verifying"
  | "extracting"
  | "error"
  | "unloaded"
  | "none";

// Each state resolves to a warm, desaturated status token (see App.css) rather
// than a raw Tailwind hue, so the dot harmonizes with the beige paper. Only the
// transient states (loading/downloading/verifying/extracting) pulse; the rest
// are steady.
const STATUS_DOT: Record<ModelStatus, string> = {
  ready: "bg-status-ready",
  loading: "bg-status-load animate-pulse",
  downloading: "bg-accent animate-pulse",
  verifying: "bg-status-warn animate-pulse",
  extracting: "bg-status-warn animate-pulse",
  error: "bg-status-error",
  unloaded: "bg-status-idle",
  none: "bg-status-error",
};

const ModelSelector: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const {
    models,
    downloadProgress,
    downloadStats,
    verifyingModels,
    extractingModels,
  } = useModelStore();

  const [modelStatus, setModelStatus] = useState<ModelStatus>("unloaded");
  const [modelError, setModelError] = useState<string | null>(null);
  // The model occupying (or last occupying) the shared engine slot. Falls back
  // to the persisted standard selection before anything has ever loaded.
  const [engineModelId, setEngineModelId] = useState<string | null>(null);

  const selectedModel = (getSetting("selected_model") as string) ?? "";
  const displayModelId = engineModelId || selectedModel;

  // Initial snapshot: is something already resident in the engine slot?
  useEffect(() => {
    const checkStatus = async () => {
      try {
        const statusResult = await commands.getTranscriptionModelStatus();
        if (statusResult.status === "ok") {
          if (statusResult.data) {
            setEngineModelId(statusResult.data);
            setModelStatus("ready");
          } else {
            setModelStatus(selectedModel ? "unloaded" : "none");
          }
        }
      } catch {
        setModelStatus("error");
        setModelError("Failed to check model status");
      }
    };
    checkStatus();
    // Re-derive the "none" fallback if the selection appears later.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedModel]);

  useEffect(() => {
    // Follow the engine slot through model loading lifecycle events. The
    // payload's model_id tells us WHICH model (batch or streaming) is being
    // loaded, so the indicator naturally flips between categories.
    const modelStateUnlisten = listen<ModelStateEvent>(
      "model-state-changed",
      (event) => {
        const { event_type, model_id, error } = event.payload;
        switch (event_type) {
          case "loading_started":
            if (model_id) setEngineModelId(model_id);
            setModelStatus("loading");
            setModelError(null);
            break;
          case "loading_completed":
            if (model_id) setEngineModelId(model_id);
            setModelStatus("ready");
            setModelError(null);
            break;
          case "loading_failed":
            setModelStatus("error");
            setModelError(error || "Failed to load model");
            break;
          case "unloaded":
            // Keep showing the last resident model's name, just as unloaded.
            setModelStatus("unloaded");
            setModelError(null);
            break;
        }
      },
    );

    return () => {
      modelStateUnlisten.then((fn) => fn());
    };
  }, []);

  const getModelDisplayText = (): string => {
    const verifyingKeys = Object.keys(verifyingModels);
    if (verifyingKeys.length > 0) {
      if (verifyingKeys.length === 1) {
        const modelId = verifyingKeys[0];
        const model = models.find((m) => m.id === modelId);
        const modelName = model
          ? getTranslatedModelName(model, t)
          : t("modelSelector.verifyingGeneric").replace("...", "");
        return t("modelSelector.verifying", { modelName });
      } else {
        return t("modelSelector.verifyingGeneric");
      }
    }

    const extractingKeys = Object.keys(extractingModels);
    if (extractingKeys.length > 0) {
      if (extractingKeys.length === 1) {
        const modelId = extractingKeys[0];
        const model = models.find((m) => m.id === modelId);
        const modelName = model
          ? getTranslatedModelName(model, t)
          : t("modelSelector.extractingGeneric").replace("...", "");
        return t("modelSelector.extracting", { modelName });
      } else {
        return t("modelSelector.extractingMultiple", {
          count: extractingKeys.length,
        });
      }
    }

    const progressValues = Object.values(downloadProgress);
    if (progressValues.length > 0) {
      if (progressValues.length === 1) {
        const progress = progressValues[0];
        const percentage = Math.max(
          0,
          Math.min(100, Math.round(progress.percentage)),
        );
        return t("modelSelector.downloading", { percentage });
      } else {
        return t("modelSelector.downloadingMultiple", {
          count: progressValues.length,
        });
      }
    }

    const currentModelInfo = models.find((m) => m.id === displayModelId);

    switch (modelStatus) {
      case "ready":
        return currentModelInfo
          ? getTranslatedModelName(currentModelInfo, t)
          : t("modelSelector.modelReady");
      case "loading":
        return currentModelInfo
          ? t("modelSelector.loading", {
              modelName: getTranslatedModelName(currentModelInfo, t),
            })
          : t("modelSelector.loadingGeneric");
      case "error":
        return modelError || t("modelSelector.modelError");
      case "unloaded":
        return currentModelInfo
          ? getTranslatedModelName(currentModelInfo, t)
          : t("modelSelector.modelUnloaded");
      case "none":
        return t("modelSelector.noModelDownloadRequired");
      default:
        return currentModelInfo
          ? getTranslatedModelName(currentModelInfo, t)
          : t("modelSelector.modelUnloaded");
    }
  };

  // Derive display status from model status + store state
  const getDisplayStatus = (): ModelStatus => {
    if (Object.keys(verifyingModels).length > 0) return "verifying";
    if (Object.keys(extractingModels).length > 0) return "extracting";
    if (Object.keys(downloadProgress).length > 0) return "downloading";
    return modelStatus;
  };

  const status = getDisplayStatus();
  const displayText = getModelDisplayText();
  const loadedLabel =
    status === "ready"
      ? t("modelSelector.stateLoaded", { defaultValue: "loaded" })
      : status === "unloaded"
        ? t("modelSelector.stateUnloaded", { defaultValue: "unloaded" })
        : null;

  return (
    <>
      {/* Read-only engine status: dot + model name (+ loaded/unloaded). */}
      <div
        className="flex items-center gap-2 cursor-default"
        title={`Model status: ${displayText}`}
      >
        <div
          className={`w-2 h-2 rounded-full ${STATUS_DOT[status] ?? "bg-status-idle"}`}
        />
        <span className="max-w-40 truncate">{displayText}</span>
        {loadedLabel && (
          <span className="font-mono text-[0.62rem] uppercase tracking-wider text-ink-faint">
            {loadedLabel}
          </span>
        )}
      </div>

      {/* Download Progress Bar for Models */}
      <DownloadProgressDisplay
        downloadProgress={downloadProgress}
        downloadStats={downloadStats}
      />
    </>
  );
};

export default ModelSelector;
