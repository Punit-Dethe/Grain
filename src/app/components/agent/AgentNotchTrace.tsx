import { useLayoutEffect, useState, type RefObject } from "react";

/** Stroke width of the hover trace, in CSS px. */
const STROKE = 1.5;
/** Dark rim kept between the trace and the shell's edge, so the line never
 * touches the desktop behind and stays visible on a light background
 * (TRACE_INSET in grain_notch_sliver.rs). */
const INSET = 1.5;

type Box = { w: number; h: number; r: number };

/** The two halves of the shell's outline, inset by [`INSET`], each starting
 * at the bottom centre and running round a bottom corner and up its side to
 * the top edge. */
function halves({ w, h, r }: Box): [string, string] {
  const o = INSET + STROKE / 2;
  const cx = w / 2;
  const radius = Math.min(r, h, cx);
  const ri = Math.max(radius - o, 0);
  const bottom = h - o;
  const corner = h - radius;
  return [
    `M ${cx} ${bottom} H ${radius} A ${ri} ${ri} 0 0 1 ${o} ${corner} V 0`,
    `M ${cx} ${bottom} H ${w - radius} A ${ri} ${ri} 0 0 0 ${w - o} ${corner} V 0`,
  ];
}

/** Hover timer for a listening or working notch: a light line traces the
 * shell's outline from the bottom centre up both sides to the top edge
 * (BAR and FILL in grain_notch_sliver.rs draw the same on the native sliver).
 * Mounted only while an early open is possible, so its size observer lives
 * no longer than that. */
export function AgentNotchTrace({
  shell,
}: {
  shell: RefObject<HTMLElement | null>;
}) {
  const [box, setBox] = useState<Box | null>(null);

  useLayoutEffect(() => {
    const element = shell.current;
    if (!element) return;
    const measure = () => {
      const r =
        parseFloat(getComputedStyle(element).borderBottomLeftRadius) || 0;
      const next = { w: element.clientWidth, h: element.clientHeight, r };
      setBox((prev) =>
        prev && prev.w === next.w && prev.h === next.h && prev.r === next.r
          ? prev
          : next,
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [shell]);

  if (!box || box.w <= 0 || box.h <= 0) return null;
  const [left, right] = halves(box);
  return (
    <svg
      className="agent-notch-trace"
      width={box.w}
      height={box.h}
      viewBox={`0 0 ${box.w} ${box.h}`}
      aria-hidden
    >
      <path className="agent-notch-trace-track" d={left} />
      <path className="agent-notch-trace-track" d={right} />
      <path className="agent-notch-trace-line" d={left} pathLength={1} />
      <path className="agent-notch-trace-line" d={right} pathLength={1} />
    </svg>
  );
}
