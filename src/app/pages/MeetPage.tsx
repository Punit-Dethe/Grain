import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type SyntheticEvent,
} from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import type {
  MeetingSummary,
  SourceDevice,
  SourceKind,
  SourceStatus,
  Stage,
  TrackStatus,
  TranscriptRevision,
} from "@/bindings";
import {
  buildTurns,
  filterTurns,
  formatClock,
  speakerLabels,
  transcriptText,
  type Turn,
} from "../meet/transcriptTurns";
import {
  groupByDay,
  highlightParts,
  levelFraction,
  turnAt,
} from "../meet/meetFormat";
import {
  useLiveRecording,
  useMeetController,
  type MeetController,
} from "../meet/useMeetController";

type T = (key: string, options?: Record<string, unknown>) => string;

/** Speaker accents, cycled in speaker order. */
const SPEAKER_TONES = ["peach", "sage", "lilac", "sky"] as const;
const STAGES: Stage[] = [
  "Preparing",
  "IdentifyingSpeakers",
  "Transcribing",
  "Publishing",
];

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function Glyph({ name }: { name: string }) {
  return (
    <svg className="icon sm" aria-hidden="true">
      <use href={`#i-${name}`} />
    </svg>
  );
}

const SOURCE_ICON: Record<SourceKind, string> = {
  Microphone: "mic",
  SystemAudio: "speaker",
};

function sourceName(kind: SourceKind, t: T): string {
  return kind === "Microphone"
    ? t("ui2.meet.sources.microphone")
    : t("ui2.meet.sources.systemAudio");
}

function trackStatusNote(status: TrackStatus, t: T): string | null {
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

/* ------------------------------------------------------------------------ */
/* Sources and recording                                                     */
/* ------------------------------------------------------------------------ */

function DevicePicker({
  label,
  devices,
  selected,
  disabled,
  onChange,
}: {
  label: string;
  devices: SourceDevice[];
  selected: string | null;
  disabled: boolean;
  onChange: (id: string | null) => void;
}) {
  const { t } = useTranslation();
  const fallback = devices.find((device) => device.is_default);
  const missing =
    selected !== null && !devices.some((device) => device.id === selected);
  return (
    <select
      className="select meet-source-device"
      value={selected ?? ""}
      disabled={disabled}
      aria-label={label}
      onChange={(event) => onChange(event.target.value || null)}
    >
      <option value="">
        {fallback
          ? t("ui2.meet.sources.followDefaultNamed", { name: fallback.name })
          : t("ui2.meet.sources.followDefault")}
      </option>
      {missing && (
        <option value={selected}>{t("ui2.meet.sources.disconnected")}</option>
      )}
      {devices.map((device) => (
        <option key={device.id} value={device.id}>
          {device.name}
        </option>
      ))}
    </select>
  );
}

function SourceTile({
  kind,
  hint,
  enabled,
  locked,
  unavailable,
  children,
  onToggle,
}: {
  kind: SourceKind;
  hint: string;
  enabled: boolean;
  /** The last source left on cannot be turned off. */
  locked: boolean;
  unavailable: boolean;
  children: ReactNode;
  onToggle: (enabled: boolean) => void;
}) {
  const { t } = useTranslation();
  const name = sourceName(kind, t);
  return (
    <div
      className={`meet-source${enabled && !unavailable ? " is-on" : ""}`}
      data-source={kind}
    >
      <div className="meet-source-head">
        <span className="meet-source-icon">
          <Glyph name={SOURCE_ICON[kind]} />
        </span>
        <div className="meet-source-copy">
          <strong>{name}</strong>
          <small>
            {unavailable ? t("ui2.meet.sources.unsupported") : hint}
          </small>
        </div>
        <button
          className={`toggle${enabled && !unavailable ? " on" : ""}`}
          type="button"
          role="switch"
          aria-checked={enabled && !unavailable}
          aria-label={name}
          title={locked ? t("ui2.meet.sources.keepOne") : undefined}
          disabled={locked || unavailable}
          onClick={() => onToggle(!enabled)}
        >
          <span />
        </button>
      </div>
      {!unavailable && children}
    </div>
  );
}

function SourceSetup({ meet }: { meet: MeetController }) {
  const { t } = useTranslation();
  const sources = meet.sources;
  if (!sources) {
    return (
      <div className="meet-sources" aria-busy="true">
        <div className="meet-source is-placeholder" />
        <div className="meet-source is-placeholder" />
      </div>
    );
  }
  const { settings } = sources;
  const micOn = settings.microphone.enabled;
  const systemOn =
    settings.system_audio.enabled && sources.system_audio_supported;
  const save = (next: typeof settings) => {
    meet.setSources(next).catch((error) => toast.error(errorText(error)));
  };
  return (
    <div className="meet-sources">
      <SourceTile
        kind="Microphone"
        hint={t("ui2.meet.sources.microphoneHint")}
        enabled={micOn}
        locked={micOn && !systemOn}
        unavailable={false}
        onToggle={(enabled) =>
          save({
            ...settings,
            microphone: { ...settings.microphone, enabled },
          })
        }
      >
        <DevicePicker
          label={t("ui2.meet.sources.microphoneDevice")}
          devices={sources.microphones}
          selected={settings.microphone.device_id}
          disabled={!micOn}
          onChange={(device_id) =>
            save({
              ...settings,
              microphone: { ...settings.microphone, device_id },
            })
          }
        />
      </SourceTile>
      <SourceTile
        kind="SystemAudio"
        hint={t("ui2.meet.sources.systemAudioHint")}
        enabled={systemOn}
        locked={systemOn && !micOn}
        unavailable={!sources.system_audio_supported}
        onToggle={(enabled) =>
          save({
            ...settings,
            system_audio: { ...settings.system_audio, enabled },
          })
        }
      >
        <DevicePicker
          label={t("ui2.meet.sources.systemAudioDevice")}
          devices={sources.outputs}
          selected={settings.system_audio.device_id}
          disabled={!systemOn}
          onChange={(device_id) =>
            save({
              ...settings,
              system_audio: { ...settings.system_audio, device_id },
            })
          }
        />
      </SourceTile>
      {micOn && systemOn && (
        <p className="meet-sources-hint">{t("ui2.meet.sources.headphones")}</p>
      )}
    </div>
  );
}

function LiveSource({ source }: { source: SourceStatus }) {
  const { t } = useTranslation();
  const fraction = levelFraction(source.level);
  const note = source.active
    ? null
    : source.kind === "Microphone"
      ? t("ui2.meet.live.micSilent")
      : t("ui2.meet.live.systemWaiting");
  return (
    <div className="meet-source is-on is-live" data-source={source.kind}>
      <div className="meet-source-head">
        <span className="meet-source-icon">
          <Glyph name={SOURCE_ICON[source.kind]} />
        </span>
        <div className="meet-source-copy">
          <strong>{sourceName(source.kind, t)}</strong>
          <small title={source.name}>{note ?? source.name}</small>
        </div>
      </div>
      <div
        className="meet-meter"
        role="meter"
        aria-label={t("ui2.meet.live.level", {
          source: sourceName(source.kind, t),
        })}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(fraction * 100)}
      >
        <i style={{ transform: `scaleX(${fraction})` }} />
      </div>
    </div>
  );
}

function Studio({ meet }: { meet: MeetController }) {
  const { t } = useTranslation();
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const live = useLiveRecording(meet.status.recording);
  const recording = live ?? meet.status.recording;

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
      <section
        className="meet-studio is-recording"
        aria-label={t("ui2.meet.live.label")}
      >
        <div className="meet-studio-row">
          <span className="meet-rec-dot" aria-hidden="true" />
          <div className="meet-live-copy">
            <strong>{t("ui2.meet.live.recording")}</strong>
            <span>{t("ui2.meet.live.afterStop")}</span>
          </div>
          <span className="meet-clock" role="timer">
            {formatClock(recording.elapsed_ms)}
          </span>
          <button
            className="button meet-stop"
            type="button"
            disabled={busy}
            onClick={stop}
          >
            <Glyph name="stop" />
            {t("ui2.meet.live.stop")}
          </button>
        </div>
        <div className="meet-sources">
          {recording.sources.map((source) => (
            <LiveSource key={source.kind} source={source} />
          ))}
        </div>
        {recording.error && (
          <p className="meet-inline-error" role="alert">
            {recording.error}
          </p>
        )}
      </section>
    );
  }

  return (
    <section className="meet-studio" aria-label={t("ui2.meet.studio.label")}>
      <form
        className="meet-studio-row"
        onSubmit={(event) => {
          event.preventDefault();
          void start();
        }}
      >
        <label className="meet-title-field">
          <span className="sr-only">{t("ui2.meet.studio.titleLabel")}</span>
          <input
            value={title}
            maxLength={120}
            onChange={(event) => setTitle(event.target.value)}
            placeholder={t("ui2.meet.studio.titlePlaceholder")}
          />
        </label>
        <button
          className="button primary meet-record"
          type="submit"
          disabled={busy || !meet.sources}
        >
          <span className="meet-record-dot" aria-hidden="true" />
          {busy ? t("ui2.meet.studio.starting") : t("ui2.meet.studio.record")}
        </button>
      </form>
      <SourceSetup meet={meet} />
    </section>
  );
}

function SpeakerModelNotice({ meet }: { meet: MeetController }) {
  const { t } = useTranslation();
  if (meet.diarizerInstalled !== false) return null;
  const downloading = meet.diarizerProgress !== null;
  const percent = Math.round((meet.diarizerProgress ?? 0) * 100);
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
      <div className="meet-notice-copy">
        <strong>{t("ui2.meet.speakerModel.title")}</strong>
        <p>{t("ui2.meet.speakerModel.body")}</p>
        {downloading && (
          <div
            className="meet-progress"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent}
          >
            <i style={{ transform: `scaleX(${percent / 100})` }} />
          </div>
        )}
      </div>
      <button
        className="button"
        type="button"
        disabled={downloading}
        onClick={download}
      >
        {downloading
          ? t("ui2.meet.speakerModel.downloading", { percent })
          : t("ui2.meet.speakerModel.download")}
      </button>
    </div>
  );
}

/* ------------------------------------------------------------------------ */
/* Meeting list                                                              */
/* ------------------------------------------------------------------------ */

function SourceGlyphs({ sources }: { sources: SourceKind[] }) {
  const { t } = useTranslation();
  const kinds = [...new Set(sources)];
  return (
    <span
      className="meet-source-glyphs"
      aria-label={kinds.map((kind) => sourceName(kind, t)).join(", ")}
      role="img"
    >
      {kinds.map((kind) => (
        <Glyph key={kind} name={SOURCE_ICON[kind]} />
      ))}
    </span>
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
  const groups = useMemo(
    () => groupByDay(meet.meetings, new Date()),
    [meet.meetings],
  );
  const dayFormat = useMemo(
    () =>
      new Intl.DateTimeFormat(i18n.language, {
        weekday: "long",
        month: "long",
        day: "numeric",
      }),
    [i18n.language],
  );
  const timeFormat = useMemo(
    () =>
      new Intl.DateTimeFormat(i18n.language, {
        hour: "numeric",
        minute: "2-digit",
      }),
    [i18n.language],
  );

  if (meet.loading) {
    return (
      <div className="meet-list meet-list-state">
        {t("ui2.meet.list.loading")}
      </div>
    );
  }
  if (meet.loadError) {
    return (
      <div className="meet-list meet-list-state">
        <p>{t("ui2.meet.list.loadError")}</p>
        <button className="button" type="button" onClick={meet.reload}>
          {t("ui2.meet.list.retry")}
        </button>
      </div>
    );
  }
  if (meet.meetings.length === 0) {
    return (
      <div className="meet-list meet-list-state">
        <p>{t("ui2.meet.list.empty")}</p>
      </div>
    );
  }
  const processingId = meet.status.processing?.session_id;
  const recordingId = meet.status.recording?.session_id;
  return (
    <nav className="meet-list" aria-label={t("ui2.meet.list.label")}>
      {groups.map((group) => (
        <section key={group.key} className="meet-day">
          <h3>
            {group.kind === "today"
              ? t("ui2.meet.list.today")
              : group.kind === "yesterday"
                ? t("ui2.meet.list.yesterday")
                : dayFormat.format(group.day)}
          </h3>
          <ul>
            {group.meetings.map((meeting) => {
              const state =
                meeting.id === recordingId
                  ? "recording"
                  : meeting.id === processingId
                    ? "processing"
                    : meeting.transcript_revision != null
                      ? "transcribed"
                      : "new";
              return (
                <li key={meeting.id}>
                  <button
                    type="button"
                    className="meet-row"
                    aria-current={selected === meeting.id ? "true" : undefined}
                    onClick={() => onSelect(meeting.id)}
                  >
                    <span className="meet-row-title">{meeting.title}</span>
                    <span className="meet-row-meta">
                      <time>{timeFormat.format(meeting.created_unix_ms)}</time>
                      <span>{formatClock(meeting.duration_ms)}</span>
                      <SourceGlyphs sources={meeting.sources} />
                    </span>
                    <span className="meet-row-state" data-state={state}>
                      {state === "processing" && (
                        <span className="meet-spinner" aria-hidden="true" />
                      )}
                      {t(`ui2.meet.list.state.${state}`)}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </nav>
  );
}

/* ------------------------------------------------------------------------ */
/* Playback                                                                  */
/* ------------------------------------------------------------------------ */

type PlaybackState = "idle" | "loading" | "ready" | "error";

/** One meeting's mixed audio, prepared on first use. */
function usePlayback(meet: MeetController, sessionId: string) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const pendingSeek = useRef<number | null>(null);
  const [src, setSrc] = useState<string | null>(null);
  const [state, setState] = useState<PlaybackState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);

  // A blob URL (Linux) holds the file in memory until revoked.
  useEffect(
    () => () => {
      if (src?.startsWith("blob:")) URL.revokeObjectURL(src);
    },
    [src],
  );

  const load = useCallback(async () => {
    if (src) return true;
    setState("loading");
    setError(null);
    try {
      setSrc(await meet.loadPlayback(sessionId));
      setState("ready");
      return true;
    } catch (failure) {
      setError(errorText(failure));
      setState("error");
      return false;
    }
  }, [meet, sessionId, src]);

  const seek = useCallback(
    async (ms: number) => {
      const audio = audioRef.current;
      if (audio && src && audio.readyState >= 1) {
        audio.currentTime = ms / 1_000;
        setPosition(ms);
        void audio.play().catch(() => undefined);
        return;
      }
      pendingSeek.current = ms;
      await load();
    },
    [load, src],
  );

  const toggle = useCallback(async () => {
    const audio = audioRef.current;
    if (!src) {
      pendingSeek.current = 0;
      await load();
      return;
    }
    if (!audio) return;
    if (audio.paused) void audio.play().catch(() => undefined);
    else audio.pause();
  }, [load, src]);

  const audioProps = {
    ref: audioRef,
    src: src ?? undefined,
    preload: "metadata" as const,
    onLoadedMetadata: (event: SyntheticEvent<HTMLAudioElement>) => {
      const audio = event.currentTarget;
      setDuration(Number.isFinite(audio.duration) ? audio.duration * 1_000 : 0);
      const at = pendingSeek.current;
      pendingSeek.current = null;
      if (at !== null) {
        audio.currentTime = at / 1_000;
        void audio.play().catch(() => undefined);
      }
    },
    onTimeUpdate: (event: SyntheticEvent<HTMLAudioElement>) =>
      setPosition(event.currentTarget.currentTime * 1_000),
    onPlay: () => setPlaying(true),
    onPause: () => setPlaying(false),
    onEnded: () => setPlaying(false),
    onError: () => {
      setState("error");
      setPlaying(false);
    },
  };

  return {
    state,
    error,
    playing,
    position,
    duration,
    toggle,
    seek,
    audioProps,
  };
}

function Player({
  playback,
  fallbackDuration,
}: {
  playback: ReturnType<typeof usePlayback>;
  fallbackDuration: number;
}) {
  const { t } = useTranslation();
  const duration = playback.duration || fallbackDuration;
  const loading = playback.state === "loading";
  return (
    <div className="meet-player">
      <audio {...playback.audioProps} />
      <button
        className="meet-play"
        type="button"
        disabled={loading}
        aria-label={
          playback.playing
            ? t("ui2.meet.player.pause")
            : t("ui2.meet.player.play")
        }
        onClick={() => void playback.toggle()}
      >
        {loading ? (
          <span className="meet-spinner" aria-hidden="true" />
        ) : (
          <Glyph name={playback.playing ? "pause" : "play"} />
        )}
      </button>
      <span className="meet-player-time">{formatClock(playback.position)}</span>
      <input
        className="meet-scrubber"
        type="range"
        min={0}
        max={Math.max(1, Math.round(duration))}
        step={100}
        value={Math.min(playback.position, duration)}
        disabled={loading}
        aria-label={t("ui2.meet.player.position")}
        aria-valuetext={formatClock(playback.position)}
        style={{
          ["--progress" as string]: `${duration > 0 ? (playback.position / duration) * 100 : 0}%`,
        }}
        onChange={(event) => void playback.seek(Number(event.target.value))}
      />
      <span className="meet-player-time">{formatClock(duration)}</span>
      {loading && (
        <span className="meet-player-note" role="status">
          {t("ui2.meet.player.preparing")}
        </span>
      )}
      {playback.state === "error" && (
        <span className="meet-inline-error" role="alert">
          {playback.error ?? t("ui2.meet.player.failed")}
        </span>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------------ */
/* Transcript                                                                */
/* ------------------------------------------------------------------------ */

interface SpeakerLook {
  label: string;
  /** Avatar text: the speaker's number, or empty for unattributed text. */
  mark: string;
  tone: string | null;
}

const TurnRow = memo(function TurnRow({
  turn,
  look,
  source,
  current,
  query,
  onSeek,
}: {
  turn: Turn;
  look: SpeakerLook;
  source: SourceKind | null;
  current: boolean;
  query: string;
  onSeek: (ms: number) => void;
}) {
  const { t } = useTranslation();
  return (
    <li className={`meet-turn${current ? " is-current" : ""}`}>
      <span
        className={`meet-avatar${look.tone ? ` tone-${look.tone}` : ""}`}
        aria-hidden="true"
      >
        {look.mark}
      </span>
      <div className="meet-turn-body">
        <div className="meet-turn-head">
          <strong>{look.label}</strong>
          {source && (
            <span
              className="meet-turn-source"
              role="img"
              aria-label={sourceName(source, t)}
              title={sourceName(source, t)}
            >
              <Glyph name={SOURCE_ICON[source]} />
            </span>
          )}
          <button
            type="button"
            className="meet-time"
            aria-label={t("ui2.meet.detail.playFrom", {
              time: formatClock(turn.startMs),
            })}
            onClick={() => onSeek(turn.startMs)}
          >
            {formatClock(turn.startMs)}
          </button>
        </div>
        <p>
          {highlightParts(turn.text, query).map((part, index) =>
            part.match ? <mark key={index}>{part.text}</mark> : part.text,
          )}
        </p>
      </div>
    </li>
  );
});

function ProcessingSteps({ stage }: { stage: Stage }) {
  const { t } = useTranslation();
  const current = STAGES.indexOf(stage);
  return (
    <div className="meet-processing">
      <ol className="meet-steps" aria-label={t("ui2.meet.detail.progress")}>
        {STAGES.map((step, index) => {
          const state =
            index < current ? "done" : index === current ? "current" : "todo";
          return (
            <li
              key={step}
              data-state={state}
              aria-current={state === "current" ? "step" : undefined}
            >
              <span className="meet-step-mark" aria-hidden="true">
                {state === "done" ? (
                  <Glyph name="check" />
                ) : state === "current" ? (
                  <span className="meet-spinner" />
                ) : null}
              </span>
              {t(`ui2.meet.steps.${step}`)}
            </li>
          );
        })}
      </ol>
      <span className="sr-only" role="status">
        {t(`ui2.meet.stage.${stage}`)}
      </span>
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
  const { t, i18n } = useTranslation();
  const [transcript, setTranscript] = useState<TranscriptRevision | null>(null);
  const [transcriptError, setTranscriptError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const processing = meet.status.processing;
  const isProcessing = processing?.session_id === meeting.id;
  const isRecording = meet.status.recording?.session_id === meeting.id;
  const playback = usePlayback(meet, meeting.id);

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

  const turns = useMemo(
    () => (transcript ? buildTurns(transcript) : []),
    [transcript],
  );
  const labels = useMemo(
    () => speakerLabels(transcript?.speakers ?? []),
    [transcript],
  );
  const looks = useMemo(() => {
    const map = new Map<string, SpeakerLook>();
    (transcript?.speakers ?? []).forEach((speaker, index) => {
      map.set(speaker.id, {
        label: speaker.label,
        mark: String(index + 1),
        tone: SPEAKER_TONES[index % SPEAKER_TONES.length],
      });
    });
    return map;
  }, [transcript]);
  const unknown = useMemo<SpeakerLook>(
    () => ({
      label: t("ui2.meet.detail.unknownSpeaker"),
      mark: "",
      tone: null,
    }),
    [t],
  );
  const overlap = useMemo<SpeakerLook>(
    () => ({ label: t("ui2.meet.detail.overlap"), mark: "", tone: null }),
    [t],
  );
  const visible = useMemo(() => filterTurns(turns, query), [turns, query]);
  const currentKey = playback.playing ? turnAt(turns, playback.position) : null;
  const multiSource = new Set(meeting.sources).size > 1;
  const { seek } = playback;
  const onSeek = useCallback((ms: number) => void seek(ms), [seek]);

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

  const notes = [
    ...new Set(
      (transcript?.tracks ?? [])
        .map((track) => trackStatusNote(track.status, t))
        .filter((note): note is string => note !== null),
    ),
  ];
  const when = new Intl.DateTimeFormat(i18n.language, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(meeting.created_unix_ms);
  const sourcesText = [...new Set(meeting.sources)]
    .map((kind) => sourceName(kind, t))
    .join(" + ");

  let body: ReactNode;
  if (isRecording) {
    body = (
      <div className="meet-empty">
        <strong>{t("ui2.meet.detail.recordingTitle")}</strong>
        <p>{t("ui2.meet.detail.recordingBody")}</p>
      </div>
    );
  } else if (transcript) {
    body = (
      <>
        <div className="meet-transcript-bar">
          <label className="meet-search">
            <Glyph name="search" />
            <input
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("ui2.meet.detail.search")}
              aria-label={t("ui2.meet.detail.search")}
            />
          </label>
          {transcript.speakers.length > 0 && (
            <ul
              className="meet-legend"
              aria-label={t("ui2.meet.detail.speakers")}
            >
              {transcript.speakers.map((speaker) => {
                const look = looks.get(speaker.id) ?? unknown;
                return (
                  <li key={speaker.id}>
                    <span
                      className={`meet-avatar is-small${look.tone ? ` tone-${look.tone}` : ""}`}
                      aria-hidden="true"
                    >
                      {look.mark}
                    </span>
                    {look.label}
                  </li>
                );
              })}
            </ul>
          )}
        </div>
        {visible.length === 0 ? (
          <p className="meet-empty-line">
            {turns.length === 0
              ? t("ui2.meet.detail.noSpeech")
              : t("ui2.meet.detail.noMatches")}
          </p>
        ) : (
          <ol className="meet-transcript">
            {visible.map((turn) => (
              <TurnRow
                key={turn.key}
                turn={turn}
                look={
                  turn.speakerId
                    ? (looks.get(turn.speakerId) ?? unknown)
                    : turn.ambiguous
                      ? overlap
                      : unknown
                }
                source={
                  multiSource ? (meeting.sources[turn.track] ?? null) : null
                }
                current={turn.key === currentKey}
                query={query}
                onSeek={onSeek}
              />
            ))}
          </ol>
        )}
      </>
    );
  } else if (meeting.transcript_revision != null && !transcriptError) {
    body = <p className="meet-empty-line">{t("ui2.meet.detail.loading")}</p>;
  } else if (!isProcessing) {
    body = (
      <div className="meet-empty">
        <strong>{t("ui2.meet.detail.noTranscriptTitle")}</strong>
        <p>{t("ui2.meet.detail.noTranscript")}</p>
        <button
          className="button primary"
          type="button"
          disabled={processing != null}
          onClick={runProcess}
        >
          {t("ui2.meet.detail.process")}
        </button>
      </div>
    );
  }

  return (
    <article className="meet-detail" aria-labelledby="meet-detail-title">
      <header className="meet-detail-head">
        <div className="meet-detail-title">
          <h2 id="meet-detail-title">{meeting.title}</h2>
          <p className="meet-detail-meta">
            <span>{when}</span>
            <span>{formatClock(meeting.duration_ms)}</span>
            <span>{sourcesText}</span>
          </p>
        </div>
        <div className="meet-detail-actions">
          {isProcessing ? (
            <button className="button" type="button" onClick={cancel}>
              {t("ui2.meet.detail.cancel")}
            </button>
          ) : (
            meeting.transcript_revision != null && (
              <button
                className="button"
                type="button"
                disabled={isRecording || processing != null}
                onClick={runProcess}
              >
                <Glyph name="refresh" />
                {t("ui2.meet.detail.reprocess")}
              </button>
            )
          )}
          <button
            className="icon-button"
            type="button"
            disabled={turns.length === 0}
            aria-label={t("ui2.meet.detail.copy")}
            title={t("ui2.meet.detail.copy")}
            onClick={copy}
          >
            <Glyph name="copy" />
          </button>
          <button
            className="icon-button meet-delete"
            type="button"
            disabled={isRecording || isProcessing}
            aria-label={t("ui2.meet.detail.delete")}
            title={t("ui2.meet.detail.delete")}
            onClick={remove}
          >
            <Glyph name="trash" />
          </button>
        </div>
      </header>

      {isProcessing && processing && (
        <ProcessingSteps stage={processing.stage} />
      )}

      {!isRecording && (
        <Player playback={playback} fallbackDuration={meeting.duration_ms} />
      )}

      {notes.map((note) => (
        <p key={note} className="meet-note">
          {note}
        </p>
      ))}
      {transcriptError && (
        <p className="meet-inline-error" role="alert">
          {transcriptError}
        </p>
      )}

      {body}
    </article>
  );
}

export function MeetPage() {
  const { t } = useTranslation();
  const meet = useMeetController();
  const [selected, setSelected] = useState<string | null>(null);
  const meeting =
    meet.meetings.find((m) => m.id === selected) ?? meet.meetings[0] ?? null;

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
            <h1>{t("ui2.meet.name")}</h1>
            <p className="page-subtitle">{t("ui2.meet.subtitle")}</p>
          </div>
        </div>
        <Studio meet={meet} />
        <SpeakerModelNotice meet={meet} />
        {meet.status.last_error && (
          <div className="meet-notice is-error" role="alert">
            <div className="meet-notice-copy">
              <strong>{t("ui2.meet.processFailed")}</strong>
              <p>{meet.status.last_error}</p>
            </div>
          </div>
        )}
        <div className="meet-layout">
          <MeetingList
            meet={meet}
            selected={meeting?.id ?? null}
            onSelect={setSelected}
          />
          {meeting ? (
            <MeetingDetail
              key={meeting.id}
              meet={meet}
              meeting={meeting}
              onDeleted={() => setSelected(null)}
            />
          ) : (
            !meet.loading && (
              <div className="meet-detail meet-detail-empty">
                <p>{t("ui2.meet.detail.pick")}</p>
              </div>
            )
          )}
        </div>
      </div>
    </section>
  );
}
