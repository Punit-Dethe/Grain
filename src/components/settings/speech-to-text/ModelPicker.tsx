/**
 * [GRAIN] One model picker for both roles.
 *
 * Grain needs two models: one for Standard and Flow, one for Live streaming.
 * They were two visually identical collapsibles stacked on top of each other,
 * which asked the user to work out from their titles alone that these were
 * different registries serving different capture modes — and gave no answer at
 * all to "which one am I supposed to pick?".
 *
 * Here both roles are stated up front as two slots. Each says what it is for,
 * what is in it, and — when it is empty — that it is empty and how to fix that.
 * Choosing happens in one library below, with a tab per role, so the picker is
 * a single place rather than two lookalikes.
 */
import React, { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronRight, Plus } from "lucide-react";
import { useModelStore } from "@/stores/modelStore";
import { useSettings } from "@/hooks/useSettings";
import { ModelLibrary } from "../ModelLibrary";
import { AsrModelLibrary } from "../AsrModelLibrary";

export type ModelRole = "standard" | "streaming";

interface ModelPickerProps {
  /** Cloud smart-rotation is on — the on-device Standard model is bypassed. */
  disabled?: boolean;
}

interface SlotProps {
  role: ModelRole;
  label: string;
  purpose: string;
  modelName: string | null;
  open: boolean;
  disabled: boolean;
  disabledNote?: string;
  onOpen: () => void;
}

/**
 * An empty slot has to read as deliberately empty and immediately actionable.
 * A blank row with a chevron reads as broken, so the empty state says what is
 * missing and carries its own call to action.
 */
const ModelSlot: React.FC<SlotProps> = ({
  role,
  label,
  purpose,
  modelName,
  open,
  disabled,
  disabledNote,
  onOpen,
}) => {
  const { t } = useTranslation();
  const filled = Boolean(modelName);

  return (
    <button
      type="button"
      className={`model-slot${filled ? " is-filled" : " is-empty"}${open ? " is-open" : ""}`}
      aria-expanded={open}
      disabled={disabled}
      onClick={onOpen}
    >
      <span className="model-slot-role">{label}</span>
      <span className="model-slot-name">
        {disabled
          ? (disabledNote ?? t("settings.speechToText.picker.unavailable"))
          : filled
            ? modelName
            : t("settings.speechToText.picker.empty")}
      </span>
      <span className="model-slot-foot">
        <span className="model-slot-purpose">{purpose}</span>
        {!disabled && (
          <span className="model-slot-action">
            {filled ? (
              <>
                <Check size={13} aria-hidden="true" />
                {t("settings.speechToText.picker.change")}
              </>
            ) : (
              <>
                <Plus size={13} aria-hidden="true" />
                {t("settings.speechToText.picker.choose")}
              </>
            )}
            <ChevronRight size={13} aria-hidden="true" />
          </span>
        )}
      </span>
      <span className="model-slot-id" aria-hidden="true">
        {role}
      </span>
    </button>
  );
};

export const ModelPicker: React.FC<ModelPickerProps> = ({
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const { models: allModels, currentModel, initialize } = useModelStore();

  // Which role's library is open. `null` = closed; the picker is not a
  // permanently-expanded wall of models.
  const [openRole, setOpenRole] = useState<ModelRole | null>(null);
  // Mount each library only once its tab has been opened, then keep it mounted
  // so reopening is instant and the height animation runs both ways.
  const [mounted, setMounted] = useState<Record<ModelRole, boolean>>({
    standard: false,
    streaming: false,
  });

  useEffect(() => {
    void initialize();
  }, [initialize]);

  useEffect(() => {
    if (openRole) setMounted((prev) => ({ ...prev, [openRole]: true }));
  }, [openRole]);

  // Cloud rotation takes the Standard model out of the pipeline; an open
  // Standard tab would be editing something with no effect.
  useEffect(() => {
    if (disabled && openRole === "standard") setOpenRole(null);
  }, [disabled, openRole]);

  const selectedAsrModel = getSetting("selected_asr_model") ?? "";
  const streamingModels = useMemo(
    () => allModels.filter((m) => m.supports_streaming),
    [allModels],
  );

  const standardName =
    allModels.find((m) => m.id === currentModel)?.name ?? null;
  const streamingName =
    streamingModels.find((m) => m.id === selectedAsrModel)?.name ?? null;

  // Close the library once a role's model actually changes — the question that
  // opened it has been answered.
  const prev = useRef({ standard: currentModel, streaming: selectedAsrModel });
  useEffect(() => {
    const changed =
      (prev.current.standard !== currentModel && currentModel != null) ||
      (prev.current.streaming !== selectedAsrModel && selectedAsrModel !== "");
    if (changed) setOpenRole(null);
    prev.current = { standard: currentModel, streaming: selectedAsrModel };
  }, [currentModel, selectedAsrModel]);

  const toggle = (role: ModelRole) =>
    setOpenRole((current) => (current === role ? null : role));

  return (
    <div className="space-y-2.5">
      <div className="flex items-center gap-2.5 px-1">
        <h2 className="font-mono text-[0.68rem] font-semibold text-ink uppercase tracking-[0.18em]">
          {t("settings.speechToText.picker.title")}
        </h2>
        <div className="flex-1 flex items-center gap-2 translate-y-[-1px]">
          <span className="flex-1 border-t border-line" />
        </div>
      </div>

      <div className="surface-well overflow-hidden">
        <div className="model-slot-row">
          <ModelSlot
            role="standard"
            label={t("settings.speechToText.picker.standard.label")}
            purpose={t("settings.speechToText.picker.standard.purpose")}
            modelName={standardName}
            open={openRole === "standard"}
            disabled={disabled}
            disabledNote={t("settings.speechToText.localModel.disabledByCloud")}
            onOpen={() => toggle("standard")}
          />
          <ModelSlot
            role="streaming"
            label={t("settings.speechToText.picker.streaming.label")}
            purpose={t("settings.speechToText.picker.streaming.purpose")}
            modelName={streamingName}
            open={openRole === "streaming"}
            disabled={false}
            onOpen={() => toggle("streaming")}
          />
        </div>

        {/* Height animates via the grid-rows 0fr↔1fr technique — no JS
            measuring, no jump. */}
        <div
          className="grid motion-reduce:transition-none"
          style={{
            gridTemplateRows: openRole ? "1fr" : "0fr",
            transition: "grid-template-rows 260ms ease-out",
          }}
        >
          <div className="overflow-hidden min-h-0">
            <div className="model-library-panel">
              {/* The same tabs inside the library, so a user who opened the
                  wrong one can switch without closing and starting again. */}
              <div
                className="model-role-tabs"
                role="tablist"
                aria-label={t("settings.speechToText.picker.title")}
              >
                {(["standard", "streaming"] as const)
                  .filter((role) => role === "streaming" || !disabled)
                  .map((role) => (
                    <button
                      key={role}
                      type="button"
                      role="tab"
                      aria-selected={openRole === role}
                      className={openRole === role ? "active" : ""}
                      onClick={() => setOpenRole(role)}
                    >
                      {t(`settings.speechToText.picker.${role}.label`)}
                    </button>
                  ))}
              </div>
              {mounted.standard && (
                <div hidden={openRole !== "standard"}>
                  <ModelLibrary />
                </div>
              )}
              {mounted.streaming && (
                <div hidden={openRole !== "streaming"}>
                  <AsrModelLibrary />
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
