/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useCallback, useEffect, useRef, useState } from "react";
import { Box } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  commands,
  type ComposioCatalogEntry,
  type ComposioStatus,
  type ComposioToolkit,
} from "@/bindings";
import { Switch } from "@/components/ui/Switch";
import {
  filterExtensions,
  signInCancelled,
  unwrapResult,
} from "./extensionRuntime";
import "./composio.css";

const DASHBOARD_URL = "https://dashboard.composio.dev";
/** Apps shown before "Show all"; the full list is long. */
const FIRST_PAGE = 24;

function Logo({ src }: { src: string }) {
  const [failed, setFailed] = useState(false);
  if (!src || failed)
    return (
      <span className="mcp-preset-icon" aria-hidden="true">
        <Box size={17} />
      </span>
    );
  return (
    <img
      className="mcp-brand-icon"
      src={src}
      alt=""
      width={24}
      height={24}
      loading="lazy"
      referrerPolicy="no-referrer"
      onError={() => setFailed(true)}
    />
  );
}

function Identity({
  logo,
  name,
  description,
}: {
  logo: string;
  name: string;
  description: string;
}) {
  return (
    <div className="mcp-preset-identity">
      <Logo src={logo} />
      <div>
        <h3>{name}</h3>
        {description && <p>{description}</p>}
      </div>
    </div>
  );
}

function KeySetup({
  busy,
  onSave,
}: {
  busy: boolean;
  onSave: (key: string) => Promise<void>;
}) {
  const [key, setKey] = useState("");
  return (
    <div className="composio-setup">
      <div className="composio-setup-copy">
        <h3>Connect your apps with Composio</h3>
        <p>
          Gmail, Google Calendar, Slack, Notion, GitHub and hundreds more, each
          one click to connect. Composio's free plan covers 20,000 app actions a
          month.
        </p>
        <ol>
          <li>
            Create a free Composio account.{" "}
            <button
              type="button"
              className="composio-link"
              onClick={() => void openUrl(DASHBOARD_URL)}
            >
              Open Composio
            </button>
          </li>
          <li>
            In Composio, open Settings → Project Settings → API Keys and copy
            your key.
          </li>
          <li>Paste it here.</li>
        </ol>
      </div>
      <form
        className="composio-key-form"
        onSubmit={(event) => {
          event.preventDefault();
          void onSave(key).then(() => setKey(""));
        }}
      >
        <input
          type="password"
          aria-label="Composio API key"
          placeholder="Composio API key"
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
          {busy ? "Checking…" : "Save"}
        </button>
      </form>
      <p className="mcp-integration-feedback">
        The key is kept in your system's credential store and only sent to
        Composio.
      </p>
    </div>
  );
}

export function ComposioExtensions({ query }: { query: string }) {
  const [status, setStatus] = useState<ComposioStatus | null>(null);
  const [catalog, setCatalog] = useState<ComposioCatalogEntry[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [signingIn, setSigningIn] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
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

  const run = async (id: string, action: () => Promise<void>) => {
    if (operation.current) return;
    operation.current = true;
    setBusy(id);
    setError(null);
    setMessage(null);
    try {
      await action();
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      if (mounted.current) setBusy(null);
      operation.current = false;
    }
  };

  const apply = (next: ComposioStatus) => {
    if (mounted.current) setStatus(next);
  };

  const saveKey = (key: string) =>
    run("key", async () => {
      apply(unwrapResult(await commands.composioSetApiKey(key)));
      await loadCatalog();
      if (mounted.current) setMessage("Composio is connected.");
    });

  const removeKey = () =>
    run("key", async () => {
      apply(unwrapResult(await commands.composioRemoveApiKey()));
      if (mounted.current) setCatalog(null);
    });

  const connect = (entry: ComposioCatalogEntry) =>
    run(entry.slug, async () => {
      if (!entry.no_auth) {
        pending.current = entry.slug;
        setSigningIn(entry.slug);
        setMessage("Finish signing in in your browser…");
      }
      try {
        apply(unwrapResult(await commands.composioConnect(entry.slug)));
      } catch (reason) {
        if (mounted.current) setMessage(null);
        if (signInCancelled(reason)) return;
        throw reason;
      } finally {
        pending.current = null;
        if (mounted.current) setSigningIn(null);
      }
      if (mounted.current) setMessage(`${entry.name} connected.`);
    });

  const cancel = (slug: string) =>
    void commands
      .composioCancelConnect(slug)
      .then(unwrapResult)
      .catch((reason) => mounted.current && setError(String(reason)));

  if (!status)
    return error ? (
      <div className="extension-inline-error" role="alert">
        {error}
      </div>
    ) : null;

  const feedback = (
    <>
      {message && (
        <p className="mcp-integration-feedback" role="status">
          {message}
        </p>
      )}
      {error && (
        <div className="extension-inline-error" role="alert">
          {error}
        </div>
      )}
    </>
  );

  if (!status.configured)
    return (
      <section className="mcp-integration-section" aria-label="Composio apps">
        <h2>Apps</h2>
        {feedback}
        <KeySetup busy={busy === "key"} onSave={saveKey} />
      </section>
    );

  const asRow = (toolkit: ComposioToolkit) => ({
    ...toolkit,
    id: toolkit.slug,
  });
  const connected = filterExtensions(status.toolkits.map(asRow), query);
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
    showAll || query.trim() ? available : available.slice(0, FIRST_PAGE);

  return (
    <>
      {feedback}
      {connected.length > 0 && (
        <section
          className="mcp-integration-section"
          aria-label="Connected apps"
        >
          <h2>Connected apps</h2>
          <div className="mcp-integration-list">
            {connected.map((toolkit) => (
              <article key={toolkit.slug} className="mcp-preset-connected">
                <Identity {...toolkit} />
                <div className="mcp-preset-actions">
                  <button
                    className="button ghost"
                    type="button"
                    aria-label={`Disconnect ${toolkit.name}`}
                    disabled={busy !== null}
                    onClick={() =>
                      void run(toolkit.slug, async () => {
                        apply(
                          unwrapResult(
                            await commands.composioDisconnect(toolkit.slug),
                          ),
                        );
                      })
                    }
                  >
                    Disconnect
                  </button>
                  <Switch
                    checked={toolkit.enabled}
                    disabled={busy !== null}
                    ariaLabel={`Enable ${toolkit.name}`}
                    onChange={(enabled) =>
                      void run(toolkit.slug, async () => {
                        apply(
                          unwrapResult(
                            await commands.composioSetEnabled(
                              toolkit.slug,
                              enabled,
                            ),
                          ),
                        );
                      })
                    }
                  />
                </div>
              </article>
            ))}
          </div>
        </section>
      )}
      <section className="mcp-integration-section" aria-label="Apps">
        <div className="composio-heading">
          <h2>Apps</h2>
          <span className="composio-account">
            via Composio ·{" "}
            <button
              type="button"
              className="composio-link"
              disabled={busy !== null}
              onClick={() => void removeKey()}
            >
              Remove key
            </button>
          </span>
        </div>
        {catalog === null ? (
          <p className="mcp-integration-feedback" role="status">
            Loading apps…
          </p>
        ) : shown.length === 0 ? (
          <p className="mcp-integration-feedback">
            {query.trim()
              ? "No apps match your search."
              : "Every app is connected."}
          </p>
        ) : (
          <div className="mcp-integration-list">
            {shown.map((entry) => (
              <article key={entry.slug} className="mcp-preset-available">
                <Identity {...entry} />
                <div className="mcp-preset-connect">
                  {signingIn === entry.slug ? (
                    <button
                      className="button"
                      type="button"
                      onClick={() => cancel(entry.slug)}
                    >
                      Cancel sign-in
                    </button>
                  ) : (
                    <button
                      className="button primary"
                      type="button"
                      disabled={busy !== null}
                      onClick={() => void connect(entry)}
                    >
                      {busy === entry.slug ? "Connecting…" : "Connect"}
                    </button>
                  )}
                </div>
              </article>
            ))}
          </div>
        )}
        {!showAll && !query.trim() && available.length > FIRST_PAGE && (
          <button
            className="button ghost composio-more"
            type="button"
            onClick={() => setShowAll(true)}
          >
            Show all {available.length} apps
          </button>
        )}
      </section>
    </>
  );
}
