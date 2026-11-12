import { useCallback, useEffect, useRef, useState } from "react";
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
} from "@/bindings";

/* Recall's layout, in CSS px. The notch shell is sized from these (it cannot
   measure content it clips), so they match the `.agr*` rules in agent.css. */
const HEADER = 34;
const PAD_TOP = 16;
const PAD_BOTTOM = 14;
const ROW = 40;
const ROW_GAP = 8;
const BEFORE_START = 14;
const BUTTON = 34;
const MESSAGE = 24;
const LIVE_TOP = 10;
const CLOCK = 28;
const LIVE_GAP = 12;
const MENU_ITEM = 30;
const MENU_PAD = 8;
const MENU_MAX_ITEMS = 5;

/** The notch's Recall: pick sources, start a meeting, then mute or stop it.
 * Mounted only while Recall is shown, so its listener and clock go with it. */
export function AgentRecall({
  onHeightChange,
}: {
  /** The shell height this view needs, in CSS px. */
  onHeightChange: (height: number) => void;
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
  const systemSupported = sources?.system_audio_supported ?? true;
  const message = error || recording?.error || (!recording && saved);

  useEffect(() => {
    if (recording) {
      onHeightChange(
        HEADER +
          LIVE_TOP +
          CLOCK +
          LIVE_GAP +
          BUTTON +
          PAD_BOTTOM +
          (message ? MESSAGE : 0),
      );
      return;
    }
    const rows = systemSupported ? 2 : 1;
    const content =
      PAD_TOP +
      rows * ROW +
      (rows - 1) * ROW_GAP +
      BEFORE_START +
      BUTTON +
      PAD_BOTTOM +
      (message ? MESSAGE : 0);
    // An open device menu drops below its row; the shell grows to fit it.
    let extra = 0;
    if (menu && sources) {
      const index = menu === "Microphone" ? 0 : 1;
      const items =
        (menu === "Microphone" ? sources.microphones : sources.outputs).length +
        1;
      const top = PAD_TOP + index * (ROW + ROW_GAP) + ROW + 4;
      const height = Math.min(items, MENU_MAX_ITEMS) * MENU_ITEM + MENU_PAD;
      extra = Math.max(0, top + height + 12 - content);
    }
    onHeightChange(HEADER + content + extra);
  }, [recording, systemSupported, message, menu, sources, onHeightChange]);

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
    const isMuted = (kind: SourceKind) =>
      recording.sources.find((source) => source.kind === kind)?.muted ?? true;
    const micMuted = isMuted("Microphone");
    const systemMuted = isMuted("SystemAudio");
    return (
      <div className="agr agr-live">
        <div className="agr-clock" role="timer">
          <span className="agr-rec-dot" aria-hidden="true" />
          <span className="agr-clock-label">{t("agent.recallRecording")}</span>
          <time className="agr-clock-time">{formatElapsed(elapsed)}</time>
        </div>
        <div className="agr-controls">
          <button
            type="button"
            className={`agr-source-btn${micMuted ? " is-off" : ""}`}
            disabled={pending}
            aria-pressed={!micMuted}
            title={t(
              micMuted ? "agent.recallUnmuteMic" : "agent.recallMuteMic",
            )}
            aria-label={t(
              micMuted ? "agent.recallUnmuteMic" : "agent.recallMuteMic",
            )}
            onClick={() => toggle("Microphone", !micMuted)}
          >
            {micMuted ? <MicOff size={15} /> : <Mic size={15} />}
          </button>
          {systemSupported && (
            <button
              type="button"
              className={`agr-source-btn${systemMuted ? " is-off" : ""}`}
              disabled={pending}
              aria-pressed={!systemMuted}
              title={t(
                systemMuted
                  ? "agent.recallUnmuteSystem"
                  : "agent.recallMuteSystem",
              )}
              aria-label={t(
                systemMuted
                  ? "agent.recallUnmuteSystem"
                  : "agent.recallMuteSystem",
              )}
              onClick={() => toggle("SystemAudio", !systemMuted)}
            >
              {systemMuted ? <VolumeOff size={15} /> : <Volume2 size={15} />}
            </button>
          )}
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
        {(error || recording.error) && (
          <div className="agr-error">{error || recording.error}</div>
        )}
      </div>
    );
  }

  const settings = sources?.settings;
  return (
    <div className="agr">
      <SourceRow
        on={<Mic size={15} />}
        off={<MicOff size={15} />}
        label={t("agent.recallMicrophone")}
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
        <SourceRow
          on={<Volume2 size={15} />}
          off={<VolumeOff size={15} />}
          label={t("agent.recallSystemAudio")}
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

/** One source: click its icon or name to turn it on or off (a slashed icon
 * means off); pick its device on the right. */
function SourceRow({
  on,
  off,
  label,
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
    <div className={`agr-row${enabled ? "" : " is-off"}`}>
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
        <span className="agr-row-icon" aria-hidden="true">
          {enabled ? on : off}
        </span>
        <span className="agr-row-label">{label}</span>
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

  useEffect(() => {
    if (!open) return;
    const selected =
      listRef.current?.querySelector<HTMLElement>('[aria-selected="true"]') ??
      listRef.current?.querySelector<HTMLElement>('[role="option"]');
    selected?.focus({ preventScroll: true });
    selected?.scrollIntoView({ block: "nearest" });
    const outside = (event: PointerEvent) => {
      if (!wrapRef.current?.contains(event.target as Node)) onOpenChange(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open, onOpenChange]);

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
      {open && (
        <div
          ref={listRef}
          className="agr-menu"
          role="listbox"
          aria-label={label}
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
        </div>
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
