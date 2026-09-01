import React, { useState, useEffect, useCallback } from 'react';
import {
  Activity,
  Wifi,
  AlertTriangle,
  Database,
  TrendingUp,
  ShieldCheck,
  CheckCircle,
  Cpu,
  Clock,
  History,
  Shield,
  Zap,
  Info,
  User
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

interface OverviewTabProps {
  bridge: NexusBridge | null;
  goldenSignals: GoldenSignals | null;
  guildsUtilization: GuildsUtilization | null;
  memoryRetention: MemoryRetention | null;
  sloSummary: SloSummary | null;
  guilds: Guild[];
  approvals: Approval[];
  memoryStats: MemoryStats | null;
  healthDetailed: any | null;
  sysStatus: any | null;
  events: NexusEvent[];
  notify: (msg: string, type?: 'info' | 'error') => void;
  refreshData: () => Promise<void>;
}

export function OverviewTab({ 
  bridge,
  goldenSignals, 
  guildsUtilization, 
  memoryRetention, 
  sloSummary, 
  guilds, 
  approvals, 
  memoryStats,
  healthDetailed,
  sysStatus,
  events,
  notify,
  refreshData
}: OverviewTabProps) {
  const [refreshTrigger, setRefreshTrigger] = useState(0);
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
  useEffect(() => {
    const fetchMetricsHistory = async () => {
      if (!bridge || document.visibilityState === 'hidden') return;
      try {
        const raw = await bridge.fetchRaw('/api/v1/metrics/history', {}) as any;
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
      const raw = await bridge.fetchRaw('/api/v1/metrics/history', {}) as any;
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
      const m = source as any;
      const curriculum = m.curriculum || {};
      const entries: any[] = curriculum.entries ?? [];
      const totalCalls = entries.reduce((s: number, e: any) => s + (e.total ?? 0), 0);
      const totalSuccess = entries.reduce((s: number, e: any) => s + (e.successes ?? 0), 0);
      const avgLat = entries.length > 0
        ? entries.reduce((s: number, e: any) => s + (e.avg_latency_ms ?? 0), 0) / entries.length : 0;
      
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
  }, [events, blackboard, sysStatus, collectivePulse]);

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

  const getAgentColor = (id: string) => {
    const lid = id.toLowerCase();
    if (lid.includes('claude')) return 'bg-blue-500/20 text-blue-400 border-blue-500/30';
    if (lid.includes('gemini') || lid.includes('antigravity')) return 'bg-red-500/20 text-red-400 border-red-500/30';
    if (lid.includes('qwen')) return 'bg-orange-500/20 text-orange-400 border-orange-500/30';
    return 'bg-slate-800 text-slate-400 border-slate-700';
  };

  const { agentProfiles } = useNexus();
  const isProfileResolved = agentProfiles && agentProfiles.length > 0;
  const isModelLoaded = sysStatus?.embeddings_loaded || sysStatus?.silva_healthy;

  const isFirstStart = (memoryStats?.node_count === 0 || liveMetrics?.graphNodes === 0) &&
    !(isProfileResolved && isModelLoaded) &&
    !(localStorage.getItem('tylluan_wizard_query') === 'true' && localStorage.getItem('tylluan_wizard_mcp') === 'true');

  if (isFirstStart) {
    return (
      <WelcomeEmptyState
        bridge={bridge}
        sysStatus={sysStatus}
        sessions={sysStatus?.sessions || []}
        memoryStats={memoryStats}
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
    tylluanStatus = { text: 'Consolidando memoria nocturna', subtext: 'Tylluan está reorganizando y consolidando conocimiento acumulado.', color: 'bg-violet-500/10 border-violet-500/30 text-violet-300', pulse: true };
  } else if (activeAgentCount > 0 && recentToolCalls.length > 0) {
    tylluanStatus = { text: `Trabajando con ${activeAgentCount} agente${activeAgentCount > 1 ? 's' : ''}`, subtext: `${recentToolCalls.length} llamadas a herramientas en los últimos 5 minutos.`, color: 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300', pulse: true };
  } else if (recentMemoryOps.length > 0) {
    tylluanStatus = { text: 'Procesando memoria', subtext: `${recentMemoryOps.length} operaciones de memoria recientes.`, color: 'bg-blue-500/10 border-blue-500/30 text-blue-300', pulse: true };
  } else if (activeAgentCount > 0) {
    tylluanStatus = { text: `${activeAgentCount} agente${activeAgentCount > 1 ? 's' : ''} conectado${activeAgentCount > 1 ? 's' : ''}`, subtext: 'Esperando instrucciones.', color: 'bg-amber-500/10 border-amber-500/30 text-amber-300', pulse: false };
  } else {
    tylluanStatus = { text: 'Inactivo', subtext: 'Sin agentes conectados. Tylluan está en standby.', color: 'bg-slate-500/10 border-slate-500/30 text-slate-400', pulse: false };
  }

  return (
    <div className="space-y-6 animate-in fade-in duration-500">
      {/* What is Tylluan doing now — human-readable status */}
      <div className={`rounded-xl border p-4 ${tylluanStatus.color} transition-all duration-500`}>
        <div className="flex items-center gap-3">
          <div className={`w-3 h-3 rounded-full ${tylluanStatus.pulse ? 'bg-emerald-400 animate-pulse' : 'bg-slate-500'}`} />
          <div>
            <h3 className="text-sm font-semibold">Tylluan está {tylluanStatus.text.toLowerCase()}</h3>
            <p className="text-[11px] opacity-70 mt-0.5">{tylluanStatus.subtext}</p>
          </div>
        </div>
      </div>

      {/* Homeostasis Hero Widget */}
      <HomeostasisWidget bridge={bridge} />

      {/* Health Banner */}
      {healthDetailed && healthDetailed.score < 70 && (
        <div className="p-4 rounded-xl bg-amber-500/10 border border-amber-500/20 flex items-center gap-3 animate-pulse">
          <AlertTriangle className="w-5 h-5 text-amber-500 shrink-0" />
          <div className="flex-1">
            <h4 className="text-sm font-bold text-amber-400">Rendimiento Degradado (Health: {healthDetailed.score}%)</h4>
            <p className="text-xs text-amber-500/70">El kernel está experimentando latencia alta o falta de recursos (RAM/Embeddings).</p>
          </div>
          <button 
            onClick={() => bridge?.fetchRaw('/api/v1/maintenance/vacuum', { method: 'POST' })}
            className="px-3 py-1.5 bg-amber-500/20 hover:bg-amber-500/30 text-amber-500 rounded-lg text-[10px] font-bold border border-amber-500/30 transition-colors cursor-pointer"
          >
            VACUUM FIX
          </button>
        </div>
      )}

      {/* Hub Status Pills */}
      <div className="flex flex-wrap gap-2">
        <div className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-slate-900">
          <div className="w-2 h-2 rounded-full bg-emerald-500"></div>
          <span className="text-[11px] font-semibold text-slate-300">Kernel</span>
        </div>
        <div className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-slate-900">
          <div className={cn("w-2 h-2 rounded-full", sysStatus?.embeddings_loaded ? "bg-emerald-500" : "bg-amber-500")}></div>
          <span className="text-[11px] font-semibold text-slate-300">BGE-M3</span>
        </div>
        <div className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-slate-900">
          <span className="text-[11px] font-bold text-emerald-400">{liveMetrics?.activeAgents || 0}</span>
          <span className="text-[11px] font-semibold text-slate-300">Agentes activos</span>
        </div>
        <div className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-slate-900">
          <span className="text-[11px] font-bold text-violet-400">{memoryStats?.node_count || 0}</span>
          <span className="text-[11px] font-semibold text-slate-300">Silva Nodes</span>
        </div>
        <div className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-slate-900">
          <MiniSparkline data={cpuHistory} color="#60a5fa" />
          <span className="text-[11px] font-semibold text-slate-300">CPU</span>
        </div>
        <div className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-slate-900">
          <MiniSparkline data={memHistory} color="#34d399" />
          <span className="text-[11px] font-semibold text-slate-300">RAM</span>
        </div>
      </div>

      {liveMetrics && (
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3">
          {[ 
            { label: 'Llamadas totales', value: liveMetrics.totalCalls, icon: Wifi },
            { label: 'Tasa de éxito', value: `${(liveMetrics.successRate * 100).toFixed(0)}%`, icon: CheckCircle },
            { label: 'Latencia media', value: `${liveMetrics.avgLatency}ms`, icon: Activity },
            { label: 'Agentes activos', value: liveMetrics.activeAgents, icon: Cpu },
            { label: 'Broadcasts/h', value: liveMetrics.broadcastsLastHour, icon: Wifi },
            { label: 'Nodos memoria', value: liveMetrics.graphNodes, icon: Database },
          ].map(({ label, value, icon: Icon }) => (
            <div key={label} className="bg-slate-900/40 rounded-xl p-3 text-center hover:bg-slate-900/60 transition-colors group">
              <Icon className="w-3 h-3 text-slate-600 mx-auto mb-1 group-hover:text-amber-400 transition-colors" />
              <div className="text-lg font-bold font-mono text-slate-100">{value}</div>
              <div className="text-[11px] font-medium text-slate-500 mt-0.5">{label}</div>
            </div>
          ))}
        </div>
      )}
      {sysStatus && (
        <div className="mt-4 flex gap-3 flex-wrap">
          <span className={`text-xs px-2 py-1 rounded-full ${sysStatus.silva_healthy ? 'bg-green-500/20 text-green-300' : 'bg-red-500/20 text-red-300'}`}>
            SilvaDB: {sysStatus.silva_healthy ? 'OK' : 'ERROR'}
          </span>
          <span className={`text-xs px-2 py-1 rounded-full ${sysStatus.mailbox_healthy ? 'bg-green-500/20 text-green-300' : 'bg-red-500/20 text-red-300'}`}>
            Mailbox: {sysStatus.mailbox_healthy ? 'OK' : 'ERROR'}
          </span>
          <span className="text-xs px-2 py-1 rounded-full bg-slate-700 text-slate-300">
            Curriculum: {sysStatus.curriculum_entries} entradas
          </span>
          <span className="text-xs px-2 py-1 rounded-full bg-slate-700 text-slate-300">
            Uptime: {Math.floor(sysStatus.uptime_secs / 60)}m
          </span>
          <span className={`text-xs px-2 py-1 rounded-full ${sysStatus.embeddings_loaded
            ? 'bg-blue-500/20 text-blue-300'
            : 'bg-amber-500/20 text-amber-400'}`}>
            Embeddings: {sysStatus.embeddings_loaded ? 'BGE-M3' : 'FTS5 fallback'}
          </span>
        </div>
      )}
      <div className="flex items-center justify-between bg-slate-900/60 rounded-xl p-4 relative overflow-hidden group">
        <div className="flex items-center gap-4 relative z-10">
          <div className="relative">
            <div className="w-14 h-14 rounded-full border border-amber-500/20 flex items-center justify-center bg-amber-500/5">
              <Activity className="w-7 h-7 text-amber-400 animate-pulse" />
            </div>
            <div className="absolute -bottom-1 -right-1 w-5 h-5 bg-slate-950 rounded-full flex items-center justify-center">
              <div className="w-2 h-2 bg-emerald-500 rounded-full animate-ping"></div>
            </div>
          </div>
          <div>
            <h3 className="text-xs font-semibold text-slate-100">Sovereign Kernel Pulse</h3>
            <p className="text-[10px] text-slate-500 mt-0.5">O3 ENGINE · UPTIME: {Math.floor((goldenSignals?.uptime_seconds || 0) / 3600)}h {Math.floor(((goldenSignals?.uptime_seconds || 0) % 3600) / 60)}m</p>
          </div>
        </div>
        <div className="text-right relative z-10">
          <div className={cn("text-xs font-semibold tracking-tight", sloSummary ? getStatusColor(sloSummary.status) : "text-slate-500")}>
            {sloSummary?.status ? sloSummary.status.toUpperCase() : 'TELEMETRÍA EN REPOSO'}
          </div>
          <div className="text-[10px] text-slate-500 mt-0.5">Availability: {sloSummary?.current_availability !== undefined ? `${sloSummary.current_availability.toFixed(3)}%` : '—'}</div>
        </div>
      </div>

      {/* Golden Signals Row */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard
          icon={Wifi} label="Traffic"
          value={goldenSignals?.traffic?.active_tools || 0} unit=" ops"
          sub={`${activeGuilds} guilds active`}
          valueClass="text-blue-400"
        />
        <MetricCard
          icon={AlertTriangle} label="Error Rate"
          value={errRate} unit="%"
          sub={`${goldenSignals?.errors?.total_errors || 0} non-critical events`}
          valueClass={getPercentColor(errRate)}
        />
        <MetricCard
          icon={Database} label="Saturation"
          value={memSat} unit="%"
          sub={`${memoryStats?.node_count || 0} nodes indexed`}
          valueClass={getPercentColor(memSat)}
        />
        <MetricCard
          icon={TrendingUp} label="SLO"
          value={sloSummary?.slo_target ?? 99.9} unit="%"
          sub={sloSummary ? `${sloSummary.error_budget_remaining_percent ?? 0}% budget left` : 'Objetivo 99.9%'}
          valueClass="text-violet-400"
        />
      </div>

      {/* Cognitive Health */}
      {interoceptionData && (
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
          {[
            { label: 'Homeostasis', value: interoceptionData.homeostasis, good: (v: number) => v > 0.7, warn: (v: number) => v > 0.4 },
            { label: 'Stress', value: interoceptionData.stress_level, good: (v: number) => v < 0.3, warn: (v: number) => v < 0.6, invert: true },
            { label: 'Densidad Silva', value: interoceptionData.graph_density, good: (v: number) => v > 0.001, warn: (v: number) => v > 0.0001 },
            { label: 'Activ. Feromonas', value: interoceptionData.active_pheromones / 20, good: (v: number) => v < 0.5, warn: (v: number) => v < 0.9 },
          ].map(({ label, value, good, warn, invert }) => {
            const pct = Math.min(100, value * 100);
            const color = good(value) ? '#34d399' : warn(value) ? '#fbbf24' : '#ef4444';
            return (
              <div key={label} className="p-3 rounded-xl bg-slate-900/40">
                <div className="text-[11px] font-medium text-slate-500 mb-1">{label}</div>
                <div className="text-lg font-black font-mono" style={{ color }}>{pct.toFixed(0)}%</div>
                <div className="mt-1.5 h-1 bg-slate-800 rounded-full overflow-hidden">
                  <div className="h-full rounded-full transition-all duration-1000" style={{ width: `${invert ? 100 - pct : pct}%`, backgroundColor: color }} />
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Multi-Agent Activity */}
      <div className="rounded-xl bg-slate-900/50 overflow-hidden">
        <div className="px-4 py-3 border-b border-slate-800/50 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <History className="w-3.5 h-3.5 text-amber-400" />
            <span className="text-[11px] font-medium text-slate-400">Stream de Actividad Soberana</span>
          </div>
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-1 text-[9px] text-slate-600 cursor-pointer select-none hover:text-slate-400 transition-colors">
              <input
                type="checkbox"
                checked={showHeartbeats}
                onChange={e => setShowHeartbeats(e.target.checked)}
                className="accent-emerald-500 w-2.5 h-2.5"
              />
              Mostrar heartbeats
            </label>
            <span className="text-[9px] text-slate-500 font-mono">LIVE EVENTS</span>
            <div className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-ping" />
          </div>
        </div>
        <div className="divide-y divide-slate-800/50">
          {silvaDelta > 0 && (
             <div className="px-4 py-2.5 flex items-center justify-between group hover:bg-slate-800/20 transition-colors">
                <div className="flex items-center gap-3 overflow-hidden">
                  <div className="w-1.5 h-1.5 rounded-full shrink-0 bg-emerald-400 shadow-[0_0_5px_rgba(52,211,153,0.5)]" />
                  <span className="text-[10px] text-slate-300">
                    <span className="font-bold text-emerald-400">{silvaDelta} nuevos nodos</span> integrados en los últimos 30s
                  </span>
                </div>
             </div>
          )}

          <div className="px-4 py-3 flex flex-wrap gap-3 bg-slate-950/40 border-b border-slate-800/50">
            {blackboard?.active_agents.map(agent => (
              <div key={agent} className="flex items-center gap-2 group">
                <div className={cn("w-7 h-7 rounded-full flex items-center justify-center border transition-all group-hover:scale-110", getAgentColor(agent))}>
                  <User className="w-4 h-4" />
                </div>
                <div className="flex flex-col">
                   <span className="text-[10px] font-semibold text-slate-200">{agent.split('-')[0]}</span>
                   <span className="text-[8px] text-slate-500">active</span>
                </div>
              </div>
            ))}
            {(!blackboard?.active_agents || blackboard.active_agents.length === 0) && (
              <div className="flex items-center gap-2 text-slate-600">
                <div className="w-7 h-7 rounded-full border border-slate-800 flex items-center justify-center opacity-30">
                  <User className="w-4 h-4" />
                </div>
                <span className="text-[10px] italic text-slate-600">No agents detected</span>
              </div>
            )}
          </div>

          <div className="max-h-96 overflow-y-auto divide-y divide-slate-800/30">
          {events
            .filter(e => ['tool_call', 'memory_added', 'memory_updated', 'heartbeat', 'log', 'maintenance_started', 'maintenance_finished'].includes(e.type))
            .filter(e => showHeartbeats || (e.type !== 'uptime' && !((e.data as any)?.uptime_secs !== undefined)))
            .slice(0, 15)
            .map((e, i) => (
              <div key={`${e.ts}-${i}`} className="px-4 py-2.5 flex items-center justify-between group hover:bg-slate-800/20 transition-colors">
                <div className="flex items-center gap-3 overflow-hidden">
                  <div className={cn(
                    "w-1.5 h-1.5 rounded-full shrink-0",
                    e.type === 'tool_call' ? "bg-blue-400" : 
                    e.type.startsWith('memory') ? "bg-violet-400" :
                    e.type.startsWith('maintenance') ? "bg-amber-400" :
                    e.type === 'heartbeat' ? "bg-emerald-400/50" : "bg-slate-500"
                  )} />
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="text-[11px] font-bold text-slate-200 shrink-0 capitalize">
                      {e.source === 'mcp' ? 'MCP' : (e.data as any).agent_id || 'System'}
                    </span>
                    <span className="text-slate-500 text-[10px]">·</span>
                    <span className="text-[10px] text-slate-400 truncate">
                      {e.type === 'tool_call' ? `tool: ${(e.data as any).tool}` : 
                       e.type === 'heartbeat' ? `uptime: ${(e.data as any).uptime_secs}s` :
                       e.type.startsWith('maintenance') ? `maint: ${(e.data as any).task}` :
                       e.type}
                    </span>
                  </div>
                </div>
                <div className="text-[9px] text-slate-600 font-mono shrink-0 ml-4">
                  <RelativeTime ts={e.ts} />
                </div>
              </div>
            ))}
            {events.length === 0 && !silvaDelta && !blackboard?.pending.length && (
              <div className="p-12 text-center flex flex-col items-center justify-center gap-2">
                <Zap className="w-6 h-6 text-slate-800 animate-pulse" />
                <span className="text-[11px] text-slate-600">Silent Mode — Awaiting Signal</span>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Running Guilds & Pending Approvals */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <div className="lg:col-span-2 rounded-xl bg-slate-900/50 overflow-hidden">
          <div className="px-4 py-3 border-b border-slate-800/50 flex items-center justify-between">
            <span className="text-[11px] font-medium text-slate-400">Operational Guilds</span>
            <span className="text-[10px] text-slate-500">{activeGuilds} / {guilds?.length || 0} online</span>
          </div>
          <div className="p-3 grid grid-cols-1 md:grid-cols-2 gap-2 max-h-64 overflow-y-auto">
            {guilds?.map((g) => (
              <div key={g.name} className={cn(
                "flex items-center justify-between p-2 rounded border transition-all",
                g.running ? "bg-slate-950/50 border-emerald-500/20" : "bg-slate-900/20 border-transparent opacity-50"
              )}>
                <div className="flex items-center gap-2">
                  <div className={cn("w-1.5 h-1.5 rounded-full", g.running ? "bg-emerald-500 shadow-[0_0_5px_rgba(16,185,129,0.5)]" : "bg-slate-700")}></div>
                  <span className="text-xs font-mono text-slate-300">{g.name}</span>
                  {(g.restarts_5m ?? 0) > 3 && (
                    <span className="text-[7px] bg-red-500 text-slate-50 px-1 rounded-sm font-bold animate-pulse">DEGRADED</span>
                  )}
                </div>
                <div className="flex gap-1 shrink-0">
                   {g.launcher_type && (
                    <span className={cn("text-[7px] px-1 rounded-sm uppercase font-bold flex items-center", 
                      g.launcher_type === 'http' ? "text-violet-400 border border-violet-500/30" :
                      g.launcher_type === 'stdio' ? "text-amber-400 border border-amber-500/30" :
                      "text-blue-400 border border-blue-500/30"
                    )}>
                      {g.launcher_type}
                    </span>
                  )}
                  <span className="text-[10px] text-slate-600 font-mono">{g.tools_count}T</span>
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="rounded-xl bg-slate-900/50 overflow-hidden flex flex-col">
          <div className="px-4 py-3 border-b border-slate-800/50 flex items-center justify-between">
            <span className="text-[11px] font-medium text-slate-400">HITL Gateway</span>
            <ShieldCheck className="w-3 h-3 text-amber-400" />
          </div>
          <div className="flex-1 p-4 flex flex-col items-center justify-center text-center">
            {approvals.length === 0 ? (
              <>
                <CheckCircle className="w-10 h-10 text-emerald-500/20 mb-2" />
                <p className="text-xs text-slate-500">Security Clearance High</p>
                <p className="text-[10px] text-slate-600">No pending approvals required.</p>
              </>
            ) : (
              <div className="w-full space-y-2">
                <p className="text-xs font-bold text-amber-400 mb-3">{approvals.length} ACTIONS BLOCKED</p>
                {approvals.slice(0, 3).map((a) => (
                  <div key={a.id} className="p-2 bg-amber-500/5 border border-amber-500/20 rounded text-left overflow-hidden">
                    <div className="text-[9px] font-mono text-amber-500 truncate">{a.id}</div>
                    <div className="text-[10px] text-slate-400 truncate">{a.tool}</div>
                  </div>
                ))}
                <button className="w-full py-1.5 bg-amber-500/10 hover:bg-amber-500/20 text-amber-500 rounded text-[10px] font-bold border border-amber-500/30 mt-2 cursor-pointer transition-colors">
                  MANAGE APPROVALS
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Guild Utilization, Memory Retention, Night Consolidation & Canary Assertions */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Tarjeta 1 — Guild Utilization */}
        <div className="p-4 rounded-xl bg-slate-900/50">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-2 text-slate-400 text-[11px] font-medium">
              <Cpu className="w-3.5 h-3.5 text-amber-400" /> Guild Utilization
            </div>
            <span className="text-[10px] text-slate-500">
              {guildsUtilization?.active || 0} / {guildsUtilization?.total || 0} active
              <span className="ml-2 text-slate-600">·</span>
              <span className="ml-2 text-slate-500">
                {guilds?.reduce((s, g) => s + (g.total_calls || 0), 0) || 0} calls
              </span>
            </span>
          </div>
          <div className="space-y-4">
            {(guildsUtilization?.active_guilds || []).slice(0, 6).map((guild: any) => {
              const idleSecs = guild.idle_secs ?? 0;
              const utilPct = idleSecs < 60 ? 80 : idleSecs < 300 ? 40 : 10;
              const barColor = utilPct >= 70 ? 'bg-emerald-500' : utilPct >= 40 ? 'bg-amber-500' : 'bg-slate-600';
              
              return (
                <div key={guild.name} className="space-y-1.5">
                  <div className="flex justify-between items-center text-[10px]">
                    <span className="text-slate-300 font-mono truncate mr-2">{guild.name}</span>
                    <span className="text-slate-500 font-mono">{utilPct}%</span>
                  </div>
                  <div className="h-1.5 w-full bg-slate-800/50 rounded-full overflow-hidden">
                    <div 
                      className={cn("h-full transition-all duration-700 ease-out", barColor)} 
                      style={{ width: `${utilPct}%` }} 
                    />
                  </div>
                </div>
              );
            })}
            {(!guildsUtilization?.active_guilds || guildsUtilization.active_guilds.length === 0) && (
              <div className="py-6 text-center">
                <p className="text-[10px] text-slate-600 italic">No active guilds monitored</p>
              </div>
            )}
          </div>
        </div>

        {/* Tarjeta 2 — Memory Retention */}
        <div className="p-4 rounded-xl bg-slate-900/50 flex flex-col items-center justify-center text-center">
          <div className="flex items-center gap-2 text-slate-400 text-[11px] font-medium mb-6">
            <Database className="w-3.5 h-3.5 text-violet-400" /> Memory Retention
          </div>
          
          {memoryRetention ? (
            <div className="space-y-2">
              <div className={cn(
                "text-6xl font-black tracking-tighter transition-colors duration-500",
                memoryRetention.silva.retention_rate_percent >= 80 ? "text-emerald-400" :
                memoryRetention.silva.retention_rate_percent >= 60 ? "text-amber-400" : "text-red-400"
              )}>
                {memoryRetention.silva.retention_rate_percent}%
              </div>
              <div className="space-y-0.5">
                <div className="text-sm font-bold text-slate-200 tracking-tight">
                  {memoryRetention.silva.total_nodes.toLocaleString()}
                </div>
                <div className="text-[10px] text-slate-500 font-medium">
                  Total Sovereign Nodes
                </div>
              </div>
            </div>
          ) : (
            <div className="animate-pulse space-y-4">
              <div className="h-12 w-24 bg-slate-800 rounded mx-auto" />
              <div className="h-4 w-32 bg-slate-800 rounded mx-auto" />
            </div>
          )}
        </div>

        {/* Tarjeta 3 — Night Consolidation */}
        <DreamStatusWidget bridge={bridge} />

        {/* Tarjeta 4 — Canary Assertions */}
        <CanaryStatusWidget bridge={bridge} />
      </div>

      {/* Recent Sessions (Digests) */}
      <div className="rounded-xl bg-slate-900/50 overflow-hidden">
        <div className="px-4 py-3 border-b border-slate-800/50 flex items-center justify-between">
          <span className="text-[11px] font-medium text-slate-400">Sesiones Recientes</span>
          <Clock className="w-3.5 h-3.5 text-blue-400" />
        </div>
        <RecentSessions bridge={bridge} />
      </div>
    </div>
  );
}

function RecentSessions({ bridge }: { bridge: NexusBridge | null }) {
  const [digests, setDigests] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!bridge) return;
    const load = async () => {
      try {
        const data = await bridge.fetchSessionDigests(3);
        setDigests(data);
      } catch (err) {
        console.error('Failed to load session digests:', err);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [bridge]);

  if (loading) {
    return <div className="p-8 text-center text-xs text-slate-600 animate-pulse">Analizando hipocampo...</div>;
  }

  if (digests.length === 0) {
    return <div className="p-8 text-center text-xs text-slate-600 italic">No hay resúmenes de sesión disponibles.</div>;
  }

  return (
    <div className="divide-y divide-slate-800/50">
      {digests.map((d, i) => (
        <div key={i} className="p-4 hover:bg-slate-800/20 transition-colors">
          <div className="flex justify-between items-start mb-2">
            <div className="text-[10px] font-bold text-blue-400 font-mono">@{d.agent_id || 'anon'}</div>
            <div className="text-[9px] text-slate-600 font-mono">{d.created_at}</div>
          </div>
          <p className="text-xs text-slate-300 leading-relaxed line-clamp-2 italic">
            "{d.content}"
          </p>
        </div>
      ))}
    </div>
  );
}
