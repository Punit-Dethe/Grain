import { describe, expect, it } from "vitest";
import {
  createOnboardingDraft,
  onboardingShortcutIds,
} from "./onboardingState";

const draft = (standard: boolean, streaming: boolean) => ({
  enabledFamilies: { standard, streaming },
  selectedModels: { standard: "standard-model", streaming: "streaming-model" },
});
describe("onboarding shortcut availability", () => {
  it("configures only Streaming for an ASR-only installation", () => {
    expect(onboardingShortcutIds(draft(false, true))).toEqual([
      "transcribe_native_asr",
    ]);
  });
  it("configures only Standard for a standard-only installation", () => {
    expect(onboardingShortcutIds(draft(true, false))).toEqual(["transcribe"]);
  });
  it("configures both installed capture families", () => {
    expect(onboardingShortcutIds(draft(true, true))).toEqual([
      "transcribe",
      "transcribe_native_asr",
    ]);
  });
  it("does not offer shortcuts for empty or deselected models", () => {
    expect(onboardingShortcutIds(createOnboardingDraft())).toEqual([]);
    expect(onboardingShortcutIds(draft(false, false))).toEqual([]);
  });
});
