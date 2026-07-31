import { describe, expect, it } from "vitest";
import {
  clampRecentNotesVisibleCount,
  RECENT_NOTES_PAGE_SIZE,
  revealMoreRecentNotes,
} from "./recentNotesPagination";

describe("recent notes pagination", () => {
  it("starts with five notes when more are available", () => {
    expect(clampRecentNotesVisibleCount(RECENT_NOTES_PAGE_SIZE, 18)).toBe(5);
  });

  it("reveals five more notes at a time", () => {
    expect(revealMoreRecentNotes(5, 18)).toBe(10);
    expect(revealMoreRecentNotes(10, 18)).toBe(15);
  });

  it("stops at the available note count", () => {
    expect(revealMoreRecentNotes(15, 18)).toBe(18);
    expect(revealMoreRecentNotes(18, 18)).toBe(18);
  });

  it("clamps the visible window when the corpus shrinks", () => {
    expect(clampRecentNotesVisibleCount(15, 3)).toBe(3);
    expect(clampRecentNotesVisibleCount(5, 0)).toBe(0);
  });
});
