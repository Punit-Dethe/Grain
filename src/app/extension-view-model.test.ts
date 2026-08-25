import { describe, expect, it } from "vitest";
import type { ExtensionChoiceCandidate } from "@/bindings";
import {
  filterChoiceCandidates,
  resolveChoiceSelection,
} from "./extension-view-model";

const candidates: ExtensionChoiceCandidate[] = [
  {
    extensionId: "grain.linear",
    name: "Linear",
    purpose: "Issue tracking",
    signal: "topical",
    icon: null,
  },
  {
    extensionId: "grain.slack",
    name: "Slack",
    purpose: "Team messages",
    signal: "none",
    icon: null,
  },
];

describe("extension chooser model", () => {
  it("matches all query terms across trusted candidate metadata", () => {
    expect(filterChoiceCandidates(candidates, "team slack")).toEqual([
      candidates[1],
    ]);
  });

  it("selects a recommendation but not an alphabetical fallback", () => {
    expect(resolveChoiceSelection(candidates, "", null)).toBe("grain.linear");
    expect(
      resolveChoiceSelection(
        candidates.map((candidate) => ({ ...candidate, signal: "none" })),
        "",
        null,
      ),
    ).toBeNull();
  });

  it("selects the first filtered result and preserves a visible choice", () => {
    expect(resolveChoiceSelection([candidates[1]], "message", null)).toBe(
      "grain.slack",
    );
    expect(resolveChoiceSelection(candidates, "", "grain.slack")).toBe(
      "grain.slack",
    );
  });
});
