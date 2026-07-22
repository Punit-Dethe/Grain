import React, { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
  "Load an unpacked extension from Overview to see its live diagnostics.";
const LIVE_EXTENSION_LABEL = "Live extension";
const EXTENSION_LABEL = "Extension";

const FILTER_CHIPS: readonly LiveLogFilterChip[] = [
  { id: "all", label: "All" },
  { id: "calls", label: "Calls", substring: "] call " },
  { id: "denials", label: "Denials", substring: "] denied " },
  { id: "errors", label: "Errors", substring: "] error " },
];

export const DeveloperSection: React.FC = () => {
  const [loaded, setLoaded] = useState<DeveloperExtension[]>([]);
  const [selectedId, setSelectedId] = useState("");

  useEffect(() => {
    let active = true;
    void invoke<ExtensionDeveloperStatus>("extension_developer_status")
      .then((status) => {
        if (!active) return;
        setLoaded(status.loaded);
        setSelectedId((current) =>
          status.loaded.some((extension) => extension.id === current)
            ? current
            : (status.loaded[0]?.id ?? ""),
        );
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  const selected = useMemo(
    () => loaded.find((extension) => extension.id === selectedId),
    [loaded, selectedId],
  );

  if (!selected) {
    return (
      <div className="rounded-xl border border-line bg-paper-raised p-5 text-sm text-ink-soft">
        {EMPTY_MESSAGE}
      </div>
    );
  }

  return (
    <div className="space-y-4">
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
