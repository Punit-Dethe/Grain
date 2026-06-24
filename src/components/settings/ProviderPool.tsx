import React from "react";
import { Plus } from "lucide-react";
import { Button } from "../ui/Button";
import { Switch } from "../ui/Switch";
import { InfoHint } from "../ui/InfoHint";

interface ProviderPoolProps {
  title: string;
  addLabel: string;
  onAdd: () => void;
  addDisabled?: boolean;
  smartRotation: boolean;
  onToggleRotation: (enabled: boolean) => void;
  togglingRotation?: boolean;
  rotationLabel: string;
  rotationInfo: string;
  /** The provider list (rows / add form / empty state). */
  children: React.ReactNode;
}

// [GRAIN] Shared provider-pool card. A titled section whose header BAR carries
// "Add provider" on the left and the smart-rotation toggle (with a hover "i") on
// the right; the provider list sits in the lighter body below. Used by both the
// Transcription (cloud STT) and Processing (LLM) pools so the two read identically.
export const ProviderPool: React.FC<ProviderPoolProps> = ({
  title,
  addLabel,
  onAdd,
  addDisabled = false,
  smartRotation,
  onToggleRotation,
  togglingRotation = false,
  rotationLabel,
  rotationInfo,
  children,
}) => {
  return (
    <div className="space-y-2.5">
      <h2 className="px-1 font-mono text-[0.68rem] font-semibold text-ink uppercase tracking-[0.18em]">
        {title}
      </h2>

      <div className="rounded-xl border border-line overflow-hidden">
        {/* Header bar — darker strip: add (left) + smart rotation (right). */}
        <div className="flex items-center justify-between gap-3 px-3 py-2.5 bg-paper-sunken border-b border-line">
          <Button
            onClick={onAdd}
            variant="secondary"
            size="sm"
            disabled={addDisabled}
            className="inline-flex items-center gap-1.5"
          >
            <Plus className="w-4 h-4" />
            {addLabel}
          </Button>

          <div className="flex items-center gap-1.5">
            <span className="text-xs font-medium text-ink-soft">
              {rotationLabel}
            </span>
            <InfoHint text={rotationInfo} position="bottom" />
            <Switch
              checked={smartRotation}
              onChange={onToggleRotation}
              isUpdating={togglingRotation}
              ariaLabel={rotationLabel}
            />
          </div>
        </div>

        {/* Body — the provider list. */}
        <div className="bg-paper-raised">
          <div className="divide-y divide-line">{children}</div>
        </div>
      </div>
    </div>
  );
};
