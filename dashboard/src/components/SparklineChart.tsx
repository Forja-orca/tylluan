import React, { useRef, useEffect, useCallback } from 'react';

interface SparklineChartProps {
  data: number[];       // values 0–100
  color: string;        // hex color for the line
  label: string;        // text top-left
  unit?: string;        // "%", "ms", etc.
  height?: number;      // px, default 48
  showLast?: boolean;   // show current value top-right
}

export function SparklineChart({
  data,
  color,
  label,
  unit = '',
  height = 48,
  showLast = false,
}: SparklineChartProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const cssWidth = container.clientWidth;
    const cssHeight = height;

    // Resize canvas buffer if needed
    if (canvas.width !== cssWidth * dpr || canvas.height !== cssHeight * dpr) {
      canvas.width = cssWidth * dpr;
      canvas.height = cssHeight * dpr;
    }
    canvas.style.width = `${cssWidth}px`;
    canvas.style.height = `${cssHeight}px`;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssWidth, cssHeight);

    const W = cssWidth;
    const H = cssHeight;
    const PAD_T = 4;
    const PAD_B = 4;
    const drawH = H - PAD_T - PAD_B;

    // Normalise data — if empty, draw flat line at 50%
    const points: number[] = data.length > 0 ? data : [50];
    const n = points.length;

    // Map value (0-100) to Y coordinate
    const toY = (v: number) => PAD_T + drawH * (1 - Math.max(0, Math.min(100, v)) / 100);
    // Map index to X coordinate
    const toX = (i: number) => n === 1 ? W / 2 : (i / (n - 1)) * W;

    // Build path
    const pathX: number[] = points.map((_, i) => toX(i));
    const pathY: number[] = points.map((v) => toY(v));

    // --- Filled area ---
    ctx.beginPath();
    // Start at bottom-left
    ctx.moveTo(pathX[0], H - PAD_B);
    // Line up to first point
    ctx.lineTo(pathX[0], pathY[0]);

    if (n === 1) {
      ctx.lineTo(pathX[0], pathY[0]);
    } else {
      // Smooth line using quadratic bezier segments
      for (let i = 1; i < n; i++) {
        const cpX = (pathX[i - 1] + pathX[i]) / 2;
        ctx.bezierCurveTo(
          cpX, pathY[i - 1],
          cpX, pathY[i],
          pathX[i], pathY[i]
        );
      }
    }

    // Close area: down to bottom-right, then across to bottom-left
    ctx.lineTo(pathX[n - 1], H - PAD_B);
    ctx.closePath();

    // Fill gradient
    const grad = ctx.createLinearGradient(0, PAD_T, 0, H - PAD_B);
    // Parse hex color to rgba
    const r = parseInt(color.slice(1, 3), 16);
    const g = parseInt(color.slice(3, 5), 16);
    const b = parseInt(color.slice(5, 7), 16);
    grad.addColorStop(0, `rgba(${r},${g},${b},0.25)`);
    grad.addColorStop(1, `rgba(${r},${g},${b},0.02)`);
    ctx.fillStyle = grad;
    ctx.fill();

    // --- Line stroke ---
    ctx.beginPath();
    ctx.moveTo(pathX[0], pathY[0]);
    if (n > 1) {
      for (let i = 1; i < n; i++) {
        const cpX = (pathX[i - 1] + pathX[i]) / 2;
        ctx.bezierCurveTo(
          cpX, pathY[i - 1],
          cpX, pathY[i],
          pathX[i], pathY[i]
        );
      }
    }
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.lineJoin = 'round';
    ctx.lineCap = 'round';
    ctx.stroke();
  }, [data, color, height]);

  // Draw on mount and when data changes
  useEffect(() => {
    draw();
  }, [draw]);

  // ResizeObserver for responsive redraws
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const ro = new ResizeObserver(() => draw());
    ro.observe(container);
    return () => ro.disconnect();
  }, [draw]);

  const lastVal = data.length > 0 ? data[data.length - 1] : null;

  return (
    <div ref={containerRef} className="relative w-full" style={{ height }}>
      {/* Label top-left */}
      <span
        className="absolute top-0 left-0 text-[10px] font-bold uppercase tracking-widest pointer-events-none z-10"
        style={{ color, opacity: 0.85, lineHeight: 1 }}
      >
        {label}
      </span>
      {/* Current value top-right */}
      {showLast && lastVal !== null && (
        <span
          className="absolute top-0 right-0 text-[10px] font-mono font-bold pointer-events-none z-10"
          style={{ color, opacity: 0.9, lineHeight: 1 }}
        >
          {lastVal.toFixed(1)}{unit}
        </span>
      )}
      <canvas
        ref={canvasRef}
        className="absolute inset-0 w-full h-full"
        aria-label={`${label} sparkline chart`}
      />
    </div>
  );
}
