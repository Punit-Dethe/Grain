import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Mic, MicOff, Square, Volume2, VolumeX } from "lucide-react";
import {
  commands,
  events,
  type MeetSources,
  type MeetStatus,
  type SourceKind,
  type SourceSettings,
} from "@/bindings";

/** The notch's Recall: pick sources, start a meeting, then mute or stop it.
 * Mounted only while Recall is shown, so its listener and clock go with it. */
export function AgentRecall({
  onRecordingChange,
}: {
  onRecordingChange: (recording: boolean) => void;
}) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<MeetStatus | null>(null);
  const [receivedAt, setReceivedAt] = useState(0);
  const [sources, setSources] = useState<MeetSources | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
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
  useEffect(
    () => onRecordingChange(!!recording),
    [recording, onRecordingChange],
  );

  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [recording]);

  const saveSources = useCallback(
    (settings: SourceSettings) => {
      if (!sources) return;
      setSources({ ...sources, settings });
      setError(null);
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
      if (pending) return;
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
    void run(() => commands.meetStartRecording(null));
  };
  const stop = () =>
    void run(() => commands.meetStopRecording()).then((ok) => setSaved(!!ok));
  const toggle = (kind: SourceKind, muted: boolean) =>
    void run(() => commands.meetSetSourceMuted(kind, muted));

  if (recording) {
    const elapsed = recording.elapsed_ms + Math.max(0, now - receivedAt);
    const isMuted = (kind: SourceKind) =>
      recording.sources.find((source) => source.kind === kind)?.muted ?? true;
    const micMuted = isMuted("Microphone");
    const systemMuted = isMuted("SystemAudio");
    const systemSupported = sources?.system_audio_supported ?? true;
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
              {systemMuted ? <VolumeX size={15} /> : <Volume2 size={15} />}
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
        icon={<Mic size={15} />}
        label={t("agent.recallMicrophone")}
        enabled={!!settings?.microphone.enabled}
        deviceId={settings?.microphone.device_id ?? null}
        devices={sources?.microphones ?? []}
        disabled={!settings || pending}
        onChange={(enabled, deviceId) =>
          settings &&
          saveSources({
            ...settings,
            microphone: { enabled, device_id: deviceId },
          })
        }
      />
      {sources?.system_audio_supported !== false && (
        <SourceRow
          icon={<Volume2 size={15} />}
          label={t("agent.recallSystemAudio")}
          enabled={!!settings?.system_audio.enabled}
          deviceId={settings?.system_audio.device_id ?? null}
          devices={sources?.outputs ?? []}
          disabled={!settings || pending}
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

function SourceRow({
  icon,
  label,
  enabled,
  deviceId,
  devices,
  disabled,
  onChange,
}: {
  icon: React.ReactNode;
  label: string;
  enabled: boolean;
  deviceId: string | null;
  devices: { id: string; name: string }[];
  disabled: boolean;
  onChange: (enabled: boolean, deviceId: string | null) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className={`agr-row${enabled ? "" : " is-off"}`}>
      <span className="agr-row-icon" aria-hidden="true">
        {icon}
      </span>
      <span className="agr-row-label">{label}</span>
      <select
        className="agr-select"
        aria-label={label}
        value={deviceId ?? ""}
        disabled={disabled || !enabled}
        onChange={(event) => onChange(enabled, event.target.value || null)}
      >
        <option value="">{t("agent.recallDefaultDevice")}</option>
        {devices.map((device) => (
          <option key={device.id} value={device.id}>
            {device.name}
          </option>
        ))}
      </select>
      <button
        type="button"
        role="switch"
        className="agr-switch"
        aria-checked={enabled}
        aria-label={label}
        disabled={disabled}
        onClick={() => onChange(!enabled, deviceId)}
      >
        <span />
      </button>
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
