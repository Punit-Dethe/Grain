import { describe, expect, it } from "vitest";
import { createBlankDraft, resolveNotesLanding } from "./notesLanding";

describe("resolveNotesLanding", () => {
  it("honors an explicit focus-note handoff in UI 2.0", () => {
    expect(resolveNotesLanding("next", "focused", "first-saved")).toEqual({
      kind: "note",
      id: "focused",
    });
  });

  it("keeps a UI 2.0 focus handoff when card listing fails", () => {
    expect(
      resolveNotesLanding("next", "focused", undefined, false, false),
    ).toEqual({ kind: "note", id: "focused" });
  });

  it("opens a blank draft on ordinary UI 2.0 navigation", () => {
    expect(resolveNotesLanding("next", null, "first-saved")).toEqual({
      kind: "draft",
    });
  });

  it("preserves the legacy first-note and empty-corpus behavior", () => {
    expect(resolveNotesLanding("default", null, "first-saved")).toEqual({
      kind: "note",
      id: "first-saved",
    });
    expect(resolveNotesLanding("default", null)).toEqual({ kind: "draft" });
  });

  it("falls back to the first legacy card for a stale focus id", () => {
    expect(
      resolveNotesLanding("default", "stale", "first-saved", true, false),
    ).toEqual({
      kind: "note",
      id: "first-saved",
    });
  });

  it("preserves the legacy no-selection state when listing fails", () => {
    expect(
      resolveNotesLanding("default", "focused", undefined, false, false),
    ).toEqual({
      kind: "none",
    });
  });

  it("selects a listed legacy focus id", () => {
    expect(
      resolveNotesLanding("default", "focused", "first-saved", true, true),
    ).toEqual({ kind: "note", id: "focused" });
  });
});

describe("createBlankDraft", () => {
  it("creates an empty unpersisted note", () => {
    expect(createBlankDraft(123)).toMatchObject({
      id: "",
      title: "",
      body: "",
      timestamp: 123,
    });
  });
});
