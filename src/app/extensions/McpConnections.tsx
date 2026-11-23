/* eslint-disable i18next/no-literal-string -- extension UI copy follows the current untranslated Extensions surface. */
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import { Box, Globe, Plus, Server, Square, Terminal } from "lucide-react";
import {
  commands,
  type BrowseSetupStatus,
  type ConnectionView,
  type LocalMcpLogs,
  type McpDiscoveryResult,
  type McpProviderStatus,
} from "@/bindings";
import { signInCancelled, unwrapResult } from "./extensionRuntime";
import "./mcp-presets.css";
import "./mcp-connections.css";

type Connection = ConnectionView;
type TransportKind = "stdio" | "local-http" | "remote";
type AccountStatus = Pick<
  McpProviderStatus,
  "connected" | "enabled" | "state" | "client_id_configured"
>;
type Row = Connection & { account?: AccountStatus; statusError?: string };
type EnvEntry = { key: string; value: string; stored?: boolean };

const owner = (row: Connection) => [row.id, row.revision] as const;
const cancelSignIn = (row: Connection) =>
  commands.mcpCancelSignIn("configured-" + row.id).then(unwrapResult);

/** Split a command-line argument string into argv tokens respecting quotes. */
function splitArgs(raw: string): string[] {
  const args: string[] = [];
  let current = "";
  let quote: '"' | "'" | null = null;
  let inToken = false;
  for (let i = 0; i < raw.length; i++) {
    const ch = raw[i];
    if (quote) {
      if (ch === quote) {
        quote = null;
      } else {
        current += ch;
      }
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      inToken = true;
    } else if (/\s/.test(ch)) {
      if (inToken) {
        args.push(current);
        current = "";
        inToken = false;
      }
    } else {
      current += ch;
      inToken = true;
    }
  }
  if (inToken) args.push(current);
  return args;
}

function formatArgs(args: string[]): string {
  return args
    .map((arg) => (/[\s"']/.test(arg) ? `"${arg.replace(/"/g, '\\"')}"` : arg))
    .join(" ");
}

function commandPreview(row: Connection): string {
  if (!row.command) return "";
  const rest = formatArgs(row.args);
  return rest ? `${row.command} ${rest}` : row.command;
}

const EXAMPLE_JSON = JSON.stringify(
  {
    mcpServers: {
      browse: {
        command: "browse",
        args: ["mcp"],
      },
    },
  },
  null,
  2,
);

export function McpConnections({
  query,
  header,
  toolbar,
  actions,
  children,
}: {
  query: string;
  /** Page title block; it shares a row with "Add custom MCP" and stays fixed while the page scrolls. */
  header?: ReactNode;
  /** Page-wide actions placed before "Add custom MCP". */
  actions?: ReactNode;
  /** Fixed row under the title (search and page-wide actions). */
  toolbar?: ReactNode;
  children: (customConnections: ReactNode) => ReactNode;
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
  const [transport, setTransport] = useState<TransportKind>("stdio");
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [authentication, setAuthentication] = useState("oauth");
  const [headers, setHeaders] = useState<Connection["headers"]>({});
  const [oauthClient, setOAuthClient] =
    useState<Connection["oauthClient"]>(null);
  const [command, setCommand] = useState("");
  const [argsText, setArgsText] = useState("");
  const [cwd, setCwd] = useState("");
  const [envRows, setEnvRows] = useState<EnvEntry[]>([]);
  const [authHeaderName, setAuthHeaderName] = useState("");
  const [authHeaderValue, setAuthHeaderValue] = useState("");
  const [json, setJson] = useState("");
  const [removing, setRemoving] = useState<string | null>(null);
  const [confirmingEnable, setConfirmingEnable] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<
    Record<string, { ok: boolean; text: string }>
  >({});
  const [logsById, setLogsById] = useState<Record<string, LocalMcpLogs>>({});
  const [browse, setBrowse] = useState<BrowseSetupStatus | null>(null);
  const [browseNote, setBrowseNote] = useState<string | null>(null);
  const [browseCode, setBrowseCode] = useState<string | null>(null);
  const [copiedCode, setCopiedCode] = useState(false);
  const [uninstallingBrowse, setUninstallingBrowse] = useState(false);

  const live = useRef(false);
  const request = useRef(0);
  const active = useRef(false);
  const pendingSignIn = useRef<Connection | null>(null);
  const cancelActive = useRef(false);
  const addButton = useRef<HTMLButtonElement>(null);
  const returnFocus = useRef<HTMLButtonElement | null>(null);
  const confirmRemoval = useRef<HTMLButtonElement | null>(null);
  const confirmEnableButton = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (removing) confirmRemoval.current?.focus();
  }, [removing]);

  useEffect(() => {
    if (confirmingEnable) confirmEnableButton.current?.focus();
  }, [confirmingEnable]);

  useEffect(() => {
    if (
      !adding &&
      !removing &&
      !confirmingEnable &&
      !busy &&
      returnFocus.current
    ) {
      const target = returnFocus.current;
      returnFocus.current = null;
      (target.isConnected ? target : addButton.current)?.focus();
    }
  }, [adding, removing, confirmingEnable, busy]);

  const refresh = useCallback(async () => {
    const serial = ++request.current;
    const browsePromise = commands
      .mcpBrowseStatus(false)
      .then(unwrapResult)
      .catch(() => null);
    const recordsPromise = commands.mcpConnectionsList().then(unwrapResult);
    const browseStatus = await browsePromise;
    if (live.current && serial === request.current && browseStatus) {
      setBrowse(browseStatus);
    }
    const records = await recordsPromise;
    const next = await Promise.all(
      records.map(async (row): Promise<Row> => {
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
    if (live.current && serial === request.current) {
      setRows(next);
    }
  }, []);

  useEffect(() => {
    live.current = true;
    void refresh()
      .catch((reason) => live.current && setError(String(reason)))
      .finally(() => live.current && setLoading(false));
    return () => {
      live.current = false;
      request.current++;
      const pending = pendingSignIn.current;
      if (pending) void cancelSignIn(pending).catch(() => {});
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
      } catch (reason) {
        if (!signInCancelled(reason)) throw reason;
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
      await cancelSignIn(row);
    } catch (reason) {
      if (live.current) setError(String(reason));
    } finally {
      cancelActive.current = false;
      if (live.current) setCancelling(false);
    }
  };

  const buildFormPayload = (): string => {
    if (transport === "stdio") {
      const envKeys: string[] = [];
      const env: Record<string, string> = {};
      for (const entry of envRows) {
        const key = entry.key.trim();
        if (!key) continue;
        if (!envKeys.includes(key)) envKeys.push(key);
        if (entry.value.length > 0 || !entry.stored) {
          env[key] = entry.value;
        }
      }
      const payload: Record<string, unknown> = {
        transport: "stdio",
        name: name.trim(),
        command: command.trim(),
        args: splitArgs(argsText),
        ...(cwd.trim() ? { cwd: cwd.trim() } : {}),
        ...(envKeys.length ? { envKeys } : {}),
        ...(Object.keys(env).length ? { env } : {}),
      };
      return JSON.stringify(payload, null, 2);
    }
    if (transport === "local-http") {
      const payload: Record<string, unknown> = {
        transport: "local-http",
        name: name.trim(),
        url: url.trim(),
        ...(Object.keys(headers).length ? { headers } : {}),
        ...(authHeaderName.trim() && authHeaderValue.trim()
          ? {
              authHeader: {
                name: authHeaderName.trim(),
                value: authHeaderValue.trim(),
              },
            }
          : {}),
      };
      return JSON.stringify(payload, null, 2);
    }
    return JSON.stringify(
      {
        transport: "remote",
        name: name.trim(),
        url: url.trim(),
        authentication: { type: authentication },
        ...(Object.keys(headers).length ? { headers } : {}),
        ...(oauthClient ? { oauthClient } : {}),
      },
      null,
      2,
    );
  };

  const openForm = (row: Connection | null, trigger: HTMLButtonElement) => {
    returnFocus.current = trigger;
    setEditing(row);
    setAdding(true);
    setJsonMode(false);
    const kind: TransportKind =
      row?.transport === "remote" ||
      row?.transport === "local-http" ||
      row?.transport === "stdio"
        ? row.transport
        : "stdio";
    setTransport(kind);
    setName(row?.name ?? "");
    setUrl(row?.url ?? "");
    setAuthentication(row?.authentication ?? "oauth");
    setHeaders(row?.headers ?? {});
    setOAuthClient(row?.oauthClient ?? null);
    setCommand(row?.command ?? "");
    setArgsText(row ? formatArgs(row.args) : "");
    setCwd(row?.cwd ?? "");
    setEnvRows(
      (row?.envKeys ?? []).map((key) => ({ key, value: "", stored: true })),
    );
    setAuthHeaderName("");
    setAuthHeaderValue("");
    setJson(
      row
        ? JSON.stringify(
            row.transport === "stdio"
              ? {
                  transport: "stdio",
                  name: row.name,
                  command: row.command ?? "",
                  args: row.args,
                  ...(row.cwd ? { cwd: row.cwd } : {}),
                  ...(row.envKeys.length ? { envKeys: row.envKeys } : {}),
                }
              : row.transport === "local-http"
                ? {
                    transport: "local-http",
                    name: row.name,
                    url: row.url,
                    ...(Object.keys(row.headers).length
                      ? { headers: row.headers }
                      : {}),
                  }
                : {
                    transport: "remote",
                    name: row.name,
                    url: row.url,
                    authentication: { type: row.authentication },
                    ...(Object.keys(row.headers).length
                      ? { headers: row.headers }
                      : {}),
                    ...(row.oauthClient
                      ? { oauthClient: row.oauthClient }
                      : {}),
                  },
            null,
            2,
          )
        : EXAMPLE_JSON,
    );
    setError(null);
  };

  const save = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const definitionJson = jsonMode ? json : buildFormPayload();
    void run("form", async () => {
      if (editing) {
        unwrapResult(
          await commands.mcpConnectionReplace(
            ...owner(editing),
            definitionJson,
          ),
        );
      } else if (jsonMode) {
        unwrapResult(await commands.mcpConnectionImportBundle(definitionJson));
      } else {
        unwrapResult(await commands.mcpConnectionImport(definitionJson));
      }
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

  const saveLocalAuth = (
    event: FormEvent<HTMLFormElement>,
    row: Connection,
  ) => {
    event.preventDefault();
    if (active.current) return;
    const data = new FormData(event.currentTarget);
    const headerName = String(data.get("headerName") ?? "");
    const headerValue = String(data.get("headerValue") ?? "");
    event.currentTarget.reset();
    void run(row.id, () =>
      commands
        .mcpConnectionSetLocalAuth(...owner(row), headerName, headerValue)
        .then(unwrapResult),
    );
  };

  const testConnection = (row: Connection) =>
    void run(row.id, async () => {
      try {
        const result: McpDiscoveryResult = unwrapResult(
          await commands.mcpConnectionTest(...owner(row)),
        );
        if (live.current) {
          setTestResults((prev) => ({
            ...prev,
            [row.id]: {
              ok: true,
              text:
                result.tool_count === 0
                  ? "Connected · 0 tools advertised"
                  : `Connected · ${result.tool_count} tool${result.tool_count === 1 ? "" : "s"} discovered`,
            },
          }));
        }
      } catch (reason) {
        if (live.current) {
          setTestResults((prev) => ({
            ...prev,
            [row.id]: { ok: false, text: String(reason) },
          }));
        }
        throw reason;
      }
    });

  const loadLogs = (row: Connection) =>
    void run(row.id, async () => {
      const logs = unwrapResult(
        await commands.mcpConnectionLogs(...owner(row)),
      );
      if (live.current) {
        setLogsById((prev) => ({ ...prev, [row.id]: logs }));
      }
    });

  const toggleEnabled = (row: Row, trigger: HTMLButtonElement) => {
    const nextEnabled = !row.account?.enabled;
    if (nextEnabled && row.transport === "stdio") {
      returnFocus.current = trigger;
      setConfirmingEnable(row.id);
      return;
    }
    void run(row.id, () =>
      commands
        .mcpConnectionSetEnabled(...owner(row), nextEnabled)
        .then(unwrapResult),
    );
  };

  const installBrowse = () =>
    void run("browse", async () => {
      const next = unwrapResult(await commands.mcpBrowseInstall());
      if (live.current) {
        setBrowse(next);
        setBrowseNote(
          next.version
            ? `Installed OpenCode Browse v${next.version}. Next, pair the browser extension.`
            : "Added OpenCode Browse to your local MCP servers. Pair the browser extension if needed.",
        );
      }
    });

  const pairBrowse = () =>
    void run("browse", async () => {
      const res = unwrapResult(await commands.mcpBrowsePair());
      if (live.current) {
        setBrowseCode(res.code ?? null);
        setCopiedCode(false);
        setBrowseNote(res.message);
        if (res.paired) {
          const checked = unwrapResult(await commands.mcpBrowseStatus(true));
          if (live.current) setBrowse(checked);
        }
      }
    });

  const openBrowseStore = () =>
    void run("browse", async () => {
      unwrapResult(await commands.mcpBrowseOpenStore());
    });

  const copyBrowseCode = async () => {
    if (!browseCode) return;
    try {
      await navigator.clipboard.writeText(browseCode);
      if (live.current) {
        setCopiedCode(true);
        setTimeout(() => {
          if (live.current) setCopiedCode(false);
        }, 2000);
      }
    } catch {
      /* ignore clipboard error */
    }
  };

  const checkBrowseExtension = () =>
    void run("browse", async () => {
      const checked = unwrapResult(await commands.mcpBrowseStatus(true));
      if (live.current) {
        setBrowse(checked);
        if (checked.extensionConnected || checked.extensionPaired) {
          setBrowseCode(null);
        }
        setBrowseNote(
          checked.extensionConnected
            ? "Browser extension paired & connected. Enable Browser Control below to use it."
            : checked.extensionPaired
              ? "Browser extension paired (waiting for Chrome/Edge/Brave to connect)."
              : (checked.note ??
                "Extension not paired yet. Click Get pairing code."),
        );
      }
    });

  const uninstallBrowse = () =>
    void run("browse", async () => {
      unwrapResult(await commands.mcpBrowseUninstall());
      if (live.current) {
        setUninstallingBrowse(false);
        setBrowseCode(null);
        setBrowseNote(null);
      }
    });

  const matching = rows.filter((row) =>
    `${row.name} ${row.url} ${row.command ?? ""} ${row.args.join(" ")} ${row.transport}`
      .toLowerCase()
      .includes(query.toLowerCase().trim()),
  );

  const browseMatchesQuery =
    !query.trim() ||
    "browser control opencode browse chrome edge brave local mcp".includes(
      query.toLowerCase().trim(),
    );

  const transportBadge = (kind: string) => {
    if (kind === "stdio") return "Local command";
    if (kind === "local-http") return "Local HTTP";
    return "Remote";
  };

  const transportIcon = (kind: string) => {
    if (kind === "stdio") return <Terminal size={17} />;
    if (kind === "local-http") return <Server size={17} />;
    if (kind === "remote") return <Globe size={17} />;
    return <Box size={17} />;
  };

  return (
    <>
      <div className="extensions-head">
        <div className="ext-page-head">
          {header}
          <div className="ext-head-actions">
            {actions}
            <button
              ref={addButton}
              type="button"
              className="button"
              disabled={busy !== null || adding}
              onClick={(event) => openForm(null, event.currentTarget)}
            >
              <Plus size={15} aria-hidden="true" />
              Add custom MCP
            </button>
          </div>
        </div>
        {toolbar}
      </div>
      {error && (
        <p className="extension-inline-error mcp-custom-error" role="alert">
          {error}
        </p>
      )}
      {adding && (
        <form
          className="mcp-connection-form mcp-inline-form"
          onSubmit={save}
          aria-label={editing ? "Edit MCP connection" : "Add an MCP connection"}
        >
          <h3>{editing ? `Edit ${editing.name}` : "Add an MCP connection"}</h3>
          <label className="mcp-json-choice">
            <input
              type="checkbox"
              checked={jsonMode}
              disabled={busy !== null}
              onChange={(event) => {
                if (event.target.checked) {
                  setJson(editing ? buildFormPayload() : EXAMPLE_JSON);
                }
                setError(null);
                setJsonMode(event.target.checked);
              }}
            />{" "}
            Paste JSON (supports standard <code>mcpServers</code> config)
          </label>
          {jsonMode ? (
            <label>
              MCP JSON
              <textarea
                required
                value={json}
                maxLength={65536}
                spellCheck={false}
                disabled={busy !== null}
                onChange={(event) => setJson(event.target.value)}
                rows={9}
              />
              <small>
                Paste a Claude Desktop / Cursor <code>mcpServers</code> block or
                a single server definition. Environment values and local auth
                headers are saved directly to the OS credential vault.
              </small>
            </label>
          ) : (
            <>
              <div
                className="mcp-transport-tabs"
                role="radiogroup"
                aria-label="MCP transport"
              >
                {(
                  [
                    ["stdio", "Local command (Stdio)"],
                    ["local-http", "Local server (HTTP)"],
                    ["remote", "Remote (HTTPS)"],
                  ] as const
                ).map(([kind, label]) => (
                  <button
                    key={kind}
                    type="button"
                    role="radio"
                    aria-checked={transport === kind}
                    className={`button mcp-transport-tab ${transport === kind ? "primary" : "ghost"}`}
                    disabled={busy !== null}
                    onClick={() => setTransport(kind)}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <label>
                Name
                <input
                  required
                  value={name}
                  placeholder={
                    transport === "stdio"
                      ? "e.g. Browser (Browse)"
                      : transport === "local-http"
                        ? "e.g. Local Playwright"
                        : "e.g. Team tools"
                  }
                  maxLength={120}
                  autoFocus
                  disabled={busy !== null}
                  onChange={(event) => setName(event.target.value)}
                />
              </label>
              {transport === "stdio" && (
                <>
                  <label>
                    Command
                    <input
                      required
                      value={command}
                      placeholder="e.g. browse, npx, uvx, or C:\path\to\server.exe"
                      maxLength={512}
                      spellCheck={false}
                      disabled={busy !== null}
                      onChange={(event) => setCommand(event.target.value)}
                    />
                    <small>
                      Executed directly without a shell; starts on demand and
                      stops automatically after 3 minutes idle.
                    </small>
                  </label>
                  <label>
                    Arguments (optional)
                    <input
                      value={argsText}
                      placeholder="e.g. mcp  or  -y @playwright/mcp --extension"
                      spellCheck={false}
                      disabled={busy !== null}
                      onChange={(event) => setArgsText(event.target.value)}
                    />
                    <small>
                      Space-separated; wrap paths containing spaces in double
                      quotes.
                    </small>
                  </label>
                  <label>
                    Working directory (optional)
                    <input
                      value={cwd}
                      placeholder="Absolute folder path (optional)"
                      maxLength={1024}
                      spellCheck={false}
                      disabled={busy !== null}
                      onChange={(event) => setCwd(event.target.value)}
                    />
                  </label>
                  <div className="mcp-env-editor">
                    <div className="mcp-env-header">
                      <span>Environment variables (stored in OS vault)</span>
                      <button
                        type="button"
                        className="button ghost"
                        disabled={busy !== null || envRows.length >= 32}
                        onClick={() =>
                          setEnvRows((prev) => [
                            ...prev,
                            { key: "", value: "" },
                          ])
                        }
                      >
                        Add variable
                      </button>
                    </div>
                    {envRows.map((entry, index) => (
                      <div className="mcp-env-row" key={index}>
                        <input
                          aria-label="Variable name"
                          placeholder="API_KEY"
                          value={entry.key}
                          spellCheck={false}
                          disabled={busy !== null}
                          onChange={(event) =>
                            setEnvRows((prev) =>
                              prev.map((item, i) =>
                                i === index
                                  ? { ...item, key: event.target.value }
                                  : item,
                              ),
                            )
                          }
                        />
                        <input
                          aria-label="Variable value"
                          type="password"
                          autoComplete="new-password"
                          placeholder={
                            entry.stored
                              ? "Stored in vault (leave blank to keep)"
                              : "Value"
                          }
                          value={entry.value}
                          disabled={busy !== null}
                          onChange={(event) =>
                            setEnvRows((prev) =>
                              prev.map((item, i) =>
                                i === index
                                  ? { ...item, value: event.target.value }
                                  : item,
                              ),
                            )
                          }
                        />
                        <button
                          type="button"
                          className="button ghost"
                          disabled={busy !== null}
                          onClick={() =>
                            setEnvRows((prev) =>
                              prev.filter((_, i) => i !== index),
                            )
                          }
                        >
                          Remove
                        </button>
                      </div>
                    ))}
                  </div>
                </>
              )}
              {transport === "local-http" && (
                <>
                  <label>
                    Local MCP URL
                    <input
                      required
                      type="url"
                      pattern="https?://(localhost|127\.0\.0\.1|\[::1\])(:[0-9]+)?(/.*)?"
                      value={url}
                      placeholder="http://127.0.0.1:8931/mcp"
                      disabled={busy !== null}
                      onChange={(event) => setUrl(event.target.value)}
                    />
                    <small>
                      Loopback addresses only (<code>localhost</code>,{" "}
                      <code>127.0.0.1</code>, or <code>[::1]</code>).
                    </small>
                  </label>
                  <div className="mcp-env-row">
                    <label>
                      Auth header name (optional)
                      <input
                        value={authHeaderName}
                        placeholder="Authorization"
                        spellCheck={false}
                        disabled={busy !== null}
                        onChange={(event) =>
                          setAuthHeaderName(event.target.value)
                        }
                      />
                    </label>
                    <label>
                      Auth header value (vault)
                      <input
                        type="password"
                        autoComplete="new-password"
                        value={authHeaderValue}
                        placeholder={
                          editing?.hasAuthHeader
                            ? "Stored in vault (leave blank to keep)"
                            : "Bearer …"
                        }
                        disabled={busy !== null}
                        onChange={(event) =>
                          setAuthHeaderValue(event.target.value)
                        }
                      />
                    </label>
                  </div>
                </>
              )}
              {transport === "remote" && (
                <>
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
                      onChange={(event) => {
                        setAuthentication(event.target.value);
                        if (event.target.value === "none") setOAuthClient(null);
                      }}
                    >
                      <option value="oauth">Browser sign-in (OAuth)</option>
                      <option value="none">No authentication</option>
                    </select>
                  </label>
                </>
              )}
            </>
          )}
          {editing && (
            <p>
              Changing the server URL, command, arguments, or authentication
              resets the connection and disables it until re-enabled.
            </p>
          )}
          <div className="mcp-connection-actions">
            <button
              className="button primary"
              type="submit"
              disabled={busy !== null}
            >
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
      {children(
        (loading || error || matching.length > 0 || browseMatchesQuery) && (
          <section
            className="mcp-connections mcp-custom-connections"
            aria-labelledby={titleId}
          >
            <header className="mcp-connections-heading">
              <h2 id={titleId}>Custom MCP servers</h2>
              {error && !adding && (
                <button
                  className="button ghost"
                  type="button"
                  disabled={busy !== null || loading}
                  onClick={() => void run("refresh", async () => {})}
                >
                  Refresh
                </button>
              )}
            </header>
            {browse && browseMatchesQuery && (
              <article className="mcp-connection-row mcp-custom-row mcp-browse-card">
                <div className="mcp-connection-copy">
                  <div className="mcp-custom-heading">
                    <span className="mcp-preset-icon" aria-hidden="true">
                      <Globe size={17} />
                    </span>
                    <h3>Browser Control</h3>
                    <span className="mcp-transport-badge">
                      Recommended &middot; OpenCode Browse
                    </span>
                  </div>
                  <p>
                    {browse.installed && browse.connectionId
                      ? `Native Rust browser bridge${browse.version ? ` (v${browse.version})` : ""} — background tab groups, 2FA handoff, 0 MB idle.`
                      : "Let the Agent act in your signed-in Chrome, Edge, or Brave tabs — ~3 MB native Rust binary, 0 MB idle, swap or remove anytime."}
                  </p>
                  {browseCode && (
                    <div className="mcp-pair-code-box" role="status">
                      <span>Pairing code:</span>
                      <code className="mcp-pair-code">{browseCode}</code>
                      <button
                        className="button ghost"
                        type="button"
                        onClick={() => void copyBrowseCode()}
                      >
                        {copiedCode ? "Copied" : "Copy"}
                      </button>
                    </div>
                  )}
                  {browseNote && (
                    <p className="mcp-test-ok" role="status">
                      {browseNote}
                    </p>
                  )}
                </div>
                <div className="mcp-connection-actions">
                  {!browse.installed || !browse.connectionId ? (
                    <button
                      className="button primary"
                      type="button"
                      disabled={busy !== null}
                      onClick={installBrowse}
                    >
                      {busy === "browse"
                        ? "Installing..."
                        : browse.installed
                          ? "Add to Grain"
                          : "Install & Add"}
                    </button>
                  ) : (
                    <>
                      <button
                        className="button primary"
                        type="button"
                        disabled={busy !== null}
                        onClick={pairBrowse}
                      >
                        {busy === "browse"
                          ? "Generating..."
                          : "Get pairing code"}
                      </button>
                      <button
                        className="button ghost"
                        type="button"
                        disabled={busy !== null}
                        onClick={openBrowseStore}
                      >
                        Chrome extension
                      </button>
                      <button
                        className="button ghost"
                        type="button"
                        disabled={busy !== null}
                        onClick={checkBrowseExtension}
                      >
                        Check status
                      </button>
                      <button
                        className="button ghost"
                        type="button"
                        disabled={busy !== null}
                        onClick={() => setUninstallingBrowse(true)}
                      >
                        Uninstall
                      </button>
                    </>
                  )}
                </div>
                {uninstallingBrowse && (
                  <div className="mcp-connection-confirm">
                    <p>
                      Remove Browser Control from Grain
                      {browse.managedByGrain
                        ? " and delete Grain's managed browse binary"
                        : ""}
                      ? You can reinstall or add any other browser MCP anytime.
                    </p>
                    <div className="mcp-connection-actions">
                      <button
                        type="button"
                        className="button danger"
                        disabled={busy !== null}
                        onClick={uninstallBrowse}
                      >
                        Uninstall Browser Control
                      </button>
                      <button
                        type="button"
                        className="button ghost"
                        disabled={busy !== null}
                        onClick={() => setUninstallingBrowse(false)}
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                )}
              </article>
            )}
            {loading ? (
              <p role="status">Loading MCP connections…</p>
            ) : matching.length === 0 ? (
              <p>
                {rows.length
                  ? "No MCP connections match your search."
                  : "Add a local or remote MCP to make its tools available to the Agent."}
              </p>
            ) : (
              matching.map((row) => {
                const isStdio = row.transport === "stdio";
                const isLocalHttp = row.transport === "local-http";
                const isRemoteOAuth =
                  row.transport === "remote" && row.authentication === "oauth";
                const testInfo = testResults[row.id];
                const logs = logsById[row.id];
                return (
                  <article
                    className="mcp-connection-row mcp-custom-row"
                    key={row.id}
                  >
                    <div className="mcp-connection-copy">
                      <div className="mcp-custom-heading">
                        <span className="mcp-preset-icon" aria-hidden="true">
                          {transportIcon(row.transport)}
                        </span>
                        <h3>{row.name}</h3>
                        <span className="mcp-transport-badge">
                          {transportBadge(row.transport)}
                        </span>
                      </div>
                      <p className="mcp-connection-url">
                        {isStdio ? <code>{commandPreview(row)}</code> : row.url}
                      </p>
                      <p>
                        {row.statusError ? (
                          row.statusError
                        ) : isStdio ? (
                          <>
                            {row.runtime.running
                              ? `Running${row.runtime.pid ? ` (PID ${row.runtime.pid})` : ""}`
                              : row.runtime.lastError
                                ? "Last start failed"
                                : "Idle (0 MB)"}
                            {row.envKeys.length > 0 &&
                              ` · ${row.envKeys.length} env var${row.envKeys.length === 1 ? "" : "s"}`}
                          </>
                        ) : isLocalHttp ? (
                          <>
                            Loopback server
                            {row.hasAuthHeader ? " · Auth header saved" : ""}
                          </>
                        ) : row.authentication === "none" ? (
                          "No account needed"
                        ) : row.account?.connected ? (
                          "Connected"
                        ) : row.account?.state === "needs_client_credentials" ||
                          row.account?.state === "disconnected" ? (
                          "Sign in to connect your account"
                        ) : (
                          row.account?.state.replace(/_/g, " ")
                        )}{" "}
                        &middot;{" "}
                        {row.account?.enabled
                          ? "Enabled"
                          : row.account
                            ? "Disabled"
                            : "Status unavailable"}
                      </p>
                      {testInfo && (
                        <p
                          className={
                            testInfo.ok
                              ? "mcp-test-ok"
                              : "extension-inline-error"
                          }
                          role="status"
                        >
                          {testInfo.text}
                        </p>
                      )}
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
                          {isStdio && row.runtime.running && (
                            <button
                              className="button ghost"
                              type="button"
                              disabled={busy !== null}
                              onClick={() =>
                                void run(row.id, () =>
                                  commands
                                    .mcpConnectionStop(...owner(row))
                                    .then(unwrapResult),
                                )
                              }
                            >
                              <Square size={12} aria-hidden="true" />
                              Stop
                            </button>
                          )}
                          {isRemoteOAuth && !row.account?.connected && (
                            <button
                              className="button primary"
                              type="button"
                              disabled={busy !== null}
                              onClick={() => connect(row)}
                            >
                              Connect
                            </button>
                          )}
                          {row.account &&
                            (!isRemoteOAuth || row.account.connected) && (
                              <button
                                className="button"
                                type="button"
                                disabled={busy !== null}
                                onClick={(event) =>
                                  toggleEnabled(row, event.currentTarget)
                                }
                              >
                                {row.account.enabled ? "Disable" : "Enable"}
                              </button>
                            )}
                        </>
                      )}
                    </div>
                    {confirmingEnable === row.id && isStdio && (
                      <div className="mcp-connection-confirm">
                        <p>Allow Grain to run this local command on demand?</p>
                        <p className="mcp-connection-url">
                          <code>{commandPreview(row)}</code>
                          {row.cwd ? (
                            <>
                              {" "}
                              in <code>{row.cwd}</code>
                            </>
                          ) : null}
                        </p>
                        <p>
                          It will only start when the Agent calls its tools and
                          stops automatically after 3 minutes idle.
                        </p>
                        <div className="mcp-connection-actions">
                          <button
                            ref={confirmEnableButton}
                            type="button"
                            className="button primary"
                            disabled={busy !== null}
                            onClick={() =>
                              void run(row.id, async () => {
                                unwrapResult(
                                  await commands.mcpConnectionSetEnabled(
                                    ...owner(row),
                                    true,
                                  ),
                                );
                                if (live.current) setConfirmingEnable(null);
                              })
                            }
                          >
                            Allow &amp; Enable
                          </button>
                          <button
                            type="button"
                            className="button ghost"
                            disabled={busy !== null}
                            onClick={() => setConfirmingEnable(null)}
                          >
                            Cancel
                          </button>
                        </div>
                      </div>
                    )}
                    <details className="mcp-preset-settings mcp-custom-settings">
                      <summary>Manage connection</summary>
                      <div className="mcp-connection-actions">
                        {(!isRemoteOAuth || row.account?.connected) && (
                          <button
                            className="button ghost"
                            type="button"
                            disabled={busy !== null}
                            onClick={() => testConnection(row)}
                          >
                            Test
                          </button>
                        )}
                        {isStdio && (
                          <button
                            className="button ghost"
                            type="button"
                            disabled={busy !== null}
                            onClick={() => loadLogs(row)}
                          >
                            Logs
                          </button>
                        )}
                        {row.account?.connected && isRemoteOAuth && (
                          <button
                            className="button ghost"
                            type="button"
                            disabled={busy !== null}
                            onClick={() => connect(row)}
                          >
                            Sign in again
                          </button>
                        )}
                        {row.account?.connected && isRemoteOAuth && (
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
                        <button
                          className="button ghost"
                          type="button"
                          disabled={busy !== null}
                          onClick={(event) =>
                            openForm(row, event.currentTarget)
                          }
                        >
                          Edit
                        </button>
                        <button
                          className="button ghost"
                          type="button"
                          disabled={busy !== null}
                          onClick={(event) => {
                            returnFocus.current = event.currentTarget;
                            setRemoving(row.id);
                          }}
                        >
                          Remove
                        </button>
                      </div>
                      {logs && isStdio && (
                        <div className="mcp-logs-box">
                          {logs.lastError && (
                            <p className="extension-inline-error">
                              {logs.lastError}
                            </p>
                          )}
                          <pre>
                            {logs.stderr.length > 0
                              ? logs.stderr.join("\n")
                              : "No stderr output recorded yet."}
                          </pre>
                        </div>
                      )}
                      {removing === row.id && (
                        <div className="mcp-connection-confirm">
                          <p>
                            Remove {row.name}? Its tools will be disabled, any
                            running process stopped, and saved secrets cleared.
                          </p>
                          <div className="mcp-connection-actions">
                            <button
                              ref={confirmRemoval}
                              type="button"
                              className="button danger"
                              disabled={busy !== null}
                              onClick={() =>
                                void run(row.id, async () => {
                                  unwrapResult(
                                    await commands.mcpConnectionRemove(
                                      ...owner(row),
                                    ),
                                  );
                                  if (live.current) setRemoving(null);
                                })
                              }
                            >
                              Remove connection
                            </button>
                            <button
                              type="button"
                              className="button ghost"
                              disabled={busy !== null}
                              onClick={() => setRemoving(null)}
                            >
                              Keep connection
                            </button>
                          </div>
                        </div>
                      )}
                      {isLocalHttp && (
                        <details className="mcp-client-settings">
                          <summary>
                            Local auth header{" "}
                            {row.hasAuthHeader ? "(configured)" : "(optional)"}
                          </summary>
                          <p>
                            Optional header sent only to this loopback server.
                            Stored in the OS credential vault.
                          </p>
                          <form
                            className="mcp-connection-form"
                            onSubmit={(event) => saveLocalAuth(event, row)}
                          >
                            <label>
                              Header name
                              <input
                                name="headerName"
                                required
                                placeholder="Authorization"
                                autoComplete="off"
                                disabled={busy !== null}
                              />
                            </label>
                            <label>
                              Header value
                              <input
                                name="headerValue"
                                required
                                type="password"
                                placeholder="Bearer …"
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
                                Save auth header
                              </button>
                              {row.hasAuthHeader && (
                                <button
                                  className="button ghost"
                                  type="button"
                                  disabled={busy !== null}
                                  onClick={() =>
                                    void run(row.id, () =>
                                      commands
                                        .mcpConnectionSetLocalAuth(
                                          ...owner(row),
                                          "",
                                          "",
                                        )
                                        .then(unwrapResult),
                                    )
                                  }
                                >
                                  Clear auth header
                                </button>
                              )}
                            </div>
                          </form>
                        </details>
                      )}
                      {isRemoteOAuth && (
                        <details className="mcp-client-settings">
                          <summary>
                            OAuth app credentials{" "}
                            {row.account?.client_id_configured
                              ? "(configured)"
                              : "(optional)"}
                          </summary>
                          <p>
                            Only needed if this server requires your own OAuth
                            app. Secrets go to the system vault.
                          </p>
                          <p className="mcp-connection-url">
                            Register redirect URI:{" "}
                            <code>
                              http://127.0.0.1:31938/mcp/oauth/callback
                            </code>
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
                    </details>
                  </article>
                );
              })
            )}
          </section>
        ),
      )}
    </>
  );
}
