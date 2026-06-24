import React, { useEffect, useRef } from "react";

/**
 * "GRAIN//" LED dot-matrix logo — ported 1:1 from DotMatrixDisplay.qml.
 * Rasterises the bold wordmark onto a canvas, samples a regular dot grid, and
 * paints lit amber dots inside the glyphs (faint dots elsewhere) with an edge
 * vignette and a soft bloom. Static (no animation), repainted on resize.
 */
export const DotMatrix: React.FC<{ dotColor?: string }> = ({
  dotColor = "#FF8A1E",
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap) return;

    // Parse the dot colour to rgb once.
    const tmp = document.createElement("canvas").getContext("2d")!;
    tmp.fillStyle = dotColor;
    const hex = tmp.fillStyle as string; // normalised "#rrggbb"
    const dr = parseInt(hex.slice(1, 3), 16);
    const dg = parseInt(hex.slice(3, 5), 16);
    const db = parseInt(hex.slice(5, 7), 16);
    const rgba = (a: number) => `rgba(${dr},${dg},${db},${a})`;

    const paint = () => {
      const W = Math.round(wrap.clientWidth);
      const H = Math.round(wrap.clientHeight);
      if (W <= 0 || H <= 0) return;
      canvas.width = W;
      canvas.height = H;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      // 1. Rasterise the wordmark with a bold (700) font.
      ctx.clearRect(0, 0, W, H);
      const text = "GRAIN//";
      let fontPx = H * 0.7;
      ctx.font = `700 ${fontPx}px 'Arial','Helvetica',sans-serif`;
      const tw = ctx.measureText(text).width;
      const maxW = W * 0.9;
      if (tw > maxW) {
        fontPx = (fontPx * maxW) / tw;
        ctx.font = `700 ${fontPx}px 'Arial','Helvetica',sans-serif`;
      }
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillStyle = "#ffffff";
      ctx.fillText(text, W / 2, H / 2);

      const imageData = ctx.getImageData(0, 0, W, H);
      const data = imageData.data;
      const IW = imageData.width;
      const IH = imageData.height;
      ctx.clearRect(0, 0, W, H);

      // 2. Measure the glyph bbox → true centring offset.
      let minX = IW;
      let minY = IH;
      let maxX = -1;
      let maxY = -1;
      for (let y = 0; y < IH; y++) {
        const rowBase = y * IW;
        for (let x = 0; x < IW; x++) {
          if (data[(rowBase + x) * 4 + 3] > 50) {
            if (x < minX) minX = x;
            if (x > maxX) maxX = x;
            if (y < minY) minY = y;
            if (y > maxY) maxY = y;
          }
        }
      }
      let offX = 0;
      let offY = 0;
      if (maxX >= 0) {
        offX = W / 2 - (minX + maxX) / 2;
        offY = H / 2 - (minY + maxY) / 2;
      }
      const coverAt = (px: number, py: number) => {
        const xi = Math.round(px - offX);
        const yi = Math.round(py - offY);
        if (xi < 0 || yi < 0 || xi >= IW || yi >= IH) return 0;
        return data[(yi * IW + xi) * 4 + 3] / 255;
      };

      // 3. Regular dot grid.
      const step = Math.max(2.8, H / 40);
      const cols = Math.max(1, Math.floor(W / step));
      const rows = Math.max(1, Math.floor(H / step));
      const gx = (W - cols * step) / 2 + step / 2;
      const gy = (H - rows * step) / 2 + step / 2;

      const mx = W * 0.12;
      const my = H * 0.14;
      const edgeFade = (px: number, py: number) => {
        const fx = Math.min(px, W - px) / mx;
        const fy = Math.min(py, H - py) / my;
        return Math.max(0, Math.min(1, Math.min(fx, fy)));
      };

      const softDot = (cx: number, cy: number, radius: number, alpha: number) => {
        const grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, radius);
        grad.addColorStop(0.0, rgba(alpha));
        grad.addColorStop(0.6, rgba(alpha * 0.78));
        grad.addColorStop(1.0, rgba(0));
        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.arc(cx, cy, radius, 0, Math.PI * 2);
        ctx.fill();
      };

      const litR = step * 0.46;
      const bgR = step * 0.42;
      const litX: number[] = [];
      const litY: number[] = [];
      const litB: number[] = [];

      for (let r = 0; r < rows; r++) {
        for (let c = 0; c < cols; c++) {
          const px = gx + c * step;
          const py = gy + r * step;
          const fade = edgeFade(px, py);
          const cov = coverAt(px, py);
          if (cov > 0.5) {
            litX.push(px);
            litY.push(py);
            litB.push(
              Math.max(0.85, Math.min(1.0, cov)) * Math.max(0.5, fade),
            );
          } else if (fade > 0.002) {
            softDot(px, py, bgR, 0.07 * fade);
          }
        }
      }
      for (let b = 0; b < litX.length; b++) {
        softDot(litX[b], litY[b], step * 1.15, 0.1 * litB[b]);
      }
      for (let i = 0; i < litX.length; i++) {
        softDot(litX[i], litY[i], litR, litB[i]);
      }
    };

    paint();
    const ro = new ResizeObserver(paint);
    ro.observe(wrap);
    // Repaint once fonts are ready (glyph metrics depend on the loaded font).
    document.fonts?.ready.then(paint).catch(() => {});
    return () => ro.disconnect();
  }, [dotColor]);

  return (
    <div ref={wrapRef} className="w-full h-full">
      <canvas ref={canvasRef} className="block w-full h-full" />
    </div>
  );
};
