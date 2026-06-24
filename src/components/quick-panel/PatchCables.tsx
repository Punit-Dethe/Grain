import React, { useLayoutEffect, useRef, useState } from "react";

/** The default signal chain: A.out → B.in (orange), B.out → C.in (green). */
const CONNECTIONS = [
  { from: "moduleA.output", to: "moduleB.input", color: "#FF5D1E" },
  { from: "moduleB.output", to: "moduleC.input", color: "#10B981" },
];

interface Cable {
  d: string;
  color: string;
  a: { x: number; y: number };
  b: { x: number; y: number };
}

/**
 * Patch-cable overlay. Draws drooping bezier cords between jack centers, in the
 * panel's fixed 1280×760 design space (the SVG shares the scaled stage's
 * transform, so coordinates are pre-scale design px). Recomputes on layout
 * changes. Decorative for now (pointer-events: none); drag-to-rewire comes with
 * the wiring pass.
 */
export const PatchCables: React.FC = () => {
  const svgRef = useRef<SVGSVGElement>(null);
  const [cables, setCables] = useState<Cable[]>([]);

  useLayoutEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;

    const compute = () => {
      const r = svg.getBoundingClientRect();
      if (r.width === 0) return;
      const scale = r.width / 1280;
      const centerOf = (id: string) => {
        const el = document.querySelector(`[data-jack-id="${id}"]`);
        if (!el) return null;
        const jr = el.getBoundingClientRect();
        return {
          x: (jr.left + jr.width / 2 - r.left) / scale,
          y: (jr.top + jr.height / 2 - r.top) / scale,
        };
      };

      const next: Cable[] = [];
      for (const c of CONNECTIONS) {
        const a = centerOf(c.from);
        const b = centerOf(c.to);
        if (!a || !b) continue;
        const dist = Math.hypot(b.x - a.x, b.y - a.y);
        const sag = Math.min(140, Math.max(36, dist * 0.45));
        next.push({
          color: c.color,
          a,
          b,
          d: `M ${a.x} ${a.y} C ${a.x} ${a.y + sag}, ${b.x} ${b.y + sag}, ${b.x} ${b.y}`,
        });
      }
      setCables(next);
    };

    compute();
    const ro = new ResizeObserver(compute);
    ro.observe(svg);
    const t = window.setTimeout(compute, 60);
    window.addEventListener("resize", compute);
    document.fonts?.ready.then(compute).catch(() => {});
    return () => {
      ro.disconnect();
      window.clearTimeout(t);
      window.removeEventListener("resize", compute);
    };
  }, []);

  return (
    <svg
      ref={svgRef}
      className="absolute inset-0 w-full h-full pointer-events-none"
      viewBox="0 0 1280 760"
      preserveAspectRatio="none"
      style={{ zIndex: 30 }}
    >
      {cables.map((c, i) => (
        <g key={i}>
          {/* drop shadow */}
          <path
            d={c.d}
            transform="translate(0,2)"
            stroke="rgba(0,0,0,0.30)"
            strokeWidth={7}
            fill="none"
            strokeLinecap="round"
          />
          {/* cable body */}
          <path
            d={c.d}
            stroke={c.color}
            strokeWidth={5}
            fill="none"
            strokeLinecap="round"
          />
          {/* faint top highlight */}
          <path
            d={c.d}
            stroke="rgba(255,255,255,0.25)"
            strokeWidth={1.4}
            fill="none"
            strokeLinecap="round"
          />
          {/* plug ends */}
          <circle cx={c.a.x} cy={c.a.y} r={4.5} fill={c.color} />
          <circle cx={c.b.x} cy={c.b.y} r={4.5} fill={c.color} />
        </g>
      ))}
    </svg>
  );
};
