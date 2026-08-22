import React, {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ShieldCheck, X } from "lucide-react";
import type { ExtensionCard, StoreEntry } from "@/bindings";
import { useTheme } from "../../../contexts/ThemeContext";
import { serializeExtensionPalette } from "@/lib/extensionTheme";
import {
  StudioExtensionCard,
  StudioExtensionMoreCard,
} from "@/extensions/StudioExtensionCard";
import {
  actionsByDomain,
  capabilityLabel,
  parseApprovalRequest,
  parseSlotConflict,
  promptLayerScope,
  slotLabel,
  studioShelfMode,
  type ApprovalRequest,
  type SlotConflict,
} from "@/extensions/extensionRuntime";

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
  "grainspace.after",
  "models.after",
] as const;

export type Anchor = (typeof ANCHORS)[number];

export interface ExtensionAnchorSnapshot {
  installedCount: number;
  loading: boolean;
  error: string | null;
}

const ANCHOR_LABELS: Record<Anchor, string> = {
  "snippets.after": "Snippets",
  "dictation.pipeline.after": "Dictation",
  "context.after": "Context awareness",
  "agent.after": "Agent",
  "grainspace.after": "Grain Space",
  "models.after": "Speech models",
};
const ANCHOR_SURFACES: Record<Anchor, readonly string[]> = {
  "snippets.after": ["snippets.after"],
  "dictation.pipeline.after": ["dictation.pipeline.after", "dictation.prompts"],
  "context.after": ["context.after"],
  "agent.after": ["agent.after", "agent.reply-surface"],
  "grainspace.after": ["grainspace.after"],
  "models.after": ["models.after"],
};
const CONFIGURE_DONE_LABEL = "Done";
const CONFIGURE_SAVED_LABEL = "Changes are saved as you make them.";
const INSTALLED_EXTENSIONS_LABEL = "Installed extensions";
const LOADING_SETTINGS_LABEL = "Loading settings…";
const NO_SETUP_LABEL = "No setup required";
const ENABLE_PERMISSION_COPY =
  "This extension runs code on your device and is asking to:";
const ENABLE_PROMPT_COPY =
  "It adds these instructions to what Grain sends the AI when you dictate. They rank below your own prompt.";
const ENABLE_ACTIONS_COPY = "It can do these things when you ask out loud:";
const ENABLE_ACTION_CONFIRM_LABEL = "asks you first";
const ENABLE_CANCEL_LABEL = "Cancel";
const ENABLE_APPROVE_LABEL = "Allow and enable";
const ENABLE_KEEP_CURRENT_LABEL = "Keep current";
const ENABLE_REPLACE_LABEL = "Replace and enable";

function surfacesMatchAnchor(surfaces: readonly string[], anchor: Anchor) {
  const accepted = ANCHOR_SURFACES[anchor];
  return surfaces.some((surface) => accepted.includes(surface));
}

const INPUT_CLASS =
  "px-2 py-1 rounded-lg bg-paper-sunken border border-line text-sm text-ink outline-none focus:border-accent/50 disabled:opacity-50";
const SWITCH_TO_APP_LABEL = "Switch to your app\u2026";
const CAPTURE_APP_LABEL = "Capture app";
const BROWSE_LABEL = "Browse\u2026";
const REMOVE_LABEL = "Remove";
const CLEAR_LABEL = "Clear";
const SHORTCUTS_LABEL = "Shortcuts";
const emptyListLabel = (noun: string) => `No ${noun}s yet.`;
const addListLabel = (noun: string) => `+ Add ${noun}`;

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
  function contentHeight(){
    var d=document.documentElement, b=document.body;
    return Math.ceil(Math.max(
      d?d.scrollHeight:0, b?b.scrollHeight:0,
      b?b.getBoundingClientRect().height:0
    ));
  }
  function postHeight(){ try{ parent.postMessage({__grainresize:1,height:contentHeight()}, "*"); }catch(e){} }
  window.addEventListener("load", postHeight);
  try{
    var ro=new ResizeObserver(postHeight);
    ro.observe(document.documentElement);
    if(document.body) ro.observe(document.body);
  }catch(e){ var t=setInterval(postHeight,500); addEventListener("pagehide",function(){clearInterval(t);}); }
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

/** Cards grow to fit rather than scrolling inside (see the bridge). The ceiling
 * only exists so a runaway card can't produce a mile-long frame; the settings
 * page itself scrolls long content. */
const PANEL_MIN_HEIGHT = 80;
const PANEL_MAX_HEIGHT = 2400;

/** Grain's live palette, handed to the card as `--grain-*` custom properties so
 * an author can adopt the app's colours rather than guess at them. Read from the
 * computed root each time a card mounts, so it cannot drift from the tokens the
 * rest of the settings window is drawn with. */
const hostPalette = (): string => {
  const root = getComputedStyle(document.documentElement);
  // Only tokens that actually RESOLVED — the same filter extension-surface.ts
  // carries, and for the same reason: an empty custom property is still a SET
  // one, so emitting `--grain-paper:` would make an author's
  // `var(--grain-paper, #ece5da)` resolve to nothing instead of to their
  // fallback, and the card would render with no colour at all. Cards were
  // missing this, so a token rename would have broken them while surfaces
  // degraded gracefully (docs/UI 2.0/PLAN.md §6.1).
  return serializeExtensionPalette((name) => root.getPropertyValue(name));
};

/**
 * Grain owns a card's colour scheme; the operating system does not.
 *
 * A sandboxed iframe has an opaque origin, so `prefers-color-scheme` inside it
 * reports the SYSTEM setting — which is the wrong answer whenever the user's OS
 * and their Grain theme disagree, and is exactly why a card rendered dark inside
 * a light Grain. Rewriting the author's query to a condition that tracks GRAIN
 * asks nothing of the author, so it corrects cards that are already installed as
 * well as ones written against `[data-grain-theme]`.
 *
 * `(min-width:0)` always matches; `(max-width:0)` never does.
 */
export function alignColorScheme(src: string, dark: boolean): string {
  const on = "(min-width:0)";
  const off = "(max-width:0)";
  return src
    .replace(/\(\s*prefers-color-scheme\s*:\s*dark\s*\)/gi, dark ? on : off)
    .replace(/\(\s*prefers-color-scheme\s*:\s*light\s*\)/gi, dark ? off : on);
}

/** The full document handed to a card's frame: host bridge, then Grain's theme,
 * then the author's markup. The theme is written into the document rather than
 * messaged in after load, so a card can never paint in the wrong one first. */
const panelDocument = (uiSource: string, dark: boolean): string =>
  PANEL_BRIDGE +
  `<style>:root{color-scheme:${dark ? "dark" : "light"};${hostPalette()}}` +
  `html,body{margin:0;padding:0;}html{overflow:hidden;}</style>` +
  `<script>document.documentElement.setAttribute("data-grain-theme","${
    dark ? "dark" : "light"
  }");<\/script>` +
  alignColorScheme(uiSource, dark);

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
  const { isSettingsDark } = useTheme();

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
      const d = ev.data as {
        __grain?: number;
        __grainresize?: number;
        [k: string]: unknown;
      } | null;
      if (
        !d ||
        !frameRef.current ||
        ev.source !== frameRef.current.contentWindow
      )
        return;
      if (d.__grainresize === 1 && typeof d.height === "number") {
        setHeight(
          Math.min(Math.max(d.height, PANEL_MIN_HEIGHT), PANEL_MAX_HEIGHT),
        );
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
    // No border, no radius, no background: the card draws its OWN surface, and
    // wrapping that in a second one is what made an extension read as a foreign
    // thing bolted into the page. The frame is given the width and gets out of
    // the way. Changing theme rewrites the document (see `panelDocument`), which
    // reloads the frame — correct by construction, and the only moment a card is
    // ever rebuilt.
    <div ref={containerRef} className="w-full">
      {mounted && (
        <iframe
          ref={frameRef}
          title="Extension settings card"
          sandbox="allow-scripts"
          srcDoc={panelDocument(uiSource, isSettingsDark)}
          className="w-full block border-0 bg-transparent"
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
        {SWITCH_TO_APP_LABEL} {countdown}
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
        {CAPTURE_APP_LABEL}
      </button>
      <button
        type="button"
        disabled={disabled}
        onClick={() => void pickAppFor(extId).then((p) => p && onChange(p))}
        title="Choose a file instead"
        className="text-ink-faint hover:text-ink cursor-pointer text-xs shrink-0"
      >
        {BROWSE_LABEL}
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
        <AppField
          value={value}
          extId={extId}
          disabled={disabled}
          onChange={onChange}
        />
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
            {emptyListLabel(noun)}
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
                {REMOVE_LABEL}
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
        {addListLabel(noun)}
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
              {CLEAR_LABEL}
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

  // A custom card renders EDGE TO EDGE, outside the settings container: it
  // draws its own surface, and nesting that inside Grain's bordered row list
  // stacked two frames around one piece of UI. So consecutive ordinary rows are
  // grouped into a container and each panel is emitted bare between them, with
  // the declared order preserved either way.
  const groups: { panel: boolean; rows: SettingRow[] }[] = [];
  for (const row of rows) {
    const panel = row.kind === "panel";
    const last = groups[groups.length - 1];
    if (!panel && last && !last.panel) last.rows.push(row);
    else groups.push({ panel, rows: [row] });
  }

  const renderPanel = (row: SettingRow) => (
    <div key={row.key} className="space-y-2">
      {(row.label || row.description) && (
        <div className="px-1">
          {row.label && (
            <div className="text-sm font-medium text-ink">{row.label}</div>
          )}
          {row.description && (
            <div className="text-xs text-ink-soft">{row.description}</div>
          )}
        </div>
      )}
      {row.ui_source ? (
        <PanelCard extId={section.id} uiSource={row.ui_source} />
      ) : null}
    </div>
  );

  const renderGroup = (
    group: { panel: boolean; rows: SettingRow[] },
    i: number,
  ) =>
    group.panel ? (
      renderPanel(group.rows[0])
    ) : (
      <div
        key={`rows-${i}`}
        className="rounded-xl border border-line bg-paper-raised divide-y divide-line"
      >
        {group.rows.map((row) =>
          row.kind === "list" ? (
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
    );

  return (
    <div className="space-y-2">
      {error && (
        <div className="px-3 py-2 rounded-lg bg-red-500/10 text-red-600 text-xs">
          {error}
        </div>
      )}
      {groups.map(renderGroup)}
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
      <h3 className="px-1 text-sm font-medium text-ink-soft">
        {SHORTCUTS_LABEL}
      </h3>
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

/** One installed extension associated with this host feature. Store surface
 * metadata keeps freshly-installed (therefore disabled) packs visible, while
 * enabled packs can also be located from their live settings rows or slots. */
interface AnchoredExtension {
  id: string;
  name: string;
  rows: SettingRow[];
  card: ExtensionCard;
  storeEntry?: StoreEntry;
}

function trustPresentation(trust?: string): {
  label: string;
  tone: "verified" | "community" | "experimental" | "core" | "dev";
} {
  if (trust === "core") return { label: "Built in", tone: "core" };
  if (trust === "dev") return { label: "Development", tone: "dev" };
  if (trust === "verified") return { label: "Verified", tone: "verified" };
  if (trust === "experimental") {
    return { label: "Experimental", tone: "experimental" };
  }
  return { label: "Community", tone: "community" };
}

function openAnchorStore(anchor: Anchor) {
  window.location.hash = `/extensions/store?q=${encodeURIComponent(ANCHOR_LABELS[anchor])}`;
}

type PendingEnable = { card: ExtensionCard } & ApprovalRequest;
type ContestedEnable = { card: ExtensionCard; conflict: SlotConflict };

function ExtensionEnableDialogs({
  pending,
  contested,
  occupantName,
  onCancelApproval,
  onCancelConflict,
  onApprove,
  onTakeOver,
}: {
  pending: PendingEnable | null;
  contested: ContestedEnable | null;
  occupantName: (id: string) => string;
  onCancelApproval: () => void;
  onCancelConflict: () => void;
  onApprove: () => void;
  onTakeOver: () => void;
}) {
  const permissionTitleId = useId();
  const conflictTitleId = useId();
  const approvalTitle = pending ? `Allow “${pending.card.name}”?` : "";
  const conflictTitle = contested
    ? `Replace ${slotLabel(contested.conflict.slot)}?`
    : "";
  const conflictDescription = contested
    ? `${occupantName(contested.conflict.currentOccupant)} currently controls ${slotLabel(contested.conflict.slot)}. Enabling ${contested.card.name} will switch it off.`
    : "";

  if (pending) {
    return (
      <div
        className="extension-confirm-backdrop"
        role="presentation"
        onClick={onCancelApproval}
      >
        <div
          className="extension-confirm"
          role="dialog"
          aria-modal="true"
          aria-labelledby={permissionTitleId}
          onClick={(event) => event.stopPropagation()}
        >
          <div className="extension-confirm-title">
            <ShieldCheck size={17} aria-hidden="true" />
            <h2 id={permissionTitleId}>{approvalTitle}</h2>
          </div>
          {pending.permissions.length > 0 && (
            <>
              <p>{ENABLE_PERMISSION_COPY}</p>
              <ul>
                {pending.permissions.map((permission) => (
                  <li key={permission}>{capabilityLabel(permission)}</li>
                ))}
              </ul>
            </>
          )}
          {pending.promptLayers.length > 0 && (
            <>
              <p>{ENABLE_PROMPT_COPY}</p>
              <ul className="extension-prompt-layers">
                {pending.promptLayers.map((layer) => (
                  <li key={layer.id}>
                    <span className="extension-prompt-layer-scope">
                      {promptLayerScope(layer)}
                    </span>
                    <q>{layer.text}</q>
                  </li>
                ))}
              </ul>
            </>
          )}
          {pending.actions.length > 0 && (
            <>
              <p>{ENABLE_ACTIONS_COPY}</p>
              <ul className="extension-actions">
                {actionsByDomain(pending.actions).map((group) => (
                  <li key={group.domain}>
                    <span className="extension-action-domain">
                      {group.domain}
                    </span>
                    <span className="extension-action-titles">
                      {group.titles.join(", ")}
                    </span>
                    {group.confirms && (
                      <span className="extension-action-confirms">
                        {ENABLE_ACTION_CONFIRM_LABEL}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </>
          )}
          <div className="extension-confirm-actions">
            <button className="button" type="button" onClick={onCancelApproval}>
              {ENABLE_CANCEL_LABEL}
            </button>
            <button
              className="button primary"
              type="button"
              onClick={onApprove}
            >
              {ENABLE_APPROVE_LABEL}
            </button>
          </div>
        </div>
      </div>
    );
  }

  if (!contested) return null;

  return (
    <div
      className="extension-confirm-backdrop"
      role="presentation"
      onClick={onCancelConflict}
    >
      <div
        className="extension-confirm"
        role="dialog"
        aria-modal="true"
        aria-labelledby={conflictTitleId}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="extension-confirm-title">
          <ShieldCheck size={17} aria-hidden="true" />
          <h2 id={conflictTitleId}>{conflictTitle}</h2>
        </div>
        <p>{conflictDescription}</p>
        <div className="extension-confirm-actions">
          <button className="button" type="button" onClick={onCancelConflict}>
            {ENABLE_KEEP_CURRENT_LABEL}
          </button>
          <button className="button primary" type="button" onClick={onTakeOver}>
            {ENABLE_REPLACE_LABEL}
          </button>
        </div>
      </div>
    </div>
  );
}

function ExtensionConfigureDialog({
  extension,
  anchor,
  loading,
  onClose,
}: {
  extension: AnchoredExtension;
  anchor: Anchor;
  loading: boolean;
  onClose: () => void;
}) {
  const dialogRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const previouslyFocused = document.activeElement as HTMLElement | null;
    const previousBodyOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    closeRef.current?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = Array.from(
        dialogRef.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.body.style.overflow = previousBodyOverflow;
      previouslyFocused?.focus();
    };
  }, [onClose]);

  const name = extension.card.name;
  const description =
    extension.card.description ||
    `Configure how this extension works with ${ANCHOR_LABELS[anchor]}.`;
  const trust = trustPresentation(extension.card.trust);
  const contextLabel = `${ANCHOR_LABELS[anchor]} extension`;
  const versionLabel = extension.card.version
    ? `Version ${extension.card.version}`
    : null;
  const section: SettingsSection = {
    id: extension.id,
    name: extension.name,
    rows: extension.rows,
  };
  const noSetupDescription = `This extension is installed and does not expose settings for ${ANCHOR_LABELS[anchor]}.`;
  const footerNote =
    extension.rows.length > 0
      ? CONFIGURE_SAVED_LABEL
      : extension.card.enabled
        ? "Installed and enabled."
        : "Installed. Enable it from Extensions when ready.";

  return (
    <div
      className="extension-configure-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section
        ref={dialogRef}
        className="extension-configure-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="extension-configure-title"
        aria-describedby="extension-configure-description"
      >
        <header className="extension-configure-header">
          <div>
            <span className="extension-configure-context">{contextLabel}</span>
            <h2 id="extension-configure-title">{name}</h2>
            <p id="extension-configure-description">{description}</p>
          </div>
          <button
            ref={closeRef}
            className="extension-configure-close"
            type="button"
            aria-label={`Close ${name} settings`}
            onClick={onClose}
          >
            <X size={17} aria-hidden="true" />
          </button>
        </header>
        <div className="extension-configure-meta">
          <span className="studio-extension-badge" data-tone={trust.tone}>
            {trust.label}
          </span>
          {versionLabel && <span>{versionLabel}</span>}
        </div>
        <div className="extension-configure-content">
          {loading ? (
            <div className="extension-configure-empty" role="status">
              <strong>{LOADING_SETTINGS_LABEL}</strong>
            </div>
          ) : extension.rows.length > 0 ? (
            <ExtensionSettings section={section} rows={extension.rows} />
          ) : (
            <div className="extension-configure-empty">
              <strong>{NO_SETUP_LABEL}</strong>
              <span>{noSetupDescription}</span>
            </div>
          )}
        </div>
        <footer className="extension-configure-footer">
          <span>{footerNote}</span>
          <button type="button" onClick={onClose}>
            {CONFIGURE_DONE_LABEL}
          </button>
        </footer>
      </section>
    </div>
  );
}

/** [GRAIN] A host-owned shelf for extensions anchored next to a core feature.
 * Third-party settings never render directly in the page: each extension gets
 * a consistent card, and Configure opens its controls in an isolated dialog. */
export const ExtensionAnchor: React.FC<{
  anchor: Anchor;
  catalogueEntries?: StoreEntry[];
  onSnapshot?: (snapshot: ExtensionAnchorSnapshot) => void;
}> = ({ anchor, catalogueEntries = [], onSnapshot }) => {
  const [sections, setSections] = useState<SettingsSection[]>([]);
  const [cards, setCards] = useState<ExtensionCard[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [configuring, setConfiguring] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(false);
  const [pendingEnable, setPendingEnable] = useState<PendingEnable | null>(
    null,
  );
  const [contestedEnable, setContestedEnable] =
    useState<ContestedEnable | null>(null);
  const [resolvedRows, setResolvedRows] = useState<
    Record<string, SettingRow[]>
  >({});
  const [activeId, setActiveId] = useState<string | null>(null);
  const aliveRef = useRef(true);
  const shelfPanelId = useId();
  const shelfHeadingId = useId();

  useEffect(() => setCollapsed(false), [anchor]);

  const refresh = useCallback(async () => {
    try {
      const [nextSections, nextCards] = await Promise.all([
        invoke<SettingsSection[]>("extension_settings_sections"),
        invoke<ExtensionCard[]>("extensions_overview"),
      ]);
      if (!aliveRef.current) return;
      setSections(nextSections);
      setCards(nextCards);
      setError(null);
    } catch (reason) {
      if (!aliveRef.current) return;
      setSections([]);
      setCards([]);
      setError(String(reason));
    } finally {
      if (aliveRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    aliveRef.current = true;
    void refresh();
    return () => {
      aliveRef.current = false;
    };
  }, [refresh]);

  const anchored = useMemo<AnchoredExtension[]>(() => {
    const sectionsById = new Map(
      sections.map((section) => [section.id, section]),
    );
    const entriesById = new Map(
      catalogueEntries.map((entry) => [entry.id, entry]),
    );

    return cards.flatMap((card) => {
      const section = sectionsById.get(card.id);
      const rows = (section?.rows ?? []).filter((row) => row.anchor === anchor);
      const storeEntry = entriesById.get(card.id);
      const belongsHere =
        rows.length > 0 ||
        surfacesMatchAnchor(card.slots, anchor) ||
        surfacesMatchAnchor(storeEntry?.extends ?? [], anchor);

      return belongsHere
        ? [{ id: card.id, name: card.name, rows, card, storeEntry }]
        : [];
    });
  }, [anchor, cards, catalogueEntries, sections]);

  useEffect(() => {
    onSnapshot?.({
      installedCount: anchored.length,
      loading,
      error,
    });
  }, [anchored.length, error, loading, onSnapshot]);

  const uninstall = useCallback(
    async (extension: AnchoredExtension) => {
      const card = extension.card;
      if (!card) return;
      if (
        !window.confirm(
          `Uninstall "${card.name}"?\n\nIts saved data is kept for a future reinstall.`,
        )
      ) {
        return;
      }
      setBusy(card.id);
      setError(null);
      try {
        await invoke("extension_uninstall", { id: card.id, purge: false });
        if (activeId === card.id) setActiveId(null);
        setResolvedRows((current) => {
          const next = { ...current };
          delete next[card.id];
          return next;
        });
        await refresh();
      } catch (reason) {
        if (aliveRef.current) setError(String(reason));
      } finally {
        if (aliveRef.current) setBusy(null);
      }
    },
    [activeId, refresh],
  );

  const enable = useCallback(
    async (extension: AnchoredExtension) => {
      const { card } = extension;
      if (card.enabled) return;
      setBusy(card.id);
      setError(null);
      try {
        await invoke("extension_set_enabled", { id: card.id, enabled: true });
        await refresh();
      } catch (reason) {
        const message =
          reason instanceof Error ? reason.message : String(reason);
        const approval = parseApprovalRequest(message);
        const conflict = parseSlotConflict(message);
        if (approval) setPendingEnable({ card, ...approval });
        else if (conflict) setContestedEnable({ card, conflict });
        else setError(message);
      } finally {
        if (aliveRef.current) setBusy(null);
      }
    },
    [refresh],
  );

  const approveEnable = useCallback(async () => {
    if (!pendingEnable) return;
    const { card, permissions } = pendingEnable;
    setPendingEnable(null);
    setBusy(card.id);
    setError(null);
    try {
      await invoke("extension_grant", { id: card.id, permissions });
      await invoke("extension_set_enabled", { id: card.id, enabled: true });
      await refresh();
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      const conflict = parseSlotConflict(message);
      if (conflict) setContestedEnable({ card, conflict });
      else setError(message);
    } finally {
      if (aliveRef.current) setBusy(null);
    }
  }, [pendingEnable, refresh]);

  const takeOverSlot = useCallback(async () => {
    if (!contestedEnable) return;
    const { card, conflict } = contestedEnable;
    setContestedEnable(null);
    setBusy(card.id);
    setError(null);
    try {
      await invoke("extension_take_slot", { id: card.id, slot: conflict.slot });
      await invoke("extension_set_enabled", { id: card.id, enabled: true });
      await refresh();
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      const nextConflict = parseSlotConflict(message);
      if (nextConflict) setContestedEnable({ card, conflict: nextConflict });
      else setError(message);
      await refresh();
    } finally {
      if (aliveRef.current) setBusy(null);
    }
  }, [contestedEnable, refresh]);

  const occupantName = useCallback(
    (id: string) =>
      id === "grain.core"
        ? "Grain's built-in default"
        : (cards.find((card) => card.id === id)?.name ?? id),
    [cards],
  );

  const closeDialog = useCallback(() => setActiveId(null), []);
  const openConfigure = useCallback(
    async (extension: AnchoredExtension) => {
      setActiveId(extension.id);
      if (resolvedRows[extension.id] || !extension.card.has_detail) {
        return;
      }

      setConfiguring(extension.id);
      setError(null);
      try {
        const rows = await invoke<SettingRow[]>("extension_settings_schema", {
          id: extension.id,
        });
        if (!aliveRef.current) return;
        setResolvedRows((current) => ({
          ...current,
          [extension.id]: rows,
        }));
      } catch (reason) {
        if (aliveRef.current) setError(String(reason));
      } finally {
        if (aliveRef.current) setConfiguring(null);
      }
    },
    [resolvedRows],
  );
  const activeBase = anchored.find((extension) => extension.id === activeId);
  const active = activeBase
    ? {
        ...activeBase,
        rows: resolvedRows[activeBase.id] ?? activeBase.rows,
      }
    : undefined;
  const showMore = studioShelfMode(anchored.length) === "installed-with-more";
  const installedDescription = `Focused additions for ${ANCHOR_LABELS[anchor]}.`;

  if (loading || anchored.length === 0) return null;

  return (
    <section
      className="studio-installed-extensions"
      data-collapsed={collapsed || undefined}
    >
      <div className="studio-extension-section-heading">
        <div>
          <span className="studio-extension-heading-line">
            <h2 id={shelfHeadingId}>{INSTALLED_EXTENSIONS_LABEL}</h2>
            <span className="studio-extension-count">{anchored.length}</span>
          </span>
          <p>{installedDescription}</p>
        </div>
        <button
          className="studio-extension-disclosure"
          type="button"
          aria-expanded={!collapsed}
          aria-controls={shelfPanelId}
          aria-label={
            collapsed
              ? "Expand installed extensions"
              : "Collapse installed extensions"
          }
          title={
            collapsed
              ? "Expand installed extensions"
              : "Collapse installed extensions"
          }
          onClick={() => setCollapsed((current) => !current)}
        >
          <ChevronDown size={17} aria-hidden="true" />
        </button>
      </div>
      <div
        id={shelfPanelId}
        className="studio-extension-panel"
        hidden={collapsed}
      >
        {error && <div className="tool-inline-error">{error}</div>}
        <div
          className="studio-extension-grid"
          data-card-count={anchored.length + (showMore ? 1 : 0)}
        >
          {anchored.map((extension) => {
            const card = extension.card;
            const trust = trustPresentation(card.trust);
            const state = card.enabled ? "On" : "Off";
            return (
              <StudioExtensionCard
                key={extension.id}
                name={card.name}
                description={
                  card.description ||
                  `Adds focused controls to ${ANCHOR_LABELS[anchor]}.`
                }
                meta={`${extension.storeEntry?.author || `Version ${card.version}`} · ${state}`}
                badge={trust.label}
                badgeTone={trust.tone}
                surface={anchor}
                primaryLabel={
                  !card.enabled
                    ? busy === extension.id
                      ? "Enabling…"
                      : "Enable"
                    : configuring === extension.id
                      ? "Opening…"
                      : "Configure"
                }
                primaryVariant={card.enabled ? "button" : "enable-toggle"}
                primaryDisabled={
                  busy === extension.id || configuring === extension.id
                }
                onPrimary={() =>
                  void (card.enabled
                    ? openConfigure(extension)
                    : enable(extension))
                }
                onRemove={
                  card.trust !== "dev"
                    ? () => void uninstall(extension)
                    : undefined
                }
              />
            );
          })}
          {showMore && (
            <StudioExtensionMoreCard
              title="Get more extensions"
              description={`Browse additions made for ${ANCHOR_LABELS[anchor]}.`}
              surface={anchor}
              onClick={() => openAnchorStore(anchor)}
            />
          )}
        </div>
      </div>
      {active && (
        <ExtensionConfigureDialog
          extension={active}
          anchor={anchor}
          loading={configuring === active.id}
          onClose={closeDialog}
        />
      )}
      <ExtensionEnableDialogs
        pending={pendingEnable}
        contested={contestedEnable}
        occupantName={occupantName}
        onCancelApproval={() => setPendingEnable(null)}
        onCancelConflict={() => setContestedEnable(null)}
        onApprove={() => void approveEnable()}
        onTakeOver={() => void takeOverSlot()}
      />
    </section>
  );
};
