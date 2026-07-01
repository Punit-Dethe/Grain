import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import { produce } from "immer";
import { listen } from "@tauri-apps/api/event";
import { commands, type AsrModelInfo } from "@/bindings";
import { toast } from "sonner";

// [GRAIN] Native ASR model registry store — mirrors `modelStore.ts` (the
// Batch/Rolling model browser) but against the separate ASR catalog/commands.
// Differences from `modelStore`, both reflecting what the backend actually
// exposes (`managers::asr_model::AsrModelManager`):
// - No verify/extract phases or their events — `download_asr_model` awaits the
//   whole fetch+extract and resolves/rejects directly, so there's nothing to
//   listen for beyond progress.
// - No "current model" here: `selected_asr_model` is a plain setting (wired
//   into `settingsStore`'s generic updater table), read via `useSettings()`.

interface AsrDownloadProgress {
  model_id: string;
  downloaded: number;
  total: number;
  percentage: number;
}

interface DownloadStats {
  startTime: number;
  lastUpdate: number;
  totalDownloaded: number;
  speed: number; // MB/s
}

interface AsrModelsStore {
  models: AsrModelInfo[];
  downloadingModels: Record<string, true>;
  extractingModels: Record<string, true>;
  downloadProgress: Record<string, AsrDownloadProgress>;
  downloadStats: Record<string, DownloadStats>;
  loading: boolean;
  error: string | null;
  initialized: boolean;

  initialize: () => Promise<void>;
  loadModels: () => Promise<void>;
  downloadModel: (modelId: string) => Promise<boolean>;
  cancelDownload: (modelId: string) => Promise<boolean>;
  deleteModel: (modelId: string) => Promise<boolean>;
  getDownloadProgress: (modelId: string) => AsrDownloadProgress | undefined;
  getDownloadSpeed: (modelId: string) => number | undefined;
  isExtracting: (modelId: string) => boolean;

  setModels: (models: AsrModelInfo[]) => void;
  setError: (error: string | null) => void;
  setLoading: (loading: boolean) => void;
}

export const useAsrModelStore = create<AsrModelsStore>()(
  subscribeWithSelector((set, get) => ({
    models: [],
    downloadingModels: {},
    extractingModels: {},
    downloadProgress: {},
    downloadStats: {},
    loading: true,
    error: null,
    initialized: false,

    setModels: (models) => set({ models }),
    setError: (error) => set({ error }),
    setLoading: (loading) => set({ loading }),

    loadModels: async () => {
      try {
        const result = await commands.listAsrModels();
        if (result.status === "ok") {
          set({ models: result.data, error: null });
          // Sync downloading state from the backend's single-flight guard, the
          // same reconciliation modelStore does for the Batch registry.
          set(
            produce((state: AsrModelsStore) => {
              const backendDownloading: Record<string, true> = {};
              result.data
                .filter((m) => m.is_downloading)
                .forEach((m) => {
                  backendDownloading[m.id] = true;
                });
              Object.keys(backendDownloading).forEach((id) => {
                state.downloadingModels[id] = true;
              });
              Object.keys(state.downloadingModels).forEach((id) => {
                if (!backendDownloading[id] && !state.downloadProgress[id]) {
                  delete state.downloadingModels[id];
                }
              });
            }),
          );
        } else {
          set({ error: `Failed to load ASR models: ${result.error}` });
        }
      } catch (err) {
        set({ error: `Failed to load ASR models: ${err}` });
      } finally {
        set({ loading: false });
      }
    },

    downloadModel: async (modelId: string) => {
      try {
        set({ error: null });
        set(
          produce((state: AsrModelsStore) => {
            state.downloadingModels[modelId] = true;
            state.downloadProgress[modelId] = {
              model_id: modelId,
              downloaded: 0,
              total: 0,
              percentage: 0,
            };
          }),
        );
        const result = await commands.downloadAsrModel(modelId);
        if (result.status !== "ok") {
          set(
            produce((state: AsrModelsStore) => {
              delete state.downloadingModels[modelId];
              delete state.extractingModels[modelId];
              delete state.downloadProgress[modelId];
              delete state.downloadStats[modelId];
            }),
          );
          toast.error(result.error);
        } else {
          set(
            produce((state: AsrModelsStore) => {
              delete state.downloadingModels[modelId];
              delete state.extractingModels[modelId];
              delete state.downloadProgress[modelId];
              delete state.downloadStats[modelId];
            }),
          );
        }
        await get().loadModels();
        return result.status === "ok";
      } catch (err) {
        set(
          produce((state: AsrModelsStore) => {
            delete state.downloadingModels[modelId];
            delete state.downloadProgress[modelId];
            delete state.downloadStats[modelId];
          }),
        );
        set({ error: `Failed to download ASR model: ${err}` });
        return false;
      }
    },

    cancelDownload: async (modelId: string) => {
      try {
        set({ error: null });
        const result = await commands.cancelAsrModelDownload(modelId);
        if (result.status === "ok") {
          set(
            produce((state: AsrModelsStore) => {
              delete state.downloadingModels[modelId];
              delete state.extractingModels[modelId];
              delete state.downloadProgress[modelId];
              delete state.downloadStats[modelId];
            }),
          );
          await get().loadModels();
          return true;
        }
        set({ error: `Failed to cancel ASR download: ${result.error}` });
        return false;
      } catch (err) {
        set({ error: `Failed to cancel ASR download: ${err}` });
        return false;
      }
    },

    deleteModel: async (modelId: string) => {
      try {
        set({ error: null });
        const result = await commands.deleteAsrModel(modelId);
        if (result.status === "ok") {
          await get().loadModels();
          return true;
        }
        set({ error: `Failed to delete ASR model: ${result.error}` });
        return false;
      } catch (err) {
        set({ error: `Failed to delete ASR model: ${err}` });
        return false;
      }
    },

    getDownloadProgress: (modelId: string) => get().downloadProgress[modelId],
    getDownloadSpeed: (modelId: string) => get().downloadStats[modelId]?.speed,
    isExtracting: (modelId: string) => modelId in get().extractingModels,

    initialize: async () => {
      if (get().initialized) return;
      await get().loadModels();

      listen<string>("asr-model-extraction-started", (event) => {
        const modelId = event.payload;
        set(
          produce((state: AsrModelsStore) => {
            state.extractingModels[modelId] = true;
          }),
        );
      });

      listen<string>("asr-model-extraction-completed", (event) => {
        const modelId = event.payload;
        set(
          produce((state: AsrModelsStore) => {
            delete state.extractingModels[modelId];
          }),
        );
      });

      listen<AsrDownloadProgress>("asr-model-download-progress", (event) => {
        const progress = event.payload;
        set(
          produce((state: AsrModelsStore) => {
            state.downloadProgress[progress.model_id] = progress;
          }),
        );

        const now = Date.now();
        set(
          produce((state: AsrModelsStore) => {
            const current = state.downloadStats[progress.model_id];
            if (!current) {
              state.downloadStats[progress.model_id] = {
                startTime: now,
                lastUpdate: now,
                totalDownloaded: progress.downloaded,
                speed: 0,
              };
            } else {
              const timeDiff = (now - current.lastUpdate) / 1000;
              const bytesDiff = progress.downloaded - current.totalDownloaded;
              if (timeDiff > 0.5) {
                const currentSpeed = bytesDiff / (1024 * 1024) / timeDiff;
                const validCurrentSpeed = Math.max(0, currentSpeed);
                const smoothedSpeed =
                  current.speed > 0
                    ? current.speed * 0.8 + validCurrentSpeed * 0.2
                    : validCurrentSpeed;
                state.downloadStats[progress.model_id] = {
                  startTime: current.startTime,
                  lastUpdate: now,
                  totalDownloaded: progress.downloaded,
                  speed: Math.max(0, smoothedSpeed),
                };
              }
            }
          }),
        );
      });

      set({ initialized: true });
    },
  })),
);
