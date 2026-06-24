import { useCallback, useEffect, useMemo, useState } from "react";
import { commands, type SttProvider, type SttPoolView } from "@/bindings";

const LOCAL_PROVIDER_ID = "local";

export type SttPoolState = {
  loading: boolean;
  error: string | null;
  smartRotation: boolean;
  /** All providers, local first. */
  providers: SttProvider[];
  /** Cloud-only providers (kind !== "local"), the rotatable set. */
  cloudProviders: SttProvider[];
  /** The implicit always-present local provider, if present. */
  localProvider: SttProvider | undefined;
  /** Set of provider ids that currently have a non-empty key stored. */
  providersWithKeys: Set<string>;
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
};

/**
 * Drives the cloud STT routing pool entirely through the dedicated `stt_*`
 * commands (never `getAppSettings`), so API keys stay backend-only — the view
 * only ever learns *which* ids have a key, not the key itself.
 */
export const useSttPool = (): SttPoolState => {
  const [view, setView] = useState<SttPoolView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    const res = await commands.sttGetPool();
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
      const res = await commands.sttSetSmartRotation(enabled);
      if (res.status === "error") {
        setError(res.error);
        return;
      }
      await reload();
    },
    [reload],
  );

  const upsertProvider = useCallback(
    async (provider: SttProvider, apiKey: string | null) => {
      const res = await commands.sttUpsertProvider(provider, apiKey);
      if (res.status === "error") {
        setError(res.error);
        throw new Error(res.error);
      }
      await reload();
    },
    [reload],
  );

  const setProviderEnabled = useCallback(
    async (provider: SttProvider, enabled: boolean) => {
      // Pass null for the key so the stored secret is left untouched.
      await upsertProvider({ ...provider, enabled }, null);
    },
    [upsertProvider],
  );

  const removeProvider = useCallback(
    async (id: string) => {
      const res = await commands.sttRemoveProvider(id);
      if (res.status === "error") {
        setError(res.error);
        throw new Error(res.error);
      }
      await reload();
    },
    [reload],
  );

  const providers = useMemo(() => view?.providers ?? [], [view]);
  const cloudProviders = useMemo(
    () => providers.filter((p) => p.kind !== "local"),
    [providers],
  );
  const localProvider = useMemo(
    () => providers.find((p) => p.id === LOCAL_PROVIDER_ID),
    [providers],
  );
  const providersWithKeys = useMemo(
    () => new Set(view?.providers_with_keys ?? []),
    [view],
  );

  return {
    loading,
    error,
    smartRotation: view?.smart_rotation ?? false,
    providers,
    cloudProviders,
    localProvider,
    providersWithKeys,
    reload,
    setSmartRotation,
    upsertProvider,
    setProviderEnabled,
    removeProvider,
  };
};
