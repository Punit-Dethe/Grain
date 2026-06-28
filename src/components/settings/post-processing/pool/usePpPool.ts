import { useCallback, useEffect, useMemo, useState } from "react";
import {
  commands,
  type PostProcessProvider,
  type PpPoolView,
} from "@/bindings";
import { usePpPoolStore } from "@/stores/ppPoolStore";

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
 * [GRAIN] usePpPool — delegates to the singleton ppPoolStore so the settings
 * panel and quick panel share one live view. The hook signature is preserved
 * for backward compatibility with all existing settings components.
 */
export const usePpPool = (): PpPoolState => {
  return usePpPoolStore();
};
