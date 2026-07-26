import React, { useLayoutEffect, useRef } from "react";

interface ScaledStageProps {
  /** The fixed design width the children are authored at (px). */
  designWidth: number;
  /** The fixed design height the children are authored at (px). */
  designHeight: number;
  /**
   * Ceiling on the scale factor. Unbounded by default. A fixed-composition
   * surface wants one: past a point "bigger" stops meaning "better fitted" and
   * starts meaning a billboard, and the extra space is better left as chassis.
   */
  maxScale?: number;
  children: React.ReactNode;
  className?: string;
}

/**
 * [GRAIN] Immaculate, look-preserving scaling — for FIXED-COMPOSITION surfaces.
 *
 * Children are authored ONCE at a fixed `designWidth × designHeight` canvas (in
 * absolute px — every font, gap, radius and border is a real pixel value). This
 * component measures its container and applies a single `transform: scale()` so
 * the whole canvas grows/shrinks as one unit to fit the window, centered. Because
 * it's a uniform transform (not a reflow), the layout is pixel-identical at any
 * size — nothing wraps, nothing squashes, the look is exactly preserved.
 *
 * "contain" fit: scale = min(w/designW, h/designH), so the canvas always fits
 * fully; any leftover space becomes symmetric margin filled by the window bg.
 *
 * [GRAIN] SCOPE: the Quick Panel console, and nothing else. The settings view
 * used to be wrapped in this too, with the main window locked to the same aspect
 * ratio to hide the letterboxing. That is gone: Grain Note lives in the main
 * window now, and scaling a text editor up renders the same few lines twice as
 * large instead of showing more of the note. Reach for this ONLY when the surface
 * is an instrument face with no scrolling content — otherwise write a fluid
 * layout.
 */
export const ScaledStage: React.FC<ScaledStageProps> = ({
  designWidth,
  designHeight,
  maxScale,
  children,
  className,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const innerRef = useRef<HTMLDivElement>(null);

  // [GRAIN] Write the scale straight to the DOM in the ResizeObserver callback —
  // NEVER through React state. Routing it through setState re-rendered the entire
  // canvas a frame (or more) behind the window, so the content visibly lagged and
  // jittered during a live drag-resize. A direct transform write tracks the window
  // synchronously and re-renders nothing. useLayoutEffect applies it before the
  // first paint so there's no unscaled flash.
  useLayoutEffect(() => {
    const el = containerRef.current;
    const inner = innerRef.current;
    if (!el || !inner) return;

    const apply = () => {
      const { width, height } = el.getBoundingClientRect();
      if (width === 0 || height === 0) return;
      let scale = Math.min(width / designWidth, height / designHeight);
      if (maxScale != null) scale = Math.min(scale, maxScale);
      inner.style.transform = `scale(${scale})`;
    };

    apply();
    const ro = new ResizeObserver(apply);
    ro.observe(el);
    return () => ro.disconnect();
  }, [designWidth, designHeight, maxScale]);

  return (
    <div
      ref={containerRef}
      className={`relative w-full h-full overflow-hidden flex items-center justify-center ${
        className ?? ""
      }`}
    >
      <div
        ref={innerRef}
        style={{
          width: designWidth,
          height: designHeight,
          flex: "none",
          transformOrigin: "center center",
          willChange: "transform",
        }}
      >
        {children}
      </div>
    </div>
  );
};
