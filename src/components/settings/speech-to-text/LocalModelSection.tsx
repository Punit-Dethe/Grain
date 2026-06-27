import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import { useModelStore } from "@/stores/modelStore";
import { ModelLibrary } from "../ModelLibrary";

interface LocalModelSectionProps {
  /** Cloud smart-rotation is on — the local model is bypassed, so gray it out. */
  disabled?: boolean;
}

// [GRAIN] The local on-device model as a collapsible. Collapsed: a decently-thick
// summary row of the active model with a bottom "browse" strip. Expanded: the full
// model library (download / select / delete / language sort). Selecting a model
// auto-collapses it. Grays out (and locks) when cloud smart-rotation takes over.
export const LocalModelSection: React.FC<LocalModelSectionProps> = ({
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { models, currentModel } = useModelStore();
  const [expanded, setExpanded] = useState(false);
  // Mount the (heavy) library only once it's first opened, then keep it mounted so
  // the open/close height animation runs both ways.
  const [hasOpened, setHasOpened] = useState(false);

  const open = expanded && !disabled;
  useEffect(() => {
    if (open) setHasOpened(true);
  }, [open]);

  // Collapse + lock when cloud rotation takes over.
  useEffect(() => {
    if (disabled) setExpanded(false);
  }, [disabled]);

  // Auto-collapse once a (different) model becomes active.
  const prevModel = useRef(currentModel);
  useEffect(() => {
    if (prevModel.current !== currentModel && currentModel != null) {
      setExpanded(false);
    }
    prevModel.current = currentModel;
  }, [currentModel]);

  const active = models.find((m) => m.id === currentModel);
  const activeName = active?.name ?? t("settings.speechToText.localModel.none");

  return (
    <div className="space-y-2.5">
      <div className="flex items-center gap-2.5 px-1">
        <h2 className="font-mono text-[0.68rem] font-semibold text-ink uppercase tracking-[0.18em]">
          {t("settings.speechToText.localModel.title")}
        </h2>
        <div className="flex-1 flex items-center gap-2 translate-y-[-1px]">
          <span className="flex-1 border-t border-line" />
          <span className="grid place-items-center w-2.5 h-2.5 rounded-full border border-[var(--line-strong)] bg-paper shrink-0">
            <span className="w-1 h-1 rounded-full bg-ink-faint/60" />
          </span>
        </div>
      </div>
      <div
        className={`surface-well overflow-hidden transition-opacity duration-200 ${
          disabled ? "opacity-50" : ""
        }`}
      >
      {/* Summary row — decently thick */}
      <div className="flex items-center justify-between gap-3 px-4 py-4">
        <div className="min-w-0">
          <div className="text-sm font-semibold truncate">{activeName}</div>
          <div className="text-xs text-ink-soft">
            {disabled
              ? t("settings.speechToText.localModel.disabledByCloud")
              : t("settings.speechToText.localModel.activeHint")}
          </div>
        </div>

        {active && !disabled && (
          <div className="flex items-center gap-3 shrink-0">
            {(active.accuracy_score > 0 || active.speed_score > 0) && (
              <div className="flex flex-col gap-1">
                <div className="flex items-center gap-2 justify-end">
                  <span className="text-[0.65rem] font-medium text-ink-faint uppercase tracking-wider">
                    Accuracy
                  </span>
                  <div className="w-10 h-1 bg-ink-faint/20 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-ink-soft rounded-full"
                      style={{ width: `${active.accuracy_score * 100}%` }}
                    />
                  </div>
                </div>
                <div className="flex items-center gap-2 justify-end">
                  <span className="text-[0.65rem] font-medium text-ink-faint uppercase tracking-wider">
                    Speed
                  </span>
                  <div className="w-10 h-1 bg-ink-faint/20 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-ink-soft rounded-full"
                      style={{ width: `${active.speed_score * 100}%` }}
                    />
                  </div>
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Bottom expand strip */}
      <button
        type="button"
        disabled={disabled}
        onClick={() => setExpanded((e) => !e)}
        className="w-full flex items-center justify-center gap-1.5 py-2 border-t border-line bg-ink/[0.07] text-xs font-medium text-ink-soft disabled:cursor-not-allowed"
      >
        {expanded
          ? t("settings.speechToText.localModel.showLess")
          : t("settings.speechToText.localModel.browse")}
        <ChevronDown
          className={`w-3.5 h-3.5 transition-transform ${
            expanded ? "rotate-180" : ""
          }`}
        />
      </button>

      {/* Smooth height animation via the grid-rows 0fr↔1fr technique — no JS
          measuring, no jump. The inner clips its overflow as the row collapses. */}
      <div
        className="grid motion-reduce:transition-none"
        style={{ 
          gridTemplateRows: open ? "1fr" : "0fr",
          transition: "grid-template-rows 300ms ease-out"
        }}
      >
        <div className="overflow-hidden min-h-0">
          {hasOpened && (
            <div className="border-t border-line p-4">
              <ModelLibrary />
            </div>
          )}
        </div>
      </div>
      </div>
    </div>
  );
};
