import { describe, expect, it } from "vitest";
import type { NoteCard } from "@/bindings";
import { moveNoteId, orderFolderNotes } from "./noteOrdering";

const card = (
  id: string,
  timestamp: number,
  manualOrder: number | null,
): NoteCard => ({
  id,
  title: id,
  tldr: "",
  timestamp,
  is_pinned: false,
  reminder_state: { status: "none", fire_at: null },
  folder: "Work",
  manual_order: manualOrder,
  readonly: false,
});

describe("orderFolderNotes", () => {
  it("places explicit positions first and keeps unordered notes newest-first", () => {
    const ordered = orderFolderNotes([
      card("new", 30, null),
      card("second", 20, 1),
      card("old", 10, null),
      card("first", 5, 0),
    ]);
    expect(ordered.map(({ id }) => id)).toEqual([
      "first",
      "second",
      "new",
      "old",
    ]);
  });
});

describe("moveNoteId", () => {
  it("moves a later note before an earlier sibling", () => {
    expect(moveNoteId(["a", "b", "c"], "c", "a", "before")).toEqual([
      "c",
      "a",
      "b",
    ]);
  });

  it("moves an earlier note after a later sibling", () => {
    expect(moveNoteId(["a", "b", "c"], "a", "c", "after")).toEqual([
      "b",
      "c",
      "a",
    ]);
  });

  it("leaves the list intact for a stale or self target", () => {
    expect(moveNoteId(["a", "b"], "a", "a", "after")).toEqual(["a", "b"]);
    expect(moveNoteId(["a", "b"], "missing", "a", "before")).toEqual([
      "a",
      "b",
    ]);
  });
});
