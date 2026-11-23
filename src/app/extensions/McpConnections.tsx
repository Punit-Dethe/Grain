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
import {
  Box,
  Check,
  ChevronDown,
  Copy,
  Eye,
  EyeOff,
  Globe,
  Play,
  Plus,
  Server,
  Settings2,
  Square,
  Terminal,
  Trash2,
  X,
} from "lucide-react";
import {
  commands,
  type ConnectionView,
  type LocalMcpLogs,
  type McpDiscoveryResult,
  type McpProviderStatus,
} from "@/bindings";
import { Switch } from "@/components/ui/Switch";
import { signInCancelled, unwrapResult } from "./extensionRuntime";
import "./extensions.css";
import "./mcp-connections.css";

type Connection = ConnectionView;
type TransportKind = "stdio" | "local-http" | "remote";
type ModalMode = "form" | "json";
type AccountStatus = Pick<
  McpProviderStatus,
  "connected" | "enabled" | "state" | "client_id_configured"
>;
type Row = Connection & { account?: AccountStatus; statusError?: string };
type EnvEntry = {
  key: string;
  value: string;
  stored?: boolean;
  show?: boolean;
};

const OAUTH_REDIRECT_URI = "http://127.0.0.1:31938/mcp/oauth/callback";
const DISARM_MS = 4000;

const EXAMPLE_JSON = JSON.stringify(
  {
    mcpServers: {
      filesystem: {
        command: "npx",
        args: ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/dir"],
      },
    },
  },
  null,
  2,
);

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

function rowToJson(row: Connection): string {
  if (row.transport === "stdio") {
    return JSON.stringify(
      {
        transport: "stdio",
        name: row.name,
        command: row.command ?? "",
        args: row.args,
        ...(row.cwd ? { cwd: row.cwd } : {}),
        ...(row.envKeys.length ? { envKeys: row.envKeys } : {}),
      },
      null,
      2,
    );
  }
  if (row.transport === "local-http") {
    return JSON.stringify(
      {
        transport: "local-http",
        name: row.name,
        url: row.url,
        ...(Object.keys(row.headers).length ? { headers: row.headers } : {}),
      },
      null,
      2,
    );
  }
  return JSON.stringify(
    {
      transport: "remote",
      name: row.name,
      url: row.url,
      authentication: { type: row.authentication },
      ...(Object.keys(row.headers).length ? { headers: row.headers } : {}),
      ...(row.oauthClient ? { oauthClient: row.oauthClient } : {}),
    },
    null,
    2,
  );
}

function transportIcon(kind: string) {
  if (kind === "stdio") return <Terminal size={16} />;
  if (kind === "local-http") return <Server size={16} />;
  if (kind === "remote") return <Globe size={16} />;
  return <Box size={16} />;
}

export function McpConnections({
  query,
  header,
  toolbar,
  actions,
  children,
}: {
  query: string;
  header?: ReactNode;
  actions?: ReactNode;
  toolbar?: ReactNode;
  children: (customConnections: ReactNode) => ReactNode;
}) {
  const titleId = useId();
  const modalTitleId = useId();
  const confirmTitleId = useId();

  const [rows, setRows] = useState<Row[]>([]);
  const [loading, setLoading] = useState(true);
  const [pageError, setPageError] = useState<string | null>(null);
  const [modalError, setModalError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);

  // Add / Edit modal state
  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<Row | null>(null);
  const [mode, setMode] = useState<ModalMode>("form");
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
  const [showAuthValue, setShowAuthValue] = useState(false);
  const [oauthClientId, setOAuthClientId] = useState("");
  const [oauthClientSecret, setOAuthClientSecret] = useState("");
  const [showClientSecret, setShowClientSecret] = useState(false);
  const [json, setJson] = useState("");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [armedRemove, setArmedRemove] = useState(false);
  const [copiedRedirect, setCopiedRedirect] = useState(false);

  // Per-row runtime feedback
  const [testResults, setTestResults] = useState<
    Record<string, { ok: boolean; text: string }>
  >({});
  const [logsOpen, setLogsOpen] = useState(false);
  const [logs, setLogs] = useState<LocalMcpLogs | null>(null);

  // First-time stdio trust dialog
  const [trustRow, setTrustRow] = useState<Row | null>(null);

  const live = useRef(false);
  const request = useRef(0);
  const active = useRef(false);
  const pendingSignIn = useRef<Connection | null>(null);
  const cancelActive = useRef(false);
  const addButton = useRef<HTMLButtonElement>(null);
  const returnFocus = useRef<HTMLButtonElement | null>(null);
  const confirmEnableButton = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!armedRemove) return;
    const timer = window.setTimeout(() => setArmedRemove(false), DISARM_MS);
    return () => window.clearTimeout(timer);
  }, [armedRemove]);

  useEffect(() => {
    if (trustRow) confirmEnableButton.current?.focus();
  }, [trustRow]);

  useEffect(() => {
    if (!modalOpen && !trustRow && !busy && returnFocus.current) {
      const target = returnFocus.current;
      returnFocus.current = null;
      (target.isConnected ? target : addButton.current)?.focus();
    }
  }, [modalOpen, trustRow, busy]);

  // Escape closes whichever dialog is open (when idle)
  useEffect(() => {
    if (!modalOpen && !trustRow) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || busy !== null) return;
      event.preventDefault();
      if (trustRow) setTrustRow(null);
      else setModalOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [modalOpen, trustRow, busy]);

  const refresh = useCallback(async () => {
    const serial = ++request.current;
    const records = unwrapResult(await commands.mcpConnectionsList());
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
      setEditing((current) =>
        current ? (next.find((item) => item.id === current.id) ?? null) : null,
      );
    }
  }, []);

  useEffect(() => {
    live.current = true;
    void refresh()
      .catch((reason) => live.current && setPageError(String(reason)))
      .finally(() => live.current && setLoading(false));
    return () => {
      live.current = false;
      request.current++;
      const pending = pendingSignIn.current;
      if (pending) void cancelSignIn(pending).catch(() => {});
    };
  }, [refresh]);

  const run = async (
    id: string,
    action: () => Promise<unknown>,
    onErr?: (message: string) => void,
  ) => {
    if (active.current) return;
    active.current = true;
    setBusy(id);
    if (onErr) onErr("");
    else setPageError(null);
    try {
      await action();
    } catch (reason) {
      if (live.current) {
        const message = String(reason);
        if (onErr) onErr(message);
        else setPageError(message);
      }
    } finally {
      if (live.current) {
        await refresh().catch((reason) => {
          if (!live.current) return;
          const message = String(reason);
          if (onErr) onErr(message);
          else setPageError((current) => current ?? message);
        });
        if (live.current) setBusy(null);
      }
      active.current = false;
    }
  };

  const connect = (row: Row) =>
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

  const cancel = async (row: Row) => {
    if (cancelActive.current) return;
    cancelActive.current = true;
    setCancelling(true);
    try {
      await cancelSignIn(row);
    } catch (reason) {
      if (live.current) setPageError(String(reason));
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
      return JSON.stringify(
        {
          transport: "stdio",
          name: name.trim(),
          command: command.trim(),
          args: splitArgs(argsText),
          ...(cwd.trim() ? { cwd: cwd.trim() } : {}),
          ...(envKeys.length ? { envKeys } : {}),
          ...(Object.keys(env).length ? { env } : {}),
        },
        null,
        2,
      );
    }
    if (transport === "local-http") {
      return JSON.stringify(
        {
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
        },
        null,
        2,
      );
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

  const openModal = (row: Row | null, trigger: HTMLButtonElement | null) => {
    returnFocus.current = trigger;
    setEditing(row);
    setMode("form");
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
      (row?.envKeys ?? []).map((key) => ({
        key,
        value: "",
        stored: true,
        show: false,
      })),
    );
    setAuthHeaderName("");
    setAuthHeaderValue("");
    setShowAuthValue(false);
    setOAuthClientId("");
    setOAuthClientSecret("");
    setShowClientSecret(false);
    setJson(row ? rowToJson(row) : EXAMPLE_JSON);
    setAdvancedOpen(false);
    setArmedRemove(false);
    setLogsOpen(false);
    setLogs(null);
    setModalError(null);
    setModalOpen(true);
  };

  const switchMode = (nextMode: ModalMode) => {
    if (nextMode === mode) return;
    if (nextMode === "json") {
      if (editing || name.trim() || command.trim() || url.trim()) {
        setJson(buildFormPayload());
      } else {
        setJson(EXAMPLE_JSON);
      }
    }
    setModalError(null);
    setMode(nextMode);
  };

  const saveModal = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const definitionJson = mode === "json" ? json : buildFormPayload();
    void run(
      "modal",
      async () => {
        if (editing) {
          unwrapResult(
            await commands.mcpConnectionReplace(
              ...owner(editing),
              definitionJson,
            ),
          );
        } else if (mode === "json") {
          unwrapResult(
            await commands.mcpConnectionImportBundle(definitionJson),
          );
        } else {
          unwrapResult(await commands.mcpConnectionImport(definitionJson));
        }
        if (live.current) setModalOpen(false);
      },
      (msg) => setModalError(msg || null),
    );
  };

  const testConnection = (row: Row, inModal = false) =>
    void run(
      row.id,
      async () => {
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
                    ? "Connected · 0 tools"
                    : `Connected · ${result.tool_count} tool${result.tool_count === 1 ? "" : "s"}`,
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
        }
      },
      inModal ? (msg) => setModalError(msg || null) : undefined,
    );

  const toggleLogs = (row: Row) => {
    if (logsOpen) {
      setLogsOpen(false);
      return;
    }
    void run(
      row.id,
      async () => {
        const fetched = unwrapResult(
          await commands.mcpConnectionLogs(...owner(row)),
        );
        if (live.current) {
          setLogs(fetched);
          setLogsOpen(true);
        }
      },
      (msg) => setModalError(msg || null),
    );
  };

  const handleToggle = (row: Row, nextEnabled: boolean) => {
    if (busy !== null) return;
    if (row.transport === "stdio" && nextEnabled) {
      setTrustRow(row);
      return;
    }
    void run(row.id, () =>
      commands
        .mcpConnectionSetEnabled(...owner(row), nextEnabled)
        .then(unwrapResult),
    );
  };

  const handleRemove = (row: Row) => {
    if (!armedRemove) {
      setArmedRemove(true);
      return;
    }
    void run(
      row.id,
      async () => {
        unwrapResult(await commands.mcpConnectionRemove(...owner(row)));
        if (live.current) {
          setArmedRemove(false);
          setModalOpen(false);
        }
      },
      (msg) => setModalError(msg || null),
    );
  };

  const saveOAuthClient = () => {
    if (!editing || !oauthClientId.trim()) return;
    void run(
      editing.id,
      async () => {
        unwrapResult(
          await commands.mcpConnectionSetClientCredentials(
            ...owner(editing),
            oauthClientId.trim(),
            oauthClientSecret,
          ),
        );
        if (live.current) {
          setOAuthClientId("");
          setOAuthClientSecret("");
        }
      },
      (msg) => setModalError(msg || null),
    );
  };

  const clearLocalAuth = () => {
    if (!editing) return;
    void run(
      editing.id,
      () =>
        commands
          .mcpConnectionSetLocalAuth(...owner(editing), "", "")
          .then(unwrapResult),
      (msg) => setModalError(msg || null),
    );
  };

  const copyRedirectUri = async () => {
    try {
      await navigator.clipboard.writeText(OAUTH_REDIRECT_URI);
      if (live.current) {
        setCopiedRedirect(true);
        window.setTimeout(() => live.current && setCopiedRedirect(false), 1800);
      }
    } catch {
      // Clipboard may be unavailable; URI remains selectable.
    }
  };

  const matching = rows.filter((row) =>
    `${row.name} ${row.url} ${row.command ?? ""} ${row.args.join(" ")} ${row.transport}`
      .toLowerCase()
      .includes(query.toLowerCase().trim()),
  );

  const advancedCount =
    transport === "stdio"
      ? (cwd.trim() ? 1 : 0) + envRows.filter((e) => e.key.trim()).length
      : transport === "local-http"
        ? editing?.hasAuthHeader ||
          (authHeaderName.trim() && authHeaderValue.trim())
          ? 1
          : 0
        : editing?.account?.client_id_configured
          ? 1
          : 0;

  const editingTest = editing ? testResults[editing.id] : undefined;

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
              disabled={busy !== null}
              onClick={(event) => openModal(null, event.currentTarget)}
            >
              <Plus size={14} aria-hidden="true" />
              Add MCP
            </button>
          </div>
        </div>
        {toolbar}
      </div>

      {pageError && (
        <p className="mcp-custom-error" role="alert">
          {pageError}
        </p>
      )}

      {children(
        !loading && matching.length > 0 ? (
          <section className="ext-section" aria-labelledby={titleId}>
            <h2 className="ext-label" id={titleId}>
              Custom MCP
            </h2>
            <div className="ext-list">
              {matching.map((row) => {
                const isStdio = row.transport === "stdio";
                const isRemoteOAuth =
                  row.transport === "remote" && row.authentication === "oauth";
                const needsSignIn = isRemoteOAuth && !row.account?.connected;
                const testInfo = testResults[row.id];
                const isLive =
                  (isStdio && row.runtime.running) ||
                  (isRemoteOAuth && Boolean(row.account?.connected));
                const hasError = Boolean(
                  row.statusError ||
                  (isStdio && row.runtime.lastError && !row.runtime.running) ||
                  (testInfo && !testInfo.ok),
                );
                const targetText = isStdio ? commandPreview(row) : row.url;

                return (
                  <div className="ext-row" key={row.id}>
                    <span className="ext-logo" aria-hidden="true">
                      {transportIcon(row.transport)}
                    </span>

                    <div className="ext-row-copy">
                      <div className="mcp-row-title">
                        <h3>{row.name}</h3>
                        {hasError ? (
                          <span
                            className="mcp-dot is-error"
                            title="Needs attention"
                          />
                        ) : isLive ? (
                          <span className="mcp-dot is-live" title="Active" />
                        ) : null}
                      </div>
                      <p className="mcp-row-meta">
                        {testInfo ? (
                          <span
                            className={
                              testInfo.ok
                                ? "mcp-row-meta-ok"
                                : "mcp-row-meta-err"
                            }
                          >
                            {testInfo.text}
                          </span>
                        ) : row.statusError ? (
                          <span className="mcp-row-meta-err">
                            {row.statusError}
                          </span>
                        ) : (
                          <>
                            <code>{targetText}</code>
                            {isStdio && row.runtime.running && (
                              <span>· Running</span>
                            )}
                            {needsSignIn && <span>· Sign in required</span>}
                          </>
                        )}
                      </p>
                    </div>

                    <div className="mcp-row-controls">
                      {isStdio && row.runtime.running && (
                        <button
                          type="button"
                          className="mcp-icon-btn ext-quiet-action"
                          title="Stop process"
                          aria-label={`Stop ${row.name}`}
                          disabled={busy !== null}
                          onClick={() =>
                            void run(row.id, () =>
                              commands
                                .mcpConnectionStop(...owner(row))
                                .then(unwrapResult),
                            )
                          }
                        >
                          <Square size={13} aria-hidden="true" />
                        </button>
                      )}

                      {!needsSignIn && (
                        <button
                          type="button"
                          className="mcp-icon-btn ext-quiet-action"
                          title="Test connection"
                          aria-label={`Test ${row.name}`}
                          disabled={busy !== null}
                          onClick={() => testConnection(row)}
                        >
                          <Play size={13} aria-hidden="true" />
                        </button>
                      )}

                      <button
                        type="button"
                        className="mcp-icon-btn ext-quiet-action"
                        title="Configure"
                        aria-label={`Configure ${row.name}`}
                        disabled={busy !== null}
                        onClick={(event) => openModal(row, event.currentTarget)}
                      >
                        <Settings2 size={14} aria-hidden="true" />
                      </button>

                      {connecting === row.id ? (
                        <button
                          type="button"
                          className="button ghost ext-tile-action"
                          disabled={cancelling}
                          onClick={() => void cancel(row)}
                        >
                          {cancelling ? "Cancelling..." : "Cancel"}
                        </button>
                      ) : needsSignIn ? (
                        <button
                          type="button"
                          className="button primary ext-tile-action"
                          disabled={busy !== null}
                          onClick={() => connect(row)}
                        >
                          Connect
                        </button>
                      ) : row.account ? (
                        <Switch
                          checked={row.account.enabled}
                          disabled={busy !== null}
                          ariaLabel={`Use ${row.name}`}
                          onChange={(on) => handleToggle(row, on)}
                        />
                      ) : null}
                    </div>
                  </div>
                );
              })}
            </div>
          </section>
        ) : null,
      )}

      {/* Add / Configure MCP modal */}
      {modalOpen && (
        <div
          className="mcp-modal-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && busy === null) {
              setModalOpen(false);
            }
          }}
        >
          <form
            className="mcp-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby={modalTitleId}
            onSubmit={saveModal}
          >
            <header className="mcp-modal-head">
              <div className="mcp-modal-title-row">
                <h2 id={modalTitleId}>
                  {editing ? editing.name : "Add MCP server"}
                </h2>
                <button
                  type="button"
                  className="mcp-modal-close"
                  aria-label="Close"
                  disabled={busy !== null}
                  onClick={() => setModalOpen(false)}
                >
                  <X size={16} aria-hidden="true" />
                </button>
              </div>

              <div
                className="mcp-segmented"
                role="tablist"
                aria-label="Editor mode"
              >
                <button
                  type="button"
                  role="tab"
                  aria-selected={mode === "form"}
                  disabled={busy !== null}
                  onClick={() => switchMode("form")}
                >
                  Form
                </button>
                <button
                  type="button"
                  role="tab"
                  aria-selected={mode === "json"}
                  disabled={busy !== null}
                  onClick={() => switchMode("json")}
                >
                  JSON
                </button>
              </div>
            </header>

            <div className="mcp-modal-body">
              {modalError && (
                <p className="mcp-alert" role="alert">
                  {modalError}
                </p>
              )}

              {editingTest && (
                <p
                  className={editingTest.ok ? "mcp-status-banner" : "mcp-alert"}
                  role="status"
                >
                  {editingTest.text}
                </p>
              )}

              {mode === "json" ? (
                <div className="mcp-field">
                  <textarea
                    className="mcp-textarea mcp-mono"
                    required
                    aria-label="MCP server JSON"
                    value={json}
                    maxLength={65536}
                    spellCheck={false}
                    autoFocus
                    disabled={busy !== null}
                    onChange={(event) => setJson(event.target.value)}
                  />
                  <p className="mcp-hint">
                    Paste a standard <code>mcpServers</code> JSON block or a
                    single server object.
                  </p>
                </div>
              ) : (
                <>
                  <div className="mcp-field">
                    <span className="mcp-field-label">Type</span>
                    <div
                      className="mcp-segmented"
                      role="radiogroup"
                      aria-label="Server type"
                    >
                      {(
                        [
                          ["stdio", "Command", <Terminal key="s" size={13} />],
                          [
                            "local-http",
                            "Local URL",
                            <Server key="l" size={13} />,
                          ],
                          ["remote", "Remote", <Globe key="r" size={13} />],
                        ] as const
                      ).map(([kind, label, icon]) => (
                        <button
                          key={kind}
                          type="button"
                          role="radio"
                          aria-checked={transport === kind}
                          disabled={busy !== null}
                          onClick={() => {
                            setTransport(kind);
                            setAdvancedOpen(false);
                          }}
                        >
                          {icon}
                          {label}
                        </button>
                      ))}
                    </div>
                  </div>

                  <label className="mcp-field">
                    <span className="mcp-field-label">Name</span>
                    <input
                      className="mcp-input"
                      required
                      value={name}
                      placeholder={
                        transport === "stdio"
                          ? "Filesystem"
                          : transport === "local-http"
                            ? "Local server"
                            : "Team tools"
                      }
                      maxLength={120}
                      autoFocus
                      disabled={busy !== null}
                      onChange={(event) => setName(event.target.value)}
                    />
                  </label>

                  {transport === "stdio" && (
                    <>
                      <label className="mcp-field">
                        <span className="mcp-field-label">Command</span>
                        <input
                          className="mcp-input mcp-mono"
                          required
                          value={command}
                          placeholder="npx"
                          maxLength={512}
                          spellCheck={false}
                          disabled={busy !== null}
                          onChange={(event) => setCommand(event.target.value)}
                        />
                      </label>

                      <label className="mcp-field">
                        <span className="mcp-field-label">Arguments</span>
                        <input
                          className="mcp-input mcp-mono"
                          value={argsText}
                          placeholder="-y @modelcontextprotocol/server-filesystem /path"
                          spellCheck={false}
                          disabled={busy !== null}
                          onChange={(event) => setArgsText(event.target.value)}
                        />
                      </label>

                      <div className="mcp-disclosure">
                        <button
                          type="button"
                          className="mcp-disclosure-toggle"
                          aria-expanded={advancedOpen}
                          onClick={() => setAdvancedOpen((open) => !open)}
                        >
                          <span>Advanced</span>
                          <span className="mcp-disclosure-meta">
                            {advancedCount > 0 && (
                              <span>{advancedCount} set</span>
                            )}
                            <ChevronDown size={14} aria-hidden="true" />
                          </span>
                        </button>

                        {advancedOpen && (
                          <div className="mcp-disclosure-body">
                            <label className="mcp-field">
                              <span className="mcp-field-label">
                                Working directory
                              </span>
                              <input
                                className="mcp-input mcp-mono"
                                value={cwd}
                                placeholder="Optional folder path"
                                maxLength={1024}
                                spellCheck={false}
                                disabled={busy !== null}
                                onChange={(event) => setCwd(event.target.value)}
                              />
                            </label>

                            <div className="mcp-field">
                              <div className="mcp-kv-head">
                                <span className="mcp-field-label">
                                  Environment variables
                                </span>
                                <button
                                  type="button"
                                  className="mcp-link-btn"
                                  disabled={
                                    busy !== null || envRows.length >= 32
                                  }
                                  onClick={() =>
                                    setEnvRows((curr) => [
                                      ...curr,
                                      { key: "", value: "", show: false },
                                    ])
                                  }
                                >
                                  <Plus size={12} aria-hidden="true" />
                                  Add variable
                                </button>
                              </div>

                              {envRows.length > 0 && (
                                <div className="mcp-kv-list">
                                  {envRows.map((entry, idx) => (
                                    <div className="mcp-kv-row" key={idx}>
                                      <input
                                        className="mcp-input mcp-mono"
                                        aria-label="Variable name"
                                        value={entry.key}
                                        placeholder="API_KEY"
                                        spellCheck={false}
                                        disabled={busy !== null}
                                        onChange={(event) =>
                                          setEnvRows((curr) =>
                                            curr.map((item, i) =>
                                              i === idx
                                                ? {
                                                    ...item,
                                                    key: event.target.value,
                                                    stored: false,
                                                  }
                                                : item,
                                            ),
                                          )
                                        }
                                      />
                                      <div className="mcp-secret-wrap">
                                        <input
                                          className="mcp-input mcp-mono"
                                          aria-label="Variable value"
                                          type={
                                            entry.show ? "text" : "password"
                                          }
                                          value={entry.value}
                                          placeholder={
                                            entry.stored
                                              ? "Saved in vault"
                                              : "Value"
                                          }
                                          autoComplete="new-password"
                                          disabled={busy !== null}
                                          onChange={(event) =>
                                            setEnvRows((curr) =>
                                              curr.map((item, i) =>
                                                i === idx
                                                  ? {
                                                      ...item,
                                                      value: event.target.value,
                                                    }
                                                  : item,
                                              ),
                                            )
                                          }
                                        />
                                        <button
                                          type="button"
                                          className="mcp-icon-btn mcp-secret-eye"
                                          aria-label={
                                            entry.show
                                              ? "Hide value"
                                              : "Show value"
                                          }
                                          onClick={() =>
                                            setEnvRows((curr) =>
                                              curr.map((item, i) =>
                                                i === idx
                                                  ? {
                                                      ...item,
                                                      show: !item.show,
                                                    }
                                                  : item,
                                              ),
                                            )
                                          }
                                        >
                                          {entry.show ? (
                                            <EyeOff size={13} />
                                          ) : (
                                            <Eye size={13} />
                                          )}
                                        </button>
                                      </div>
                                      <button
                                        type="button"
                                        className="mcp-icon-btn"
                                        aria-label="Remove variable"
                                        disabled={busy !== null}
                                        onClick={() =>
                                          setEnvRows((curr) =>
                                            curr.filter((_, i) => i !== idx),
                                          )
                                        }
                                      >
                                        <Trash2 size={13} />
                                      </button>
                                    </div>
                                  ))}
                                </div>
                              )}
                            </div>
                          </div>
                        )}
                      </div>
                    </>
                  )}

                  {transport === "local-http" && (
                    <>
                      <label className="mcp-field">
                        <span className="mcp-field-label">URL</span>
                        <input
                          className="mcp-input mcp-mono"
                          type="url"
                          required
                          value={url}
                          placeholder="http://127.0.0.1:3000/mcp"
                          maxLength={2048}
                          spellCheck={false}
                          disabled={busy !== null}
                          onChange={(event) => setUrl(event.target.value)}
                        />
                      </label>

                      <div className="mcp-disclosure">
                        <button
                          type="button"
                          className="mcp-disclosure-toggle"
                          aria-expanded={advancedOpen}
                          onClick={() => setAdvancedOpen((open) => !open)}
                        >
                          <span>Auth header</span>
                          <span className="mcp-disclosure-meta">
                            {editing?.hasAuthHeader && <span>Saved</span>}
                            <ChevronDown size={14} aria-hidden="true" />
                          </span>
                        </button>

                        {advancedOpen && (
                          <div className="mcp-disclosure-body">
                            <div className="mcp-kv-row">
                              <input
                                className="mcp-input mcp-mono"
                                aria-label="Header name"
                                value={authHeaderName}
                                placeholder="Authorization"
                                autoComplete="off"
                                disabled={busy !== null}
                                onChange={(event) =>
                                  setAuthHeaderName(event.target.value)
                                }
                              />
                              <div className="mcp-secret-wrap">
                                <input
                                  className="mcp-input mcp-mono"
                                  aria-label="Header value"
                                  type={showAuthValue ? "text" : "password"}
                                  value={authHeaderValue}
                                  placeholder={
                                    editing?.hasAuthHeader
                                      ? "Saved in vault"
                                      : "Bearer …"
                                  }
                                  autoComplete="new-password"
                                  disabled={busy !== null}
                                  onChange={(event) =>
                                    setAuthHeaderValue(event.target.value)
                                  }
                                />
                                <button
                                  type="button"
                                  className="mcp-icon-btn mcp-secret-eye"
                                  aria-label={
                                    showAuthValue ? "Hide value" : "Show value"
                                  }
                                  onClick={() =>
                                    setShowAuthValue((curr) => !curr)
                                  }
                                >
                                  {showAuthValue ? (
                                    <EyeOff size={13} />
                                  ) : (
                                    <Eye size={13} />
                                  )}
                                </button>
                              </div>
                              {editing?.hasAuthHeader && (
                                <button
                                  type="button"
                                  className="mcp-icon-btn"
                                  title="Clear saved header"
                                  aria-label="Clear saved auth header"
                                  disabled={busy !== null}
                                  onClick={clearLocalAuth}
                                >
                                  <Trash2 size={13} />
                                </button>
                              )}
                            </div>
                          </div>
                        )}
                      </div>
                    </>
                  )}

                  {transport === "remote" && (
                    <>
                      <label className="mcp-field">
                        <span className="mcp-field-label">URL</span>
                        <input
                          className="mcp-input mcp-mono"
                          type="url"
                          required
                          value={url}
                          placeholder="https://mcp.example.com/mcp"
                          maxLength={2048}
                          spellCheck={false}
                          disabled={busy !== null}
                          onChange={(event) => setUrl(event.target.value)}
                        />
                      </label>

                      <div className="mcp-field">
                        <span className="mcp-field-label">Authentication</span>
                        <div
                          className="mcp-segmented"
                          role="radiogroup"
                          aria-label="Authentication"
                        >
                          {(
                            [
                              ["oauth", "OAuth"],
                              ["none", "None"],
                            ] as const
                          ).map(([kind, label]) => (
                            <button
                              key={kind}
                              type="button"
                              role="radio"
                              aria-checked={authentication === kind}
                              disabled={busy !== null}
                              onClick={() => {
                                setAuthentication(kind);
                                if (kind === "none") setOAuthClient(null);
                              }}
                            >
                              {label}
                            </button>
                          ))}
                        </div>
                      </div>

                      {editing && authentication === "oauth" && (
                        <div className="mcp-disclosure">
                          <button
                            type="button"
                            className="mcp-disclosure-toggle"
                            aria-expanded={advancedOpen}
                            onClick={() => setAdvancedOpen((open) => !open)}
                          >
                            <span>OAuth settings</span>
                            <span className="mcp-disclosure-meta">
                              {editing.account?.client_id_configured && (
                                <span>Custom app</span>
                              )}
                              <ChevronDown size={14} aria-hidden="true" />
                            </span>
                          </button>

                          {advancedOpen && (
                            <div className="mcp-disclosure-body">
                              {editing.account?.connected && (
                                <div className="mcp-inline-actions">
                                  <button
                                    type="button"
                                    className="button ghost"
                                    disabled={busy !== null}
                                    onClick={() => connect(editing)}
                                  >
                                    Sign in again
                                  </button>
                                  <button
                                    type="button"
                                    className="button ghost"
                                    disabled={busy !== null}
                                    onClick={() =>
                                      void run(
                                        editing.id,
                                        () =>
                                          commands
                                            .mcpConnectionDisconnect(
                                              ...owner(editing),
                                            )
                                            .then(unwrapResult),
                                        (msg) => setModalError(msg || null),
                                      )
                                    }
                                  >
                                    Sign out
                                  </button>
                                </div>
                              )}

                              <div className="mcp-field">
                                <span className="mcp-field-label">
                                  Redirect URI
                                </span>
                                <div className="mcp-code-chip">
                                  <code>{OAUTH_REDIRECT_URI}</code>
                                  <button
                                    type="button"
                                    className="mcp-icon-btn"
                                    aria-label="Copy redirect URI"
                                    onClick={() => void copyRedirectUri()}
                                  >
                                    {copiedRedirect ? (
                                      <Check size={13} />
                                    ) : (
                                      <Copy size={13} />
                                    )}
                                  </button>
                                </div>
                              </div>

                              <div className="mcp-kv-row">
                                <input
                                  className="mcp-input mcp-mono"
                                  aria-label="Client ID"
                                  value={oauthClientId}
                                  placeholder="Client ID"
                                  autoComplete="off"
                                  disabled={busy !== null}
                                  onChange={(event) =>
                                    setOAuthClientId(event.target.value)
                                  }
                                />
                                <div className="mcp-secret-wrap">
                                  <input
                                    className="mcp-input mcp-mono"
                                    aria-label="Client secret"
                                    type={
                                      showClientSecret ? "text" : "password"
                                    }
                                    value={oauthClientSecret}
                                    placeholder="Client secret"
                                    autoComplete="new-password"
                                    disabled={busy !== null}
                                    onChange={(event) =>
                                      setOAuthClientSecret(event.target.value)
                                    }
                                  />
                                  <button
                                    type="button"
                                    className="mcp-icon-btn mcp-secret-eye"
                                    aria-label={
                                      showClientSecret
                                        ? "Hide secret"
                                        : "Show secret"
                                    }
                                    onClick={() =>
                                      setShowClientSecret((curr) => !curr)
                                    }
                                  >
                                    {showClientSecret ? (
                                      <EyeOff size={13} />
                                    ) : (
                                      <Eye size={13} />
                                    )}
                                  </button>
                                </div>
                                <button
                                  type="button"
                                  className="button ghost"
                                  disabled={
                                    busy !== null || !oauthClientId.trim()
                                  }
                                  onClick={saveOAuthClient}
                                >
                                  Save
                                </button>
                              </div>

                              {editing.account?.client_id_configured && (
                                <button
                                  type="button"
                                  className="mcp-link-btn"
                                  disabled={busy !== null}
                                  onClick={() =>
                                    void run(
                                      editing.id,
                                      () =>
                                        commands
                                          .mcpConnectionClearClientCredentials(
                                            ...owner(editing),
                                          )
                                          .then(unwrapResult),
                                      (msg) => setModalError(msg || null),
                                    )
                                  }
                                >
                                  Reset OAuth app credentials
                                </button>
                              )}
                            </div>
                          )}
                        </div>
                      )}
                    </>
                  )}
                </>
              )}

              {editing && logsOpen && logs && (
                <div className="mcp-logs-panel">
                  {logs.lastError && (
                    <p className="mcp-alert">{logs.lastError}</p>
                  )}
                  <pre>
                    {logs.stderr.length > 0
                      ? logs.stderr.join("\n")
                      : "No stderr output recorded."}
                  </pre>
                </div>
              )}
            </div>

            <footer className="mcp-modal-foot">
              <div className="mcp-modal-foot-left">
                {editing && (
                  <>
                    <button
                      type="button"
                      className="button ghost"
                      disabled={busy !== null}
                      onClick={() => testConnection(editing, true)}
                    >
                      Test
                    </button>
                    {editing.transport === "stdio" && (
                      <button
                        type="button"
                        className="button ghost"
                        disabled={busy !== null}
                        onClick={() => toggleLogs(editing)}
                      >
                        {logsOpen ? "Hide logs" : "Logs"}
                      </button>
                    )}
                    <button
                      type="button"
                      className="button ghost ext-remove-key"
                      data-armed={armedRemove || undefined}
                      disabled={busy !== null}
                      onClick={() => handleRemove(editing)}
                    >
                      {armedRemove ? "Confirm remove" : "Remove"}
                    </button>
                  </>
                )}
              </div>

              <div className="mcp-modal-foot-right">
                <button
                  type="button"
                  className="button ghost"
                  disabled={busy !== null}
                  onClick={() => setModalOpen(false)}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="button primary"
                  disabled={busy !== null}
                >
                  {busy === "modal"
                    ? "Saving..."
                    : editing
                      ? "Save"
                      : "Add server"}
                </button>
              </div>
            </footer>
          </form>
        </div>
      )}

      {/* Focused trust dialog when enabling a local command */}
      {trustRow && (
        <div
          className="mcp-modal-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && busy === null) {
              setTrustRow(null);
            }
          }}
        >
          <div
            className="mcp-modal mcp-modal-compact"
            role="dialog"
            aria-modal="true"
            aria-labelledby={confirmTitleId}
          >
            <header className="mcp-modal-head">
              <div className="mcp-modal-title-row">
                <h2 id={confirmTitleId}>Allow {trustRow.name}?</h2>
                <button
                  type="button"
                  className="mcp-modal-close"
                  aria-label="Close"
                  disabled={busy !== null}
                  onClick={() => setTrustRow(null)}
                >
                  <X size={16} aria-hidden="true" />
                </button>
              </div>
            </header>
            <div className="mcp-modal-body">
              <div className="mcp-code-chip">
                <code>{commandPreview(trustRow)}</code>
              </div>
              <p className="mcp-confirm-copy">
                Starts only when the Agent calls its tools and stops
                automatically when idle.
              </p>
            </div>
            <footer className="mcp-modal-foot">
              <div className="mcp-modal-foot-left" />
              <div className="mcp-modal-foot-right">
                <button
                  type="button"
                  className="button ghost"
                  disabled={busy !== null}
                  onClick={() => setTrustRow(null)}
                >
                  Cancel
                </button>
                <button
                  ref={confirmEnableButton}
                  type="button"
                  className="button primary"
                  disabled={busy !== null}
                  onClick={() => {
                    const row = trustRow;
                    void run(row.id, async () => {
                      unwrapResult(
                        await commands.mcpConnectionSetEnabled(
                          ...owner(row),
                          true,
                        ),
                      );
                      if (live.current) setTrustRow(null);
                    });
                  }}
                >
                  Allow &amp; Enable
                </button>
              </div>
            </footer>
          </div>
        </div>
      )}
    </>
  );
}
