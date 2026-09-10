import { describe, expect, it } from "vitest";
import type { ModelInfo } from "@/bindings";
import { getFlowAvailability, isReviewedFlowModelId } from "./flowAvailability";

const v3 =
  "handy-computer/parakeet-tdt-0.6b-v3-gguf/parakeet-tdt-0.6b-v3-Q8_0.gguf";

const model = (id: string, is_downloaded = true): ModelInfo =>
  ({ id, is_downloaded }) as ModelInfo;

describe("Flow availability", () => {
  it("accepts only reviewed v2/v3 catalog artifacts and quantizations", () => {
    expect(isReviewedFlowModelId(v3)).toBe(true);
    expect(
      isReviewedFlowModelId(
        "handy-computer/parakeet-tdt-0.6b-v2-gguf/parakeet-tdt-0.6b-v2-F16.gguf",
      ),
    ).toBe(true);
    expect(isReviewedFlowModelId(`local/${v3.split("/")[1]}`)).toBe(false);
    expect(isReviewedFlowModelId(v3.replace("Q8_0", "Q2_K"))).toBe(false);
    expect(isReviewedFlowModelId(v3.replace(".gguf", ".bin"))).toBe(false);
  });

  it("requires the selected artifact to be installed", () => {
    expect(getFlowAvailability([model(v3)], v3, false)).toEqual({
      available: true,
    });
    expect(getFlowAvailability([model(v3, false)], v3, false)).toEqual({
      available: false,
      reason: "model",
    });
    expect(getFlowAvailability([], v3, false)).toEqual({
      available: false,
      reason: "model",
    });
  });

  it("disables Flow while translation is enabled", () => {
    expect(getFlowAvailability([model(v3)], v3, true)).toEqual({
      available: false,
      reason: "translation",
    });
    expect(getFlowAvailability([], v3, true)).toEqual({
      available: false,
      reason: "modelAndTranslation",
    });
  });
});
