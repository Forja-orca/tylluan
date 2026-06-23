import React, { useState, useEffect } from 'react';
import { Activity, ShieldAlert, Cpu, Database, Heart, Clock } from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

interface HomeostasisWidgetProps {
  bridge: NexusBridge | null;
}

export function HomeostasisWidget({ bridge }: HomeostasisWidgetProps) {
  const [health, setHealth] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!bridge) return;
    const fetchHealth = async () => {
      try {
        const h = await bridge.health_detailed();
        setHealth(h);
      } catch (e) {
        console.error('Failed to fetch detailed health in HomeostasisWidget:', e);
      } finally {
        setLoading(false);
      }
    };
    fetchHealth();
    // Poll detailed health every 10 seconds as per requirement
    const interval = setInterval(fetchHealth, 10000);
    return () => clearInterval(interval);
  }, [bridge]);

  // Calculate homeostasis score: guilds_ok * 40 + memory_ok * 30 + no_recent_errors * 30
  const guildsOk = health?.components?.guilds?.ok ? 1 : 0;
  const memoryOk = health?.components?.silva?.ok ? 1 : 0;
  const noRecentErrors = health?.status === 'healthy' ? 1 : 0;
  
  const score = Math.round((guildsOk * 40) + (memoryOk * 30) + (noRecentErrors * 30));

  // Determine color theme
  let statusColor = "text-emerald-400";
  let statusBg = "bg-emerald-500/10";
  let statusBorder = "border-emerald-500/20";
  let strokeColor = "#34d399";
  let glowColor = "rgba(52, 211, 153, 0.4)";

  if (score < 50) {
    statusColor = "text-red-400 animate-pulse";
    statusBg = "bg-red-500/10";
    statusBorder = "border-red-500/20";
    strokeColor = "#f87171";
    glowColor = "rgba(248, 113, 113, 0.4)";
  } else if (score < 80) {
    statusColor = "text-amber-400";
    statusBg = "bg-amber-500/10";
    statusBorder = "border-amber-500/20";
    strokeColor = "#fbbf24";
    glowColor = "rgba(251, 191, 36, 0.4)";
  }

  // SVG circle calculations
  const radius = 38;
  const circumference = 2 * Math.PI * radius;
  const strokeDashoffset = circumference - (score / 100) * circumference;

  return (
    <div className="rounded-2xl border border-slate-800 bg-slate-900/40 p-5 shadow-xl relative overflow-hidden group">
      {/* Background radial gradient glow */}
      <div 
        className="absolute -top-12 -right-12 w-36 h-36 rounded-full filter blur-2xl opacity-10 pointer-events-none transition-all duration-1000"
        style={{ background: strokeColor }}
      />
      
      <div className="flex flex-col sm:flex-row items-center gap-6 justify-between relative z-10">
        {/* Left Side: Text and status description */}
        <div className="space-y-3 text-center sm:text-left flex-1">
          <div className="flex items-center gap-2 justify-center sm:justify-start">
            <Heart className={cn("w-4 h-4", statusColor)} />
            <span className="text-[10px] font-bold uppercase tracking-[0.2em] text-slate-400">Homeostasis del Sistema</span>
          </div>
          <div>
            <h3 className="text-lg font-black text-slate-100 tracking-tight">Tylluan Cortex</h3>
            <p className="text-xs text-slate-500 leading-relaxed max-w-sm mt-1">
              Indicador unificado de la integridad del hub local. Monitorea guilds, bases de datos y fallos en tiempo real.
            </p>
          </div>
          
          {/* Status badge */}
          <div className={cn("inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full border text-[9px] font-bold uppercase tracking-widest", statusBg, statusBorder, statusColor)}>
            <div className={cn("w-1.5 h-1.5 rounded-full", score >= 80 ? "bg-emerald-400" : score >= 50 ? "bg-amber-400 animate-pulse" : "bg-red-500 animate-ping")} />
            {loading ? "Calculating..." : health?.status === "healthy" ? "Cortex Stable" : "System Strain"}
          </div>
        </div>

        {/* Right Side: Circular Gauge */}
        <div className="flex flex-col items-center shrink-0">
          <div className="relative w-24 h-24 flex items-center justify-center">
            {/* SVG Circle Gauge */}
            <svg className="w-full h-full transform -rotate-90">
              <circle
                cx="48"
                cy="48"
                r={radius}
                className="stroke-slate-800"
                strokeWidth="6"
                fill="transparent"
              />
              <circle
                cx="48"
                cy="48"
                r={radius}
                stroke={strokeColor}
                strokeWidth="6"
                fill="transparent"
                strokeDasharray={circumference}
                strokeDashoffset={loading ? circumference : strokeDashoffset}
                strokeLinecap="round"
                className="transition-all duration-1000 ease-out"
                style={{ filter: `drop-shadow(0 0 4px ${glowColor})` }}
              />
            </svg>
            {/* Score inside circular gauge */}
            <div className="absolute flex flex-col items-center justify-center">
              <span className="text-xl font-black font-mono text-slate-100">{loading ? "—" : score}</span>
              <span className="text-[7px] text-slate-600 font-bold uppercase tracking-widest">Score</span>
            </div>
          </div>
        </div>
      </div>

      {/* Metrics Row below */}
      <div className="grid grid-cols-3 gap-2 mt-5 pt-4 border-t border-slate-800/60 relative z-10">
        {/* Metric 1: Guilds */}
        <div className="bg-slate-950/40 border border-slate-800/40 rounded-xl p-2.5 text-center">
          <Cpu className="w-3.5 h-3.5 text-blue-400 mx-auto mb-1 opacity-70" />
          <div className="text-xs font-bold font-mono text-slate-200">
            {loading ? "—" : `${health?.components?.guilds?.active ?? 0} / ${health?.components?.guilds?.total ?? 0}`}
          </div>
          <div className="text-[7px] font-bold uppercase tracking-widest text-slate-500 mt-0.5">Guilds Activos</div>
        </div>
        {/* Metric 2: Memory (SilvaDB) */}
        <div className="bg-slate-950/40 border border-slate-800/40 rounded-xl p-2.5 text-center">
          <Database className="w-3.5 h-3.5 text-violet-400 mx-auto mb-1 opacity-70" />
          <div className="text-xs font-bold font-mono text-slate-200 truncate">
            {loading ? "—" : `${health?.components?.silva?.nodes ?? 0} Nodes`}
          </div>
          <div className="text-[7px] font-bold uppercase tracking-widest text-slate-500 mt-0.5">SilvaDB Nodos</div>
        </div>
        {/* Metric 3: System Status */}
        <div className="bg-slate-950/40 border border-slate-800/40 rounded-xl p-2.5 text-center">
          <Activity className="w-3.5 h-3.5 text-emerald-400 mx-auto mb-1 opacity-70" />
          <div className={cn("text-xs font-bold font-mono truncate", statusColor)}>
            {loading ? "—" : health?.status?.toUpperCase() ?? "UNKNOWN"}
          </div>
          <div className="text-[7px] font-bold uppercase tracking-widest text-slate-500 mt-0.5">Estado Cortex</div>
        </div>
      </div>
    </div>
  );
}
