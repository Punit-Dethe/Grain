/**
 * The update notice that sits directly above the model card in the sidebar.
 *
 * It is deliberately the same shape as the model card below it: same width, same
 * corner radius, a bold first line and a quiet second. The only thing that
 * separates them is an accent left edge, so a new release reads as one more
 * piece of app status rather than an alert bolted onto the rail.
 *
 * The notice does not exist unless there is something to say. No "you are up to
 * date" state lives here — that answer belongs to the Check-now button in
 * Settings, where the user asked the question. A row that is present-but-empty
 * most of the time is a row the eye learns to skip.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { commands, events, type UpdateInfo } from "@/bindings";

/** Give launch a moment before touching the network. */
const CHECK_DELAY_MS = 4000;

const COPY = {
  title: "Update available",
  install: "Install and restart",
  retry: "Retry update",
  step: (from: string, to: string) => `${from} → ${to}`,
};

type Phase = "idle" | "installing";

export function UpdateNotice() {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [percent, setPercent] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    // `force: false` — the backend honours `update_checks_enabled` and answers
    // "nothing to show" when the user has turned checks off.
    const timer = setTimeout(() => {
      void commands
        .checkForUpdate(false)
        .then((result) => {
          if (!alive.current || result.status !== "ok") return;
          setUpdate(result.data);
        })
        .catch(() => {
          // A failed check is not worth a message. The user did not ask.
        });
    }, CHECK_DELAY_MS);

    return () => {
      alive.current = false;
      clearTimeout(timer);
    };
  }, []);

  useEffect(() => {
    if (phase !== "installing") return;
    let disposed = false;
    const unlisten = events.updateDownloadProgress.listen((event) => {
      if (!disposed) setPercent(event.payload.percentage);
    });
    return () => {
      disposed = true;
      void unlisten.then((off) => off());
    };
  }, [phase]);

  const install = useCallback(() => {
    setPhase("installing");
    setError(null);
    setPercent(0);
    void commands.installUpdate().then((result) => {
      // On success the app restarts and this component never runs again, so
      // only the failure path needs handling.
      if (result.status === "error" && alive.current) {
        setError(result.error);
        setPhase("idle");
      }
    });
  }, []);

  if (!update) return null;

  return (
    <div className="update-notice" data-phase={phase}>
      <div className="update-notice-head">
        <span className="update-dot" aria-hidden="true" />
        <strong>{COPY.title}</strong>
      </div>
      <p>{COPY.step(update.current_version, update.version)}</p>
      {phase === "installing" ? (
        <div
          className="update-progress"
          role="progressbar"
          aria-valuenow={Math.round(percent)}
          aria-valuemin={0}
          aria-valuemax={100}
        >
          <span style={{ width: `${Math.max(2, Math.min(100, percent))}%` }} />
        </div>
      ) : (
        <button className="update-notice-cta" type="button" onClick={install}>
          {error ? COPY.retry : COPY.install}
        </button>
      )}
      {error && <p className="update-notice-error">{error}</p>}
    </div>
  );
}
