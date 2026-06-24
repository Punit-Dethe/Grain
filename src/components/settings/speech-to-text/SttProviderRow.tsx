import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import { Check, KeyRound, Pencil, Trash2 } from "lucide-react";
import type { SttProvider } from "@/bindings";

interface SttProviderRowProps {
  provider: SttProvider;
  hasKey: boolean;
  /** Dim the row when cloud routing is off (providers are inactive). */
  inactive: boolean;
  onToggleRotate: (provider: SttProvider, enabled: boolean) => Promise<void>;
  onEdit: (provider: SttProvider) => void;
  onRemove: (id: string) => Promise<void>;
}

export const SttProviderRow: React.FC<SttProviderRowProps> = ({
  provider,
  hasKey,
  inactive,
  onToggleRotate,
  onEdit,
  onRemove,
}) => {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);

  const enabled = provider.enabled ?? true;
  const quotaUsed = provider.quota_used_today ?? 0;
  const quotaLimit = provider.quota_limit ?? null;

  const handleToggle = async () => {
    setBusy(true);
    try {
      await onToggleRotate(provider, !enabled);
    } finally {
      setBusy(false);
    }
  };

  const handleRemove = async () => {
    const confirmed = await ask(
      t("settings.speechToText.removeConfirm", { name: provider.name }),
      { title: t("settings.speechToText.remove"), kind: "warning" },
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
    <div
      className={`flex items-center gap-3 px-4 py-3 transition-opacity ${
        inactive ? "opacity-60" : ""
      }`}
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-ink truncate">
            {provider.name}
          </span>
          <span
            className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[0.65rem] font-medium ${
              hasKey
                ? "bg-status-ready/15 text-status-ready"
                : "bg-status-error/15 text-status-error"
            }`}
          >
            {hasKey ? (
              <Check className="w-3 h-3" />
            ) : (
              <KeyRound className="w-3 h-3" />
            )}
            {hasKey
              ? t("settings.speechToText.keySet")
              : t("settings.speechToText.noKey")}
          </span>
        </div>
        <div className="mt-0.5 flex items-center gap-2 text-xs text-ink-faint font-mono truncate">
          <span className="truncate">{provider.base_url}</span>
          {provider.model ? <span>· {provider.model}</span> : null}
        </div>
        <div className="mt-0.5 text-[0.7rem] text-ink-soft">
          {quotaLimit != null
            ? t("settings.speechToText.quotaUsed", {
                used: quotaUsed,
                limit: quotaLimit,
              })
            : t("settings.speechToText.quotaUnlimited", { used: quotaUsed })}
        </div>
      </div>

      {/* Rotate toggle — compact pill */}
      <button
        type="button"
        onClick={handleToggle}
        disabled={busy}
        title={t("settings.speechToText.rotateTooltip")}
        className={`shrink-0 inline-flex items-center gap-1 px-2 py-1 rounded-lg border text-xs font-medium transition-colors ${
          enabled
            ? "bg-accent text-[var(--on-accent)] border-accent"
            : "bg-paper-sunken text-ink-soft border-line hover:border-accent"
        } ${busy ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
      >
        {enabled && <Check className="w-3 h-3" />}
        {t("settings.speechToText.rotateLabel")}
      </button>

      <button
        type="button"
        onClick={() => onEdit(provider)}
        title={t("settings.speechToText.edit")}
        className="shrink-0 p-1.5 rounded-lg text-ink-soft hover:text-ink hover:bg-paper-sunken transition-colors cursor-pointer"
      >
        <Pencil className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={handleRemove}
        disabled={busy}
        title={t("settings.speechToText.remove")}
        className="shrink-0 p-1.5 rounded-lg text-ink-soft hover:text-status-error hover:bg-[var(--status-error-tint)] transition-colors cursor-pointer disabled:opacity-50"
      >
        <Trash2 className="w-4 h-4" />
      </button>
    </div>
  );
};
