import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import { Check, GripVertical, Pencil, Trash2 } from "lucide-react";
import type { PostProcessProvider } from "@/bindings";
import { isBuiltinPpId } from "./usePpPool";

interface PpProviderRowProps {
  provider: PostProcessProvider;
  model: string;
  /** Single-select active state (used when fallback is off). */
  isActive: boolean;
  /** Multi-select fallback state (used when fallback is on). */
  fallbackEnabled: boolean;
  onToggleFallbackProvider: (
    provider: PostProcessProvider,
    enabled: boolean,
  ) => Promise<void>;
  onSetActive: (id: string) => Promise<void>;
  onEdit: (provider: PostProcessProvider) => void;
  onRemove: (id: string) => Promise<void>;
  position: number;
  reorderDisabled: boolean;
  dragging: boolean;
  onDragStart: (
    event: React.PointerEvent<HTMLButtonElement>,
    id: string,
  ) => void;
  onMove: (id: string, direction: number) => void;
}

export const PpProviderRow: React.FC<PpProviderRowProps> = ({
  provider,
  model,
  isActive,
  fallbackEnabled,
  onToggleFallbackProvider,
  onSetActive,
  onEdit,
  onRemove,
  position,
  reorderDisabled,
  dragging,
  onDragStart,
  onMove,
}) => {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);

  const enabled = provider.enabled ?? true;
  const removable = !isBuiltinPpId(provider.id);
  // The single control: checkbox in fallback mode, radio in single mode.
  const selected = fallbackEnabled ? enabled : isActive;

  const handleSelect = async () => {
    setBusy(true);
    try {
      if (fallbackEnabled) {
        await onToggleFallbackProvider(provider, !enabled);
      } else if (!isActive) {
        await onSetActive(provider.id);
      }
    } catch {
      // The shared pool store displays command errors.
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
    } catch {
      // The shared pool store displays command errors.
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      data-provider-row={provider.id}
      className={`group flex items-center gap-3 px-4 py-3 ${dragging ? "bg-paper-sunken ring-1 ring-inset ring-accent/40" : ""}`}
    >
      {/* One selection control — radio (single) or checkbox (fallback). */}
      <button
        type="button"
        onClick={handleSelect}
        disabled={busy || reorderDisabled}
        role={fallbackEnabled ? "checkbox" : "radio"}
        aria-checked={selected}
        aria-label={provider.label}
        title={
          fallbackEnabled
            ? t("settings.postProcessing.pool.fallbackProviderTooltip")
            : t("settings.postProcessing.pool.setActive")
        }
        className={`shrink-0 w-[1.1rem] h-[1.1rem] flex items-center justify-center border-2 transition-colors ${
          fallbackEnabled ? "rounded-[4px]" : "rounded-full"
        } ${
          selected
            ? "border-accent bg-accent"
            : "border-line hover:border-accent"
        } ${busy ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
      >
        {selected &&
          (fallbackEnabled ? (
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
          {fallbackEnabled && (
            <span
              className="text-xs text-ink-faint font-mono"
              aria-label={t("settings.postProcessing.pool.priority", {
                position: position + 1,
              })}
            >
              {position + 1}
            </span>
          )}
          <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-all duration-300 ease-in-out min-w-0">
            <span className="text-line font-medium shrink-0">|</span>
            <div className="flex items-center gap-1.5 text-xs text-ink-faint font-mono truncate">
              <span className="truncate">{provider.base_url}</span>
              {model ? <span className="shrink-0">· {model}</span> : null}
            </div>
          </div>
        </div>
      </div>

      {fallbackEnabled && (
        <button
          type="button"
          disabled={busy || reorderDisabled}
          aria-label={t("settings.postProcessing.pool.reorder", {
            name: provider.label,
          })}
          title={t("settings.postProcessing.pool.reorder", {
            name: provider.label,
          })}
          data-provider-grip={provider.id}
          aria-keyshortcuts="ArrowUp ArrowDown"
          onPointerDown={(event) => onDragStart(event, provider.id)}
          onKeyDown={(event) => {
            if (event.key === "ArrowUp" || event.key === "ArrowDown") {
              event.preventDefault();
              onMove(provider.id, event.key === "ArrowUp" ? -1 : 1);
            }
          }}
          style={{ touchAction: "none" }}
          className="shrink-0 p-1.5 rounded-lg text-ink-soft hover:text-ink hover:bg-paper-sunken cursor-grab active:cursor-grabbing disabled:opacity-50 disabled:cursor-not-allowed focus-visible:outline-2 focus-visible:outline-accent"
        >
          <GripVertical className="w-4 h-4" aria-hidden="true" />
        </button>
      )}
      <button
        type="button"
        disabled={busy || reorderDisabled}
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
          disabled={busy || reorderDisabled}
          title={t("settings.postProcessing.pool.remove")}
          className="shrink-0 p-1.5 rounded-lg text-ink-soft hover:text-status-error hover:bg-[var(--status-error-tint)] transition-colors cursor-pointer disabled:opacity-50"
        >
          <Trash2 className="w-4 h-4" />
        </button>
      )}
    </div>
  );
};
