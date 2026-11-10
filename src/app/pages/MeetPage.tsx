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
import { MicOff, VolumeOff } from "lucide-react";
import { toast } from "sonner";
import type {
  MeetingSummary,
  SourceDevice,
  SourceKind,
  SourceSettings,
  Stage,
  TrackStatus,
  TranscriptRevision,
} from "@/bindings";
import { Dropdown } from "../components/ui/Dropdown";
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
  turnAt,
  wordAt,
  type RecordingFilters,
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
/* Recording controls                                                        */
/* ------------------------------------------------------------------------ */

function SourceControl({
  kind,
  choice,
  devices = [],
  pending,
  locked,
  unsupported = false,
  onChange,
}: {
  kind: SourceKind;
  choice: SourceSettings["microphone"] | undefined;
  devices?: SourceDevice[];
  pending: boolean;
  locked: boolean;
  unsupported?: boolean;
  onChange: (choice: SourceSettings["microphone"]) => void;
}) {
  const { t } = useTranslation();
  const name = sourceName(kind, t);
  const MutedIcon = kind === "Microphone" ? MicOff : VolumeOff;
  const enabled = Boolean(choice?.enabled && !unsupported);
  const pickerDisabled = !choice || pending || unsupported;
  const toggleDisabled = pickerDisabled || locked;
  const selected = choice?.device_id ?? "";
  const fallback = devices.find((device) => device.is_default);
  const options = [
    {
      value: "",
      label: fallback
        ? t("ui2.meet.sources.followDefaultNamed", { name: fallback.name })
        : t("ui2.meet.sources.followDefault"),
    },
    ...devices.map((device) => ({ value: device.id, label: device.name })),
  ];
  if (selected && !devices.some((device) => device.id === selected)) {
    options.push({
      value: selected,
      label: t("ui2.meet.sources.disconnected"),
    });
  }

  let toggleTitle = enabled
    ? t("ui2.meet.sources.turnOff", { source: name })
    : t("ui2.meet.sources.turnOn", { source: name });
  if (unsupported) {
    toggleTitle = t("ui2.meet.sources.systemUnavailable");
  } else if (locked) {
    toggleTitle = t("ui2.meet.sources.keepOne");
  }

  return (
    <div className="meet-source-control">
      <button
        className="meet-source-toggle"
        type="button"
        aria-pressed={enabled}
        aria-disabled={toggleDisabled}
        aria-label={name}
        title={toggleTitle}
        onClick={() => {
          if (choice && !toggleDisabled)
            onChange({ ...choice, enabled: !enabled });
        }}
      >
        {enabled ? (
          <Glyph name={SOURCE_ICON[kind]} />
        ) : (
          <MutedIcon className="icon sm" aria-hidden="true" />
        )}
      </button>
      <Dropdown
        compact
        options={options}
        selectedValue={selected}
        disabled={pickerDisabled}
        ariaLabel={
          kind === "Microphone"
            ? t("ui2.meet.sources.microphoneDevice")
            : t("ui2.meet.sources.systemAudioDevice")
        }
        onSelect={(value) => {
          if (choice && value !== selected)
            onChange({ ...choice, device_id: value || null });
        }}
      />
    </div>
  );
}

type CaptureCommand = "sources" | "start" | "stop";

function CaptureDock({ meet }: { meet: MeetController }) {
  const { t } = useTranslation();
  const [pending, setPending] = useState<CaptureCommand | null>(null);
  const commandInFlight = useRef(false);
  const currentRecording = meet.status.recording;
  const live = useLiveRecording(currentRecording);
  const recording =
    live?.session_id === currentRecording?.session_id ? live : currentRecording;
  const sources = meet.sources;
  const settings = sources?.settings;
  const micOn = Boolean(settings?.microphone.enabled);
  const systemOn = Boolean(
    settings?.system_audio.enabled && sources?.system_audio_supported,
  );

  const run = async (
    command: CaptureCommand,
    action: () => Promise<unknown>,
  ) => {
    if (commandInFlight.current) return;
    commandInFlight.current = true;
    setPending(command);
    try {
      await action();
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      commandInFlight.current = false;
      setPending(null);
    }
  };

  const recordDisabled =
    pending !== null || (!recording && (!settings || (!micOn && !systemOn)));
  return (
    <section
      className="meet-capture-dock"
      aria-label={t("ui2.meet.studio.label")}
    >
      {recording?.error && (
        <p className="meet-inline-error" role="alert">
          {recording.error}
        </p>
      )}
      <div className="meet-toolbar">
        {recording ? (
          <span
            className="meet-record-clock"
            role="timer"
            aria-label={t("ui2.meet.live.label")}
          >
            {formatClock(recording.elapsed_ms)}
          </span>
        ) : (
          <div
            className="meet-source-controls"
            role="group"
            aria-busy={!sources}
            aria-label={t("ui2.meet.sources.label")}
          >
            <SourceControl
              kind="Microphone"
              choice={settings?.microphone}
              devices={sources?.microphones}
              pending={pending !== null}
              locked={micOn && !systemOn}
              onChange={(microphone) => {
                if (settings)
                  void run("sources", () =>
                    meet.setSources({ ...settings, microphone }),
                  );
              }}
            />
            <SourceControl
              kind="SystemAudio"
              choice={settings?.system_audio}
              devices={sources?.outputs}
              pending={pending !== null}
              locked={systemOn && !micOn}
              unsupported={sources?.system_audio_supported === false}
              onChange={(system_audio) => {
                if (settings)
                  void run("sources", () =>
                    meet.setSources({ ...settings, system_audio }),
                  );
              }}
            />
          </div>
        )}
        <button
          className="button primary meet-record-action"
          type="button"
          aria-disabled={recordDisabled}
          onClick={() => {
            if (!recordDisabled) {
              void run(
                recording ? "stop" : "start",
                recording
                  ? meet.stopRecording
                  : () => meet.startRecording(null),
              );
            }
          }}
        >
          <span
            className={
              recording ? "meet-record-mark is-stop" : "meet-record-mark"
            }
            aria-hidden="true"
          />
          {recording
            ? t("ui2.meet.studio.stop")
            : pending === "start"
              ? t("ui2.meet.studio.starting")
              : t("ui2.meet.studio.start")}
        </button>
      </div>
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
      <span className="meet-lead" aria-hidden="true">
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
  if (meeting.speaker_count > 0) {
    const shown = Math.min(meeting.speaker_count, 3);
    const extra = meeting.speaker_count - shown;
    return (
      <span className="meet-lead-stack" aria-hidden="true">
        {Array.from({ length: shown }, (_, index) => (
          <span
            key={index}
            className={`meet-avatar is-stack tone-${SPEAKER_TONES[index % SPEAKER_TONES.length]}`}
          >
            {index + 1}
          </span>
        ))}
        {extra > 0 && (
          <span className="meet-avatar is-stack is-more">+{extra}</span>
        )}
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

function Recordings({
  meet,
  filters,
  onFiltersChange,
}: {
  meet: MeetController;
  filters: RecordingFilters;
  onFiltersChange: (filters: RecordingFilters) => void;
}) {
  const { t, i18n } = useTranslation();
  const recordingId = meet.status.recording?.session_id;
  const now = useSecondTick(Boolean(recordingId));
  const groups = useMemo(
    () => groupByDay(meet.meetings, new Date(), filters),
    [meet.meetings, filters],
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
        <span className="meet-source-icon">
          <Glyph name="mic" />
        </span>
        <strong>{t("ui2.meet.list.emptyTitle")}</strong>
        <p>{t("ui2.meet.list.empty")}</p>
      </div>
    );
  } else if (groups.length === 0) {
    body = (
      <p className="meet-list-state" role="status">
        {t("ui2.meet.list.noMatches")}
      </p>
    );
  } else {
    const processingId = meet.status.processing?.session_id;
    body = groups.map((group) => (
      <section key={group.key} className="meet-day">
        <h2>
          {group.kind === "today"
            ? t("ui2.meet.list.today")
            : group.kind === "yesterday"
              ? t("ui2.meet.list.yesterday")
              : dayFormat.format(group.day)}
        </h2>
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
            const sourcesText = [...new Set(meeting.sources)]
              .map((kind) => sourceName(kind, t))
              .join(" + ");
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
                    <span className="meet-row-top">
                      <span className="meet-row-title">{meeting.title}</span>
                      <span className="meet-row-duration">
                        {formatClock(
                          state === "recording"
                            ? now - meeting.created_unix_ms
                            : meeting.duration_ms,
                        )}
                      </span>
                    </span>
                    {state === "recording" ? (
                      <span className="sr-only">
                        {t("ui2.meet.list.state.recording")}
                      </span>
                    ) : (
                      <span className="meet-row-preview">
                        {state === "transcribed" && meeting.preview
                          ? meeting.preview
                          : t(`ui2.meet.list.state.${state}`)}
                      </span>
                    )}
                    <span className="meet-row-meta">
                      <time>{timeFormat.format(meeting.created_unix_ms)}</time>
                      {meeting.speaker_count > 0 && (
                        <span>
                          {t("ui2.meet.list.speakers", {
                            count: meeting.speaker_count,
                          })}
                        </span>
                      )}
                      <span>{sourcesText}</span>
                    </span>
                  </span>
                  <span className="meet-row-go" aria-hidden="true">
                    <Glyph name="chev-right" />
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      </section>
    ));
  }

  return (
    <section
      className="meet-recordings"
      aria-label={t("ui2.meet.list.heading")}
    >
      <div className="meet-recordings-controls">
        <label className="search-field">
          <Glyph name="search" />
          <input
            type="search"
            value={filters.query}
            onChange={(event) =>
              onFiltersChange({ ...filters, query: event.target.value })
            }
            placeholder={t("ui2.meet.list.search")}
            aria-label={t("ui2.meet.list.search")}
          />
        </label>
        <select
          className="select meet-recordings-sort"
          value={filters.oldestFirst ? "oldest" : "newest"}
          onChange={(event) =>
            onFiltersChange({
              ...filters,
              oldestFirst: event.target.value === "oldest",
            })
          }
          aria-label={t("ui2.meet.list.sortByDate")}
        >
          <option value="newest">{t("ui2.meet.list.newestFirst")}</option>
          <option value="oldest">{t("ui2.meet.list.oldestFirst")}</option>
        </select>
      </div>
      <SpeakerModelNotice meet={meet} />
      {meet.status.last_error && (
        <div className="meet-callout is-error" role="alert">
          <p>
            <strong>{t("ui2.meet.processFailed")}</strong>{" "}
            {meet.status.last_error}
          </p>
        </div>
      )}
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
  const [filters, setFilters] = useState<RecordingFilters>({
    query: "",
    oldestFirst: false,
  });
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
      <>
        <div className="page-wrap meet-wrap">
          <header>
            <h1>{t("ui2.meet.name")}</h1>
            <p className="page-subtitle">{t("ui2.meet.subtitle")}</p>
          </header>
          <Recordings
            meet={meet}
            filters={filters}
            onFiltersChange={setFilters}
          />
        </div>
        <CaptureDock meet={meet} />
      </>
    );
  }

  return (
    <section ref={pageRef} className="page active" data-page-panel="meet">
      {content}
    </section>
  );
}
