import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { HistoryEntry } from "@/bindings";
import { AudioPlayer } from "@/components/ui/AudioPlayer";
import { formatDateTime } from "@/utils/dateFormat";
import type { NextHistoryController } from "./useNextHistoryController";

export type NextHistoryViewMode = "original" | "processed";

function PrototypeIcon({ name }: { name: string }) {
  return (
    <svg className="icon sm" aria-hidden="true">
      <use href={`#i-${name}`} />
    </svg>
  );
}

export function NextHistoryCard({
  entry,
  viewMode,
  controller,
}: {
  entry: HistoryEntry;
  viewMode: NextHistoryViewMode;
  controller: NextHistoryController;
}) {
  const { t, i18n } = useTranslation();
  const [copied, setCopied] = useState(false);
  const [retrying, setRetrying] = useState(false);
  const copiedTimer = useRef<number | undefined>(undefined);
  const hasProcessed = (entry.post_processed_text?.trim().length ?? 0) > 0;
  const displayText =
    viewMode === "processed" && hasProcessed
      ? (entry.post_processed_text ?? "")
      : entry.transcription_text;
  const hasText = displayText.trim().length > 0;

  useEffect(
    () => () => {
      window.clearTimeout(copiedTimer.current);
    },
    [],
  );

  const copy = async () => {
    if (!hasText) return;
    try {
      await controller.copyText(displayText);
      setCopied(true);
      window.clearTimeout(copiedTimer.current);
      copiedTimer.current = window.setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      console.error("Failed to copy history text:", error);
    }
  };

  const retry = async () => {
    setRetrying(true);
    try {
      await controller.retryEntry(entry.id);
    } catch (error) {
      console.error("Failed to re-transcribe history entry:", error);
      toast.error(t("settings.history.retranscribeError"));
    } finally {
      setRetrying(false);
    }
  };

  const remove = async () => {
    try {
      await controller.deleteEntry(entry.id);
    } catch {
      toast.error(t("settings.history.deleteError"));
    }
  };

  const loadAudio = useCallback(
    () => controller.getAudioUrl(entry.file_name),
    [controller, entry.file_name],
  );

  return (
    <article className="transcript-card">
      <div className="transcript-head">
        <div>
          <time>{formatDateTime(String(entry.timestamp), i18n.language)}</time>
          <span className="capture-mode">
            {entry.title.trim() || "Standard"}
          </span>
        </div>
        <div className="transcript-actions">
          <button
            type="button"
            onClick={() => void copy()}
            disabled={!hasText || retrying}
            title={copied ? "Copied" : t("settings.history.copyToClipboard")}
          >
            <PrototypeIcon name="copy" />
          </button>
          <button
            type="button"
            className={entry.saved ? "active" : ""}
            onClick={() => void controller.toggleSaved(entry.id)}
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
            onClick={() => void retry()}
            disabled={retrying}
            title={t("settings.history.retranscribe")}
          >
            <PrototypeIcon name="refresh" />
          </button>
          <button
            type="button"
            onClick={() => void remove()}
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
        onLoadRequest={loadAudio}
        className="w-full"
      />
    </article>
  );
}
