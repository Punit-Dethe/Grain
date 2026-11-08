/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type McpProviderStatus } from "@/bindings";
import { Switch } from "@/components/ui/Switch";
import {
  filterExtensions,
  signInCancelled,
  unwrapResult,
} from "./extensionRuntime";

const presetIds = new Set(["github", "notion"]);

export function McpPresets({
  browse,
  query,
}: {
  browse: boolean;
  query: string;
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
  if (!browse && !visible.length && !error && !message) return null;

  return (
    <section
      className="mcp-connections"
      aria-label={browse ? "Preconfigured connections" : "Connected apps"}
    >
      <header className="mcp-connections-heading">
        <h2>{browse ? "Preconfigured connections" : "Connected apps"}</h2>
      </header>
      {message && <p role="status">{message}</p>}
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
      {!rows && !error && <p role="status">Loading connections…</p>}
      {rows && !visible.length && browse && (
        <p>No connections match your search.</p>
      )}
      <div className="installed-extension-list">
        {visible.map((row) => (
          <article
            key={row.id}
            className="extension-card installed-extension-card"
          >
            <div className="installed-extension-copy">
              <strong>{row.name}</strong>
              <p>{row.description}</p>
              {row.state === "needs_client_credentials" && (
                <p>
                  This build is missing the {row.name} OAuth registration. Use a
                  build that includes it.
                </p>
              )}
              {row.state !== "stored" &&
                row.state !== "needs_client_credentials" &&
                row.connected && <p>{row.state.replace(/_/g, " ")}</p>}
            </div>
            <div className="installed-extension-actions">
              {signingIn === row.id ? (
                <button
                  className="button"
                  type="button"
                  disabled={cancelling}
                  onClick={() => void cancel(row.id)}
                >
                  Cancel sign-in
                </button>
              ) : row.connected ? (
                <>
                  {row.state === "reconnect_required" && (
                    <button
                      className="button primary"
                      type="button"
                      disabled={busy !== null}
                      onClick={() => void connect(row)}
                    >
                      Reconnect
                    </button>
                  )}
                  <Switch
                    checked={row.enabled}
                    disabled={
                      busy !== null || row.state === "needs_client_credentials"
                    }
                    ariaLabel={[
                      row.enabled ? "Disable" : "Enable",
                      row.name,
                    ].join(" ")}
                    onChange={() =>
                      void run(row.id, async () => {
                        unwrapResult(
                          await commands.mcpSetProviderEnabled(
                            row.id,
                            !row.enabled,
                          ),
                        );
                      })
                    }
                  />
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
                      })
                    }
                  >
                    Disconnect
                  </button>
                </>
              ) : (
                <button
                  className="button primary"
                  type="button"
                  disabled={
                    busy !== null || row.state === "needs_client_credentials"
                  }
                  onClick={() => void connect(row)}
                >
                  {busy === row.id ? "Connecting…" : "Connect"}
                </button>
              )}
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
