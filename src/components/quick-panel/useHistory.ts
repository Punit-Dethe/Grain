import { useEffect, useState } from "react";
import {
  commands,
  events,
  type HistoryUpdatePayload,
  type HistoryEntry as DbHistoryEntry,
} from "@/bindings";
import type { HistoryEntry } from "./widgets";

const fmtTime = (ts: number): string => {
  // History timestamps are unix seconds; tolerate ms just in case.
  const d = new Date(ts > 1e12 ? ts : ts * 1000);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
};

/**
 * Live transcription history for the Quick Panel. Loads the most recent entries
 * and stays current via the `historyUpdatePayload` event (shared with the main
 * History tab). Maps to the panel's `{ id, time, text }` shape — text is the
 * raw transcription; the row collapses whitespace + ellipsises for display.
 */
export const useTranscriptionHistory = (limit = 40): HistoryEntry[] => {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);

  useEffect(() => {
    let active = true;
    commands.getHistoryEntries(null, limit).then((res) => {
      if (!active) return;
      if (res.status === "ok") {
        setEntries(
          res.data.entries.map((e) => ({
            id: e.id,
            time: fmtTime(e.timestamp),
            text: e.transcription_text || e.title || "",
          })),
        );
      }
    });
    return () => {
      active = false;
    };
  }, [limit]);

  useEffect(() => {
    const unlisten = events.historyUpdatePayload.listen((event) => {
      const payload: HistoryUpdatePayload = event.payload;
      if (payload.action === "added") {
        const e = payload.entry;
        setEntries((prev) => [
          { id: e.id, time: fmtTime(e.timestamp), text: e.transcription_text || e.title || "" },
          ...prev,
        ]);
      } else if (payload.action === "updated") {
        const e = payload.entry;
        setEntries((prev) =>
          prev.map((p) =>
            p.id === e.id
              ? { id: e.id, time: fmtTime(e.timestamp), text: e.transcription_text || e.title || "" }
              : p,
          ),
        );
      } else if (payload.action === "deleted") {
        setEntries((prev) => prev.filter((p) => p.id !== payload.id));
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return entries;
};

/**
 * Live PROCESSING history for the Quick Panel's Module C — only entries the AI has
 * processed (their `post_processed_text`). A plain transcription never appears
 * here; if an entry's processing is later cleared (e.g. re-transcribed), it drops
 * out. Same `{ id, time, text }` shape, text = the processed output.
 */
export const useProcessingHistory = (limit = 40): HistoryEntry[] => {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);

  const isProcessed = (e: DbHistoryEntry): boolean =>
    (e.post_processed_text?.trim().length ?? 0) > 0;
  const toRow = (e: DbHistoryEntry): HistoryEntry => ({
    id: e.id,
    time: fmtTime(e.timestamp),
    text: e.post_processed_text ?? "",
  });

  useEffect(() => {
    let active = true;
    commands.getHistoryEntries(null, limit).then((res) => {
      if (!active) return;
      if (res.status === "ok") {
        setEntries(res.data.entries.filter(isProcessed).map(toRow));
      }
    });
    return () => {
      active = false;
    };
  }, [limit]);

  useEffect(() => {
    const unlisten = events.historyUpdatePayload.listen((event) => {
      const payload: HistoryUpdatePayload = event.payload;
      if (payload.action === "added") {
        if (isProcessed(payload.entry)) {
          setEntries((prev) => [toRow(payload.entry), ...prev]);
        }
      } else if (payload.action === "updated") {
        const e = payload.entry;
        setEntries((prev) => {
          const exists = prev.some((p) => p.id === e.id);
          if (isProcessed(e)) {
            return exists
              ? prev.map((p) => (p.id === e.id ? toRow(e) : p))
              : [toRow(e), ...prev];
          }
          // Processing was cleared (e.g. re-transcribe) — remove it from Module C.
          return exists ? prev.filter((p) => p.id !== e.id) : prev;
        });
      } else if (payload.action === "deleted") {
        setEntries((prev) => prev.filter((p) => p.id !== payload.id));
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return entries;
};
