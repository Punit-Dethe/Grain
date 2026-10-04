import { type UnlistenFn } from "@tauri-apps/api/event";
import { memo, useEffect, useRef, useState } from "react";
import { events } from "@/bindings";
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

export const Waveform = memo(function Waveform({
  ready,
  session,
}: {
  ready: boolean;
  session: number;
}) {
  const row = useRef<HTMLDivElement>(null);
  const amplitude = useRef(0);
  // Original compact icon slot leaves ~50.75px for 13 bars; use it in both forms.
  const count = barCount(WAVE_WIDTH);
  const [field] = useState(() => new WaveField());
  useEffect(() => {
    let disposed = false;
    let release: UnlistenFn | undefined;
    void events.micLevel
      .listen(({ payload }) => {
        if (!disposed) amplitude.current = audioAmplitude(payload);
      })
      .then((fn) => {
        if (disposed) fn();
        else release = fn;
      })
      .catch(console.error);
    return () => {
      disposed = true;
      release?.();
    };
  }, []);
  useEffect(() => {
    field.reset();
    amplitude.current = 0;
  }, [session, field]);
  useEffect(() => {
    const element = row.current;
    if (!element) return;
    const bars = element.querySelectorAll("line");
    let frame = 0;
    let previous = performance.now();
    const animate = (now: number) => {
      const dt = (now - previous) / 1000;
      field.advance(dt, ready ? amplitude.current : 0, count);
      previous = now;
      for (let i = 0; i < bars.length; i++) {
        const half = waveBarHalfLength(field.bars[i]);
        bars[i].setAttribute("y1", String(WAVE_HEIGHT / 2 - half));
        bars[i].setAttribute("y2", String(WAVE_HEIGHT / 2 + half));
      }
      frame = requestAnimationFrame(animate);
    };
    frame = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(frame);
  }, [count, ready, field]);
  return (
    <div
      ref={row}
      className={`swave grain-wave ${ready ? "ready" : "arming"}`}
      aria-hidden="true"
    >
      <svg
        width={WAVE_WIDTH}
        height={WAVE_HEIGHT}
        viewBox={`0 0 ${WAVE_WIDTH} ${WAVE_HEIGHT}`}
        stroke="currentColor"
        strokeWidth={WAVE_BAR_WIDTH}
        strokeLinecap="round"
      >
        {Array.from({ length: count }, (_, i) => (
          <line
            key={i}
            x1={waveBarX(i, count)}
            x2={waveBarX(i, count)}
            y1={WAVE_HEIGHT / 2}
            y2={WAVE_HEIGHT / 2}
          />
        ))}
      </svg>
    </div>
  );
});
