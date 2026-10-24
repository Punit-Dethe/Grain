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
  it("retains the original motion with a linear normalized input", () => {
    // Original Rust trajectories rescaled from the retired sensitivity curve
    // to 0.2 input. Attack, outward travel, spring and release are unchanged.
    const reference = new Map([
      [1, [0.003300566, 0.008199905, 0.008199905]],
      [5, [0.021118981, 0.052467871, 0.089708968]],
      [12, [0.033888339, 0.126323993, 0.179940944]],
      [30, [0.078988262, 0.199483215, 0.199922023]],
      [45, [0.078154862, 0.126891633, 0.064268621]],
      [60, [0.021152419, 0.024461955, 0.011190585]],
      [90, [0.000611331, 0.000692642, 0.000315173]],
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
  it("preserves Handy's normalized scale without gating quiet levels or boosting speech", () => {
    for (const amplitude of [0, 0.001, 0.006, 0.012, 0.2, 0.5, 1]) {
      const field = new WaveField();
      const full = new WaveField();
      // A flat normalized spectrum must reach the motion unchanged, including
      // values below Grain's old 0.012 cutoff. Repeat through attack/release.
      const input = audioAmplitude(Array(16).fill(amplitude));
      for (let frame = 1; frame <= 90; frame++) {
        const active = frame <= 30;
        field.advance(1 / 60, active ? input : 0, 13);
        full.advance(1 / 60, active ? 1 : 0, 13);
        for (let i = 0; i < 13; i++) {
          expect(field.bars[i]).toBeCloseTo(full.bars[i] * amplitude, 10);
        }
      }
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
