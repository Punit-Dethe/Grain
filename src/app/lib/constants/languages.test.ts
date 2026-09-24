import { expect, it } from "vitest";
import { supportsLanguageCode } from "./languages";

it("offers Norwegian when a model advertises Bokmal", () => {
  expect(supportsLanguageCode(["nb"], "no")).toBe(true);
});

it("offers Tagalog when a model advertises Filipino", () => {
  expect(supportsLanguageCode(["fil"], "tl")).toBe(true);
});
