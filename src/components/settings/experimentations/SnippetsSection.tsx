import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, Trash2 } from "lucide-react";
import { toast } from "sonner";
import type { Snippet } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Textarea } from "../../ui/Textarea";

const MAX_TRIGGER_LENGTH = 100;

/** Mirror of the backend matcher's normalization (lowercase, alphanumeric
 * only) — used to flag duplicate triggers that would collide at match time
 * even when they differ in case or punctuation. */
const normalizeTrigger = (trigger: string): string =>
  trigger.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");

/** [GRAIN] Voice snippets: trigger phrase → verbatim expansion. Extracted from
 * the old ExperimentationsSettings so each Experimentations feature lives in its
 * own isolated sub-tab. */
export const SnippetsSection: React.FC = () => {
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
      title={t("settings.experimentations.snippets.title")}
      description={t("settings.experimentations.snippets.description")}
    >
      {/* Add / edit form */}
      <div className="px-4 py-3 space-y-2">
        <Input
          type="text"
          className="w-full"
          variant="compact"
          value={trigger}
          onChange={(e) => setTrigger(e.target.value)}
          maxLength={MAX_TRIGGER_LENGTH}
          placeholder={t(
            "settings.experimentations.snippets.triggerPlaceholder",
          )}
          disabled={updating}
        />
        <Textarea
          className="w-full"
          variant="compact"
          value={replacement}
          onChange={(e) => setReplacement(e.target.value)}
          placeholder={t(
            "settings.experimentations.snippets.replacementPlaceholder",
          )}
          disabled={updating}
        />
        <div className="flex items-center justify-end gap-2">
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

      {/* Saved snippets */}
      {snippets.length === 0 ? (
        <div className="px-4 py-3 text-center text-sm text-ink-soft">
          {t("settings.experimentations.snippets.empty")}
        </div>
      ) : (
        snippets.map((snippet) => {
          const enabled = snippet.enabled ?? true;
          return (
            <div
              key={snippet.id}
              className={`px-4 py-2.5 flex items-center gap-3 ${
                enabled ? "" : "opacity-50"
              }`}
            >
              <div className="flex-1 min-w-0">
                <p className="font-mono text-sm font-medium text-ink truncate">
                  {snippet.trigger}
                </p>
                <p className="text-xs text-ink-soft truncate whitespace-pre">
                  {snippet.replacement}
                </p>
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
          );
        })
      )}
    </SettingsGroup>
  );
};
