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
    <div className="p-3.5 rounded border border-border bg-card relative">
      <div className="flex items-center gap-1.5 text-muted-foreground text-xs font-sans font-semibold mb-1">
        {Icon && <Icon className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />}
        <span>{label}</span>
      </div>
      <p className={`text-2xl font-bold font-sans ${valueClass || 'text-foreground'}`}>
        {value}{unit && <span className="text-xs text-muted-foreground ml-1 font-normal font-mono">{unit}</span>}
      </p>
      {sub && <p className="text-[11px] text-muted-foreground mt-1 font-mono">{sub}</p>}
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

export function MiniSparkline({ data, color = 'hsl(var(--blue-400))', w = 80, h = 28 }: {
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
    ctx.fillStyle = fill(0.2);
    ctx.fill();

    // stroke line
    ctx.beginPath();
    pts.forEach((v, i) => {
      const x = pad + i * xStep;
      const y = toY(v);
      if (i === 0) { ctx.moveTo(x, y); } else { ctx.lineTo(x, y); }
    });
    ctx.strokeStyle = stroke;
    ctx.lineWidth = 1.5;
    ctx.lineJoin = 'round';
    ctx.stroke();
  }, [data, color, w, h]);

  return <canvas ref={canvasRef} width={w} height={h} />;
}
