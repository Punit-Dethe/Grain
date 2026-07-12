import React, { useEffect, useRef, useState } from "react";
import { AppWindow, Globe, Crosshair, Pencil, Trash2 } from "lucide-react";
import { toast } from "sonner";
import type { AppMatch, AppMode } from "@/bindings";
import { commands } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Textarea } from "../../ui/Textarea";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { CountChip } from "../../ui/CountChip";
import { FieldLabel } from "./ui";

type MatchKind = AppMatch["kind"]; // "process" | "url_host"

/** Quick-start prompts a user can drop into a mode with one click. Kept short so
 * the LLM stays cheap; the user is free to edit after applying. */
const TEMPLATES: { key: string; name: string; prompt: string }[] = [
  {
    key: "email",
    name: "Email",
    prompt:
      "Rewrite this dictated transcript as a clear, professional email body. Fix grammar, capitalization, and punctuation, and organize it into natural paragraphs with a polite, professional tone. Do NOT invent a greeting, sign-off, or subject line, and do not add anything that was not said. Keep the original language. Return only the email body.",
  },
  {
    key: "coding",
    name: "Coding",
    prompt:
      'Format this dictated transcript as a concise technical message. Correct spoken programming terms to their real form (e.g. "snake case" → snake_case, "use effect" → useEffect), wrap identifiers, symbols, and code in backticks, and keep it terse. Do not implement or explain anything that was not asked. Keep the original language. Return only the cleaned text.',
  },
  {
    key: "x",
    name: "X post",
    prompt:
      "Rewrite this dictated transcript as a single social post in the user's own voice. Keep it under 280 characters, casual and punchy. Do not add hashtags or emoji unless they were dictated. Return only the post text.",
  },
  {
    key: "chat",
    name: "Chat",
    prompt:
      "Lightly clean this dictated transcript for a casual chat message. Fix obvious spelling and punctuation errors but keep the user's casual tone, slang, and phrasing — do not formalize or restructure. Keep the original language. Return only the message.",
  },
];

const emptyForm = {
  editingId: null as string | null,
  name: "",
  matchKind: "process" as MatchKind,
  matchValue: "",
  prompt: "",
};

/** [GRAIN] Context awareness: the automatic SOFT tone/vocab layer (a global
 * toggle) plus user-defined MODES (HARD formatting bound to an app or website).
 * Everything here writes plain AppSettings fields via `useSettings`; the backend
 * composes the three-stage prompt at post-processing time. */
export const ContextAwareSection: React.FC = () => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("context_awareness_enabled") ?? false;
  const nearbyTerms = getSetting("context_nearby_terms") ?? false;
  const modes = getSetting("app_modes") ?? [];
  const savingModes = isUpdating("app_modes");

  const [form, setForm] = useState(emptyForm);
  const [countdown, setCountdown] = useState<number | null>(null);
  const timers = useRef<ReturnType<typeof setTimeout>[]>([]);

  // Clear any pending capture timers on unmount so we never touch a dead component.
  useEffect(
    () => () => {
      timers.current.forEach(clearTimeout);
    },
    [],
  );

  const resetForm = () => setForm(emptyForm);

  const matcherOf = (kind: MatchKind, value: string): AppMatch =>
    kind === "process"
      ? { kind: "process", value }
      : { kind: "url_host", value };

  const trimmedValue = form.matchValue.trim();
  const canSave =
    form.name.trim().length > 0 &&
    form.prompt.trim().length > 0 &&
    trimmedValue.length > 0 &&
    !savingModes;

  const handleSave = () => {
    if (!canSave) return;
    // Normalize the target the same way the backend matches it.
    const value =
      form.matchKind === "process"
        ? trimmedValue.replace(/\.exe$/i, "").toLowerCase()
        : trimmedValue
            .replace(/^https?:\/\//i, "")
            .replace(/\/.*$/, "")
            .replace(/^www\./i, "")
            .toLowerCase();

    const next: AppMode[] = form.editingId
      ? modes.map((m) =>
          m.id === form.editingId
            ? {
                ...m,
                name: form.name.trim(),
                match: matcherOf(form.matchKind, value),
                prompt: form.prompt.trim(),
              }
            : m,
        )
      : [
          ...modes,
          {
            id: crypto.randomUUID(),
            name: form.name.trim(),
            match: matcherOf(form.matchKind, value),
            prompt: form.prompt.trim(),
            enabled: true,
          },
        ];
    updateSetting("app_modes", next);
    resetForm();
  };

  const handleEdit = (mode: AppMode) => {
    setForm({
      editingId: mode.id,
      name: mode.name,
      matchKind: mode.match.kind,
      matchValue: mode.match.value,
      prompt: mode.prompt,
    });
  };

  const handleDelete = (id: string) => {
    if (form.editingId === id) resetForm();
    updateSetting(
      "app_modes",
      modes.filter((m) => m.id !== id),
    );
  };

  const handleToggleMode = (id: string) => {
    updateSetting(
      "app_modes",
      modes.map((m) =>
        m.id === id ? { ...m, enabled: !(m.enabled ?? true) } : m,
      ),
    );
  };

  const applyTemplate = (tpl: (typeof TEMPLATES)[number]) => {
    setForm((f) => ({
      ...f,
      // Only fill the name if the user hasn't typed one yet.
      name: f.name.trim().length > 0 ? f.name : tpl.name,
      prompt: tpl.prompt,
    }));
  };

  // "Capture focused app": a short countdown gives the user time to switch to
  // their target app (this Settings window is focused right now), then we read
  // the foreground app on the backend and pre-fill the matcher.
  const startCapture = () => {
    timers.current.forEach(clearTimeout);
    timers.current = [];
    let n = 3;
    setCountdown(n);
    for (let i = 1; i <= 3; i++) {
      timers.current.push(
        setTimeout(() => {
          n -= 1;
          setCountdown(n > 0 ? n : null);
        }, i * 1000),
      );
    }
    timers.current.push(
      setTimeout(async () => {
        setCountdown(null);
        const app = await commands.detectActiveApp();
        if (!app) {
          toast.error("Couldn't detect the focused app. Try again.");
          return;
        }
        // If a browser URL was resolved, prefer a website matcher.
        if (app.url_host) {
          setForm((f) => ({
            ...f,
            matchKind: "url_host",
            matchValue: app.url_host!,
            name: f.name.trim().length > 0 ? f.name : app.url_host!,
          }));
        } else {
          setForm((f) => ({
            ...f,
            matchKind: "process",
            matchValue: app.exe,
            name: f.name.trim().length > 0 ? f.name : app.name || app.exe,
          }));
        }
      }, 3000),
    );
  };

  return (
    <div className="space-y-6">
      {/* Master toggle + soft-context explanation. */}
      <SettingsGroup
        title="Context awareness"
        info="Adapts post-processing to the app you're dictating into. It softly nudges tone and vocabulary (an IDE keeps technical terms; chat stays casual; email gets slightly more polished) without hard-reformatting. Applied on top of your selected post-processing prompt. Requires post-processing to be on."
      >
        <ToggleSwitch
          label="Enable context awareness"
          description="Detect the foreground app and layer soft context (and any matching mode below) onto post-processing."
          descriptionMode="tooltip"
          grouped
          checked={enabled}
          isUpdating={isUpdating("context_awareness_enabled")}
          onChange={(v) => updateSetting("context_awareness_enabled", v)}
        />
        <ToggleSwitch
          label="Nearby-term hints (silent)"
          description="Read UNIQUE names and identifiers (e.g. Rita, useGrainStore, PyTorch) from the field you're dictating into and pass them as a spelling hint only. Never sends raw text, never stored, password fields skipped. Improves accuracy on names and jargon."
          descriptionMode="tooltip"
          grouped
          disabled={!enabled}
          checked={nearbyTerms}
          isUpdating={isUpdating("context_nearby_terms")}
          onChange={(v) => updateSetting("context_nearby_terms", v)}
        />
      </SettingsGroup>

      {/* Modes: user-defined hard formatting bound to an app/site. */}
      <SettingsGroup
        title="Modes"
        info="Your own instructions applied only inside a specific app or website (e.g. an 'X post' mode on x.com). This is where hard formatting lives — you define it. Modes only run while context awareness is on."
        trailing={modes.length > 0 ? <CountChip n={modes.length} /> : null}
      >
        {/* Composer */}
        <div className={`p-4 space-y-3.5 ${enabled ? "" : "opacity-60"}`}>
          {/* Where the mode applies: kind + matcher + capture. */}
          <div className="space-y-1.5">
            <FieldLabel>Applies in</FieldLabel>
            <div className="flex items-center gap-2">
              <div className="flex items-center shrink-0 rounded-md overflow-hidden border border-line">
                <button
                  type="button"
                  onClick={() =>
                    setForm((f) => ({ ...f, matchKind: "process" }))
                  }
                  title="A desktop application"
                  className={`flex items-center gap-1 px-2.5 py-1.5 text-sm transition-colors cursor-pointer ${
                    form.matchKind === "process"
                      ? "bg-accent text-black"
                      : "text-ink-soft hover:text-ink"
                  }`}
                >
                  <AppWindow width={14} height={14} /> App
                </button>
                <button
                  type="button"
                  onClick={() =>
                    setForm((f) => ({ ...f, matchKind: "url_host" }))
                  }
                  title="A website, by host"
                  className={`flex items-center gap-1 px-2.5 py-1.5 text-sm transition-colors cursor-pointer ${
                    form.matchKind === "url_host"
                      ? "bg-accent text-black"
                      : "text-ink-soft hover:text-ink"
                  }`}
                >
                  <Globe width={14} height={14} /> Web
                </button>
              </div>
              <Input
                type="text"
                className="w-full"
                variant="compact"
                value={form.matchValue}
                onChange={(e) =>
                  setForm((f) => ({ ...f, matchValue: e.target.value }))
                }
                placeholder={
                  form.matchKind === "process"
                    ? "App executable, e.g. Code, slack, cursor"
                    : "Website host, e.g. x.com, mail.google.com"
                }
              />
              {form.matchKind === "process" && (
                <Button
                  onClick={startCapture}
                  variant="secondary"
                  size="md"
                  disabled={countdown !== null}
                  className="shrink-0 whitespace-nowrap"
                >
                  <Crosshair width={15} height={15} className="mr-1.5" />
                  {countdown !== null
                    ? `Switch to your app… ${countdown}`
                    : "Capture"}
                </Button>
              )}
            </div>
          </div>

          {/* Mode name. */}
          <div className="space-y-1.5">
            <FieldLabel htmlFor="mode-name">Mode name</FieldLabel>
            <Input
              id="mode-name"
              type="text"
              className="w-full"
              variant="compact"
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
              placeholder="e.g. X post, Work email"
            />
          </div>

          {/* Instructions, with quick-start templates on the right. */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between gap-3">
              <FieldLabel htmlFor="mode-prompt">Instructions</FieldLabel>
              <div className="flex flex-wrap items-center justify-end gap-1.5">
                {TEMPLATES.map((tpl) => (
                  <button
                    key={tpl.key}
                    type="button"
                    onClick={() => applyTemplate(tpl)}
                    className="px-2 py-0.5 rounded-md text-xs border border-line text-ink-soft hover:text-ink hover:border-accent/50 transition-colors cursor-pointer"
                  >
                    {tpl.name}
                  </button>
                ))}
              </div>
            </div>
            <Textarea
              id="mode-prompt"
              className="w-full"
              variant="compact"
              rows={4}
              value={form.prompt}
              onChange={(e) =>
                setForm((f) => ({ ...f, prompt: e.target.value }))
              }
              placeholder="Instructions for this app/site. e.g. 'Rewrite as a tweet under 280 characters in my voice.'"
            />
          </div>

          <div className="flex items-center justify-end gap-2">
            {form.editingId && (
              <Button onClick={resetForm} variant="secondary" size="md">
                Cancel
              </Button>
            )}
            <Button
              onClick={handleSave}
              disabled={!canSave}
              variant="primary"
              size="md"
            >
              {form.editingId ? "Update mode" : "Add mode"}
            </Button>
          </div>
        </div>

        {/* Saved modes */}
        {modes.length === 0 ? (
          <div className="px-4 py-6 text-center text-sm text-ink-faint">
            No modes yet. Add one above to apply your own formatting in a
            specific app or website.
          </div>
        ) : (
          <div className="max-h-[360px] overflow-y-auto divide-y divide-line">
            {modes.map((mode) => {
              const on = mode.enabled ?? true;
              const editing = form.editingId === mode.id;
              return (
                <div
                  key={mode.id}
                  className={`group px-4 py-2.5 flex items-center gap-3 transition-colors ${
                    editing
                      ? "bg-[var(--accent-tint)]"
                      : "hover:bg-[rgba(20,19,18,0.02)]"
                  } ${on ? "" : "opacity-45"}`}
                >
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="text-sm font-medium text-ink truncate">
                        {mode.name}
                      </span>
                      <span className="inline-flex items-center gap-1 shrink-0 font-mono text-[0.7rem] text-ink-soft bg-paper-sunken border border-line rounded-md px-1.5 py-[2px]">
                        {mode.match.kind === "process" ? (
                          <AppWindow
                            width={11}
                            height={11}
                            className="text-ink-faint"
                          />
                        ) : (
                          <Globe
                            width={11}
                            height={11}
                            className="text-ink-faint"
                          />
                        )}
                        <span className="truncate max-w-[160px]">
                          {mode.match.value}
                        </span>
                      </span>
                    </div>
                    <p className="mt-0.5 text-xs text-ink-soft truncate">
                      {mode.prompt}
                    </p>
                  </div>
                  <label
                    className="inline-flex items-center shrink-0 transition-transform duration-100 active:scale-90 cursor-pointer"
                    title="Enable / disable mode"
                  >
                    <input
                      type="checkbox"
                      className="sr-only peer"
                      checked={on}
                      onChange={() => handleToggleMode(mode.id)}
                    />
                    <div
                      className="relative w-8 h-[18px] rounded-full transition-colors duration-200 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-[var(--focus)] after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:h-[14px] after:w-[14px] after:rounded-full after:shadow-[0_1px_3px_rgba(0,0,0,0.35)] after:transition-all after:duration-200 peer-checked:after:translate-x-[14px] peer-checked:after:bg-[var(--color-accent)] after:bg-[#f0ebe3] rtl:peer-checked:after:-translate-x-[14px]"
                      style={{ backgroundColor: "#0a0a0a" }}
                    ></div>
                  </label>
                  <div className="flex items-center gap-0.5 shrink-0 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity">
                    <button
                      type="button"
                      onClick={() => handleEdit(mode)}
                      className="p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer text-ink-soft hover:text-accent"
                      title="Edit mode"
                    >
                      <Pencil width={15} height={15} />
                    </button>
                    <button
                      type="button"
                      onClick={() => handleDelete(mode.id)}
                      className="p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer text-ink-soft hover:text-accent"
                      title="Delete mode"
                    >
                      <Trash2 width={15} height={15} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </SettingsGroup>
    </div>
  );
};
