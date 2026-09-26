import type { OnboardingTestMode } from "@/bindings";
import { isReviewedFlowModelId } from "@/lib/flowAvailability";

export type ModelFamily = "standard" | "streaming";
export interface OnboardingModelDraft {
  enabledFamilies: Record<ModelFamily, boolean>;
  selectedModels: Record<ModelFamily, string>;
}
export const createOnboardingDraft = (): OnboardingModelDraft => ({
  enabledFamilies: { standard: true, streaming: true },
  selectedModels: { standard: "", streaming: "" },
});

/** Only present modes backed by the models selected during this setup. */
export function onboardingModes(
  draft: OnboardingModelDraft,
  translateToEnglish = false,
): OnboardingTestMode[] {
  const modes: OnboardingTestMode[] = [];
  if (draft.enabledFamilies.standard && draft.selectedModels.standard) {
    if (
      isReviewedFlowModelId(draft.selectedModels.standard) &&
      !translateToEnglish
    )
      modes.push("flow");
    modes.push("standard");
  }
  if (draft.enabledFamilies.streaming && draft.selectedModels.streaming)
    modes.push("streaming");
  return modes;
}
