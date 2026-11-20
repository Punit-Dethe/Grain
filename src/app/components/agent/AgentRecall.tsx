import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import {
  Check,
  ChevronDown,
  Mic,
  MicOff,
  Square,
  Volume2,
  VolumeOff,
} from "lucide-react";
import {
  commands,
  events,
  type MeetSources,
  type MeetStatus,
  type SourceDevice,
  type SourceKind,
  type SourceSettings,
  type SourceStatus,
} from "@/bindings";
import { useLiveRecording, useMeterFill } from "@/meet/live";

/* Recall's layout, in CSS px. The notch shell is sized from these (it cannot
   measure content it clips), so they match the `.agr*` rules in agent.css. */
const HEADER = 34;
const PAD_TOP = 16;
const PAD_BOTTOM = 14;
/** A source card: icon, name, then its device picker. */
const CARD = 108;
const BEFORE_START = 18;
const BUTTON = 34;
const MESSAGE = 24;
/** While recording: the clock and Stop bar, then the source cards. */
const LIVE_BAR = 34;
const LIVE_GAP = 14;
const LIVE_CARD = 88;
const METER_SEGMENTS = 18;
/** `.agr-menu` max-height; shorter when the window has less room below. */
const MENU_MAX_HEIGHT = 158;
/** `.agr-menu` width. */
const MENU_WIDTH = 236;

/** The notch's Recall: pick sources, start a meeting, then mute or stop it.
 * Mounted only while Recall is shown, so its listener and clock go with it. */
export function AgentRecall({
  onHeightChange,
  onMenuChange,
}: {
  /** The shell height this view needs, in CSS px. */
  onHeightChange: (height: number) => void;
  /** A device menu is open (it floats below the shell). */
  onMenuChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<MeetStatus | null>(null);
  const [receivedAt, setReceivedAt] = useState(0);
  const [sources, setSources] = useState<MeetSources | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [menu, setMenu] = useState<SourceKind | null>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    let dead = false;
    let release: (() => void) | undefined;
    const take = (next: MeetStatus) => {
      if (dead) return;
      setStatus(next);
      setReceivedAt(Date.now());
      setNow(Date.now());
    };
    void commands.meetStatus().then(take).catch(console.error);
    void events.meetStatusChanged
      .listen(({ payload }) => take(payload))
      .then((unlisten) => {
        if (dead) unlisten();
        else release = unlisten;
      });
    void commands.meetSources().then((result) => {
      if (!dead && result.status === "ok") setSources(result.data);
    });
    return () => {
      dead = true;
      release?.();
    };
  }, []);

  const recording = status?.recording ?? null;
  // Levels change many times a second; poll them only while recording.
  const live = useLiveRecording(recording);
  const systemSupported = sources?.system_audio_supported ?? true;
  const message = error || recording?.error || (!recording && saved);

  useEffect(() => {
    if (recording) {
      onHeightChange(
        HEADER +
          PAD_TOP +
          LIVE_BAR +
          LIVE_GAP +
          LIVE_CARD +
          PAD_BOTTOM +
          (message ? MESSAGE : 0),
      );
      return;
    }
    onHeightChange(
      HEADER +
        PAD_TOP +
        CARD +
        BEFORE_START +
        BUTTON +
        PAD_BOTTOM +
        (message ? MESSAGE : 0),
    );
  }, [recording, message, onHeightChange]);

  // The device menu floats outside the shell; the window must take clicks there.
  useEffect(() => onMenuChange(menu !== null), [menu, onMenuChange]);
  useEffect(() => () => onMenuChange(false), [onMenuChange]);

  useEffect(() => {
    if (!recording) return;
    setMenu(null);
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [recording]);

  const saveSources = useCallback(
    (settings: SourceSettings) => {
      if (!sources) return;
      setSources({ ...sources, settings });
      setError(null);
      setSaved(false);
      void commands.meetSetSources(settings).then((result) => {
        if (result.status !== "ok") setError(result.error);
      });
    },
    [sources],
  );

  const run = useCallback(
    async (
      work: () => Promise<
        { status: "ok" } | { status: "error"; error: string }
      >,
    ) => {
      if (pending) return false;
      setPending(true);
      setError(null);
      try {
        const result = await work();
        if (result.status !== "ok") setError(result.error);
        return result.status === "ok";
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
        return false;
      } finally {
        setPending(false);
      }
    },
    [pending],
  );

  const start = () => {
    setSaved(false);
    setMenu(null);
    void run(() => commands.meetStartRecording(null));
  };
  const stop = () =>
    void run(() => commands.meetStopRecording()).then((ok) => setSaved(ok));
  const toggle = (kind: SourceKind, muted: boolean) =>
    void run(() => commands.meetSetSourceMuted(kind, muted));

  if (recording) {
    const elapsed = recording.elapsed_ms + Math.max(0, now - receivedAt);
    const sourceOf = (kind: SourceKind) =>
      (live ?? recording).sources.find((source) => source.kind === kind) ??
      null;
    return (
      <div className="agr agr-live">
        <div className="agr-live-bar">
          <div className="agr-clock" role="timer">
            <span className="agr-rec-dot" aria-hidden="true" />
            <span className="agr-clock-label">
              {t("agent.recallRecording")}
            </span>
            <time className="agr-clock-time">{formatElapsed(elapsed)}</time>
          </div>
          <button
            type="button"
            className="agr-stop"
            disabled={pending}
            onClick={stop}
          >
            <Square size={11} fill="currentColor" />
            {t("agent.recallStop")}
          </button>
        </div>
        <div className="agr-cards">
          <LiveSourceCard
            on={<Mic size={17} />}
            off={<MicOff size={17} />}
            label={t("agent.recallMicrophone")}
            tone="mic"
            source={sourceOf("Microphone")}
            disabled={pending}
            muteLabel={t("agent.recallMuteMic")}
            unmuteLabel={t("agent.recallUnmuteMic")}
            onToggle={(muted) => toggle("Microphone", muted)}
          />
          {systemSupported && (
            <LiveSourceCard
              on={<Volume2 size={17} />}
              off={<VolumeOff size={17} />}
              label={t("agent.recallSystemAudio")}
              tone="system"
              source={sourceOf("SystemAudio")}
              disabled={pending}
              muteLabel={t("agent.recallMuteSystem")}
              unmuteLabel={t("agent.recallUnmuteSystem")}
              onToggle={(muted) => toggle("SystemAudio", muted)}
            />
          )}
        </div>
        {(error || (live ?? recording).error) && (
          <div className="agr-error">{error || (live ?? recording).error}</div>
        )}
      </div>
    );
  }

  const settings = sources?.settings;
  return (
    <div className="agr">
      <div className="agr-cards">
        <SourceCard
          on={<Mic size={17} />}
          off={<MicOff size={17} />}
          label={t("agent.recallMicrophone")}
          tone="mic"
          enabled={!!settings?.microphone.enabled}
          deviceId={settings?.microphone.device_id ?? null}
          devices={sources?.microphones ?? []}
          disabled={!settings || pending}
          menuOpen={menu === "Microphone"}
          onMenuOpen={(open) => setMenu(open ? "Microphone" : null)}
          onChange={(enabled, deviceId) =>
            settings &&
            saveSources({
              ...settings,
              microphone: { enabled, device_id: deviceId },
            })
          }
        />
        {systemSupported && (
          <SourceCard
            on={<Volume2 size={17} />}
            off={<VolumeOff size={17} />}
            label={t("agent.recallSystemAudio")}
            tone="system"
            enabled={!!settings?.system_audio.enabled}
            deviceId={settings?.system_audio.device_id ?? null}
            devices={sources?.outputs ?? []}
            disabled={!settings || pending}
            menuOpen={menu === "SystemAudio"}
            onMenuOpen={(open) => setMenu(open ? "SystemAudio" : null)}
            onChange={(enabled, deviceId) =>
              settings &&
              saveSources({
                ...settings,
                system_audio: { enabled, device_id: deviceId },
              })
            }
          />
        )}
      </div>
      <button
        type="button"
        className="agr-start"
        disabled={!settings || pending}
        onClick={start}
      >
        {t(pending ? "agent.recallStarting" : "agent.recallStart")}
      </button>
      {error ? (
        <div className="agr-error">{error}</div>
      ) : (
        saved && <div className="agr-note">{t("agent.recallSaved")}</div>
      )}
    </div>
  );
}

/** One source as a card: anywhere on it turns the source on or off (a
 * slashed icon means off), except its device picker at the bottom. The toggle
 * is one button whose hit area covers the card (see `.agr-source::after`), so
 * the picker is a sibling, never a button inside a button. */
function SourceCard({
  on,
  off,
  label,
  tone,
  enabled,
  deviceId,
  devices,
  disabled,
  menuOpen,
  onMenuOpen,
  onChange,
}: {
  on: React.ReactNode;
  off: React.ReactNode;
  label: string;
  /** Which source (a class hook; both share one monochrome look). */
  tone: "mic" | "system";
  enabled: boolean;
  deviceId: string | null;
  devices: SourceDevice[];
  disabled: boolean;
  menuOpen: boolean;
  onMenuOpen: (open: boolean) => void;
  onChange: (enabled: boolean, deviceId: string | null) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className={`agr-card is-${tone}${enabled ? "" : " is-off"}`}>
      <button
        type="button"
        className="agr-source"
        aria-pressed={enabled}
        disabled={disabled}
        title={t(enabled ? "agent.recallTurnOff" : "agent.recallTurnOn", {
          source: label,
        })}
        onClick={() => onChange(!enabled, deviceId)}
      >
        <span className="agr-card-icon" aria-hidden="true">
          {enabled ? on : off}
        </span>
        <span className="agr-card-label">{label}</span>
      </button>
      <DeviceMenu
        label={label}
        value={deviceId}
        devices={devices}
        disabled={disabled}
        open={menuOpen}
        onOpenChange={onMenuOpen}
        onPick={(id) => {
          onMenuOpen(false);
          if (id !== deviceId) onChange(enabled, id);
        }}
      />
    </div>
  );
}

/** A source while recording: its icon, name and live level. Anywhere on the
 * card mutes or unmutes it. A source left off at the start is muted here and
 * can be turned on. */
function LiveSourceCard({
  on,
  off,
  label,
  tone,
  source,
  disabled,
  muteLabel,
  unmuteLabel,
  onToggle,
}: {
  on: React.ReactNode;
  off: React.ReactNode;
  label: string;
  tone: "mic" | "system";
  source: SourceStatus | null;
  disabled: boolean;
  muteLabel: string;
  unmuteLabel: string;
  onToggle: (muted: boolean) => void;
}) {
  const muted = source?.muted ?? true;
  const fill = useMeterFill(source ?? SILENT);
  const lit = fill < 0.04 ? 0 : Math.ceil(fill * METER_SEGMENTS);
  return (
    <button
      type="button"
      className={`agr-card agr-live-card is-${tone}${muted ? " is-off" : ""}`}
      aria-pressed={!muted}
      title={muted ? unmuteLabel : muteLabel}
      disabled={disabled}
      onClick={() => onToggle(!muted)}
    >
      <span className="agr-live-head">
        <span className="agr-card-icon" aria-hidden="true">
          {muted ? off : on}
        </span>
        <span className="agr-card-label">{label}</span>
      </span>
      <span className="agr-meter" aria-hidden="true">
        {Array.from({ length: METER_SEGMENTS }, (_, index) => (
          <i key={index} className={index < lit ? "is-lit" : undefined} />
        ))}
      </span>
    </button>
  );
}

/** A source not in the recording: silent and muted. */
const SILENT = { level: 0, muted: true };

/** A compact device picker: the current device, and a short list below it. */
function DeviceMenu({
  label,
  value,
  devices,
  disabled,
  open,
  onOpenChange,
  onPick,
}: {
  label: string;
  value: string | null;
  devices: SourceDevice[];
  disabled: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onPick: (id: string | null) => void;
}) {
  const { t } = useTranslation();
  const wrapRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const options = [
    { id: "", name: t("agent.recallDefaultDevice") },
    ...devices,
  ];
  const current =
    devices.find((device) => device.id === value)?.name ??
    t("agent.recallDefaultDevice");
  // Where the list floats, measured from the trigger once it opens.
  const [place, setPlace] = useState<{
    top: number;
    left: number;
    maxHeight: number;
    host: Element;
  } | null>(null);

  useLayoutEffect(() => {
    const trigger = triggerRef.current;
    if (!open || !trigger) {
      setPlace(null);
      return;
    }
    const rect = trigger.getBoundingClientRect();
    const top = rect.bottom + 6;
    setPlace({
      top,
      // Under the trigger, kept inside the window (the left card's menu
      // would otherwise run off the left edge, the right card's off the right).
      left: Math.max(
        8,
        Math.min(rect.left, window.innerWidth - MENU_WIDTH - 8),
      ),
      maxHeight: Math.max(
        60,
        Math.min(MENU_MAX_HEIGHT, window.innerHeight - top - 8),
      ),
      host: trigger.closest(".agent-panel-root") ?? document.body,
    });
  }, [open]);

  useEffect(() => {
    if (!place) return;
    const selected =
      listRef.current?.querySelector<HTMLElement>('[aria-selected="true"]') ??
      listRef.current?.querySelector<HTMLElement>('[role="option"]');
    selected?.focus({ preventScroll: true });
    selected?.scrollIntoView({ block: "nearest" });
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        !wrapRef.current?.contains(target) &&
        !listRef.current?.contains(target)
      )
        onOpenChange(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [place, onOpenChange]);

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (!open) return;
    if (event.key === "Escape") {
      // Close the menu only, not the notch.
      event.preventDefault();
      onOpenChange(false);
      triggerRef.current?.focus();
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const items = Array.from(
      listRef.current?.querySelectorAll<HTMLElement>('[role="option"]') ?? [],
    );
    const at = items.indexOf(document.activeElement as HTMLElement);
    const next = event.key === "ArrowDown" ? at + 1 : at - 1;
    items[(next + items.length) % items.length]?.focus();
  };

  return (
    <div className="agr-menu-wrap" ref={wrapRef} onKeyDown={onKeyDown}>
      <button
        ref={triggerRef}
        type="button"
        className="agr-menu-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${label}: ${current}`}
        title={current}
        disabled={disabled}
        onClick={() => onOpenChange(!open)}
      >
        <span className="agr-menu-value">{current}</span>
        <ChevronDown size={13} aria-hidden="true" />
      </button>
      {/* Rendered outside the shell so it floats over the screen below the
          notch instead of making the notch grow to hold it. React still
          bubbles its keys to the wrapper above. */}
      {open &&
        place &&
        createPortal(
          <div
            ref={listRef}
            className="agr-menu"
            role="listbox"
            aria-label={label}
            style={{
              top: place.top,
              left: place.left,
              maxHeight: place.maxHeight,
            }}
          >
            {options.map((option) => {
              const selected = (value ?? "") === option.id;
              return (
                <button
                  key={option.id || "default"}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  className="agr-menu-item"
                  title={option.name}
                  onClick={() => onPick(option.id || null)}
                >
                  <span>{option.name}</span>
                  {selected && <Check size={13} aria-hidden="true" />}
                </button>
              );
            })}
          </div>,
          place.host,
        )}
    </div>
  );
}

function formatElapsed(ms: number) {
  const total = Math.floor(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  const pad = (value: number) => value.toString().padStart(2, "0");
  return hours
    ? `${hours}:${pad(minutes)}:${pad(seconds)}`
    : `${pad(minutes)}:${pad(seconds)}`;
}
