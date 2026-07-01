import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import { useAsrModelStore } from "@/stores/asrModelStore";
import { useSettings } from "@/hooks/useSettings";
import { AsrModelLibrary } from "../AsrModelLibrary";

// [GRAIN] The Streaming / Native-ASR model section — the structural twin of
// `LocalModelSection`, sitting directly below it. Collapsed: a summary row of
// the selected streaming model with a "browse" strip. Expanded: the full ASR
// model library (download / select / delete). Selecting a model auto-collapses.
export const AsrModelSection: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const { models, initialize } = useAsrModelStore();

  const selectedAsrModel = getSetting("selected_asr_model") ?? "";

  const [expanded, setExpanded] = useState(false);
  const [hasOpened, setHasOpened] = useState(false);

  // Populate the ASR registry on mount (lists the catalog + install state).
  useEffect(() => {
    void initialize();
  }, [initialize]);

  useEffect(() => {
    if (expanded) setHasOpened(true);
  }, [expanded]);

  // Auto-collapse once a (different) streaming model becomes selected.
  const prevModel = useRef(selectedAsrModel);
  useEffect(() => {
    if (prevModel.current !== selectedAsrModel && selectedAsrModel) {
      setExpanded(false);
    }
    prevModel.current = selectedAsrModel;
  }, [selectedAsrModel]);

  const active = models.find((m) => m.id === selectedAsrModel);
  const activeName = active?.name ?? t("settings.speechToText.asrModel.none");

  return (
    <div className="space-y-2.5">
      <div className="flex items-center gap-2.5 px-1">
        <h2 className="font-mono text-[0.68rem] font-semibold text-ink uppercase tracking-[0.18em]">
          {t("settings.speechToText.asrModel.title")}
        </h2>
        <div className="flex-1 flex items-center gap-2 translate-y-[-1px]">
          <span className="flex-1 border-t border-line" />
          <span className="grid place-items-center w-2.5 h-2.5 rounded-full border border-[var(--line-strong)] bg-paper shrink-0">
            <span className="w-1 h-1 rounded-full bg-ink-faint/60" />
          </span>
        </div>
      </div>
      <div className="surface-well overflow-hidden">
        {/* Summary row */}
        <div className="flex items-center justify-between gap-3 px-4 py-4">
          <div className="min-w-0">
            <div className="text-sm font-semibold truncate">{activeName}</div>
            <div className="text-xs text-ink-soft">
              {active
                ? t("settings.speechToText.asrModel.activeHint")
                : t("settings.speechToText.asrModel.noneHint")}
            </div>
          </div>
          {active && (
            <div className="flex items-center gap-1.5 shrink-0 text-[0.65rem] font-medium text-ink-faint uppercase tracking-wider">
              ~{active.memory_mb} MB
            </div>
          )}
        </div>

        {/* Bottom expand strip */}
        <button
          type="button"
          onClick={() => setExpanded((e) => !e)}
          className="w-full flex items-center justify-center gap-1.5 py-2 border-t border-line bg-ink/[0.07] text-xs font-medium text-ink-soft"
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

        <div
          className="grid motion-reduce:transition-none"
          style={{
            gridTemplateRows: expanded ? "1fr" : "0fr",
            transition: "grid-template-rows 300ms ease-out",
          }}
        >
          <div className="overflow-hidden min-h-0">
            {hasOpened && (
              <div className="border-t border-line p-4">
                <AsrModelLibrary />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
