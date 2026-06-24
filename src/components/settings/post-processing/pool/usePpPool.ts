import { useCallback, useEffect, useMemo, useState } from "react";
import {
  commands,
  type PostProcessProvider,
  type PpPoolView,
} from "@/bindings";

/** Built-in providers seeded by the backend — these can't be removed (only their
 *  key/model/quota edited); the backend re-seeds them anyway. User-added entries
 *  (multi-key duplicates, custom endpoints) get generated ids and ARE removable. */
export const BUILTIN_PP_IDS = new Set([
  "openai",
  "zai",
  "openrouter",
  "anthropic",
  "groq",
  "cerebras",
  "bedrock_mantle",
  "custom",
  "apple_intelligence",
]);

export type PpPoolState = {
  loading: boolean;
  error: string | null;
  smartRotation: boolean;
  providers: PostProcessProvider[];
  selectedProviderId: string;
  providersWithKeys: Set<string>;
  models: Record<string, string>;
  reload: () => Promise<void>;
  setSmartRotation: (enabled: boolean) => Promise<void>;
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
};

/**
 * Drives the post-process (LLM) rotation pool through the dedicated `pp_*`
 * commands. Keys stay backend-only — the view only learns which ids have a key.
 * The single-active selection (rotation OFF) uses `setPostProcessProvider`.
 */
export const usePpPool = (): PpPoolState => {
  const [view, setView] = useState<PpPoolView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    const res = await commands.ppGetPool();
    if (res.status === "ok") {
      setView(res.data);
      setError(null);
    } else {
      setError(res.error);
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const setSmartRotation = useCallback(
    async (enabled: boolean) => {
      const res = await commands.ppSetSmartRotation(enabled);
      if (res.status === "error") {
        setError(res.error);
        return;
      }
      await reload();
    },
    [reload],
  );

  const setActiveProvider = useCallback(
    async (id: string) => {
      const res = await commands.setPostProcessProvider(id);
      if (res.status === "error") {
        setError(res.error);
        return;
      }
      await reload();
    },
    [reload],
  );

  const upsertProvider = useCallback(
    async (
      provider: PostProcessProvider,
      apiKey: string | null,
      model: string | null,
    ) => {
      const res = await commands.ppUpsertProvider(provider, apiKey, model);
      if (res.status === "error") {
        setError(res.error);
        throw new Error(res.error);
      }
      await reload();
    },
    [reload],
  );

  const setProviderEnabled = useCallback(
    async (provider: PostProcessProvider, enabled: boolean) => {
      await upsertProvider({ ...provider, enabled }, null, null);
    },
    [upsertProvider],
  );

  const removeProvider = useCallback(
    async (id: string) => {
      const res = await commands.ppRemoveProvider(id);
      if (res.status === "error") {
        setError(res.error);
        throw new Error(res.error);
      }
      await reload();
    },
    [reload],
  );

  const fetchModels = useCallback(async (id: string): Promise<string[]> => {
    const res = await commands.fetchPostProcessModels(id);
    return res.status === "ok" ? res.data : [];
  }, []);

  const providersWithKeys = useMemo(
    () => new Set(view?.providers_with_keys ?? []),
    [view],
  );
  const models = useMemo<Record<string, string>>(() => {
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(view?.models ?? {})) {
      if (typeof v === "string") out[k] = v;
    }
    return out;
  }, [view]);

  return {
    loading,
    error,
    smartRotation: view?.smart_rotation ?? false,
    providers: view?.providers ?? [],
    selectedProviderId: view?.selected_provider_id ?? "",
    providersWithKeys,
    models,
    reload,
    setSmartRotation,
    setActiveProvider,
    upsertProvider,
    setProviderEnabled,
    removeProvider,
    fetchModels,
  };
};
