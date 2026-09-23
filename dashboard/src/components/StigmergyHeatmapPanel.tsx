import React, { useState, useEffect, useCallback, useMemo } from 'react';
import {
  Flame,
  Snowflake,
  Activity,
  Search,
  RefreshCw,
  ChevronRight,
  X,
  Compass,
  FolderTree,
  Clock,
  Radio,
} from 'lucide-react';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import { agentStyle } from '../lib/agent-meta';
import { MetricCard } from './ui/MetricPrimitives';

export type HeatCategory = 'cold' | 'cool' | 'warm' | 'hot' | 'blazing';

export interface WorkprintTrace {
  trace_id: string | number;
  agent_id: string;
  trace_type: 'read' | 'write' | 'verify' | 'block' | 'diffuse';
  weight: number;
  touched_at: number; // unix timestamp in seconds
  note?: string;
}

export interface StigmergicZone {
  zone_id: string; // e.g. "crates/tylluan-kernel/transport"
  subsystem: 'kernel' | 'link' | 'dashboard' | 'guilds' | 'docs' | string;
  description: string;
  heat: number; // 0.0 to 2.0
  half_life_hours: number; // default 4.0
  active_agents: string[];
  total_traces: number;
  last_touched_at: number;
  traces: WorkprintTrace[];
  neighbor_zones?: string[]; // 1-hop diffusion targets
  contention_risk?: 'low' | 'moderate' | 'high';
}

export interface StigmergyHeatmapPanelProps {
  bridge?: {
    fetchRaw: (url: string, init?: RequestInit) => Promise<unknown>;
  } | null;
  notify?: (msg: string, type?: 'info' | 'error') => void;
}


// Realistic repository mock data based on Tylluan's actual workspace structure
export const MOCK_STIGMERGIC_ZONES: StigmergicZone[] = [
  {
    zone_id: 'crates/tylluan-kernel/transport',
    subsystem: 'kernel',
    description: 'Sovereign MCP transport handlers, SSE event loop, HTTP router & rate limiter',
    heat: 1.85,
    half_life_hours: 4,
    active_agents: ['deep', 'claude-code'],
    total_traces: 42,
    last_touched_at: Math.floor(Date.now() / 1000) - 120, // 2 min ago
    contention_risk: 'high',
    neighbor_zones: ['crates/tylluan-kernel/router', 'crates/tylluan-link/p2p'],
    traces: [
      { trace_id: 't-1', agent_id: 'deep', trace_type: 'write', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 120, note: 'Noise XK transport handshake refactor' },
      { trace_id: 't-2', agent_id: 'claude-code', trace_type: 'verify', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 300, note: 'Supervision & latency verification' },
      { trace_id: 't-3', agent_id: 'deep', trace_type: 'write', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 900, note: 'TCP pool session dispatch' },
    ],
  },
  {
    zone_id: 'dashboard/src/components',
    subsystem: 'dashboard',
    description: 'React dashboard UI, consolidated tab suites, metric primitives, and observability panels',
    heat: 1.62,
    half_life_hours: 4,
    active_agents: ['antigravity', 'claude-code'],
    total_traces: 36,
    last_touched_at: Math.floor(Date.now() / 1000) - 60, // 1 min ago
    contention_risk: 'low',
    neighbor_zones: ['dashboard/src/hooks', 'packages/tylluan-ui-core'],
    traces: [
      { trace_id: 't-4', agent_id: 'antigravity', trace_type: 'write', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 60, note: 'AgentCompetencyPanel delivery (ADR-014)' },
      { trace_id: 't-5', agent_id: 'claude-code', trace_type: 'verify', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 180, note: 'Vitest 27/27 verification & origin push' },
      { trace_id: 't-6', agent_id: 'antigravity', trace_type: 'write', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 1800, note: 'WorkContractsPanel & FleetHealthPanel' },
    ],
  },
  {
    zone_id: 'docs/reference/adr',
    subsystem: 'docs',
    description: 'Architecture Decision Records (ADR-001..015), declarative contracts, and specifications',
    heat: 1.35,
    half_life_hours: 4,
    active_agents: ['antigravity', 'buffy', 'claude-code'],
    total_traces: 28,
    last_touched_at: Math.floor(Date.now() / 1000) - 450,
    contention_risk: 'moderate',
    neighbor_zones: ['docs/roadmap', 'docs/internal'],
    traces: [
      { trace_id: 't-7', agent_id: 'buffy', trace_type: 'verify', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 450, note: 'Dossier cross-verification (commit 8064e27)' },
      { trace_id: 't-8', agent_id: 'antigravity', trace_type: 'write', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 1200, note: 'ADR-015 Stigmergic Coordination spec' },
    ],
  },
  {
    zone_id: 'crates/tylluan-kernel/memory',
    subsystem: 'kernel',
    description: 'SilvaDB semantic graph, FSRS-5 spaced consolidation, decay.rs stigmergy math, and agent profiles',
    heat: 0.95,
    half_life_hours: 4,
    active_agents: ['deep', 'buffy'],
    total_traces: 19,
    last_touched_at: Math.floor(Date.now() / 1000) - 1800,
    contention_risk: 'low',
    neighbor_zones: ['crates/tylluan-kernel/router'],
    traces: [
      { trace_id: 't-9', agent_id: 'deep', trace_type: 'read', weight: 0.5, touched_at: Math.floor(Date.now() / 1000) - 1800, note: 'Inspecting decay.rs heat formula' },
      { trace_id: 't-10', agent_id: 'buffy', trace_type: 'verify', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 3600, note: 'Verifying touch_node call sites count' },
    ],
  },
  {
    zone_id: 'crates/tylluan-link/gossip',
    subsystem: 'link',
    description: 'Gossip protocol anti-entropy sync, LRU vector stores, and peer capability registry',
    heat: 0.55,
    half_life_hours: 4,
    active_agents: ['deep'],
    total_traces: 11,
    last_touched_at: Math.floor(Date.now() / 1000) - 7200,
    contention_risk: 'low',
    neighbor_zones: ['crates/tylluan-link/p2p'],
    traces: [
      { trace_id: 't-11', agent_id: 'deep', trace_type: 'read', weight: 0.5, touched_at: Math.floor(Date.now() / 1000) - 7200, note: 'Inspecting Anti-entropy cursors' },
    ],
  },
  {
    zone_id: 'guilds/core',
    subsystem: 'guilds',
    description: 'Python ecosystem tools, vision moondream, check_coloquio, and worker coordinators',
    heat: 0.38,
    half_life_hours: 4,
    active_agents: ['deep', 'buffy'],
    total_traces: 8,
    last_touched_at: Math.floor(Date.now() / 1000) - 14400,
    contention_risk: 'low',
    neighbor_zones: ['guilds/vision'],
    traces: [
      { trace_id: 't-12', agent_id: 'deep', trace_type: 'verify', weight: 1.0, touched_at: Math.floor(Date.now() / 1000) - 14400, note: 'opencode-cron runtime check' },
    ],
  },
  {
    zone_id: 'crates/tylluan-link/p2p',
    subsystem: 'link',
    description: 'Noise XK session pools, direct TCP socket dispatch, and NAT traversal handlers',
    heat: 0.25,
    half_life_hours: 4,
    active_agents: ['deep'],
    total_traces: 5,
    last_touched_at: Math.floor(Date.now() / 1000) - 28800,
    contention_risk: 'low',
    neighbor_zones: ['crates/tylluan-kernel/transport'],
    traces: [],
  },
  {
    zone_id: 'docs-site/src',
    subsystem: 'docs',
    description: 'Next.js 3010 interactive architecture visualizer, 3D maps, and interactive graphs',
    heat: 0.12,
    half_life_hours: 4,
    active_agents: [],
    total_traces: 2,
    last_touched_at: Math.floor(Date.now() / 1000) - 43200,
    contention_risk: 'low',
    neighbor_zones: ['docs/reference/adr'],
    traces: [],
  },
  {
    zone_id: 'crates/tylluan-kernel/config',
    subsystem: 'kernel',
    description: 'tylluan.toml declarative configuration parser, identity keys, and environment guards',
    heat: 0.05,
    half_life_hours: 4,
    active_agents: [],
    total_traces: 1,
    last_touched_at: Math.floor(Date.now() / 1000) - 86400,
    contention_risk: 'low',
    traces: [],
  },
];

export function getHeatCategory(heat: number): HeatCategory {
  if (heat >= 1.5) return 'blazing';
  if (heat >= 1.0) return 'hot';
  if (heat >= 0.5) return 'warm';
  if (heat >= 0.2) return 'cool';
  return 'cold';
}

export function getHeatBadgeStyle(category: HeatCategory) {
  switch (category) {
    case 'blazing':
      return {
        label: 'Blazing / High Contention',
        bg: 'bg-rose-950/60 border-rose-500/60 text-rose-300 shadow-rose-500/20 shadow-sm',
        bar: 'bg-rose-500',
        dot: 'bg-rose-400 animate-pulse',
        icon: <Flame className="w-3.5 h-3.5 text-rose-400 animate-bounce" />,
      };
    case 'hot':
      return {
        label: 'Hot / Active Flow',
        bg: 'bg-amber-950/60 border-amber-500/50 text-amber-300 shadow-amber-500/10 shadow-sm',
        bar: 'bg-amber-500',
        dot: 'bg-amber-400',
        icon: <Flame className="w-3.5 h-3.5 text-amber-400" />,
      };
    case 'warm':
      return {
        label: 'Warm / Steady',
        bg: 'bg-emerald-950/50 border-emerald-500/40 text-emerald-300',
        bar: 'bg-emerald-500',
        dot: 'bg-emerald-400',
        icon: <Activity className="w-3.5 h-3.5 text-emerald-400" />,
      };
    case 'cool':
      return {
        label: 'Cool / Periodic',
        bg: 'bg-cyan-950/40 border-cyan-800/40 text-cyan-300',
        bar: 'bg-cyan-500',
        dot: 'bg-cyan-400',
        icon: <Compass className="w-3.5 h-3.5 text-cyan-400" />,
      };
    case 'cold':
      return {
        label: 'Cold / Quiescent',
        bg: 'bg-slate-900/60 border-slate-800 text-slate-400',
        bar: 'bg-slate-600',
        dot: 'bg-slate-500',
        icon: <Snowflake className="w-3.5 h-3.5 text-slate-500" />,
      };
  }
}

export function formatTimeAgo(unixSecs: number): string {
  if (!unixSecs) return 'never';
  const now = Math.floor(Date.now() / 1000);
  const diff = now - unixSecs;
  if (diff < 0) return 'just now';
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

export function StigmergyHeatmapPanel({ bridge, notify }: StigmergyHeatmapPanelProps = {}) {
  const [zones, setZones] = useState<StigmergicZone[]>(MOCK_STIGMERGIC_ZONES);
  const [selectedSubsystem, setSelectedSubsystem] = useState<string>('all');
  const [selectedCategory, setSelectedCategory] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedZone, setSelectedZone] = useState<StigmergicZone | null>(null);
  const [loading, setLoading] = useState(false);
  const [isLive, setIsLive] = useState(false);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);

  const fetchZones = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      const res = (await bridge.fetchRaw('/api/v1/stigmergy/zones')) as {
        zones?: StigmergicZone[];
        count?: number;
      };
      if (res && Array.isArray(res.zones) && res.zones.length > 0) {
        setZones(res.zones);
        setIsLive(true);
      }
      setLastUpdated(new Date());
    } catch (err) {
      console.warn('Failed to fetch live stigmergy zones, using fallback:', err);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  usePolling('stigmergy-heatmap-zones', fetchZones, { interval: 'standard' });

  useEffect(() => {
    if (bridge) {
      fetchZones();
    }
  }, [bridge, fetchZones]);

  // Statistics calculation
  const stats = useMemo(() => {
    let totalHeat = 0;
    let blazingHotCount = 0;
    let warmCount = 0;
    let coldCount = 0;

    zones.forEach((z) => {
      totalHeat += z.heat;
      const cat = getHeatCategory(z.heat);
      if (cat === 'blazing' || cat === 'hot') blazingHotCount++;
      else if (cat === 'warm' || cat === 'cool') warmCount++;
      else coldCount++;
    });

    return {
      totalZones: zones.length,
      avgHeat: zones.length > 0 ? (totalHeat / zones.length).toFixed(2) : '0.00',
      blazingHotCount,
      warmCount,
      coldCount,
    };
  }, [zones]);

  // Filtered zones
  const filteredZones = useMemo(() => {
    return zones
      .filter((zone) => {
        const matchesSearch =
          !searchQuery.trim() ||
          zone.zone_id.toLowerCase().includes(searchQuery.toLowerCase()) ||
          zone.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
          zone.active_agents.some((a) => a.toLowerCase().includes(searchQuery.toLowerCase()));

        const matchesSubsystem =
          selectedSubsystem === 'all' || zone.subsystem === selectedSubsystem;

        const cat = getHeatCategory(zone.heat);
        const matchesCategory =
          selectedCategory === 'all' || cat === selectedCategory;

        return matchesSearch && matchesSubsystem && matchesCategory;
      })
      .sort((a, b) => b.heat - a.heat);
  }, [zones, searchQuery, selectedSubsystem, selectedCategory]);

  return (
    <div className="flex-1 min-h-0 flex flex-col space-y-4 font-sans text-slate-100 overflow-y-auto pr-1">
      {/* Header Banner */}
      <div className="p-4 rounded-xl border border-slate-800 bg-slate-900/40 backdrop-blur-md flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="p-2.5 rounded-lg bg-amber-500/10 border border-amber-500/30 text-amber-400">
            <Flame className="w-6 h-6" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-base font-bold text-slate-100 tracking-tight font-mono">
                Stigmergic Heatmap & Workprints
              </h2>
              {isLive ? (
                <span className="flex items-center gap-1 px-2 py-0.5 text-[10px] font-mono uppercase rounded-full bg-emerald-950/60 border border-emerald-800/50 text-emerald-300">
                  <Radio className="w-2.5 h-2.5 text-emerald-400 animate-pulse" />
                  Live Kernel Stream
                </span>
              ) : (
                <span className="px-2 py-0.5 text-[10px] font-mono uppercase rounded-full bg-amber-950/60 border border-amber-800/50 text-amber-300">
                  ADR-015 Phase 4
                </span>
              )}
            </div>
            <p className="text-xs text-slate-400 mt-0.5">
              Emergent coordination by semantic warmth, active code footprints, and collision detection (T½ = 4h)
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {bridge && (
            <button
              onClick={fetchZones}
              disabled={loading}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-800 bg-slate-900/60 hover:bg-slate-800/80 text-slate-300 text-xs font-mono transition-all disabled:opacity-50"
            >
              <RefreshCw className={cn('w-3.5 h-3.5', loading && 'animate-spin text-amber-400')} />
              <span>Refresh</span>
            </button>
          )}
          <button
            onClick={() => {
              setZones([...MOCK_STIGMERGIC_ZONES]);
              if (notify) notify('Reset heatmap to reference workspace topology', 'info');
            }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-800 bg-slate-900/60 hover:bg-slate-800/80 text-slate-300 text-xs font-mono transition-all"
          >
            <Activity className="w-3.5 h-3.5 text-amber-400" />
            <span>Reset / Mock</span>
          </button>
        </div>
      </div>


      {/* KPI Metric Cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard
          icon={FolderTree}
          label="Tracked Work Zones"
          value={stats.totalZones}
          sub="Workspace granularity"
        />
        <MetricCard
          icon={Flame}
          label="Hot / Active Focus"
          value={stats.blazingHotCount}
          unit={`/ ${stats.totalZones}`}
          sub="Heat >= 1.0 (High Traffic)"
          valueClass="text-amber-400"
        />
        <MetricCard
          icon={Activity}
          label="Steady / Warm Zones"
          value={stats.warmCount}
          sub="0.2 <= Heat < 1.0"
          valueClass="text-emerald-400"
        />
        <MetricCard
          icon={Snowflake}
          label="Quiescent / Cold"
          value={stats.coldCount}
          sub="Potential cold debt zones"
          valueClass="text-slate-400"
        />
      </div>

      {/* Filter & Search Toolbar */}
      <div className="p-3 rounded-xl border border-slate-800 bg-slate-900/30 flex flex-col md:flex-row items-stretch md:items-center justify-between gap-3">
        <div className="flex-1 relative">
          <Search className="w-4 h-4 text-slate-500 absolute left-3 top-1/2 -translate-y-1/2" />
          <input
            type="text"
            placeholder="Search by zone path, subsystem, description or active agent..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full pl-9 pr-3 py-1.5 bg-slate-950/60 border border-slate-800 rounded-lg text-xs font-mono text-slate-200 placeholder-slate-500 focus:outline-none focus:border-amber-500/50"
          />
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* Subsystem Filter */}
          <select
            value={selectedSubsystem}
            onChange={(e) => setSelectedSubsystem(e.target.value)}
            className="px-2.5 py-1.5 bg-slate-950/60 border border-slate-800 rounded-lg text-xs font-mono text-slate-300 focus:outline-none focus:border-amber-500/50"
          >
            <option value="all">All Subsystems</option>
            <option value="kernel">Kernel (crates/kernel)</option>
            <option value="link">Link & Mesh (crates/link)</option>
            <option value="dashboard">Dashboard (dashboard/)</option>
            <option value="guilds">Guilds Ecosystem (guilds/)</option>
            <option value="docs">Documentation & ADRs</option>
          </select>

          {/* Heat Category Filter */}
          <select
            value={selectedCategory}
            onChange={(e) => setSelectedCategory(e.target.value)}
            className="px-2.5 py-1.5 bg-slate-950/60 border border-slate-800 rounded-lg text-xs font-mono text-slate-300 focus:outline-none focus:border-amber-500/50"
          >
            <option value="all">All Heat Levels</option>
            <option value="blazing">🔥 Blazing (&gt;=1.5)</option>
            <option value="hot">⚡ Hot (1.0 - 1.5)</option>
            <option value="warm">🌱 Warm (0.5 - 1.0)</option>
            <option value="cool">💧 Cool (0.2 - 0.5)</option>
            <option value="cold">❄️ Cold (&lt;0.2)</option>
          </select>
        </div>
      </div>

      {/* Main Heatmap Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {filteredZones.map((zone) => {
          const category = getHeatCategory(zone.heat);
          const badge = getHeatBadgeStyle(category);
          const heatPercent = Math.min(100, Math.round((zone.heat / 2.0) * 100));

          return (
            <div
              key={zone.zone_id}
              className={cn(
                'p-4 rounded-xl border backdrop-blur-md transition-all flex flex-col justify-between space-y-3 cursor-pointer group hover:scale-[1.01]',
                badge.bg
              )}
              onClick={() => setSelectedZone(zone)}
            >
              <div>
                {/* Card Header: Zone Name + Category Badge */}
                <div className="flex items-start justify-between gap-2 mb-2">
                  <div className="min-w-0">
                    <span className="text-[10px] font-mono uppercase tracking-wider text-slate-400 block">
                      {zone.subsystem}
                    </span>
                    <h3 className="font-mono font-bold text-xs text-slate-100 truncate" title={zone.zone_id}>
                      {zone.zone_id}
                    </h3>
                  </div>
                  <div className="flex items-center gap-1 text-[10px] font-mono font-bold flex-shrink-0">
                    {badge.icon}
                    <span>{zone.heat.toFixed(2)}</span>
                  </div>
                </div>

                <p className="text-[11px] text-slate-300 line-clamp-2 font-mono leading-relaxed">
                  {zone.description}
                </p>
              </div>

              {/* Heat Meter Bar */}
              <div className="space-y-1 pt-1 border-t border-slate-800/60">
                <div className="flex items-center justify-between text-[10px] font-mono text-slate-400">
                  <span>Warmth Intensity</span>
                  <span className="font-bold">{heatPercent}%</span>
                </div>
                <div className="w-full bg-slate-900/80 rounded-full h-1.5 overflow-hidden">
                  <div
                    className={cn('h-1.5 rounded-full transition-all duration-500', badge.bar)}
                    style={{ width: `${heatPercent}%` }}
                  />
                </div>
              </div>

              {/* Card Footer: Active Contributors + Last Touch */}
              <div className="flex items-center justify-between text-[10px] font-mono pt-1 text-slate-400">
                {/* Contributor Avatars */}
                <div className="flex items-center -space-x-1.5">
                  {zone.active_agents.length === 0 ? (
                    <span className="text-slate-500 italic">No recent touches</span>
                  ) : (
                    zone.active_agents.map((aid) => {
                      const style = agentStyle(aid);
                      return (
                        <div
                          key={aid}
                          title={`@${aid}`}
                          className={cn(
                            'w-5 h-5 rounded-full flex items-center justify-center font-bold text-[9px] border ring-1 ring-slate-900',
                            style.bg,
                            style.color,
                            style.border
                          )}
                        >
                          {style.initial}
                        </div>
                      );
                    })
                  )}
                </div>

                <div className="flex items-center gap-1.5 text-slate-400">
                  <Clock className="w-3 h-3 text-slate-500" />
                  <span>{formatTimeAgo(zone.last_touched_at)}</span>
                  <ChevronRight className="w-3 h-3 text-slate-500 group-hover:text-slate-200 transition-colors" />
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {/* Zone Detail Inspector Drawer / Modal */}
      {selectedZone && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-4 animate-in fade-in duration-150">
          <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-2xl max-h-[85vh] flex flex-col shadow-2xl overflow-hidden font-sans">
            {/* Modal Header */}
            <div className="p-4 border-b border-slate-800 flex items-center justify-between bg-slate-950/60">
              <div className="flex items-center gap-3">
                <div className="p-2 rounded-xl bg-amber-500/10 border border-amber-500/30 text-amber-400">
                  <Flame className="w-5 h-5" />
                </div>
                <div>
                  <h3 className="font-mono font-bold text-slate-100 text-sm">
                    {selectedZone.zone_id}
                  </h3>
                  <p className="text-xs text-slate-400 font-mono">
                    Subsystem: <span className="uppercase text-amber-300">{selectedZone.subsystem}</span> · Heat: <strong>{selectedZone.heat.toFixed(2)}</strong> / 2.0
                  </p>
                </div>
              </div>

              <button
                onClick={() => setSelectedZone(null)}
                className="p-1.5 text-slate-400 hover:text-slate-200 rounded-lg hover:bg-slate-800 transition-colors"
                aria-label="Close modal"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            {/* Modal Body */}
            <div className="p-4 overflow-y-auto space-y-4 text-xs font-mono">
              {/* Key Zone Metrics */}
              <div className="grid grid-cols-3 gap-2">
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800">
                  <span className="text-[10px] text-slate-500 uppercase block">Heat Index</span>
                  <span className="text-base font-bold text-amber-400">{selectedZone.heat.toFixed(2)}</span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800">
                  <span className="text-[10px] text-slate-500 uppercase block">Half-Life</span>
                  <span className="text-base font-bold text-slate-200">{selectedZone.half_life_hours}h</span>
                </div>
                <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800">
                  <span className="text-[10px] text-slate-500 uppercase block">Contention Risk</span>
                  <span
                    className={cn(
                      'text-base font-bold capitalize',
                      selectedZone.contention_risk === 'high'
                        ? 'text-rose-400'
                        : selectedZone.contention_risk === 'moderate'
                        ? 'text-amber-400'
                        : 'text-emerald-400'
                    )}
                  >
                    {selectedZone.contention_risk || 'low'}
                  </span>
                </div>
              </div>

              {/* Description & Neighbors */}
              <div className="p-3 rounded-lg bg-slate-950/60 border border-slate-800 space-y-2">
                <div>
                  <span className="text-[10px] text-slate-500 uppercase block">Zone Scope</span>
                  <p className="text-slate-200 text-[11px] mt-0.5">{selectedZone.description}</p>
                </div>

                {selectedZone.neighbor_zones && selectedZone.neighbor_zones.length > 0 && (
                  <div className="pt-2 border-t border-slate-800/60">
                    <span className="text-[10px] text-slate-500 uppercase block">
                      1-Hop Diffusion Neighbors (β = 0.25)
                    </span>
                    <div className="flex flex-wrap gap-1.5 mt-1">
                      {selectedZone.neighbor_zones.map((nz) => (
                        <span key={nz} className="px-2 py-0.5 rounded bg-slate-900 border border-slate-800 text-slate-400 text-[10px]">
                          {nz}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
              </div>

              {/* Recent Workprint Traces */}
              <div>
                <h4 className="text-xs font-bold text-slate-300 mb-2 uppercase tracking-wider flex items-center gap-1.5">
                  <Activity className="w-4 h-4 text-emerald-400" />
                  Recent Workprint Trail (ADR-015)
                </h4>
                {selectedZone.traces.length === 0 ? (
                  <p className="text-slate-500 italic py-2">No individual work traces recorded in the current decay window.</p>
                ) : (
                  <div className="space-y-2">
                    {selectedZone.traces.map((t) => {
                      const style = agentStyle(t.agent_id);
                      return (
                        <div
                          key={t.trace_id}
                          className="p-2.5 rounded-lg bg-slate-950/60 border border-slate-800 flex items-start justify-between gap-2"
                        >
                          <div className="flex items-start gap-2">
                            <div
                              className={cn(
                                'w-5 h-5 rounded flex items-center justify-center font-bold text-[10px] border mt-0.5',
                                style.bg,
                                style.color,
                                style.border
                              )}
                            >
                              {style.initial}
                            </div>
                            <div>
                              <div className="flex items-center gap-2">
                                <span className="font-bold text-slate-200">@{t.agent_id}</span>
                                <span className="px-1.5 py-0.2 rounded text-[9px] uppercase bg-slate-800 border border-slate-700 text-slate-300">
                                  {t.trace_type}
                                </span>
                                <span className="text-slate-500 text-[10px]">w={t.weight.toFixed(1)}</span>
                              </div>
                              {t.note && (
                                <p className="text-slate-400 text-[11px] mt-0.5">{t.note}</p>
                              )}
                            </div>
                          </div>
                          <span className="text-slate-500 text-[10px] whitespace-nowrap">
                            {formatTimeAgo(t.touched_at)}
                          </span>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>

            {/* Modal Footer */}
            <div className="p-3 border-t border-slate-800 bg-slate-950/60 flex justify-end">
              <button
                onClick={() => setSelectedZone(null)}
                className="px-4 py-1.5 rounded-lg border border-slate-700 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono transition-colors"
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default StigmergyHeatmapPanel;
