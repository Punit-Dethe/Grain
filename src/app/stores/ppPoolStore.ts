/**
 * [GRAIN] Singleton Zustand store for the post-process (LLM) provider pool.
 *
 * One live view shared between the settings
 * panel and the quick panel, so provider renames / key additions / fallback
 * changes are reflected everywhere without a second fetch.
 */
import { create } from "zustand";
import {
  commands,
  type PostProcessProvider,
  type PpPoolView,
} from "@/bindings";

export interface PpPoolStore {
  view: PpPoolView | null;
  loading: boolean;
  error: string | null;

  // Derived
  fallbackEnabled: boolean;
  providers: PostProcessProvider[];
  selectedProviderId: string;
  providersWithKeys: Set<string>;
  configuredProviderIds: Set<string>;
  models: Record<string, string>;

  // Actions
  reload: () => Promise<void>;
  setFallbackEnabled: (enabled: boolean) => Promise<void>;
  setActiveProvider: (id: string) => Promise<void>;
  upsertProvider: (
    provider: PostProcessProvider,
    apiKey: string | null,
    model: string | null,
  ) => Promise<void>;
  setProviderEnabled: (
    provider: PostProcessProvider,
    enabled: boolean,
  ) => Promise<void>;
  removeProvider: (id: string) => Promise<void>;
  fetchModels: (id: string) => Promise<string[]>;
  reorderProviders: (ids: string[]) => Promise<void>;
}

export const usePpPoolStore = create<PpPoolStore>()((set, get) => ({
  view: null,
  loading: true,
  error: null,

  fallbackEnabled: false,
  providers: [],
  selectedProviderId: "",
  providersWithKeys: new Set(),
  configuredProviderIds: new Set(),
  models: {},

  reload: async () => {
    try {
      const res = await commands.ppGetPool();
      if (res.status === "ok") {
        const v = res.data;
        const modelsOut: Record<string, string> = {};
        for (const [k, val] of Object.entries(v.models ?? {})) {
          if (typeof val === "string") modelsOut[k] = val;
        }
        set({
          view: v,
          loading: false,
          error: null,
          fallbackEnabled: v.fallback_enabled ?? false,
          providers: v.providers ?? [],
          selectedProviderId: v.selected_provider_id ?? "",
          providersWithKeys: new Set(v.providers_with_keys ?? []),
          configuredProviderIds: new Set(v.configured_provider_ids ?? []),
          models: modelsOut,
        });
      } else {
        throw new Error(res.error);
      }
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : String(error),
        loading: false,
      });
      throw error;
    }
  },

  reorderProviders: async (ids) => {
    const res = await commands.ppReorderProviders(ids);
    if (res.status === "error") {
      set({ error: res.error });
      throw new Error(res.error);
    }
    await get().reload();
  },

  setFallbackEnabled: async (enabled) => {
    const res = await commands.ppSetFallbackEnabled(enabled);
    if (res.status === "error") {
      set({ error: res.error });
      throw new Error(res.error);
    }
    await get().reload();
  },

  setActiveProvider: async (id) => {
    const res = await commands.setPostProcessProvider(id);
    if (res.status === "error") {
      set({ error: res.error });
      throw new Error(res.error);
    }
    await get().reload();
  },

  upsertProvider: async (provider, apiKey, model) => {
    const res = await commands.ppUpsertProvider(provider, apiKey, model);
    if (res.status === "error") {
      set({ error: res.error });
      throw new Error(res.error);
    }
    await get().reload();
  },

  setProviderEnabled: async (provider, enabled) => {
    await get().upsertProvider({ ...provider, enabled }, null, null);
  },

  removeProvider: async (id) => {
    const res = await commands.ppRemoveProvider(id);
    if (res.status === "error") {
      set({ error: res.error });
      throw new Error(res.error);
    }
    await get().reload();
  },

  fetchModels: async (id) => {
    const res = await commands.fetchPostProcessModels(id);
    return res.status === "ok" ? res.data : [];
  },
}));

let initialization: Promise<void> | null = null;

/** Lazily load on first use. Concurrent StrictMode mounts share one request;
 * failed attempts clear the guard so entering the subsection again can retry. */
export const initPpPool = (): Promise<void> => {
  const store = usePpPoolStore.getState();
  if (store.view) return Promise.resolve();
  if (initialization) return initialization;

  usePpPoolStore.setState({ loading: true, error: null });
  initialization = store.reload().then(
    () => {
      const { view, error } = usePpPoolStore.getState();
      initialization = null;
      if (!view) {
        throw new Error(
          error ?? "Failed to load post-processing provider pool",
        );
      }
    },
    (error) => {
      initialization = null;
      usePpPoolStore.setState({
        loading: false,
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    },
  );
  return initialization;
};
