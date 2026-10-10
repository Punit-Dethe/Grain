/* eslint-disable i18next/no-literal-string -- extension UI copy follows the current untranslated Extensions surface. */
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type FormEvent,
} from "react";
import {
  commands,
  type ConnectionView,
  type McpProviderStatus,
} from "@/bindings";
import { unwrapResult } from "./extensionRuntime";
import "./mcp-connections.css";

type Connection = ConnectionView;
type AccountStatus = Pick<
  McpProviderStatus,
  "connected" | "enabled" | "state" | "client_id_configured"
>;
type Row = Connection & { account?: AccountStatus; statusError?: string };
const owner = (row: Connection) => [row.id, row.revision] as const;

export function McpConnections({
  query,
  source = "configured",
  connectionId,
  onChange,
  onRemoved,
}: {
  query: string;
  source?: "configured" | "store";
  connectionId?: string;
  onChange?: () => Promise<void>;
  onRemoved?: () => void;
}) {
  const titleId = useId();
  const [rows, setRows] = useState<Row[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [editing, setEditing] = useState<Connection | null>(null);
  const [adding, setAdding] = useState(false);
  const [jsonMode, setJsonMode] = useState(false);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [authentication, setAuthentication] = useState("oauth");
  const [json, setJson] = useState("");
  const [removing, setRemoving] = useState<string | null>(null);
  const live = useRef(false);
  const request = useRef(0);
  const active = useRef(false);
  const pendingSignIn = useRef<Connection | null>(null);
  const cancelActive = useRef(false);
  const addButton = useRef<HTMLButtonElement>(null);
  const returnFocus = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!adding && !removing && !busy && returnFocus.current) {
      const target = returnFocus.current;
      returnFocus.current = null;
      (target.isConnected ? target : addButton.current)?.focus();
    }
  }, [adding, removing, busy]);

  const refresh = useCallback(async () => {
    const serial = ++request.current;
    const records = unwrapResult(await commands.mcpConnectionsList());
    const next = await Promise.all(
      records
        .filter(
          (row) =>
            row.source === source && (!connectionId || row.id === connectionId),
        )
        .map(async (row): Promise<Row> => {
          try {
            return {
              ...row,
              account: unwrapResult(
                await commands.mcpConnectionStatus(...owner(row)),
              ),
            };
          } catch (reason) {
            return { ...row, statusError: String(reason) };
          }
        }),
    );
    if (live.current && serial === request.current) setRows(next);
  }, [source, connectionId]);

  useEffect(() => {
    live.current = true;
    void refresh()
      .catch((reason) => live.current && setError(String(reason)))
      .finally(() => live.current && setLoading(false));
    return () => {
      live.current = false;
      request.current++;
      const pending = pendingSignIn.current;
      if (pending)
        void commands
          .mcpConnectionDisconnect(...owner(pending))
          .then(unwrapResult)
          .catch(() => {});
    };
  }, [refresh]);

  const run = async (id: string, action: () => Promise<unknown>) => {
    if (active.current) return;
    active.current = true;
    setBusy(id);
    setError(null);
    try {
      await action();
    } catch (reason) {
      if (live.current) setError(String(reason));
    } finally {
      if (live.current) {
        await refresh().catch(
          (reason) =>
            live.current && setError((current) => current ?? String(reason)),
        );
        if (live.current)
          await onChange?.().catch(
            (reason) => live.current && setError(String(reason)),
          );
        if (live.current) setBusy(null);
      }
      active.current = false;
    }
  };

  const connect = (row: Connection) =>
    void run(row.id, async () => {
      pendingSignIn.current = row;
      setConnecting(row.id);
      try {
        unwrapResult(await commands.mcpConnectionConnect(...owner(row)));
      } finally {
        pendingSignIn.current = null;
        if (live.current) setConnecting(null);
      }
    });

  const cancel = async (row: Connection) => {
    if (cancelActive.current) return;
    cancelActive.current = true;
    setCancelling(true);
    try {
      unwrapResult(await commands.mcpConnectionDisconnect(...owner(row)));
    } catch (reason) {
      if (live.current) setError(String(reason));
    } finally {
      cancelActive.current = false;
      if (live.current) setCancelling(false);
    }
  };

  const openForm = (row: Connection | null, trigger: HTMLButtonElement) => {
    returnFocus.current = trigger;
    setEditing(row);
    setAdding(true);
    setJsonMode(false);
    setName(row?.name ?? "");
    setUrl(row?.url ?? "");
    setAuthentication(row?.authentication ?? "oauth");
    setJson(
      JSON.stringify(
        {
          name: row?.name ?? "",
          url: row?.url ?? "",
          authentication: { type: row?.authentication ?? "oauth" },
        },
        null,
        2,
      ),
    );
    setError(null);
  };

  const save = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const definitionJson = jsonMode
      ? json
      : JSON.stringify({ name, url, authentication: { type: authentication } });
    void run("form", async () => {
      unwrapResult(
        await (editing
          ? commands.mcpConnectionReplace(...owner(editing), definitionJson)
          : commands.mcpConnectionImport(definitionJson)),
      );
      if (live.current) setAdding(false);
    });
  };

  const saveClient = (event: FormEvent<HTMLFormElement>, row: Connection) => {
    event.preventDefault();
    if (active.current) return;
    const data = new FormData(event.currentTarget);
    const clientId = String(data.get("clientId") ?? "");
    const clientSecret = String(data.get("clientSecret") ?? "");
    event.currentTarget.reset();
    void run(row.id, () =>
      commands
        .mcpConnectionSetClientCredentials(
          ...owner(row),
          clientId,
          clientSecret,
        )
        .then(unwrapResult),
    );
  };

  const matching = rows.filter((row) =>
    `${row.name} ${row.url}`.toLowerCase().includes(query.toLowerCase().trim()),
  );
  return (
    <section className="mcp-connections" aria-labelledby={titleId}>
      <header className="mcp-connections-heading">
        <div>
          <h2 id={titleId}>
            {source === "store" ? "Settings" : "Your MCP connections"}
          </h2>
          <p>
            {source === "store"
              ? "Manage your account and whether this extension is enabled."
              : "Connect a remote tool server directly. No extension package is needed."}
          </p>
        </div>
        <div className="mcp-connection-actions">
          <button
            ref={source === "store" ? addButton : undefined}
            type="button"
            className="button ghost"
            disabled={busy !== null || loading}
            onClick={() => void run("refresh", async () => {})}
          >
            Refresh
          </button>
          {source === "configured" && (
            <button
              ref={addButton}
              type="button"
              className="button"
              disabled={busy !== null}
              onClick={(event) => openForm(null, event.currentTarget)}
            >
              Add MCP
            </button>
          )}
        </div>
      </header>
      {error && (
        <p className="extension-inline-error" role="alert">
          {error}
        </p>
      )}
      {adding && (
        <form className="mcp-connection-form" onSubmit={save}>
          <h3>{editing ? `Edit ${editing.name}` : "Add an MCP connection"}</h3>
          <label className="mcp-json-choice">
            <input
              type="checkbox"
              checked={jsonMode}
              disabled={busy !== null}
              onChange={(event) => {
                if (event.target.checked)
                  setJson(
                    JSON.stringify(
                      { name, url, authentication: { type: authentication } },
                      null,
                      2,
                    ),
                  );
                else {
                  try {
                    const value = JSON.parse(json);
                    if (
                      !value ||
                      Object.keys(value).sort().join() !==
                        "authentication,name,url" ||
                      typeof value.name !== "string" ||
                      typeof value.url !== "string" ||
                      !value.authentication ||
                      Object.keys(value.authentication).join() !== "type" ||
                      !["oauth", "none"].includes(value.authentication.type)
                    )
                      throw new Error(
                        "Use name, url and authentication.type before switching to the form.",
                      );
                    setName(value.name);
                    setUrl(value.url);
                    setAuthentication(value.authentication.type);
                  } catch (reason) {
                    setError(String(reason));
                    return;
                  }
                }
                setError(null);
                setJsonMode(event.target.checked);
              }}
            />{" "}
            Enter JSON instead
          </label>
          {jsonMode ? (
            <label>
              Connection JSON
              <textarea
                required
                value={json}
                maxLength={8192}
                spellCheck={false}
                disabled={busy !== null}
                onChange={(event) => setJson(event.target.value)}
                rows={7}
              />
              <small>
                Use name, url and authentication.type (oauth or none).
                Credentials are entered separately.
              </small>
            </label>
          ) : (
            <>
              <label>
                Name
                <input
                  required
                  value={name}
                  maxLength={120}
                  autoFocus
                  disabled={busy !== null}
                  onChange={(event) => setName(event.target.value)}
                />
              </label>
              <label>
                MCP server URL
                <input
                  required
                  type="url"
                  pattern="https://.*"
                  value={url}
                  placeholder="https://example.com/mcp"
                  disabled={busy !== null}
                  onChange={(event) => setUrl(event.target.value)}
                />
                <small>Remote HTTPS servers using Streamable HTTP.</small>
              </label>
              <label>
                Authentication
                <select
                  value={authentication}
                  disabled={busy !== null}
                  onChange={(event) => setAuthentication(event.target.value)}
                >
                  <option value="oauth">Browser sign-in (OAuth)</option>
                  <option value="none">No authentication</option>
                </select>
              </label>
            </>
          )}
          {editing && (
            <p>
              Changing the server URL or authentication signs out the old
              connection and disables it.
            </p>
          )}
          <div className="mcp-connection-actions">
            <button className="button" type="submit" disabled={busy !== null}>
              {busy === "form" ? "Saving..." : "Save connection"}
            </button>
            <button
              className="button ghost"
              type="button"
              disabled={busy !== null}
              onClick={() => setAdding(false)}
            >
              Cancel
            </button>
          </div>
        </form>
      )}
      {loading ? (
        <p role="status">
          {source === "store"
            ? "Loading settings…"
            : "Loading MCP connections…"}
        </p>
      ) : matching.length === 0 ? (
        <p>
          {rows.length
            ? "No MCP connections match your search."
            : source === "store"
              ? "This extension is no longer installed."
              : "Add your first MCP to make its tools available to the Agent."}
        </p>
      ) : (
        matching.map((row) => (
          <article className="mcp-connection-row" key={row.id}>
            <div className="mcp-connection-copy">
              {source === "configured" && (
                <>
                  <h3>{row.name}</h3>
                  <p className="mcp-connection-url">{row.url}</p>
                </>
              )}
              <p>
                {row.statusError ??
                  (row.authentication === "none"
                    ? "No account needed"
                    : row.account?.state === "stored"
                      ? "Account saved"
                      : row.account?.state === "needs_client_credentials"
                        ? "App setup required"
                        : row.account?.state === "disconnected"
                          ? "Sign in to connect your account"
                          : row.account?.state.replace(/_/g, " "))}{" "}
                &middot;{" "}
                {row.account?.enabled
                  ? "Enabled"
                  : row.account
                    ? "Disabled"
                    : "Status unavailable"}
              </p>
            </div>
            <div className="mcp-connection-actions">
              {connecting === row.id ? (
                <button
                  className="button"
                  type="button"
                  disabled={cancelling}
                  onClick={() => void cancel(row)}
                >
                  {cancelling ? "Cancelling..." : "Cancel sign-in"}
                </button>
              ) : (
                <>
                  {row.authentication === "oauth" && (
                    <button
                      className="button"
                      type="button"
                      disabled={busy !== null}
                      onClick={() => connect(row)}
                    >
                      {row.account?.connected
                        ? "Sign in again"
                        : "Sign in & enable"}
                    </button>
                  )}
                  {row.account &&
                    (row.authentication === "none" ||
                      row.account.connected) && (
                      <button
                        className="button"
                        type="button"
                        disabled={busy !== null}
                        onClick={() =>
                          void run(row.id, () =>
                            commands
                              .mcpConnectionSetEnabled(
                                ...owner(row),
                                !row.account?.enabled,
                              )
                              .then(unwrapResult),
                          )
                        }
                      >
                        {row.account.enabled ? "Disable" : "Enable"}
                      </button>
                    )}
                  {row.account?.connected && (
                    <button
                      className="button ghost"
                      type="button"
                      disabled={busy !== null}
                      onClick={() =>
                        void run(row.id, () =>
                          commands
                            .mcpConnectionDisconnect(...owner(row))
                            .then(unwrapResult),
                        )
                      }
                    >
                      Sign out
                    </button>
                  )}
                  {source === "configured" && (
                    <button
                      className="button ghost"
                      type="button"
                      disabled={busy !== null}
                      onClick={(event) => openForm(row, event.currentTarget)}
                    >
                      Edit
                    </button>
                  )}
                  <button
                    className="button ghost"
                    type="button"
                    disabled={busy !== null}
                    onClick={(event) => {
                      returnFocus.current = event.currentTarget;
                      setRemoving(row.id);
                    }}
                  >
                    {source === "store" ? "Uninstall" : "Remove"}
                  </button>
                </>
              )}
            </div>
            {removing === row.id && (
              <div className="mcp-connection-confirm">
                <p>
                  {source === "store" ? "Uninstall" : "Remove"} {row.name}? Its
                  tools will be disabled and its saved account cleared.
                </p>
                <div className="mcp-connection-actions">
                  <button
                    type="button"
                    className="button danger"
                    disabled={busy !== null}
                    onClick={() =>
                      void run(row.id, async () => {
                        unwrapResult(
                          await commands.mcpConnectionRemove(...owner(row)),
                        );
                        if (live.current) setRemoving(null);
                        if (onRemoved) await onChange?.();
                        if (live.current) onRemoved?.();
                      })
                    }
                  >
                    {source === "store"
                      ? "Uninstall extension"
                      : "Remove connection"}
                  </button>
                  <button
                    type="button"
                    className="button ghost"
                    disabled={busy !== null}
                    onClick={() => setRemoving(null)}
                  >
                    {source === "store" ? "Keep extension" : "Keep connection"}
                  </button>
                </div>
              </div>
            )}
            {row.authentication === "oauth" && (
              <details className="mcp-client-settings">
                <summary>
                  {source === "store"
                    ? "Advanced connection settings"
                    : "OAuth app credentials"}{" "}
                  {row.account?.client_id_configured
                    ? "(configured)"
                    : "(optional)"}
                </summary>
                <p>
                  Only needed if this server requires your own OAuth app.
                  Secrets go to the system vault.
                </p>
                {source === "store" && (
                  <p className="mcp-connection-url">Server: {row.url}</p>
                )}
                <p className="mcp-connection-url">
                  Register redirect URI:{" "}
                  <code>http://127.0.0.1:31938/mcp/oauth/callback</code>
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
                    Client secret (if required)
                    <input
                      name="clientSecret"
                      type="password"
                      autoComplete="new-password"
                      disabled={busy !== null}
                    />
                  </label>
                  <div className="mcp-connection-actions">
                    <button
                      className="button"
                      type="submit"
                      disabled={busy !== null}
                    >
                      Save app credentials
                    </button>
                    {row.account?.client_id_configured && (
                      <button
                        className="button ghost"
                        type="button"
                        disabled={busy !== null}
                        onClick={() =>
                          void run(row.id, () =>
                            commands
                              .mcpConnectionClearClientCredentials(
                                ...owner(row),
                              )
                              .then(unwrapResult),
                          )
                        }
                      >
                        Reset app credentials
                      </button>
                    )}
                  </div>
                </form>
              </details>
            )}
          </article>
        ))
      )}
    </section>
  );
}
