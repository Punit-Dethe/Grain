/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowUpRight, Box } from "lucide-react";
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

function Logo({ src, size = 22 }: { src: string; size?: number }) {
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
      <div className="ext-setup-copy">
        <h3>Connect your apps</h3>
        <p>
          Gmail, Google Calendar, Slack, Notion, GitHub and hundreds more, each
          one click away. Grain reaches them through your own free Composio
          account, which covers 20,000 app actions a month.
        </p>
      </div>
      <ol className="ext-steps">
        <li>
          <span>
            Create a free account at Composio. If the top-left corner says “For
            You”, click <b>Switch</b> and pick the developer platform.
          </span>
          <button
            type="button"
            className="button ghost ext-inline-action"
            onClick={() => void openUrl(DASHBOARD_URL)}
          >
            Open Composio
            <ArrowUpRight size={14} aria-hidden="true" />
          </button>
        </li>
        <li>
          <span>
            Open <b>Settings → Project Settings → API Keys</b> and copy the key
            that starts with <code>ak_</code>.
          </span>
        </li>
        <li>
          <span>Paste it here.</span>
        </li>
      </ol>
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
        The key stays in your system’s credential store and is only ever sent to
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
  const [confirmRemove, setConfirmRemove] = useState(false);
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
    if (operation.current) return false;
    operation.current = true;
    setBusy(id);
    setError(null);
    setMessage(null);
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

  const saveKey = (key: string) =>
    run("key", async () => {
      apply(unwrapResult(await commands.composioSetApiKey(key)));
      await loadCatalog();
    });

  const removeKey = () =>
    void run("key", async () => {
      apply(unwrapResult(await commands.composioRemoveApiKey()));
      if (!mounted.current) return;
      setCatalog(null);
      setConfirmRemove(false);
    });

  const connect = (entry: ComposioCatalogEntry) =>
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
      if (mounted.current) setMessage(`${entry.name} is connected.`);
    });

  const cancel = (slug: string) =>
    void commands
      .composioCancelConnect(slug)
      .then(unwrapResult)
      .catch((reason) => mounted.current && setError(String(reason)));

  const feedback = (
    <>
      {message && (
        <p className="ext-feedback" role="status">
          {message}
        </p>
      )}
      {error && (
        <p className="extension-inline-error" role="alert">
          {error}
        </p>
      )}
    </>
  );

  const head = (
    <header className="ext-section-head">
      <h2 id="ext-apps">Apps</h2>
      <p>Your accounts in other apps, connected through Composio.</p>
    </header>
  );

  if (!status)
    return error ? (
      <section className="ext-section" aria-labelledby="ext-apps">
        {head}
        {feedback}
      </section>
    ) : null;

  if (!status.configured)
    return (
      <section className="ext-section" aria-labelledby="ext-apps">
        {head}
        {feedback}
        <KeySetup busy={busy === "key"} onSave={saveKey} />
      </section>
    );

  const searching = query.trim() !== "";
  const connected = filterExtensions(
    status.toolkits.map((toolkit) => ({ ...toolkit, id: toolkit.slug })),
    query,
  );
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
    <section className="ext-section" aria-labelledby="ext-apps">
      {head}
      <div className="ext-account">
        <span className="ext-account-dot" aria-hidden="true" />
        <span className="ext-account-label">
          Composio account connected
          <span>
            {status.toolkits.length === 1
              ? "1 app"
              : `${status.toolkits.length} apps`}{" "}
            in use
          </span>
        </span>
        {confirmRemove ? (
          <span className="ext-account-confirm">
            <span>
              Grain forgets every app here. They stay in your Composio account.
            </span>
            <button
              type="button"
              className="button danger"
              disabled={busy !== null}
              onClick={removeKey}
            >
              {busy === "key" ? "Removing…" : "Remove key"}
            </button>
            <button
              type="button"
              className="button ghost"
              disabled={busy !== null}
              onClick={() => setConfirmRemove(false)}
            >
              Keep
            </button>
          </span>
        ) : (
          <span className="ext-account-actions">
            <button
              type="button"
              className="button ghost"
              onClick={() => void openUrl(DASHBOARD_URL)}
            >
              Open Composio
              <ArrowUpRight size={14} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="button ghost"
              disabled={busy !== null}
              onClick={() => setConfirmRemove(true)}
            >
              Remove key
            </button>
          </span>
        )}
      </div>
      {feedback}

      {connected.length > 0 && (
        <div className="ext-group">
          <h3 className="ext-group-title">Connected</h3>
          <div className="ext-list">
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
                  {busy === toolkit.slug ? "Disconnecting…" : "Disconnect"}
                </button>
                <Switch
                  checked={toolkit.enabled}
                  disabled={busy !== null}
                  ariaLabel={`Use ${toolkit.name}`}
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
            ))}
          </div>
        </div>
      )}

      <div className="ext-group">
        <h3 className="ext-group-title">
          {searching ? "Matching apps" : "Browse apps"}
        </h3>
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
                        onClick={() => cancel(entry.slug)}
                      >
                        Cancel
                      </button>
                    ) : (
                      <button
                        className="button ext-tile-action"
                        type="button"
                        aria-label={`Connect ${entry.name}`}
                        disabled={busy !== null}
                        onClick={() => connect(entry)}
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
      </div>
    </section>
  );
}
