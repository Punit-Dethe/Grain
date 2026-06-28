import React, { useEffect, useRef } from "react";

export const DotMatrixSplash: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animationId: number;
    let width = canvas.width = window.innerWidth;
    let height = canvas.height = window.innerHeight;

    const handleResize = () => {
      width = canvas.width = window.innerWidth;
      height = canvas.height = window.innerHeight;
    };
    window.addEventListener("resize", handleResize);

    const spacing = 12; // High density dot spacing
    const text = "GRAIN";
    const activeDots = new Set<string>();

    let cols = Math.floor(width / spacing);
    let rows = Math.floor(height / spacing);

    // Offscreen canvas to render text and scan active pixels
    const offscreen = document.createElement("canvas");
    const offCtx = offscreen.getContext("2d");

    const scanText = () => {
      if (!offCtx) return;
      cols = Math.floor(width / spacing);
      rows = Math.floor(height / spacing);
      offscreen.width = cols;
      offscreen.height = rows;

      offCtx.clearRect(0, 0, cols, rows);
      offCtx.fillStyle = "black";
      
      // Calculate font size relative to grid width to ensure it fits nicely
      let fontSize = Math.floor(cols * 0.15); 
      fontSize = Math.max(8, Math.min(fontSize, 20)); // Clamp size
      
      // Syne font is a ultra-wide, futuristic font which looks awesome as a dot matrix
      offCtx.font = `800 ${fontSize}px Syne, sans-serif`;
      offCtx.textAlign = "center";
      offCtx.textBaseline = "middle";
      offCtx.fillText(text, cols / 2, rows / 2);

      const imgData = offCtx.getImageData(0, 0, cols, rows);
      activeDots.clear();
      for (let y = 0; y < rows; y++) {
        for (let x = 0; x < cols; x++) {
          const idx = (y * cols + x) * 4;
          if (imgData.data[idx + 3] > 128) {
            activeDots.add(`${x},${y}`);
          }
        }
      }
    };

    // Scan initially
    scanText();

    // Re-scan when fonts are loaded to ensure Syne is captured rather than a fallback sans-serif
    if ("fonts" in document) {
      document.fonts.ready.then(() => {
        scanText();
      });
    }

    const startTime = Date.now();

    const draw = () => {
      const elapsed = (Date.now() - startTime) / 1000;
      
      // Recalculate if dimensions changed
      const currentCols = Math.floor(width / spacing);
      const currentRows = Math.floor(height / spacing);
      if (currentCols !== cols || currentRows !== rows) {
        scanText();
      }

      // Beige background
      ctx.fillStyle = "#ece5da";
      ctx.fillRect(0, 0, width, height);

      // Centering calculations
      const startX = (width - (cols - 1) * spacing) / 2;
      const startY = (height - (rows - 1) * spacing) / 2;

      // Scanline sweep position (sweeps across the screen repeatedly)
      const sweepDuration = 3.5; // seconds per sweep
      const progress = (elapsed % sweepDuration) / sweepDuration;
      const waveX = progress * (width + 400) - 200;
      const waveWidth = 250;

      for (let y = 0; y < rows; y++) {
        for (let x = 0; x < cols; x++) {
          const dotX = startX + x * spacing;
          const dotY = startY + y * spacing;

          const isActive = activeDots.has(`${x},${y}`);
          const distToWave = Math.abs(dotX - waveX);
          const waveFactor = distToWave < waveWidth ? 1 - distToWave / waveWidth : 0;

          // Gentle ambient ripple
          const ripple = Math.sin(x * 0.12 + y * 0.15 + elapsed * 3.0) * 0.5 + 0.5;

          if (isActive) {
            // Sweep reveal: dots are revealed as the wave passes them
            const revealPos = dotX;
            const hasBeenSwept = waveX > revealPos || elapsed > 1.8; // Force reveal after 1.8s

            if (hasBeenSwept) {
              // Pulse/breathing effect
              const pulse = Math.sin(elapsed * 4.5 + x * 0.4) * 0.15 + 0.85;
              const radius = 2.0 * pulse;

              // Draw glow behind active text dots for premium depth
              ctx.beginPath();
              ctx.arc(dotX, dotY, radius * 2.5, 0, Math.PI * 2);
              ctx.fillStyle = `rgba(240, 96, 0, ${0.15 * pulse})`;
              ctx.fill();

              // Draw solid dot (dark ink)
              ctx.beginPath();
              ctx.arc(dotX, dotY, radius, 0, Math.PI * 2);
              ctx.fillStyle = "#141210";
              ctx.fill();
            } else {
              // Ambient background state before the sweep reveals it
              ctx.beginPath();
              ctx.arc(dotX, dotY, 1.0 + ripple * 0.4, 0, Math.PI * 2);
              ctx.fillStyle = `rgba(20, 18, 16, ${0.04 + ripple * 0.04})`;
              ctx.fill();
            }
          } else {
            // Inactive dot
            let radius = 1.0 + ripple * 0.4;
            let color = `rgba(20, 18, 16, ${0.04 + ripple * 0.04})`;

            if (waveFactor > 0) {
              // Animate background dots as the sweep wave passes
              radius += waveFactor * 1.5;
              color = `rgba(240, 96, 0, ${0.05 + waveFactor * 0.25})`;

              // Optional small sweep trail glow
              ctx.beginPath();
              ctx.arc(dotX, dotY, radius * 2.0, 0, Math.PI * 2);
              ctx.fillStyle = `rgba(240, 96, 0, ${0.05 * waveFactor})`;
              ctx.fill();
            }

            ctx.beginPath();
            ctx.arc(dotX, dotY, radius, 0, Math.PI * 2);
            ctx.fillStyle = color;
            ctx.fill();
          }
        }
      }

      animationId = requestAnimationFrame(draw);
    };

    draw();

    return () => {
      cancelAnimationFrame(animationId);
      window.removeEventListener("resize", handleResize);
    };
  }, []);

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        backgroundColor: "#ece5da",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 9999,
      }}
    >
      <canvas
        ref={canvasRef}
        style={{
          display: "block",
          width: "100%",
          height: "100%",
        }}
      />
    </div>
  );
};
