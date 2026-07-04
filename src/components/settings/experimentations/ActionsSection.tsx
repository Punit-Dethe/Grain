import React, { useState } from "react";
import {
  AppWindow,
  Globe,
  Plus,
  X,
  Play,
  Pencil,
  Trash2,
  FolderOpen,
} from "lucide-react";
import { toast } from "sonner";
import type { ActionTarget, VoiceAction } from "@/bindings";
import { commands } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";

const MAX_TRIGGER_LENGTH = 100;

/** Mirror of the backend matcher's normalization (lowercase, alphanumeric only)
 * — used to flag duplicate triggers that would collide at match time even when
 * they differ in case or punctuation. */
const normalizeTrigger = (trigger: string): string =>
  trigger.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");

const emptyForm = {
  editingId: null as string | null,
  trigger: "",
  targets: [{ kind: "app", value: "" }] as ActionTarget[],
};

/** [GRAIN] Voice actions: a spoken trigger opens one or more apps/websites. The
 * local sibling of snippets — instead of expanding to text, a matched trigger
 * fires side effects (launch apps, open URLs) and is stripped from the paste.
 * One action can bundle several targets, so "start coding" opens a whole
 * workflow at once. No AI, no network — a plain OS launch. */
export const ActionsSection: React.FC = () => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const actions = getSetting("actions") || [];
  const updating = isUpdating("actions");

  const [form, setForm] = useState(emptyForm);

  const trimmedTrigger = form.trigger.trim();
  const validTargets = form.targets.filter((t) => t.value.trim().length > 0);
  const canSubmit =
    trimmedTrigger.length > 0 &&
    trimmedTrigger.length <= MAX_TRIGGER_LENGTH &&
    validTargets.length > 0 &&
    !updating;

  const resetForm = () => setForm(emptyForm);

  const setTarget = (i: number, patch: Partial<ActionTarget>) =>
    setForm((f) => ({
      ...f,
      targets: f.targets.map((t, idx) =>
        idx === i ? ({ ...t, ...patch } as ActionTarget) : t,
      ),
    }));

  const addTarget = () =>
    setForm((f) => ({
      ...f,
      targets: [...f.targets, { kind: "app", value: "" }],
    }));

  const removeTarget = (i: number) =>
    setForm((f) => ({
      ...f,
      // Always keep at least one row so the form never collapses to nothing.
      targets:
        f.targets.length > 1
          ? f.targets.filter((_, idx) => idx !== i)
          : [{ kind: "app", value: "" }],
    }));

  const browseForApp = async (i: number) => {
    const res = await commands.pickActionApp();
    if (res.status === "ok" && res.data) {
      setTarget(i, { value: res.data });
    }
  };

  const testTargets = async (targets: ActionTarget[]) => {
    const clean = targets.filter((t) => t.value.trim().length > 0);
    if (clean.length === 0) {
      toast.error("Add a target to test first.");
      return;
    }
    await commands.runAction(clean);
  };

  const handleSubmit = () => {
    if (!canSubmit) return;

    const normalized = normalizeTrigger(trimmedTrigger);
    const collision = actions.find(
      (a) =>
        a.id !== form.editingId && normalizeTrigger(a.trigger) === normalized,
    );
    if (collision) {
      toast.error(`"${collision.trigger}" already uses that trigger.`);
      return;
    }

    const cleanTargets: ActionTarget[] = validTargets.map((t) => ({
      kind: t.kind,
      value: t.value.trim(),
    }));

    const next: VoiceAction[] = form.editingId
      ? actions.map((a) =>
          a.id === form.editingId
            ? { ...a, trigger: trimmedTrigger, targets: cleanTargets }
            : a,
        )
      : [
          ...actions,
          {
            id: crypto.randomUUID(),
            trigger: trimmedTrigger,
            targets: cleanTargets,
            enabled: true,
          },
        ];
    updateSetting("actions", next);
    resetForm();
  };

  const handleEdit = (action: VoiceAction) => {
    setForm({
      editingId: action.id,
      trigger: action.trigger,
      targets:
        action.targets.length > 0
          ? action.targets.map((t) => ({ ...t }))
          : [{ kind: "app", value: "" }],
    });
  };

  const handleDelete = (id: string) => {
    if (form.editingId === id) resetForm();
    updateSetting(
      "actions",
      actions.filter((a) => a.id !== id),
    );
  };

  const handleToggle = (id: string) => {
    updateSetting(
      "actions",
      actions.map((a) =>
        a.id === id ? { ...a, enabled: !(a.enabled ?? true) } : a,
      ),
    );
  };

  return (
    <SettingsGroup
      title="Actions"
      description="Say a phrase to open apps and websites. One action can open several at once — say “start coding” to launch your editor and open two docs in your browser. Fully local; the trigger phrase is removed from what gets typed."
    >
      {/* Add / edit form */}
      <div className="px-4 py-3 space-y-3">
        <Input
          type="text"
          className="w-full"
          variant="compact"
          value={form.trigger}
          onChange={(e) => setForm((f) => ({ ...f, trigger: e.target.value }))}
          maxLength={MAX_TRIGGER_LENGTH}
          placeholder="Trigger phrase, e.g. start coding, open email"
          disabled={updating}
        />

        {/* Targets */}
        <div className="space-y-2">
          {form.targets.map((target, i) => (
            <div key={i} className="flex items-center gap-2">
              {/* App / Website toggle */}
              <div className="flex items-center shrink-0 rounded-md overflow-hidden border border-[color-mix(in_srgb,var(--color-mid-gray)_40%,transparent)]">
                <button
                  type="button"
                  onClick={() => setTarget(i, { kind: "app" })}
                  title="Application, file, or folder"
                  className={`flex items-center gap-1 px-2.5 py-1.5 text-sm transition-colors cursor-pointer ${
                    target.kind === "app"
                      ? "bg-[var(--color-accent)] text-black"
                      : "text-ink-soft hover:text-ink"
                  }`}
                >
                  <AppWindow width={14} height={14} /> App
                </button>
                <button
                  type="button"
                  onClick={() => setTarget(i, { kind: "url" })}
                  title="Website (opens in your default browser)"
                  className={`flex items-center gap-1 px-2.5 py-1.5 text-sm transition-colors cursor-pointer ${
                    target.kind === "url"
                      ? "bg-[var(--color-accent)] text-black"
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
                value={target.value}
                onChange={(e) => setTarget(i, { value: e.target.value })}
                placeholder={
                  target.kind === "app"
                    ? "App path, e.g. C:\\Program Files\\…\\Code.exe"
                    : "Website, e.g. github.com, mail.google.com"
                }
                disabled={updating}
              />

              {target.kind === "app" && (
                <button
                  type="button"
                  onClick={() => browseForApp(i)}
                  disabled={updating}
                  title="Browse for an application"
                  className="shrink-0 p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer text-ink-soft hover:text-accent"
                >
                  <FolderOpen width={16} height={16} />
                </button>
              )}
              <button
                type="button"
                onClick={() => removeTarget(i)}
                disabled={updating}
                title="Remove target"
                className="shrink-0 p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer text-ink-soft hover:text-accent"
              >
                <X width={16} height={16} />
              </button>
            </div>
          ))}

          <button
            type="button"
            onClick={addTarget}
            disabled={updating}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs border border-[color-mix(in_srgb,var(--color-mid-gray)_40%,transparent)] text-ink-soft hover:text-ink hover:border-[var(--color-accent)] transition-colors cursor-pointer"
          >
            <Plus width={14} height={14} /> Add target
          </button>
        </div>

        <div className="flex items-center justify-end gap-2">
          <Button
            onClick={() => testTargets(form.targets)}
            variant="secondary"
            size="md"
            disabled={validTargets.length === 0}
          >
            <Play width={14} height={14} className="mr-1.5" />
            Test
          </Button>
          {form.editingId && (
            <Button onClick={resetForm} variant="secondary" size="md">
              Cancel
            </Button>
          )}
          <Button
            onClick={handleSubmit}
            disabled={!canSubmit}
            variant="primary"
            size="md"
          >
            {form.editingId ? "Update action" : "Add action"}
          </Button>
        </div>
      </div>

      {/* Saved actions */}
      {actions.length === 0 ? (
        <div className="px-4 py-3 text-center text-sm text-ink-soft">
          No actions yet. Add one above to open apps or sites with your voice.
        </div>
      ) : (
        actions.map((action) => {
          const enabled = action.enabled ?? true;
          return (
            <div
              key={action.id}
              className={`px-4 py-2.5 flex items-center gap-3 ${
                enabled ? "" : "opacity-50"
              }`}
            >
              <div className="flex-1 min-w-0">
                <p className="font-mono text-sm font-medium text-ink truncate">
                  {action.trigger}
                </p>
                <p className="text-xs text-ink-soft truncate flex items-center gap-2">
                  {action.targets.map((t, i) => (
                    <span key={i} className="inline-flex items-center gap-1">
                      {t.kind === "app" ? (
                        <AppWindow width={12} height={12} />
                      ) : (
                        <Globe width={12} height={12} />
                      )}
                      {t.value}
                    </span>
                  ))}
                </p>
              </div>
              <button
                type="button"
                onClick={() => testTargets(action.targets)}
                title="Run now"
                className="p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer text-ink-soft hover:text-accent"
              >
                <Play width={15} height={15} />
              </button>
              <label
                className={`inline-flex items-center shrink-0 transition-transform duration-100 active:scale-90 ${
                  updating
                    ? "cursor-not-allowed active:scale-100"
                    : "cursor-pointer"
                }`}
                title="Enable / disable action"
              >
                <input
                  type="checkbox"
                  className="sr-only peer"
                  checked={enabled}
                  disabled={updating}
                  onChange={() => handleToggle(action.id)}
                />
                <div
                  className="relative w-8 h-[18px] rounded-full transition-colors duration-200 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-[var(--focus)] peer-disabled:opacity-50 after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:h-[14px] after:w-[14px] after:rounded-full after:shadow-[0_1px_3px_rgba(0,0,0,0.35)] after:transition-all after:duration-200 peer-checked:after:translate-x-[14px] peer-checked:after:bg-[var(--color-accent)] after:bg-[#f0ebe3] rtl:peer-checked:after:-translate-x-[14px]"
                  style={{ backgroundColor: "#0a0a0a" }}
                ></div>
              </label>
              <button
                type="button"
                onClick={() => handleEdit(action)}
                disabled={updating}
                className="p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer disabled:cursor-not-allowed text-ink-soft hover:text-accent"
                title="Edit action"
              >
                <Pencil width={15} height={15} />
              </button>
              <button
                type="button"
                onClick={() => handleDelete(action.id)}
                disabled={updating}
                className="p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer disabled:cursor-not-allowed text-ink-soft hover:text-accent"
                title="Delete action"
              >
                <Trash2 width={15} height={15} />
              </button>
            </div>
          );
        })
      )}
    </SettingsGroup>
  );
};
