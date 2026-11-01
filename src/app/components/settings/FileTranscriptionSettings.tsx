import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { commands, events, type TranscriptionJob } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { SettingContainer } from "@/components/ui/SettingContainer";

export function FileTranscriptionSettings() {
  const { t } = useTranslation();
  const [reading, setReading] = useState(false);
  const [error, setError] = useState("");
  const [jobs, setJobs] = useState<Record<number, TranscriptionJob>>({});
  const [names, setNames] = useState<Record<number, string>>({});
  const [cancelling, setCancelling] = useState<Set<number>>(new Set());
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const buffered: TranscriptionJob[] = [];
    let initialized = false;
    const apply = (job: TranscriptionJob) => {
      if (!disposed)
        setJobs((current) => ({ ...current, [job.entry_id]: job }));
    };
    void (async () => {
      // Subscribe first; events received during the snapshot win over it.
      const stop = await events.transcriptionJob.listen(({ payload }) => {
        if (initialized) apply(payload);
        else buffered.push(payload);
      });
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
      const snapshot = await commands.getTranscriptionJobs();
      snapshot.forEach(apply);
      buffered.forEach(apply);
      initialized = true;
    })().catch((reason) => {
      if (!disposed) setError(String(reason));
    });
    return () => {
      disposed = true;
      mounted.current = false;
      unlisten?.();
    };
  }, []);

  const chooseFile = async () => {
    setError("");
    setReading(true);
    try {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: t("fileImport.audioFiles", "Audio files"),
            extensions: [
              "wav",
              "mp3",
              "m4a",
              "aac",
              "flac",
              "aif",
              "aiff",
              "ogg",
              "mp4",
              "m4v",
              "mov",
            ],
          },
        ],
      });
      if (!path || Array.isArray(path)) return;
      const result = await commands.transcribeFile(path);
      if (result.status === "error") throw new Error(result.error);
      if (!mounted.current) return;
      const entry = result.data;
      setNames((current) => ({
        ...current,
        [entry.id]: entry.source_name ?? entry.file_name,
      }));
      setJobs((current) => ({
        ...current,
        [entry.id]: current[entry.id] ?? {
          entry_id: entry.id,
          status: { state: "queued" },
        },
      }));
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      if (mounted.current) setReading(false);
    }
  };

  const cancel = async (id: number) => {
    setCancelling((current) => new Set(current).add(id));
    try {
      await commands.cancelHistoryEntryTranscription(id);
    } catch (reason) {
      if (mounted.current) {
        setError(String(reason));
        setCancelling((current) => {
          const next = new Set(current);
          next.delete(id);
          return next;
        });
      }
    }
  };

  return (
    <div>
      <SettingContainer
        title={t("fileImport.title", "Transcribe an audio file")}
        description={t(
          "fileImport.description",
          "Use your Standard transcription model to save a file’s transcript in History. Up to 60 minutes per file; no text is pasted into other apps.",
        )}
        descriptionMode="inline"
        grouped
        layout="horizontal"
      >
        <Button
          variant="secondary"
          size="sm"
          disabled={reading}
          onClick={() => void chooseFile()}
        >
          {reading
            ? t("fileImport.reading", "Reading file…")
            : t("fileImport.choose", "Choose file…")}
        </Button>
      </SettingContainer>
      {error && (
        <p role="alert" className="px-4 pb-3 text-sm text-status-error">
          {error}
        </p>
      )}
      <div aria-live="polite" className="px-4">
        {Object.values(jobs).map(({ entry_id: id, status }) => {
          const active =
            status.state === "queued" || status.state === "running";
          const label =
            active && cancelling.has(id)
              ? t("fileImport.stopping", "Stopping after the current chunk…")
              : status.state === "cancelled"
                ? t("fileImport.cancelled", "Cancelled")
                : status.state === "running"
                  ? t(
                      "fileImport.progress",
                      "Transcribing {{percent}}% — pauses during dictation",
                      { percent: status.percent },
                    )
                  : status.state === "queued"
                    ? t("fileImport.queued", "Queued")
                    : status.state === "failed"
                      ? status.error
                      : t("fileImport.finished", "Finished — check History");
          return (
            <div
              key={id}
              className="flex items-center justify-between gap-3 pb-3 text-sm"
            >
              <div className="min-w-0">
                <p className="truncate text-ink">
                  {names[id] ??
                    t("fileImport.entry", "History entry {{id}}", { id })}
                </p>
                <p
                  className={
                    status.state === "failed"
                      ? "text-status-error"
                      : "text-ink-soft"
                  }
                >
                  {label}
                </p>
              </div>
              <Button
                size="sm"
                variant="ghost"
                disabled={active && cancelling.has(id)}
                onClick={() => {
                  if (active) void cancel(id);
                  else {
                    setJobs((current) => {
                      const next = { ...current };
                      delete next[id];
                      return next;
                    });
                    setNames((current) => {
                      const next = { ...current };
                      delete next[id];
                      return next;
                    });
                    setCancelling((current) => {
                      const next = new Set(current);
                      next.delete(id);
                      return next;
                    });
                  }
                }}
              >
                {active
                  ? t("fileImport.cancel", "Cancel")
                  : t("fileImport.dismiss", "Dismiss")}
              </Button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
