import { useCallback, useEffect, useMemo, useState } from "react";
import {
  commands,
  type PostProcessProvider,
  type PpPoolView,
} from "@/bindings";
import { usePpPoolStore } from "@/stores/ppPoolStore";

/** Prefix this UI stamps on every provider IT creates (see `PpAddProvider`).
 *  Nothing the backend seeds carries it. */
const USER_ADDED_PREFIX = "pp_";

/** Whether a provider is one the backend seeded, rather than one the user added.
 *
 *  Built-ins can't be removed (only their key/model/quota edited) — the backend
 *  re-seeds them anyway — and they are the templates the "add provider" picker
 *  offers. User-added entries (multi-key duplicates, custom endpoints) ARE
 *  removable.
 *
 *  This used to be a hardcoded id list here in the frontend, which meant every
 *  provider added to `default_post_process_providers()` in Rust was invisible in
 *  the picker until someone remembered to type its id here a second time —
 *  exactly what happened to Gemini. The id shape is a rule this file already
 *  owns, so asking it cannot go stale. */
export const isBuiltinPpId = (id: string): boolean =>
  !id.startsWith(USER_ADDED_PREFIX);

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
 * [GRAIN] usePpPool — delegates to the singleton ppPoolStore so the settings
 * panel and quick panel share one live view. The hook signature is preserved
 * for backward compatibility with all existing settings components.
 */
export const usePpPool = (): PpPoolState => {
  return usePpPoolStore();
};
