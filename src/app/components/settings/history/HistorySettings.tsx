import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import { Check, Copy, FolderOpen, RotateCcw, Star, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  commands,
  events,
  type HistoryEntry,
  type HistoryUpdatePayload,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";
import { formatDateTime } from "@/utils/dateFormat";
import { AudioPlayer, AudioPlayerGroup } from "../../ui/AudioPlayer";
import { Button } from "../../ui/Button";
import { HistoryCard, type HistoryViewMode } from "@/history/HistoryCard";
import {
  hasProcessedText,
  type HistoryController,
} from "@/history/useHistoryController";

const IconButton: React.FC<{
  onClick: () => void;
  title: string;
  disabled?: boolean;
  active?: boolean;
  children: React.ReactNode;
}> = ({ onClick, title, disabled, active, children }) => (
  <button
    onClick={onClick}
    disabled={disabled}
    className={`p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer disabled:cursor-not-allowed disabled:text-text/20 ${
      active
        ? "text-logo-primary hover:text-logo-primary/80"
        : "text-text/50 hover:text-logo-primary"
    }`}
    title={title}
  >
    {children}
  </button>
);

const PAGE_SIZE = 30;

interface OpenRecordingsButtonProps {
  onClick: () => void;
  label: string;
}

const OpenRecordingsButton: React.FC<OpenRecordingsButtonProps> = ({
  onClick,
  label,
}) => (
  <Button
    onClick={onClick}
    variant="secondary"
    size="sm"
    className="flex items-center gap-2"
    title={label}
  >
    <FolderOpen className="w-4 h-4" />
    <span>{label}</span>
  </Button>
);

interface HistorySettingsProps {
  variant?: "settings" | "next";
  controller?: HistoryController;
}

/**
 * [GRAIN] The archive stores no capture mode, so the old Flow/Standard pills
 * could only ever have been guessed from the title. What every entry does
 * carry is whether AI post-processing produced text for it — the same axis the
 * Original / AI processed view switch already reads. Filtering on that is real.
 */
type HistoryFilter = "all" | "today" | "processed" | "unprocessed";

const HISTORY_FILTERS: readonly HistoryFilter[] = [
  "all",
  "today",
  "processed",
  "unprocessed",
] as const;

const PROTOTYPE_HISTORY_COPY = {
  eyebrow: "Local archive",
  title: "History",
  subtitle:
    "Review the text first, compare processing, and return to the recording only when needed.",
  original: "Original",
  processed: "AI processed",
  copied: "Copied",
} as const;

const PrototypeIcon: React.FC<{ name: string }> = ({ name }) => (
  <svg className="icon sm" aria-hidden="true">
    <use href={`#i-${name}`} />
  </svg>
);

export const HistorySettings: React.FC<HistorySettingsProps> = ({
  variant = "settings",
  controller,
}) => {
  const { t } = useTranslation();
  const osType = useOsType();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const entriesRef = useRef<HistoryEntry[]>([]);
  const loadingRef = useRef(false);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<HistoryFilter>("all");
  const [viewMode, setViewMode] = useState<HistoryViewMode>("original");

  const activeEntries = controller?.entries ?? entries;
  const activeLoading = controller?.loading ?? loading;
  const activeLoadError = controller?.loadError ?? loadError;
  const activeHasMore = controller?.hasMore ?? hasMore;

  // Keep ref in sync for use in IntersectionObserver callback
  useEffect(() => {
    entriesRef.current = activeEntries;
  }, [activeEntries]);

  const loadPage = useCallback(async (cursor?: number) => {
    const isFirstPage = cursor === undefined;
    if (!isFirstPage && loadingRef.current) return;
    loadingRef.current = true;

    if (isFirstPage) setLoading(true);
    setLoadError(false);

    try {
      const result = await commands.getHistoryEntries(
        cursor ?? null,
        PAGE_SIZE,
      );
      if (result.status === "ok") {
        const { entries: newEntries, has_more } = result.data;
        setEntries((prev) =>
          isFirstPage ? newEntries : [...prev, ...newEntries],
        );
        setHasMore(has_more);
      } else {
        setLoadError(true);
      }
    } catch (error) {
      console.error("Failed to load history entries:", error);
      setLoadError(true);
    } finally {
      setLoading(false);
      loadingRef.current = false;
    }
  }, []);

  // Initial load
  useEffect(() => {
    if (controller) return;
    loadPage();
  }, [controller, loadPage]);

  // Infinite scroll via IntersectionObserver
  useEffect(() => {
    if (activeLoading) return;

    const sentinel = sentinelRef.current;
    if (!sentinel || !activeHasMore) return;

    const observer = new IntersectionObserver(
      (observerEntries) => {
        const first = observerEntries[0];
        if (first.isIntersecting) {
          if (controller) {
            void controller.loadMore();
            return;
          }
          const lastEntry = entriesRef.current[entriesRef.current.length - 1];
          if (lastEntry) {
            loadPage(lastEntry.id);
          }
        }
      },
      { threshold: 0 },
    );

    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [activeHasMore, activeLoading, controller, loadPage]);

  // Listen for new entries added from the transcription pipeline
  useEffect(() => {
    if (controller) return;
    const unlisten = events.historyUpdatePayload.listen((event) => {
      const payload: HistoryUpdatePayload = event.payload;
      if (payload.action === "added") {
        setEntries((prev) => [payload.entry, ...prev]);
      } else if (payload.action === "updated") {
        setEntries((prev) =>
          prev.map((e) => (e.id === payload.entry.id ? payload.entry : e)),
        );
      }
      // "deleted" and "toggled" are handled by optimistic updates only,
      // so we intentionally ignore them here to avoid double-mutation.
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [controller]);

  const toggleSaved = async (id: number) => {
    // Optimistic update
    setEntries((prev) =>
      prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
    );
    try {
      const result = await commands.toggleHistoryEntrySaved(id);
      if (result.status !== "ok") {
        // Revert on failure
        setEntries((prev) =>
          prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
        );
      }
    } catch (error) {
      console.error("Failed to toggle saved status:", error);
      // Revert on failure
      setEntries((prev) =>
        prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
      );
    }
  };

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch (error) {
      console.error("Failed to copy to clipboard:", error);
    }
  };

  const getAudioUrl = useCallback(
    async (fileName: string) => {
      try {
        const result = await commands.getAudioFilePath(fileName);
        if (result.status === "ok") {
          if (osType === "linux") {
            const fileData = await readFile(result.data);
            const blob = new Blob([fileData], { type: "audio/wav" });
            return URL.createObjectURL(blob);
          }
          return convertFileSrc(result.data, "asset");
        }
        return null;
      } catch (error) {
        console.error("Failed to get audio file path:", error);
        return null;
      }
    },
    [osType],
  );

  const deleteAudioEntry = async (id: number) => {
    // Optimistically remove
    setEntries((prev) => prev.filter((e) => e.id !== id));
    try {
      const result = await commands.deleteHistoryEntry(id);
      if (result.status !== "ok") {
        // Reload on failure
        loadPage();
      }
    } catch (error) {
      console.error("Failed to delete entry:", error);
      loadPage();
    }
  };

  const retryHistoryEntry = async (id: number) => {
    const result = await commands.retryHistoryEntryTranscription(id);
    if (result.status !== "ok") {
      throw new Error(String(result.error));
    }
  };

  const openRecordingsFolder = async () => {
    try {
      const result = await commands.openRecordingsFolder();
      if (result.status !== "ok") {
        throw new Error(String(result.error));
      }
    } catch (error) {
      console.error("Failed to open recordings folder:", error);
    }
  };

  const baseEntries = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    const startOfToday = new Date();
    startOfToday.setHours(0, 0, 0, 0);

    return activeEntries.filter((entry) => {
      const searchableText =
        `${entry.title} ${entry.transcription_text} ${entry.post_processed_text ?? ""}`.toLocaleLowerCase();
      if (normalizedQuery && !searchableText.includes(normalizedQuery)) {
        return false;
      }
      if (filter === "processed") return hasProcessedText(entry);
      if (filter === "unprocessed") return !hasProcessedText(entry);
      if (filter !== "today") return true;

      const timestamp =
        entry.timestamp > 10_000_000_000
          ? entry.timestamp
          : entry.timestamp * 1000;
      return timestamp >= startOfToday.getTime();
    });
  }, [activeEntries, filter, query]);

  // The AI processed view shows ONLY entries the AI actually rewrote. Mixing in
  // raw transcripts as a silent fallback made it impossible to tell which text
  // was processed and which was not, so they are hidden here entirely. When
  // that leaves nothing, the feed says so rather than dropping back to Original.
  const visibleEntries = useMemo(
    () =>
      viewMode === "processed"
        ? baseEntries.filter(hasProcessedText)
        : baseEntries,
    [baseEntries, viewMode],
  );

  if (variant === "next") {
    let nextContent: React.ReactNode;
    if (activeLoading) {
      nextContent = (
        <div className="history-state" role="status">
          {t("settings.history.loading")}
        </div>
      );
    } else if (activeLoadError && activeEntries.length === 0) {
      nextContent = (
        <div className="history-state history-state-error">
          <p>{t("settings.history.loadError")}</p>
          <button
            className="button"
            type="button"
            onClick={() => void (controller ? controller.reload() : loadPage())}
          >
            {t("settings.history.retryLoad")}
          </button>
        </div>
      );
    } else if (visibleEntries.length === 0) {
      nextContent = (
        <div className="history-state">
          {viewMode === "processed"
            ? t("ui2.history.noProcessedInView")
            : t("settings.history.empty")}
        </div>
      );
    } else {
      nextContent = (
        <AudioPlayerGroup>
          {visibleEntries.map((entry) =>
            controller ? (
              <HistoryCard
                key={entry.id}
                entry={entry}
                viewMode={viewMode}
                controller={controller}
              />
            ) : (
              <HistoryEntryComponent
                key={entry.id}
                entry={entry}
                variant="next"
                viewMode={viewMode}
                onToggleSaved={() => toggleSaved(entry.id)}
                copyText={copyToClipboard}
                getAudioUrl={getAudioUrl}
                deleteAudio={deleteAudioEntry}
                retryTranscription={retryHistoryEntry}
              />
            ),
          )}
        </AudioPlayerGroup>
      );
    }

    return (
      <section className="page active" data-page-panel="history">
        <div className="page-wrap history-page-wrap">
          <div className="page-header">
            <div>
              <div className="eyebrow">{PROTOTYPE_HISTORY_COPY.eyebrow}</div>
              <h1>{PROTOTYPE_HISTORY_COPY.title}</h1>
              <p className="page-subtitle">{PROTOTYPE_HISTORY_COPY.subtitle}</p>
            </div>
            <div className="header-actions">
              <button
                className="button"
                type="button"
                onClick={openRecordingsFolder}
              >
                <PrototypeIcon name="folder" />
                {t("settings.history.openFolder")}
              </button>
            </div>
          </div>
          <div className="history-toolbar">
            <label className="history-search">
              <PrototypeIcon name="search" />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search transcription history"
              />
            </label>
            {HISTORY_FILTERS.map((item) => (
              <button
                className={`filter-pill${filter === item ? " active" : ""}`}
                type="button"
                key={item}
                aria-pressed={filter === item}
                onClick={() => setFilter(item)}
              >
                {t(`ui2.history.filters.${item}`)}
              </button>
            ))}
            <span className="toolbar-spacer" />
            <div
              aria-label="History transcription view"
              className="view-switch"
              data-transcript-switch="history"
            >
              <button
                className={viewMode === "original" ? "active" : ""}
                type="button"
                onClick={() => setViewMode("original")}
              >
                {PROTOTYPE_HISTORY_COPY.original}
              </button>
              <button
                className={viewMode === "processed" ? "active" : ""}
                type="button"
                onClick={() => setViewMode("processed")}
              >
                {PROTOTYPE_HISTORY_COPY.processed}
              </button>
            </div>
          </div>
          <div className="transcript-feed history-feed">{nextContent}</div>
          <div ref={sentinelRef} className="history-sentinel" />
        </div>
      </section>
    );
  }

  let content: React.ReactNode;

  if (loading) {
    content = (
      <div className="px-4 py-3 text-center text-ink-soft">
        {t("settings.history.loading")}
      </div>
    );
  } else if (loadError && entries.length === 0) {
    content = (
      <div className="px-4 py-8 flex flex-col items-center gap-3 text-center text-ink-soft">
        <p>{t("settings.history.loadError")}</p>
        <Button variant="secondary" size="sm" onClick={() => loadPage()}>
          {t("settings.history.retryLoad")}
        </Button>
      </div>
    );
  } else if (entries.length === 0) {
    content = (
      <div className="px-4 py-3 text-center text-ink-soft">
        {t("settings.history.empty")}
      </div>
    );
  } else {
    content = (
      <>
        <AudioPlayerGroup>
          <div className="divide-y divide-line">
            {entries.map((entry) => (
              <HistoryEntryComponent
                key={entry.id}
                entry={entry}
                variant="settings"
                viewMode="original"
                onToggleSaved={() => toggleSaved(entry.id)}
                copyText={copyToClipboard}
                getAudioUrl={getAudioUrl}
                deleteAudio={deleteAudioEntry}
                retryTranscription={retryHistoryEntry}
              />
            ))}
          </div>
        </AudioPlayerGroup>
        {/* Sentinel for infinite scroll */}
        <div ref={sentinelRef} className="h-1" />
      </>
    );
  }

  return (
    <div className="max-w-4xl w-full mx-auto space-y-6">
      <div className="space-y-2">
        <div className="px-4 flex items-center justify-between">
          <div>
            <h2 className="text-xs font-medium text-ink-soft uppercase tracking-wide">
              {t("settings.history.title")}
            </h2>
          </div>
          <OpenRecordingsButton
            onClick={openRecordingsFolder}
            label={t("settings.history.openFolder")}
          />
        </div>
        <div className="bg-paper-sunken border border-line rounded-lg overflow-visible">
          {content}
        </div>
      </div>
    </div>
  );
};

interface HistoryEntryProps {
  entry: HistoryEntry;
  variant: "settings" | "next";
  viewMode: HistoryViewMode;
  onToggleSaved: () => void;
  copyText: (text: string) => void;
  getAudioUrl: (fileName: string) => Promise<string | null>;
  deleteAudio: (id: number) => Promise<void>;
  retryTranscription: (id: number) => Promise<void>;
}

const HistoryEntryComponent: React.FC<HistoryEntryProps> = ({
  entry,
  variant,
  viewMode,
  onToggleSaved,
  copyText,
  getAudioUrl,
  deleteAudio,
  retryTranscription,
}) => {
  const { t, i18n } = useTranslation();
  const [showCopied, setShowCopied] = useState(false);
  const [retrying, setRetrying] = useState(false);

  // An entry has a processed version only when the AI returned text. The TRS/PRO
  // toggle appears ONLY then; processed text is shown by default. Re-transcribing
  // clears the processed text (backend), so this collapses back to TRS.
  const hasProcessed = (entry.post_processed_text?.trim().length ?? 0) > 0;
  const [mode, setMode] = useState<"pro" | "trs">(hasProcessed ? "pro" : "trs");
  const showProcessed =
    variant === "next"
      ? viewMode === "processed" && hasProcessed
      : mode === "pro" && hasProcessed;
  const displayText = showProcessed
    ? (entry.post_processed_text ?? "")
    : entry.transcription_text;
  const hasText = displayText.trim().length > 0;

  const handleLoadAudio = useCallback(
    () => getAudioUrl(entry.file_name),
    [getAudioUrl, entry.file_name],
  );

  const handleCopyText = () => {
    if (!hasText) {
      return;
    }

    // Copy whatever is currently shown — processed text in PRO, raw in TRS.
    copyText(displayText);
    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 2000);
  };

  const handleDeleteEntry = async () => {
    try {
      await deleteAudio(entry.id);
    } catch (error) {
      console.error("Failed to delete entry:", error);
      toast.error(t("settings.history.deleteError"));
    }
  };

  const handleRetranscribe = async () => {
    try {
      setRetrying(true);
      await retryTranscription(entry.id);
    } catch (error) {
      console.error("Failed to re-transcribe:", error);
      toast.error(t("settings.history.retranscribeError"));
    } finally {
      setRetrying(false);
    }
  };

  const formattedDate = formatDateTime(String(entry.timestamp), i18n.language);

  if (variant === "next") {
    return (
      <article className="transcript-card">
        <div className="transcript-head">
          <div>
            <time>{formattedDate}</time>
            <span className="capture-mode">
              {entry.title.trim() || "Standard"}
            </span>
          </div>
          <div className="transcript-actions">
            <button
              type="button"
              onClick={handleCopyText}
              disabled={!hasText || retrying}
              title={
                showCopied
                  ? PROTOTYPE_HISTORY_COPY.copied
                  : t("settings.history.copyToClipboard")
              }
            >
              <PrototypeIcon name="copy" />
            </button>
            <button
              type="button"
              className={entry.saved ? "active" : ""}
              onClick={onToggleSaved}
              disabled={retrying}
              title={
                entry.saved
                  ? t("settings.history.unsave")
                  : t("settings.history.save")
              }
            >
              <PrototypeIcon name="star" />
            </button>
            <button
              type="button"
              className={retrying ? "is-retrying" : ""}
              onClick={handleRetranscribe}
              disabled={retrying}
              title={t("settings.history.retranscribe")}
            >
              <PrototypeIcon name="refresh" />
            </button>
            <button
              type="button"
              onClick={handleDeleteEntry}
              disabled={retrying}
              title={t("settings.history.delete")}
            >
              <PrototypeIcon name="trash" />
            </button>
          </div>
        </div>
        <p
          className={`transcript-body${retrying ? " is-retrying" : ""}${hasText ? "" : " is-empty"}`}
        >
          {retrying
            ? t("settings.history.transcribing")
            : hasText
              ? displayText
              : t("settings.history.transcriptionFailed")}
        </p>
        <AudioPlayer
          variant="prototype"
          onLoadRequest={handleLoadAudio}
          className="w-full"
        />
      </article>
    );
  }

  return (
    <div className="px-4 py-2 pb-5 flex flex-col gap-3">
      <div className="flex justify-between items-center">
        <p className="text-sm font-medium">{formattedDate}</p>
        <div className="flex items-center gap-2">
          {/* TRS / PRO toggle — only on entries that have an AI-processed version. */}
          {hasProcessed && !retrying && (
            <div className="inline-flex rounded-md border border-line overflow-hidden text-[0.6rem] font-mono font-semibold">
              <button
                type="button"
                onClick={() => setMode("trs")}
                title={t("settings.history.transcriptionTooltip")}
                className={`px-2 py-0.5 transition-colors ${
                  mode === "trs" ? "bg-ink/15 text-ink" : "text-ink-soft"
                }`}
              >
                {t("settings.history.showTranscription")}
              </button>
              <button
                type="button"
                onClick={() => setMode("pro")}
                title={t("settings.history.processedTooltip")}
                className={`px-2 py-0.5 border-l border-line transition-colors ${
                  mode === "pro" ? "bg-ink/15 text-ink" : "text-ink-soft"
                }`}
              >
                {t("settings.history.showProcessed")}
              </button>
            </div>
          )}
          <div className="flex items-center">
            <IconButton
              onClick={handleCopyText}
              disabled={!hasText || retrying}
              title={t("settings.history.copyToClipboard")}
            >
              {showCopied ? (
                <Check width={16} height={16} />
              ) : (
                <Copy width={16} height={16} />
              )}
            </IconButton>
            <IconButton
              onClick={onToggleSaved}
              disabled={retrying}
              active={entry.saved}
              title={
                entry.saved
                  ? t("settings.history.unsave")
                  : t("settings.history.save")
              }
            >
              <Star
                width={16}
                height={16}
                fill={entry.saved ? "currentColor" : "none"}
              />
            </IconButton>
            <IconButton
              onClick={handleRetranscribe}
              disabled={retrying}
              title={t("settings.history.retranscribe")}
            >
              <RotateCcw
                width={16}
                height={16}
                style={
                  retrying
                    ? { animation: "spin 1s linear infinite reverse" }
                    : undefined
                }
              />
            </IconButton>
            <IconButton
              onClick={handleDeleteEntry}
              disabled={retrying}
              title={t("settings.history.delete")}
            >
              <Trash2 width={16} height={16} />
            </IconButton>
          </div>
        </div>
      </div>

      <p
        className={`italic text-sm pb-2 ${
          retrying
            ? ""
            : hasText
              ? "text-ink/90 select-text cursor-text whitespace-pre-wrap break-words"
              : "text-ink/40"
        }`}
        style={
          retrying
            ? { animation: "transcribe-pulse 3s ease-in-out infinite" }
            : undefined
        }
      >
        {retrying && (
          <style>{`
            @keyframes transcribe-pulse {
              0%, 100% { color: color-mix(in srgb, var(--color-text) 40%, transparent); }
              50% { color: color-mix(in srgb, var(--color-text) 90%, transparent); }
            }
          `}</style>
        )}
        {retrying
          ? t("settings.history.transcribing")
          : hasText
            ? displayText
            : t("settings.history.transcriptionFailed")}
      </p>

      <AudioPlayer onLoadRequest={handleLoadAudio} className="w-full" />
    </div>
  );
};
