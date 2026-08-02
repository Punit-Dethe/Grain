import { describe, expect, it } from "vitest";
import {
  hashForRoute,
  routeFromHash,
  routeUsesCompactGlobalRail,
} from "./navigation";

describe("UI 2.0 hash navigation", () => {
  it("round-trips the Notes workspace", () => {
    const route = { page: "notes" } as const;
    expect(hashForRoute(route)).toBe("#/notes");
    expect(routeFromHash("#/notes")).toEqual(route);
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
    "speech-to-text",
    "post-processing",
    "debug",
    "about",
  ] as const)("round-trips the %s settings section", (section) => {
    const route = { page: "settings", section } as const;
    expect(routeFromHash(hashForRoute(route))).toEqual(route);
  });

  it.each(["dictionary", "snippets", "context", "agent"] as const)(
    "round-trips the %s tool section",
    (section) => {
      const route = { page: "tools", section } as const;
      expect(routeFromHash(hashForRoute(route))).toEqual(route);
    },
  );

  it.each(["installed", "store"] as const)(
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

  it("round-trips an encoded standalone extension settings destination", () => {
    const route = {
      page: "extension-settings",
      extensionId: "voice/actions local",
    } as const;
    expect(hashForRoute(route)).toBe(
      "#/extension-settings/voice%2Factions%20local",
    );
    expect(routeFromHash(hashForRoute(route))).toEqual(route);
  });

  it("rejects missing or malformed standalone extension identifiers", () => {
    expect(routeFromHash("#/extension-settings")).toEqual({
      page: "overview",
    });
    expect(routeFromHash("#/extension-settings/%E0%A4%A")).toEqual({
      page: "overview",
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
      routeUsesCompactGlobalRail({
        page: "extension-settings",
        extensionId: "voice-actions",
      }),
    ).toBe(true);
    expect(
      routeUsesCompactGlobalRail({ page: "extensions", view: "installed" }),
    ).toBe(false);
    expect(routeUsesCompactGlobalRail({ page: "overview" })).toBe(false);
  });
});
