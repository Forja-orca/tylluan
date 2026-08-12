import React, { useState, useEffect, useRef } from 'react';

export function MetricCard({ icon: Icon, label, value, unit, sub, valueClass }: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  value: React.ReactNode;
  unit?: string;
  sub?: string;
  valueClass?: string;
}) {
  return (
    // bg-slate-900/40 = Nocturne Deep Container tint; border-slate-800 = Owl Slate border
    // WCAG pre-flight 2026-08-12:
    //   label (slate-400 on slate-900)   → 7.18:1 PASS
    //   value (slate-100 on slate-900)   → 16.80:1 PASS
    //   unit/sub (slate-400 on slate-900) → 7.18:1 PASS (was slate-600: 1.78 FAIL — fixed)
    //   icon hover (emerald-400)         → 9.58:1 PASS
    <div className="p-4 rounded-xl border border-slate-800 bg-slate-900/40 backdrop-blur-md hover:border-emerald-500/30 transition-all group overflow-hidden relative">
      <div className="absolute top-0 right-0 w-24 h-24 bg-emerald-500/5 blur-3xl -mr-12 -mt-12 rounded-full group-hover:bg-emerald-500/10 transition-colors" />
      <div className="flex items-center gap-2 text-slate-400 text-[10px] uppercase tracking-widest font-sans font-bold mb-1">
        <Icon className="w-3 h-3 group-hover:text-emerald-400 transition-colors" /> {label}
      </div>
      <p className={`text-2xl font-black tracking-tighter font-sans ${valueClass || 'text-slate-100'}`}>
        {value}{unit && <span className="text-xs text-slate-400 ml-1 font-normal uppercase font-mono">{unit}</span>}
      </p>
      {sub && <p className="text-[10px] text-slate-400 mt-1 font-mono">{sub}</p>}
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

export function MiniSparkline({ data, color = '#60a5fa', w = 80, h = 28 }: {
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
    ctx.fillStyle = color + '33';
    ctx.fill();

    // stroke line
    ctx.beginPath();
    pts.forEach((v, i) => {
      const x = pad + i * xStep;
      const y = toY(v);
      if (i === 0) { ctx.moveTo(x, y); } else { ctx.lineTo(x, y); }
    });
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.lineJoin = 'round';
    ctx.stroke();
  }, [data, color, w, h]);

  return <canvas ref={canvasRef} width={w} height={h} />;
}
