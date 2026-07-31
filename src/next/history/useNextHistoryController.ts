import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  commands,
  events,
  type HistoryEntry,
  type HistoryUpdatePayload,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";

const INITIAL_PAGE_SIZE = 3;
const HISTORY_PAGE_SIZE = 30;

export function reduceNextHistoryEntries(
  entries: HistoryEntry[],
  payload: HistoryUpdatePayload,
): HistoryEntry[] {
  switch (payload.action) {
    case "added":
      return [
        payload.entry,
        ...entries.filter((entry) => entry.id !== payload.entry.id),
      ];
    case "updated": {
      const found = entries.some((entry) => entry.id === payload.entry.id);
      return found
        ? entries.map((entry) =>
            entry.id === payload.entry.id ? payload.entry : entry,
          )
        : entries;
    }
    case "deleted":
    case "toggled":
      return entries;
  }
}

export interface NextHistoryController {
  entries: HistoryEntry[];
  loading: boolean;
  loadingMore: boolean;
  loadError: boolean;
  hasMore: boolean;
  reload: () => Promise<void>;
  loadMore: () => Promise<void>;
  toggleSaved: (id: number) => Promise<void>;
  deleteEntry: (id: number) => Promise<void>;
  retryEntry: (id: number) => Promise<void>;
  copyText: (text: string) => Promise<void>;
  getAudioUrl: (fileName: string) => Promise<string | null>;
}

export function useNextHistoryController(): NextHistoryController {
  const osType = useOsType();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const entriesRef = useRef<HistoryEntry[]>([]);
  const mountedRef = useRef(false);
  const requestRef = useRef(false);

  useEffect(() => {
    entriesRef.current = entries;
  }, [entries]);

  const fetchPage = useCallback(
    async (cursor: number | null, limit: number) => {
      if (requestRef.current) return;
      requestRef.current = true;
      const first = cursor == null;
      if (first) setLoading(true);
      else setLoadingMore(true);
      setLoadError(false);
      try {
        const result = await commands.getHistoryEntries(cursor, limit);
        if (result.status !== "ok") throw new Error(result.error);
        if (!mountedRef.current) return;
        setEntries((current) =>
          first ? result.data.entries : [...current, ...result.data.entries],
        );
        setHasMore(result.data.has_more);
      } catch (error) {
        console.error("Failed to load UI 2.0 history:", error);
        if (mountedRef.current) setLoadError(true);
      } finally {
        requestRef.current = false;
        if (mountedRef.current) {
          setLoading(false);
          setLoadingMore(false);
        }
      }
    },
    [],
  );

  const reload = useCallback(
    () => fetchPage(null, INITIAL_PAGE_SIZE),
    [fetchPage],
  );

  const loadMore = useCallback(async () => {
    if (requestRef.current || !hasMore) return;
    const last = entriesRef.current[entriesRef.current.length - 1];
    if (!last) {
      await reload();
      return;
    }
    await fetchPage(last.id, HISTORY_PAGE_SIZE);
  }, [fetchPage, hasMore, reload]);

  useEffect(() => {
    mountedRef.current = true;
    const unlisten = events.historyUpdatePayload.listen((event) => {
      const payload = event.payload;
      if (payload.action === "added" || payload.action === "updated") {
        setEntries((current) => reduceNextHistoryEntries(current, payload));
      }
    });
    void reload();
    return () => {
      mountedRef.current = false;
      void unlisten.then((cleanup) => cleanup());
    };
  }, [reload]);

  const toggleSaved = useCallback(async (id: number) => {
    setEntries((current) =>
      current.map((entry) =>
        entry.id === id ? { ...entry, saved: !entry.saved } : entry,
      ),
    );
    try {
      const result = await commands.toggleHistoryEntrySaved(id);
      if (result.status !== "ok") throw new Error(result.error);
    } catch (error) {
      console.error("Failed to toggle saved history entry:", error);
      if (mountedRef.current) {
        setEntries((current) =>
          current.map((entry) =>
            entry.id === id ? { ...entry, saved: !entry.saved } : entry,
          ),
        );
      }
    }
  }, []);

  const deleteEntry = useCallback(async (id: number) => {
    const index = entriesRef.current.findIndex((entry) => entry.id === id);
    const removed = index >= 0 ? entriesRef.current[index] : undefined;
    setEntries((current) => current.filter((entry) => entry.id !== id));
    try {
      const result = await commands.deleteHistoryEntry(id);
      if (result.status !== "ok") throw new Error(result.error);
    } catch (error) {
      console.error("Failed to delete history entry:", error);
      if (mountedRef.current && removed) {
        setEntries((current) => {
          const restored = [...current];
          restored.splice(Math.min(index, restored.length), 0, removed);
          return restored;
        });
      }
      throw error;
    }
  }, []);

  const retryEntry = useCallback(async (id: number) => {
    const result = await commands.retryHistoryEntryTranscription(id);
    if (result.status !== "ok") throw new Error(result.error);
  }, []);

  const copyText = useCallback(async (text: string) => {
    await navigator.clipboard.writeText(text);
  }, []);

  const getAudioUrl = useCallback(
    async (fileName: string) => {
      try {
        const result = await commands.getAudioFilePath(fileName);
        if (result.status !== "ok") return null;
        if (osType === "linux") {
          const fileData = await readFile(result.data);
          return URL.createObjectURL(
            new Blob([fileData], { type: "audio/wav" }),
          );
        }
        return convertFileSrc(result.data, "asset");
      } catch (error) {
        console.error("Failed to load history audio:", error);
        return null;
      }
    },
    [osType],
  );

  return {
    entries,
    loading,
    loadingMore,
    loadError,
    hasMore,
    reload,
    loadMore,
    toggleSaved,
    deleteEntry,
    retryEntry,
    copyText,
    getAudioUrl,
  };
}
