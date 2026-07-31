/**
 * [GRAIN] Singleton Zustand store for the STT provider pool.
 *
 * Lifting useSttPool out of component-local state means the settings panel
 * and the quick panel share one live view — provider name changes, smart
 * rotation toggles, and key additions are reflected everywhere immediately
 * without a second round-trip.
 */
import { create } from "zustand";
import { commands, type SttProvider, type SttPoolView } from "@/bindings";

const LOCAL_PROVIDER_ID = "local";

export interface SttPoolStore {
  view: SttPoolView | null;
  loading: boolean;
  error: string | null;

  // Derived (computed inline — no useMemo needed outside)
  smartRotation: boolean;
  providers: SttProvider[];
  cloudProviders: SttProvider[];
  localProvider: SttProvider | undefined;
  providersWithKeys: Set<string>;

  // Actions
  reload: () => Promise<void>;
  setSmartRotation: (enabled: boolean) => Promise<void>;
  upsertProvider: (
    provider: SttProvider,
    apiKey: string | null,
  ) => Promise<void>;
  setProviderEnabled: (
    provider: SttProvider,
    enabled: boolean,
  ) => Promise<void>;
  removeProvider: (id: string) => Promise<void>;
}

export const useSttPoolStore = create<SttPoolStore>()((set, get) => ({
  view: null,
  loading: true,
  error: null,

  // Derived — kept in sync by reload()
  smartRotation: false,
  providers: [],
  cloudProviders: [],
  localProvider: undefined,
  providersWithKeys: new Set(),

  reload: async () => {
    const res = await commands.sttGetPool();
    if (res.status === "ok") {
      const v = res.data;
      const providers = v.providers ?? [];
      set({
        view: v,
        loading: false,
        error: null,
        smartRotation: v.smart_rotation ?? false,
        providers,
        cloudProviders: providers.filter((p) => p.kind !== "local"),
        localProvider: providers.find((p) => p.id === LOCAL_PROVIDER_ID),
        providersWithKeys: new Set(v.providers_with_keys ?? []),
      });
    } else {
      set({ error: res.error, loading: false });
    }
  },

  setSmartRotation: async (enabled) => {
    const res = await commands.sttSetSmartRotation(enabled);
    if (res.status === "error") {
      set({ error: res.error });
      return;
    }
    await get().reload();
  },

  upsertProvider: async (provider, apiKey) => {
    const res = await commands.sttUpsertProvider(provider, apiKey);
    if (res.status === "error") {
      set({ error: res.error });
      throw new Error(res.error);
    }
    await get().reload();
  },

  setProviderEnabled: async (provider, enabled) => {
    await get().upsertProvider({ ...provider, enabled }, null);
  },

  removeProvider: async (id) => {
    const res = await commands.sttRemoveProvider(id);
    if (res.status === "error") {
      set({ error: res.error });
      throw new Error(res.error);
    }
    await get().reload();
  },
}));

let initialization: Promise<void> | null = null;

/** Lazily load on first use. Concurrent StrictMode mounts share one request;
 * failed attempts clear the guard so entering the subsection again can retry. */
export const initSttPool = (): Promise<void> => {
  const store = useSttPoolStore.getState();
  if (store.view) return Promise.resolve();
  if (initialization) return initialization;

  useSttPoolStore.setState({ loading: true, error: null });
  initialization = store.reload().then(
    () => {
      const { view, error } = useSttPoolStore.getState();
      initialization = null;
      if (!view) throw new Error(error ?? "Failed to load STT provider pool");
    },
    (error) => {
      initialization = null;
      useSttPoolStore.setState({
        loading: false,
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    },
  );
  return initialization;
};
