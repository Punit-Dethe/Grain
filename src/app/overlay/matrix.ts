// Grain's original 25×8 density field and two-row live strip, without controls.
const edge = (x: number, y: number) =>
  x === 0 ||
  x === 24 ||
  ((x === 1 || x === 23) && (y < 2 || y > 5)) ||
  ((x === 2 || x === 22) && (y < 1 || y > 6));

export class MatrixField {
  readonly dots = new Uint8Array(25 * 8 * 4);
  private readonly eligible = Uint16Array.from(
    Array.from({ length: 200 }, (_, i) => i).filter(
      (i) => !edge(i % 25, Math.floor(i / 25)),
    ),
  );
  private energy = 0;
  private clock = 0;

  reset() {
    this.dots.fill(0);
    this.energy = this.clock = 0;
  }

  private paint(index: number, r: number, g: number, b: number, a: number) {
    const start = index * 4;
    this.dots[start] = r;
    this.dots[start + 1] = g;
    this.dots[start + 2] = b;
    this.dots[start + 3] = a;
  }

  advance(seconds: number, amplitude: number, live: boolean) {
    this.clock += Math.max(0, Math.min(0.1, seconds));
    if (this.clock < 1 / 30) return false;
    this.clock %= 1 / 30;
    const amp = Math.max(0, Math.min(1, amplitude));
    this.energy =
      live && amp <= this.energy
        ? this.energy * 0.7 + amp * 0.3
        : this.energy * 0.35 + amp * 0.65;
    this.dots.fill(0);
    if (live) {
      const reach = Math.sqrt(this.energy) * 12;
      for (let y = 3; y <= 4; y++)
        for (let x = 1; x < 24; x++) {
          const distance = Math.abs(x - 12);
          if (distance > reach) continue;
          const alpha = Math.min(
            0.92,
            0.3 +
              (1 - distance / Math.max(0.01, reach)) * 0.55 +
              Math.random() * 0.08,
          );
          this.paint(y * 25 + x, 200, 204, 212, Math.floor(alpha * 255));
        }
    } else {
      const ratio = Math.max(
        0,
        Math.min(
          0.94,
          this.energy + (this.energy > 0.001 ? (Math.random() - 0.5) * 0.1 : 0),
        ),
      );
      const active = Math.round(this.eligible.length * ratio);
      const hot = Math.round(active * 0.08);
      for (let i = this.eligible.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        const value = this.eligible[i];
        this.eligible[i] = this.eligible[j];
        this.eligible[j] = value;
      }
      for (const index of this.eligible) this.paint(index, 12, 12, 12, 255);
      for (let i = 0; i < active; i++) {
        const index = this.eligible[i];
        if (i < hot) this.paint(index, 189, 193, 201, 235);
        else {
          const alpha = Math.min(
            0.82,
            0.34 + this.energy * 0.3 + Math.random() * 0.1,
          );
          const shade = Math.random();
          if (shade < 0.33)
            this.paint(index, 168, 174, 184, Math.floor(alpha * 255));
          else if (shade < 0.66)
            this.paint(index, 140, 148, 160, Math.floor(alpha * 255));
          else this.paint(index, 200, 204, 212, Math.floor(alpha * 255));
        }
      }
      this.energy *= 0.74;
    }
    return true;
  }
}
