import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AlertTriangle,
  ArrowRight,
  Check,
  CheckCircle2,
  Clipboard,
  Download,
  LoaderCircle,
  Route,
  Search,
  Send,
  ShieldCheck,
  TextCursorInput,
  X,
  XCircle,
  Replace as ReplaceIcon,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  ExtensionViewContent,
  ExtensionChoiceCandidate,
  ExtensionViewInit,
} from "@/bindings";
import {
  filterChoiceCandidates,
  resolveChoiceSelection,
} from "./extension-view-model";
import "./extension-view.css";

type Session = ExtensionViewInit;
type Content = ExtensionViewContent;

const PRESENT_EVENT = "extension-view://present";
const THEME_EVENT = "theme-changed";
const COPY = {
  mode: "Grain Extension Mode",
  close: "Close",
  copied: "Copied",
  copy: "Copy result",
  openExtensions: "Open Extensions",
  insert: "Insert",
  replace: "Replace selection",
  empty: "This extension did not provide anything to review.",
  error: "The extension surface could not be loaded.",
  retry: "Close this window and try the request again.",
  safeBoundary: "Secure Grain surface",
  finishing: "Finishing your request",
  finding: "Finding the right extension",
  routingHint:
    "Grain is matching your words locally against the extensions you approved.",
  choose: "Choose an extension",
  chooseHint: "The request stays with Grain until you choose where it goes.",
  search: "Search extensions",
  continue: "Continue",
  chooserKeys: "↑↓ to choose · Enter to continue",
  recommended: "Recommended",
  noMatch:
    "No confident match. Search or choose from your installed extensions.",
  noResults: "No extensions match that search.",
  noCandidates: "No searchable extensions are available for this request.",
  languageModel:
    "On-device language matching is not installed, so Grain matched names only.",
  downloadModel: "Download language model",
  opening: "Opening",
  autoSending: "Sent automatically to",
  runningHint:
    "The extension is working on your request. Grain will keep the result in this window.",
} as const;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function CandidateIcon({ candidate }: { candidate: ExtensionChoiceCandidate }) {
  const initial = candidate.name.trim().charAt(0).toLocaleUpperCase() || "·";
  const icon = candidate.icon?.startsWith("data:image/png;base64,")
    ? candidate.icon
    : null;
  return (
    <span className="ev-candidate-icon" aria-hidden="true">
      <span>{initial}</span>
      {icon ? (
        <img
          alt=""
          onError={(event) => {
            event.currentTarget.hidden = true;
          }}
          src={icon}
        />
      ) : null}
    </span>
  );
}

function RoutingView({
  content,
}: {
  content: Extract<ExtensionViewContent, { kind: "routing" }>;
}) {
  return (
    <section className="ev-routing ev-stage" aria-live="polite">
      <div className="ev-routing-orbit" aria-hidden="true">
        <Route size={22} strokeWidth={1.8} />
        <span />
      </div>
      <h1>{content.request_preview ? COPY.finding : COPY.finishing}</h1>
      <p>{COPY.routingHint}</p>
      {content.request_preview ? (
        <blockquote>{content.request_preview}</blockquote>
      ) : null}
      <div className="ev-routing-progress" aria-hidden="true">
        <span />
      </div>
    </section>
  );
}

function ChoiceView({
  busy,
  content,
  modelBusy,
  onChoose,
  onDownloadModel,
}: {
  busy: boolean;
  content: Extract<ExtensionViewContent, { kind: "choose" }>;
  modelBusy: boolean;
  onChoose: (extensionId: string) => void;
  onDownloadModel: () => void;
}) {
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const filtered = useMemo(
    () => filterChoiceCandidates(content.candidates, query),
    [content.candidates, query],
  );
  const hasRecommendation = content.candidates.some(
    (candidate) => candidate.signal !== "none",
  );

  useEffect(() => {
    setQuery("");
    setSelectedId(resolveChoiceSelection(content.candidates, "", null));
  }, [content.presentation_id, content.candidates]);

  useEffect(() => {
    setSelectedId((current) =>
      resolveChoiceSelection(filtered, query, current),
    );
  }, [filtered, query]);

  const move = (delta: number) => {
    if (filtered.length === 0) return;
    const current = filtered.findIndex(
      (candidate) => candidate.extensionId === selectedId,
    );
    const next =
      current < 0
        ? delta > 0
          ? 0
          : filtered.length - 1
        : (current + delta + filtered.length) % filtered.length;
    setSelectedId(filtered[next].extensionId);
    document
      .getElementById(`ev-choice-${filtered[next].extensionId}`)
      ?.scrollIntoView({ block: "nearest" });
  };

  const chooseSelected = () => {
    if (!busy && selectedId) onChoose(selectedId);
  };

  return (
    <section
      className="ev-chooser ev-stage"
      onKeyDown={(event) => {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          move(1);
        } else if (event.key === "ArrowUp") {
          event.preventDefault();
          move(-1);
        } else if (
          event.key === "Enter" &&
          event.target instanceof HTMLInputElement
        ) {
          event.preventDefault();
          chooseSelected();
        }
      }}
    >
      <div className="ev-choice-heading">
        <div>
          <h1>{COPY.choose}</h1>
          <p>{COPY.chooseHint}</p>
        </div>
        <span className="ev-choice-count">{content.candidates.length}</span>
      </div>

      <p className="ev-request-preview">{content.request_preview}</p>

      <label className="ev-choice-search">
        <Search aria-hidden="true" size={16} strokeWidth={1.8} />
        <span className="sr-only">{COPY.search}</span>
        <input
          aria-activedescendant={
            selectedId ? `ev-choice-${selectedId}` : undefined
          }
          aria-autocomplete="list"
          aria-controls="ev-choice-list"
          aria-expanded="true"
          aria-label={COPY.search}
          autoComplete="off"
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder={COPY.search}
          role="combobox"
          spellCheck={false}
          value={query}
        />
      </label>

      {!hasRecommendation && !query ? (
        <p className="ev-choice-notice" role="status">
          <AlertTriangle aria-hidden="true" size={15} />
          {COPY.noMatch}
        </p>
      ) : null}

      <div
        aria-label={COPY.choose}
        className="ev-choice-list"
        id="ev-choice-list"
        role="listbox"
      >
        {filtered.map((candidate) => {
          const recommended = candidate.signal !== "none";
          const selected = candidate.extensionId === selectedId;
          return (
            <button
              aria-selected={selected}
              className="ev-choice-row"
              data-recommended={recommended ? "true" : "false"}
              id={`ev-choice-${candidate.extensionId}`}
              key={candidate.extensionId}
              onClick={() => setSelectedId(candidate.extensionId)}
              onDoubleClick={() => {
                if (!busy) onChoose(candidate.extensionId);
              }}
              role="option"
              type="button"
            >
              <CandidateIcon candidate={candidate} />
              <span className="ev-choice-copy">
                <span className="ev-choice-name">
                  {candidate.name}
                  {recommended ? <small>{COPY.recommended}</small> : null}
                </span>
                <span>{candidate.purpose || candidate.extensionId}</span>
              </span>
              <ArrowRight aria-hidden="true" size={16} strokeWidth={1.8} />
            </button>
          );
        })}
        {filtered.length === 0 ? (
          <div className="ev-choice-empty" role="status">
            {content.candidates.length === 0
              ? COPY.noCandidates
              : COPY.noResults}
          </div>
        ) : null}
      </div>

      {content.name_only ? (
        <div className="ev-model-notice">
          <span>{COPY.languageModel}</span>
          <button disabled={modelBusy} onClick={onDownloadModel} type="button">
            {modelBusy ? (
              <LoaderCircle
                aria-hidden="true"
                className="ev-spinner"
                size={14}
              />
            ) : (
              <Download aria-hidden="true" size={14} />
            )}
            {COPY.downloadModel}
          </button>
        </div>
      ) : null}

      <div className="ev-choice-submit">
        <span>{COPY.chooserKeys}</span>
        <button
          disabled={!selectedId || busy}
          onClick={chooseSelected}
          type="button"
        >
          {busy ? (
            <LoaderCircle aria-hidden="true" className="ev-spinner" size={15} />
          ) : null}
          {COPY.continue}
          {!busy ? <ArrowRight aria-hidden="true" size={15} /> : null}
        </button>
      </div>
    </section>
  );
}

function RunningView({
  automatic,
  extensionName,
}: {
  automatic: boolean;
  extensionName: string;
}) {
  return (
    <section className="ev-running ev-stage" aria-live="polite">
      <div className="ev-running-mark" aria-hidden="true">
        <Send size={22} strokeWidth={1.8} />
        <LoaderCircle className="ev-running-ring" size={52} strokeWidth={1.2} />
      </div>
      <h1>
        {automatic ? COPY.autoSending : COPY.opening} {extensionName}
      </h1>
      <p>{COPY.runningHint}</p>
    </section>
  );
}

function ResultView({
  content,
  onCopy,
  onOpenExtensions,
  onOutput,
  copied,
}: {
  content: Extract<ExtensionViewContent, { kind: "result" }>;
  onCopy: () => void;
  onOpenExtensions: () => void;
  onOutput: (action: "insert" | "replace") => void;
  copied: boolean;
}) {
  const Icon =
    content.tone === "success"
      ? CheckCircle2
      : content.tone === "warning"
        ? AlertTriangle
        : XCircle;
  return (
    <div className="ev-result" data-tone={content.tone}>
      <div className="ev-result-icon">
        <Icon aria-hidden="true" size={23} strokeWidth={1.8} />
      </div>
      <p>{content.message}</p>
      {content.can_copy ||
      content.can_insert ||
      content.can_replace ||
      content.can_open_extensions ? (
        <div className="ev-result-actions">
          {content.can_open_extensions ? (
            <button
              className="ev-copy ev-output ev-output-primary"
              onClick={onOpenExtensions}
              type="button"
            >
              {COPY.openExtensions}
              <ArrowRight aria-hidden="true" size={15} />
            </button>
          ) : null}
          {content.can_copy ? (
            <button className="ev-copy" onClick={onCopy} type="button">
              {copied ? (
                <Check aria-hidden="true" size={15} />
              ) : (
                <Clipboard aria-hidden="true" size={15} />
              )}
              {copied ? COPY.copied : COPY.copy}
            </button>
          ) : null}
          {content.tone === "success" && content.can_insert ? (
            <button
              className="ev-copy ev-output"
              onClick={() => onOutput("insert")}
              type="button"
            >
              <TextCursorInput aria-hidden="true" size={15} />
              {COPY.insert}
            </button>
          ) : null}
          {content.tone === "success" && content.can_replace ? (
            <button
              className="ev-copy ev-output ev-output-primary"
              onClick={() => onOutput("replace")}
              type="button"
            >
              <ReplaceIcon aria-hidden="true" size={15} />
              {COPY.replace}
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function ExtensionViewApp() {
  const [session, setSession] = useState<Session | null>(null);
  const [busy, setBusy] = useState(false);
  const [modelBusy, setModelBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const sessionRef = useRef<Session | null>(null);
  const readySession = useRef<number | null>(null);
  const copyTimer = useRef<number | null>(null);
  const autoCloseTimer = useRef<number | null>(null);

  const adopt = useCallback((next: Session) => {
    if (sessionRef.current && next.sessionId < sessionRef.current.sessionId)
      return;
    sessionRef.current = next;
    setSession(next);
    setError(null);
    setBusy(false);
    setModelBusy(false);
    setCopied(false);
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlistenPresent = listen<Session>(PRESENT_EVENT, (event) => {
      if (!disposed) adopt(event.payload);
    });
    const unlistenTheme = listen<{ resolved: "light" | "dark" }>(
      THEME_EVENT,
      (event) => {
        document.documentElement.dataset.theme = event.payload.resolved;
      },
    );
    void invoke<{ resolved: "light" | "dark" }>("get_theme")
      .then((theme) => {
        document.documentElement.dataset.theme = theme.resolved;
      })
      .catch(() => {
        document.documentElement.dataset.theme = "dark";
      });
    void invoke<Session | null>("extension_view_init")
      .then((initial) => {
        if (disposed) return;
        if (!initial) {
          if (!sessionRef.current) setError(COPY.empty);
          return;
        }
        // A present event may win the race with this initial snapshot. Never
        // let the older/same-session snapshot roll that newer content back.
        if (
          !sessionRef.current ||
          initial.sessionId > sessionRef.current.sessionId
        )
          adopt(initial);
      })
      .catch((reason) => {
        if (!disposed) setError(errorMessage(reason));
      });
    return () => {
      disposed = true;
      void unlistenPresent.then((unlisten) => unlisten());
      void unlistenTheme.then((unlisten) => unlisten());
      if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
      if (autoCloseTimer.current !== null)
        window.clearTimeout(autoCloseTimer.current);
    };
  }, [adopt]);

  useEffect(() => {
    if (!session || readySession.current === session.sessionId) return;
    readySession.current = session.sessionId;
    const frame = window.requestAnimationFrame(() => {
      void invoke("extension_view_ready", { sessionId: session.sessionId })
        .then(() => {
          document
            .querySelector<HTMLElement>(
              "input:not(:disabled), textarea:not(:disabled), select:not(:disabled), .ev-action:not(:disabled)",
            )
            ?.focus();
        })
        .catch((reason) => setError(errorMessage(reason)));
    });
    return () => window.cancelAnimationFrame(frame);
  }, [session]);

  const close = useCallback(async () => {
    if (!session) {
      await getCurrentWindow().close();
      return;
    }
    try {
      await invoke("extension_view_close", { sessionId: session.sessionId });
    } catch {
      await getCurrentWindow().close();
    }
  }, [session]);

  useEffect(() => {
    if (autoCloseTimer.current !== null) {
      window.clearTimeout(autoCloseTimer.current);
      autoCloseTimer.current = null;
    }
    const delay =
      session?.content.kind === "result"
        ? session.content.dismiss_after_ms
        : null;
    if (!delay) return;
    autoCloseTimer.current = window.setTimeout(() => void close(), delay);
    return () => {
      if (autoCloseTimer.current !== null) {
        window.clearTimeout(autoCloseTimer.current);
        autoCloseTimer.current = null;
      }
    };
  }, [close, session]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  const copyResult = useCallback(async () => {
    if (!session || session.content.kind !== "result") return;
    try {
      await invoke("extension_view_copy", { sessionId: session.sessionId });
      setCopied(true);
      if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
      copyTimer.current = window.setTimeout(() => setCopied(false), 1600);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }, [session]);

  const outputResult = useCallback(
    async (action: "insert" | "replace") => {
      if (!session || session.content.kind !== "result") return;
      try {
        await invoke("extension_view_output", {
          sessionId: session.sessionId,
          action,
        });
      } catch (reason) {
        setError(errorMessage(reason));
      }
    },
    [session],
  );

  const openExtensions = useCallback(async () => {
    if (
      !session ||
      session.content.kind !== "result" ||
      !session.content.can_open_extensions
    )
      return;
    try {
      await invoke("extension_view_open_extensions", {
        sessionId: session.sessionId,
      });
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }, [session]);

  const chooseExtension = useCallback(
    async (extensionId: string) => {
      if (!session || session.content.kind !== "choose" || busy) return;
      setBusy(true);
      setError(null);
      try {
        await invoke("extension_view_choose", {
          sessionId: session.sessionId,
          presentationId: session.content.presentation_id,
          extensionId,
        });
      } catch (reason) {
        setBusy(false);
        setError(errorMessage(reason));
      }
    },
    [busy, session],
  );

  const downloadModel = useCallback(async () => {
    if (!session || session.content.kind !== "choose" || modelBusy) return;
    setModelBusy(true);
    setError(null);
    try {
      await invoke("extension_view_download_model", {
        sessionId: session.sessionId,
      });
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setModelBusy(false);
    }
  }, [modelBusy, session]);

  const ownerName = session?.extensionName ?? COPY.mode;

  if (!session) {
    return (
      <main className="ev-shell ev-shell-empty">
        <XCircle aria-hidden="true" size={25} />
        <p>{error ?? COPY.error}</p>
        <small>{COPY.retry}</small>
      </main>
    );
  }

  return (
    <main className="ev-shell" data-stage={session.content.kind}>
      <header className="ev-titlebar" data-tauri-drag-region>
        <div className="ev-identity" data-tauri-drag-region>
          <span className="ev-mark" aria-hidden="true">
            {session.content.kind === "routing" ||
            session.content.kind === "choose" ? (
              <Route size={14} strokeWidth={2} />
            ) : (
              <ShieldCheck size={14} strokeWidth={2} />
            )}
          </span>
          <span data-tauri-drag-region>{ownerName}</span>
          {session.extensionId ? (
            <span className="ev-id" data-tauri-drag-region>
              {session.extensionId}
            </span>
          ) : null}
        </div>
        <button
          aria-label={COPY.close}
          className="ev-icon-button"
          onClick={() => void close()}
          title={COPY.close}
          type="button"
        >
          <X aria-hidden="true" size={17} strokeWidth={1.8} />
        </button>
      </header>

      <div className="ev-scroll">
        {session.content.kind === "routing" ? (
          <RoutingView content={session.content} />
        ) : session.content.kind === "choose" ? (
          <ChoiceView
            busy={busy}
            content={session.content}
            modelBusy={modelBusy}
            onChoose={(extensionId) => void chooseExtension(extensionId)}
            onDownloadModel={() => void downloadModel()}
          />
        ) : session.content.kind === "running" ? (
          <RunningView
            automatic={session.content.automatic}
            extensionName={ownerName}
          />
        ) : (
          <div className="ev-stage">
            <ResultView
              content={session.content}
              copied={copied}
              onCopy={() => void copyResult()}
              onOpenExtensions={() => void openExtensions()}
              onOutput={(action) => void outputResult(action)}
            />
          </div>
        )}
        {error ? (
          <div className="ev-error" role="alert">
            <AlertTriangle aria-hidden="true" size={15} />
            {error}
          </div>
        ) : null}
      </div>

      {session.content.kind === "result" ? (
        <footer className="ev-footer ev-footer-result">
          <div className="ev-trust">
            <ShieldCheck aria-hidden="true" size={14} />
            <span>{COPY.safeBoundary}</span>
          </div>
          <button
            className="ev-action"
            data-intent="primary"
            onClick={() => void close()}
            type="button"
          >
            {COPY.close}
          </button>
        </footer>
      ) : null}
    </main>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ExtensionViewApp />
  </StrictMode>,
);
