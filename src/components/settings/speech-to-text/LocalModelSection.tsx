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
      <h2 className="px-1 font-mono text-[0.68rem] font-semibold text-ink uppercase tracking-[0.18em]">
        {t("settings.speechToText.localModel.title")}
      </h2>
      <div
        className={`surface-well overflow-hidden transition-opacity duration-200 ${
          disabled ? "opacity-50" : ""
        }`}
      >
      {/* Summary row — decently thick */}
      <div className="flex items-center gap-3 px-4 py-4">
        <span
          className={`w-2 h-2 rounded-full shrink-0 ${
            disabled ? "bg-ink-faint" : "bg-status-ready"
          }`}
        />
        <div className="min-w-0">
          <div className="text-sm font-semibold truncate">{activeName}</div>
          <div className="text-xs text-ink-soft">
            {disabled
              ? t("settings.speechToText.localModel.disabledByCloud")
              : t("settings.speechToText.localModel.activeHint")}
          </div>
        </div>
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
        className="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none"
        style={{ gridTemplateRows: open ? "1fr" : "0fr" }}
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
