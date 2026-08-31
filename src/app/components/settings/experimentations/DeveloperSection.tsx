/* eslint-disable i18next/no-literal-string -- developer-only validation UI copy is intentionally not part of the shipped localization surface. */
import React, { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ExternalLink,
  FlaskConical,
  FolderOpen,
  Network,
  Trash2,
  X,
} from "lucide-react";
import { LiveLogViewer, type LiveLogFilterChip } from "../debug/LiveLogViewer";

interface DeveloperExtension {
  id: string;
  path: string;
}

interface ExtensionDeveloperStatus {
  enabled: boolean;
  loaded: DeveloperExtension[];
  lab_count: number;
}

interface McpProviderStatus {
  id: string;
  name: string;
  description: string;
  endpoint: string;
  setup_url: string;
  requires_client_credentials: boolean;
  client_id_configured: boolean;
  connected: boolean;
  enabled: boolean;
  state: string;
}

interface McpDiscoveryResult {
  provider_name: string;
  tool_count: number;
  tools: string[];
}

// User-facing developer tooling labels. Constants keep the established
// extension chrome independent from translation-key churn during Phase 3.5.
const EMPTY_MESSAGE =
  "Load an unpacked extension above to see its live diagnostics.";
const LIVE_EXTENSION_LABEL = "Live extension";
const EXTENSION_LABEL = "Extension";
const LOAD_UNPACKED_LABEL = "Load unpacked";
const LOAD_UNPACKED_DESCRIPTION =
  "Local code has the same permission checks as installed extensions.";
const CHOOSE_FOLDER_LABEL = "Choose folder\u2026";
const DEV_LABEL = "dev";
const LAB_LABEL = "Recommendation Lab";
const LAB_CORE_LABEL = "Core 6";
const LAB_STRESS_LABEL = "Stress 24";
const LAB_REMOVE_LABEL = "Remove lab";
const LAB_DESCRIPTION =
  "Installs local, zero-permission fixtures through Grain's real unpacked-extension runtime. Core covers six representative cases; Stress expands the same test to 24 extensions.";
const labLoadedLabel = (count: number) => `${count} loaded`;

const McpProviders: React.FC<{
  providers: McpProviderStatus[];
  refresh: () => Promise<void>;
}> = ({ providers, refresh }) => {
  const [busy, setBusy] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [credentials, setCredentials] = useState<
    Record<string, { clientId: string; clientSecret: string }>
  >({});

  const run = async (id: string, action: () => Promise<unknown>) => {
    setBusy(id);
    setError(null);
    setResult(null);
    try {
      await action();
      await refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy("");
    }
  };

  const saveCredentials = (provider: McpProviderStatus) => {
    const values = credentials[provider.id] ?? {
      clientId: "",
      clientSecret: "",
    };
    void run(provider.id, async () => {
      await invoke("mcp_set_client_credentials", {
        id: provider.id,
        clientId: values.clientId,
        clientSecret: values.clientSecret,
      });
      // Do not retain a secret in React state after the vault accepts it.
      setCredentials((current) => ({
        ...current,
        [provider.id]: { clientId: "", clientSecret: "" },
      }));
    });
  };

  const test = (provider: McpProviderStatus) => {
    void run(provider.id, async () => {
      const discovered = await invoke<McpDiscoveryResult>("mcp_test_provider", {
        id: provider.id,
      });
      setResult(
        `${discovered.provider_name}: ${discovered.tool_count} tools — ${discovered.tools.slice(0, 8).join(", ")}${discovered.tools.length > 8 ? "…" : ""}`,
      );
    });
  };

  return (
    <div className="rounded-xl border border-line bg-paper-raised">
      <div className="flex items-start gap-2.5 px-4 py-3">
        <Network
          className="mt-0.5 shrink-0 text-accent"
          width={15}
          height={15}
          aria-hidden="true"
        />
        <div>
          <div className="text-sm font-medium text-ink">
            Hosted MCP validation
          </div>
          <p className="mt-1 max-w-xl text-xs leading-relaxed text-ink-faint">
            Development-only, remote HTTPS providers for testing Grain&apos;s
            real Agent discovery and tool-calling stack. No local MCP processes
            or persistent sessions are started.
          </p>
        </div>
      </div>

      {(error || result) && (
        <div
          className={`border-t border-line px-4 py-2 text-xs ${error ? "text-red-600" : "text-ink-soft"}`}
        >
          {error ?? result}
        </div>
      )}

      <div className="divide-y divide-line border-t border-line">
        {providers.map((provider) => {
          const values = credentials[provider.id] ?? {
            clientId: "",
            clientSecret: "",
          };
          return (
            <div key={provider.id} className="px-4 py-3">
              <div className="flex flex-wrap items-center gap-2">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-ink">
                      {provider.name}
                    </span>
                    <span className="rounded-full border border-line px-1.5 py-0.5 text-[10px] text-ink-faint">
                      {provider.connected ? "connected" : provider.state}
                    </span>
                  </div>
                  <div className="mt-0.5 text-xs text-ink-faint">
                    {provider.description}
                  </div>
                </div>
                <a
                  href={provider.setup_url}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 text-xs text-ink-faint hover:text-ink"
                >
                  Setup <ExternalLink width={11} height={11} />
                </a>
                {provider.connected ? (
                  <>
                    <button
                      type="button"
                      className="button"
                      disabled={busy === provider.id}
                      onClick={() =>
                        void run(provider.id, () =>
                          invoke("mcp_set_provider_enabled", {
                            id: provider.id,
                            enabled: !provider.enabled,
                          }),
                        )
                      }
                    >
                      {provider.enabled ? "Disable" : "Enable"}
                    </button>
                    <button
                      type="button"
                      className="button"
                      disabled={busy === provider.id || !provider.enabled}
                      onClick={() => test(provider)}
                    >
                      Test
                    </button>
                    <button
                      type="button"
                      className="button danger"
                      disabled={busy === provider.id}
                      onClick={() =>
                        void run(provider.id, () =>
                          invoke("mcp_disconnect_provider", {
                            id: provider.id,
                          }),
                        )
                      }
                    >
                      Disconnect
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    className="button"
                    disabled={
                      busy === provider.id ||
                      (provider.requires_client_credentials &&
                        !provider.client_id_configured)
                    }
                    onClick={() =>
                      void run(provider.id, () =>
                        invoke("mcp_connect_provider", { id: provider.id }),
                      )
                    }
                  >
                    Connect
                  </button>
                )}
              </div>

              {provider.requires_client_credentials && !provider.connected && (
                <div className="mt-3">
                  <div className="mb-2 text-[10px] text-ink-faint">
                    Register redirect URI:{" "}
                    <code>http://127.0.0.1:31938/mcp/oauth/callback</code>
                  </div>
                  <div className="grid gap-2 sm:grid-cols-[1fr_1fr_auto]">
                    <input
                      value={values.clientId}
                      maxLength={512}
                      autoComplete="off"
                      placeholder="OAuth client ID"
                      aria-label={`${provider.name} OAuth client ID`}
                      onChange={(event) =>
                        setCredentials((current) => ({
                          ...current,
                          [provider.id]: {
                            ...values,
                            clientId: event.target.value,
                          },
                        }))
                      }
                      className="rounded-lg border border-line bg-paper px-2.5 py-1.5 text-xs text-ink outline-none focus:border-accent"
                    />
                    <input
                      type="password"
                      value={values.clientSecret}
                      maxLength={4096}
                      autoComplete="new-password"
                      placeholder="OAuth client secret"
                      aria-label={`${provider.name} OAuth client secret`}
                      onChange={(event) =>
                        setCredentials((current) => ({
                          ...current,
                          [provider.id]: {
                            ...values,
                            clientSecret: event.target.value,
                          },
                        }))
                      }
                      className="rounded-lg border border-line bg-paper px-2.5 py-1.5 text-xs text-ink outline-none focus:border-accent"
                    />
                    <button
                      type="button"
                      className="button"
                      disabled={
                        busy === provider.id ||
                        !values.clientId.trim() ||
                        !values.clientSecret
                      }
                      onClick={() => saveCredentials(provider)}
                    >
                      Save to vault
                    </button>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};

const FILTER_CHIPS: readonly LiveLogFilterChip[] = [
  { id: "all", label: "All" },
  { id: "calls", label: "Calls", substring: "] call " },
  { id: "denials", label: "Denials", substring: "] denied " },
  { id: "errors", label: "Errors", substring: "] error " },
];

/** [GRAIN] The Developer tab. Loading unpacked code lives HERE rather than in
 * Overview: this tab only exists once developer mode is on, so the tooling sits
 * behind the same switch that admits it instead of explaining itself to every
 * user above their installed list. */
export const DeveloperSection: React.FC<{
  onExtensionsChanged?: () => Promise<void>;
}> = ({ onExtensionsChanged }) => {
  const [loaded, setLoaded] = useState<DeveloperExtension[]>([]);
  const [mcpProviders, setMcpProviders] = useState<McpProviderStatus[]>([]);
  const [labCount, setLabCount] = useState(0);
  const [selectedId, setSelectedId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [status, providers] = await Promise.all([
      invoke<ExtensionDeveloperStatus>("extension_developer_status"),
      invoke<McpProviderStatus[]>("mcp_provider_status"),
    ]);
    setLoaded(status.loaded);
    setMcpProviders(providers);
    setLabCount(status.lab_count);
    setSelectedId((current) =>
      status.loaded.some((extension) => extension.id === current)
        ? current
        : (status.loaded[0]?.id ?? ""),
    );
  }, []);

  useEffect(() => {
    void refresh().catch(() => undefined);
  }, [refresh]);

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      await refresh();
      await onExtensionsChanged?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const selected = useMemo(
    () => loaded.find((extension) => extension.id === selectedId),
    [loaded, selectedId],
  );

  const installLab = (size: "core" | "stress") => {
    const count = size === "core" ? 6 : 24;
    if (
      !window.confirm(
        `Install and enable ${count} local Recommendation Lab extensions? Selecting one hands it the full captured request. The fixtures request no permissions and call no external services.`,
      )
    ) {
      return;
    }
    void run(() =>
      invoke<number>("extension_recommendation_lab_install", { size }),
    );
  };

  const removeLab = () => {
    if (
      !window.confirm(
        "Remove every Recommendation Lab extension and clear its synthetic ranking feedback?",
      )
    ) {
      return;
    }
    void run(() => invoke("extension_recommendation_lab_remove"));
  };

  const loader = (
    <div className="rounded-xl border border-line bg-paper-raised">
      <div className="flex items-center justify-between gap-3 px-4 py-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-ink">
            {LOAD_UNPACKED_LABEL}
          </div>
          <div className="text-xs text-ink-faint">
            {LOAD_UNPACKED_DESCRIPTION}
          </div>
        </div>
        <button
          type="button"
          disabled={busy}
          onClick={() =>
            void run(() => invoke<string | null>("extension_load_unpacked"))
          }
          className="inline-flex shrink-0 items-center gap-1.5 rounded-lg border border-line px-2.5 py-1.5 text-xs text-ink hover:border-ink-faint disabled:opacity-50 cursor-pointer"
        >
          <FolderOpen width={13} height={13} />
          {CHOOSE_FOLDER_LABEL}
        </button>
      </div>

      {import.meta.env.DEV && (
        <div className="border-t border-line px-4 py-3">
          <div className="flex items-start gap-2.5">
            <FlaskConical
              className="mt-0.5 shrink-0 text-accent"
              width={15}
              height={15}
              aria-hidden="true"
            />
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <div className="text-sm font-medium text-ink">{LAB_LABEL}</div>
                {labCount > 0 && (
                  <span className="rounded-full bg-accent/10 px-2 py-0.5 text-[10px] font-medium tabular-nums text-accent">
                    {labLoadedLabel(labCount)}
                  </span>
                )}
              </div>
              <p className="mt-1 max-w-xl text-xs leading-relaxed text-ink-faint">
                {LAB_DESCRIPTION}
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <button
                  type="button"
                  className="button"
                  disabled={busy}
                  aria-pressed={labCount === 6}
                  onClick={() => installLab("core")}
                >
                  {LAB_CORE_LABEL}
                </button>
                <button
                  type="button"
                  className="button"
                  disabled={busy}
                  aria-pressed={labCount === 24}
                  onClick={() => installLab("stress")}
                >
                  {LAB_STRESS_LABEL}
                </button>
                {labCount > 0 && (
                  <button
                    type="button"
                    className="button danger"
                    disabled={busy}
                    onClick={removeLab}
                  >
                    <Trash2 width={13} height={13} aria-hidden="true" />
                    {LAB_REMOVE_LABEL}
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {loaded.length > 0 && (
        <div className="border-t border-line px-4 py-3 space-y-1.5">
          {loaded.map((entry) => (
            <div
              key={entry.id}
              className="flex items-center gap-2 rounded-lg bg-paper-sunken px-2.5 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="text-xs font-medium text-ink truncate">
                  {entry.id}
                </div>
                <div
                  className="text-[10px] text-ink-faint truncate"
                  title={entry.path}
                >
                  {entry.path}
                </div>
              </div>
              <span className="rounded border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-amber-700 dark:text-amber-300">
                {DEV_LABEL}
              </span>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void run(() =>
                    invoke("extension_unload_dev", { id: entry.id }),
                  )
                }
                className="text-ink-faint hover:text-ink disabled:opacity-50 cursor-pointer"
                aria-label={`Unload ${entry.id}`}
                title="Unload"
              >
                <X width={13} height={13} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );

  if (!selected) {
    return (
      <div className="space-y-4">
        {error && (
          <div className="rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-600">
            {error}
          </div>
        )}
        {loader}
        <McpProviders providers={mcpProviders} refresh={refresh} />
        <div className="rounded-xl border border-line bg-paper-raised p-5 text-sm text-ink-soft">
          {EMPTY_MESSAGE}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {error && (
        <div className="rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-600">
          {error}
        </div>
      )}
      {loader}
      <McpProviders providers={mcpProviders} refresh={refresh} />
      <div className="flex flex-wrap items-end justify-between gap-3 rounded-xl border border-line bg-paper-raised p-4">
        <div className="min-w-0">
          <div className="text-sm font-medium text-ink">
            {LIVE_EXTENSION_LABEL}
          </div>
          <div className="mt-1 truncate font-mono text-xs text-ink-faint">
            {selected.path}
          </div>
        </div>
        {loaded.length > 1 ? (
          <label className="flex items-center gap-2 text-xs text-ink-soft">
            {EXTENSION_LABEL}
            <select
              value={selectedId}
              onChange={(event) => setSelectedId(event.target.value)}
              className="rounded-lg border border-line bg-paper px-2.5 py-1.5 text-sm text-ink outline-none focus:border-accent"
            >
              {loaded.map((extension) => (
                <option key={extension.id} value={extension.id}>
                  {extension.id}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <span className="rounded-full border border-accent/25 bg-accent/10 px-2.5 py-1 font-mono text-xs text-accent">
            {selected.id}
          </span>
        )}
      </div>

      <LiveLogViewer
        descriptionMode="inline"
        filter={{ prefix: `[ext:${selected.id}]` }}
        filterChips={FILTER_CHIPS}
      />
    </div>
  );
};
