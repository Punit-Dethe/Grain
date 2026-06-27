import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import { Check, Pencil, Trash2 } from "lucide-react";
import type { PostProcessProvider } from "@/bindings";
import { BUILTIN_PP_IDS } from "./usePpPool";

interface PpProviderRowProps {
  provider: PostProcessProvider;
  model: string;
  /** Single-select active state (used when rotation is off). */
  isActive: boolean;
  /** Multi-select rotation state (used when rotation is on). */
  smartRotation: boolean;
  onToggleRotate: (
    provider: PostProcessProvider,
    enabled: boolean,
  ) => Promise<void>;
  onSetActive: (id: string) => Promise<void>;
  onEdit: (provider: PostProcessProvider) => void;
  onRemove: (id: string) => Promise<void>;
}

export const PpProviderRow: React.FC<PpProviderRowProps> = ({
  provider,
  model,
  isActive,
  smartRotation,
  onToggleRotate,
  onSetActive,
  onEdit,
  onRemove,
}) => {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);

  const enabled = provider.enabled ?? true;
  const quotaUsed = provider.quota_used_today ?? 0;
  const quotaLimit = provider.quota_limit ?? null;
  const removable = !BUILTIN_PP_IDS.has(provider.id);
  // The single control: checkbox in rotation mode, radio in single mode.
  const selected = smartRotation ? enabled : isActive;

  const handleSelect = async () => {
    setBusy(true);
    try {
      if (smartRotation) {
        await onToggleRotate(provider, !enabled);
      } else if (!isActive) {
        await onSetActive(provider.id);
      }
    } finally {
      setBusy(false);
    }
  };

  const handleRemove = async () => {
    const confirmed = await ask(
      t("settings.postProcessing.pool.removeConfirm", {
        name: provider.label,
      }),
      { title: t("settings.postProcessing.pool.remove"), kind: "warning" },
    );
    if (!confirmed) return;
    setBusy(true);
    try {
      await onRemove(provider.id);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="group flex items-center gap-3 px-4 py-3">
      {/* One selection control — radio (single) or checkbox (rotation). */}
      <button
        type="button"
        onClick={handleSelect}
        disabled={busy}
        title={
          smartRotation
            ? t("settings.postProcessing.pool.rotateTooltip")
            : t("settings.postProcessing.pool.setActive")
        }
        className={`shrink-0 w-[1.1rem] h-[1.1rem] flex items-center justify-center border-2 transition-colors ${
          smartRotation ? "rounded-[4px]" : "rounded-full"
        } ${
          selected ? "border-accent bg-accent" : "border-line hover:border-accent"
        } ${busy ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
      >
        {selected &&
          (smartRotation ? (
            <Check className="w-3 h-3 text-[var(--on-accent)]" />
          ) : (
            <span className="w-2 h-2 rounded-full bg-[var(--on-accent)]" />
          ))}
      </button>

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 min-w-0">
          <div className="text-sm font-medium text-ink truncate">
            {provider.label}
          </div>
          <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity duration-300 min-w-0">
            <span className="text-line font-medium shrink-0">|</span>
            <div className="flex items-center gap-1.5 text-xs text-ink-faint font-mono truncate">
              <span className="truncate">{provider.base_url}</span>
              {model ? <span className="shrink-0">· {model}</span> : null}
            </div>
          </div>
        </div>
        <div className="mt-0.5 text-[0.7rem] text-ink-soft">
          {quotaLimit != null
            ? t("settings.postProcessing.pool.quotaUsed", {
                used: quotaUsed,
                limit: quotaLimit,
              })
            : t("settings.postProcessing.pool.quotaUnlimited", {
                used: quotaUsed,
              })}
        </div>
      </div>

      <button
        type="button"
        onClick={() => onEdit(provider)}
        title={t("settings.postProcessing.pool.edit")}
        className="shrink-0 p-1.5 rounded-lg text-ink-soft hover:text-ink hover:bg-paper-sunken transition-colors cursor-pointer"
      >
        <Pencil className="w-4 h-4" />
      </button>
      {removable && (
        <button
          type="button"
          onClick={handleRemove}
          disabled={busy}
          title={t("settings.postProcessing.pool.remove")}
          className="shrink-0 p-1.5 rounded-lg text-ink-soft hover:text-status-error hover:bg-[var(--status-error-tint)] transition-colors cursor-pointer disabled:opacity-50"
        >
          <Trash2 className="w-4 h-4" />
        </button>
      )}
    </div>
  );
};
