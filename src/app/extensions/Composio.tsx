/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowRight,
  ArrowUpRight,
  Box,
  KeyRound,
  Lock,
  Sparkles,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  commands,
  type ComposioCatalogEntry,
  type ComposioStatus,
} from "@/bindings";
import { Switch } from "@/components/ui/Switch";
import {
  filterExtensions,
  signInCancelled,
  unwrapResult,
} from "./extensionRuntime";
import "./extensions.css";

const DASHBOARD_URL = "https://dashboard.composio.dev";
/** Apps shown before "Show all"; Composio lists hundreds. */
const FIRST_PAGE = 18;
/** An armed "Remove Composio" falls back to safe after this long. */
const DISARM_MS = 4000;

export function Logo({ src, size = 22 }: { src: string; size?: number }) {
  const [failed, setFailed] = useState(false);
  return (
    <span className="ext-logo" style={{ width: size + 10, height: size + 10 }}>
      {src && !failed ? (
        <img
          src={src}
          alt=""
          width={size}
          height={size}
          loading="lazy"
          referrerPolicy="no-referrer"
          onError={() => setFailed(true)}
        />
      ) : (
        <Box size={size - 6} aria-hidden="true" />
      )}
    </span>
  );
}

/**
 * App marks flanking the Agent in the setup card. Marks from Simple Icons
 * (CC0); GitHub and Notion follow the text colour so they read in both themes.
 */
const MARKS: { label: string; tint: string; d: string }[] = [
  {
    label: "Gmail",
    tint: "#EA4335",
    d: "M24 5.457v13.909c0 .904-.732 1.636-1.636 1.636h-3.819V11.73L12 16.64l-6.545-4.91v9.273H1.636A1.636 1.636 0 0 1 0 19.366V5.457c0-2.023 2.309-3.178 3.927-1.964L5.455 4.64 12 9.548l6.545-4.91 1.528-1.145C21.69 2.28 24 3.434 24 5.457z",
  },
  {
    label: "Google Calendar",
    tint: "#4285F4",
    d: "M18.316 5.684H24v12.632h-5.684V5.684zM5.684 24h12.632v-5.684H5.684V24zM18.316 5.684V0H1.895A1.894 1.894 0 0 0 0 1.895v16.421h5.684V5.684h12.632zm-7.207 6.25v-.065c.272-.144.5-.349.687-.617s.279-.595.279-.982c0-.379-.099-.72-.3-1.025a2.05 2.05 0 0 0-.832-.714 2.703 2.703 0 0 0-1.197-.257c-.6 0-1.094.156-1.481.467-.386.311-.65.671-.793 1.078l1.085.452c.086-.249.224-.461.413-.633.189-.172.445-.257.767-.257.33 0 .602.088.816.264a.86.86 0 0 1 .322.703c0 .33-.12.589-.36.778-.24.19-.535.284-.886.284h-.567v1.085h.633c.407 0 .748.109 1.02.327.272.218.407.499.407.843 0 .336-.129.614-.387.832s-.565.327-.924.327c-.351 0-.651-.103-.897-.311-.248-.208-.422-.502-.521-.881l-1.096.452c.178.616.505 1.082.977 1.401.472.319.984.478 1.538.477a2.84 2.84 0 0 0 1.293-.291c.382-.193.684-.458.902-.794.218-.336.327-.72.327-1.149 0-.429-.115-.797-.344-1.105a2.067 2.067 0 0 0-.881-.689zm2.093-1.931l.602.913L15 10.045v5.744h1.187V8.446h-.827l-2.158 1.557zM22.105 0h-3.289v5.184H24V1.895A1.894 1.894 0 0 0 22.105 0zm-3.289 23.5l4.684-4.684h-4.684V23.5zM0 22.105C0 23.152.848 24 1.895 24h3.289v-5.184H0v3.289z",
  },
  {
    label: "Slack",
    tint: "#E01E5A",
    d: "M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zM6.313 15.165a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313zM8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zM8.834 6.313a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312zM18.956 8.834a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zM17.688 8.834a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.165 0a2.528 2.528 0 0 1 2.523 2.522v6.312zM15.165 18.956a2.528 2.528 0 0 1 2.523 2.522A2.528 2.528 0 0 1 15.165 24a2.527 2.527 0 0 1-2.52-2.522v-2.522h2.52zM15.165 17.688a2.527 2.527 0 0 1-2.52-2.523 2.526 2.526 0 0 1 2.52-2.52h6.313A2.527 2.527 0 0 1 24 15.165a2.528 2.528 0 0 1-2.522 2.523h-6.313z",
  },
  {
    label: "Notion",
    tint: "currentColor",
    d: "M4.459 4.208c.746.606 1.026.56 2.428.466l13.215-.793c.28 0 .047-.28-.046-.326L17.86 1.968c-.42-.326-.981-.7-2.055-.607L3.01 2.295c-.466.046-.56.28-.374.466zm.793 3.08v13.904c0 .747.373 1.027 1.214.98l14.523-.84c.841-.046.935-.56.935-1.167V6.354c0-.606-.233-.933-.748-.887l-15.177.887c-.56.047-.747.327-.747.933zm14.337.745c.093.42 0 .84-.42.888l-.7.14v10.264c-.608.327-1.168.514-1.635.514-.748 0-.935-.234-1.495-.933l-4.577-7.186v6.952L12.21 19s0 .84-1.168.84l-3.222.186c-.093-.186 0-.653.327-.746l.84-.233V9.854L7.822 9.76c-.094-.42.14-1.026.793-1.073l3.456-.233 4.764 7.279v-6.44l-1.215-.139c-.093-.514.28-.887.747-.933zM1.936 1.035l13.31-.98c1.634-.14 2.055-.047 3.082.7l4.249 2.986c.7.513.934.653.934 1.213v16.378c0 1.026-.373 1.634-1.68 1.726l-15.458.934c-.98.047-1.448-.093-1.962-.747l-3.129-4.06c-.56-.747-.793-1.306-.793-1.96V2.667c0-.839.374-1.54 1.447-1.632z",
  },
  {
    label: "GitHub",
    tint: "currentColor",
    d: "M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12",
  },
  {
    label: "Figma",
    tint: "#F24E1E",
    d: "M15.852 8.981h-4.588V0h4.588c2.476 0 4.49 2.014 4.49 4.49s-2.014 4.491-4.49 4.491zM12.735 7.51h3.117c1.665 0 3.019-1.355 3.019-3.019s-1.355-3.019-3.019-3.019h-3.117V7.51zm0 1.471H8.148c-2.476 0-4.49-2.014-4.49-4.49S5.672 0 8.148 0h4.588v8.981zm-4.587-7.51c-1.665 0-3.019 1.355-3.019 3.019s1.354 3.02 3.019 3.02h3.117V1.471H8.148zm4.587 15.019H8.148c-2.476 0-4.49-2.014-4.49-4.49s2.014-4.49 4.49-4.49h4.588v8.98zM8.148 8.981c-1.665 0-3.019 1.355-3.019 3.019s1.355 3.019 3.019 3.019h3.117V8.981H8.148zM8.172 24c-2.489 0-4.515-2.014-4.515-4.49s2.014-4.49 4.49-4.49h4.588v4.441c0 2.503-2.047 4.539-4.563 4.539zm-.024-7.51a3.023 3.023 0 0 0-3.019 3.019c0 1.665 1.365 3.019 3.044 3.019 1.705 0 3.093-1.376 3.093-3.068v-2.97H8.148zm7.704 0h-.098c-2.476 0-4.49-2.014-4.49-4.49s2.014-4.49 4.49-4.49h.098c2.476 0 4.49 2.014 4.49 4.49s-2.014 4.49-4.49 4.49zm-.097-7.509c-1.665 0-3.019 1.355-3.019 3.019s1.355 3.019 3.019 3.019h.098c1.665 0 3.019-1.355 3.019-3.019s-1.355-3.019-3.019-3.019h-.098z",
  },
];

function Mark({ label, tint, d }: (typeof MARKS)[number]) {
  return (
    <svg
      className="ext-mark"
      viewBox="0 0 24 24"
      width={30}
      height={30}
      style={{ color: tint }}
      aria-label={label}
    >
      <path d={d} fill="currentColor" />
    </svg>
  );
}

function SetupMarks() {
  // Every app gets a short thread to its neighbour, so all six lead to the Agent.
  const thread = <span className="ext-thread" />;
  return (
    <div
      className="ext-marks"
      role="img"
      aria-label="Your apps, connected to the Agent"
    >
      {MARKS.slice(0, 3).map((mark) => (
        <Fragment key={mark.label}>
          <Mark {...mark} />
          {thread}
        </Fragment>
      ))}
      <Sparkles className="ext-marks-hub" size={46} strokeWidth={1.4} />
      {MARKS.slice(3).map((mark) => (
        <Fragment key={mark.label}>
          {thread}
          <Mark {...mark} />
        </Fragment>
      ))}
    </div>
  );
}

function KeySetup({
  busy,
  onSave,
}: {
  busy: boolean;
  onSave: (key: string) => Promise<boolean>;
}) {
  const [key, setKey] = useState("");
  return (
    <div className="ext-setup">
      <header className="ext-setup-head">
        <SetupMarks />
        <h3 id="ext-apps">Let the Agent work in your apps</h3>
        <p>
          Gmail, Calendar, Slack, Notion, GitHub and hundreds more, through your
          own free Composio account. Set it up once, then connect any app in a
          click.
        </p>
      </header>

      <ol className="ext-setup-steps">
        <li className="ext-step">
          <span className="ext-step-num">1</span>
          <h4>Create a free account</h4>
          <p>
            If the top-left corner says <b>For You</b>, click <b>Switch</b> and
            choose the developer platform.
          </p>
          <button
            type="button"
            className="ext-link"
            onClick={() => void openUrl(DASHBOARD_URL)}
          >
            Open Composio
            <ArrowUpRight size={14} aria-hidden="true" />
          </button>
        </li>
        <li className="ext-step">
          <span className="ext-step-num">2</span>
          <h4>Copy your API key</h4>
          <p className="ext-path">
            <span>Settings</span>
            <ArrowRight size={13} strokeWidth={2.2} aria-hidden="true" />
            <span>Project Settings</span>
            <ArrowRight size={13} strokeWidth={2.2} aria-hidden="true" />
            <span>API Keys</span>
          </p>
          <p>
            Copy the key that starts with <code>ak_</code>.
          </p>
        </li>
      </ol>

      <form
        className="ext-key"
        onSubmit={(event) => {
          event.preventDefault();
          void onSave(key).then((saved) => saved && setKey(""));
        }}
      >
        <label className="ext-key-label" htmlFor="ext-key-input">
          <span className="ext-step-num">3</span>
          Paste your key
        </label>
        <div className="ext-key-field">
          <KeyRound size={16} aria-hidden="true" />
          <input
            id="ext-key-input"
            type="password"
            placeholder="ak_…"
            autoComplete="off"
            spellCheck={false}
            value={key}
            disabled={busy}
            onChange={(event) => setKey(event.target.value)}
          />
          <button
            className="button primary"
            type="submit"
            disabled={busy || !key.trim()}
          >
            {busy ? "Checking…" : "Connect"}
          </button>
        </div>
        <p className="ext-fine-print">
          <Lock size={12} aria-hidden="true" />
          Kept in your system’s credential store and only ever sent to Composio.
          The free plan covers 20,000 app actions a month.
        </p>
      </form>
    </div>
  );
}

/** Composio state and actions, shared by the header button and the page body. */
export function useComposio() {
  const [status, setStatus] = useState<ComposioStatus | null>(null);
  const [catalog, setCatalog] = useState<ComposioCatalogEntry[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [signingIn, setSigningIn] = useState<string | null>(null);
  const [armed, setArmed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(false);
  const operation = useRef(false);
  const pending = useRef<string | null>(null);
  /** Bumped by every status a user action applies. */
  const changes = useRef(0);

  const loadCatalog = useCallback(async () => {
    const entries = unwrapResult(await commands.composioCatalog());
    if (mounted.current) setCatalog(entries);
  }, []);

  useEffect(() => {
    mounted.current = true;
    void (async () => {
      try {
        // Saved state first so the page paints at once; the catalog comes
        // from its on-disk copy.
        const saved = unwrapResult(await commands.composioStatus(false));
        if (!mounted.current) return;
        setStatus(saved);
        if (!saved.configured) return;
        // Checking for accounts removed elsewhere needs the network, so it
        // runs alongside and only replaces the list if nothing changed it since.
        const seen = changes.current;
        void commands
          .composioStatus(true)
          .then((result) => {
            if (
              mounted.current &&
              !operation.current &&
              changes.current === seen &&
              result.status === "ok"
            )
              setStatus(result.data);
          })
          .catch(() => {});
        await loadCatalog();
      } catch (reason) {
        if (mounted.current) setError(String(reason));
      }
    })();
    return () => {
      mounted.current = false;
      if (pending.current)
        void commands.composioCancelConnect(pending.current).catch(() => {});
    };
  }, [loadCatalog]);

  useEffect(() => {
    if (!armed) return;
    const timer = window.setTimeout(() => setArmed(false), DISARM_MS);
    return () => window.clearTimeout(timer);
  }, [armed]);

  const run = async (id: string, action: () => Promise<void>) => {
    if (operation.current) return false;
    operation.current = true;
    setBusy(id);
    setError(null);
    try {
      await action();
      return true;
    } catch (reason) {
      if (mounted.current) setError(String(reason));
      return false;
    } finally {
      if (mounted.current) setBusy(null);
      operation.current = false;
    }
  };

  const apply = (next: ComposioStatus) => {
    changes.current += 1;
    if (mounted.current) setStatus(next);
  };

  return {
    status,
    catalog,
    busy,
    signingIn,
    armed,
    error,
    disarm: () => setArmed(false),
    saveKey: (key: string) =>
      run("key", async () => {
        apply(unwrapResult(await commands.composioSetApiKey(key)));
        await loadCatalog();
      }),
    removeKey: () => {
      if (!armed) {
        setArmed(true);
        return;
      }
      setArmed(false);
      void run("key", async () => {
        apply(unwrapResult(await commands.composioRemoveApiKey()));
        if (mounted.current) setCatalog(null);
      });
    },
    connect: (entry: ComposioCatalogEntry) =>
      void run(entry.slug, async () => {
        if (!entry.no_auth) {
          pending.current = entry.slug;
          setSigningIn(entry.slug);
        }
        try {
          apply(unwrapResult(await commands.composioConnect(entry.slug)));
        } catch (reason) {
          if (signInCancelled(reason)) return;
          throw reason;
        } finally {
          pending.current = null;
          if (mounted.current) setSigningIn(null);
        }
      }),
    cancel: (slug: string) =>
      void commands
        .composioCancelConnect(slug)
        .then(unwrapResult)
        .catch((reason) => mounted.current && setError(String(reason))),
    disconnect: (slug: string) =>
      void run(slug, async () => {
        apply(unwrapResult(await commands.composioDisconnect(slug)));
      }),
    setEnabled: (slug: string, enabled: boolean) =>
      void run(slug, async () => {
        apply(unwrapResult(await commands.composioSetEnabled(slug, enabled)));
      }),
  };
}

export type Composio = ReturnType<typeof useComposio>;

/** Header action: two clicks forget the Composio key. */
export function RemoveComposio({ composio }: { composio: Composio }) {
  if (!composio.status?.configured) return null;
  return (
    <button
      type="button"
      className="button ghost ext-remove-key"
      data-armed={composio.armed || undefined}
      disabled={composio.busy !== null}
      onClick={composio.removeKey}
      onBlur={composio.disarm}
    >
      {composio.busy === "key"
        ? "Removing…"
        : composio.armed
          ? "Click again to remove"
          : "Remove Composio"}
    </button>
  );
}

export function ComposioError({ composio }: { composio: Composio }) {
  return composio.error ? (
    <p className="extension-inline-error ext-error" role="alert">
      {composio.error}
    </p>
  ) : null;
}

/** Connected Composio apps, shown under "Connected" beside Built-in. */
export function ComposioConnected({
  composio,
  query,
}: {
  composio: Composio;
  query: string;
}) {
  const { status, busy } = composio;
  if (!status?.configured) return null;
  const connected = filterExtensions(
    status.toolkits.map((toolkit) => ({ ...toolkit, id: toolkit.slug })),
    query,
  );
  // Rows only: they follow the built-ins in the one Connected list.
  return (
    <>
      {connected.map((toolkit) => (
        <div key={toolkit.slug} className="ext-row">
          <Logo src={toolkit.logo} />
          <div className="ext-row-copy">
            <h3>{toolkit.name}</h3>
            {toolkit.description && <p>{toolkit.description}</p>}
          </div>
          <button
            className="button ghost ext-quiet-action"
            type="button"
            aria-label={`Disconnect ${toolkit.name}`}
            disabled={busy !== null}
            onClick={() => composio.disconnect(toolkit.slug)}
          >
            {busy === toolkit.slug ? "Disconnecting…" : "Disconnect"}
          </button>
          <Switch
            checked={toolkit.enabled}
            disabled={busy !== null}
            ariaLabel={`Use ${toolkit.name}`}
            onChange={(enabled) => composio.setEnabled(toolkit.slug, enabled)}
          />
        </div>
      ))}
    </>
  );
}

/** App directory, or the one-time key setup before Composio is connected. */
export function ComposioBrowse({
  composio,
  query,
}: {
  composio: Composio;
  query: string;
}) {
  const [showAll, setShowAll] = useState(false);
  const { status, catalog, busy, signingIn } = composio;
  if (!status) return null;
  if (!status.configured)
    return (
      <section className="ext-section" aria-labelledby="ext-apps">
        <KeySetup busy={busy === "key"} onSave={composio.saveKey} />
      </section>
    );

  const searching = query.trim() !== "";
  const connectedSlugs = new Set(
    status.toolkits.map((toolkit) => toolkit.slug),
  );
  const available = filterExtensions(
    (catalog ?? [])
      .filter((entry) => !connectedSlugs.has(entry.slug))
      .map((entry) => ({ ...entry, id: entry.slug })),
    query,
  );
  const shown =
    showAll || searching ? available : available.slice(0, FIRST_PAGE);

  return (
    <section className="ext-section" aria-labelledby="ext-browse">
      <h2 className="ext-label" id="ext-browse">
        Browse apps
      </h2>
      {catalog === null ? (
        <p className="ext-feedback" role="status">
          Loading apps…
        </p>
      ) : shown.length === 0 ? (
        <p className="ext-feedback">
          {searching
            ? "No apps match your search."
            : "Every available app is connected."}
        </p>
      ) : (
        <div className="ext-grid">
          {shown.map((entry) => {
            const waiting = signingIn === entry.slug;
            return (
              <div
                key={entry.slug}
                className="ext-tile"
                data-waiting={waiting || undefined}
              >
                <div className="ext-tile-top">
                  <Logo src={entry.logo} size={24} />
                  {waiting ? (
                    <button
                      className="button ext-tile-action"
                      type="button"
                      onClick={() => composio.cancel(entry.slug)}
                    >
                      Cancel
                    </button>
                  ) : (
                    <button
                      className="button ext-tile-action"
                      type="button"
                      aria-label={`Connect ${entry.name}`}
                      disabled={busy !== null}
                      onClick={() => composio.connect(entry)}
                    >
                      {busy === entry.slug ? "Connecting…" : "Connect"}
                    </button>
                  )}
                </div>
                <h4>{entry.name}</h4>
                <p>
                  {waiting
                    ? "Finish signing in in your browser…"
                    : entry.description || "No description from Composio."}
                </p>
              </div>
            );
          })}
        </div>
      )}
      {!showAll && !searching && available.length > FIRST_PAGE && (
        <button
          className="button ghost ext-more"
          type="button"
          onClick={() => setShowAll(true)}
        >
          Show all {available.length} apps
        </button>
      )}
    </section>
  );
}
