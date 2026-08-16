/**
 * Update discovery has two layers: a compact, persistent sidebar affordance and
 * a focused detail dialog for release notes, trust context, and installation.
 * Rust owns discovery so both layers still work when the WebView was absent.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  ArrowRight,
  Check,
  ChevronRight,
  Download,
  ShieldCheck,
  X,
} from "lucide-react";
import { commands, events, type UpdateInfo } from "@/bindings";
import { Markdown } from "@/components/markdown/Markdown";
import "@/components/markdown/markdown.css";

/** Give an ordinary launch a moment before touching the network. */
const CHECK_DELAY_MS = 4000;

const COPY = {
  cardTitle: "Update ready",
  cardAction: "Install & restart",
  cardVersion: (version: string) => `Grain ${version}`,
  retry: "Try again",
  details: "View update details",
  installing: "Preparing update",
  previewComplete: "Preview complete",
  closeDetails: "Close update details",
  updateProgress: "Update installation progress",
  closePreview: "Close preview",
  notNow: "Not now",
  previewInstall: "Preview install",
  dialogTitle: (version: string) => `Grain ${version} is ready`,
  dialogDescription:
    "Review what changed, then install the signed update when you are ready. Grain will restart once the installation finishes.",
  installed: "Installed",
  available: "Available",
  whatsNew: "What’s new",
  noNotes:
    "This release did not include notes. You can still install it securely from Grain.",
  trust: "Signed release · installed in place · restart required",
  previewFinished: "Preview finished",
  previewBody: "No files changed, and Grain was not restarted.",
  installErrorTitle: "The update could not be installed.",
} as const;

type Phase = "idle" | "installing" | "complete";

function releaseDate(value: string | null): string | null {
  if (!value) return null;
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    day: "numeric",
    month: "long",
    year: "numeric",
  }).format(parsed);
}

function UpdateDialog({
  update,
  phase,
  percent,
  error,
  onInstall,
  onClose,
}: {
  update: UpdateInfo;
  phase: Phase;
  percent: number;
  error: string | null;
  onInstall: () => void;
  onClose: () => void;
}) {
  const dialogRef = useRef<HTMLElement>(null);
  const busy = phase === "installing";
  const published = releaseDate(update.date);
  const notes = update.notes?.trim();

  useEffect(() => {
    const returnFocusTo =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const frame = requestAnimationFrame(() => {
      dialogRef.current
        ?.querySelector<HTMLElement>("[data-update-initial-focus]")
        ?.focus();
    });
    return () => {
      cancelAnimationFrame(frame);
      returnFocusTo?.focus();
    };
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab") return;

      const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not(:disabled), a[href], [tabindex]:not([tabindex="-1"])',
      );
      if (!focusable?.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [busy, onClose]);

  return (
    <div
      className="update-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <section
        ref={dialogRef}
        className="update-dialog"
        data-phase={phase}
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-dialog-title"
        aria-describedby="update-dialog-description"
      >
        <button
          className="update-dialog-close"
          type="button"
          aria-label={COPY.closeDetails}
          disabled={busy}
          onClick={onClose}
        >
          <X size={17} strokeWidth={1.8} aria-hidden="true" />
        </button>

        <header className="update-dialog-hero">
          <span className="update-dialog-mark" aria-hidden="true">
            <Download size={20} strokeWidth={1.8} />
          </span>
          <div>
            <h2 id="update-dialog-title">{COPY.dialogTitle(update.version)}</h2>
            <p id="update-dialog-description">{COPY.dialogDescription}</p>
          </div>
        </header>

        <div
          className="update-version-track"
          aria-label={`Updating Grain from ${update.current_version} to ${update.version}`}
        >
          <div>
            <span>{COPY.installed}</span>
            <strong>{update.current_version}</strong>
          </div>
          <ArrowRight size={17} strokeWidth={1.7} aria-hidden="true" />
          <div>
            <span>{COPY.available}</span>
            <strong>{update.version}</strong>
          </div>
        </div>

        <section
          className="update-release-section"
          aria-labelledby="update-release-title"
        >
          <div className="update-release-heading">
            <h3 id="update-release-title">{COPY.whatsNew}</h3>
            {published && (
              <time dateTime={update.date ?? undefined}>{published}</time>
            )}
          </div>
          <div className="update-release-notes">
            {notes ? (
              <Markdown markdown={notes} softBreaks />
            ) : (
              <p>{COPY.noNotes}</p>
            )}
          </div>
        </section>

        <div className="update-trust-row">
          <ShieldCheck size={17} strokeWidth={1.8} aria-hidden="true" />
          <span>{COPY.trust}</span>
        </div>

        {busy && (
          <div className="update-dialog-progress" aria-live="polite">
            <div>
              <span>{COPY.installing}</span>
              <strong>{Math.round(percent)}%</strong>
            </div>
            <div
              className="update-progress"
              role="progressbar"
              aria-label={COPY.updateProgress}
              aria-valuenow={Math.round(percent)}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <span
                style={{ width: `${Math.max(2, Math.min(100, percent))}%` }}
              />
            </div>
          </div>
        )}

        {phase === "complete" && (
          <div className="update-preview-success" role="status">
            <span aria-hidden="true">
              <Check size={15} strokeWidth={2} />
            </span>
            <div>
              <strong>{COPY.previewFinished}</strong>
              <p>{COPY.previewBody}</p>
            </div>
          </div>
        )}

        {error && (
          <div className="update-dialog-error" role="alert">
            <strong>{COPY.installErrorTitle}</strong>
            <span>{error}</span>
          </div>
        )}

        <footer className="update-dialog-actions">
          <button
            className="update-dialog-secondary"
            type="button"
            disabled={busy}
            onClick={onClose}
          >
            {phase === "complete" ? COPY.closePreview : COPY.notNow}
          </button>
          {phase !== "complete" && (
            <button
              className="update-dialog-primary"
              type="button"
              data-update-initial-focus
              disabled={busy}
              onClick={onInstall}
            >
              {busy
                ? `${COPY.installing}…`
                : update.preview
                  ? COPY.previewInstall
                  : error
                    ? COPY.retry
                    : COPY.cardAction}
            </button>
          )}
        </footer>
      </section>
    </div>
  );
}

export function UpdateNotice() {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [percent, setPercent] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const alive = useRef(true);

  const revealUpdate = useCallback((next: UpdateInfo) => {
    setUpdate(next);
    setPhase("idle");
    setPercent(0);
    setError(null);
    setDialogOpen(true);
  }, []);

  useEffect(() => {
    alive.current = true;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unlisten = events.updateAvailable.listen((event) => {
      if (!disposed) revealUpdate(event.payload.update);
    });

    const scheduleCheck = () => {
      if (!alive.current) return;
      timer = setTimeout(() => {
        void commands
          .checkForUpdate(false)
          .then((result) => {
            if (!alive.current || result.status !== "ok" || !result.data)
              return;
            setUpdate(result.data);
          })
          .catch(() => {
            // Automatic checks stay quiet. The manual Settings action reports
            // network failures when the user explicitly asks.
          });
      }, CHECK_DELAY_MS);
    };

    void commands
      .getCachedUpdate()
      .then((cached) => {
        if (!alive.current) return;
        if (cached) revealUpdate(cached);
        else scheduleCheck();
      })
      .catch(scheduleCheck);

    return () => {
      alive.current = false;
      disposed = true;
      if (timer !== undefined) clearTimeout(timer);
      void unlisten.then((off) => off());
    };
  }, [revealUpdate]);

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
    if (!update) return;
    setDialogOpen(true);
    setPhase("installing");
    setError(null);
    setPercent(0);
    void commands.installUpdate().then((result) => {
      // A real success restarts the process and never returns. Preview mode is
      // deliberately the sole successful return path.
      if (!alive.current) return;
      if (result.status === "error") {
        setError(result.error);
        setPhase("idle");
      } else if (update.preview) {
        setPercent(100);
        setPhase("complete");
      }
    });
  }, [update]);

  if (!update) return null;

  return (
    <>
      <aside className="update-notice" data-phase={phase}>
        <button
          className="update-notice-summary"
          type="button"
          aria-haspopup="dialog"
          aria-expanded={dialogOpen}
          aria-label={`${COPY.details}: Grain ${update.version}`}
          onClick={() => setDialogOpen(true)}
        >
          <span className="update-notice-mark" aria-hidden="true">
            <Download size={15} strokeWidth={1.9} />
          </span>
          <span className="update-notice-copy">
            <strong>{COPY.cardTitle}</strong>
            <span>{COPY.cardVersion(update.version)}</span>
          </span>
          <ChevronRight
            className="update-notice-chevron"
            size={15}
            strokeWidth={1.8}
            aria-hidden="true"
          />
        </button>

        {phase === "installing" ? (
          <div className="update-notice-installing" aria-live="polite">
            <div>
              <span>{COPY.installing}</span>
              <strong>{Math.round(percent)}%</strong>
            </div>
            <div className="update-progress" aria-hidden="true">
              <span
                style={{ width: `${Math.max(2, Math.min(100, percent))}%` }}
              />
            </div>
          </div>
        ) : phase === "complete" ? (
          <button
            className="update-notice-complete"
            type="button"
            onClick={() => setDialogOpen(true)}
          >
            <Check size={13} strokeWidth={2} aria-hidden="true" />
            {COPY.previewComplete}
          </button>
        ) : (
          <button className="update-notice-cta" type="button" onClick={install}>
            {error
              ? COPY.retry
              : update.preview
                ? COPY.previewInstall
                : COPY.cardAction}
          </button>
        )}
      </aside>

      {dialogOpen &&
        createPortal(
          <UpdateDialog
            update={update}
            phase={phase}
            percent={percent}
            error={error}
            onInstall={install}
            onClose={() => setDialogOpen(false)}
          />,
          document.body,
        )}
    </>
  );
}
