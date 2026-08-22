import { describe, expect, it } from "vitest";
import { normalizeCustomWord } from "./customWords";

describe("normalizeCustomWord", () => {
  it("preserves phrases while collapsing whitespace and unsafe punctuation", () => {
    expect(normalizeCustomWord("  <Mac   Book>  Pro  ")).toBe("Mac Book Pro");
  });
});
