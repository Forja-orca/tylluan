import React, { useState, useEffect } from 'react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';

interface HomeostasisWidgetProps {
  bridge: NexusBridge | null;
}

interface HomeostasisHealth {
  status?: string;
  components?: {
    guilds?: { ok?: boolean; active?: number; total?: number };
    silva?: { ok?: boolean; nodes?: number };
  };
}

export function HomeostasisWidget({ bridge }: HomeostasisWidgetProps) {
  const [health, setHealth] = useState<HomeostasisHealth | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!bridge) return;
    const fetchHealth = async () => {
      try {
        const h = (await bridge.health_detailed()) as HomeostasisHealth;
        setHealth(h);
      } catch (e: unknown) {
        console.error('Failed to fetch detailed health in HomeostasisWidget:', e);
      } finally {
        setLoading(false);
      }
    };
    fetchHealth();
  }, [bridge]);

  // Polling via centralized coordinator (replaces 1 scattered setInterval)
  usePolling('homeostasis-health', async () => {
    if (!bridge) return;
    try { setHealth(await bridge.health_detailed()); } catch {}
  }, { interval: 'medium', enabled: !!bridge });

  // Calculate homeostasis score: guilds_ok * 40 + memoria_ok * 30 + sin_errores_recientes * 30
  const guildsOk = health?.components?.guilds?.ok ? 1 : 0;
  const memoryOk = health?.components?.silva?.ok ? 1 : 0;
  const noRecentErrors = health?.status === 'healthy' ? 1 : 0;
  
  const score = Math.round((guildsOk * 40) + (memoryOk * 30) + (noRecentErrors * 30));

  // Determine color theme (logo blue / standard status semaphores)
  let statusColor = "text-emerald-600";
  let statusBg = "bg-emerald-500/10";
  let statusBorder = "border-emerald-500/30";
  let strokeColor = "#16a34a";

  if (score < 50) {
    statusColor = "text-red-600";
    statusBg = "bg-red-500/10";
    statusBorder = "border-red-500/30";
    strokeColor = "#dc2626";
  } else if (score < 80) {
    statusColor = "text-amber-600";
    statusBg = "bg-amber-500/10";
    statusBorder = "border-amber-500/30";
    strokeColor = "#d97706";
  }

  // SVG circle calculations
  const radius = 38;
  const circumference = 2 * Math.PI * radius;
  const strokeDashoffset = circumference - (score / 100) * circumference;

  return (
    <div className="rounded border border-border bg-card p-4 relative font-sans">
      <div className="flex flex-col sm:flex-row items-center gap-6 justify-between">
        {/* Left Side: Text and status description */}
        <div className="space-y-2 text-center sm:text-left flex-1">
          <div className="flex items-center gap-2 justify-center sm:justify-start">
            <span className="text-xs font-bold text-foreground">Homeostasis del Sistema</span>
          </div>
          <div>
            <h3 className="text-base font-bold text-foreground">Córtex TylluanNexus</h3>
            <p className="text-xs text-muted-foreground leading-relaxed max-w-sm mt-0.5">
              Indicador de integridad unificado: gremios, base SilvaDB y estabilidad en tiempo real.
            </p>
          </div>
          
          {/* Status badge */}
          <div className={cn("inline-flex items-center gap-1.5 px-2 py-0.5 rounded border text-xs font-bold", statusBg, statusBorder, statusColor)}>
            <div className={cn("w-1.5 h-1.5 rounded-full", score >= 80 ? "bg-emerald-600" : score >= 50 ? "bg-amber-600" : "bg-red-600")} />
            {loading ? "Calculando..." : health?.status === "healthy" ? "Córtex Estable" : "Tensión en el Sistema"}
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
                className="stroke-muted"
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
                className="transition-all duration-500 ease-out"
              />
            </svg>
            {/* Score inside circular gauge */}
            <div className="absolute flex flex-col items-center justify-center">
              <span className="text-xl font-bold font-mono text-foreground">{loading ? "—" : score}</span>
              <span className="text-[11px] text-muted-foreground font-mono">Puntos</span>
            </div>
          </div>
        </div>
      </div>

      {/* Metrics Row below */}
      <div className="grid grid-cols-3 gap-2 mt-4 pt-3 border-t border-border">
        {/* Metric 1: Guilds */}
        <div className="bg-muted/40 border border-border rounded p-2 text-center">
          <div className="text-xs font-bold font-mono text-foreground">
            {loading ? "—" : `${health?.components?.guilds?.active ?? 0} / ${health?.components?.guilds?.total ?? 0}`}
          </div>
          <div className="text-[11px] text-muted-foreground mt-0.5">Gremios activos</div>
        </div>
        {/* Metric 2: Memory (SilvaDB) */}
        <div className="bg-muted/40 border border-border rounded p-2 text-center">
          <div className="text-xs font-bold font-mono text-foreground truncate">
            {loading ? "—" : `${health?.components?.silva?.nodes ?? 0} Nodos`}
          </div>
          <div className="text-[11px] text-muted-foreground mt-0.5">Nodos SilvaDB</div>
        </div>
        {/* Metric 3: System Status */}
        <div className="bg-muted/40 border border-border rounded p-2 text-center">
          <div className={cn("text-xs font-bold font-mono truncate", statusColor)}>
            {loading ? "—" : health?.status?.toUpperCase() ?? "DESCONOCIDO"}
          </div>
          <div className="text-[11px] text-muted-foreground mt-0.5">Estado Córtex</div>
        </div>
      </div>
    </div>
  );
}
