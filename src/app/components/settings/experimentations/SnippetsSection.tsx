import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, Trash2, Replace } from "lucide-react";
import { toast } from "sonner";
import type { Snippet } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Textarea } from "../../ui/Textarea";
import { CountChip } from "../../ui/CountChip";
import { FieldLabel, TriggerChip, MapArrow } from "./ui";

const MAX_TRIGGER_LENGTH = 100;

/** Starter snippets that prefill the form (never auto-saved) so the tab has
 * something to act on immediately instead of an empty box. */
const EXAMPLES: { trigger: string; replacement: string }[] = [
  { trigger: "my email", replacement: "you@example.com" },
  {
    trigger: "meeting link",
    replacement: "https://meet.google.com/xxx-xxxx-xxx",
  },
  {
    trigger: "sign off",
    replacement: "Thanks,\nAlex",
  },
];

/** Mirror of the backend matcher's normalization (lowercase, alphanumeric
 * only) — used to flag duplicate triggers that would collide at match time
 * even when they differ in case or punctuation. */
const normalizeTrigger = (trigger: string): string =>
  trigger.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");

/** [GRAIN] Voice snippets: trigger phrase → verbatim expansion. Extracted from
 * the old ExperimentationsSettings so each Extensions feature lives in its own
 * isolated sub-tab. */
export const SnippetsSection: React.FC<{
  /** The feature's own switch above already names it (see [`FeaturePanel`]);
   * repeating "Snippets" as a group title over the editor prints it twice. */
  untitled?: boolean;
}> = ({ untitled }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const snippets = getSetting("snippets") || [];
  const updating = isUpdating("snippets");

  const [trigger, setTrigger] = useState("");
  const [replacement, setReplacement] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);

  const trimmedTrigger = trigger.trim();
  const canSubmit =
    trimmedTrigger.length > 0 &&
    trimmedTrigger.length <= MAX_TRIGGER_LENGTH &&
    replacement.length > 0 &&
    !updating;

  const resetForm = () => {
    setTrigger("");
    setReplacement("");
    setEditingId(null);
  };

  const handleSubmit = () => {
    if (!canSubmit) return;

    const normalized = normalizeTrigger(trimmedTrigger);
    const collision = snippets.find(
      (s) => s.id !== editingId && normalizeTrigger(s.trigger) === normalized,
    );
    if (collision) {
      toast.error(
        t("settings.experimentations.snippets.duplicate", {
          trigger: collision.trigger,
        }),
      );
      return;
    }

    const next: Snippet[] = editingId
      ? snippets.map((s) =>
          s.id === editingId
            ? { ...s, trigger: trimmedTrigger, replacement }
            : s,
        )
      : [
          ...snippets,
          {
            id: crypto.randomUUID(),
            trigger: trimmedTrigger,
            replacement,
            enabled: true,
          },
        ];
    updateSetting("snippets", next);
    resetForm();
  };

  const handleEdit = (snippet: Snippet) => {
    setEditingId(snippet.id);
    setTrigger(snippet.trigger);
    setReplacement(snippet.replacement);
  };

  const handleDelete = (id: string) => {
    if (editingId === id) resetForm();
    updateSetting(
      "snippets",
      snippets.filter((s) => s.id !== id),
    );
  };

  const handleToggle = (id: string) => {
    updateSetting(
      "snippets",
      snippets.map((s) =>
        s.id === id ? { ...s, enabled: !(s.enabled ?? true) } : s,
      ),
    );
  };

  return (
    <SettingsGroup
      title={untitled ? undefined : t("settings.experimentations.snippets.title")}
      info={untitled ? undefined : t("settings.experimentations.snippets.description")}
      trailing={
        untitled || snippets.length === 0 ? null : <CountChip n={snippets.length} />
      }
    >
      {/* Composer. Stacked, not side-by-side: the two fields are a MAPPING, and
          setting them across from each other left both cramped while saying
          nothing about the relationship. Down the page with the arrow between
          them, the form reads as the sentence it is — say this, get that. */}
      <div className="p-5 space-y-4">
        <div className="space-y-2">
          <FieldLabel htmlFor="snippet-trigger">When I say</FieldLabel>
          <Input
            id="snippet-trigger"
            type="text"
            className="w-full"
            value={trigger}
            onChange={(e) => setTrigger(e.target.value)}
            maxLength={MAX_TRIGGER_LENGTH}
            placeholder={t(
              "settings.experimentations.snippets.triggerPlaceholder",
            )}
            disabled={updating}
          />
        </div>

        <div className="flex items-center gap-2 ps-1 text-ink-faint">
          <MapArrow className="rotate-90" />
          <span className="h-px flex-1 bg-line" />
        </div>

        <div className="space-y-2">
          <FieldLabel htmlFor="snippet-expansion">Grain types</FieldLabel>
          <Textarea
            id="snippet-expansion"
            className="w-full"
            autoResize
            maxRows={5}
            value={replacement}
            onChange={(e) => setReplacement(e.target.value)}
            placeholder={t(
              "settings.experimentations.snippets.replacementPlaceholder",
            )}
            disabled={updating}
          />
        </div>

        {/* Starter examples share the action row: they were a line of their
            own above the button, which put a band of dead space across the
            widest part of the form to hold three small chips. */}
        <div className="flex items-center justify-between gap-3 pt-1">
          {!editingId &&
          trimmedTrigger.length === 0 &&
          replacement.length === 0 ? (
            <div className="flex flex-wrap items-center gap-2 min-w-0">
              <span className="text-xs text-ink-soft shrink-0">Start from</span>
              {EXAMPLES.map((ex) => (
                <button
                  key={ex.trigger}
                  type="button"
                  onClick={() => {
                    setTrigger(ex.trigger);
                    setReplacement(ex.replacement);
                  }}
                  className="px-2.5 py-1 rounded-md text-xs font-mono text-ink-soft bg-paper-sunken border border-line hover:text-ink hover:border-ink-faint transition-colors cursor-pointer"
                >
                  {ex.trigger}
                </button>
              ))}
            </div>
          ) : (
            <span />
          )}

          <div className="flex items-center gap-2 shrink-0">
            {editingId && (
              <Button onClick={resetForm} variant="secondary" size="md">
                {t("settings.experimentations.snippets.cancel")}
              </Button>
            )}
            <Button
              onClick={handleSubmit}
              disabled={!canSubmit}
              variant="primary"
              size="md"
            >
              {editingId
                ? t("settings.experimentations.snippets.update")
                : t("settings.experimentations.snippets.add")}
            </Button>
          </div>
        </div>
      </div>

      {/* Saved snippets — scrolls internally so a long list never grows the page. */}
      {snippets.length === 0 ? (
        <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
          <Replace width={20} height={20} className="text-ink-faint" />
          <p className="text-sm text-ink-faint">
            {t("settings.experimentations.snippets.empty")}
          </p>
        </div>
      ) : (
        <div className="max-h-[360px] overflow-y-auto divide-y divide-line">
          {snippets.map((snippet) => {
            const enabled = snippet.enabled ?? true;
            const editing = editingId === snippet.id;
            return (
              <div
                key={snippet.id}
                className={`group px-5 py-3.5 flex items-center gap-3 transition-colors ${
                  editing
                    ? "bg-[var(--accent-tint)]"
                    : "hover:bg-[rgba(20,19,18,0.02)]"
                } ${enabled ? "" : "opacity-45"}`}
              >
                {/* The trigger is what you SAY and the expansion is what you
                    get; the payload was set in faint grey, which is the colour
                    the eye skips. */}
                <div className="flex-1 min-w-0 flex items-center gap-2.5">
                  <TriggerChip>{snippet.trigger}</TriggerChip>
                  <MapArrow />
                  <span
                    className="min-w-0 truncate text-sm text-ink"
                    title={snippet.replacement}
                  >
                    {snippet.replacement}
                  </span>
                </div>
                <label
                  className={`inline-flex items-center shrink-0 transition-transform duration-100 active:scale-90 ${
                    updating
                      ? "cursor-not-allowed active:scale-100"
                      : "cursor-pointer"
                  }`}
                  title={t("settings.experimentations.snippets.toggle")}
                >
                  <input
                    type="checkbox"
                    className="sr-only peer"
                    checked={enabled}
                    disabled={updating}
                    onChange={() => handleToggle(snippet.id)}
                  />
                  {/* Same hardware toggle as ToggleSwitch, without the row chrome. */}
                  <div
                    className="relative w-8 h-[18px] rounded-full transition-colors duration-200 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-[var(--focus)] peer-disabled:opacity-50 after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:h-[14px] after:w-[14px] after:rounded-full after:shadow-[0_1px_3px_rgba(0,0,0,0.35)] after:transition-all after:duration-200 peer-checked:after:translate-x-[14px] peer-checked:after:bg-[var(--color-accent)] after:bg-[#f0ebe3] rtl:peer-checked:after:-translate-x-[14px]"
                    style={{ backgroundColor: "#0a0a0a" }}
                  ></div>
                </label>
                <div className="flex items-center gap-0.5 shrink-0 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity">
                  <button
                    type="button"
                    onClick={() => handleEdit(snippet)}
                    disabled={updating}
                    className="p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer disabled:cursor-not-allowed text-ink-soft hover:text-accent"
                    title={t("settings.experimentations.snippets.edit")}
                  >
                    <Pencil width={15} height={15} />
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDelete(snippet.id)}
                    disabled={updating}
                    className="p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer disabled:cursor-not-allowed text-ink-soft hover:text-accent"
                    title={t("settings.experimentations.snippets.delete")}
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
  );
};
