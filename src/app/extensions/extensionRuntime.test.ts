import { describe, expect, it } from "vitest";
import { filterExtensions } from "./extensionRuntime";

describe("extension collection helpers", () => {
  it("searches names, ids, and descriptions directly", () => {
    const entries = [
      {
        id: "voice.actions",
        name: "Voice Actions",
        description: "Voice controls",
      },
      { id: "app.modes", name: "App Modes", description: "App profiles" },
    ];
    expect(filterExtensions(entries, "voice")).toEqual([entries[0]]);
    expect(filterExtensions(entries, "app.modes")).toEqual([entries[1]]);
  });
});
