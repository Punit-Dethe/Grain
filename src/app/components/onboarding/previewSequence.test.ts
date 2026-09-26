import { describe, expect, it } from "vitest";
import { modePreviewFrames } from "./previewSequence";

const phrase = "Let’s move tomorrow’s meeting to eleven.";
describe("illustrative capture-mode preview", () => {
  it("keeps Standard work and text until recording stops", () => {
    const frames = modePreviewFrames("standard", phrase);
    expect(
      frames
        .filter((frame) => frame.phase === "recording")
        .every((frame) => frame.processed === 0 && !frame.transcript),
    ).toBe(true);
    expect(frames[frames.length - 1]?.transcript).toBe(phrase);
  });
  it("shows Flow work during capture but text only after finishing", () => {
    const frames = modePreviewFrames("flow", phrase);
    expect(
      frames.some(
        (frame) => frame.phase === "recording" && frame.processed > 0,
      ),
    ).toBe(true);
    expect(
      frames
        .filter((frame) => frame.phase !== "complete")
        .every((frame) => !frame.transcript),
    ).toBe(true);
  });
  it("shows native ASR words progressively while recording", () => {
    const frames = modePreviewFrames("streaming", phrase);
    const liveText = frames.filter(
      (frame) => frame.phase === "recording" && frame.transcript,
    );
    expect(liveText.length).toBeGreaterThan(1);
    expect(liveText[0].transcript.length).toBeLessThan(
      liveText[liveText.length - 1].transcript.length,
    );
  });
  it("ends each finite preview in the same complete transcript", () => {
    for (const mode of ["standard", "flow", "streaming"] as const) {
      const frames = modePreviewFrames(mode, phrase);
      expect(frames[frames.length - 1]).toMatchObject({
        phase: "complete",
        captured: 1,
        processed: 1,
        transcript: phrase,
      });
      expect(frames.map((frame) => frame.at)).toEqual(
        frames.map((frame) => frame.at).sort((a, b) => a - b),
      );
    }
  });
  it("provides three timer-free manual stages for reduced motion", () => {
    for (const mode of ["standard", "flow", "streaming"] as const) {
      const frames = modePreviewFrames(mode, phrase, true);
      expect(frames.map((frame) => frame.at)).toEqual([0, 0, 0]);
      expect(frames.map((frame) => frame.phase)).toEqual([
        "recording",
        "finishing",
        "complete",
      ]);
      expect(frames[0].transcript).toBe(mode === "streaming" ? phrase : "");
    }
  });
});
