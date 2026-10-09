import { describe, expect, it } from "vitest";
import {
  audioAmplitude,
  barCount,
  WaveField,
  WAVE_WIDTH,
  WAVE_HEIGHT,
  WAVE_BAR_WIDTH,
  waveBarX,
  waveBarHalfLength,
} from "./wave";

describe("Grain's retained waveform", () => {
  it("matches trajectories evaluated by the original Rust WaveField", () => {
    // Captured from main's native renderer with 25 bars, 0.2 input for 30
    // frames, then silence. Covers attack, outward travel and delayed release.
    const reference = new Map([
      [1, [0.007821477, 0.01943163, 0.01943163]],
      [5, [0.05004646, 0.12433513, 0.21258679]],
      [12, [0.0803065, 0.29935482, 0.42641297]],
      [30, [0.18718152, 0.47272304, 0.4737629]],
      [45, [0.18520658, 0.30069998, 0.15229982]],
      [60, [0.0501257, 0.057968434, 0.02651876]],
      [90, [0.0014486954, 0.0016413801, 0.0007468766]],
    ]);
    // The compact 13-bar slot has the same sampled positions at 0, 3, 6.
    for (const count of [13, 25]) {
      const field = new WaveField();
      for (let frame = 1; frame <= 90; frame++) {
        field.advance(1 / 60, frame <= 30 ? 0.2 : 0, count);
        reference
          .get(frame)
          ?.forEach((value, index) =>
            expect(field.bars[(index * (count - 1)) / 4]).toBeCloseTo(value, 5),
          );
        for (let i = 0; i < count; i++)
          expect(field.bars[i]).toBeCloseTo(field.bars[count - 1 - i], 10);
      }
      field.reset();
      expect([...field.bars].every((value) => value === 0)).toBe(true);
    }
  });
  it("retains native compact bar spacing and visible round-cap heights", () => {
    const count = barCount(WAVE_WIDTH);
    expect(count).toBe(13);
    expect(waveBarX(0, count)).toBeCloseTo(3.3);
    expect(waveBarX(6, count)).toBeCloseTo(25.5);
    expect(waveBarX(12, count)).toBeCloseTo(47.7);
    for (let i = 1; i < count; i++)
      expect(
        waveBarX(i, count) - waveBarX(i - 1, count) - WAVE_BAR_WIDTH,
      ).toBeCloseTo(1.7);
    expect(2 * waveBarHalfLength(0) + WAVE_BAR_WIDTH).toBe(2);
    const fullHeight = 2 * waveBarHalfLength(1) + WAVE_BAR_WIDTH;
    expect(fullHeight).toBeCloseTo(23.6096);
    expect(fullHeight).toBeLessThan(WAVE_HEIGHT);
  });
  it("rejects malformed audio and stays bounded across a stalled frame", () => {
    expect(audioAmplitude([NaN, Infinity, -1, 2])).toBeCloseTo(Math.sqrt(0.5));
    expect(audioAmplitude([NaN])).toBe(0);
    const field = new WaveField();
    field.advance(60, 1, 48);
    expect(
      [...field.bars].every(
        (value) => Number.isFinite(value) && value >= 0 && value <= 1,
      ),
    ).toBe(true);
    expect(barCount(0)).toBe(0);
    expect(barCount(10000)).toBe(47);
  });
});
