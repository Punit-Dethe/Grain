import { describe, expect, it } from "vitest";
import { reorderVisibleProviders } from "./providerOrder";

describe("provider fallback ordering", () => {
  it("moves in both directions without losing unconfigured templates", () => {
    const all = ["template", "a", "hidden", "b", "c"];
    expect(reorderVisibleProviders(all, ["a", "b", "c"], "c", 0)).toEqual([
      "template",
      "c",
      "hidden",
      "a",
      "b",
    ]);
    expect(reorderVisibleProviders(all, ["a", "b", "c"], "a", 2)).toEqual([
      "template",
      "b",
      "hidden",
      "c",
      "a",
    ]);
  });
  it("clamps end positions and ignores an unknown dragged provider", () => {
    expect(reorderVisibleProviders(["a", "b"], ["a", "b"], "a", 10)).toEqual([
      "b",
      "a",
    ]);
    expect(reorderVisibleProviders(["a", "b"], ["a", "b"], "b", -1)).toEqual([
      "b",
      "a",
    ]);
    expect(
      reorderVisibleProviders(["a", "b"], ["a", "b"], "missing", 0),
    ).toEqual(["a", "b"]);
  });
});
