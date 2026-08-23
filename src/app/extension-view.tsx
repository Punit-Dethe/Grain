import { StrictMode, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  ChevronDown,
  Clipboard,
  LoaderCircle,
  ShieldCheck,
  X,
  XCircle,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  ExtensionViewContent,
  ExtensionViewEvent,
  ExtensionViewEventResult,
  ExtensionViewInit,
  ViewNode,
  ViewValue,
} from "@/bindings";
import "./extension-view.css";

type Values = Record<string, ViewValue>;
type Session = ExtensionViewInit;
type Content = ExtensionViewContent;
type EventResult = ExtensionViewEventResult;
type ViewEvent = Exclude<ExtensionViewEvent, { kind: "cancel" }>;

const PRESENT_EVENT = "extension-view://present";
const THEME_EVENT = "theme-changed";
const COPY = {
  app: "Grain extension",
  close: "Close",
  copied: "Copied",
  copy: "Copy result",
  empty: "This extension did not provide anything to review.",
  error: "The extension surface could not be loaded.",
  retry: "Close this window and try the request again.",
  safeBoundary: "Secure Grain surface",
  select: "Select an option",
  sendsTo: "Grain sends this action to",
  destructive: "This action can make a destructive change through",
} as const;

function collectValues(node: ViewNode, values: Values = {}): Values {
  switch (node.type) {
    case "text_field":
    case "text_area":
    case "select":
      values[node.id] = node.value ?? "";
      break;
    case "checkbox":
      values[node.id] = node.checked ?? false;
      break;
    case "stack":
    case "inline":
    case "grid":
    case "section":
      for (const child of node.children ?? []) collectValues(child, values);
      break;
  }
  return values;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

type RendererProps = {
  node: ViewNode;
  path: string;
  values: Values;
  disabled: boolean;
  setLocal: (id: string, value: ViewValue) => void;
  sendChange: (id: string, value: ViewValue) => void;
};

function NodeRenderer(props: RendererProps): ReactNode {
  const { node, path, values, disabled, setLocal, sendChange } = props;
  const renderChildren = (children?: ViewNode[]) =>
    children?.map((child, index) => (
      <NodeRenderer
        {...props}
        key={"id" in child ? child.id : `${path}-${index}`}
        node={child}
        path={`${path}-${index}`}
      />
    ));

  switch (node.type) {
    case "stack":
      return (
        <div className="ev-stack" data-gap={node.gap ?? "md"}>
          {renderChildren(node.children)}
        </div>
      );
    case "inline":
      return (
        <div
          className="ev-inline"
          data-align={node.align ?? "start"}
          data-gap={node.gap ?? "md"}
          data-wrap={node.wrap ? "true" : "false"}
        >
          {renderChildren(node.children)}
        </div>
      );
    case "grid":
      return (
        <div
          className="ev-grid"
          data-columns={node.columns}
          data-gap={node.gap ?? "md"}
        >
          {renderChildren(node.children)}
        </div>
      );
    case "section":
      return (
        <section className="ev-section">
          {node.title ? (
            <h2 className="ev-section-title">{node.title}</h2>
          ) : null}
          <div className="ev-section-body">{renderChildren(node.children)}</div>
        </section>
      );
    case "divider":
      return <hr className="ev-divider" />;
    case "heading": {
      const level = node.level ?? "two";
      // The trusted view title is the page's single h1. Author heading levels
      // are relative to it so a tree cannot create competing top-level titles.
      if (level === "one")
        return <h2 className="ev-heading ev-heading-1">{node.text}</h2>;
      if (level === "three")
        return <h4 className="ev-heading ev-heading-3">{node.text}</h4>;
      return <h3 className="ev-heading ev-heading-2">{node.text}</h3>;
    }
    case "text":
      return (
        <p className="ev-text" data-tone={node.tone ?? "neutral"}>
          {node.text}
        </p>
      );
    case "badge":
      return (
        <span className="ev-badge" data-tone={node.tone ?? "neutral"}>
          {node.text}
        </span>
      );
    case "metadata":
      return (
        <dl className="ev-metadata">
          <dt>{node.label}</dt>
          <dd>{node.value}</dd>
        </dl>
      );
    case "text_field":
      return (
        <label className="ev-field">
          <span className="ev-label">
            {node.label}
            {node.required ? <span aria-hidden="true">*</span> : null}
          </span>
          <input
            disabled={disabled || node.disabled}
            maxLength={node.maxLength ?? undefined}
            onBlur={(event) => sendChange(node.id, event.currentTarget.value)}
            onChange={(event) => setLocal(node.id, event.currentTarget.value)}
            placeholder={node.placeholder ?? undefined}
            required={node.required}
            type="text"
            value={String(values[node.id] ?? "")}
          />
        </label>
      );
    case "text_area":
      return (
        <label className="ev-field">
          <span className="ev-label">
            {node.label}
            {node.required ? <span aria-hidden="true">*</span> : null}
          </span>
          <textarea
            disabled={disabled || node.disabled}
            maxLength={node.maxLength ?? undefined}
            onBlur={(event) => sendChange(node.id, event.currentTarget.value)}
            onChange={(event) => setLocal(node.id, event.currentTarget.value)}
            placeholder={node.placeholder ?? undefined}
            required={node.required}
            rows={node.rows ?? 5}
            value={String(values[node.id] ?? "")}
          />
        </label>
      );
    case "select":
      return (
        <label className="ev-field">
          <span className="ev-label">
            {node.label}
            {node.required ? <span aria-hidden="true">*</span> : null}
          </span>
          <span className="ev-select-wrap">
            <select
              disabled={disabled || node.disabled}
              onChange={(event) =>
                sendChange(node.id, event.currentTarget.value)
              }
              required={node.required}
              value={String(values[node.id] ?? "")}
            >
              <option disabled={node.required} value="">
                {COPY.select}
              </option>
              {node.options.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            <ChevronDown aria-hidden="true" size={15} strokeWidth={1.8} />
          </span>
        </label>
      );
    case "checkbox":
      return (
        <label className="ev-check">
          <input
            checked={Boolean(values[node.id])}
            disabled={disabled || node.disabled}
            onChange={(event) =>
              sendChange(node.id, event.currentTarget.checked)
            }
            required={node.required}
            type="checkbox"
          />
          <span className="ev-check-box" aria-hidden="true">
            <Check size={13} strokeWidth={2.5} />
          </span>
          <span>{node.label}</span>
        </label>
      );
  }
}

function ResultView({
  content,
  onCopy,
  copied,
}: {
  content: Extract<ExtensionViewContent, { kind: "result" }>;
  onCopy: () => void;
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
      <button className="ev-copy" onClick={onCopy} type="button">
        {copied ? (
          <Check aria-hidden="true" size={15} />
        ) : (
          <Clipboard aria-hidden="true" size={15} />
        )}
        {copied ? COPY.copied : COPY.copy}
      </button>
    </div>
  );
}

function ExtensionViewApp() {
  const [session, setSession] = useState<Session | null>(null);
  const [values, setValues] = useState<Values>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const sessionRef = useRef<Session | null>(null);
  const readySession = useRef<number | null>(null);
  const copyTimer = useRef<number | null>(null);
  const eventQueue = useRef<Promise<void>>(Promise.resolve());
  const pendingEvents = useRef(0);

  const adopt = useCallback((next: Session) => {
    if (sessionRef.current && next.sessionId < sessionRef.current.sessionId)
      return;
    sessionRef.current = next;
    setSession(next);
    setError(null);
    setBusy(false);
    setCopied(false);
    if (next.content.kind === "view")
      setValues(collectValues(next.content.view.root));
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
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  const applyResult = useCallback((result: EventResult) => {
    if (!result.unchanged && result.content.kind === "view")
      setValues(collectValues(result.content.view.root));
    setSession((current) => {
      const next = current ? { ...current, content: result.content } : current;
      sessionRef.current = next;
      return next;
    });
  }, []);

  const send = useCallback(
    (event: ViewEvent) => {
      const queuedFor = sessionRef.current;
      if (!queuedFor || queuedFor.content.kind !== "view")
        return Promise.resolve();
      pendingEvents.current += 1;
      // A blur-generated change must not disable the button that is about to
      // receive the same pointer click. It is still serialized ahead of submit;
      // only the submit itself makes the surface visibly busy.
      if (event.kind === "submit") setBusy(true);
      setError(null);
      const task = eventQueue.current
        .catch(() => undefined)
        .then(async () => {
          const current = sessionRef.current;
          if (
            !current ||
            current.sessionId !== queuedFor.sessionId ||
            current.content.kind !== "view"
          )
            return;
          const result = await invoke<EventResult>("extension_view_event", {
            sessionId: current.sessionId,
            event,
          });
          applyResult(result);
        })
        .catch((reason) => {
          if (sessionRef.current?.sessionId === queuedFor.sessionId)
            setError(errorMessage(reason));
        })
        .finally(() => {
          pendingEvents.current = Math.max(0, pendingEvents.current - 1);
          if (pendingEvents.current === 0) setBusy(false);
        });
      eventQueue.current = task;
      return task;
    },
    [applyResult],
  );

  const setLocal = useCallback(
    (id: string, value: ViewValue) =>
      setValues((current) => ({ ...current, [id]: value })),
    [],
  );
  const sendChange = useCallback(
    (id: string, value: ViewValue) => {
      const next = { ...values, [id]: value };
      setValues(next);
      void send({ kind: "change", target: id, value, values: next });
    },
    [send, values],
  );

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

  const currentView =
    session?.content.kind === "view" ? session.content.view : null;
  const destructive = useMemo(
    () =>
      currentView?.actions?.some(
        (action) => action.intent === "danger" && !action.disabled,
      ) ?? false,
    [currentView],
  );

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
    <main className="ev-shell">
      <header className="ev-titlebar" data-tauri-drag-region>
        <div className="ev-identity" data-tauri-drag-region>
          <span className="ev-mark" aria-hidden="true">
            <ShieldCheck size={14} strokeWidth={2} />
          </span>
          <span data-tauri-drag-region>{session.extensionName}</span>
          <span className="ev-id" data-tauri-drag-region>
            {session.extensionId}
          </span>
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
        {session.content.kind === "view" ? (
          <form
            id="extension-view-form"
            onSubmit={(event) => {
              event.preventDefault();
              const primary =
                session.content.kind === "view"
                  ? session.content.view.actions?.find(
                      (action) =>
                        (action.intent ?? "primary") === "primary" &&
                        (action.kind ?? "submit") === "submit" &&
                        !action.disabled,
                    )
                  : undefined;
              if (primary)
                void send({ kind: "submit", target: primary.id, values });
            }}
          >
            <div className="ev-intro">
              <span className="ev-eyebrow">{COPY.app}</span>
              <h1>{session.content.view.title}</h1>
              {session.content.view.description ? (
                <p>{session.content.view.description}</p>
              ) : null}
            </div>
            <NodeRenderer
              disabled={busy}
              node={session.content.view.root}
              path="root"
              sendChange={sendChange}
              setLocal={setLocal}
              values={values}
            />
          </form>
        ) : (
          <ResultView
            content={session.content}
            copied={copied}
            onCopy={() => void copyResult()}
          />
        )}
        {error ? (
          <div className="ev-error" role="alert">
            <AlertTriangle aria-hidden="true" size={15} />
            {error}
          </div>
        ) : null}
      </div>

      {currentView ? (
        <footer className="ev-footer">
          <div
            className="ev-trust"
            data-danger={destructive ? "true" : "false"}
          >
            {destructive ? (
              <AlertTriangle aria-hidden="true" size={14} />
            ) : (
              <ShieldCheck aria-hidden="true" size={14} />
            )}
            <span>
              {destructive ? COPY.destructive : COPY.sendsTo}{" "}
              <strong>{session.extensionName}</strong>.
            </span>
          </div>
          <div className="ev-actions">
            {(currentView.actions ?? []).map((action) => {
              const cancel = (action.kind ?? "submit") === "cancel";
              return (
                <button
                  className="ev-action"
                  data-intent={action.intent ?? "primary"}
                  disabled={(!cancel && busy) || action.disabled}
                  key={action.id}
                  onClick={() =>
                    cancel
                      ? void close()
                      : void send({ kind: "submit", target: action.id, values })
                  }
                  type="button"
                >
                  {busy && !cancel ? (
                    <LoaderCircle
                      aria-hidden="true"
                      className="ev-spinner"
                      size={15}
                    />
                  ) : null}
                  {action.label}
                </button>
              );
            })}
          </div>
        </footer>
      ) : (
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
      )}
    </main>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ExtensionViewApp />
  </StrictMode>,
);
