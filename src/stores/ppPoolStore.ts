/**
 * [GRAIN] Singleton Zustand store for the post-process (LLM) provider pool.
 *
 * Same rationale as sttPoolStore — one live view shared between the settings
 * panel and the quick panel, so provider renames / key additions / smart
 * rotation changes are reflected everywhere without a second fetch.
 */
import { create } from "zustand";
import { commands, type PostProcessProvider, type PpPoolView } from "@/bindings";

export interface PpPoolStore {
  view: PpPoolView | null;
  loading: boolean;
  error: string | null;

  // Derived
  smartRotation: boolean;
  providers: PostProcessProvider[];
  selectedProviderId: string;
  providersWithKeys: Set<string>;
  models: Record<string, string>;

  // Actions
  reload: () => Promise<void>;
  setSmartRotation: (enabled: boolean) => Promise<void>;
  setActiveProvider: (id: string) => Promise<void>;
  upsertProvider: (
    provider: PostProcessProvider,
    apiKey: string | null,
    model: string | null,
  ) => Promise<void>;
  setProviderEnabled: (provider: PostProcessProvider, enabled: boolean) => Promise<void>;
  removeProvider: (id: string) => Promise<void>;
  fetchModels: (id: string) => Promise<string[]>;
}

export const usePpPoolStore = create<PpPoolStore>()((set, get) => ({
  view: null,
  loading: true,
  error: null,

  smartRotation: false,
  providers: [],
  selectedProviderId: "",
  providersWithKeys: new Set(),
  models: {},

  reload: async () => {
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
        smartRotation: v.smart_rotation ?? false,
        providers: v.providers ?? [],
        selectedProviderId: v.selected_provider_id ?? "",
        providersWithKeys: new Set(v.providers_with_keys ?? []),
        models: modelsOut,
      });
    } else {
      set({ error: res.error, loading: false });
    }
  },

  setSmartRotation: async (enabled) => {
    const res = await commands.ppSetSmartRotation(enabled);
    if (res.status === "error") {
      set({ error: res.error });
      return;
    }
    await get().reload();
  },

  setActiveProvider: async (id) => {
    const res = await commands.setPostProcessProvider(id);
    if (res.status === "error") {
      set({ error: res.error });
      return;
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

/** Call once at app startup. */
export const initPpPool = () => usePpPoolStore.getState().reload();
