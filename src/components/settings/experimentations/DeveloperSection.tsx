import React, { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, X } from "lucide-react";
import { LiveLogViewer, type LiveLogFilterChip } from "../debug/LiveLogViewer";

interface DeveloperExtension {
  id: string;
  path: string;
}

interface ExtensionDeveloperStatus {
  enabled: boolean;
  loaded: DeveloperExtension[];
}

// User-facing developer tooling labels. Constants keep the established
// extension chrome independent from translation-key churn during Phase 3.5.
const EMPTY_MESSAGE =
  "Load an unpacked extension above to see its live diagnostics.";
const LIVE_EXTENSION_LABEL = "Live extension";
const EXTENSION_LABEL = "Extension";

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
export const DeveloperSection: React.FC = () => {
  const [loaded, setLoaded] = useState<DeveloperExtension[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const status = await invoke<ExtensionDeveloperStatus>(
      "extension_developer_status",
    );
    setLoaded(status.loaded);
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

  const loader = (
    <div className="rounded-xl border border-line bg-paper-raised">
      <div className="flex items-center justify-between gap-3 px-4 py-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-ink">Load unpacked</div>
          <div className="text-xs text-ink-faint">
            Local code has the same permission checks as installed extensions.
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
          Choose folder…
        </button>
      </div>

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
                dev
              </span>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void run(() => invoke("extension_unload_dev", { id: entry.id }))
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
