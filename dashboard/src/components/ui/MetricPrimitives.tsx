import React, { useState, useEffect, useRef } from 'react';

export function MetricCard({ icon: Icon, label, value, unit, sub, valueClass }: {
  icon?: React.ComponentType<{ className?: string }>;
  label: string;
  value: React.ReactNode;
  unit?: string;
  sub?: string;
  valueClass?: string;
}) {
  return (
    <div className="p-2 rounded-sm border border-[#ACA899] dark:border-border bg-white dark:bg-card relative">
      <div className="flex items-center justify-between text-[11px] font-bold text-[#01285F] dark:text-primary mb-1">
        <span className="truncate">{label}</span>
        {Icon && <Icon className="w-3.5 h-3.5 opacity-60 shrink-0" aria-hidden="true" />}
      </div>
      <p className={`text-base font-bold font-mono leading-tight ${valueClass || 'text-[#000000] dark:text-foreground'}`}>
        {value}{unit && <span className="text-[11px] text-[#555555] dark:text-muted-foreground ml-1 font-normal font-mono">{unit}</span>}
      </p>
      {sub && <p className="text-[10px] text-[#555555] dark:text-muted-foreground mt-1 font-mono truncate">{sub}</p>}
    </div>
  );
}

export function RelativeTime({ ts }: { ts: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  const diff = Math.floor((now - ts) / 1000);
  if (diff < 60) return <>{diff}s</>;
  return <>{Math.floor(diff / 60)}m</>;
}

import { resolveCanvasColor } from '../../lib/canvas-color';

export function MiniSparkline({ data, color = 'hsl(var(--blue-400))', w = 80, h = 24 }: {
  data: number[];
  color?: string;
  w?: number;
  h?: number;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, w, h);

    const { stroke, fill } = resolveCanvasColor(color, canvas);

    const pts = data.length > 0 ? data : [0];
    const minV = Math.min(...pts);
    const maxV = Math.max(...pts);
    const range = maxV - minV || 1;
    const pad = 2;

    const xStep = pts.length > 1 ? (w - pad * 2) / (pts.length - 1) : 0;
    const toY = (v: number) => pad + ((1 - (v - minV) / range) * (h - pad * 2));

    ctx.beginPath();
    pts.forEach((v, i) => {
      const x = pad + i * xStep;
      const y = toY(v);
      if (i === 0) { ctx.moveTo(x, y); } else { ctx.lineTo(x, y); }
    });

    // filled area
    ctx.lineTo(pad + (pts.length - 1) * xStep, h - pad);
    ctx.lineTo(pad, h - pad);
    ctx.closePath();
    ctx.fillStyle = fill(0.15);
    ctx.fill();

    // stroke line
    ctx.beginPath();
    pts.forEach((v, i) => {
      const x = pad + i * xStep;
      const y = toY(v);
      if (i === 0) { ctx.moveTo(x, y); } else { ctx.lineTo(x, y); }
    });
    ctx.strokeStyle = stroke;
    ctx.lineWidth = 1.2;
    ctx.lineJoin = 'round';
    ctx.stroke();
  }, [data, color, w, h]);

  return <canvas ref={canvasRef} width={w} height={h} />;
}
