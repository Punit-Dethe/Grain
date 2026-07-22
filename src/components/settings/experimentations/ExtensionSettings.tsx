import React, { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Mirror of the Rust `ExtensionSettingRow` (grain_commands.rs). Local type
 * until the next dev run regenerates bindings.ts — never hand-edit bindings. */
export interface SettingRow {
  key: string;
  label: string;
  description: string;
  kind:
    | "bool"
    | "string"
    | "secret"
    | "number"
    | "select"
    | "shortcut"
    | "color"
    | "slider"
    | "unsupported";
  anchor: string | null;
  order: number;
  value: unknown;
  notice: string | null;
  min: number | null;
  max: number | null;
  step: number | null;
  options: { value: string; label: string }[];
}

export interface SettingsSection {
  id: string;
  name: string;
  rows: SettingRow[];
}

/** Anchors this build renders (SPEC §4.3 v1, mirroring grain-sdk's `ANCHORS`).
 * A row whose anchor is absent from this list is NOT an error — it falls back
 * to the extension's own section, because settings are never lost. */
export const ANCHORS = [
  "snippets.after",
  "dictation.pipeline.after",
  "context.after",
  "agent.after",
  "models.after",
] as const;

export type Anchor = (typeof ANCHORS)[number];

/** One schema-declared control. The renderer knows `kind`, never the
 * extension — there is no per-extension code anywhere in this file. */
const Control: React.FC<{
  row: SettingRow;
  disabled: boolean;
  onCommit: (value: unknown) => void;
}> = ({ row, disabled, onCommit }) => {
  // Text-like controls edit locally and commit on blur, so the backend isn't
  // asked to validate every keystroke.
  const [draft, setDraft] = useState<string>(
    row.kind !== "secret" && typeof row.value === "string" ? row.value : "",
  );
  useEffect(() => {
    if (row.kind === "secret") setDraft("");
    else if (typeof row.value === "string") setDraft(row.value);
  }, [row.kind, row.value]);

  const inputClass =
    "px-2 py-1 rounded-lg bg-paper-sunken border border-line text-sm text-ink outline-none focus:border-accent/50 disabled:opacity-50";

  switch (row.kind) {
    case "bool":
      return (
        <button
          type="button"
          role="switch"
          aria-checked={row.value === true}
          aria-label={row.label}
          disabled={disabled}
          onClick={() => onCommit(row.value !== true)}
          className={`relative w-9 h-5 rounded-full transition-colors cursor-pointer shrink-0 ${
            row.value === true
              ? "bg-accent"
              : "bg-paper-sunken border border-line"
          } ${disabled ? "opacity-50" : ""}`}
        >
          <span
            className={`absolute top-0.5 w-4 h-4 rounded-full bg-paper-raised shadow transition-all ${
              row.value === true ? "left-[18px]" : "left-0.5"
            }`}
          />
        </button>
      );

    case "select":
      return (
        <select
          aria-label={row.label}
          disabled={disabled}
          value={typeof row.value === "string" ? row.value : ""}
          onChange={(e) => onCommit(e.target.value)}
          className={`${inputClass} cursor-pointer`}
        >
          {row.options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      );

    case "number":
      return (
        <input
          type="number"
          aria-label={row.label}
          disabled={disabled}
          min={row.min ?? undefined}
          max={row.max ?? undefined}
          defaultValue={typeof row.value === "number" ? row.value : 0}
          key={String(row.value)}
          onBlur={(e) => {
            const n = Number(e.target.value);
            if (!Number.isNaN(n) && n !== row.value) onCommit(n);
          }}
          className={`${inputClass} w-24 text-right`}
        />
      );

    case "slider":
      return (
        <div className="flex items-center gap-2 shrink-0">
          <input
            type="range"
            aria-label={row.label}
            disabled={disabled}
            min={row.min ?? 0}
            max={row.max ?? 100}
            step={row.step ?? 1}
            value={typeof row.value === "number" ? row.value : (row.min ?? 0)}
            onChange={(e) => onCommit(Number(e.target.value))}
            className="w-32 accent-accent cursor-pointer disabled:opacity-50"
          />
          <span className="text-xs text-ink-faint tabular-nums w-8 text-right">
            {typeof row.value === "number" ? row.value : ""}
          </span>
        </div>
      );

    case "color":
      return (
        <input
          type="color"
          aria-label={row.label}
          disabled={disabled}
          value={typeof row.value === "string" ? row.value : "#000000"}
          onChange={(e) => onCommit(e.target.value)}
          className="w-9 h-6 rounded border border-line bg-transparent cursor-pointer disabled:opacity-50"
        />
      );

    case "secret":
      return (
        <div className="flex items-center gap-2">
          <input
            type="password"
            autoComplete="new-password"
            aria-label={row.label}
            disabled={disabled}
            value={draft}
            placeholder={row.value === "[REDACTED]" ? "Saved" : "Not set"}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={() => {
              if (draft !== "") onCommit(draft);
            }}
            className={`${inputClass} w-48`}
          />
          {row.value === "[REDACTED]" && (
            <button
              type="button"
              disabled={disabled}
              onClick={() => onCommit("")}
              className="text-xs text-ink-faint hover:text-ink disabled:opacity-50 cursor-pointer"
            >
              Clear
            </button>
          )}
        </div>
      );

    // A shortcut is a string here: the binding registry owns chord capture and
    // conflict resolution, and it arrives with `contributes.shortcuts`.
    case "shortcut":
    case "string":
      return (
        <input
          type="text"
          aria-label={row.label}
          disabled={disabled}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => {
            if (draft !== row.value) onCommit(draft);
          }}
          className={`${inputClass} w-48`}
        />
      );

    // Declared by a manifest written against a newer contract. Dropped by the
    // backend before it gets here; handled anyway so a future row can never
    // render as a broken control.
    default:
      return null;
  }
};

/** [GRAIN] An extension's settings section (SPEC §4, levels 1–2): the host
 * renders the controls the manifest declares. Entirely schema-driven — adding a
 * setting to a pack requires no code here.
 *
 * `filter` selects which of the extension's rows belong to this mount point, so
 * the same component serves both an anchored group inside a core section and
 * the extension's own full section. */
export const ExtensionSettings: React.FC<{
  section: SettingsSection;
  rows?: SettingRow[];
  onChanged?: () => void;
}> = ({ section, rows: only, onChanged }) => {
  const [rows, setRows] = useState<SettingRow[]>(only ?? section.rows);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setRows(only ?? section.rows);
  }, [only, section.rows]);

  const commit = async (row: SettingRow, value: unknown) => {
    setBusy(row.key);
    setError(null);
    try {
      // The backend is the authority: it validates, may clamp, and returns the
      // row it actually stored — so the control shows the truth, not the input.
      const stored = await invoke<SettingRow>("extension_setting_set", {
        id: section.id,
        key: row.key,
        value,
      });
      setRows((prev) => prev.map((r) => (r.key === row.key ? stored : r)));
      onChanged?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  if (rows.length === 0) return null;

  return (
    <div className="space-y-2">
      {error && (
        <div className="px-3 py-2 rounded-lg bg-red-500/10 text-red-600 text-xs">
          {error}
        </div>
      )}
      <div className="rounded-xl border border-line bg-paper-raised divide-y divide-line">
        {rows.map((row) => (
          <div key={row.key} className="flex items-center gap-3 px-4 py-3">
            <div className="flex-1 min-w-0">
              <div className="text-sm text-ink">{row.label}</div>
              {row.description && (
                <div className="text-xs text-ink-faint">{row.description}</div>
              )}
              {/* A value the user did not change must say so (SPEC §6:
                  "invalid values → default + notice"). */}
              {row.notice && (
                <div className="text-xs text-amber-600 mt-0.5">
                  {row.notice}
                </div>
              )}
            </div>
            <Control
              row={row}
              disabled={busy === row.key}
              onCommit={(v) => void commit(row, v)}
            />
          </div>
        ))}
      </div>
    </div>
  );
};

/** Mirror of the Rust `ShortcutStatus` (extension_shortcuts.rs). */
export interface ShortcutStatus {
  id: string;
  label: string;
  binding: string;
  active: boolean;
  conflicts_with: string | null;
}

/** [GRAIN] An extension's contributed shortcuts (SPEC §3.3). Read-only here:
 * the chord itself is rebound through the normal binding UI, and this exists so
 * an inactive hotkey names its holder instead of just failing to fire. */
export const ExtensionShortcuts: React.FC<{ id: string }> = ({ id }) => {
  const [rows, setRows] = useState<ShortcutStatus[]>([]);

  useEffect(() => {
    invoke<ShortcutStatus[]>("extension_shortcuts_status", { id })
      .then(setRows)
      .catch(() => setRows([]));
  }, [id]);

  if (rows.length === 0) return null;

  return (
    <div className="space-y-2">
      <h3 className="px-1 text-sm font-medium text-ink-soft">Shortcuts</h3>
      <div className="rounded-xl border border-line bg-paper-raised divide-y divide-line">
        {rows.map((row) => (
          <div key={row.id} className="flex items-center gap-3 px-4 py-3">
            <div className="flex-1 min-w-0">
              <div className="text-sm text-ink">{row.label}</div>
              {!row.active && (
                <div className="text-xs text-amber-600">
                  {row.conflicts_with
                    ? `Inactive — ${row.conflicts_with} already uses this shortcut. Rebind it to activate.`
                    : "Inactive — no shortcut is assigned."}
                </div>
              )}
            </div>
            <kbd
              className={`px-2 py-1 rounded-lg border border-line bg-paper-sunken text-xs ${
                row.active ? "text-ink" : "text-ink-faint line-through"
              }`}
            >
              {row.binding || "—"}
            </kbd>
          </div>
        ))}
      </div>
    </div>
  );
};

/** [GRAIN] The extension settings anchored at one point in a core section
 * (SPEC §4.3) — this is what puts an extension's settings *next to the feature
 * it extends* instead of in a tab of its own.
 *
 * Renders nothing at all when no enabled extension anchors here, so a core
 * section is untouched by the platform until an extension actually uses it. */
export const ExtensionAnchor: React.FC<{ anchor: Anchor }> = ({ anchor }) => {
  const [sections, setSections] = useState<SettingsSection[]>([]);

  const refresh = useCallback(async () => {
    try {
      setSections(
        await invoke<SettingsSection[]>("extension_settings_sections"),
      );
    } catch {
      // A settings page must never fail to render because of an extension.
      setSections([]);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const anchored = sections
    .map((s) => ({ s, rows: s.rows.filter((r) => r.anchor === anchor) }))
    .filter((g) => g.rows.length > 0);

  if (anchored.length === 0) return null;

  return (
    <div className="space-y-6">
      {anchored.map(({ s, rows }) => (
        <div key={s.id} className="space-y-2">
          <h3 className="px-1 text-sm font-medium text-ink-soft">{s.name}</h3>
          <ExtensionSettings section={s} rows={rows} />
        </div>
      ))}
    </div>
  );
};
