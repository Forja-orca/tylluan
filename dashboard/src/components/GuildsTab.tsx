/**
 * GuildsTab V4 — Real-time guild monitoring panel.
 *
 * Features:
 *  - Real guild state (Running / Degraded / Crashed / Stopped) from SSE events
 *  - Latency badge per guild card
 *  - Guild category badge (Builder / Scholar / Watcher / Core) from catalog
 *  - Live "⚡ Running…" badge from guild_progress SSE events
 *  - Crash loop detection from restarts_5m counter
 *  - No Math.random() — all placeholder values are deterministic
 */
import React, { useState, useEffect, useCallback } from 'react';
import {
  Cpu,
  AlertTriangle,
  Square,
  RefreshCw,
  Play,
  Zap,
  Activity,
  Layers,
  Settings,
  ShieldCheck,
} from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import type { Guild, NexusBridge, NexusEvent } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { GuildInspector } from './GuildInspector';

// ─── Guild Category Mapping ────────────────────────────────────────────────────
// Mirrors the GuildCategory enum in catalog.rs

type GuildCategory = 'Core' | 'Builder' | 'Scholar' | 'Watcher';

const CATEGORY_MAP: Record<string, GuildCategory> = {
  // Core (always-on system tools)
  bash: 'Core', filesystem: 'Core', memory: 'Core', monitor: 'Core',
  // Builders (create, build, deploy)
  git: 'Builder', code: 'Builder', docker: 'Builder', rust_specialist: 'Builder',
  // Scholars (research, analyze, learn)
  search: 'Scholar', browser: 'Scholar', knowledge: 'Scholar', pdf: 'Scholar',
  vision: 'Scholar', code_analysis: 'Scholar', sequential_thinking: 'Scholar',
  deep_analysis: 'Scholar', ingest: 'Scholar',
  // Watchers (audit, observe, protect)
  audit: 'Watcher', system_metrics: 'Watcher', security: 'Watcher',
};

const CATEGORY_STYLE: Record<GuildCategory, { label: string; cls: string }> = {
  Core:    { label: 'Core',    cls: 'bg-slate-700 text-slate-300 border-slate-600' },
  Builder: { label: 'Builder', cls: 'bg-blue-500/15 text-blue-400 border-blue-500/25' },
  Scholar: { label: 'Scholar', cls: 'bg-violet-500/15 text-violet-400 border-violet-500/25' },
  Watcher: { label: 'Watcher', cls: 'bg-amber-500/15 text-amber-400 border-amber-500/25' },
};

const DEPRECATED_GUILDS = new Set([
  'formatter', 'web_search', 'data_tools', 'database',
  'code_analysis', 'pdf', 'browser', 'n8n'
]);

function getGuildCategory(name: string): GuildCategory {
  return CATEGORY_MAP[name.toLowerCase().replace(/-/g, '_')] ?? 'Core';
}

// ─── Guild Status Helpers ──────────────────────────────────────────────────────

type GuildStatus = 'running' | 'degraded' | 'crashed' | 'down' | 'lazy';

function resolveStatus(guild: Guild): GuildStatus {
  if ((guild.restarts_5m ?? 0) >= 3) return 'crashed';
  if (guild.running && (guild.last_latency_ms ?? 0) > 5000) return 'degraded';
  if (guild.running) return 'running';
  if (guild.always_on) return 'down';
  return 'lazy';
}

const STATUS_STYLE: Record<GuildStatus, { label: string; dot: string; badge: string; tooltip?: string }> = {
  running:  { label: 'RUNNING',  dot: 'bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.5)]',  badge: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/20' },
  degraded: { label: 'DEGRADED', dot: 'bg-amber-500 animate-pulse',                             badge: 'bg-amber-500/20 text-amber-400 border-amber-500/20' },
  crashed:  { label: 'CRASH_LOOP', dot: 'bg-red-500 animate-pulse',                             badge: 'bg-red-500/20 text-red-400 border-red-500/20' },
  down:     { label: 'DOWN',     dot: 'bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.5)] animate-pulse', badge: 'bg-red-500/20 text-red-400 border-red-500/20' },
  lazy:     { label: 'LAZY',     dot: 'bg-sky-500/60',                                          badge: 'bg-sky-500/10 text-sky-400 border-sky-500/20', tooltip: 'Starts on first use' },
};

// ─── Props ─────────────────────────────────────────────────────────────────────

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  events?: NexusEvent[];
}

// ─── Component ─────────────────────────────────────────────────────────────────

export function GuildsTab({ bridge, notify, events }: Props) {
  // Guilds actively executing right now (from SSE guild_progress events, last 5s)
  const activeGuilds = events
    ?.filter(e => e.type === 'guild_progress' && Date.now() - e.ts < 5000)
    .map(e => (e as any).guild)
    .filter((g): g is string => typeof g === 'string') || [];

  const { guilds: globalGuilds, refreshData } = useNexus();
  const [guilds, setGuilds] = useState<Guild[]>(globalGuilds);
  const [loading, setLoading] = useState<string | null>(null);
  const [dockerStatus, setDockerStatus] = useState<{ status: string; error?: string; version?: string } | null>(null);
  const [lastRestart, setLastRestart] = useState<Record<string, number>>({});
  const [subTab, setSubTab] = useState<'overview' | 'inspector'>('overview');

  // Category filter state
  const [activeCategory, setActiveCategory] = useState<GuildCategory | 'all'>('all');
  const [hideInactive, setHideInactive] = useState(true);
  const [sandboxEnabled, setSandboxEnabled] = useState(false);
  const [sandboxProfile, setSandboxProfile] = useState<'strict' | 'balanced' | 'permissive'>('balanced');
  const [isBackendMock, setIsBackendMock] = useState(false);
  const [fullConfig, setFullConfig] = useState<any>(null);
  const [guildOverrides, setGuildOverrides] = useState<Record<string, 'inherited' | 'strict' | 'balanced' | 'permissive'>>({});

  const getEffectiveProfile = (guildName: string) => {
    const override = guildOverrides[guildName] || 'inherited';
    if (override !== 'inherited') {
      return { profile: override, source: 'guild override' };
    }
    return { profile: sandboxProfile, source: 'global' };
  };

  useEffect(() => {
    setGuilds(globalGuilds);
  }, [globalGuilds]);

  useEffect(() => {
    if (!bridge) return;
    bridge.getConfig()
      .then(cfg => {
        setFullConfig(cfg);
        
        // Detect if backend natively supports sandbox_profile in any configuration struct
        if (cfg?.guilds && 'sandbox_profile' in cfg.guilds) {
          setSandboxProfile(cfg.guilds.sandbox_profile);
          setIsBackendMock(false);
        } else if (cfg?.security?.sandbox && 'profile' in cfg.security.sandbox) {
          setSandboxProfile(cfg.security.sandbox.profile);
          setIsBackendMock(false);
        } else {
          setIsBackendMock(true);
        }

        if (cfg?.security?.sandbox) {
          setSandboxEnabled(!!cfg.security.sandbox.enabled);
        }
      })
      .catch(err => {
        console.error("Failed to load config for sandbox status:", err);
        setIsBackendMock(true);
      });
  }, [bridge]);

  const handleProfileChange = async (newProfile: 'strict' | 'balanced' | 'permissive') => {
    const previous = sandboxProfile;
    setSandboxProfile(newProfile);
    if (!bridge) {
      setSandboxProfile(previous);
      notify(`Cannot set Sandbox Profile — no bridge connection`, 'error');
      return;
    }
    try {
      const result = await bridge.setSandboxProfile(newProfile);
      setIsBackendMock(false);
      notify(
        result.restart_required
          ? `Sandbox Profile set to ${newProfile} — restart required to apply`
          : `Sandbox Profile set to ${newProfile}`,
        'info'
      );
    } catch (err: any) {
      setSandboxProfile(previous);
      notify(`Failed to update Sandbox Profile: ${err.message}`, 'error');
    }
  };

  // React to live SSE events for instant status updates (no polling lag)
  useEffect(() => {
    if (!events?.length) return;
    const last = events[0];
    if (last.type === 'guild_spawned' || last.type === 'guild_killed') {
      // Brief delay for kernel to settle, then refresh
      setTimeout(() => refreshData(), 300);
    }
  }, [events, refreshData]);

  const fetchDockerStatus = useCallback(() => {
    if (!bridge) return;
    bridge.fetchRaw('/api/v1/docker/containers', {})
      .then(res => setDockerStatus(res))
      .catch(() => setDockerStatus({ status: 'error', error: 'Service unreachable' }));
  }, [bridge]);

  useEffect(() => {
    fetchDockerStatus();
    const interval = setInterval(fetchDockerStatus, 30_000);
    return () => clearInterval(interval);
  }, [fetchDockerStatus]);

  const handleGuildAction = async (
    name: string,
    action: 'start' | 'stop' | 'reset_start' | 'restart'
  ) => {
    if (!bridge) return;

    if (action === 'restart') {
      const now = Date.now();
      if (lastRestart[name] && now - lastRestart[name] < 3000) {
        notify(`Please wait before restarting ${name} again`, 'error');
        return;
      }
      setLastRestart(prev => ({ ...prev, [name]: now }));
    }

    setLoading(name);
    try {
      if (action === 'reset_start') {
        await bridge.fetchRaw(`/api/v1/guilds/${name}/reset-backoff`, { method: 'POST' });
        await bridge.startGuild(name);
        notify(`Reset & started guild: ${name}`, 'info');
      } else if (action === 'start' || action === 'restart') {
        await bridge.startGuild(name);
        notify(`${action === 'restart' ? 'Restarted' : 'Started'} guild: ${name}`, 'info');
      } else {
        await bridge.stopGuild(name);
        notify(`Stopped guild: ${name}`, 'info');
      }
      const data = await bridge.getGuildHealth();
      if (Array.isArray(data)) setGuilds(data);
    } catch (e) {
      notify(`Failed: ${name} — ${e instanceof Error ? e.message : 'Unknown error'}`, 'error');
    }
    setLoading(null);
  };

  // Filter guilds by category and deprecated status
  const filteredGuilds = guilds
    .filter(g => {
      // Apply deprecated/inactive filter first
      if (hideInactive && DEPRECATED_GUILDS.has(g.name) && !g.running && g.restarts_5m === 0 && g.total_calls === 0) {
        return false;
      }
      // Apply category filter
      if (activeCategory === 'all') return true;
      return getGuildCategory(g.name) === activeCategory;
    });

  // Aggregate counts by category for filter badges
  const categoryCounts = guilds.reduce<Record<string, number>>((acc, g) => {
    const cat = getGuildCategory(g.name);
    acc[cat] = (acc[cat] || 0) + 1;
    return acc;
  }, {});

  const CATEGORIES: (GuildCategory | 'all')[] = ['all', 'Builder', 'Scholar', 'Watcher', 'Core'];

  const renderGuildCard = (guild: Guild) => {
    const status = resolveStatus(guild);
    const statusStyle = STATUS_STYLE[status];
    const category = getGuildCategory(guild.name);
    const catStyle = CATEGORY_STYLE[category];
    const isActive = activeGuilds.includes(guild.name);

    return (
      <div key={guild.name} className={cn(
        'p-4 rounded-lg border bg-slate-900/50 transition-all duration-200',
        status === 'running' ? 'border-emerald-500/30' :
        status === 'degraded' ? 'border-emerald-500/30' :
        status === 'crashed' ? 'border-red-500/30 bg-red-950/10' :
        status === 'down' ? 'border-red-500/30 bg-red-950/5' :
        'border-slate-800'
      )}>
        {/* Card Header */}
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2 min-w-0">
            <div className={cn('w-2 h-2 flex-shrink-0 rounded-full', statusStyle.dot)} />
            <span className="text-sm font-mono font-semibold truncate">{guild.name}</span>
            {/* Live execution badge */}
            {isActive && (
              <span className="text-[8px] bg-blue-500/20 text-blue-400 border border-blue-500/30 px-1 rounded font-bold animate-pulse flex-shrink-0">
                ⚡ Live
              </span>
            )}
          </div>
          <div className="flex items-center gap-1.5 flex-shrink-0">
            {/* Category badge (V2 Gremio) */}
            <span className={cn(
              'text-[8px] px-1.5 py-0.5 rounded font-bold border uppercase tracking-tighter',
              catStyle.cls
            )}>
              {catStyle.label}
            </span>
            {/* Status badge */}
            <span 
              title={statusStyle.tooltip}
              className={cn(
                'text-[9px] px-2 py-0.5 rounded-full font-black border tracking-tighter cursor-help',
                statusStyle.badge
              )}
            >
              {statusStyle.label}
            </span>
          </div>
        </div>

        {/* Metrics Row — CPU/RAM from kernel (0/unset → N/A badge) */}
        <div className="grid grid-cols-2 gap-2 mb-3 bg-slate-950/40 p-2 rounded-lg border border-slate-800/50">
          {(() => {
            const cpu = (guild as any).cpu_percent;
            const mem = (guild as any).memory_mb;
            const hasCpu = cpu !== undefined && cpu !== null && cpu > 0;
            const hasMem = mem !== undefined && mem !== null && mem > 0;

            if (!hasCpu && !hasMem) {
              return (
                <div className="col-span-2 flex items-center justify-center gap-2 py-2">
                  <Cpu className="w-3 h-3 text-slate-600" />
                  <span className="text-[9px] text-slate-600 font-mono bg-slate-800/50 px-2 py-0.5 rounded-full">N/A (no medido)</span>
                </div>
              );
            }

            return (
              <>
                {hasCpu && (
                  <div className="space-y-1">
                    <div className="flex justify-between items-center text-[8px] font-bold text-slate-500 uppercase tracking-tighter">
                      <span>CPU</span>
                      <span className="text-slate-300">{cpu}%</span>
                    </div>
                    <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
                      <div className="h-full bg-blue-500 transition-all duration-1000"
                        style={{ width: `${Math.min(cpu, 100)}%` }} />
                    </div>
                  </div>
                )}
                {hasMem && (
                  <div className="space-y-1">
                    <div className="flex justify-between items-center text-[8px] font-bold text-slate-500 uppercase tracking-tighter">
                      <span>RAM</span>
                      <span className="text-slate-300">{mem}MB</span>
                    </div>
                    <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
                      <div className="h-full bg-violet-500 transition-all duration-1000"
                        style={{ width: `${Math.min(mem / 1024 * 100, 100)}%` }} />
                    </div>
                  </div>
                )}
              </>
            );
          })()}
        </div>

        {/* Details */}
        <div className="space-y-1 text-[10px] text-slate-500 mb-4 font-mono">
          <div className="flex justify-between">
            <span>Always-on</span>
            <span className={guild.always_on ? 'text-amber-400' : 'text-slate-600'}>
              {guild.always_on ? 'Yes' : 'No'}
            </span>
          </div>
          <div className="flex justify-between">
            <span>Type</span>
            <span className="text-slate-300 uppercase">{guild.launcher_type || 'Python'}</span>
          </div>
          <div className="flex justify-between">
            <span>Calls / Latency</span>
            <span className={cn(
              guild.last_latency_ms && guild.last_latency_ms > 5000
                ? 'text-amber-400'
                : 'text-slate-400'
            )}>
              {guild.total_calls || 0} / {guild.last_latency_ms ? `${guild.last_latency_ms}ms` : '—'}
            </span>
          </div>
          <div className="flex justify-between">
            <span className="uppercase font-bold tracking-tighter text-[9px]">Tools</span>
            <span className={!guild.running && !guild.tools_count ? 'text-slate-700 italic' : 'text-slate-300'}>
              {guild.tools_count || (guild.running ? '0' : '— offline')}
            </span>
          </div>
          {/* Restarts warning */}
          {(guild.restarts_5m ?? 0) > 0 && (
            <div className="flex justify-between text-red-400">
              <span className="flex items-center gap-1">
                <Activity className="w-2.5 h-2.5" />
                Restarts (5m)
              </span>
              <span className="font-bold">{guild.restarts_5m}</span>
            </div>
          )}
        </div>

        {/* Capabilities & Sandbox Badges */}
        {(() => {
          const caps = (guild as any).capabilities;
          const isSandboxActive = sandboxEnabled && (guild.name === 'bash' || guild.name === 'code');
          const hasCaps = caps !== undefined && caps !== null;

          if (!hasCaps && !isSandboxActive) return null;

          return (
            <div className="flex flex-wrap gap-1 mb-4 mt-2">
              {isSandboxActive && (
                <span 
                  title="Enforced Docker Sandbox Active"
                  className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-sky-500/10 text-sky-400 border border-sky-500/20 text-[9px] font-mono font-bold uppercase tracking-tight"
                >
                  <ShieldCheck className="w-2.5 h-2.5 text-sky-400" />
                  Sandbox
                </span>
              )}
              {hasCaps && (
                <>
                  {/* Process Execution */}
                  {caps.process_execution !== false && (
                    <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-rose-500/10 text-rose-400 border border-rose-500/20 text-[9px] font-mono uppercase tracking-tight">
                      <span className="w-1 h-1 rounded-full bg-rose-400" />
                      ProcExec
                    </span>
                  )}
                  {/* Filesystem Scope */}
                  {Array.isArray(caps.filesystem_scope) && caps.filesystem_scope.length > 0 && (
                    <span 
                      title={`FileSystem Scope: ${caps.filesystem_scope.join(', ')}`}
                      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20 text-[9px] font-mono uppercase tracking-tight cursor-help"
                    >
                      <span className="w-1 h-1 rounded-full bg-amber-400" />
                      FS ({caps.filesystem_scope.length})
                    </span>
                  )}
                  {/* Network Hosts */}
                  {Array.isArray(caps.network_hosts) && caps.network_hosts.length > 0 && (
                    <span 
                      title={`Allowed Network Hosts: ${caps.network_hosts.join(', ')}`}
                      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/20 text-[9px] font-mono uppercase tracking-tight cursor-help"
                    >
                      <span className="w-1 h-1 rounded-full bg-blue-400" />
                      NET ({caps.network_hosts.length})
                    </span>
                  )}
                </>
              )}
            </div>
          );
        })()}

        {/* Sandbox Override Selector */}
        <div className={cn(
          "mt-2 mb-3 pt-2 border-t border-slate-800/60 space-y-1.5 font-mono text-[9px]",
          !sandboxEnabled && "opacity-60"
        )}>
          <div className="flex justify-between items-center">
            <span className="text-slate-500">Effective Sandbox</span>
            <div className="flex items-center gap-1">
              <span className={cn(
                "px-1 py-0.5 rounded text-[8px] font-bold uppercase",
                !sandboxEnabled ? 'bg-slate-800 text-slate-500 border border-slate-700' :
                getEffectiveProfile(guild.name).profile === 'strict' ? 'bg-red-500/10 text-red-400 border border-red-500/20' :
                getEffectiveProfile(guild.name).profile === 'balanced' ? 'bg-blue-500/10 text-blue-400 border border-blue-500/20' :
                'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
              )}>
                {sandboxEnabled ? getEffectiveProfile(guild.name).profile : 'inactive'}
              </span>
              <span className="text-[8px] text-slate-600">
                ({sandboxEnabled ? getEffectiveProfile(guild.name).source : 'global off'})
              </span>
            </div>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-slate-500">Sandbox Override</span>
            <select
              value={guildOverrides[guild.name] || 'inherited'}
              disabled={!sandboxEnabled}
              onChange={async (e) => {
                const val = e.target.value as 'inherited' | 'strict' | 'balanced' | 'permissive';
                const previous = guildOverrides[guild.name] || 'inherited';
                if (val === 'inherited') {
                  notify(`Clearing a guild override isn't wired up yet — edit tylluan.toml directly or set another profile.`, 'error');
                  return;
                }
                if (!bridge) {
                  notify(`Cannot set guild override — no bridge connection`, 'error');
                  return;
                }
                setGuildOverrides(prev => ({ ...prev, [guild.name]: val }));
                try {
                  const result = await bridge.setGuildSandboxOverride(guild.name, val);
                  notify(
                    result.restart_required
                      ? `${guild.name} sandbox override set to ${val} — restart required to apply`
                      : `${guild.name} sandbox override set to ${val}`,
                    'info'
                  );
                } catch (err: any) {
                  setGuildOverrides(prev => ({ ...prev, [guild.name]: previous }));
                  notify(`Failed to set override for ${guild.name}: ${err.message}`, 'error');
                }
              }}
              title={!sandboxEnabled ? "Enable global sandbox to configure overrides" : undefined}
              className={cn(
                "bg-slate-950 border border-slate-800 rounded px-1.5 py-0.5 text-[8px] text-slate-300 font-mono focus:border-emerald-500 focus:outline-none",
                !sandboxEnabled && "cursor-not-allowed bg-slate-900 text-slate-600"
              )}
            >
              <option value="inherited">Inherited</option>
              <option value="strict">Strict</option>
              <option value="balanced">Balanced</option>
              <option value="permissive">Permissive</option>
            </select>
          </div>
        </div>

        {/* Action Buttons */}
        <div className="flex gap-2">
          {(guild.running || (guild.restarts_5m ?? 0) > 0) ? (
            <>
              <button type="button"
                onClick={() => handleGuildAction(guild.name, 'stop')}
                disabled={loading === guild.name}
                className="flex-1 flex items-center justify-center gap-1 px-2 py-1.5 bg-red-500/10 hover:bg-red-500/20 text-red-400 rounded text-xs transition-colors cursor-pointer disabled:cursor-not-allowed">
                <Square className="w-3 h-3" /> Stop
              </button>
              <button type="button"
                onClick={() => handleGuildAction(guild.name, 'restart')}
                disabled={loading === guild.name}
                title="Force Restart"
                className={cn(
                  'px-2 py-1.5 rounded text-xs transition-colors cursor-pointer disabled:cursor-not-allowed',
                  guild.running
                    ? 'bg-blue-500/10 hover:bg-blue-500/20 text-blue-400'
                    : 'bg-red-500/20 hover:bg-red-500/30 text-red-400'
                )}>
                <RefreshCw className={cn('w-3 h-3', loading === guild.name && 'animate-spin')} />
              </button>
            </>
          ) : status === 'lazy' ? (
            <>
              <button type="button"
                onClick={() => handleGuildAction(guild.name, 'start')}
                disabled={loading === guild.name}
                className="flex-1 flex items-center justify-center gap-1 px-2 py-1.5 bg-sky-500/15 hover:bg-sky-500/25 text-sky-400 border border-sky-500/20 rounded text-xs font-bold transition-all disabled:opacity-50 cursor-pointer">
                {loading === guild.name ? (
                  <RefreshCw className="w-3 h-3 animate-spin animate-spin-fast" />
                ) : (
                  <Zap className="w-3 h-3 text-sky-400" />
                )}
                Wake
              </button>
              <button type="button"
                onClick={() => handleGuildAction(guild.name, 'reset_start')}
                disabled={loading === guild.name}
                title="Reset crash backoff and start"
                className="px-2 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-400 rounded text-xs transition-colors cursor-pointer disabled:cursor-not-allowed">
                <Zap className="w-3 h-3" />
              </button>
            </>
          ) : (
            <>
              <button type="button"
                onClick={() => handleGuildAction(guild.name, 'start')}
                disabled={loading === guild.name}
                className="flex-1 flex items-center justify-center gap-1 px-2 py-1.5 bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400 rounded text-xs transition-colors cursor-pointer disabled:cursor-not-allowed">
                <Play className="w-3 h-3" /> Start
              </button>
              <button type="button"
                onClick={() => handleGuildAction(guild.name, 'reset_start')}
                disabled={loading === guild.name}
                title="Reset crash backoff and start"
                className="px-2 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-400 rounded text-xs transition-colors cursor-pointer disabled:cursor-not-allowed">
                <Zap className="w-3 h-3" />
              </button>
            </>
          )}
        </div>
      </div>
    );
  };

  const alwaysOnGuilds = filteredGuilds.filter(g => g.always_on);
  const onDemandGuilds = filteredGuilds.filter(g => !g.always_on);

  return (
    <div className="space-y-4">
      {/* Docker offline warning */}
      {dockerStatus?.status === 'offline' && (
        <div className="flex items-center gap-3 p-3 bg-amber-500/10 border border-amber-500/20 rounded-lg text-amber-500 text-xs">
          <AlertTriangle className="w-4 h-4 flex-shrink-0" />
          <div className="flex-1">
            <p className="font-bold">Sandbox Isolation Degraded</p>
            <p className="opacity-80">Docker Desktop is not running. Sandbox guilds will fail to initialize.</p>
          </div>
          <button type="button" onClick={() => window.location.reload()}
            className="px-3 py-1 bg-amber-500/20 hover:bg-amber-500/30 rounded font-bold uppercase tracking-wider text-[10px]">
            Retry
          </button>
        </div>
      )}

      {/* Sub-tab Navigation */}
      <div className="flex border-b border-slate-800 pb-px">
        <button
          type="button"
          onClick={() => setSubTab('overview')}
          className={cn(
            'flex items-center gap-2 px-4 py-2 border-b-2 text-xs font-mono font-bold tracking-wider uppercase transition-all',
            subTab === 'overview'
              ? 'border-blue-500 text-blue-400 bg-blue-500/5'
              : 'border-transparent text-slate-500 hover:text-slate-300'
          )}
        >
          <Layers className="w-3.5 h-3.5" />
          Overview
        </button>
        <button
          type="button"
          onClick={() => setSubTab('inspector')}
          className={cn(
            'flex items-center gap-2 px-4 py-2 border-b-2 text-xs font-mono font-bold tracking-wider uppercase transition-all',
            subTab === 'inspector'
              ? 'border-blue-500 text-blue-400 bg-blue-500/5'
              : 'border-transparent text-slate-500 hover:text-slate-300'
          )}
        >
          <Settings className="w-3.5 h-3.5" />
          Inspector & Playground
        </button>
      </div>

      {subTab === 'overview' ? (
        <>
          {/* Header + Category Filters */}
          <div className="flex items-center justify-between gap-3 flex-wrap">
            <div className="flex items-center gap-2">
              <Cpu className="w-4 h-4 text-slate-500" />
              <span className="text-xs text-slate-500 font-mono">{guilds?.length || 0} guilds registered</span>
              <span className="text-slate-700">·</span>
              <span className="text-xs text-emerald-500 font-mono">
                {guilds?.filter(g => g.running).length || 0} running
              </span>
            </div>
            {/* Docker status badge */}
            <div className="flex items-center gap-2">
              {dockerStatus === null ? (
                <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-slate-800 text-slate-500 border border-slate-700">
                  Docker...
                </span>
              ) : dockerStatus.status === 'online' ? (
                <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-500/20 text-emerald-400 border border-emerald-500/20">
                  Docker {dockerStatus.version ? `v${dockerStatus.version}` : ''}
                </span>
              ) : dockerStatus.status === 'error' ? (
                <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-red-500/20 text-red-400 border border-red-500/20">
                  Docker Unreachable
                </span>
              ) : (
                <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-red-500/20 text-red-400 border border-red-500/20">
                  Docker Offline
                </span>
              )}
              {/* Hide inactive toggle */}
              <button
                onClick={() => setHideInactive(h => !h)}
                className="text-xs text-slate-500 hover:text-slate-300 transition-colors px-2 py-1 rounded border border-slate-700 hover:border-slate-600"
              >
                {hideInactive ? `Show all (${guilds.length})` : `Hide inactive`}
              </button>
            </div>
            {/* Category filter pills */}
            <div className="flex gap-1.5 flex-wrap">
              {CATEGORIES.map(cat => (
                <button
                  key={cat}
                  type="button"
                  onClick={() => setActiveCategory(cat)}
                  className={cn(
                    'px-2.5 py-0.5 rounded-full text-[10px] font-bold border transition-colors uppercase tracking-wider',
                    activeCategory === cat
                      ? cat === 'all'
                        ? 'bg-slate-700 text-slate-200 border-slate-500'
                        : CATEGORY_STYLE[cat as GuildCategory].cls + ' opacity-100'
                      : 'bg-slate-900 text-slate-600 border-slate-800 hover:border-slate-600'
                  )}
                >
                  {cat === 'all' ? `All (${guilds.length})` : `${cat} (${categoryCounts[cat] || 0})`}
                </button>
              ))}
            </div>
          </div>

          {/* Sandbox Configuration Panel */}
          <div className="p-4 rounded-xl border border-slate-800/80 bg-slate-900/20 backdrop-blur-md space-y-4">
            <div className="flex items-center justify-between border-b border-slate-800/60 pb-3 flex-wrap gap-2">
              <div className="flex items-center gap-2">
                <ShieldCheck className="w-5 h-5 text-emerald-400" />
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider font-mono">Sandbox Security Profile</h3>
                  <p className="text-[10px] text-slate-500 font-mono">Configure execution isolation constraints for active guilds</p>
                </div>
              </div>
              <div className="flex items-center gap-2">
                <span className={cn(
                  "px-2 py-0.5 rounded text-[8px] font-mono font-bold uppercase border",
                  sandboxEnabled 
                    ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/25"
                    : "bg-slate-800 text-slate-500 border-slate-700"
                )}>
                  Sandbox: {sandboxEnabled ? "Enabled" : "Disabled"}
                </span>
                {isBackendMock && (
                  <span className="px-2 py-0.5 rounded text-[8px] font-mono font-bold bg-amber-500/10 text-amber-400 border border-amber-500/20 uppercase tracking-tighter" title="Backend implementation is currently a mock stub">
                    Mock Mode
                  </span>
                )}
              </div>
            </div>

            {/* Profile Options Grid */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
              {[
                {
                  id: 'strict',
                  label: 'Strict',
                  desc: 'Full isolation: Docker for all guilds, process_execution=false, network=none, filesystem=read-only workspace.',
                  color: 'hover:border-red-500/30 active:border-red-500/50',
                  activeColor: 'border-red-500 bg-red-950/10 text-red-400 shadow-[0_0_12px_rgba(239,68,68,0.05)]'
                },
                {
                  id: 'balanced',
                  label: 'Balanced',
                  desc: 'Moderate isolation: Docker for bash/code only, enforcement per declared capabilities, network/filesystem per guild declaration. (Default)',
                  color: 'hover:border-blue-500/30 active:border-blue-500/50',
                  activeColor: 'border-blue-500 bg-blue-950/10 text-blue-400 shadow-[0_0_12px_rgba(59,130,246,0.05)]'
                },
                {
                  id: 'permissive',
                  label: 'Permissive',
                  desc: 'No isolation: no Docker, process_execution allowed, full network and filesystem access. Advisory-only capability declarations.',
                  color: 'hover:border-emerald-500/30 active:border-emerald-500/50',
                  activeColor: 'border-emerald-500 bg-emerald-950/10 text-emerald-400 shadow-[0_0_12px_rgba(16,185,129,0.05)]'
                }
              ].map((p) => {
                const isActive = sandboxProfile === p.id;
                return (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() => handleProfileChange(p.id as any)}
                    className={cn(
                      "p-3 rounded-xl border text-left transition-all duration-200 cursor-pointer flex flex-col space-y-1.5",
                      isActive 
                        ? p.activeColor 
                        : "border-slate-850 bg-slate-950/30 text-slate-400 " + p.color
                    )}
                  >
                    <span className="text-xs font-bold font-mono uppercase tracking-wider">{p.label}</span>
                    <span className="text-[10px] text-slate-500 leading-normal font-mono">{p.desc}</span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Main Guild Cards Sections */}
          {filteredGuilds.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-16 text-slate-600">
              <Cpu className="w-10 h-10 mb-3 opacity-30" />
              <p className="text-sm font-mono">
                {activeCategory === 'all'
                  ? 'No guilds registered yet'
                  : `No ${activeCategory} guilds found`}
              </p>
            </div>
          ) : (
            <div className="space-y-8">
              {/* Section 1: Always-On Systems */}
              <div className="space-y-3">
                <div className="border-b border-slate-800/80 pb-2">
                  <h3 className="text-sm font-bold text-slate-300 uppercase font-mono tracking-wider flex items-center gap-2">
                    <Cpu className="w-4 h-4 text-emerald-400" />
                    Always-On Systems
                  </h3>
                </div>
                {alwaysOnGuilds.length === 0 ? (
                  <p className="text-xs text-slate-600 font-mono italic pl-2">No always-on guilds fit the active filter</p>
                ) : (
                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                    {alwaysOnGuilds.map(renderGuildCard)}
                  </div>
                )}
              </div>

              {/* Section 2: On-Demand (Lazy) Guilds */}
              <div className="space-y-3 pt-2">
                <div className="border-b border-slate-800/80 pb-2">
                  <div className="flex items-baseline justify-between gap-4 flex-wrap">
                    <h3 className="text-sm font-bold text-slate-300 uppercase font-mono tracking-wider flex items-center gap-2">
                      <Zap className="w-4 h-4 text-sky-400" />
                      On-Demand Guilds
                    </h3>
                    <span className="text-[10px] text-slate-500 font-mono">Start automatically when needed</span>
                  </div>
                </div>
                {onDemandGuilds.length === 0 ? (
                  <p className="text-xs text-slate-600 font-mono italic pl-2">No on-demand guilds fit the active filter</p>
                ) : (
                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                    {onDemandGuilds.map(renderGuildCard)}
                  </div>
                )}
              </div>
            </div>
          )}
        </>
      ) : (
        <GuildInspector bridge={bridge} notify={notify} guilds={guilds} />
      )}
    </div>
  );
}
