import React, { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { HostView, hostViewId } from "./hostViews";

/** Mirror of the Rust `ExtensionSettingRow` (grain_commands.rs). Local type
 * until the next dev run regenerates bindings.ts — never hand-edit bindings. */
type SettingKindName =
  | "bool"
  | "string"
  | "secret"
  | "number"
  | "select"
  | "shortcut"
  | "color"
  | "slider"
  | "app_path"
  | "url"
  | "list"
  | "panel"
  | "unsupported";

/** The SCHEMA of one field (no value) — mirror of Rust `ExtensionSettingField`.
 * Recursive: a `list` field carries its own `fields`. */
export interface SettingField {
  key: string;
  label: string;
  description: string;
  kind: SettingKindName;
  min: number | null;
  max: number | null;
  step: number | null;
  options: { value: string; label: string }[];
  fields: SettingField[];
  item_label: string | null;
  /** The card HTML for a `panel` field (null otherwise). */
  ui_source: string | null;
}

/** Mirror of the Rust `ExtensionSettingRow` (grain_commands.rs). Local type
 * until the next dev run regenerates bindings.ts — never hand-edit bindings. */
export interface SettingRow {
  key: string;
  label: string;
  description: string;
  kind: SettingKindName;
  anchor: string | null;
  order: number;
  value: unknown;
  notice: string | null;
  min: number | null;
  max: number | null;
  step: number | null;
  options: { value: string; label: string }[];
  fields: SettingField[];
  item_label: string | null;
  /** The card HTML for a `panel` row (null otherwise). */
  ui_source: string | null;
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

const INPUT_CLASS =
  "px-2 py-1 rounded-lg bg-paper-sunken border border-line text-sm text-ink outline-none focus:border-accent/50 disabled:opacity-50";

/** Open the host's native file picker for `extId`; resolves to the chosen path
 * (also recorded as approved for open:app) or null. */
function pickAppFor(extId: string): Promise<string | null> {
  return invoke<string | null>("extension_pick_app", { id: extId });
}

/** [GRAIN] The runtime injected ahead of a custom card's HTML (SPEC §4.1 Level
 * 3). It gives the sandboxed iframe a `window.grain` that is a postMessage proxy
 * to this (trusted) settings page, which relays each call to the host — so an
 * author writes a card exactly like a worker/surface. The iframe is sandboxed
 * (`allow-scripts` only, opaque origin), so this channel is the ONLY thing it
 * has, and every method is capability-checked in Rust. A ResizeObserver reports
 * the content height so the host can grow the frame to fit (no inner scrollbar).*/
const PANEL_BRIDGE = `<script>(function(){
  var seq=0, pending={};
  function call(method, params){
    return new Promise(function(resolve,reject){
      var id=++seq; pending[id]={resolve:resolve,reject:reject};
      parent.postMessage({__grain:1,id:id,method:method,params:params||{}}, "*");
    });
  }
  function asErr(raw){
    var info = raw && typeof raw==="object" ? raw : {code:"E_INTERNAL",message:String(raw),hint:"",docs:""};
    var e=new Error(String(info.message||"Host call failed")); e.name="GrainError";
    e.code=String(info.code||"E_INTERNAL"); e.hint=String(info.hint||""); e.docs=String(info.docs||"");
    if(info.capability!=null) e.capability=String(info.capability); return e;
  }
  window.addEventListener("message", function(ev){
    var d=ev.data; if(!d||d.__grainres!==1) return;
    var p=pending[d.id]; if(!p) return; delete pending[d.id];
    if(d.err!=null) p.reject(asErr(d.err)); else p.resolve(d.ok);
  });
  function postHeight(){ try{ parent.postMessage({__grainresize:1,height:Math.ceil(document.documentElement.getBoundingClientRect().height)}, "*"); }catch(e){} }
  window.addEventListener("load", postHeight);
  try{ new ResizeObserver(postHeight).observe(document.documentElement); }catch(e){ setInterval(postHeight, 500); }
  window.grain={
    log:{info:function(m){return call("log.info",{msg:String(m)});},warn:function(m){return call("log.warn",{msg:String(m)});}},
    storage:{get:function(k){return call("storage.get",{key:k});},set:function(k,v){return call("storage.set",{key:k,value:v});},"delete":function(k){return call("storage.delete",{key:k});}},
    settings:{get:function(k){return call("settings.get",{key:k});},set:function(k,v){return call("settings.set",{key:k,value:v});}},
    llm:{complete:function(p){return call("llm.complete",{prompt:String(p)});}},
    embed:function(t){return call("embed",{texts:t}).then(function(r){return r&&r.vectors!=null?r.vectors:r;});},
    open:{url:function(u){return call("open.url",{url:String(u)});},app:function(p){return call("open.app",{path:String(p)});},pickApp:function(){return call("open.pickApp",{}).then(function(r){return r&&r.path!=null?r.path:null;});}},
    capture:{selection:function(){return call("capture.selection",{}).then(function(r){return r&&r.text!=null?r.text:null;});}},
    focusedApp:function(){return call("capture.app",{});},
    call:call
  };
})();<\/script>`;

/** [GRAIN] A custom card (SPEC §4.1 Level 3): the extension's own HTML in a
 * sandboxed iframe. Created on scroll-into-view and destroyed on unmount (the
 * "destroy if not in use" rule). Host calls from the frame are relayed to
 * `extension_host_call` with a FIXED extension id — the iframe can neither forge
 * an identity nor reach another extension's grants. */
const PanelCard: React.FC<{ extId: string; uiSource: string }> = ({
  extId,
  uiSource,
}) => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const frameRef = useRef<HTMLIFrameElement | null>(null);
  const [mounted, setMounted] = useState(false);
  const [height, setHeight] = useState(320);

  // Lazy-mount: build the iframe (and the extension's DOM/JS realm) only once it
  // scrolls into view.
  useEffect(() => {
    const el = containerRef.current;
    if (!el || mounted) return;
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) setMounted(true);
    });
    io.observe(el);
    return () => io.disconnect();
  }, [mounted]);

  // Relay calls (and height reports) from THIS frame only; the id is ours.
  useEffect(() => {
    if (!mounted) return;
    const onMsg = (ev: MessageEvent) => {
      const d = ev.data as
        | { __grain?: number; __grainresize?: number; [k: string]: unknown }
        | null;
      if (!d || !frameRef.current || ev.source !== frameRef.current.contentWindow)
        return;
      if (d.__grainresize === 1 && typeof d.height === "number") {
        setHeight(Math.min(Math.max(d.height, 80), 900));
        return;
      }
      if (d.__grain !== 1 || typeof d.method !== "string") return;
      const id = d.id;
      const reply = (ok: unknown, err: unknown) =>
        frameRef.current?.contentWindow?.postMessage(
          { __grainres: 1, id, ok, err },
          "*",
        );
      void invoke("extension_host_call", {
        id: extId,
        method: d.method,
        params: (d.params as unknown) ?? {},
      })
        .then((ok) => reply(ok, null))
        .catch((err) => reply(null, err));
    };
    window.addEventListener("message", onMsg);
    return () => window.removeEventListener("message", onMsg);
  }, [mounted, extId]);

  return (
    <div ref={containerRef} className="w-full">
      {mounted && (
        <iframe
          ref={frameRef}
          title="Extension settings card"
          sandbox="allow-scripts"
          srcDoc={PANEL_BRIDGE + uiSource}
          className="w-full block rounded-xl border border-line bg-paper"
          style={{ height }}
        />
      )}
    </div>
  );
};

/** [GRAIN] The `app_path` control: primary action is "Capture focused app" — a
 * short countdown lets the user switch to the target app, then the host
 * snapshots it (and records it as approved for open:app). A file-choose
 * fallback stays for apps that are hard to focus. Shared by list rows and
 * top-level rows. */
const AppField: React.FC<{
  value: unknown;
  extId: string;
  disabled: boolean;
  onChange: (value: unknown) => void;
}> = ({ value, extId, disabled, onChange }) => {
  const [countdown, setCountdown] = useState<number | null>(null);
  const name =
    typeof value === "string" && value ? value.split(/[\\/]/).pop() : null;

  const capture = () => {
    let n = 3;
    setCountdown(n);
    const tick = () => {
      n -= 1;
      if (n > 0) {
        setCountdown(n);
        setTimeout(tick, 1000);
      } else {
        setCountdown(null);
        void invoke<string | null>("extension_capture_app", { id: extId }).then(
          (p) => {
            if (p) onChange(p);
          },
        );
      }
    };
    setTimeout(tick, 1000);
  };

  if (countdown != null) {
    return (
      <span className="text-xs text-accent tabular-nums whitespace-nowrap">
        Switch to your app… {countdown}
      </span>
    );
  }
  return (
    <div className="flex items-center gap-2 min-w-0">
      <span
        className="text-xs text-ink-soft truncate max-w-[9rem]"
        title={typeof value === "string" ? value : ""}
      >
        {name || "No app chosen"}
      </span>
      <button
        type="button"
        disabled={disabled}
        onClick={capture}
        className="px-2 py-1 rounded-lg border border-line text-xs text-ink hover:border-ink-faint cursor-pointer shrink-0"
      >
        Capture app
      </button>
      <button
        type="button"
        disabled={disabled}
        onClick={() => void pickAppFor(extId).then((p) => p && onChange(p))}
        title="Choose a file instead"
        className="text-ink-faint hover:text-ink cursor-pointer text-xs shrink-0"
      >
        Browse…
      </button>
    </div>
  );
};

/** [GRAIN] A single field editor used INSIDE a `list` row — edits local state and
 * bubbles the whole value up via `onChange` (the parent list commits the array
 * as one write). Reusable across any list/nested-list schema. */
const FieldInput: React.FC<{
  field: SettingField;
  value: unknown;
  extId: string;
  disabled: boolean;
  onChange: (value: unknown) => void;
}> = ({ field, value, extId, disabled, onChange }) => {
  switch (field.kind) {
    case "bool":
      return (
        <button
          type="button"
          role="switch"
          aria-checked={value === true}
          aria-label={field.label}
          disabled={disabled}
          onClick={() => onChange(value !== true)}
          className={`relative w-9 h-5 rounded-full transition-colors cursor-pointer shrink-0 ${
            value === true ? "bg-accent" : "bg-paper-sunken border border-line"
          }`}
        >
          <span
            className={`absolute top-0.5 w-4 h-4 rounded-full bg-paper-raised shadow transition-all ${
              value === true ? "left-[18px]" : "left-0.5"
            }`}
          />
        </button>
      );
    case "select":
      return (
        <select
          aria-label={field.label}
          disabled={disabled}
          value={typeof value === "string" ? value : ""}
          onChange={(e) => onChange(e.target.value)}
          className={`${INPUT_CLASS} cursor-pointer`}
        >
          {field.options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      );
    case "app_path":
      return (
        <AppField value={value} extId={extId} disabled={disabled} onChange={onChange} />
      );
    case "number":
    case "slider":
      return (
        <input
          type="number"
          aria-label={field.label}
          disabled={disabled}
          min={field.min ?? undefined}
          max={field.max ?? undefined}
          step={field.step ?? undefined}
          defaultValue={typeof value === "number" ? value : 0}
          key={String(value)}
          onBlur={(e) => {
            const n = Number(e.target.value);
            if (!Number.isNaN(n)) onChange(n);
          }}
          className={`${INPUT_CLASS} w-24 text-right`}
        />
      );
    case "color":
      return (
        <input
          type="color"
          aria-label={field.label}
          disabled={disabled}
          value={typeof value === "string" ? value : "#000000"}
          onChange={(e) => onChange(e.target.value)}
          className="w-9 h-6 rounded border border-line bg-transparent cursor-pointer"
        />
      );
    case "list":
      return (
        <ListEditor
          field={field}
          value={
            Array.isArray(value) ? (value as Record<string, unknown>[]) : []
          }
          extId={extId}
          disabled={disabled}
          onChange={onChange}
        />
      );
    // string / url / shortcut — a plain text field. URL validity is enforced by
    // the backend on commit; the field just captures text.
    default:
      return (
        <input
          type="text"
          aria-label={field.label}
          disabled={disabled}
          defaultValue={typeof value === "string" ? value : ""}
          key={String(value)}
          placeholder={field.kind === "url" ? "https://…" : ""}
          onBlur={(e) => onChange(e.target.value)}
          className={`${INPUT_CLASS} flex-1 min-w-0`}
        />
      );
  }
};

/** [GRAIN] The reusable repeatable-list editor (SPEC §4 `list`). Renders each
 * row's fields via [`FieldInput`], with add/remove — the native, no-webview way
 * an extension builds a rich config (workflows, rules, mappings) at an anchor. */
const ListEditor: React.FC<{
  field: SettingField;
  value: Record<string, unknown>[];
  extId: string;
  disabled: boolean;
  onChange: (value: unknown) => void;
}> = ({ field, value, extId, disabled, onChange }) => {
  const noun = field.item_label || "item";
  const blankRow = (): Record<string, unknown> => {
    const row: Record<string, unknown> = {};
    field.fields.forEach((f) => {
      row[f.key] =
        f.kind === "bool"
          ? false
          : f.kind === "list"
            ? []
            : f.kind === "number" || f.kind === "slider"
              ? 0
              : "";
    });
    return row;
  };
  const setRow = (i: number, key: string, v: unknown) => {
    const next = value.map((r, idx) => (idx === i ? { ...r, [key]: v } : r));
    onChange(next);
  };
  return (
    <div className="w-full space-y-2">
      {/* The list grows row by row and, once it passes ~6 rows, becomes a
          scroll area of fixed height (SPEC list rule) rather than pushing the
          page down forever. */}
      <div className="space-y-2 max-h-[22rem] overflow-y-auto">
        {value.length === 0 && (
          <div className="text-xs text-ink-soft italic px-1 py-2">
            No {noun}s yet.
          </div>
        )}
        {value.map((row, i) => (
          <div
            key={i}
            className="rounded-lg border border-line bg-paper p-3 space-y-2.5 shadow-[0_1px_2px_rgba(0,0,0,0.04)]"
          >
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-semibold uppercase tracking-wide text-ink-soft capitalize">
                {noun} {i + 1}
              </span>
              <button
                type="button"
                disabled={disabled}
                onClick={() => onChange(value.filter((_, idx) => idx !== i))}
                className="text-ink-soft hover:text-red-600 cursor-pointer text-xs font-medium"
                aria-label={`Remove ${noun} ${i + 1}`}
              >
                Remove
              </button>
            </div>
            {field.fields.map((f) => (
              <div
                key={f.key}
                className={
                  f.kind === "list"
                    ? "space-y-1.5"
                    : "flex items-center gap-3 justify-between"
                }
              >
                <span className="text-xs font-medium text-ink shrink-0">
                  {f.label}
                </span>
                <div
                  className={
                    f.kind === "list"
                      ? "w-full"
                      : "flex-1 min-w-0 flex justify-end"
                  }
                >
                  <FieldInput
                    field={f}
                    value={row[f.key]}
                    extId={extId}
                    disabled={disabled}
                    onChange={(v) => setRow(i, f.key, v)}
                  />
                </div>
              </div>
            ))}
          </div>
        ))}
      </div>
      <button
        type="button"
        disabled={disabled}
        onClick={() => onChange([...value, blankRow()])}
        className="px-3 py-1.5 rounded-lg border border-dashed border-ink-faint/40 text-xs font-medium text-ink-soft hover:text-ink hover:border-ink-faint hover:bg-paper-sunken/50 cursor-pointer transition-colors"
      >
        + Add {noun}
      </button>
    </div>
  );
};

/** One schema-declared control. The renderer knows `kind`, never the
 * extension — there is no per-extension code anywhere in this file. */
const Control: React.FC<{
  row: SettingRow;
  extId: string;
  disabled: boolean;
  onCommit: (value: unknown) => void;
}> = ({ row, extId, disabled, onCommit }) => {
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
    case "url":
      return (
        <input
          type="text"
          aria-label={row.label}
          disabled={disabled}
          value={draft}
          placeholder={row.kind === "url" ? "https://…" : ""}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => {
            if (draft !== row.value) onCommit(draft);
          }}
          className={`${inputClass} w-48`}
        />
      );

    case "app_path":
      // Same native control as inside a list row: primary "Capture app" with a
      // countdown, plus a file-choose fallback.
      return (
        <AppField
          value={row.value}
          extId={extId}
          disabled={disabled}
          onChange={onCommit}
        />
      );

    // `list` is rendered full-width at the row level (see below), and an
    // unsupported kind is dropped by the backend before it reaches here.
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
        {rows.map((row) =>
          row.kind === "panel" ? (
            // A custom card: the extension's own UI, full-width. An optional
            // label/description sits above the sandboxed iframe.
            <div key={row.key} className="px-4 py-3 space-y-2">
              {(row.label || row.description) && (
                <div>
                  {row.label && (
                    <div className="text-sm font-medium text-ink">
                      {row.label}
                    </div>
                  )}
                  {row.description && (
                    <div className="text-xs text-ink-soft">
                      {row.description}
                    </div>
                  )}
                </div>
              )}
              {row.ui_source ? (
                // `grain://<view-id>` is a HOST view — Grain's own React, not
                // author markup — and only a builtin-tier pack can name one
                // (enforced in grain-sdk at import, not here).
                hostViewId(row.ui_source) ? (
                  <HostView viewId={hostViewId(row.ui_source)!} />
                ) : (
                  <PanelCard extId={section.id} uiSource={row.ui_source} />
                )
              ) : null}
            </div>
          ) : row.kind === "list" ? (
            // A list is a full-width editor: label on top, rows below.
            <div key={row.key} className="px-4 py-3 space-y-2">
              {(row.label || row.description || row.notice) && (
                <div>
                  {row.label && (
                    <div className="text-sm font-medium text-ink">
                      {row.label}
                    </div>
                  )}
                  {row.description && (
                    <div className="text-xs text-ink-soft">
                      {row.description}
                    </div>
                  )}
                  {row.notice && (
                    <div className="text-xs text-amber-600 mt-0.5">
                      {row.notice}
                    </div>
                  )}
                </div>
              )}
              <ListEditor
                field={{
                  key: row.key,
                  label: row.label,
                  description: row.description,
                  kind: "list",
                  min: row.min,
                  max: row.max,
                  step: row.step,
                  options: row.options,
                  fields: row.fields,
                  item_label: row.item_label,
                  ui_source: null,
                }}
                value={
                  Array.isArray(row.value)
                    ? (row.value as Record<string, unknown>[])
                    : []
                }
                extId={section.id}
                disabled={busy === row.key}
                onChange={(v) => void commit(row, v)}
              />
            </div>
          ) : (
            <div key={row.key} className="flex items-center gap-3 px-4 py-3">
              <div className="flex-1 min-w-0">
                <div className="text-sm text-ink">{row.label}</div>
                {row.description && (
                  <div className="text-xs text-ink-faint">
                    {row.description}
                  </div>
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
                extId={section.id}
                disabled={busy === row.key}
                onCommit={(v) => void commit(row, v)}
              />
            </div>
          ),
        )}
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
