import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { FolderOpen, HardDrive } from "lucide-react";
import { commands } from "../../../bindings";
import { useSettings } from "../../../hooks/useSettings";
import { InfoHint } from "../../ui/InfoHint";
import { SettingsGroup } from "../../ui/SettingsGroup";

type Backend = "grain" | "obsidian";

/**
 * [GRAIN] Where notes live.
 *
 * This was a switch labelled "Store notes in an Obsidian vault", which framed
 * one of two equal choices as a modification of the other and left the Grain
 * store's own location unsaid — so the common question, "where are my notes?",
 * had no answer anywhere in the UI.
 *
 * It is now a choice between two places, each showing its own path and each
 * letting you change it. Choosing does not move anything, and says so: a folder
 * picker that silently relocated the user's files would be a surprise nobody
 * asked for.
 */
export const StorageSection: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const backend = (getSetting("grain_space_backend") ?? "grain") as Backend;
  const vaultPath = getSetting("grain_space_vault_path") ?? "";
  const vaultFolder = getSetting("grain_space_vault_folder") ?? "Grain";

  const [storePath, setStorePath] = useState("");
  const [folderDraft, setFolderDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The resolved path, not the setting: empty means "the default", and the
  // whole point of this row is to say where that actually is.
  const refreshStorePath = useCallback(() => {
    void commands.grainSpaceStoreFolder().then((res) => {
      if (res.status === "ok") setStorePath(res.data);
    });
  }, []);
  useEffect(refreshStorePath, [refreshStorePath]);

  const choose = async (next: Backend) => {
    setError(null);
    if (next === backend) return;
    // Obsidian without a vault is a broken state, so ask for the vault as part
    // of choosing it rather than switching first and failing afterwards.
    if (next === "obsidian" && !vaultPath) {
      const res = await commands.grainSpacePickVault();
      if (res.status !== "ok" || !res.data) return;
    }
    await updateSetting("grain_space_backend", next);
  };

  const pickVault = async () => {
    setError(null);
    const res = await commands.grainSpacePickVault();
    if (res.status !== "ok") setError(res.error);
  };

  const pickStore = async () => {
    setError(null);
    const res = await commands.grainSpacePickStoreFolder();
    if (res.status !== "ok") setError(res.error);
    else refreshStorePath();
  };

  const resetStore = async () => {
    await updateSetting("grain_space_store_path", "");
    refreshStorePath();
  };

  const commitFolder = async () => {
    const draft = folderDraft?.trim();
    setFolderDraft(null);
    if (!draft || draft === vaultFolder) return;
    await updateSetting("grain_space_vault_folder", draft);
  };

  return (
    <SettingsGroup
      title="Where your notes live"
      info="Notes are plain Markdown files either way — nothing is locked in a database. Switching between the two places changes where NEW notes are written and what search covers; it never moves or deletes what is already there."
    >
      <div className="px-4 py-4 grid grid-cols-1 sm:grid-cols-2 gap-2.5">
        <Choice
          icon={<HardDrive width={16} height={16} />}
          title="On this computer"
          blurb="A folder Grain manages. Nothing else needs to be installed."
          selected={backend === "grain"}
          busy={isUpdating("grain_space_backend")}
          onSelect={() => void choose("grain")}
        />
        <Choice
          icon={<FolderOpen width={16} height={16} />}
          title="In an Obsidian vault"
          blurb="Notes join a vault you already keep — edited in Obsidian, synced by whatever syncs it."
          selected={backend === "obsidian"}
          busy={isUpdating("grain_space_backend")}
          onSelect={() => void choose("obsidian")}
        />
      </div>

      {backend === "grain" ? (
        <PathRow
          label="Notes folder"
          value={storePath}
          empty="Resolving…"
          action="Change"
          onAction={() => void pickStore()}
          secondary={
            getSetting("grain_space_store_path")
              ? { label: "Use the default", onClick: () => void resetStore() }
              : undefined
          }
        />
      ) : (
        <>
          <PathRow
            label={t("settings.grainSpace.vaultFolderLabel")}
            value={vaultPath}
            empty={t("settings.grainSpace.vaultUnset")}
            action={t("settings.grainSpace.chooseVault")}
            onAction={() => void pickVault()}
          />
          <div className="flex items-center gap-3 px-4 py-3">
            <div className="flex-1 min-w-0 flex items-center gap-2">
              <div className="text-sm text-ink">
                {t("settings.grainSpace.subfolderLabel")}
              </div>
              <InfoHint text={t("settings.grainSpace.subfolderHint")} />
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
              className="w-40 shrink-0 rounded-lg border border-line bg-paper-sunken px-2.5 py-1.5 text-sm text-ink focus:border-accent focus:outline-none"
            />
          </div>
        </>
      )}

      {error && <div className="px-4 pb-3 text-xs text-red-500">{error}</div>}
    </SettingsGroup>
  );
};

/** One of the two places, as a card you pick rather than a switch you flip. */
const Choice: React.FC<{
  icon: React.ReactNode;
  title: string;
  blurb: string;
  selected: boolean;
  busy: boolean;
  onSelect: () => void;
}> = ({ icon, title, blurb, selected, busy, onSelect }) => (
  <button
    type="button"
    onClick={onSelect}
    disabled={busy}
    aria-pressed={selected}
    className={`text-left rounded-xl border p-3.5 transition-colors disabled:opacity-60 disabled:cursor-not-allowed ${
      selected
        ? "border-accent bg-[var(--accent-tint)]"
        : "border-line hover:border-ink-faint cursor-pointer"
    }`}
  >
    <div className="flex items-center gap-2">
      <span className={selected ? "text-accent" : "text-ink-faint"}>
        {icon}
      </span>
      <span className="text-sm font-medium text-ink">{title}</span>
    </div>
    <p className="mt-1.5 text-xs text-ink-soft leading-relaxed">{blurb}</p>
  </button>
);

/** A path, in full, with the button that changes it. */
const PathRow: React.FC<{
  label: string;
  value: string;
  empty: string;
  action: string;
  onAction: () => void;
  secondary?: { label: string; onClick: () => void };
}> = ({ label, value, empty, action, onAction, secondary }) => (
  <div className="flex items-center gap-3 px-4 py-3 border-t border-line">
    <div className="flex-1 min-w-0">
      <div className="text-sm text-ink">{label}</div>
      <div className="text-xs text-ink-soft break-all" title={value}>
        {value || empty}
      </div>
    </div>
    <div className="flex items-center gap-3 shrink-0">
      {secondary && (
        <button
          type="button"
          onClick={secondary.onClick}
          className="text-xs text-ink-faint hover:text-ink transition-colors cursor-pointer"
        >
          {secondary.label}
        </button>
      )}
      <button
        type="button"
        onClick={onAction}
        className="text-xs font-medium text-accent hover:underline cursor-pointer"
      >
        {action}
      </button>
    </div>
  </div>
);
