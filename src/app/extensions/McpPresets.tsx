/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowLeft, Box, Github } from "lucide-react";
import { commands, type McpProviderStatus } from "@/bindings";
import { Switch } from "@/components/ui/Switch";
import { hashForRoute } from "../navigation";
import {
  filterExtensions,
  signInCancelled,
  unwrapResult,
} from "./extensionRuntime";
import { presetArtwork } from "./presetArtwork";
import "./mcp-presets.css";

const presetIds = new Set(["github", "notion"]);

function PresetIcon({ id }: { id: string }) {
  return (
    <span className="mcp-preset-icon" aria-hidden="true">
      {id === "github" ? (
        <Github size={17} />
      ) : id === "notion" ? (
        <span className="mcp-preset-notion-mark">N</span>
      ) : (
        <Box size={17} />
      )}
    </span>
  );
}

function PresetBanner({ id, lazy }: { id: string; lazy?: boolean }) {
  const artwork = presetArtwork.get(id);
  return (
    <div className="mcp-preset-banner" aria-hidden="true">
      {artwork ? (
        <img src={artwork} alt="" loading={lazy ? "lazy" : undefined} />
      ) : (
        <PresetIcon id={id} />
      )}
    </div>
  );
}

function connectionStatus(row: McpProviderStatus) {
  switch (row.state) {
    case "needs_client_credentials":
      return `This build is missing the ${row.name} OAuth registration.`;
    case "reconnect_required":
      return "Reconnect your account to restore access.";
    case "needs_client_registration":
      return "This provider requires OAuth client registration.";
    case "configuration_required":
      return "The provider’s authorization configuration needs attention.";
    case "credential_store_unavailable":
    case "unavailable":
      return "Grain could not read the saved connection.";
    case "temporarily_unavailable":
      return "The provider is temporarily unavailable. Try again later.";
    default:
      if (!row.connected) return "Not connected";
      return row.enabled ? "Connected and enabled" : "Connected and disabled";
  }
}

export function McpPresets({
  browse,
  query,
  provider,
}: {
  browse: boolean;
  query: string;
  provider?: string;
}) {
  const [rows, setRows] = useState<McpProviderStatus[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [signingIn, setSigningIn] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [feedbackProvider, setFeedbackProvider] = useState<string | null>(null);
  const operation = useRef(false);
  const pending = useRef<string | null>(null);
  const mounted = useRef(false);
  const detailHeading = useRef<HTMLHeadingElement>(null);

  const refresh = useCallback(async () => {
    const status = unwrapResult(await commands.mcpProviderStatus());
    if (mounted.current) setRows(status.filter((row) => presetIds.has(row.id)));
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh().catch((reason) => {
      if (mounted.current) setError(String(reason));
    });
    return () => {
      mounted.current = false;
      if (pending.current)
        void commands.mcpCancelSignIn(pending.current).catch(() => {});
    };
  }, [refresh]);

  const run = async (id: string, action: () => Promise<void>) => {
    if (operation.current) return;
    operation.current = true;
    setBusy(id);
    setFeedbackProvider(id === "refresh" ? null : id);
    setError(null);
    setMessage(null);
    try {
      await action();
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      if (mounted.current) {
        await refresh().catch((reason) =>
          setError((current) => current ?? String(reason)),
        );
        setBusy(null);
      }
      operation.current = false;
    }
  };

  const test = async (row: McpProviderStatus) => {
    setMessage("Checking tools…");
    try {
      const discovery = unwrapResult(await commands.mcpTestProvider(row.id));
      if (mounted.current) {
        setMessage(
          discovery.tool_count
            ? [row.name, discovery.tool_count, "tools available"].join(" ")
            : [row.name, "reported no tools."].join(" "),
        );
      }
    } catch (reason) {
      if (mounted.current) setMessage(null);
      throw reason;
    }
  };

  const connect = (row: McpProviderStatus) =>
    run(row.id, async () => {
      pending.current = row.id;
      setSigningIn(row.id);
      setMessage("Finish signing in in your browser…");
      try {
        unwrapResult(await commands.mcpConnectProvider(row.id));
      } catch (reason) {
        if (mounted.current) setMessage(null);
        if (signInCancelled(reason)) return;
        throw reason;
      } finally {
        pending.current = null;
        if (mounted.current) setSigningIn(null);
      }
      if (mounted.current) await test(row);
    });

  const cancel = async (id: string) => {
    setCancelling(true);
    try {
      unwrapResult(await commands.mcpCancelSignIn(id));
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      if (mounted.current) setCancelling(false);
    }
  };

  const visible = filterExtensions(
    (rows ?? []).filter((row) => browse || row.connected),
    query,
  );
  const selected = provider
    ? rows?.find((row) => row.id === provider)
    : undefined;
  const selectedId = selected?.id;
  useEffect(() => {
    if (selectedId) detailHeading.current?.focus();
  }, [selectedId]);

  const view = browse ? "browse" : "installed";
  const showFeedback =
    !provider || !feedbackProvider || provider === feedbackProvider;
  const feedback = (
    <>
      {showFeedback && message && <p role="status">{message}</p>}
      {showFeedback && error && (
        <div className="extension-inline-error" role="alert">
          {error}
          {!rows && (
            <button
              className="button"
              type="button"
              disabled={busy !== null}
              onClick={() => void run("refresh", async () => {})}
            >
              Retry
            </button>
          )}
        </div>
      )}
      {!rows && !error && <p role="status">Loading connections…</p>}
    </>
  );

  const primaryAction = (row: McpProviderStatus) => {
    if (signingIn === row.id) {
      return (
        <button
          className="button"
          type="button"
          disabled={cancelling}
          onClick={() => void cancel(row.id)}
        >
          Cancel sign-in
        </button>
      );
    }

    const disabled = busy !== null || row.state === "needs_client_credentials";
    if (row.connected) {
      return (
        <Switch
          checked={row.enabled}
          disabled={disabled}
          ariaLabel={`Enable ${row.name}`}
          onChange={(enabled) =>
            void run(row.id, async () => {
              unwrapResult(
                await commands.mcpSetProviderEnabled(row.id, enabled),
              );
            })
          }
        />
      );
    }

    return (
      <button
        className="button primary"
        type="button"
        disabled={disabled}
        onClick={() => void connect(row)}
      >
        {busy === row.id ? "Connecting…" : "Connect"}
      </button>
    );
  };

  if (provider) {
    return (
      <div className="mcp-preset-detail">
        <a
          className="button mcp-preset-back"
          href={hashForRoute({ page: "extensions", view })}
        >
          <ArrowLeft size={14} aria-hidden="true" />
          {browse ? "Browse extensions" : "My connections"}
        </a>
        {selected ? (
          <>
            <PresetBanner id={selected.id} />
            <div className="mcp-preset-detail-heading">
              <PresetIcon id={selected.id} />
              <h1 ref={detailHeading} tabIndex={-1}>
                {selected.name}
              </h1>
            </div>
            <p className="page-subtitle">{selected.description}</p>
            <section
              className="mcp-connections"
              aria-label="Connection settings"
            >
              <header className="mcp-connections-heading">
                <h2>Connection settings</h2>
              </header>
              <p>{connectionStatus(selected)}</p>
              <div className="mcp-preset-detail-actions">
                {primaryAction(selected)}
                {selected.state === "reconnect_required" &&
                  selected.connected &&
                  signingIn !== selected.id && (
                    <button
                      className="button primary"
                      type="button"
                      disabled={busy !== null}
                      onClick={() => void connect(selected)}
                    >
                      Reconnect
                    </button>
                  )}
                {selected.connected && (
                  <>
                    <button
                      className="button"
                      type="button"
                      disabled={
                        busy !== null ||
                        !selected.enabled ||
                        selected.state === "needs_client_credentials"
                      }
                      onClick={() =>
                        void run(selected.id, () => test(selected))
                      }
                    >
                      Check tools
                    </button>
                    <button
                      className="button"
                      type="button"
                      disabled={busy !== null}
                      onClick={() =>
                        void run(selected.id, async () => {
                          unwrapResult(
                            await commands.mcpDisconnectProvider(selected.id),
                          );
                        })
                      }
                    >
                      Disconnect
                    </button>
                  </>
                )}
              </div>
              {feedback}
            </section>
          </>
        ) : (
          <>
            {feedback}
            {rows && <h1>Extension unavailable</h1>}
          </>
        )}
      </div>
    );
  }

  return (
    <section
      className="mcp-connections"
      aria-label={browse ? "Preconfigured connections" : "Connected apps"}
    >
      <header className="mcp-connections-heading">
        <h2>{browse ? "Preconfigured connections" : "Connected apps"}</h2>
      </header>
      {feedback}
      {rows && !visible.length && (
        <p>
          {query.trim()
            ? "No connections match your search."
            : browse
              ? "No preconfigured connections are available."
              : "No apps connected yet. Open Browse extensions to connect an app."}
        </p>
      )}
      <div className={browse ? "mcp-preset-grid" : "mcp-preset-list"}>
        {visible.map((row) => (
          <article
            key={row.id}
            className={browse ? "mcp-preset-card" : "mcp-preset-row"}
          >
            {browse && <PresetBanner id={row.id} lazy />}
            <div className="mcp-preset-entry">
              <a
                className="mcp-preset-link"
                href={hashForRoute({
                  page: "extensions",
                  view,
                  provider: row.id,
                })}
              >
                <PresetIcon id={row.id} />
                <strong>{row.name}</strong>
              </a>
              <div className="mcp-preset-action">{primaryAction(row)}</div>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
