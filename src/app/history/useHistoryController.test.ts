import { describe, expect, it } from "vitest";
import type { HistoryEntry } from "@/bindings";
import { hasProcessedText, reduceHistoryEntries } from "./useHistoryController";

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
      reduceHistoryEntries([entry(1)], {
        action: "added",
        entry: entry(2),
      }).map((item) => item.id),
    ).toEqual([2, 1]);
    expect(
      reduceHistoryEntries([entry(2), entry(1)], {
        action: "updated",
        entry: { ...entry(2), title: "Updated" },
      })[0].title,
    ).toBe("Updated");
  });

  it("ignores updates for entries outside the loaded window", () => {
    const current = [entry(2), entry(1)];
    expect(
      reduceHistoryEntries(current, {
        action: "updated",
        entry: entry(99),
      }),
    ).toBe(current);
  });

  it("ignores delete and toggle events handled by optimistic commands", () => {
    const current = [entry(2), entry(1)];
    expect(reduceHistoryEntries(current, { action: "deleted", id: 2 })).toBe(
      current,
    );
    expect(reduceHistoryEntries(current, { action: "toggled", id: 1 })).toBe(
      current,
    );
  });
});

describe("UI 2.0 history processed-text test", () => {
  it("counts an entry as processed only when processing produced text", () => {
    expect(hasProcessedText(entry(1))).toBe(false);
    expect(
      hasProcessedText({ ...entry(1), post_processed_text: "Cleaned up" }),
    ).toBe(true);
  });

  it("does not trust post_process_requested or whitespace-only output", () => {
    // A requested run that failed or returned nothing leaves the flag set with
    // nothing to show — the History filter must not call that processed.
    expect(
      hasProcessedText({ ...entry(1), post_process_requested: true }),
    ).toBe(false);
    expect(
      hasProcessedText({
        ...entry(1),
        post_process_requested: true,
        post_processed_text: "   \n ",
      }),
    ).toBe(false);
  });
});
