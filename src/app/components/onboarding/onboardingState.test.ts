import { describe, expect, it } from "vitest";
import { createOnboardingDraft, onboardingModes } from "./onboardingState";

const batch =
  "handy-computer/parakeet-tdt-0.6b-v2-gguf/parakeet-tdt-0.6b-v2-Q4_K_M.gguf";
const draft = (standard: boolean, streaming: boolean, model = batch) => ({
  enabledFamilies: { standard, streaming },
  selectedModels: { standard: model, streaming: "native-asr-model" },
});

describe("onboarding capture availability", () => {
  it("offers only native Streaming for ASR-only setup", () => {
    expect(onboardingModes(draft(false, true))).toEqual(["streaming"]);
  });
  it("offers Standard and Flow for a supported batch-only setup", () => {
    expect(onboardingModes(draft(true, false))).toEqual(["flow", "standard"]);
  });
  it("offers all three for both selected families", () => {
    expect(onboardingModes(draft(true, true))).toEqual([
      "flow",
      "standard",
      "streaming",
    ]);
  });
  it("does not offer Flow for arbitrary batch or custom Parakeet models", () => {
    expect(onboardingModes(draft(true, false, "whisper-small"))).toEqual([
      "standard",
    ]);
    expect(
      onboardingModes(
        draft(true, false, "custom/parakeet-tdt-0.6b-v2-Q4_K_M.gguf"),
      ),
    ).toEqual(["standard"]);
  });
  it("does not offer Flow when translation is enabled", () => {
    expect(onboardingModes(draft(true, true), true)).toEqual([
      "standard",
      "streaming",
    ]);
  });
  it("does not infer usable modes from an empty or deselected setup", () => {
    expect(onboardingModes(createOnboardingDraft())).toEqual([]);
    expect(onboardingModes(draft(false, false))).toEqual([]);
  });
});
