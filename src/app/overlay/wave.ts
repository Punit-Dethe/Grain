// Grain WaveField, translated from the retired Rust renderer. The input remains
// Handy's normalized FFT buckets; this is bounded presentation state only.
const MAX_BARS = 48;
const HISTORY = 24;
const HZ = 60;
export const WAVE_WIDTH = 51;
export const WAVE_HEIGHT = 26;
export const WAVE_BAR_WIDTH = 2;
const WAVE_BAR_GAP = 1.7;
// Native compact renderer: 40px body, 0.85 shrink, 1.12 boost, 0.62 fill.
const WAVE_MAX_HEIGHT = 40 * 0.85 * 1.12 * 0.62;
export function waveBarX(index: number, count: number): number {
  const pitch = WAVE_BAR_WIDTH + WAVE_BAR_GAP;
  const used = count * pitch - WAVE_BAR_GAP;
  return (WAVE_WIDTH - used) / 2 + index * pitch + WAVE_BAR_WIDTH / 2;
}
// Round line caps add one bar width to the segment's visible height.
export const waveBarHalfLength = (value: number) =>
  (value * (WAVE_MAX_HEIGHT - WAVE_BAR_WIDTH)) / 2;
export const barCount = (width: number) =>
  width <= 0.5
    ? 0
    : Math.max(0, Math.min(MAX_BARS, Math.floor((width + 1.7) / 3.7)) - 1);

export function audioAmplitude(levels: readonly number[]): number {
  let sum = 0;
  let count = 0;
  for (const level of levels) {
    if (!Number.isFinite(level)) continue;
    const value = Math.max(0, Math.min(1, level));
    sum += value * value;
    count++;
  }
  return count ? Math.sqrt(sum / count) : 0;
}

export class WaveField {
  readonly bars = new Float64Array(MAX_BARS);
  private readonly velocity = new Float64Array(MAX_BARS);
  private readonly history = new Float64Array(HISTORY);
  private level = 0;
  private written = 0;
  private accumulated = 0;

  reset() {
    this.bars.fill(0);
    this.velocity.fill(0);
    this.history.fill(0);
    this.level = this.written = this.accumulated = 0;
  }

  private at(back: number) {
    const newest = this.written - 1;
    const index = Math.max(
      Math.max(0, newest - HISTORY + 1),
      Math.min(newest, newest - back),
    );
    return this.history[index % HISTORY];
  }

  advance(seconds: number, amplitude: number, count: number) {
    const dt = Math.max(0, Math.min(0.1, seconds));
    const target = Math.pow(
      Math.max(0, Math.min(1, amplitude) - 0.012) / (1 - 0.012),
      0.45,
    );
    const tau = target > this.level ? 0.028 : 0.14;
    this.level += (target - this.level) * (1 - Math.exp(-dt / tau));
    this.accumulated += dt * HZ;
    let budget = HISTORY;
    while (this.accumulated >= 1 && budget-- > 0) {
      this.accumulated--;
      this.history[this.written++ % HISTORY] = this.level;
    }
    if (this.accumulated >= 1) this.accumulated = 0;
    const n = Math.min(MAX_BARS, count);
    const edge = Math.min(3, Math.floor((n - 1) / 2));
    for (let i = 0; i < n; i++) {
      const position = n <= 1 ? 0.5 : i / (n - 1);
      const back = Math.abs(position - 0.5) * 2 * 0.22 * HZ + this.accumulated;
      const whole = Math.floor(back);
      const delayed =
        this.written === 0
          ? this.level
          : this.at(whole) +
            (this.at(whole + 1) - this.at(whole)) * (back - whole);
      const distance = Math.min(i, n - 1 - i);
      const gain =
        edge && distance < edge
          ? 0.3 +
            0.7 *
              (0.5 - 0.5 * Math.cos((Math.PI * (distance + 1)) / (edge + 1)))
          : 1;
      const x = this.bars[i];
      const v = this.velocity[i];
      const f = 1 + 2 * dt * 26;
      const hoo = dt * 26 * 26;
      const hhoo = dt * hoo;
      const inverse = 1 / (f + hhoo);
      this.bars[i] = Math.max(
        0,
        Math.min(1, (f * x + dt * v + hhoo * delayed * gain) * inverse),
      );
      this.velocity[i] = (v + hoo * (delayed * gain - x)) * inverse;
    }
  }
}
