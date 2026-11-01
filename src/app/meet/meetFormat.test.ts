import { describe, expect, it } from "vitest";
import type { MeetingSummary } from "@/bindings";
import {
  groupByDay,
  highlightParts,
  levelFraction,
  turnAt,
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
    sources: ["Microphone"],
  };
}

function turn(key: string, startMs: number, endMs: number): Turn {
  return {
    key,
    track: 0,
    speakerId: null,
    ambiguous: false,
    startMs,
    endMs,
    text: key,
  };
}

describe("levelFraction", () => {
  it("maps peaks onto a 60 dB scale", () => {
    expect(levelFraction(0)).toBe(0);
    expect(levelFraction(Number.NaN)).toBe(0);
    expect(levelFraction(1)).toBe(1);
    expect(levelFraction(0.001)).toBeCloseTo(0, 5);
    expect(levelFraction(0.1)).toBeCloseTo(2 / 3, 5);
    expect(levelFraction(4)).toBe(1);
  });
});

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
