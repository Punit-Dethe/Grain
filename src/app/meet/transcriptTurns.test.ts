import { describe, expect, it } from "vitest";
import type { Attribution, TranscriptRevision, Word } from "@/bindings";
import {
  buildTurns,
  filterTurns,
  formatClock,
  speakerLabels,
  transcriptText,
} from "./transcriptTurns";

function word(
  text: string,
  start_ms: number,
  end_ms: number,
  speaker: Attribution,
): Word {
  return { text, start_ms, end_ms, speaker };
}

function revision(
  tracks: { track: number; words: Word[] }[],
): TranscriptRevision {
  return {
    version: 1,
    revision: 1,
    session_id: "s",
    attempt_id: "a",
    created_unix_ms: 0,
    diarizer: null,
    asr: null,
    speakers: [
      { id: "t0-s1", track: 0, slot: 1, label: "Speaker 1" },
      { id: "t0-s2", track: 0, slot: 2, label: "Speaker 2" },
    ],
    tracks: tracks.map(({ track, words }) => ({
      track,
      status: "Complete",
      text: words.map((w) => w.text).join(" "),
      words,
      activity: [],
    })),
  };
}

describe("buildTurns", () => {
  it("groups by speaker and splits on long pauses", () => {
    const turns = buildTurns(
      revision([
        {
          track: 0,
          words: [
            word("Hello", 0, 300, { Speaker: 1 }),
            word("there.", 320, 600, { Speaker: 1 }),
            word("Hi!", 700, 900, { Speaker: 2 }),
            word("Again", 3_000, 3_300, { Speaker: 2 }),
            word("maybe", 3_400, 3_600, { Ambiguous: [1, 2] }),
            word("hm", 3_700, 3_800, "Unassigned"),
          ],
        },
      ]),
    );
    expect(turns.map((t) => [t.speakerId, t.text, t.ambiguous])).toEqual([
      ["t0-s1", "Hello there.", false],
      ["t0-s2", "Hi!", false],
      ["t0-s2", "Again", false],
      [null, "maybe", true],
      [null, "hm", false],
    ]);
    expect(turns[0]).toMatchObject({ startMs: 0, endMs: 600 });
  });

  it("interleaves tracks by time without merging across them", () => {
    const turns = buildTurns(
      revision([
        {
          track: 0,
          words: [
            word("a", 0, 100, { Speaker: 1 }),
            word("c", 400, 500, { Speaker: 1 }),
          ],
        },
        { track: 1, words: [word("b", 200, 300, { Speaker: 1 })] },
      ]),
    );
    expect(turns.map((t) => [t.track, t.text])).toEqual([
      [0, "a c"],
      [1, "b"],
    ]);
  });
});

describe("formatting", () => {
  it("formats meeting clock times", () => {
    expect(formatClock(0)).toBe("0:00");
    expect(formatClock(65_400)).toBe("1:05");
    expect(formatClock(3_725_000)).toBe("1:02:05");
  });

  it("exports labelled lines and filters by text", () => {
    const data = revision([
      {
        track: 0,
        words: [
          word("Budget", 0, 300, { Speaker: 2 }),
          word("ok", 5_000, 5_200, "Unavailable"),
        ],
      },
    ]);
    const turns = buildTurns(data);
    const labels = speakerLabels(data.speakers);
    expect(transcriptText(turns, labels, "Unknown")).toBe(
      "[0:00] Speaker 2: Budget\n[0:05] Unknown: ok",
    );
    expect(filterTurns(turns, "budg").map((t) => t.text)).toEqual(["Budget"]);
    expect(filterTurns(turns, "  ")).toHaveLength(2);
  });
});
