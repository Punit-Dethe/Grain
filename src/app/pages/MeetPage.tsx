import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import type {
  MeetingSummary,
  TrackStatus,
  TranscriptRevision,
} from "@/bindings";
import { formatDateTime } from "@/utils/dateFormat";
import {
  buildTurns,
  filterTurns,
  formatClock,
  speakerLabels,
  transcriptText,
  type Turn,
} from "../meet/transcriptTurns";
import {
  useMeetController,
  type MeetController,
} from "../meet/useMeetController";

/** Speaker accents, cycled in label order. */
const SPEAKER_TONES = ["peach", "sage", "lilac", "sky"] as const;

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function trackStatusNote(
  status: TrackStatus,
  t: (key: string, options?: Record<string, unknown>) => string,
): string | null {
  if (status === "Complete") return null;
  if (status === "Empty") return t("ui2.meet.status.empty");
  if ("SpeakersUnavailable" in status) {
    return t("ui2.meet.status.speakersUnavailable");
  }
  if ("TooLong" in status) {
    return t("ui2.meet.status.tooLong", {
      minutes: status.TooLong.minutes,
      limit: status.TooLong.limit_minutes,
    });
  }
  return t("ui2.meet.status.failed", { reason: status.Failed.reason });
}

function Recorder({ meet }: { meet: MeetController }) {
  const { t } = useTranslation();
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const recording = meet.status.recording;

  const start = async () => {
    setBusy(true);
    try {
      await meet.startRecording(title.trim() || null);
      setTitle("");
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    setBusy(true);
    try {
      await meet.stopRecording();
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  if (recording) {
    return (
      <div className="meet-recorder is-recording" role="status">
        <span className="meet-rec-dot" aria-hidden="true" />
        <div className="meet-recorder-copy">
          <strong>{t("ui2.meet.recorder.recording")}</strong>
          <span className="meet-clock">
            {formatClock(recording.elapsed_ms)}
          </span>
          {!recording.source_active && (
            <span className="meet-warning">
              {t("ui2.meet.recorder.noAudio")}
            </span>
          )}
          {recording.error && (
            <span className="meet-warning">{recording.error}</span>
          )}
        </div>
        <button
          className="button danger"
          type="button"
          disabled={busy}
          onClick={stop}
        >
          {t("ui2.meet.recorder.stop")}
        </button>
      </div>
    );
  }

  return (
    <div className="meet-recorder">
      <label className="history-search meet-title-input">
        <input
          value={title}
          maxLength={120}
          onChange={(event) => setTitle(event.target.value)}
          placeholder={t("ui2.meet.recorder.titlePlaceholder")}
          aria-label={t("ui2.meet.recorder.titleLabel")}
        />
      </label>
      <button
        className="button primary"
        type="button"
        disabled={busy}
        onClick={start}
      >
        {t("ui2.meet.recorder.start")}
      </button>
    </div>
  );
}

function SpeakerModelNotice({ meet }: { meet: MeetController }) {
  const { t } = useTranslation();
  if (meet.diarizerInstalled !== false) return null;
  const downloading = meet.diarizerProgress !== null;
  const download = async () => {
    try {
      await meet.downloadDiarizer();
      toast.success(t("ui2.meet.speakerModel.ready"));
    } catch (error) {
      toast.error(errorText(error));
    }
  };
  return (
    <div className="meet-notice">
      <div>
        <strong>{t("ui2.meet.speakerModel.title")}</strong>
        <p>{t("ui2.meet.speakerModel.body")}</p>
      </div>
      <button
        className="button"
        type="button"
        disabled={downloading}
        onClick={download}
      >
        {downloading
          ? t("ui2.meet.speakerModel.downloading", {
              percent: Math.round((meet.diarizerProgress ?? 0) * 100),
            })
          : t("ui2.meet.speakerModel.download")}
      </button>
    </div>
  );
}

function MeetingList({
  meet,
  selected,
  onSelect,
}: {
  meet: MeetController;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const { t, i18n } = useTranslation();
  if (meet.loading) {
    return <div className="history-state">{t("ui2.meet.list.loading")}</div>;
  }
  if (meet.loadError) {
    return (
      <div className="history-state history-state-error">
        <p>{t("ui2.meet.list.loadError")}</p>
        <button className="button" type="button" onClick={meet.reload}>
          {t("ui2.meet.list.retry")}
        </button>
      </div>
    );
  }
  if (meet.meetings.length === 0) {
    return <div className="history-state">{t("ui2.meet.list.empty")}</div>;
  }
  const processingId = meet.status.processing?.session_id;
  const recordingId = meet.status.recording?.session_id;
  return (
    <div className="transcript-feed meet-list">
      {meet.meetings.map((meeting) => {
        const state =
          meeting.id === recordingId
            ? t("ui2.meet.list.recording")
            : meeting.id === processingId
              ? t("ui2.meet.list.processing")
              : meeting.transcript_revision != null
                ? t("ui2.meet.list.transcribed")
                : t("ui2.meet.list.notProcessed");
        return (
          <button
            key={meeting.id}
            type="button"
            className={`transcript-card meet-list-item${selected === meeting.id ? " active" : ""}`}
            aria-pressed={selected === meeting.id}
            onClick={() => onSelect(meeting.id)}
          >
            <strong>{meeting.title}</strong>
            <span className="meet-meta">
              {formatDateTime(
                String(Math.floor(meeting.created_unix_ms / 1_000)),
                i18n.language,
              )}
            </span>
            <span className="meet-meta">
              {formatClock(meeting.duration_ms)}
              <span className="capture-mode">{state}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

function TurnRow({
  turn,
  label,
  tone,
  onSeek,
}: {
  turn: Turn;
  label: string;
  tone: string | null;
  onSeek: (ms: number) => void;
}) {
  return (
    <div className="meet-turn">
      <div className="meet-turn-head">
        <span className={`meet-speaker${tone ? ` tone-${tone}` : ""}`}>
          {label}
        </span>
        <button
          type="button"
          className="text-button meet-time"
          onClick={() => onSeek(turn.startMs)}
        >
          {formatClock(turn.startMs)}
        </button>
      </div>
      <p className="transcript-body">{turn.text}</p>
    </div>
  );
}

function MeetingDetail({
  meet,
  meeting,
  onDeleted,
}: {
  meet: MeetController;
  meeting: MeetingSummary;
  onDeleted: () => void;
}) {
  const { t } = useTranslation();
  const [transcript, setTranscript] = useState<TranscriptRevision | null>(null);
  const [transcriptError, setTranscriptError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [audioSrc, setAudioSrc] = useState<string | null>(null);
  const [audioError, setAudioError] = useState<string | null>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const processing = meet.status.processing;
  const isProcessing = processing?.session_id === meeting.id;
  const isRecording = meet.status.recording?.session_id === meeting.id;

  useEffect(() => {
    let active = true;
    setTranscript(null);
    setTranscriptError(null);
    if (meeting.transcript_revision == null) return;
    meet
      .loadTranscript(meeting.id)
      .then((revision) => active && setTranscript(revision))
      .catch((error) => active && setTranscriptError(errorText(error)));
    return () => {
      active = false;
    };
  }, [meet, meeting.id, meeting.transcript_revision]);

  // Release the previous audio source when the meeting changes.
  useEffect(() => {
    setAudioSrc(null);
    setAudioError(null);
  }, [meeting.id]);
  useEffect(
    () => () => {
      if (audioSrc?.startsWith("blob:")) URL.revokeObjectURL(audioSrc);
    },
    [audioSrc],
  );

  const turns = useMemo(
    () => (transcript ? buildTurns(transcript) : []),
    [transcript],
  );
  const labels = useMemo(
    () => speakerLabels(transcript?.speakers ?? []),
    [transcript],
  );
  const tones = useMemo(
    () =>
      new Map(
        (transcript?.speakers ?? []).map((speaker, index) => [
          speaker.id,
          SPEAKER_TONES[index % SPEAKER_TONES.length],
        ]),
      ),
    [transcript],
  );
  const visible = useMemo(() => filterTurns(turns, query), [turns, query]);

  const loadAudio = useCallback(async () => {
    try {
      const src = await meet.loadPlayback(meeting.id, 0);
      setAudioSrc(src);
      return src;
    } catch (error) {
      setAudioError(errorText(error));
      return null;
    }
  }, [meet, meeting.id]);

  const seek = useCallback(
    async (ms: number) => {
      if (!audioSrc && !(await loadAudio())) return;
      // Wait one frame so a freshly loaded source is attached.
      requestAnimationFrame(() => {
        const audio = audioRef.current;
        if (!audio) return;
        audio.currentTime = ms / 1_000;
        void audio.play().catch(() => undefined);
      });
    },
    [audioSrc, loadAudio],
  );

  const runProcess = async () => {
    try {
      await meet.process(meeting.id);
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const cancel = async () => {
    try {
      await meet.cancelProcessing();
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(
        transcriptText(turns, labels, t("ui2.meet.detail.unknownSpeaker")),
      );
      toast.success(t("ui2.meet.detail.copied"));
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const remove = async () => {
    const confirmed = await ask(t("ui2.meet.detail.deleteConfirm"), {
      kind: "warning",
    });
    if (!confirmed) return;
    try {
      await meet.deleteMeeting(meeting.id);
      onDeleted();
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const notes = (transcript?.tracks ?? [])
    .map((track) => trackStatusNote(track.status, t))
    .filter((note): note is string => note !== null);

  return (
    <div className="meet-detail">
      <div className="meet-detail-head">
        <div>
          <h2>{meeting.title}</h2>
          <span className="meet-meta">{formatClock(meeting.duration_ms)}</span>
        </div>
        <div className="header-actions">
          {isProcessing ? (
            <button className="button" type="button" onClick={cancel}>
              {t("ui2.meet.detail.cancel")}
            </button>
          ) : (
            <button
              className="button primary"
              type="button"
              disabled={isRecording || processing != null}
              onClick={runProcess}
            >
              {meeting.transcript_revision == null
                ? t("ui2.meet.detail.process")
                : t("ui2.meet.detail.reprocess")}
            </button>
          )}
          <button
            className="button"
            type="button"
            disabled={turns.length === 0}
            onClick={copy}
          >
            {t("ui2.meet.detail.copy")}
          </button>
          <button
            className="button danger"
            type="button"
            disabled={isRecording || isProcessing}
            onClick={remove}
          >
            {t("ui2.meet.detail.delete")}
          </button>
        </div>
      </div>

      {isProcessing && processing && (
        <div className="meet-notice" role="status">
          <strong>{t(`ui2.meet.stage.${processing.stage}`)}</strong>
        </div>
      )}

      {!isRecording && (
        <div className="meet-player">
          {audioSrc ? (
            <audio ref={audioRef} src={audioSrc} controls preload="metadata" />
          ) : (
            <button className="button" type="button" onClick={loadAudio}>
              {t("ui2.meet.detail.loadAudio")}
            </button>
          )}
          {audioError && <span className="meet-warning">{audioError}</span>}
        </div>
      )}

      {notes.map((note) => (
        <p key={note} className="meet-warning">
          {note}
        </p>
      ))}
      {transcriptError && <p className="meet-warning">{transcriptError}</p>}

      {transcript ? (
        <>
          <label className="history-search meet-search">
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("ui2.meet.detail.search")}
              aria-label={t("ui2.meet.detail.search")}
            />
          </label>
          <div className="meet-transcript">
            {visible.length === 0 ? (
              <p className="history-state">{t("ui2.meet.detail.noMatches")}</p>
            ) : (
              visible.map((turn) => (
                <TurnRow
                  key={turn.key}
                  turn={turn}
                  label={
                    turn.speakerId
                      ? (labels.get(turn.speakerId) ??
                        t("ui2.meet.detail.unknownSpeaker"))
                      : turn.ambiguous
                        ? t("ui2.meet.detail.overlap")
                        : t("ui2.meet.detail.unknownSpeaker")
                  }
                  tone={
                    turn.speakerId ? (tones.get(turn.speakerId) ?? null) : null
                  }
                  onSeek={seek}
                />
              ))
            )}
          </div>
        </>
      ) : (
        !isProcessing &&
        meeting.transcript_revision == null && (
          <p className="history-state">{t("ui2.meet.detail.noTranscript")}</p>
        )
      )}
    </div>
  );
}

export function MeetPage() {
  const { t } = useTranslation();
  const meet = useMeetController();
  const [selected, setSelected] = useState<string | null>(null);
  const meeting = meet.meetings.find((m) => m.id === selected) ?? null;

  // Follow a just-started recording.
  const recordingId = meet.status.recording?.session_id;
  useEffect(() => {
    if (recordingId) setSelected(recordingId);
  }, [recordingId]);

  return (
    <section className="page active" data-page-panel="meet">
      <div className="page-wrap wide">
        <div className="page-header">
          <div>
            <div className="eyebrow">{t("ui2.meet.eyebrow")}</div>
            <h1>{t("ui2.meet.name")}</h1>
            <p className="page-subtitle">{t("ui2.meet.subtitle")}</p>
          </div>
        </div>
        <Recorder meet={meet} />
        <SpeakerModelNotice meet={meet} />
        {meet.status.last_error && (
          <div className="meet-notice is-error" role="alert">
            <strong>{t("ui2.meet.processFailed")}</strong>
            <p>{meet.status.last_error}</p>
          </div>
        )}
        <div className="meet-layout">
          <MeetingList meet={meet} selected={selected} onSelect={setSelected} />
          {meeting ? (
            <MeetingDetail
              key={meeting.id}
              meet={meet}
              meeting={meeting}
              onDeleted={() => setSelected(null)}
            />
          ) : (
            <div className="meet-detail history-state">
              {t("ui2.meet.detail.pick")}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
