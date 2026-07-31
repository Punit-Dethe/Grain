import { describe, expect, it } from "vitest";
import { hashForRoute, routeFromHash } from "./navigation";

describe("UI 2.0 hash navigation", () => {
  it.each([
    "general",
    "advanced",
    "speech-to-text",
    "post-processing",
    "debug",
  ] as const)("round-trips the %s settings section", (section) => {
    const route = { page: "settings", section } as const;
    expect(routeFromHash(hashForRoute(route))).toEqual(route);
  });

  it("keeps query and anchor targets out of route parsing", () => {
    expect(
      routeFromHash("#/settings/advanced?focus=history#retention"),
    ).toEqual({ page: "settings", section: "advanced" });
  });

  it.each(["", "#/settings", "#/settings/unknown", "#/not-a-page"])(
    "resolves unknown hash %s safely to overview",
    (hash) => expect(routeFromHash(hash)).toEqual({ page: "overview" }),
  );
});
