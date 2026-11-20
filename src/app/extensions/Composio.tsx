/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowUpRight,
  Box,
  CalendarDays,
  ChevronRight,
  ClipboardPaste,
  Clock3,
  Figma,
  Gift,
  Github,
  KeyRound,
  Lock,
  Mail,
  NotebookText,
  Slack,
  Sparkles,
  UserPlus,
  type LucideIcon,
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

/** Apps orbiting the Agent in the setup illustration; brand tints only. */
const ORBIT: {
  icon: LucideIcon;
  label: string;
  tint: string;
  x: number;
  y: number;
}[] = [
  { icon: Mail, label: "Gmail", tint: "#ea4335", x: 14, y: 22 },
  { icon: Slack, label: "Slack", tint: "#c26bd6", x: 50, y: 6 },
  { icon: CalendarDays, label: "Calendar", tint: "#4f8df5", x: 86, y: 22 },
  { icon: Github, label: "GitHub", tint: "currentColor", x: 88, y: 76 },
  { icon: NotebookText, label: "Notion", tint: "currentColor", x: 50, y: 94 },
  { icon: Figma, label: "Figma", tint: "#f2784b", x: 12, y: 76 },
];

function SetupArt() {
  return (
    <div className="ext-setup-art" aria-hidden="true">
      <svg viewBox="0 0 100 100" preserveAspectRatio="none">
        {ORBIT.map((app) => (
          <line key={app.label} x1="50" y1="50" x2={app.x} y2={app.y} />
        ))}
      </svg>
      <span className="ext-orbit-hub">
        <Sparkles size={22} strokeWidth={1.75} />
      </span>
      {ORBIT.map(({ icon: Icon, label, tint, x, y }, index) => (
        <span
          key={label}
          className="ext-orbit-app"
          style={{
            left: `${x}%`,
            top: `${y}%`,
            color: tint,
            animationDelay: `${index * -1.1}s`,
          }}
        >
          <Icon size={17} strokeWidth={1.9} />
        </span>
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
      <div className="ext-setup-hero">
        <div className="ext-setup-copy">
          <span className="ext-eyebrow">Apps, through Composio</span>
          <h3 id="ext-apps">Let the Agent work in your apps</h3>
          <p>
            Gmail, Calendar, Slack, Notion, GitHub and hundreds more. Set up
            Composio once, then connect any app with a single click.
          </p>
          <ul className="ext-setup-facts">
            <li>
              <Gift size={13} aria-hidden="true" />
              Free, 20,000 actions a month
            </li>
            <li>
              <Clock3 size={13} aria-hidden="true" />
              About two minutes
            </li>
          </ul>
        </div>
        <SetupArt />
      </div>

      <ol className="ext-steps">
        <li>
          <span className="ext-step-badge">
            <UserPlus size={15} aria-hidden="true" />
          </span>
          <div className="ext-step-body">
            <h4>Create a free Composio account</h4>
            <p>
              If the top-left corner says <b>For You</b>, click <b>Switch</b>{" "}
              and choose the developer platform.
            </p>
            <button
              type="button"
              className="button ghost ext-inline-action"
              onClick={() => void openUrl(DASHBOARD_URL)}
            >
              Open Composio
              <ArrowUpRight size={14} aria-hidden="true" />
            </button>
          </div>
        </li>
        <li>
          <span className="ext-step-badge">
            <KeyRound size={15} aria-hidden="true" />
          </span>
          <div className="ext-step-body">
            <h4>Copy your project API key</h4>
            <p className="ext-path">
              <span>Settings</span>
              <ChevronRight size={12} aria-hidden="true" />
              <span>Project Settings</span>
              <ChevronRight size={12} aria-hidden="true" />
              <span>API Keys</span>
            </p>
            <p>
              It starts with <code>ak_</code>.
            </p>
          </div>
        </li>
        <li>
          <span className="ext-step-badge">
            <ClipboardPaste size={15} aria-hidden="true" />
          </span>
          <div className="ext-step-body">
            <h4>Paste it here</h4>
            <form
              className="ext-key-form"
              onSubmit={(event) => {
                event.preventDefault();
                void onSave(key).then((saved) => saved && setKey(""));
              }}
            >
              <input
                type="password"
                aria-label="Composio API key"
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
            </form>
            <p className="ext-fine-print">
              <Lock size={12} aria-hidden="true" />
              Kept in your system’s credential store and only ever sent to
              Composio.
            </p>
          </div>
        </li>
      </ol>
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

  const loadCatalog = useCallback(async () => {
    const entries = unwrapResult(await commands.composioCatalog());
    if (mounted.current) setCatalog(entries);
  }, []);

  useEffect(() => {
    mounted.current = true;
    void (async () => {
      try {
        const next = unwrapResult(await commands.composioStatus(true));
        if (!mounted.current) return;
        setStatus(next);
        if (next.configured) await loadCatalog();
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
