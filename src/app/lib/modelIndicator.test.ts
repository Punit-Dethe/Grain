import { describe, expect, it } from "vitest";
import { getModelIndicatorStatus } from "./modelIndicator";

const state = {
  loading: false,
  currentModel: "standard",
  loadedModelId: null as string | null,
  models: [
    { id: "standard", name: "Parakeet TDT v2" },
    { id: "streaming", name: "Parakeet Unified" },
  ],
  isModelLoaded: false,
};

describe("sidebar model indicator", () => {
  it("shows the selected model before anything has been loaded", () => {
    expect(getModelIndicatorStatus(state)).toEqual({
      title: "Parakeet TDT v2",
      subtitle: "Unloaded · Local",
    });
  });

  it("keeps Streaming identity after idle unload instead of reverting to Standard", () => {
    const streaming = { ...state, loadedModelId: "streaming" };
    expect(
      getModelIndicatorStatus({ ...streaming, isModelLoaded: true }),
    ).toEqual({
      title: "Parakeet Unified",
      subtitle: "Loaded · Local",
    });
    expect(getModelIndicatorStatus(streaming)).toEqual({
      title: "Parakeet Unified",
      subtitle: "Unloaded · Local",
    });
  });

  it("follows the next resident model and retains it when unloaded", () => {
    const next = {
      ...state,
      currentModel: "streaming",
      loadedModelId: "standard",
    };
    expect(
      getModelIndicatorStatus({ ...next, isModelLoaded: true }).title,
    ).toBe("Parakeet TDT v2");
    expect(getModelIndicatorStatus(next)).toEqual({
      title: "Parakeet TDT v2",
      subtitle: "Unloaded · Local",
    });
  });

  it("distinguishes startup discovery from an empty selection", () => {
    const empty = { ...state, currentModel: "" };
    expect(getModelIndicatorStatus({ ...empty, loading: true })).toEqual({
      title: "Checking model",
      subtitle: "Checking",
    });
    expect(getModelIndicatorStatus(empty)).toEqual({
      title: "No model",
      subtitle: "Not loaded",
    });
  });
});
