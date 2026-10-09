export type ModelFamily = "standard" | "streaming";
export interface OnboardingModelDraft {
  enabledFamilies: Record<ModelFamily, boolean>;
  selectedModels: Record<ModelFamily, string>;
}
export const createOnboardingDraft = (): OnboardingModelDraft => ({
  enabledFamilies: { standard: true, streaming: true },
  selectedModels: { standard: "", streaming: "" },
});

/** Configure only the capture families installed during this setup. */
export function onboardingShortcutIds(draft: OnboardingModelDraft): string[] {
  const ids: string[] = [];
  if (draft.enabledFamilies.standard && draft.selectedModels.standard)
    ids.push("transcribe");
  if (draft.enabledFamilies.streaming && draft.selectedModels.streaming)
    ids.push("transcribe_native_asr");
  return ids;
}
