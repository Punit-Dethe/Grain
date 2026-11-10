import { describe, expect, it } from "vitest";
import type { MeetingSummary } from "@/bindings";
import {
  groupByDay,
  highlightParts,
  recordingActivity,
  turnAt,
  wordAt,
} from "./meetFormat";
import type { Turn } from "./transcriptTurns";

function meeting(id: string, created: Date): MeetingSummary {
  return {
    id,
    title: id,
    created_unix_ms: created.getTime(),
    state: "Recorded",
    duration_ms: 0,
    transcript_revision: null,
    preview: null,
    speaker_count: 0,
    sources: ["Microphone"],
  };
}

function turn(key: string, startMs: number, endMs: number): Turn {
  return {
    key,
    track: 0,
    speakerId: null,
    startMs,
    endMs,
    text: key,
    words: [],
  };
}

describe("groupByDay", () => {
  it("groups consecutive meetings by local day", () => {
    const now = new Date(2026, 10, 2, 15, 0);
    const groups = groupByDay(
      [
        meeting("a", new Date(2026, 10, 2, 14, 0)),
        meeting("b", new Date(2026, 10, 2, 9, 0)),
        meeting("c", new Date(2026, 10, 1, 23, 59)),
        meeting("d", new Date(2026, 9, 28, 10, 0)),
      ],
      now,
    );
    expect(groups.map((g) => [g.kind, g.meetings.map((m) => m.id)])).toEqual([
      ["today", ["a", "b"]],
      ["yesterday", ["c"]],
      ["date", ["d"]],
    ]);
  });

  it("filters titles and previews, then reverses days and rows for oldest first", () => {
    const now = new Date(2026, 10, 2, 15, 0);
    const meetings = [
      meeting("Plan review", new Date(2026, 10, 2, 14, 0)),
      meeting("Planning call", new Date(2026, 10, 2, 9, 0)),
      {
        ...meeting("Weekly sync", new Date(2026, 10, 1, 16, 0)),
        preview: "Agreed on a plan.",
      },
      meeting("Lunch", new Date(2026, 10, 1, 12, 0)),
    ];

    const groups = groupByDay(meetings, now, {
      query: " PLAN ",
      oldestFirst: true,
    });

    expect(
      groups.map((group) => [group.kind, group.meetings.map((m) => m.id)]),
    ).toEqual([
      ["yesterday", ["Weekly sync"]],
      ["today", ["Planning call", "Plan review"]],
    ]);
    expect(meetings[0].id).toBe("Plan review");
    expect(
      groupByDay(meetings, now, { query: "missing", oldestFirst: false }),
    ).toEqual([]);
  });
});

describe("recordingActivity", () => {
  it("counts saved recordings by local date and leaves future cells blank", () => {
    const now = new Date(2026, 10, 2, 15);
    const activity = recordingActivity(
      [
        meeting("before range", new Date(2026, 2, 7, 23, 59)),
        meeting("first day", new Date(2026, 2, 8, 23, 59)),
        meeting("Sunday", new Date(2026, 10, 1, 23, 59)),
        meeting("Monday morning", new Date(2026, 10, 2, 0, 1)),
        meeting("Monday afternoon", new Date(2026, 10, 2, 14)),
        { ...meeting("live", now), state: "Recording" },
        meeting("future", new Date(2026, 10, 3)),
      ],
      now,
    );

    expect(activity.weeks).toHaveLength(35);
    expect(activity.weeks[0][0]).toEqual({
      date: new Date(2026, 2, 8),
      count: 1,
    });
    expect(activity.activeDays).toBe(3);
    expect(activity.weeks[activity.weeks.length - 1]).toEqual([
      { date: new Date(2026, 10, 1), count: 1 },
      { date: new Date(2026, 10, 2), count: 2 },
      null,
      null,
      null,
      null,
      null,
    ]);
  });
});

describe("highlightParts", () => {
  it("marks every case-insensitive match", () => {
    expect(highlightParts("Plan the plan", "plan")).toEqual([
      { text: "Plan", match: true },
      { text: " the ", match: false },
      { text: "plan", match: true },
    ]);
    expect(highlightParts("text", " ")).toEqual([
      { text: "text", match: false },
    ]);
  });
});

describe("turnAt", () => {
  it("finds the turn being heard, and none in a pause", () => {
    const turns = [turn("a", 0, 1_000), turn("b", 2_000, 3_000)];
    expect(turnAt(turns, 500)).toBe("a");
    expect(turnAt(turns, 1_500)).toBeNull();
    expect(turnAt(turns, 2_000)).toBe("b");
    expect(turnAt(turns, 9_000)).toBeNull();
  });
});

describe("wordAt", () => {
  it("finds the last word that has started", () => {
    const words = [
      { text: "a", startMs: 0, endMs: 200 },
      { text: "b", startMs: 300, endMs: 500 },
      { text: "c", startMs: 600, endMs: 900 },
    ];
    expect(wordAt(words, -1)).toBe(-1);
    expect(wordAt(words, 0)).toBe(0);
    expect(wordAt(words, 250)).toBe(0);
    expect(wordAt(words, 300)).toBe(1);
    expect(wordAt(words, 5_000)).toBe(2);
    expect(wordAt([], 10)).toBe(-1);
  });
});
