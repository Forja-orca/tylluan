import React, { useState, useEffect, useMemo, useCallback } from 'react';
import {
  Activity,
  Wifi,
  AlertTriangle,
  Database,
  TrendingUp,
  CheckCircle,
  Cpu
} from 'lucide-react';
import type {
  GoldenSignals,
  GuildsUtilization,
  MemoryRetention,
  SloSummary,
  Guild,
  Approval,
  NexusBridge,
  NexusEvent,
  BlackboardData,
  CollectivePulse,
  Interoception
} from '../lib/nexus-bridge';
import { useNexus } from '../hooks/useNexus';
import { usePolling } from '../hooks/usePolling';
import type { MemoryStats } from '../hooks/useNexus';
import { cn } from '../lib/utils';
import { MetricCard, RelativeTime, MiniSparkline } from './ui/MetricPrimitives';
import { HomeostasisWidget } from './HomeostasisWidget';
import { DreamStatusWidget } from './DreamStatusWidget';
import { CanaryStatusWidget } from './CanaryStatusWidget';
import { WelcomeEmptyState } from './WelcomeEmptyState';

interface CurriculumEntry {
  total?: number;
  successes?: number;
  avg_latency_ms?: number;
}

interface HealthDetailedShape {
  score?: number;
  [key: string]: unknown;
}

interface SysStatusShape {
  sessions?: unknown[];
  night_consolidation_active?: boolean;
  embeddings_loaded?: boolean;
  silva_healthy?: boolean;
  mailbox_healthy?: boolean;
  curriculum_entries?: number;
  uptime_secs?: number;
  [key: string]: unknown;
}

interface OverviewTabProps {
  bridge: NexusBridge | null;
  goldenSignals: GoldenSignals | null;
  guildsUtilization: GuildsUtilization | null;
  memoryRetention: MemoryRetention | null;
  sloSummary: SloSummary | null;
  guilds: Guild[];
  approvals: Approval[];
  memoryStats: MemoryStats | null;
  healthDetailed: HealthDetailedShape | null;
  sysStatus: SysStatusShape | null;
  events: NexusEvent[];
  notify: (msg: string, type?: 'info' | 'error') => void;
  refreshData: () => Promise<void>;
  online: boolean;
}

export function OverviewTab({ 
  bridge,
  goldenSignals, 
  guildsUtilization, 
  memoryRetention, 
  sloSummary, 
  guilds = [], 
  approvals = [], 
  memoryStats,
  healthDetailed,
  sysStatus,
  events = [],
  notify,
  refreshData,
  online = true
}: OverviewTabProps) {
  const [_refreshTrigger, setRefreshTrigger] = useState(0);
  const [liveMetrics, setLiveMetrics] = useState<{
    totalCalls: number; successRate: number; avgLatency: number;
    activeAgents: number; broadcastsLastHour: number; graphNodes: number;
  } | null>(null);
  const [blackboard, setBlackboard] = useState<BlackboardData | null>(null);
  const [collectivePulse, setCollectivePulse] = useState<CollectivePulse | null>(null);
  const [interoceptionData, setInteroceptionData] = useState<Interoception | null>(null);
  const [prevSilvaCount, setPrevSilvaCount] = useState<number>(0);
  const [silvaDelta, setSilvaDelta] = useState<number>(0);
  const [cpuHistory, setCpuHistory] = useState<number[]>([]);
  const [memHistory, setMemHistory] = useState<number[]>([]);
  const [showHeartbeats, setShowHeartbeats] = useState(false);

  const kernelHealth = useMemo(() => {
    // Offline wins over any cached data: healthDetailed/sysStatus are never cleared on disconnect.
    if (!online) {
      return { label: 'OFFLINE', colorClass: 'text-rose-400', dotClass: 'bg-rose-500' };
    }
    if (healthDetailed) {
      const s = typeof healthDetailed.status === 'string' ? healthDetailed.status.toLowerCase() : null;
      if (s === 'healthy') {
        return { label: 'HEALTHY', colorClass: 'text-emerald-400', dotClass: 'bg-emerald-500' };
      }
      if (s === 'degraded') {
        return { label: 'DEGRADED', colorClass: 'text-amber-400', dotClass: 'bg-amber-500' };
      }
      if (s === 'critical') {
        return { label: 'CRITICAL', colorClass: 'text-rose-400', dotClass: 'bg-rose-500' };
      }
      if (s === 'warming_up') {
        return { label: 'WARMING UP', colorClass: 'text-amber-400', dotClass: 'bg-amber-500' };
      }
      if (typeof healthDetailed.score === 'number') {
        // Fallback to kernel's canonical score thresholds from api_admin.rs:992-994
        if (healthDetailed.score >= 80) {
          return { label: 'HEALTHY', colorClass: 'text-emerald-400', dotClass: 'bg-emerald-500' };
        } else if (healthDetailed.score >= 50) {
          return { label: 'DEGRADED', colorClass: 'text-amber-400', dotClass: 'bg-amber-500' };
        } else {
          return { label: 'CRITICAL', colorClass: 'text-rose-400', dotClass: 'bg-rose-500' };
        }
      }
    }
    // Connected but no detailed telemetry received yet: honest UNKNOWN state.
    return { label: 'UNKNOWN', colorClass: 'text-slate-400', dotClass: 'bg-slate-500' };
  }, [online, healthDetailed]);
  const embeddingsKnown = online && typeof sysStatus?.embeddings_loaded === 'boolean';
  const embeddingsState = !embeddingsKnown
    ? { label: 'UNKNOWN', colorClass: 'text-slate-400', dotClass: 'bg-slate-500' }
    : sysStatus?.embeddings_loaded
      ? { label: 'LOADED', colorClass: 'text-emerald-400', dotClass: 'bg-emerald-500' }
      : { label: 'FALLBACK', colorClass: 'text-amber-400', dotClass: 'bg-amber-500' };

  // D5: Sovereign tools invariant ({ok, count}) from /health?verbose=true
  const [sovereignToolsData, setSovereignToolsData] = useState<{ ok: boolean; count: number } | null>(null);

  const fetchSovereignTools = useCallback(async () => {
    if (!bridge || !online) return;
    try {
      const res = (await bridge.fetchRaw('/health?verbose=true')) as {
        components?: { sovereign_tools?: { ok: boolean; count: number } };
      };
      if (res?.components?.sovereign_tools) {
        setSovereignToolsData(res.components.sovereign_tools);
      }
    } catch {
      setSovereignToolsData(null);
    }
  }, [bridge, online]);

  useEffect(() => {
    fetchSovereignTools();
  }, [fetchSovereignTools]);

  usePolling('overview-sovereign-tools', fetchSovereignTools, {
    interval: 'medium',
    enabled: !!bridge && online,
  });

  const sovereignToolsState = useMemo(() => {
    if (!online) {
      return { label: 'OFFLINE', colorClass: 'text-rose-400', dotClass: 'bg-rose-500' };
    }
    if (!sovereignToolsData) {
      return { label: 'UNKNOWN', colorClass: 'text-slate-400', dotClass: 'bg-slate-500' };
    }
    const isDrift = !sovereignToolsData.ok || sovereignToolsData.count !== 6;
    if (isDrift) {
      return {
        label: `${sovereignToolsData.count}/6 DRIFT`,
        colorClass: 'text-rose-400',
        dotClass: 'bg-rose-500',
      };
    }
    return {
      label: '6/6 VERIFIED',
      colorClass: 'text-emerald-400',
      dotClass: 'bg-emerald-500',
    };
  }, [online, sovereignToolsData]);
  // T-DSH-114: tarjeta del centinela (bloque /health.sentinel + selector de modo).
  type SentinelState = {
    mode?: string;
    loaded?: boolean;
    verdicts?: { safe?: number; controversial?: number; unsafe?: number; unavailable?: number };
    fallbacks?: number;
    cpu_skips?: number;
    max_cpu_pct?: number;
    in_flight?: number;
  };
  const [sentinel, setSentinel] = useState<SentinelState | null>(null);
  const [sentinelBusy, setSentinelBusy] = useState(false);

  const fetchSentinel = useCallback(async () => {
    if (!bridge || !online) return;
    try {
      const res = (await bridge.fetchRaw('/health')) as { sentinel?: SentinelState };
      setSentinel(res?.sentinel ?? null);
    } catch {
      setSentinel(null);
    }
  }, [bridge, online]);

  useEffect(() => {
    fetchSentinel();
  }, [fetchSentinel]);

  usePolling('overview-sentinel', fetchSentinel, {
    interval: 'medium',
    enabled: !!bridge && online,
  });

  const setSentinelMode = useCallback(
    async (mode: string) => {
      if (!bridge) return;
      setSentinelBusy(true);
      try {
        const res = (await bridge.fetchRaw('/api/v1/security/sentinel/mode', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ mode }),
        })) as { state?: SentinelState };
        if (res?.state) setSentinel(res.state);
        else await fetchSentinel();
      } finally {
        setSentinelBusy(false);
      }
    },
    [bridge, fetchSentinel],
  );

  useEffect(() => {
    const fetchMetricsHistory = async () => {
      if (!bridge || document.visibilityState === 'hidden') return;
      try {
        const raw = await bridge.fetchRaw('/api/v1/metrics/history', {}) as { snapshots?: Array<{ ts: number; cpu: number; mem: number }> };
        const snapshots: Array<{ ts: number; cpu: number; mem: number }> = raw?.snapshots ?? [];
        setCpuHistory(snapshots.map((s) => s.cpu));
        setMemHistory(snapshots.map((s) => s.mem));
      } catch { /* ignore — kernel may not expose endpoint yet */ }
    };
    fetchMetricsHistory();
  }, [bridge]);

  useEffect(() => {
    const loadStatic = async () => {
      try {
        if (!bridge) return;
        const results = await Promise.allSettled([
          bridge.getBlackboard(),
          bridge.getCollectivePulse(),
          bridge.getInteroception(),
        ]);
        const bb = results[0].status === 'fulfilled' ? results[0].value : null;
        const pulse = results[1].status === 'fulfilled' ? results[1].value : null;
        const intero = results[2].status === 'fulfilled' ? results[2].value : null;

        if (bb) setBlackboard(bb);
        if (pulse) setCollectivePulse(pulse);
        if (intero) setInteroceptionData(intero);
      } catch {}
    };
    loadStatic();
  }, [bridge]);

  // Polling via centralized coordinator (replaces 2 scattered setInterval calls)
  usePolling('overview-metrics', async () => {
    if (!bridge) return;
    try {
      const raw = await bridge.fetchRaw('/api/v1/metrics/history', {}) as { snapshots?: Array<{ ts: number; cpu: number; mem: number }> };
      const snapshots: Array<{ ts: number; cpu: number; mem: number }> = raw?.snapshots ?? [];
      setCpuHistory(snapshots.map((s) => s.cpu));
      setMemHistory(snapshots.map((s) => s.mem));
    } catch {}
  }, { interval: 'fast', enabled: !!bridge });
  usePolling('overview-static', async () => {
    if (!bridge) return;
    try {
      const results = await Promise.allSettled([
        bridge.getBlackboard(),
        bridge.getCollectivePulse(),
        bridge.getInteroception(),
      ]);
      const bb = results[0].status === 'fulfilled' ? results[0].value : null;
      const pulse = results[1].status === 'fulfilled' ? results[1].value : null;
      const intero = results[2].status === 'fulfilled' ? results[2].value : null;
      if (bb) setBlackboard(bb);
      if (pulse) setCollectivePulse(pulse);
      if (intero) setInteroceptionData(intero);
    } catch {}
  }, { interval: 'idle', enabled: !!bridge });

  // Update live metrics from SSE events or sysStatus
  useEffect(() => {
    // Priority 1: SSE metrics event
    const metricsEvent = events.find(e => e.type === 'metrics');
    const source = metricsEvent?.data || sysStatus;
    
    if (source) {
      const m = source as { curriculum?: { entries?: CurriculumEntry[] }; nodes_count?: number; storage?: { nodes_count?: number } };
      const curriculum = m.curriculum || {};
      const entries: CurriculumEntry[] = curriculum.entries ?? [];
      const totalCalls = entries.reduce((s: number, e: CurriculumEntry) => s + (e.total ?? 0), 0);
      const totalSuccess = entries.reduce((s: number, e: CurriculumEntry) => s + (e.successes ?? 0), 0);
      const avgLat = entries.length > 0
        ? entries.reduce((s: number, e: CurriculumEntry) => s + (e.avg_latency_ms ?? 0), 0) / entries.length : 0;
      
      setLiveMetrics({
        totalCalls,
        successRate: totalCalls > 0 ? totalSuccess / totalCalls : 0,
        avgLatency: Math.round(avgLat),
        activeAgents: blackboard?.active_agents?.length || 0,
        broadcastsLastHour: collectivePulse?.broadcasts_last_hour ?? 0,
        graphNodes: m.nodes_count || m.storage?.nodes_count || 0,
      });

      const current = m.nodes_count || m.storage?.nodes_count || 0;
      if (current) {
        if (prevSilvaCount > 0 && current > prevSilvaCount) {
          setSilvaDelta(current - prevSilvaCount);
        }
        setPrevSilvaCount(current);
      }
    }
  }, [events, blackboard, sysStatus, collectivePulse, prevSilvaCount]);

  const activeGuilds = guilds?.filter(g => g.running).length || 0;
  
  const errRate = goldenSignals?.errors?.rate_percent ?? 0;
  const memSat = goldenSignals?.saturation?.memory_percent ?? 0;

  const getStatusColor = (status: string) => {
    if (status === 'healthy' || status === 'stable') return 'text-emerald-400';
    if (status === 'degraded') return 'text-amber-400';
    return 'text-red-400';
  };

  const getPercentColor = (pct: number) => {
    if (pct < 10) return 'text-emerald-400';
    if (pct < 30) return 'text-amber-400';
    return 'text-red-400';
  };

  const { agentProfiles } = useNexus();
  const isProfileResolved = Boolean(agentProfiles && agentProfiles.length > 0);

  const totalNodes = memoryStats?.node_count ?? goldenSignals?.saturation?.node_count ?? liveMetrics?.graphNodes ?? 0;
  const hasKernelData = totalNodes > 0 || isProfileResolved;

  // The welcome onboarding checklist should appear ONLY when the kernel has zero data,
  // unless the wizard was already marked completed in localStorage.
  const isWizardCompleted = localStorage.getItem('tylluan_wizard_query') === 'true' && localStorage.getItem('tylluan_wizard_mcp') === 'true';
  const isFirstStart = !hasKernelData && !isWizardCompleted;

  if (isFirstStart) {
    return (
      <WelcomeEmptyState
        bridge={bridge}
        sysStatus={sysStatus}
        sessions={Array.isArray(sysStatus?.sessions) ? sysStatus.sessions : []}
        memoryStats={memoryStats as unknown as Record<string, unknown> | null}
        notify={notify}
        onRefresh={() => {
          setRefreshTrigger(p => p + 1);
          if (bridge) {
            refreshData();
          }
        }}
      />
    );
  }

  // Derive human-readable status from real data
  const activeAgentCount = blackboard?.active_agents?.length || 0;
  const recentEvents = events.filter(e => (Date.now() / 1000 - e.ts) < 300);
  const recentToolCalls = recentEvents.filter(e => e.type === 'tool_call');
  const recentMemoryOps = recentEvents.filter(e => e.type.startsWith('memory'));
  const isNightMode = sysStatus?.night_consolidation_active || false;

  let tylluanStatus: { text: string; subtext: string; color: string; pulse: boolean };
  if (isNightMode) {
    tylluanStatus = { text: 'Consolidando memoria nocturna', subtext: 'El sustrato está reorganizando y consolidando el conocimiento acumulado.', color: 'bg-white dark:bg-card border-[#ACA899] dark:border-border text-[#0A246A] dark:text-primary', pulse: false };
  } else if (activeAgentCount > 0 && recentToolCalls.length > 0) {
    tylluanStatus = { text: `Operando con ${activeAgentCount} agente${activeAgentCount > 1 ? 's' : ''}`, subtext: `${recentToolCalls.length} llamadas a herramientas en los últimos 5 minutos.`, color: 'bg-white dark:bg-card border-[#ACA899] dark:border-border text-[#008000] dark:text-emerald-400', pulse: false };
  } else if (recentMemoryOps.length > 0) {
    tylluanStatus = { text: 'Procesando memoria', subtext: `${recentMemoryOps.length} operaciones de memoria recientes en SilvaDB.`, color: 'bg-white dark:bg-card border-[#ACA899] dark:border-border text-[#0A246A] dark:text-primary', pulse: false };
  } else if (activeAgentCount > 0) {
    tylluanStatus = { text: `${activeAgentCount} agente${activeAgentCount > 1 ? 's' : ''} en línea`, subtext: 'En espera de instrucciones de la flota.', color: 'bg-white dark:bg-card border-[#ACA899] dark:border-border text-[#B8860B] dark:text-amber-400', pulse: false };
  } else {
    tylluanStatus = { text: 'En espera', subtext: 'Sin agentes conectados en este momento. El sustrato permanece en reposo.', color: 'bg-white dark:bg-card border-[#ACA899] dark:border-border text-[#444444] dark:text-muted-foreground', pulse: false };
  }

  return (
    <div className="space-y-3 font-sans">
      {/* What is Tylluan doing now — human-readable status */}
      <fieldset className="border border-[#ACA899] dark:border-border p-2.5 bg-[#ECE9D8] dark:bg-card rounded-sm">
        <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">Estado de Actividad del Sustrato</legend>
        <div className="flex items-center gap-2.5">
          <span className={`w-2.5 h-2.5 rounded-none border border-[#7F9DB9] ${tylluanStatus.pulse ? 'bg-emerald-600' : 'bg-[#0A246A]'}`} aria-hidden="true" />
          <div>
            <h3 className="text-xs font-bold text-[#000000] dark:text-foreground" aria-label={`Tylluan is ${tylluanStatus.text.toLowerCase()}`}>
              {tylluanStatus.text}
            </h3>
            <p className="text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5">{tylluanStatus.subtext}</p>
          </div>
        </div>
      </fieldset>

      {/* Homeostasis Hero Widget */}
      <HomeostasisWidget bridge={bridge} />

      {/* Health Banner */}
      {healthDetailed && typeof healthDetailed.score === 'number' && healthDetailed.score < 70 && (
        <div role="alert" className="p-3.5 rounded bg-amber-500/10 border border-amber-500/30 flex items-center gap-3">
          <AlertTriangle className="w-5 h-5 text-amber-600 shrink-0" aria-hidden="true" />
          <div className="flex-1">
            <h4 className="text-xs font-bold text-amber-600">Rendimiento Degradado (Salud: {healthDetailed.score}%)</h4>
            <p className="text-[11px] text-muted-foreground">El kernel experimenta mayor latencia o restricciones de memoria/embeddings.</p>
          </div>
          <button 
            type="button"
            aria-label="Ejecutar corrección de mantenimiento VACUUM"
            onClick={() => bridge?.fetchRaw('/api/v1/maintenance/vacuum', { method: 'POST' })}
            className="px-3 py-1.5 bg-amber-500/20 hover:bg-amber-500/30 text-amber-700 dark:text-amber-300 rounded text-xs font-bold border border-amber-500/40 transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            VACUUM FIX
          </button>
        </div>
      )}

      {/* Hub Status Badges */}
      <fieldset aria-label="Resumen de Telemetría del Sistema" className="border border-[#ACA899] dark:border-border p-2 bg-[#ECE9D8] dark:bg-card rounded-sm">
        <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">Señales del Sistema</legend>
        <div className="flex flex-wrap gap-2 font-mono text-xs">
          <div className="flex items-center gap-1.5 px-2 py-0.5 rounded-none bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border">
            <div className={cn("w-2 h-2 rounded-none", kernelHealth.dotClass)} aria-hidden="true"></div>
            <span className="text-[11px] text-[#444444] dark:text-muted-foreground">Kernel: <span className={cn("font-bold text-[#000000] dark:text-foreground", kernelHealth.colorClass)}>{kernelHealth.label}</span></span>
          </div>
          <div className="flex items-center gap-1.5 px-2 py-0.5 rounded-none bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border">
            <div className={cn("w-2 h-2 rounded-none", embeddingsState.dotClass)} aria-hidden="true"></div>
            <span className="text-[11px] text-[#444444] dark:text-muted-foreground">Embeddings: <span className={cn("font-bold text-[#000000] dark:text-foreground", embeddingsState.colorClass)}>{embeddingsState.label}</span></span>
          </div>
          <div className="flex items-center gap-1.5 px-2 py-0.5 rounded-none bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border">
            <div className={cn("w-2 h-2 rounded-none", sovereignToolsState.dotClass)} aria-hidden="true"></div>
            <span className="text-[11px] text-[#444444] dark:text-muted-foreground">Herramientas Soberanas: <span className={cn("font-bold text-[#000000] dark:text-foreground", sovereignToolsState.colorClass)}>{sovereignToolsState.label}</span></span>
          </div>
          <div className="flex items-center gap-1.5 px-2 py-0.5 rounded-none bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border">
            <span className="text-[11px] font-bold text-[#008000] dark:text-emerald-400">{liveMetrics?.activeAgents || 0}</span>
            <span className="text-[11px] text-[#444444] dark:text-muted-foreground">Agentes activos</span>
          </div>
          <div className="flex items-center gap-1.5 px-2 py-0.5 rounded-none bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border">
            <span className="text-[11px] font-bold text-[#0A246A] dark:text-primary">{memoryStats?.node_count || 0}</span>
            <span className="text-[11px] text-[#444444] dark:text-muted-foreground">Nodos Silva</span>
          </div>
          <div className="flex items-center gap-1.5 px-2 py-0.5 rounded-none bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border">
            <MiniSparkline data={cpuHistory} color="hsl(var(--primary))" />
            <span className="text-[11px] text-[#444444] dark:text-muted-foreground">CPU</span>
          </div>
          <div className="flex items-center gap-1.5 px-2 py-0.5 rounded-none bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border">
            <MiniSparkline data={memHistory} color="hsl(var(--primary))" />
            <span className="text-[11px] text-[#444444] dark:text-muted-foreground">RAM</span>
          </div>
        </div>
      </fieldset>

      {/* T-DSH-114: centinela de write_gate L2 (opt-in, default off) */}
      <fieldset className="border border-[#ACA899] dark:border-border p-2.5 bg-[#ECE9D8] dark:bg-card rounded-sm font-sans">
        <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">Centinela de Escritura (write_gate L2)</legend>
        <div className="flex items-center justify-between gap-3 flex-wrap">
          <div>
            <p className="text-[11px] text-[#444444] dark:text-muted-foreground">
              {sentinel
                ? `Modo: ${sentinel.mode ?? 'inactivo'} · Cargado: ${sentinel.loaded ? 'sí' : 'no'} · Fallbacks: ${sentinel.fallbacks ?? 0} · Saltos CPU: ${sentinel.cpu_skips ?? 0} (umbral ${sentinel.max_cpu_pct ?? 85}%)`
                : 'Estado no disponible'}
            </p>
            {sentinel?.verdicts && (
              <p className="text-[11px] font-mono text-[#555555] dark:text-muted-foreground mt-0.5">
                Seguro: {sentinel.verdicts.safe ?? 0} · Polémico: {sentinel.verdicts.controversial ?? 0} · Inseguro: {sentinel.verdicts.unsafe ?? 0} · No disponible: {sentinel.verdicts.unavailable ?? 0}
              </p>
            )}
          </div>
          <div className="flex items-center gap-1" role="group" aria-label="Modo del centinela">
            {[
              { id: 'off', label: 'DESACTIVADO' },
              { id: 'observe', label: 'OBSERVACIÓN' },
              { id: 'enforce', label: 'BLOQUEO' }
            ].map((m) => (
              <button
                key={m.id}
                type="button"
                disabled={sentinelBusy || !bridge}
                onClick={() => setSentinelMode(m.id)}
                className={`px-2 py-0.5 rounded-none text-[10px] font-bold border transition-colors cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A] disabled:opacity-50 ${
                  sentinel?.mode === m.id
                    ? 'bg-[#0A246A] text-white border-[#0A246A]'
                    : 'bg-white dark:bg-muted border-[#ACA899] dark:border-border text-[#000000] dark:text-foreground hover:bg-[#F5F4EA]'
                }`}
              >
                {m.label}
              </button>
            ))}
          </div>
        </div>
      </fieldset>

      {liveMetrics && (
        <fieldset className="border border-[#ACA899] dark:border-border p-2.5 bg-[#ECE9D8] dark:bg-card rounded-sm font-sans">
          <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">Métricas de Operación en Tiempo Real</legend>
          <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-2">
            {[ 
              { label: 'Llamadas Totales', value: liveMetrics.totalCalls },
              { label: 'Tasa de Éxito', value: `${(liveMetrics.successRate * 100).toFixed(0)}%` },
              { label: 'Latencia Media', value: `${liveMetrics.avgLatency}ms` },
              { label: 'Agentes Activos', value: liveMetrics.activeAgents },
              { label: 'Difusiones/h', value: liveMetrics.broadcastsLastHour },
              { label: 'Nodos SilvaDB', value: liveMetrics.graphNodes },
            ].map(({ label, value }) => (
              <div key={label} className="bg-white dark:bg-card border border-[#ACA899] dark:border-border rounded-none p-2 text-center">
                <div className="text-sm font-bold font-mono text-[#000000] dark:text-foreground">{value}</div>
                <div className="text-[10px] text-[#555555] dark:text-muted-foreground mt-0.5">{label}</div>
              </div>
            ))}
          </div>
        </fieldset>
      )}
      {sysStatus && (
        <div className="mt-4 flex gap-2 flex-wrap font-mono text-xs">
          <span className={`text-[11px] px-2 py-0.5 rounded border ${sysStatus.silva_healthy ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/30' : 'bg-destructive/15 text-destructive border border-destructive/30'}`}>
            SilvaDB: {sysStatus.silva_healthy ? '● CORRECTO' : '■ ERROR'}
          </span>
          <span className={`text-[11px] px-2 py-0.5 rounded border ${sysStatus.mailbox_healthy ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/30' : 'bg-destructive/15 text-destructive border border-destructive/30'}`}>
            Buzón: {sysStatus.mailbox_healthy ? '● CORRECTO' : '■ ERROR'}
          </span>
          <span className="text-[11px] px-2 py-0.5 rounded bg-muted border border-border text-foreground">
            Curriculum: {sysStatus.curriculum_entries ?? 0} entradas
          </span>
          <span className="text-[11px] px-2 py-0.5 rounded bg-muted border border-border text-foreground">
            Activo: {Math.floor((sysStatus.uptime_secs ?? 0) / 60)}m
          </span>
          <span className={`text-[11px] px-2 py-0.5 rounded border ${sysStatus.embeddings_loaded
            ? 'bg-primary/15 text-primary border border-primary/30'
            : 'bg-amber-500/15 text-amber-600 dark:text-amber-400 border border-amber-500/30'}`}>
            Embeddings: {sysStatus.embeddings_loaded ? 'mxbai-embed-large' : '▲ FTS5 fallback'}
          </span>
        </div>
      )}
      <div className="flex items-center justify-between bg-card border border-border rounded p-4 relative font-sans">
        <div className="flex items-center gap-4">
          <div className="relative">
            <div className="w-12 h-12 rounded border border-primary/30 flex items-center justify-center bg-primary/10">
              <Activity className="w-6 h-6 text-primary" aria-hidden="true" />
            </div>
            <div className="absolute -bottom-1 -right-1 w-4 h-4 bg-card rounded flex items-center justify-center border border-border">
              <div className="w-2 h-2 bg-emerald-600 rounded-full" aria-hidden="true"></div>
            </div>
          </div>
          <div>
            <h3 className="text-xs font-semibold text-foreground">Pulso del Kernel Soberano</h3>
            <p className="text-[11px] text-muted-foreground mt-0.5">MOTOR O3 · TIEMPO ACTIVO: {Math.floor((goldenSignals?.uptime_seconds || 0) / 3600)}h {Math.floor(((goldenSignals?.uptime_seconds || 0) % 3600) / 60)}m</p>
          </div>
        </div>
        <div className="text-right">
          <div className={cn("text-xs font-semibold tracking-tight", sloSummary ? getStatusColor(sloSummary.status) : "text-muted-foreground")}>
            {sloSummary?.status ? (sloSummary.status === 'healthy' ? '● SALUDABLE' : sloSummary.status === 'degraded' ? '▲ DEGRADADO' : '■ CRÍTICO') : '○ TELEMETRÍA EN ESPERA'}
          </div>
          <div className="text-[11px] text-muted-foreground mt-0.5">Disponibilidad: {sloSummary?.current_availability !== undefined ? `${sloSummary.current_availability.toFixed(3)}%` : '—'}</div>
        </div>
      </div>

      {/* Golden Signals Row */}
      <fieldset className="border border-[#ACA899] dark:border-border p-2.5 bg-[#ECE9D8] dark:bg-card rounded-sm font-sans">
        <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">Señales Doradas de Rendimiento (Golden Signals)</legend>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
          <MetricCard
            label="Tráfico"
            value={goldenSignals?.traffic?.active_tools || 0} unit=" ops"
            sub={`${activeGuilds} gremios activos`}
            valueClass="text-[#0A246A] dark:text-primary"
          />
          <MetricCard
            label="Tasa de Error"
            value={errRate} unit="%"
            sub={`${goldenSignals?.errors?.total_errors || 0} eventos no críticos`}
            valueClass={getPercentColor(errRate)}
          />
          <MetricCard
            label="Saturación"
            value={memSat} unit="%"
            sub={`${memoryStats?.node_count || 0} nodos indexados`}
            valueClass={getPercentColor(memSat)}
          />
          <MetricCard
            label="Disponibilidad SLO"
            value={sloSummary?.slo_target ?? 99.9} unit="%"
            sub={sloSummary ? `${sloSummary.error_budget_remaining_percent ?? 0}% presupuesto restante` : 'Objetivo 99.9%'}
            valueClass="text-[#0A246A] dark:text-primary"
          />
        </div>
      </fieldset>

      {/* Cognitive Health */}
      {interoceptionData && (
        <fieldset className="border border-[#ACA899] dark:border-border p-2.5 bg-[#ECE9D8] dark:bg-card rounded-sm font-sans">
          <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">Salud Cognitiva e Interocepción</legend>
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
            {[
              { label: 'Homeostasis', value: interoceptionData.homeostasis, good: (v: number) => v > 0.7, warn: (v: number) => v > 0.4 },
              { label: 'Estrés', value: interoceptionData.stress_level, good: (v: number) => v < 0.3, warn: (v: number) => v < 0.6, invert: true },
              { label: 'Densidad Silva', value: interoceptionData.graph_density, good: (v: number) => v > 0.001, warn: (v: number) => v > 0.0001 },
              { label: 'Feromonas Activas', value: interoceptionData.active_pheromones / 20, good: (v: number) => v < 0.5, warn: (v: number) => v < 0.9 },
            ].map(({ label, value, good, warn, invert }) => {
              const pct = Math.min(100, value * 100);
              const colorClass = good(value) ? 'text-emerald-600 dark:text-emerald-400' : warn(value) ? 'text-amber-600 dark:text-amber-400' : 'text-red-600 dark:text-red-400';
              const bgClass = good(value) ? 'bg-emerald-600' : warn(value) ? 'bg-amber-600' : 'bg-red-600';
              return (
                <div key={label} className="p-2 rounded-none bg-white dark:bg-card border border-[#ACA899] dark:border-border">
                  <div className="text-[10px] font-bold text-[#01285F] dark:text-muted-foreground mb-0.5">{label}</div>
                  <div className={cn("text-sm font-bold font-mono", colorClass)}>{pct.toFixed(0)}%</div>
                  <div className="w-full bg-[#E0E0E0] dark:bg-muted h-1.5 mt-1 rounded-none border border-[#ACA899]">
                    <div className={cn("h-full", bgClass)} style={{ width: `${pct}%` }} />
                  </div>
                </div>
              );
            })}
          </div>
        </fieldset>
      )}

      {/* Multi-Agent Activity */}
      <div className="rounded bg-card border border-border overflow-hidden font-sans">
        <div className="px-4 py-2.5 border-b border-border flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-xs font-bold text-foreground">Flujo de Actividad Soberana</span>
          </div>
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-1.5 text-[11px] text-muted-foreground cursor-pointer select-none hover:text-foreground transition-colors">
              <input
                type="checkbox"
                checked={showHeartbeats}
                onChange={e => setShowHeartbeats(e.target.checked)}
                className="accent-primary w-3 h-3"
              />
              Mostrar latidos
            </label>
            <span className="text-[11px] text-muted-foreground font-mono">EVENTOS EN VIVO</span>
            <div className="w-2 h-2 rounded-full bg-emerald-600" aria-hidden="true" />
          </div>
        </div>
        <div className="divide-y divide-border/60 font-sans">
          {silvaDelta > 0 && (
             <div className="px-4 py-2 flex items-center justify-between hover:bg-muted/30 transition-colors">
                <div className="flex items-center gap-2 overflow-hidden">
                  <div className="w-2 h-2 rounded-full shrink-0 bg-emerald-600" />
                  <span className="text-xs text-foreground">
                    <span className="font-bold text-emerald-600">{silvaDelta} nodos nuevos</span> integrados en los últimos 30s
                  </span>
                </div>
             </div>
          )}

          <div className="px-4 py-2.5 flex flex-wrap gap-2.5 bg-muted/20 border-b border-border">
            {blackboard?.active_agents.map(agent => (
              <div key={agent} className="flex items-center gap-1.5">
                <div className="w-6 h-6 rounded flex items-center justify-center border border-border bg-muted font-mono font-bold text-[11px] text-foreground">
                  {agent.slice(0, 2).toUpperCase()}
                </div>
                <div className="flex flex-col">
                   <span className="text-[11px] font-semibold text-foreground font-mono">@{agent.split('-')[0]}</span>
                   <span className="text-[10px] text-muted-foreground">activo</span>
                </div>
              </div>
            ))}
            {(!blackboard?.active_agents || blackboard.active_agents.length === 0) && (
              <div className="flex items-center gap-2 text-muted-foreground">
                <span className="text-xs italic text-muted-foreground">Sin agentes activos detectados</span>
              </div>
            )}
          </div>

          <div
            tabIndex={0}
            role="region"
            aria-label="Registro de eventos recientes"
            className="max-h-96 overflow-y-auto divide-y divide-border/40 focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
          >
          {events
            .filter(e => ['tool_call', 'memory_added', 'memory_updated', 'heartbeat', 'log', 'maintenance_started', 'maintenance_finished'].includes(e.type))
            .filter(e => showHeartbeats || (e.type !== 'uptime' && !((e.data as Record<string, unknown>)?.uptime_secs !== undefined)))
            .slice(0, 15)
            .map((e, i) => {
              const d = (e.data || {}) as Record<string, unknown>;
              return (
              <div key={`${e.ts}-${i}`} className="px-4 py-2 flex items-center justify-between hover:bg-muted/30 transition-colors">
                <div className="flex items-center gap-2.5 min-w-0">
                  <div className={cn(
                    "w-2 h-2 rounded-full shrink-0",
                    e.type === 'tool_call' ? "bg-primary" : 
                    e.type.startsWith('memory') ? "bg-primary" :
                    e.type.startsWith('maintenance') ? "bg-amber-600" :
                    e.type === 'heartbeat' ? "bg-emerald-600" : "bg-muted-foreground"
                  )} />
                  <div className="flex items-center gap-1.5 min-w-0 text-xs font-mono">
                    <span className="font-bold text-foreground shrink-0">
                      {e.source === 'mcp' ? 'MCP' : String(d.agent_id || 'Sistema')}
                    </span>
                    <span className="text-muted-foreground">·</span>
                    <span className="text-muted-foreground truncate">
                      {e.type === 'tool_call' ? `tool: ${String(d.tool ?? '')}` : 
                       e.type === 'heartbeat' ? `uptime: ${String(d.uptime_secs ?? '')}s` :
                       e.type.startsWith('maintenance') ? `maint: ${String(d.task ?? '')}` :
                       e.type}
                    </span>
                  </div>
                </div>
                <div className="text-[11px] text-muted-foreground font-mono shrink-0 ml-4">
                  <RelativeTime ts={e.ts} />
                </div>
              </div>
              );
            })}
            {events.length === 0 && !silvaDelta && !blackboard?.pending?.length && (
              <div className="p-8 text-center text-xs text-muted-foreground font-mono">
                Modo Silencioso — En espera de señales
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Running Guilds & Pending Approvals */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4 font-sans">
        <div className="lg:col-span-2 rounded bg-card border border-border overflow-hidden">
          <div className="px-4 py-2.5 border-b border-border flex items-center justify-between">
            <span className="text-xs font-bold text-foreground">Gremios Operacionales</span>
            <span className="text-[11px] font-mono text-muted-foreground">{activeGuilds} / {guilds?.length || 0} activos</span>
          </div>
          <div
            tabIndex={0}
            role="region"
            aria-label="Lista de guilds operacionales"
            className="p-3 grid grid-cols-1 md:grid-cols-2 gap-2 max-h-64 overflow-y-auto focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
          >
            {guilds?.map((g) => (
              <div key={g.name} className={cn(
                "flex items-center justify-between p-2 rounded border transition-colors",
                g.running ? "bg-muted/40 border-border" : "bg-card border-transparent opacity-50"
              )}>
                <div className="flex items-center gap-2">
                  <div className={cn("w-2 h-2 rounded-full", g.running ? "bg-emerald-600" : "bg-muted-foreground")}></div>
                  <span className="text-xs font-mono text-foreground">{g.name}</span>
                  {(g.restarts_5m ?? 0) > 3 && (
                    <span className="text-[11px] bg-red-600 text-white px-1 rounded-sm font-bold">DEGRADADO</span>
                  )}
                </div>
                <div className="flex gap-1 shrink-0 font-mono">
                   {g.launcher_type && (
                    <span className={cn("text-[11px] px-1 rounded-sm uppercase font-bold flex items-center", 
                      g.launcher_type === 'http' ? "text-primary border border-primary/30" :
                      g.launcher_type === 'stdio' ? "text-amber-600 border border-amber-500/30" :
                      "text-primary border border-primary/30"
                    )}>
                      {g.launcher_type}
                    </span>
                  )}
                  <span className="text-[11px] text-muted-foreground">{g.tools_count}T</span>
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="rounded bg-card border border-border overflow-hidden flex flex-col font-sans">
          <div className="px-4 py-2.5 border-b border-border flex items-center justify-between">
            <span className="text-xs font-bold text-foreground">Puerta HITL</span>
          </div>
          <div className="flex-1 p-4 flex flex-col items-center justify-center text-center">
            {approvals.length === 0 ? (
              <>
                <p className="text-xs font-semibold text-foreground">Autorización Máxima</p>
                <p className="text-[11px] text-muted-foreground mt-0.5">Sin aprobaciones pendientes requeridas.</p>
              </>
            ) : (
              <div className="w-full space-y-2">
                <p className="text-xs font-bold text-amber-600 mb-2">{approvals.length} ACCIONES BLOQUEADAS</p>
                {approvals.slice(0, 3).map((a) => (
                  <div key={a.id} className="p-2 bg-amber-500/10 border border-amber-500/30 rounded text-left overflow-hidden">
                    <div className="text-[11px] font-mono text-amber-600 truncate">{a.id}</div>
                    <div className="text-[11px] text-foreground truncate">{a.tool}</div>
                  </div>
                ))}
                <button
                  type="button"
                  className="w-full py-1.5 bg-amber-500/20 hover:bg-amber-500/30 text-amber-700 dark:text-amber-300 rounded text-xs font-bold border border-amber-500/40 mt-2 cursor-pointer transition-colors focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                >
                  GESTIONAR APROBACIONES
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Guild Utilization, Memory Retention, Night Consolidation & Canary Assertions */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 font-sans">
        {/* Tarjeta 1 — Guild Utilization */}
        <div className="p-4 rounded bg-card border border-border">
          <div className="flex items-center justify-between mb-4">
            <div className="text-xs font-bold text-foreground">
              Utilización de Gremios
            </div>
            <span className="text-[11px] font-mono text-muted-foreground">
              {guildsUtilization?.active || 0} / {guildsUtilization?.total || 0}
            </span>
          </div>
          <div className="space-y-3">
            {(guildsUtilization?.active_guilds || []).slice(0, 6).map((guild: { name: string; idle_secs?: number }) => {
              const idleSecs = guild.idle_secs ?? 0;
              const utilPct = idleSecs < 60 ? 80 : idleSecs < 300 ? 40 : 10;
              const barColor = utilPct >= 70 ? 'bg-emerald-600' : utilPct >= 40 ? 'bg-amber-600' : 'bg-muted-foreground';
              
              return (
                <div key={guild.name} className="space-y-1">
                  <div className="flex justify-between items-center text-[11px]">
                    <span className="text-foreground font-mono truncate mr-2">{guild.name}</span>
                    <span className="text-muted-foreground font-mono">{utilPct}%</span>
                  </div>
                  <div className="h-1.5 w-full bg-muted rounded overflow-hidden border border-border">
                    <div 
                      className={cn("h-full rounded transition-all duration-500", barColor)} 
                      style={{ width: `${utilPct}%` }} 
                     />
                  </div>
                </div>
              );
            })}
            {(!guildsUtilization?.active_guilds || guildsUtilization.active_guilds.length === 0) && (
              <div className="py-6 text-center">
                <p className="text-xs text-muted-foreground italic">Sin gremios activos monitorizados</p>
              </div>
            )}
          </div>
        </div>

        {/* Tarjeta 2 — Memory Retention */}
        <div className="p-4 rounded bg-card border border-border flex flex-col items-center justify-center text-center">
          <div className="text-xs font-bold text-foreground mb-4">
            Retención de Memoria
          </div>
          
          {memoryRetention ? (
            <div className="space-y-2">
              <div className={cn(
                "text-5xl font-bold font-mono transition-colors duration-300",
                memoryRetention.silva.retention_rate_percent >= 80 ? "text-emerald-600" :
                memoryRetention.silva.retention_rate_percent >= 60 ? "text-amber-600" : "text-red-600"
              )}>
                {memoryRetention.silva.retention_rate_percent}%
              </div>
              <div className="space-y-0.5">
                <div className="text-sm font-bold text-foreground font-mono">
                  {memoryRetention.silva.total_nodes.toLocaleString()}
                </div>
                <div className="text-[11px] text-muted-foreground">
                  Nodos Soberanos Totales
                </div>
              </div>
            </div>
          ) : (
            <div className="animate-pulse space-y-4">
              <div className="h-12 w-24 bg-muted rounded mx-auto" />
              <div className="h-4 w-32 bg-muted rounded mx-auto" />
            </div>
          )}
        </div>

        {/* Tarjeta 3 — Night Consolidation */}
        <DreamStatusWidget bridge={bridge} />

        {/* Tarjeta 4 — Canary Assertions */}
        <CanaryStatusWidget bridge={bridge} />
      </div>

      {/* Recent Sessions (Digests) */}
      <div className="rounded bg-card border border-border overflow-hidden font-sans">
        <div className="px-4 py-2.5 border-b border-border flex items-center justify-between">
          <span className="text-xs font-bold text-foreground">Sesiones Recientes</span>
        </div>
        <RecentSessions bridge={bridge} />
      </div>
    </div>
  );
}

interface SessionDigest {
  id?: string;
  created_at: number | string;
  content?: string;
  summary?: string;
  agent_id?: string;
}

function RecentSessions({ bridge }: { bridge: NexusBridge | null }) {
  const [digests, setDigests] = useState<SessionDigest[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!bridge) return;
    const load = async () => {
      try {
        const data = await bridge.fetchSessionDigests(3);
        setDigests(Array.isArray(data) ? data : []);
      } catch (err) {
        console.error('Failed to load session digests:', err);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [bridge]);

  if (loading) {
    return <div className="p-6 text-center text-xs text-muted-foreground font-mono animate-pulse">Analizando hipocampo...</div>;
  }

  if (!digests || digests.length === 0) {
    return <div className="p-6 text-center text-xs text-muted-foreground italic font-mono">Sin resúmenes de sesión disponibles.</div>;
  }

  return (
    <div className="divide-y divide-border/60 font-sans">
      {digests.map((d, i) => (
        <div key={i} className="p-3.5 hover:bg-muted/30 transition-colors">
          <div className="flex justify-between items-start mb-1">
            <div className="text-xs font-bold text-primary font-mono">@{d.agent_id || 'anon'}</div>
            <div className="text-[11px] text-muted-foreground font-mono">{d.created_at}</div>
          </div>
          <p className="text-xs text-foreground leading-relaxed line-clamp-2">
            "{d.content || d.summary}"
          </p>
        </div>
      ))}
    </div>
  );
}
