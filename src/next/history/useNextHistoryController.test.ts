import { describe, expect, it } from "vitest";
import type { HistoryEntry } from "@/bindings";
import { reduceNextHistoryEntries } from "./useNextHistoryController";

const entry = (id: number, saved = false): HistoryEntry => ({
  id,
  file_name: `${id}.wav`,
  timestamp: id,
  saved,
  title: `Entry ${id}`,
  transcription_text: `Text ${id}`,
  post_processed_text: null,
  post_process_prompt: null,
  post_process_requested: false,
});

describe("UI 2.0 history reducer", () => {
  it("adds and updates entries without duplicates", () => {
    expect(
      reduceNextHistoryEntries([entry(1)], {
        action: "added",
        entry: entry(2),
      }).map((item) => item.id),
    ).toEqual([2, 1]);
    expect(
      reduceNextHistoryEntries([entry(2), entry(1)], {
        action: "updated",
        entry: { ...entry(2), title: "Updated" },
      })[0].title,
    ).toBe("Updated");
  });

  it("ignores updates for entries outside the loaded window", () => {
    const current = [entry(2), entry(1)];
    expect(
      reduceNextHistoryEntries(current, {
        action: "updated",
        entry: entry(99),
      }),
    ).toBe(current);
  });

  it("ignores delete and toggle events handled by optimistic commands", () => {
    const current = [entry(2), entry(1)];
    expect(
      reduceNextHistoryEntries(current, { action: "deleted", id: 2 }),
    ).toBe(current);
    expect(
      reduceNextHistoryEntries(current, { action: "toggled", id: 1 }),
    ).toBe(current);
  });
});
