/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { Box, Github, UserRound } from "lucide-react";
import { commands, type McpProviderStatus } from "@/bindings";
import { Switch } from "@/components/ui/Switch";
import {
  filterExtensions,
  signInCancelled,
  unwrapResult,
} from "./extensionRuntime";
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

function PresetAccount({
  id,
  lookup,
}: {
  id: string;
  lookup: (id: string) => Promise<string | null>;
}) {
  const [label, setLabel] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void lookup(id).then((name) => {
      if (active) setLabel(name);
    });
    return () => {
      active = false;
    };
  }, [id, lookup]);

  return <strong>{label ?? "Account connected"}</strong>;
}

export function McpPresets({
  query,
  customConnections,
}: {
  query: string;
  customConnections: ReactNode;
}) {
  const [rows, setRows] = useState<McpProviderStatus[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [signingIn, setSigningIn] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const operation = useRef(false);
  const pending = useRef<string | null>(null);
  const mounted = useRef(false);
  const accountLookups = useRef(new Map<string, Promise<string | null>>());

  const lookupAccount = useCallback((id: string) => {
    const cached = accountLookups.current.get(id);
    if (cached) return cached;
    const request = commands
      .mcpPresetAccountLabel(id)
      .then(unwrapResult)
      .catch(() => null);
    accountLookups.current.set(id, request);
    return request;
  }, []);

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
        accountLookups.current.delete(row.id);
      } catch (reason) {
        if (mounted.current) setMessage(null);
        if (signInCancelled(reason)) return;
        throw reason;
      } finally {
        pending.current = null;
        if (mounted.current) setSigningIn(null);
      }
      if (mounted.current) setMessage(`${row.name} connected.`);
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

  const visible = filterExtensions(rows ?? [], query);
  const connected = visible.filter((row) => row.connected);
  const available = visible.filter((row) => !row.connected);

  const signInButton = (row: McpProviderStatus) =>
    signingIn === row.id ? (
      <button
        className="button"
        type="button"
        disabled={cancelling}
        onClick={() => void cancel(row.id)}
      >
        Cancel sign-in
      </button>
    ) : (
      <button
        className="button primary"
        type="button"
        disabled={busy !== null || row.state === "needs_client_credentials"}
        onClick={() => void connect(row)}
      >
        {busy === row.id
          ? "Connecting…"
          : row.connected
            ? "Reconnect"
            : "Connect"}
      </button>
    );

  const identity = (row: McpProviderStatus) => (
    <div className="mcp-preset-identity">
      <PresetIcon id={row.id} />
      <div>
        <h3>{row.name}</h3>
        <p>{row.description}</p>
      </div>
    </div>
  );

  return (
    <>
      {message && (
        <p className="mcp-integration-feedback" role="status">
          {message}
        </p>
      )}
      {error && (
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
      {!rows && !error && (
        <p className="mcp-integration-feedback" role="status">
          Loading integrations…
        </p>
      )}
      {connected.length > 0 && (
        <section
          className="mcp-integration-section"
          aria-label="Connected integrations"
        >
          <h2>Connected</h2>
          <div className="mcp-integration-list">
            {connected.map((row) => (
              <article key={row.id} className="mcp-preset-connected">
                {identity(row)}
                <div className="mcp-preset-account">
                  <UserRound size={16} aria-hidden="true" />
                  <div className="mcp-preset-account-copy">
                    {signingIn === row.id ? (
                      <strong>Signing in…</strong>
                    ) : (
                      <PresetAccount id={row.id} lookup={lookupAccount} />
                    )}
                    <p>{connectionStatus(row)}</p>
                  </div>
                  {signingIn === row.id ||
                  row.state === "reconnect_required" ? (
                    signInButton(row)
                  ) : (
                    <Switch
                      checked={row.enabled}
                      disabled={
                        busy !== null ||
                        row.state === "needs_client_credentials"
                      }
                      ariaLabel={`Enable ${row.name}`}
                      onChange={(enabled) =>
                        void run(row.id, async () => {
                          unwrapResult(
                            await commands.mcpSetProviderEnabled(
                              row.id,
                              enabled,
                            ),
                          );
                        })
                      }
                    />
                  )}
                </div>
                <details className="mcp-preset-settings">
                  <summary>Manage connection</summary>
                  <div className="mcp-connection-actions">
                    <button
                      className="button"
                      type="button"
                      disabled={
                        busy !== null ||
                        !row.enabled ||
                        row.state === "needs_client_credentials"
                      }
                      onClick={() => void run(row.id, () => test(row))}
                    >
                      Check tools
                    </button>
                    <button
                      className="button"
                      type="button"
                      disabled={busy !== null}
                      onClick={() =>
                        void run(row.id, async () => {
                          unwrapResult(
                            await commands.mcpDisconnectProvider(row.id),
                          );
                          accountLookups.current.delete(row.id);
                        })
                      }
                    >
                      Disconnect
                    </button>
                  </div>
                </details>
              </article>
            ))}
          </div>
        </section>
      )}
      {customConnections}
      {rows && (
        <section
          className="mcp-integration-section"
          aria-label="Available integrations"
        >
          <h2>Available integrations</h2>
          {available.length === 0 ? (
            <p className="mcp-integration-feedback">
              {query.trim()
                ? "No available integrations match your search."
                : "All bundled integrations are connected."}
            </p>
          ) : (
            <div className="mcp-integration-list">
              {available.map((row) => (
                <article key={row.id} className="mcp-preset-available">
                  {identity(row)}
                  <div className="mcp-preset-connect">
                    {signInButton(row)}
                    {row.state === "needs_client_credentials" && (
                      <small>Setup required</small>
                    )}
                  </div>
                </article>
              ))}
            </div>
          )}
        </section>
      )}
    </>
  );
}
