import { describe, expect, it } from "vitest";
import {
  hashForRoute,
  routeFromHash,
  routeUsesCompactGlobalRail,
} from "./navigation";

describe("UI 2.0 hash navigation", () => {
  it("opens merged transcription settings from the former models route", () => {
    expect(routeFromHash("#/settings/speech-to-text")).toEqual({
      page: "settings",
      section: "capture",
    });
  });
  it("falls back to overview for the removed Notes route", () => {
    expect(routeFromHash("#/notes")).toEqual({ page: "overview" });
  });

  it("round-trips the meeting workspace and keeps the full rail", () => {
    const route = { page: "meet" } as const;
    expect(hashForRoute(route)).toBe("#/meet");
    expect(routeFromHash("#/meet")).toEqual(route);
    expect(routeFromHash("#/meet/")).toEqual(route);
    expect(routeUsesCompactGlobalRail(route)).toBe(false);
  });

  it("round-trips an opened recording and rejects unsafe ids", () => {
    const route = { page: "meet", meeting: "5032d7a6-e1ab" } as const;
    expect(hashForRoute(route)).toBe("#/meet/5032d7a6-e1ab");
    expect(routeFromHash("#/meet/5032d7a6-e1ab")).toEqual(route);
    expect(routeFromHash("#/meet/../x")).toEqual({ page: "overview" });
  });

  it("sends the retired About tab to its settings section", () => {
    const section = { page: "settings", section: "about" } as const;
    expect(routeFromHash("#/about")).toEqual(section);
    expect(routeFromHash("#/about/")).toEqual(section);
    expect(hashForRoute(section)).toBe("#/settings/about");
  });

  it.each([
    "capture",
    "audio",
    "output",
    "application",
    "post-processing",
    "debug",
    "about",
  ] as const)("round-trips the %s settings section", (section) => {
    const route = { page: "settings", section } as const;
    expect(routeFromHash(hashForRoute(route))).toEqual(route);
  });

  it.each(["dictionary", "snippets", "context"] as const)(
    "round-trips the %s tool section",
    (section) => {
      const route = { page: "tools", section } as const;
      expect(routeFromHash(hashForRoute(route))).toEqual(route);
    },
  );

  it("opens Agent as a standalone page, including the former Studio link", () => {
    const route = { page: "agent" } as const;
    expect(hashForRoute(route)).toBe("#/agent");
    expect(routeFromHash(hashForRoute(route))).toEqual(route);
    expect(routeFromHash("#/tools/agent")).toEqual(route);
    expect(routeFromHash("#/agent/?focus=shortcut")).toEqual(route);
  });

  it.each(["installed", "browse"] as const)(
    "round-trips the %s extension view",
    (view) => {
      const route = { page: "extensions", view } as const;
      expect(routeFromHash(hashForRoute(route))).toEqual(route);
    },
  );

  it("resolves top-level Tools and Extensions to their default views", () => {
    expect(routeFromHash("#/tools")).toEqual({
      page: "tools",
      section: "dictionary",
    });
    expect(routeFromHash("#/extensions")).toEqual({
      page: "extensions",
      view: "installed",
    });
  });

  it("keeps query and anchor targets out of route parsing", () => {
    expect(
      routeFromHash("#/settings/application?focus=history#retention"),
    ).toEqual({ page: "settings", section: "application" });
  });

  it("still answers the pre-split settings routes", () => {
    // A bookmark, a deep link in a doc, or a stale window all still land
    // somewhere sensible rather than on the fallback.
    expect(routeFromHash("#/settings/general")).toEqual({
      page: "settings",
      section: "capture",
    });
    expect(routeFromHash("#/settings/advanced")).toEqual({
      page: "settings",
      section: "application",
    });
  });

  it.each([
    "",
    "#/settings",
    "#/settings/unknown",
    "#/tools/unknown",
    "#/extensions/unknown",
    "#/not-a-page",
  ])("resolves unknown hash %s safely to overview", (hash) =>
    expect(routeFromHash(hash)).toEqual({ page: "overview" }),
  );

  it("marks only nested workspace routes for the compact global rail", () => {
    expect(
      routeUsesCompactGlobalRail({ page: "tools", section: "dictionary" }),
    ).toBe(true);
    expect(
      routeUsesCompactGlobalRail({ page: "extensions", view: "installed" }),
    ).toBe(false);
    expect(routeUsesCompactGlobalRail({ page: "overview" })).toBe(false);
    expect(routeUsesCompactGlobalRail({ page: "agent" })).toBe(false);
  });
});
