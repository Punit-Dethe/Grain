/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import { Box } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, type McpProviderStatus } from "@/bindings";
import { Switch } from "@/components/ui/Switch";
import {
  filterExtensions,
  signInCancelled,
  unwrapResult,
} from "./extensionRuntime";
import githubIcon from "./icons/github.svg";
import linearIcon from "./icons/linear.svg";
import linearDarkIcon from "./icons/linear-light.svg";
import notionIcon from "./icons/notion.png";
import "./mcp-connections.css";
import "./mcp-presets.css";

const presetIcons = new Map<string, { src: string; darkSrc?: string }>([
  ["github", { src: githubIcon }],
  ["linear", { src: linearIcon, darkSrc: linearDarkIcon }],
  ["notion", { src: notionIcon }],
]);

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
  const refresh = useCallback(async () => {
    // Development entries are listed only when the backend grants access
    // (development build + Extension Developer Mode); releases see presets.
    const status = unwrapResult(await commands.mcpProviderStatus(false));
    if (mounted.current) setRows(status);
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
  // Local servers and no-account fixtures have no sign-in: a switch turns
  // them on (installing a local server first) and off.
  const noAccount = (row: McpProviderStatus) =>
    row.state === "local" || row.state === "fixture_no_auth";
  const isConnected = (row: McpProviderStatus) =>
    row.connected || (noAccount(row) && row.enabled);
  const connected = visible.filter(isConnected);
  const available = visible.filter((row) => !isConnected(row));

  const setEnabled = (row: McpProviderStatus, enabled: boolean) =>
    run(row.id, async () => {
      if (enabled && row.state === "local")
        setMessage(`Installing ${row.name}… this can take a minute.`);
      unwrapResult(await commands.mcpSetProviderEnabled(row.id, enabled));
      if (mounted.current) setMessage(null);
    });

  const saveClient = (
    event: FormEvent<HTMLFormElement>,
    row: McpProviderStatus,
  ) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const clientId = String(data.get("clientId") ?? "");
    const clientSecret = String(data.get("clientSecret") ?? "");
    event.currentTarget.reset();
    void run(row.id, async () => {
      unwrapResult(
        await commands.mcpSetClientCredentials(row.id, clientId, clientSecret),
      );
      if (mounted.current)
        setMessage("OAuth client saved. Connect to sign in.");
    });
  };

  // Providers that sign in with the user's own OAuth client (Google
  // Workspace) take its ID and secret here. Secrets go to the OS vault.
  const clientSettings = (row: McpProviderStatus) =>
    row.requires_client_credentials && (
      <details
        className="mcp-client-settings mcp-preset-client"
        open={!row.client_id_configured}
      >
        <summary>
          OAuth client {row.client_id_configured ? "(saved)" : "(required)"}
        </summary>
        <p className="mcp-integration-feedback">
          Create a Desktop app OAuth client for your own account, then paste its
          ID and secret.{" "}
          <button
            className="button ghost"
            type="button"
            onClick={() => void openUrl(row.setup_url)}
          >
            Open setup page
          </button>
        </p>
        <form
          className="mcp-connection-form"
          onSubmit={(event) => saveClient(event, row)}
        >
          <label>
            Client ID
            <input
              name="clientId"
              required
              autoComplete="off"
              disabled={busy !== null}
            />
          </label>
          <label>
            Client secret
            <input
              name="clientSecret"
              type="password"
              required
              autoComplete="new-password"
              disabled={busy !== null}
            />
          </label>
          <div className="mcp-connection-actions">
            <button className="button" type="submit" disabled={busy !== null}>
              Save OAuth client
            </button>
          </div>
        </form>
      </details>
    );

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
        disabled={busy !== null}
        onClick={() => void connect(row)}
      >
        {busy === row.id ? "Connecting…" : "Connect"}
      </button>
    );

  const identity = (row: McpProviderStatus) => {
    const icon = presetIcons.get(row.id);
    return (
      <div className="mcp-preset-identity">
        {icon ? (
          <>
            <img
              className="mcp-brand-icon"
              data-provider={row.id}
              data-variant={icon.darkSrc ? "light" : undefined}
              src={icon.src}
              alt=""
              width={24}
              height={24}
            />
            {icon.darkSrc && (
              <img
                className="mcp-brand-icon"
                data-variant="dark"
                src={icon.darkSrc}
                alt=""
                width={24}
                height={24}
              />
            )}
          </>
        ) : (
          <span className="mcp-preset-icon" aria-hidden="true">
            <Box size={17} />
          </span>
        )}
        <div>
          <h3>{row.name}</h3>
          <p>{row.description}</p>
        </div>
      </div>
    );
  };

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
          Loading extensions…
        </p>
      )}
      {connected.length > 0 && (
        <section
          className="mcp-integration-section"
          aria-label="Connected extensions"
        >
          <h2>Connected</h2>
          <div className="mcp-integration-list">
            {connected.map((row) => (
              <article key={row.id} className="mcp-preset-connected">
                {identity(row)}
                <div className="mcp-preset-actions">
                  {!noAccount(row) && (
                    <button
                      className="button ghost"
                      type="button"
                      aria-label={`Disconnect ${row.name}`}
                      disabled={busy !== null}
                      onClick={() =>
                        void run(row.id, async () => {
                          unwrapResult(
                            await commands.mcpDisconnectProvider(row.id),
                          );
                        })
                      }
                    >
                      Disconnect
                    </button>
                  )}
                  <Switch
                    checked={row.enabled}
                    disabled={busy !== null}
                    ariaLabel={`Enable ${row.name}`}
                    onChange={(enabled) => void setEnabled(row, enabled)}
                  />
                </div>
              </article>
            ))}
          </div>
        </section>
      )}
      {customConnections}
      {rows && (
        <section
          className="mcp-integration-section"
          aria-label="Available extensions"
        >
          <h2>Available extensions</h2>
          {available.length === 0 ? (
            <p className="mcp-integration-feedback">
              {query.trim()
                ? "No available extensions match your search."
                : "All bundled extensions are connected."}
            </p>
          ) : (
            <div className="mcp-integration-list">
              {available.map((row) => (
                <article key={row.id} className="mcp-preset-available">
                  {identity(row)}
                  <div className="mcp-preset-connect">
                    {noAccount(row) ? (
                      <Switch
                        checked={false}
                        disabled={busy !== null}
                        ariaLabel={`Enable ${row.name}`}
                        onChange={(enabled) => void setEnabled(row, enabled)}
                      />
                    ) : (
                      (!row.requires_client_credentials ||
                        row.client_id_configured) &&
                      signInButton(row)
                    )}
                  </div>
                  {clientSettings(row)}
                </article>
              ))}
            </div>
          )}
        </section>
      )}
    </>
  );
}
