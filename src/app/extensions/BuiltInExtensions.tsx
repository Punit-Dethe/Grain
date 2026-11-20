/* eslint-disable i18next/no-literal-string -- copy follows the current untranslated Extensions surface. */
import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type McpProviderStatus } from "@/bindings";
import { Switch } from "@/components/ui/Switch";
import { unwrapResult } from "./extensionRuntime";
import exaIcon from "./icons/exa.png";
import pilotIcon from "./icons/pilot.svg";
import "./extensions.css";

const icons = new Map<string, string>([
  ["exa", exaIcon],
  ["pilot", pilotIcon],
]);

/** Extensions that ship with Grain. They need no account, so a switch is the whole control. */
export function BuiltInExtensions() {
  const [rows, setRows] = useState<McpProviderStatus[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(false);

  const refresh = useCallback(async () => {
    const status = unwrapResult(await commands.mcpProviderStatus(true));
    status.sort((a, b) =>
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
    );
    if (mounted.current) setRows(status);
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh().catch(
      (reason) => mounted.current && setError(String(reason)),
    );
    return () => {
      mounted.current = false;
    };
  }, [refresh]);

  const toggle = async (row: McpProviderStatus, on: boolean) => {
    if (busy) return;
    setBusy(row.id);
    setError(null);
    try {
      // First switch-on connects (and may prepare what it runs); after that
      // the switch only turns the Agent's use of it on and off.
      if (on && !row.connected)
        unwrapResult(await commands.mcpConnectProvider(row.id));
      else unwrapResult(await commands.mcpSetProviderEnabled(row.id, on));
      await refresh();
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      if (mounted.current) setBusy(null);
    }
  };

  const visible = rows ?? [];
  if (rows && visible.length === 0 && !error) return null;

  return (
    <section className="ext-section" aria-labelledby="ext-built-in">
      <h2 className="ext-label" id="ext-built-in">
        Built-in
      </h2>
      {error && (
        <p className="extension-inline-error" role="alert">
          {error}
        </p>
      )}
      <div className="ext-list">
        {visible.map((row) => (
          <div key={row.id} className="ext-row">
            <span className="ext-logo">
              {icons.has(row.id) && (
                <img src={icons.get(row.id)} alt="" width={22} height={22} />
              )}
            </span>
            <div className="ext-row-copy">
              <h3>{row.name}</h3>
              <p>{row.description}</p>
            </div>
            <Switch
              checked={row.enabled}
              disabled={busy !== null}
              ariaLabel={`Use ${row.name}`}
              onChange={(on) => void toggle(row, on)}
            />
          </div>
        ))}
      </div>
    </section>
  );
}
