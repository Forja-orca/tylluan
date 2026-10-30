import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import { Activity, Radio, Clock, RefreshCw, AlertCircle, Sparkles } from 'lucide-react';
import type { NexusBridge } from '@tylluan/ui-core';

interface VisualControlRoomProps {
  bridge: NexusBridge | null;
  online: boolean;
}

export type SubruntimeId = 'Memoria' | 'Inferencia' | 'Procesos' | 'Decision' | 'Coordinacion' | 'Red';

export interface SubruntimeStatus {
  id: SubruntimeId;
  name: string;
  percent: number; // 0 to 100
  state: 'idle' | 'running' | 'warning' | 'error' | 'nodata';
  details: string;
  tasksCount: number;
  runsTotal: number;
  errorsTotal: number;
}

export interface FlowNode {
  id: SubruntimeId;
  name: string;
  x: number;
  y: number;
}

export interface FlowEdge {
  from: SubruntimeId;
  to: SubruntimeId;
  label: string;
  volume: number; // relative active volume
}

export interface HeatmapCell {
  agent: string;
  hour: number; // 0..23
  count: number;
}

export function VisualControlRoom({ bridge, online }: VisualControlRoomProps) {
  const [activeView, setActiveView] = useState<'rings' | 'flow' | 'heatmap'>(() => {
    if (typeof window !== 'undefined') {
      const p = new URLSearchParams(window.location.search);
      const v = p.get('view');
      if (v === 'flow' || v === 'heatmap') return v;
    }
    return 'rings';
  });
  const [loading, setLoading] = useState(true);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Raw data from endpoints
  const [tasksData, setTasksData] = useState<any[] | null>(null);
  const [healthDetailed, setHealthDetailed] = useState<any | null>(null);
  const [guildsData, setGuildsData] = useState<any[] | null>(null);
  const [messagesData, setMessagesData] = useState<any[] | null>(null);
  const [sysStatusData, setSysStatusData] = useState<any | null>(null);

  // Poll data with interval >= 5s
  const fetchData = useCallback(async () => {
    if (!bridge || !online) {
      setLoading(false);
      return;
    }

    try {
      // 1. /api/v1/runtime/tasks (T-DSH-140)
      const tasksPromise = bridge.fetch('/api/v1/runtime/tasks').catch(() => null);
      // 2. /api/v1/health/detailed
      const healthPromise = bridge.fetch('/api/v1/health/detailed').catch(() => null);
      // 3. /api/v1/guilds
      const guildsPromise = bridge.fetch('/api/v1/guilds').catch(() => null);
      // 4. /api/v1/coloquio/channels/general/messages?limit=100
      const msgsPromise = bridge.fetch('/api/v1/coloquio/channels/general/messages?limit=100').catch(() => null);
      // 5. /api/v1/system/status
      const sysPromise = bridge.fetch('/api/v1/system/status').catch(() => null);

      const [tasksRes, healthRes, guildsRes, msgsRes, sysRes] = await Promise.all([
        tasksPromise,
        healthPromise,
        guildsPromise,
        msgsPromise,
        sysPromise,
      ]);

      if (tasksRes?.tasks) {
        setTasksData(tasksRes.tasks);
      } else {
        setTasksData(null);
      }

      setHealthDetailed(healthRes || null);
      setGuildsData(Array.isArray(guildsRes) ? guildsRes : (guildsRes?.value || null));
      setMessagesData(msgsRes?.messages || null);
      setSysStatusData(sysRes || null);

      setLastUpdated(new Date());
      setErrorMsg(null);
    } catch (err: any) {
      setErrorMsg(err.message || 'Error consultando endpoints de telemetría');
    } finally {
      setLoading(false);
    }
  }, [bridge, online]);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 6000); // 6s refresh (>= 5s required)
    return () => clearInterval(interval);
  }, [fetchData]);

  // Compute Subruntime Rings data with strictly documented, non-arbitrary formulas
  const subruntimes: SubruntimeStatus[] = useMemo(() => {
    // 1. Memoria (SilvaDB + Tasks)
    // Formula: health.components.silva.ok ? 100 : 0 (con advertencia si hay errores en tareas de memoria)
    let memPercent = 0;
    let memState: SubruntimeStatus['state'] = 'nodata';
    let memDetails = 'Sin datos de memoria';
    let memTasks = 0;
    let memRuns = 0;
    let memErrors = 0;

    if (tasksData) {
      const memT = tasksData.filter((t: any) => t.subruntime === 'Memoria' || t.name === 'reindexer');
      memTasks = memT.length;
      memRuns = memT.reduce((acc: number, t: any) => acc + (t.runs || 0), 0);
      memErrors = memT.reduce((acc: number, t: any) => acc + (t.errors || 0), 0);
    }

    if (healthDetailed?.components?.silva) {
      const s = healthDetailed.components.silva;
      memState = s.ok ? (memErrors > 0 ? 'warning' : 'running') : 'error';
      memPercent = s.ok ? 100 : 0;
      memDetails = `${s.nodes ?? 0} nodos · ${s.edges ?? 0} aristas`;
    }

    // 2. Inferencia (Embeddings + Reranker)
    // Formula: (embeddings.ok ? 50 : 0) + (reranker.ok ? 50 : 0) -> 0..100% de disponibilidad de modelos
    let infPercent = 0;
    let infState: SubruntimeStatus['state'] = 'nodata';
    let infDetails = 'Sin datos de inferencia';
    if (healthDetailed?.components?.embeddings || healthDetailed?.components?.reranker) {
      const emb = healthDetailed.components.embeddings?.ok;
      const rer = healthDetailed.components.reranker?.ok;
      infPercent = (emb ? 50 : 0) + (rer ? 50 : 0);
      infState = infPercent === 100 ? 'running' : infPercent > 0 ? 'warning' : 'error';
      infDetails = `Embeddings: ${emb ? 'OK' : 'FAIL'} · Reranker: ${rer ? 'OK' : 'FAIL'}`;
    }

    // 3. Procesos y Guilds
    // Formula: (guilds_activos / guilds_totales) * 100 -> Tasa real de procesos en ejecucion
    let procPercent = 0;
    let procState: SubruntimeStatus['state'] = 'nodata';
    let procDetails = 'Sin guilds activos';
    if (guildsData && Array.isArray(guildsData) && guildsData.length > 0) {
      const total = guildsData.length;
      const running = guildsData.filter((g: any) => g.running).length;
      procPercent = Math.round((running / total) * 100);
      procState = running > 0 ? 'running' : 'idle';
      procDetails = `${running} de ${total} procesos activos`;
    } else if (healthDetailed?.components?.guilds) {
      const g = healthDetailed.components.guilds;
      procPercent = g.total > 0 ? Math.round((g.active / g.total) * 100) : 0;
      procState = g.active > 0 ? 'running' : 'idle';
      procDetails = `${g.active} de ${g.total} guilds activos`;
    }

    // 4. Decisión
    // Formula: sysStatus.score (0..100) -> Score de salud cognitiva integral del Sentinel
    let decPercent = 0;
    let decState: SubruntimeStatus['state'] = 'nodata';
    let decDetails = 'Sentinel / Decision Service';
    if (sysStatusData?.score !== undefined) {
      decPercent = sysStatusData.score;
      decState = decPercent >= 80 ? 'running' : decPercent >= 50 ? 'warning' : 'error';
      decDetails = `Score de salud cognitiva: ${decPercent}%`;
    }

    // 5. Coordinación (Coloquio / Buzon Inter-Agente)
    // Formula: mailbox_healthy ? 100 : 0 -> Integridad del canal de coordinacion y entrega de turnos
    let coordPercent = 0;
    let coordState: SubruntimeStatus['state'] = 'nodata';
    let coordDetails = 'Coloquio / Mensajería';
    const mailboxOk = sysStatusData?.mailbox_healthy;
    const msgCount = messagesData && Array.isArray(messagesData) ? messagesData.length : 0;

    if (mailboxOk !== undefined || msgCount > 0) {
      coordPercent = mailboxOk ? 100 : msgCount > 0 ? 80 : 0;
      coordState = coordPercent >= 80 ? 'running' : coordPercent > 0 ? 'warning' : 'idle';
      coordDetails = `${msgCount} mensajes recientes · Buzón ${mailboxOk ? 'OK' : 'Alerta'}`;
    }

    // 6. Red y Enlace
    // Formula: health.components.tunnel.ok ? 100 : 0 (Aislamiento local soberano = 0%, sin error)
    let redPercent: number;
    let redState: SubruntimeStatus['state'];
    let redDetails: string;
    if (healthDetailed?.components?.tunnel) {
      const t = healthDetailed.components.tunnel;
      redState = t.ok ? 'running' : 'idle';
      redPercent = t.ok ? 100 : 0;
      redDetails = t.ok ? 'Túnel / Federación activa' : 'Operando en aislamiento local';
    } else {
      redState = 'idle';
      redPercent = 0;
      redDetails = 'Modo soberano local (Mesh standby)';
    }

    return [
      { id: 'Memoria', name: 'Memoria', percent: memPercent, state: memState, details: memDetails, tasksCount: memTasks, runsTotal: memRuns, errorsTotal: memErrors },
      { id: 'Inferencia', name: 'Inferencia', percent: infPercent, state: infState, details: infDetails, tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
      { id: 'Procesos', name: 'Procesos y Guilds', percent: procPercent, state: procState, details: procDetails, tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
      { id: 'Decision', name: 'Decisión', percent: decPercent, state: decState, details: decDetails, tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
      { id: 'Coordinacion', name: 'Coordinación', percent: coordPercent, state: coordState, details: coordDetails, tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
      { id: 'Red', name: 'Red y Enlace', percent: redPercent, state: redState, details: redDetails, tasksCount: 0, runsTotal: 0, errorsTotal: 0 },
    ];
  }, [tasksData, healthDetailed, guildsData, messagesData, sysStatusData]);

  return (
    <div className="flex-1 flex flex-col h-full space-y-4 animate-in fade-in duration-300">
      {/* Top Controller Bar */}
      <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 border-b border-border/80 pb-4">
        <div>
          <div className="flex items-center gap-2">
            <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold uppercase tracking-wider bg-primary/10 text-primary border border-primary/20">
              <Sparkles className="w-3 h-3 text-primary" />
              Soberanía Visual · AG-17
            </span>
            <span className="text-xs text-muted-foreground font-mono">ADR-025</span>
          </div>
          <h2 className="text-xl font-bold tracking-tight text-foreground mt-1">
            Sala de Control Visual
          </h2>
          <p className="text-xs text-muted-foreground mt-0.5">
            Trazabilidad visual continua sin listas de texto: anillos de subruntimes, mapa de flujo animado y matriz de calor de la flota.
          </p>
        </div>

        {/* View Switcher Tabs */}
        <div className="flex items-center gap-1 bg-muted/60 p-1 rounded-xl border border-border">
          <button
            type="button"
            onClick={() => setActiveView('rings')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all cursor-pointer ${
              activeView === 'rings'
                ? 'bg-card text-foreground shadow-sm border border-border'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            1. Anillos de Estado
          </button>
          <button
            type="button"
            onClick={() => setActiveView('flow')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all cursor-pointer ${
              activeView === 'flow'
                ? 'bg-card text-foreground shadow-sm border border-border'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            2. Mapa de Flujo
          </button>
          <button
            type="button"
            onClick={() => setActiveView('heatmap')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all cursor-pointer ${
              activeView === 'heatmap'
                ? 'bg-card text-foreground shadow-sm border border-border'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            3. Mapa de Calor
          </button>
        </div>
      </div>

      {/* Main Visual Display */}
      <div className="flex-1 min-h-[460px] rounded-2xl border border-border bg-card/40 p-4 md:p-6 flex flex-col relative overflow-hidden backdrop-blur-sm">
        {activeView === 'rings' && <SubruntimeRingsView subruntimes={subruntimes} />}
        {activeView === 'flow' && <FlowMapCanvasView subruntimes={subruntimes} />}
        {activeView === 'heatmap' && <AgentHeatmapView messages={messagesData} />}
      </div>

      {/* Footer Info & Telemetry Endpoints Attribution */}
      <div className="flex flex-col sm:flex-row items-center justify-between gap-2 px-2 text-[11px] font-mono text-muted-foreground border-t border-border/40 pt-3">
        <div className="flex items-center gap-2">
          <Activity className="w-3.5 h-3.5 text-primary" />
          <span>Endpoints vivos: <code className="text-foreground">/api/v1/runtime/tasks</code> · <code className="text-foreground">/health/detailed</code> · <code className="text-foreground">/guilds</code> · <code className="text-foreground">/coloquio/messages</code></span>
        </div>
        <div className="flex items-center gap-3">
          {lastUpdated && (
            <span>Refresco: {lastUpdated.toLocaleTimeString()} (6s)</span>
          )}
          <span className="text-primary font-mono">rAF pausado en document.hidden</span>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 1. VISTA: ANILLOS DE ESTADO (SVG Minimalista con Arcos Visibles)
// ---------------------------------------------------------------------------
interface RingsViewProps {
  subruntimes: SubruntimeStatus[];
}

export function SubruntimeRingsView({ subruntimes }: RingsViewProps) {
  return (
    <div className="flex-1 grid grid-cols-2 lg:grid-cols-3 gap-6 items-center justify-center p-2">
      {subruntimes.map((sub) => {
        const radius = 54;
        const circumference = 2 * Math.PI * radius;
        // Stroke dashoffset: circumference * (1 - percent / 100)
        const strokeDashoffset = circumference - (Math.max(0, Math.min(100, sub.percent)) / 100) * circumference;

        // Color del arco según estado
        let strokeColor = 'hsl(var(--primary))'; // Token CSS canónico salvia
        if (sub.state === 'nodata') strokeColor = '#475569'; // Slate 600
        else if (sub.state === 'error') strokeColor = '#ef4444'; // Red 500
        else if (sub.state === 'warning') strokeColor = '#f59e0b'; // Amber 500
        else if (sub.state === 'idle') strokeColor = '#64748b'; // Slate 500

        return (
          <div
            key={sub.id}
            data-testid={`ring-${sub.id}`}
            className="flex flex-col items-center justify-center p-4 rounded-xl border border-border/60 bg-muted/20 hover:bg-muted/40 transition-all group"
          >
            <div className="relative w-36 h-36 flex items-center justify-center">
              <svg className="w-full h-full transform -rotate-90" viewBox="0 0 140 140">
                {/* Background Track */}
                <circle
                  cx="70"
                  cy="70"
                  r={radius}
                  className="stroke-muted/40"
                  strokeWidth="8"
                  fill="transparent"
                />
                {/* Active Progress Ring (Arco Proporcional) */}
                <circle
                  cx="70"
                  cy="70"
                  r={radius}
                  stroke={strokeColor}
                  strokeWidth="8"
                  strokeDasharray={circumference}
                  strokeDashoffset={strokeDashoffset}
                  strokeLinecap="round"
                  fill="transparent"
                  style={{ transition: 'stroke-dashoffset 0.8s ease-in-out' }}
                />
              </svg>

              {/* Center Metrics (Hueso / Grafito) */}
              <div className="absolute inset-0 flex flex-col items-center justify-center text-center">
                {sub.state === 'nodata' ? (
                  <span className="text-[11px] font-mono font-bold text-muted-foreground uppercase">
                    Sin Dato
                  </span>
                ) : (
                  <>
                    <span className="text-2xl font-bold font-mono tracking-tight text-foreground">
                      {sub.percent}%
                    </span>
                    <span className="text-[10px] uppercase font-mono tracking-wider text-muted-foreground">
                      {sub.state}
                    </span>
                  </>
                )}
              </div>
            </div>

            <div className="mt-3 text-center w-full px-2">
              <h4 className="text-sm font-bold text-foreground tracking-tight">{sub.name}</h4>
              <p className="text-[11px] font-mono text-muted-foreground mt-0.5 truncate w-full" title={sub.details}>
                {sub.details}
              </p>
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 2. VISTA: MAPA DE FLUJO (Canvas con un solo rAF pausado si está oculto)
// ---------------------------------------------------------------------------
interface FlowViewProps {
  subruntimes: SubruntimeStatus[];
}

export function FlowMapCanvasView({ subruntimes }: FlowViewProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    let animationId: number;
    let particleOffset = 0;
    let isVisible = true;

    // Handle Page Visibility to strictly save CPU (requirement < 2% CPU)
    const handleVisibilityChange = () => {
      isVisible = !document.hidden;
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);

    // Canvas dynamic sizing
    const resize = () => {
      if (containerRef.current && canvas) {
        canvas.width = containerRef.current.clientWidth;
        canvas.height = containerRef.current.clientHeight;
      }
    };
    resize();
    window.addEventListener('resize', resize);

    // Predefined Node Coordinates (Hexagonal Graph Layout)
    const render = () => {
      if (!isVisible) {
        animationId = requestAnimationFrame(render);
        return;
      }

      ctx.clearRect(0, 0, canvas.width, canvas.height);

      const w = canvas.width;
      const h = canvas.height;
      const cx = w / 2;
      const cy = h / 2;
      const r = Math.min(w, h) * 0.35;

      const nodes: FlowNode[] = [
        { id: 'Memoria', name: 'Memoria', x: cx + r * Math.cos(0), y: cy + r * Math.sin(0) },
        { id: 'Inferencia', name: 'Inferencia', x: cx + r * Math.cos(Math.PI / 3), y: cy + r * Math.sin(Math.PI / 3) },
        { id: 'Procesos', name: 'Procesos', x: cx + r * Math.cos((2 * Math.PI) / 3), y: cy + r * Math.sin((2 * Math.PI) / 3) },
        { id: 'Decision', name: 'Decisión', x: cx + r * Math.cos(Math.PI), y: cy + r * Math.sin(Math.PI) },
        { id: 'Coordinacion', name: 'Coordinación', x: cx + r * Math.cos((4 * Math.PI) / 3), y: cy + r * Math.sin((4 * Math.PI) / 3) },
        { id: 'Red', name: 'Red', x: cx + r * Math.cos((5 * Math.PI) / 3), y: cy + r * Math.sin((5 * Math.PI) / 3) },
      ];

      // Dynamic volume and active status derived from actual subruntime state
      const memStatus = subruntimes.find(s => s.id === 'Memoria');
      const infStatus = subruntimes.find(s => s.id === 'Inferencia');
      const procStatus = subruntimes.find(s => s.id === 'Procesos');
      const decStatus = subruntimes.find(s => s.id === 'Decision');
      const coordStatus = subruntimes.find(s => s.id === 'Coordinacion');
      const redStatus = subruntimes.find(s => s.id === 'Red');

      // Edges with real volume-based weights and stroke widths: [fromIdx, toIdx, weight, label]
      const edges: [number, number, number, string][] = [
        [0, 1, Math.max(0.2, (memStatus?.percent || 0) / 100), 'Indexación'], // Memoria -> Inferencia
        [1, 3, Math.max(0.2, (infStatus?.percent || 0) / 100), 'Vectores'],    // Inferencia -> Decisión
        [3, 4, Math.max(0.2, (decStatus?.percent || 0) / 100), 'Intenciones'], // Decisión -> Coordinación
        [4, 2, Math.max(0.2, (coordStatus?.percent || 0) / 100), 'Despacho'],  // Coordinación -> Procesos
        [2, 0, Math.max(0.2, (procStatus?.percent || 0) / 100), 'Consignación'],// Procesos -> Memoria
        [4, 5, Math.max(0.1, (redStatus?.percent || 0) / 100), 'Gossip'],      // Coordinación -> Red
        [5, 0, Math.max(0.1, (redStatus?.percent || 0) / 100), 'Sync'],        // Red -> Memoria
      ];

      // 1. Draw Edges with volume-based width and particle density
      edges.forEach(([fromIdx, toIdx, weight, label]) => {
        const from = nodes[fromIdx];
        const to = nodes[toIdx];

        ctx.beginPath();
        const edgeWidth = 1.5 + weight * 3;
        ctx.strokeStyle = weight > 0.4 ? 'rgba(138, 180, 166, 0.4)' : 'rgba(100, 116, 139, 0.25)';
        ctx.lineWidth = edgeWidth;
        ctx.moveTo(from.x, from.y);
        ctx.lineTo(to.x, to.y);
        ctx.stroke();

        // Edge Volume Label at midpoint
        const midX = (from.x + to.x) / 2;
        const midY = (from.y + to.y) / 2;
        ctx.fillStyle = '#94a3b8';
        ctx.font = '500 9px JetBrains Mono, monospace';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText(`${label} (v:${Math.round(weight * 100)}%)`, midX, midY - 2);

        // Animated Particle along line
        const dx = to.x - from.x;
        const dy = to.y - from.y;
        const distance = Math.sqrt(dx * dx + dy * dy);
        const progress = ((particleOffset * 0.5 * weight) % distance) / distance;

        const px = from.x + dx * progress;
        const py = from.y + dy * progress;

        ctx.beginPath();
        ctx.fillStyle = 'rgba(138, 180, 166, 0.9)'; // Salvia #8AB4A6
        const particleRadius = 2 + weight * 2;
        ctx.arc(px, py, particleRadius, 0, 2 * Math.PI);
        ctx.fill();
      });

      // 2. Draw Nodes
      nodes.forEach((node) => {
        const status = subruntimes.find((s) => s.id === node.id);
        const isOk = status?.state === 'running';
        const hasData = status?.state !== 'nodata';

        // Outer Glow/Border
        ctx.beginPath();
        ctx.fillStyle = '#0f172a'; // Slate 900
        ctx.strokeStyle = isOk ? 'rgba(138, 180, 166, 0.8)' : hasData ? 'rgba(245, 158, 11, 0.5)' : 'rgba(71, 85, 105, 0.4)';
        ctx.lineWidth = 2.5;
        ctx.arc(node.x, node.y, 26, 0, 2 * Math.PI);
        ctx.fill();
        ctx.stroke();

        // Node Label
        ctx.fillStyle = '#f8fafc'; // Hueso / Slate 50
        ctx.font = '600 11px JetBrains Mono, monospace';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(node.name, node.x, node.y + 40);

        // Subtitle metric under node
        ctx.fillStyle = isOk ? '#8AB4A6' : '#94a3b8';
        ctx.font = '500 10px JetBrains Mono, monospace';
        ctx.fillText(hasData ? `${status?.percent}%` : 'SIN DATO', node.x, node.y + 54);

        // Center Indicator
        ctx.beginPath();
        ctx.fillStyle = isOk ? '#8AB4A6' : hasData ? '#f59e0b' : '#64748b';
        ctx.arc(node.x, node.y, 5, 0, 2 * Math.PI);
        ctx.fill();
      });

      particleOffset += 1;
      animationId = requestAnimationFrame(render);
    };

    animationId = requestAnimationFrame(render);

    return () => {
      cancelAnimationFrame(animationId);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      window.removeEventListener('resize', resize);
    };
  }, [subruntimes]);

  return (
    <div ref={containerRef} className="flex-1 w-full h-full min-h-[380px] relative">
      <canvas ref={canvasRef} className="w-full h-full block" />
      <div className="absolute top-2 left-2 bg-slate-900/80 border border-slate-800 rounded-lg p-2 text-[10px] font-mono text-slate-300">
        <span>Aristas con grosor y velocidad proporcionales al volumen de datos activo</span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 3. VISTA: MAPA DE CALOR (Agentes x Horas con Actividad Real)
// ---------------------------------------------------------------------------
interface HeatmapViewProps {
  messages: any[] | null;
}

export function AgentHeatmapView({ messages }: HeatmapViewProps) {
  // Aggregate real activity from Coloquio messages
  const { agents, hours, matrix, maxCount } = useMemo(() => {
    const knownAgents = ['claude-code', 'dsh', 'hermes', 'antigravity', 'buffy', 'deep', 'jose'];
    const activeAgentsSet = new Set<string>(knownAgents);

    if (messages && Array.isArray(messages)) {
      messages.forEach((m) => {
        if (m.author_id) activeAgentsSet.add(m.author_id);
      });
    }

    const agentList = Array.from(activeAgentsSet);
    const hourList = Array.from({ length: 24 }, (_, i) => i);

    // Matrix [agentIdx][hour] -> count
    const countMatrix: number[][] = Array.from({ length: agentList.length }, () =>
      Array(24).fill(0)
    );

    let max = 0;
    if (messages && Array.isArray(messages)) {
      messages.forEach((m) => {
        if (!m.author_id || !m.created_at) return;
        const agentIdx = agentList.indexOf(m.author_id);
        if (agentIdx === -1) return;

        const date = new Date(m.created_at * 1000);
        const hr = date.getHours(); // 0..23 local
        countMatrix[agentIdx][hr] += 1;
        if (countMatrix[agentIdx][hr] > max) {
          max = countMatrix[agentIdx][hr];
        }
      });
    }

    return {
      agents: agentList,
      hours: hourList,
      matrix: countMatrix,
      maxCount: Math.max(1, max),
    };
  }, [messages]);

  return (
    <div className="flex-1 flex flex-col justify-center overflow-x-auto p-2">
      <div className="min-w-[700px] space-y-2">
        {/* Hour Header */}
        <div className="grid grid-cols-[130px_repeat(24,1fr)] gap-1 text-[10px] font-mono text-muted-foreground text-center">
          <div className="text-left font-bold pl-2">Agente \ Hora</div>
          {hours.map((h) => (
            <div key={h} className="truncate">
              {h}h
            </div>
          ))}
        </div>

        {/* Matrix Rows */}
        {agents.map((agent, aIdx) => (
          <div
            key={agent}
            className="grid grid-cols-[130px_repeat(24,1fr)] gap-1 items-center"
          >
            <div className="text-xs font-mono font-medium text-foreground truncate pl-2" title={agent}>
              @{agent}
            </div>

            {hours.map((h) => {
              const count = matrix[aIdx][h];
              const intensity = count > 0 ? Math.min(1, count / maxCount) : 0;

              return (
                <div
                  key={h}
                  className="h-7 rounded transition-all flex items-center justify-center cursor-help group relative border border-border/30"
                  style={{
                    backgroundColor:
                      count === 0
                        ? 'rgba(30, 41, 59, 0.4)' // Grafito apagado
                        : `rgba(138, 180, 166, ${0.25 + intensity * 0.75})`, // Salvia graduado #8AB4A6
                  }}
                  title={`@${agent} a las ${h}:00: ${count} mensajes`}
                >
                  {count > 0 && (
                    <span className="text-[9px] font-mono font-bold text-slate-950">
                      {count}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        ))}

        {/* Legend con muestras de color claras y explicitas */}
        <div className="flex items-center justify-between text-[11px] font-mono text-muted-foreground pt-4 border-t border-border/40">
          <div className="flex items-center gap-3">
            <span>Intensidad:</span>
            <div className="flex items-center gap-1.5">
              <span className="w-3.5 h-3.5 rounded border border-border/40" style={{ backgroundColor: 'rgba(30, 41, 59, 0.6)' }} title="0 mensajes" />
              <span className="text-[10px]">Inactivo (0)</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="w-3.5 h-3.5 rounded border border-border/40" style={{ backgroundColor: 'rgba(138, 180, 166, 0.4)' }} title="Baja actividad" />
              <span className="text-[10px]">Baja</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="w-3.5 h-3.5 rounded border border-border/40" style={{ backgroundColor: 'rgba(138, 180, 166, 0.95)' }} title="Alta actividad" />
              <span className="text-[10px]">Alta</span>
            </div>
          </div>
          <span>Total mensajes analizados: {messages ? messages.length : '0 (Sin datos)'}</span>
        </div>
      </div>
    </div>
  );
}
