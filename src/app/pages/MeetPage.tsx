import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type RefObject,
  type SyntheticEvent,
} from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import type {
  MeetingSummary,
  SourceDevice,
  SourceKind,
  SourceSettings,
  SourceStatus,
  Stage,
  TrackStatus,
  TranscriptRevision,
} from "@/bindings";
import { hashForRoute } from "../navigation";
import {
  buildTurns,
  filterTurns,
  formatClock,
  speakerLabels,
  transcriptText,
  type Turn,
  type TurnWord,
} from "../meet/transcriptTurns";
import {
  groupByDay,
  highlightParts,
  levelFraction,
  turnAt,
  wordAt,
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
const PLAYBACK_RATES = [1, 1.25, 1.5, 2] as const;
const SKIP_MS = 10_000;
const METER_TICKS = 18;

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function openMeeting(id: string | null) {
  window.location.hash = hashForRoute(
    id ? { page: "meet", meeting: id } : { page: "meet" },
  );
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
/* Recording studio                                                          */
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
      className="meet-device"
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

function SourceRow({
  kind,
  enabled,
  locked,
  unavailable,
  children,
  onToggle,
}: {
  kind: SourceKind;
  enabled: boolean;
  locked: boolean;
  unavailable: boolean;
  children: ReactNode;
  onToggle: (enabled: boolean) => void;
}) {
  const { t } = useTranslation();
  const name = sourceName(kind, t);
  const on = enabled && !unavailable;
  return (
    <div className={`meet-source${on ? " is-on" : ""}`}>
      <span className="meet-source-icon">
        <Glyph name={SOURCE_ICON[kind]} />
      </span>
      <div className="meet-source-body">
        <span className="meet-source-name">{name}</span>
        {unavailable ? (
          <span className="meet-source-note">
            {t("ui2.meet.sources.unsupported")}
          </span>
        ) : (
          children
        )}
      </div>
      <button
        className={`toggle${on ? " on" : ""}`}
        type="button"
        role="switch"
        aria-checked={on}
        aria-label={name}
        title={locked ? t("ui2.meet.sources.keepOne") : undefined}
        disabled={locked || unavailable}
        onClick={() => onToggle(!enabled)}
      >
        <span />
      </button>
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
  const save = (next: SourceSettings) => {
    meet.setSources(next).catch((error) => toast.error(errorText(error)));
  };
  return (
    <>
      <div className="meet-sources">
        <SourceRow
          kind="Microphone"
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
        </SourceRow>
        <SourceRow
          kind="SystemAudio"
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
        </SourceRow>
      </div>
      {micOn && systemOn && (
        <p className="meet-sources-hint">{t("ui2.meet.sources.headphones")}</p>
      )}
    </>
  );
}

function Meter({ fraction, label }: { fraction: number; label: string }) {
  const lit = Math.round(fraction * METER_TICKS);
  return (
    <div
      className="meet-ticks"
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(fraction * 100)}
    >
      {Array.from({ length: METER_TICKS }, (_, index) => (
        <i
          key={index}
          className={
            index < lit
              ? index >= METER_TICKS - 3
                ? "on is-hot"
                : "on"
              : undefined
          }
        />
      ))}
    </div>
  );
}

function LiveSource({ source }: { source: SourceStatus }) {
  const { t } = useTranslation();
  const name = sourceName(source.kind, t);
  const note = source.active
    ? source.name
    : source.kind === "Microphone"
      ? t("ui2.meet.live.micSilent")
      : t("ui2.meet.live.systemWaiting");
  return (
    <div className="meet-source is-on is-live">
      <span className="meet-source-icon">
        <Glyph name={SOURCE_ICON[source.kind]} />
      </span>
      <div className="meet-source-body">
        <span className="meet-source-name">{name}</span>
        <span className="meet-source-note" title={source.name}>
          {note}
        </span>
      </div>
      <Meter
        fraction={levelFraction(source.level)}
        label={t("ui2.meet.live.level", { source: name })}
      />
    </div>
  );
}

function StudioWave() {
  return (
    <svg className="meet-studio-wave" viewBox="0 0 80 48" aria-hidden="true">
      <path d="M3 22v4m10-8v12m10-14v16m10-20v24m11-29v34m11-39v44m11-35v26m11-17v8" />
    </svg>
  );
}

function Studio({ meet }: { meet: MeetController }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const live = useLiveRecording(meet.status.recording);
  const recording = live ?? meet.status.recording;
  const settings = meet.sources?.settings;
  const micOn = Boolean(settings?.microphone.enabled);
  const systemOn = Boolean(
    settings?.system_audio.enabled && meet.sources?.system_audio_supported,
  );

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await action();
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  const level = recording?.sources.reduce(
    (peak, source) => Math.max(peak, levelFraction(source.level)),
    0,
  );
  return (
    <section
      className={`meet-studio${recording ? " is-recording" : ""}`}
      aria-label={t(
        recording ? "ui2.meet.live.label" : "ui2.meet.studio.label",
      )}
    >
      <div className="meet-studio-stage">
        <StudioWave />
        <button
          className="meet-orb"
          type="button"
          disabled={
            busy || (!recording && (!meet.sources || (!micOn && !systemOn)))
          }
          style={
            recording ? ({ "--level": level } as CSSProperties) : undefined
          }
          onClick={() =>
            void run(
              recording ? meet.stopRecording : () => meet.startRecording(null),
            )
          }
        >
          <Glyph name={recording ? "stop" : "mic"} />
          <strong>
            {recording
              ? t("ui2.meet.studio.stop")
              : busy
                ? t("ui2.meet.studio.starting")
                : t("ui2.meet.studio.start")}
          </strong>
        </button>
        <StudioWave />
      </div>
      {recording ? (
        <>
          <div className="meet-studio-status">
            <span className="meet-live-dot" aria-hidden="true" />
            <span>{t("ui2.meet.live.recording")}</span>
            <strong className="meet-clock" role="timer">
              {formatClock(recording.elapsed_ms)}
            </strong>
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
        </>
      ) : (
        <SourceSetup meet={meet} />
      )}
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
    <div className="meet-callout">
      <p>{t("ui2.meet.speakerModel.body")}</p>
      {downloading ? (
        <div
          className="meet-progress"
          role="progressbar"
          aria-label={t("ui2.meet.speakerModel.downloading", { percent })}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
        >
          <i style={{ transform: `scaleX(${percent / 100})` }} />
        </div>
      ) : (
        <button className="text-button meet-callout-action" onClick={download}>
          {t("ui2.meet.speakerModel.download")}
        </button>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------------ */
/* Recordings                                                                */
/* ------------------------------------------------------------------------ */

function RecordingLead({
  meeting,
  state,
}: {
  meeting: MeetingSummary;
  state: string;
}) {
  if (state === "recording") {
    return (
      <span className="meet-lead is-live" aria-hidden="true">
        <span className="meet-bars">
          <i />
          <i />
          <i />
          <i />
        </span>
      </span>
    );
  }
  if (state === "processing") {
    return (
      <span className="meet-lead" aria-hidden="true">
        <span className="meet-spinner" />
      </span>
    );
  }
  return (
    <span className="meet-lead" aria-hidden="true">
      <Glyph name={SOURCE_ICON[meeting.sources[0] ?? "Microphone"]} />
    </span>
  );
}

/** A clock that ticks once a second while `active`, for live durations. */
function useSecondTick(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [active]);
  return now;
}

function Recordings({ meet }: { meet: MeetController }) {
  const { t, i18n } = useTranslation();
  const recordingId = meet.status.recording?.session_id;
  const now = useSecondTick(Boolean(recordingId));
  const groups = useMemo(
    () => groupByDay(meet.meetings, new Date()),
    [meet.meetings],
  );
  const dayFormat = useMemo(
    () =>
      new Intl.DateTimeFormat(i18n.language, {
        month: "short",
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

  let body: ReactNode;
  if (meet.loading) {
    body = <p className="meet-list-state">{t("ui2.meet.list.loading")}</p>;
  } else if (meet.loadError) {
    body = (
      <div className="meet-list-state">
        <p>{t("ui2.meet.list.loadError")}</p>
        <button className="button" type="button" onClick={meet.reload}>
          {t("ui2.meet.list.retry")}
        </button>
      </div>
    );
  } else if (meet.meetings.length === 0) {
    body = (
      <div className="meet-list-empty">
        <strong>{t("ui2.meet.list.emptyTitle")}</strong>
        <p>{t("ui2.meet.list.empty")}</p>
      </div>
    );
  } else {
    const processingId = meet.status.processing?.session_id;
    body = (
      <ul className="meet-recording-list">
        {groups.flatMap((group) => {
          const day =
            group.kind === "today"
              ? t("ui2.meet.list.today")
              : group.kind === "yesterday"
                ? t("ui2.meet.list.yesterday")
                : dayFormat.format(group.day);
          return group.meetings.map((meeting) => {
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
                  data-state={state}
                  onClick={() => openMeeting(meeting.id)}
                >
                  <RecordingLead meeting={meeting} state={state} />
                  <span className="meet-row-main">
                    <span className="meet-row-title">{meeting.title}</span>
                    <time
                      className="meet-row-meta"
                      dateTime={new Date(meeting.created_unix_ms).toISOString()}
                    >
                      {day}, {timeFormat.format(meeting.created_unix_ms)}
                    </time>
                    {state !== "transcribed" && (
                      <span className="meet-row-state">
                        {t(`ui2.meet.list.state.${state}`)}
                      </span>
                    )}
                  </span>
                  <span className="meet-row-duration">
                    {formatClock(
                      state === "recording"
                        ? now - meeting.created_unix_ms
                        : meeting.duration_ms,
                    )}
                  </span>
                  <span className="meet-row-go" aria-hidden="true">
                    <Glyph name="chev-right" />
                  </span>
                </button>
              </li>
            );
          });
        })}
      </ul>
    );
  }

  return (
    <section className="meet-recordings" aria-labelledby="meet-recordings">
      <h2 id="meet-recordings">{t("ui2.meet.list.heading")}</h2>
      {body}
    </section>
  );
}

/* ------------------------------------------------------------------------ */
/* Playback                                                                  */
/* ------------------------------------------------------------------------ */

type PlaybackState = "idle" | "loading" | "ready" | "error";

/** One meeting's mixed audio, prepared on first use. */
function usePlayback(meet: MeetController, sessionId: string) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const pending = useRef<{ ms: number; play: boolean } | null>(null);
  const [src, setSrc] = useState<string | null>(null);
  const [state, setState] = useState<PlaybackState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const [rate, setRate] = useState<number>(1);

  // A blob URL (Linux) holds the file in memory until revoked.
  useEffect(
    () => () => {
      if (src?.startsWith("blob:")) URL.revokeObjectURL(src);
    },
    [src],
  );

  useEffect(() => {
    if (audioRef.current) audioRef.current.playbackRate = rate;
  }, [rate, src]);

  const load = useCallback(async () => {
    if (src) return;
    setState("loading");
    setError(null);
    try {
      setSrc(await meet.loadPlayback(sessionId));
      setState("ready");
    } catch (failure) {
      setError(errorText(failure));
      setState("error");
    }
  }, [meet, sessionId, src]);

  const seek = useCallback(
    async (ms: number, play = true) => {
      const audio = audioRef.current;
      const at = Math.max(0, ms);
      if (audio && src && audio.readyState >= 1) {
        audio.currentTime = at / 1_000;
        setPosition(at);
        if (play) void audio.play().catch(() => undefined);
        return;
      }
      pending.current = { ms: at, play };
      setPosition(at);
      await load();
    },
    [load, src],
  );

  const toggle = useCallback(async () => {
    const audio = audioRef.current;
    if (!src) {
      await seek(position, true);
      return;
    }
    if (!audio) return;
    if (audio.paused) void audio.play().catch(() => undefined);
    else audio.pause();
  }, [position, seek, src]);

  const skip = useCallback(
    (delta: number) => {
      const audio = audioRef.current;
      const limit = duration > 0 ? duration : Number.POSITIVE_INFINITY;
      void seek(
        Math.min(limit, position + delta),
        Boolean(audio && !audio.paused),
      );
    },
    [duration, position, seek],
  );

  const cycleRate = useCallback(() => {
    setRate((current) => {
      const index = PLAYBACK_RATES.indexOf(
        current as (typeof PLAYBACK_RATES)[number],
      );
      return PLAYBACK_RATES[(index + 1) % PLAYBACK_RATES.length];
    });
  }, []);

  const audioProps = {
    ref: audioRef,
    src: src ?? undefined,
    preload: "metadata" as const,
    onLoadedMetadata: (event: SyntheticEvent<HTMLAudioElement>) => {
      const audio = event.currentTarget;
      audio.playbackRate = rate;
      setDuration(Number.isFinite(audio.duration) ? audio.duration * 1_000 : 0);
      const next = pending.current;
      pending.current = null;
      if (next) {
        audio.currentTime = next.ms / 1_000;
        if (next.play) void audio.play().catch(() => undefined);
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
    rate,
    toggle,
    seek,
    skip,
    cycleRate,
    audioProps,
    audioRef,
  };
}

type Playback = ReturnType<typeof usePlayback>;

function Player({
  playback,
  fallbackDuration,
  title,
  now,
}: {
  playback: Playback;
  fallbackDuration: number;
  title: string;
  /** The speaker at the playback position, if the transcript has one. */
  now: SpeakerLook | null;
}) {
  const { t } = useTranslation();
  const duration = playback.duration || fallbackDuration;
  const loading = playback.state === "loading";
  const progress = duration > 0 ? (playback.position / duration) * 100 : 0;
  return (
    <div
      className="meet-dock"
      role="region"
      aria-label={t("ui2.meet.player.label")}
    >
      <audio {...playback.audioProps} />
      <div className="meet-dock-inner">
        <div className="meet-dock-now">
          {now ? (
            <span
              className={`meet-avatar${now.tone ? ` tone-${now.tone}` : ""}`}
              aria-hidden="true"
            >
              {now.mark}
            </span>
          ) : (
            <span className="meet-avatar" aria-hidden="true">
              <Glyph name="mic" />
            </span>
          )}
          <span className="meet-dock-now-copy">
            <strong>{now ? now.label : title}</strong>
            <span>{now ? title : t("ui2.meet.player.label")}</span>
          </span>
        </div>
        <div className="meet-dock-center">
          <div className="meet-dock-controls">
            <button
              className="meet-dock-skip"
              type="button"
              aria-label={t("ui2.meet.player.back")}
              title={t("ui2.meet.player.back")}
              onClick={() => playback.skip(-SKIP_MS)}
            >
              <Glyph name="back10" />
            </button>
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
            <button
              className="meet-dock-skip"
              type="button"
              aria-label={t("ui2.meet.player.forward")}
              title={t("ui2.meet.player.forward")}
              onClick={() => playback.skip(SKIP_MS)}
            >
              <Glyph name="fwd10" />
            </button>
          </div>
          <div className="meet-dock-bar">
            <span className="meet-dock-time">
              {formatClock(playback.position)}
            </span>
            <input
              className="meet-scrubber"
              type="range"
              min={0}
              max={Math.max(1, Math.round(duration))}
              step={100}
              value={Math.min(playback.position, duration)}
              aria-label={t("ui2.meet.player.position")}
              aria-valuetext={formatClock(playback.position)}
              style={{ "--progress": `${progress}%` } as CSSProperties}
              onChange={(event) =>
                void playback.seek(Number(event.target.value), playback.playing)
              }
            />
            <span className="meet-dock-time">{formatClock(duration)}</span>
          </div>
        </div>
        <div className="meet-dock-end">
          {playback.state === "error" && (
            <span className="meet-dock-error" role="alert">
              {playback.error ?? t("ui2.meet.player.failed")}
            </span>
          )}
          <button
            className="meet-rate"
            type="button"
            aria-label={t("ui2.meet.player.speed", { rate: playback.rate })}
            title={t("ui2.meet.player.speed", { rate: playback.rate })}
            onClick={playback.cycleRate}
          >
            {playback.rate}×
          </button>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------------ */
/* Recording view                                                            */
/* ------------------------------------------------------------------------ */

interface SpeakerLook {
  label: string;
  /** Avatar text: the speaker's number, or empty for unattributed text. */
  mark: string;
  tone: string | null;
}

/** The current turn's words: the word being heard is underlined, and the
 * underline glides from word to word. While playing it follows the audio
 * every frame; the component only re-renders when the word changes. */
function SpokenText({
  words,
  audio,
  playing,
  position,
  onSeek,
}: {
  words: TurnWord[];
  audio: RefObject<HTMLAudioElement | null>;
  playing: boolean;
  position: number;
  onSeek: (ms: number) => void;
}) {
  const [index, setIndex] = useState(() => wordAt(words, position));
  const textRef = useRef<HTMLParagraphElement>(null);
  const lineRef = useRef<HTMLSpanElement>(null);
  const lastTop = useRef<number | null>(null);

  // Paused: follow seeks. Playing: follow the audio every frame.
  useEffect(() => {
    if (!playing) setIndex(wordAt(words, position));
  }, [playing, position, words]);

  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    const tick = () => {
      const element = audio.current;
      if (element) {
        const next = wordAt(words, element.currentTime * 1_000);
        setIndex((current) => (current === next ? current : next));
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [audio, playing, words]);

  useLayoutEffect(() => {
    const line = lineRef.current;
    const word = textRef.current?.querySelector<HTMLElement>(
      `[data-word="${index}"]`,
    );
    if (!line) return;
    if (!word) {
      line.style.opacity = "0";
      lastTop.current = null;
      return;
    }
    const top = word.offsetTop + word.offsetHeight - 2;
    // Moving to a new line jumps; along a line the underline glides.
    line.classList.toggle("is-jump", lastTop.current !== top);
    lastTop.current = top;
    line.style.opacity = "1";
    line.style.width = `${word.offsetWidth}px`;
    line.style.transform = `translate(${word.offsetLeft}px, ${top}px)`;
  }, [index]);

  return (
    <p ref={textRef} className="meet-spoken">
      {words.map((word, i) => (
        <span key={i}>
          <span
            data-word={i}
            className={i <= index ? "is-said" : undefined}
            onClick={() => onSeek(word.startMs)}
          >
            {word.text}
          </span>{" "}
        </span>
      ))}
      <span ref={lineRef} className="meet-underline" aria-hidden="true" />
    </p>
  );
}

interface Heard {
  audio: RefObject<HTMLAudioElement | null>;
  playing: boolean;
  position: number;
}

const TurnRow = memo(function TurnRow({
  turn,
  look,
  source,
  heard,
  query,
  onSeek,
}: {
  turn: Turn;
  look: SpeakerLook;
  source: SourceKind | null;
  /** Set only on the turn being heard. */
  heard: Heard | null;
  query: string;
  onSeek: (ms: number) => void;
}) {
  const { t } = useTranslation();
  return (
    <li
      className={`meet-turn${heard ? " is-current" : ""}`}
      data-turn={turn.key}
    >
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
        {heard && !query.trim() ? (
          <SpokenText
            words={turn.words}
            audio={heard.audio}
            playing={heard.playing}
            position={heard.position}
            onSeek={onSeek}
          />
        ) : (
          <p>
            {highlightParts(turn.text, query).map((part, index) =>
              part.match ? <mark key={index}>{part.text}</mark> : part.text,
            )}
          </p>
        )}
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

function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable ||
      ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(target.tagName))
  );
}

function MeetingView({
  meet,
  meeting,
}: {
  meet: MeetController;
  meeting: MeetingSummary;
}) {
  const { t, i18n } = useTranslation();
  const [transcript, setTranscript] = useState<TranscriptRevision | null>(null);
  const [transcriptError, setTranscriptError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const processing = meet.status.processing;
  const isProcessing = processing?.session_id === meeting.id;
  const isRecording = meet.status.recording?.session_id === meeting.id;
  const otherRecording = Boolean(meet.status.recording) && !isRecording;
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

  // Space plays and pauses, as in every media app.
  const { toggle } = playback;
  useEffect(() => {
    if (isRecording) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.code !== "Space" || isTyping(event.target)) return;
      event.preventDefault();
      void toggle();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isRecording, toggle]);

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
  const visible = useMemo(() => filterTurns(turns, query), [turns, query]);
  // The turn being heard, once audio is loaded (paused keeps its place).
  const heardKey =
    playback.state === "ready" ? turnAt(turns, playback.position) : null;
  const heardTurn = turns.find((turn) => turn.key === heardKey);
  const now = heardTurn?.speakerId
    ? (looks.get(heardTurn.speakerId) ?? null)
    : null;
  const heard = useMemo<Heard>(
    () => ({
      audio: playback.audioRef,
      playing: playback.playing,
      position: playback.position,
    }),
    [playback.audioRef, playback.playing, playback.position],
  );

  // Keep the turn being heard in view while playing, clear of the player.
  useEffect(() => {
    if (!heardKey || !playback.playing) return;
    const row = document.querySelector<HTMLElement>(
      `[data-turn="${CSS.escape(heardKey)}"]`,
    );
    const page = row?.closest<HTMLElement>(".page");
    if (!row || !page) return;
    const box = row.getBoundingClientRect();
    const view = page.getBoundingClientRect();
    if (box.top >= view.top + 24 && box.bottom <= view.bottom - 140) return;
    const reduce = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    page.scrollTo({
      top: page.scrollTop + box.top - view.top - view.height / 3,
      behavior: reduce ? "auto" : "smooth",
    });
  }, [heardKey, playback.playing]);
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
      openMeeting(null);
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
    weekday: "long",
    month: "long",
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
        <button
          className="button"
          type="button"
          onClick={() => openMeeting(null)}
        >
          {t("ui2.meet.detail.backToRecorder")}
        </button>
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
                    : unknown
                }
                source={
                  multiSource ? (meeting.sources[turn.track] ?? null) : null
                }
                heard={turn.key === heardKey ? heard : null}
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
    <>
      <article
        className="page-wrap meet-wrap meet-view"
        aria-labelledby="meet-view-title"
      >
        <header className="meet-view-head">
          <div className="meet-view-nav">
            <button
              className="meet-back"
              type="button"
              onClick={() => openMeeting(null)}
            >
              <Glyph name="chev-left" />
              {t("ui2.meet.detail.back")}
            </button>
            {otherRecording && (
              <button
                className="meet-live-pill"
                type="button"
                onClick={() => openMeeting(null)}
              >
                <span className="meet-live-dot" aria-hidden="true" />
                {t("ui2.meet.live.recording")}
              </button>
            )}
          </div>
          <div className="meet-view-title">
            <div>
              <h1 id="meet-view-title">{meeting.title}</h1>
              <p className="meet-meta">
                <span>{when}</span>
                <span>{formatClock(meeting.duration_ms)}</span>
                <span>{sourcesText}</span>
              </p>
            </div>
            <div className="meet-view-actions">
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
          </div>
        </header>

        {isProcessing && processing && (
          <ProcessingSteps stage={processing.stage} />
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

        <div className="meet-view-body">{body}</div>
      </article>
      {!isRecording && (
        <Player
          playback={playback}
          fallbackDuration={meeting.duration_ms}
          title={meeting.title}
          now={now}
        />
      )}
    </>
  );
}

export function MeetPage({ meetingId }: { meetingId: string | null }) {
  const { t } = useTranslation();
  const meet = useMeetController();
  const pageRef = useRef<HTMLElement>(null);
  const meeting = meetingId
    ? (meet.meetings.find((m) => m.id === meetingId) ?? null)
    : null;

  // Each view starts at its top.
  useEffect(() => {
    pageRef.current?.scrollTo({ top: 0 });
  }, [meetingId]);

  let content: ReactNode;
  if (meetingId) {
    content = meeting ? (
      <MeetingView key={meeting.id} meet={meet} meeting={meeting} />
    ) : meet.loading ? null : (
      <div className="page-wrap meet-wrap meet-empty">
        <strong>{t("ui2.meet.detail.missing")}</strong>
        <button
          className="button"
          type="button"
          onClick={() => openMeeting(null)}
        >
          {t("ui2.meet.detail.back")}
        </button>
      </div>
    );
  } else {
    content = (
      <div className="page-wrap wide meet-home">
        <div className="meet-capture">
          <header className="meet-head">
            <h1>{t("ui2.meet.name")}</h1>
            <p className="page-subtitle">{t("ui2.meet.subtitle")}</p>
          </header>
          <Studio meet={meet} />
          <SpeakerModelNotice meet={meet} />
          {meet.status.last_error && (
            <div className="meet-callout is-error" role="alert">
              <p>
                <strong>{t("ui2.meet.processFailed")}</strong>{" "}
                {meet.status.last_error}
              </p>
            </div>
          )}
        </div>
        <Recordings meet={meet} />
      </div>
    );
  }

  return (
    <section ref={pageRef} className="page active" data-page-panel="meet">
      {content}
    </section>
  );
}
