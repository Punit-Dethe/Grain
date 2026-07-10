import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { AlarmClock, ExternalLink, Pin, PinOff, Trash2 } from "lucide-react";
import type { Note, ReminderState } from "@/bindings";
import { commands } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { ShortcutInput } from "../ShortcutInput";

/** `#[serde(default)]` fields come out optional in the generated types; the
 * backend always serializes them, but normalize anyway. */
const reminderOf = (note: Note): ReminderState =>
  note.reminder_state ?? { status: "none", fire_at: null };

/** Backend event fired after any note mutation (save/delete/pin/reminder). */
const NOTES_CHANGED_EVENT = "grain-space://notes-changed";
/** Semantic-model download events (see grain_space/embed.rs). */
const MODEL_PROGRESS_EVENT = "grain-space://embed-model-progress";
const MODEL_COMPLETE_EVENT = "grain-space://embed-model-complete";
const MODEL_ERROR_EVENT = "grain-space://embed-model-error";

type ModelFlow =
  | { state: "consent" }
  | { state: "downloading"; percentage: number }
  | { state: "error"; message: string };

const timeFormat = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
});
const dateFormat = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  day: "numeric",
  month: "short",
  year: "numeric",
});
const fireFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
  hour: "2-digit",
  minute: "2-digit",
});

/** Local-day bucket label: Today / Yesterday / formatted date. */
function dayLabel(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const startOf = (x: Date) =>
    new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round(
    (startOf(today) - startOf(d)) / (24 * 60 * 60 * 1000),
  );
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  return dateFormat.format(d);
}

function noteDisplayTitle(note: Note): string {
  if (note.title.trim()) return note.title;
  const firstLine = note.body.split("\n")[0]?.trim() ?? "";
  return firstLine.length > 60 ? `${firstLine.slice(0, 57)}…` : firstLine;
}

/** [GRAIN] Grain Space tab: master toggle, capture shortcuts, search behavior,
 * reminders (top) and the date-grouped notes list (bottom). The whole feature
 * is create/destroy — this tab is just a window onto the on-disk notes. */
export const GrainSpaceSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("grain_space_enabled") ?? false;
  const semantic = getSetting("grain_space_semantic") ?? false;
  const embedF16 = getSetting("grain_space_embed_f16") ?? false;
  const autoReminders = getSetting("grain_space_auto_reminders") ?? true;
  // [GRAIN] Obsidian vault backend (OBSIDIAN-PLAN.md) — a hard switch.
  const backend = getSetting("grain_space_backend") ?? "grain";
  const vaultPath = getSetting("grain_space_vault_path") ?? "";
  const vaultFolder = getSetting("grain_space_vault_folder") ?? "Grain";

  const [notes, setNotes] = useState<Note[]>([]);
  const [vaultMsg, setVaultMsg] = useState<string | null>(null);
  const [folderDraft, setFolderDraft] = useState<string | null>(null);
  const [exportMsg, setExportMsg] = useState<string | null>(null);
  const [uninstallMsg, setUninstallMsg] = useState<string | null>(null);
  // Consent → download → verify flow for the semantic model. The toggle stays
  // OFF until the model is verified on disk (edge-case rule in the plan).
  const [modelFlow, setModelFlow] = useState<ModelFlow | null>(null);
  const modelFlowRef = useRef<ModelFlow | null>(null);
  modelFlowRef.current = modelFlow;

  const refresh = useCallback(async () => {
    if (!enabled) {
      setNotes([]);
      return;
    }
    const result = await commands.grainSpaceListNotes();
    if (result.status === "ok") setNotes(result.data);
    else console.error("Grain Space: list failed:", result.error);
  }, [enabled]);

  useEffect(() => {
    refresh();
    if (!enabled) return;
    const unlisten = listen(NOTES_CHANGED_EVENT, () => refresh());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [enabled, refresh]);

  // Watch the download while this tab drives it (flow active).
  useEffect(() => {
    const unlistens = [
      listen<{ percentage: number }>(MODEL_PROGRESS_EVENT, (event) => {
        if (modelFlowRef.current)
          setModelFlow({
            state: "downloading",
            percentage: event.payload.percentage,
          });
      }),
      listen(MODEL_COMPLETE_EVENT, () => {
        if (modelFlowRef.current) {
          setModelFlow(null);
          // Verified on disk — NOW the setting may turn on.
          updateSetting("grain_space_semantic", true);
        }
      }),
      listen<string>(MODEL_ERROR_EVENT, (event) => {
        if (modelFlowRef.current)
          setModelFlow({ state: "error", message: event.payload });
      }),
    ];
    return () => {
      unlistens.forEach((p) => p.then((fn) => fn()));
    };
  }, []);

  const onSemanticToggle = async (value: boolean) => {
    if (!value) {
      setModelFlow(null);
      updateSetting("grain_space_semantic", false);
      return;
    }
    const status = await commands.grainSpaceEmbedModelStatus();
    if (status === "ready") {
      updateSetting("grain_space_semantic", true);
    } else if (status === "downloading") {
      setModelFlow({ state: "downloading", percentage: 0 });
    } else {
      setModelFlow({ state: "consent" });
    }
  };

  const startModelDownload = () => {
    setModelFlow({ state: "downloading", percentage: 0 });
    commands.grainSpaceDownloadEmbedModel().then((result) => {
      if (result.status === "error" && modelFlowRef.current) {
        setModelFlow((f) =>
          f?.state === "error" ? f : { state: "error", message: result.error },
        );
      }
    });
  };

  const cancelModelDownload = () => {
    void commands.grainSpaceCancelEmbedModelDownload();
    setModelFlow(null);
  };

  const openInOverlay = (note: Note) => {
    void commands.grainSpaceOpenWindow(note.id);
  };

  // Open the note's file in Obsidian via its deep link (vault backend only;
  // the backend builds + opens the URI so no custom-scheme capability is
  // needed). No-op result on the grain store.
  const openInObsidian = (note: Note) => {
    void commands.grainSpaceOpenInObsidian(note.id);
  };

  // Backend switch: turning the vault ON without a chosen vault first opens
  // the picker; the switch only flips once a valid folder is set.
  const onBackendToggle = async (useVault: boolean) => {
    setVaultMsg(null);
    if (!useVault) {
      await updateSetting("grain_space_backend", "grain");
      refresh();
      return;
    }
    if (!vaultPath) {
      const picked = await commands.grainSpacePickVault();
      if (picked.status !== "ok" || !picked.data) {
        if (picked.status === "error") setVaultMsg(picked.error);
        return; // cancelled — stay on the grain store
      }
    }
    await updateSetting("grain_space_backend", "obsidian");
    refresh();
  };

  const pickVault = async () => {
    setVaultMsg(null);
    const picked = await commands.grainSpacePickVault();
    if (picked.status === "error") setVaultMsg(picked.error);
    else if (picked.data) refresh();
  };

  const commitFolder = async () => {
    const draft = folderDraft?.trim();
    setFolderDraft(null);
    if (!draft || draft === vaultFolder) return;
    setVaultMsg(null);
    try {
      await updateSetting("grain_space_vault_folder", draft);
    } catch (e) {
      setVaultMsg(String(e));
    }
  };

  const deleteNote = async (id: string) => {
    await commands.grainSpaceDeleteNote(id);
    refresh();
  };
  const togglePin = async (note: Note) => {
    await commands.grainSpaceSetPinned(note.id, !note.is_pinned);
    refresh();
  };
  const exportNotes = async () => {
    setExportMsg(null);
    const res = await commands.grainSpaceExportNotes();
    if (res.status === "ok") {
      // null = the user cancelled the save dialog → stay quiet.
      if (res.data) setExportMsg(`Exported to ${res.data}`);
    } else {
      setExportMsg(res.error);
    }
  };
  const uninstallModel = async () => {
    setUninstallMsg(null);
    const res = await commands.grainSpaceUninstallEmbedModel();
    if (res.status === "ok") {
      // Model gone ⇒ semantic can't run; turn it off (also hides this button,
      // which is the visible confirmation).
      await updateSetting("grain_space_semantic", false);
    } else {
      setUninstallMsg(res.error);
    }
  };
  const dismissReminder = async (id: string) => {
    await commands.grainSpaceDismissReminder(id);
    refresh();
  };
  const armReminder = async (note: Note) => {
    const fireAt = reminderOf(note).fire_at;
    if (fireAt == null) return;
    await commands.grainSpaceArmReminder(note.id, fireAt);
    refresh();
  };

  const reminders = notes.filter((n) =>
    ["pending", "armed", "fired"].includes(reminderOf(n).status),
  );

  // Pinned first, then newest; grouped by local day.
  const sorted = [...notes].sort(
    (a, b) =>
      Number(b.is_pinned) - Number(a.is_pinned) || b.timestamp - a.timestamp,
  );
  const groups: { label: string; items: Note[] }[] = [];
  for (const note of sorted) {
    const label = note.is_pinned ? "Pinned" : dayLabel(note.timestamp);
    const last = groups[groups.length - 1];
    if (last && last.label === label) last.items.push(note);
    else groups.push({ label, items: [note] });
  }

  return (
    <div className="max-w-4xl w-full mx-auto space-y-6">
      <SettingsGroup
        title="Grain Space"
        description="A local scratch space for spoken and captured notes. Everything stays on this machine as plain files, and the feature holds zero memory while its surfaces are closed. Turning it off unregisters its shortcuts and loads nothing — your notes stay on disk."
      >
        <ToggleSwitch
          label="Enable Grain Space"
          description="Registers the capture shortcuts and the reminder timer."
          descriptionMode="inline"
          grouped
          checked={enabled}
          isUpdating={isUpdating("grain_space_enabled")}
          onChange={(v) => updateSetting("grain_space_enabled", v)}
        />
      </SettingsGroup>

      {enabled && (
        <>
          <SettingsGroup
            title="Storage"
            description="Where your notes live. The built-in Grain store keeps them as plain files in the app's data folder. Or point Grain at an Obsidian vault: notes become ordinary Markdown files you own — searchable here, editable in Obsidian, synced by whatever your vault already uses. Switching is a hard swap between the two stores; nothing is migrated or deleted."
          >
            <ToggleSwitch
              label="Store notes in an Obsidian vault"
              description="Capture writes Markdown into your vault; search and Recall cover the whole vault."
              descriptionMode="inline"
              grouped
              checked={backend === "obsidian"}
              isUpdating={isUpdating("grain_space_backend")}
              onChange={(v) => void onBackendToggle(v)}
            />
            {backend === "obsidian" && (
              <>
                <div className="flex items-center gap-3 px-4 py-3">
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-ink">
                      {t("settings.grainSpace.vaultFolderLabel")}
                    </div>
                    <div
                      className="text-xs text-ink-soft truncate"
                      title={vaultPath}
                    >
                      {vaultPath || t("settings.grainSpace.vaultUnset")}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => void pickVault()}
                    className="text-xs font-medium text-accent hover:underline shrink-0"
                  >
                    {t("settings.grainSpace.chooseVault")}
                  </button>
                </div>
                <div className="flex items-center gap-3 px-4 py-3">
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-ink">
                      {t("settings.grainSpace.subfolderLabel")}
                    </div>
                    <div className="text-xs text-ink-soft">
                      {t("settings.grainSpace.subfolderHint")}
                    </div>
                  </div>
                  <input
                    type="text"
                    value={folderDraft ?? vaultFolder}
                    onChange={(e) => setFolderDraft(e.target.value)}
                    onBlur={() => void commitFolder()}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                    }}
                    spellCheck={false}
                    className="w-36 shrink-0 rounded border border-line bg-transparent px-2 py-1 text-sm text-ink focus:border-accent focus:outline-none"
                  />
                </div>
                {vaultMsg && (
                  <div className="px-4 pb-3 text-xs text-red-500">{vaultMsg}</div>
                )}
              </>
            )}
          </SettingsGroup>

          <SettingsGroup
            title="Capture"
            description="Quick Add silently saves the text you have highlighted in any app. Create Note opens the Grain pill so you can speak OR type a note — and if you have text selected, that becomes the note (say what it's for). Each note gets an AI title, summary, and extracted reminders when a processing provider is configured."
          >
            <ShortcutInput
              shortcutId="grain_space_quick_add"
              grouped
              descriptionMode="inline"
            />
            <ShortcutInput
              shortcutId="grain_space_capture"
              grouped
              descriptionMode="inline"
            />
            <ShortcutInput
              shortcutId="grain_space_open"
              grouped
              descriptionMode="inline"
            />
            <ShortcutInput
              shortcutId="grain_space_recall"
              grouped
              descriptionMode="inline"
            />
            <ToggleSwitch
              label="Auto-set reminders"
              description="Arm reminders extracted from a dictated note automatically. When off, notes keep the suggestion and you arm it manually."
              descriptionMode="inline"
              grouped
              checked={autoReminders}
              isUpdating={isUpdating("grain_space_auto_reminders")}
              onChange={(v) => updateSetting("grain_space_auto_reminders", v)}
            />
          </SettingsGroup>

          <SettingsGroup
            title="Search"
            description="Notes are always searchable with fast exact/fuzzy text matching. Semantic search understands meaning ('that café Anna mentioned') but relies on a small local model (~34 MB) that downloads the first time you use it — nothing is bundled with the app, and with this off the model never loads into memory."
          >
            <ToggleSwitch
              label="Semantic search"
              description="Meaning-based search with a small local model, downloaded on first use."
              descriptionMode="inline"
              grouped
              checked={semantic}
              isUpdating={isUpdating("grain_space_semantic")}
              onChange={(v) => void onSemanticToggle(v)}
            />
            {modelFlow?.state === "consent" && (
              <div className="px-4 py-3 space-y-2">
                <div className="text-sm text-ink font-medium">
                  {t("settings.grainSpace.consentTitle")}
                </div>
                <div className="text-xs text-ink-soft">
                  {t("settings.grainSpace.consentBody")}
                </div>
                <div className="flex items-center gap-3 pt-1">
                  <button
                    type="button"
                    onClick={startModelDownload}
                    className="text-xs font-medium text-accent hover:underline"
                  >
                    {t("settings.grainSpace.consentConfirm")}
                  </button>
                  <button
                    type="button"
                    onClick={() => setModelFlow(null)}
                    className="text-xs text-ink-soft hover:text-ink"
                  >
                    {t("settings.grainSpace.consentCancel")}
                  </button>
                </div>
              </div>
            )}
            {modelFlow?.state === "downloading" && (
              <div className="px-4 py-3 flex items-center gap-3">
                <span className="text-xs text-ink-soft shrink-0">
                  {t("settings.grainSpace.downloading")}
                </span>
                <div className="flex-1 h-1 rounded bg-line overflow-hidden">
                  <div
                    className="h-full bg-accent transition-all"
                    style={{ width: `${modelFlow.percentage.toFixed(1)}%` }}
                  />
                </div>
                <button
                  type="button"
                  onClick={cancelModelDownload}
                  className="text-xs text-ink-soft hover:text-ink shrink-0"
                >
                  {t("settings.grainSpace.cancelDownload")}
                </button>
              </div>
            )}
            {modelFlow?.state === "error" && (
              <div className="px-4 py-3 flex items-center gap-3">
                <span className="text-xs text-red-500 flex-1 min-w-0 truncate">
                  {t("settings.grainSpace.downloadFailed", {
                    message: modelFlow.message,
                  })}
                </span>
                <button
                  type="button"
                  onClick={startModelDownload}
                  className="text-xs font-medium text-accent hover:underline shrink-0"
                >
                  {t("settings.grainSpace.consentConfirm")}
                </button>
                <button
                  type="button"
                  onClick={() => setModelFlow(null)}
                  className="text-xs text-ink-soft hover:text-ink shrink-0"
                >
                  {t("settings.grainSpace.dismiss")}
                </button>
              </div>
            )}
            {/* Precision choice — only meaningful once the model is in use. */}
            {semantic && !modelFlow && (
              <ToggleSwitch
                label="Half-precision (f16) model"
                description="Load the embedding model in f16 — about half the memory, near-identical results. Same download."
                descriptionMode="inline"
                grouped
                checked={embedF16}
                isUpdating={isUpdating("grain_space_embed_f16")}
                onChange={(v) => updateSetting("grain_space_embed_f16", v)}
              />
            )}
            {/* Semantic on ⇒ the model is on disk; offer to reclaim its ~130 MB. */}
            {semantic && !modelFlow && (
              <div className="px-4 py-3">
                <button
                  type="button"
                  onClick={() => void uninstallModel()}
                  className="font-mono text-[0.6rem] uppercase tracking-[0.18em] text-ink-soft hover:text-red-500 transition-colors"
                >
                  {t("settings.grainSpace.uninstallModel")}
                </button>
                {uninstallMsg && (
                  <div className="mt-1.5 text-xs text-red-500">{uninstallMsg}</div>
                )}
              </div>
            )}
          </SettingsGroup>

          {reminders.length > 0 && (
            <SettingsGroup
              title="Reminders"
              description="Reminders and timers extracted from your notes."
            >
              {reminders.map((note) => {
                const reminder = reminderOf(note);
                return (
                  <div
                    key={note.id}
                    className="flex items-center gap-3 px-4 py-3"
                  >
                    <AlarmClock
                      width={15}
                      height={15}
                      className={
                        reminder.status === "armed"
                          ? "text-accent shrink-0"
                          : "text-ink-faint shrink-0"
                      }
                    />
                    <div className="flex-1 min-w-0">
                      <div className="text-sm text-ink truncate">
                        {noteDisplayTitle(note)}
                      </div>
                      <div className="text-xs text-ink-soft">
                        {reminder.status === "pending"
                          ? "Suggested"
                          : reminder.status === "fired"
                            ? "Fired"
                            : "Armed"}
                        {reminder.fire_at != null &&
                          ` · ${fireFormat.format(new Date(reminder.fire_at))}`}
                      </div>
                    </div>
                    {reminder.status === "pending" &&
                      reminder.fire_at != null && (
                        <button
                          type="button"
                          onClick={() => armReminder(note)}
                          className="text-xs font-medium text-accent hover:underline shrink-0"
                        >
                          {t("settings.grainSpace.arm")}
                        </button>
                      )}
                    <button
                      type="button"
                      onClick={() => dismissReminder(note.id)}
                      className="text-xs text-ink-soft hover:text-ink shrink-0"
                    >
                      {t("settings.grainSpace.dismiss")}
                    </button>
                  </div>
                );
              })}
            </SettingsGroup>
          )}

          <SettingsGroup
            title="Notes"
            description={
              notes.length === 0
                ? "Nothing captured yet — highlight text and press Quick Add, or press Create Note to speak or type one."
                : undefined
            }
          >
            {groups.length === 0 ? (
              <div className="px-4 py-6 text-sm text-ink-faint text-center">
                {t("settings.grainSpace.empty")}
              </div>
            ) : (
              groups.map((group) => (
                <div key={group.label}>
                  <div className="px-4 pt-3 pb-1 font-mono text-[0.58rem] tracking-[0.18em] uppercase text-ink-faint">
                    {group.label}
                  </div>
                  {group.items.map((note) => (
                    <div
                      key={note.id}
                      className="group flex items-center gap-3 px-4 py-2.5"
                    >
                      {/* Click opens the overlay focused on this note. */}
                      <button
                        type="button"
                        onClick={() => openInOverlay(note)}
                        className="flex-1 min-w-0 text-left cursor-pointer"
                        title="Open in Grain Space"
                      >
                        <div className="text-sm text-ink truncate">
                          {noteDisplayTitle(note) || "(empty note)"}
                        </div>
                        {note.tldr.trim() && (
                          <div className="text-xs text-ink-soft truncate">
                            {note.tldr}
                          </div>
                        )}
                      </button>
                      <span className="text-xs text-ink-faint tabular-nums shrink-0">
                        {timeFormat.format(new Date(note.timestamp))}
                      </span>
                      {backend === "obsidian" && (
                        <button
                          type="button"
                          title={t("settings.grainSpace.openInObsidian")}
                          onClick={() => openInObsidian(note)}
                          className="shrink-0 text-ink-faint opacity-0 group-hover:opacity-100 hover:text-ink transition-opacity"
                        >
                          <ExternalLink width={14} height={14} />
                        </button>
                      )}
                      <button
                        type="button"
                        title={note.is_pinned ? "Unpin" : "Pin"}
                        onClick={() => togglePin(note)}
                        className={`shrink-0 transition-opacity ${
                          note.is_pinned
                            ? "text-accent"
                            : "text-ink-faint opacity-0 group-hover:opacity-100 hover:text-ink"
                        }`}
                      >
                        {note.is_pinned ? (
                          <Pin width={14} height={14} />
                        ) : (
                          <PinOff width={14} height={14} />
                        )}
                      </button>
                      <button
                        type="button"
                        title="Delete note"
                        onClick={() => deleteNote(note.id)}
                        className="shrink-0 text-ink-faint opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity"
                      >
                        <Trash2 width={14} height={14} />
                      </button>
                    </div>
                  ))}
                </div>
              ))
            )}
            {notes.length > 0 && (
              <div className="px-4 py-3">
                <button
                  type="button"
                  onClick={() => void exportNotes()}
                  className="font-mono text-[0.6rem] uppercase tracking-[0.18em] text-ink-soft hover:text-ink transition-colors"
                >
                  {t("settings.grainSpace.exportNotes")}
                </button>
                {exportMsg && (
                  <div className="mt-1.5 text-xs text-ink-faint break-all">
                    {exportMsg}
                  </div>
                )}
              </div>
            )}
          </SettingsGroup>
        </>
      )}
    </div>
  );
};
