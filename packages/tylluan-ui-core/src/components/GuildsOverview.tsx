import { useState, useEffect, useCallback } from 'react';
import { Cpu, Play, Square, RefreshCw, Zap, AlertTriangle } from 'lucide-react';
import { cn } from '../lib/utils';
import type { NexusBridge, Guild } from '../lib/api-client';
import { getGuildCategory, CATEGORY_STYLE, DEPRECATED_GUILDS } from '../lib/guild-meta';
import type { GuildCategory } from '../lib/guild-meta';

type GuildStatus = 'running' | 'degraded' | 'crashed' | 'down' | 'lazy';

function resolveStatus(guild: Guild): GuildStatus {
  if ((guild.restarts_5m ?? 0) >= 3) return 'crashed';
  if (guild.running && (guild.last_latency_ms ?? 0) > 5000) return 'degraded';
  if (guild.running) return 'running';
  if (guild.always_on) return 'down';
  return 'lazy';
}

const STATUS_STYLE: Record<GuildStatus, { label: string; dot: string; badge: string }> = {
  running:  { label: 'RUNNING',  dot: 'bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.5)]',  badge: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/20' },
  degraded: { label: 'DEGRADED', dot: 'bg-amber-500 animate-pulse',                             badge: 'bg-amber-500/20 text-amber-400 border-amber-500/20' },
  crashed:  { label: 'CRASH',    dot: 'bg-red-500 animate-pulse',                               badge: 'bg-red-500/20 text-red-400 border-red-500/20' },
  down:     { label: 'DOWN',     dot: 'bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.5)]',       badge: 'bg-red-500/20 text-red-400 border-red-500/20' },
  lazy:     { label: 'LAZY',     dot: 'bg-blue-500/60',                                          badge: 'bg-blue-500/10 text-blue-400 border-blue-500/20' },
};

interface Props {
  bridge: NexusBridge | null;
  guilds: Guild[];
  notify?: (msg: string, type?: 'info' | 'error') => void;
  onRefresh?: () => void;
}

export function GuildsOverview({ bridge, guilds, notify, onRefresh }: Props) {
  const [loading, setLoading] = useState<string | null>(null);
  const [activeCategory, setActiveCategory] = useState<GuildCategory | 'all'>('all');
  const [hideInactive, setHideInactive] = useState(true);

  const filteredGuilds = guilds.filter(g => {
    if (hideInactive && DEPRECATED_GUILDS.has(g.name) && !g.running && g.restarts_5m === 0) return false;
    if (activeCategory === 'all') return true;
    return getGuildCategory(g.name) === activeCategory;
  });

  const categoryCounts = guilds.reduce<Record<string, number>>((acc, g) => {
    const cat = getGuildCategory(g.name);
    acc[cat] = (acc[cat] || 0) + 1;
    return acc;
  }, {});

  const CATEGORIES: (GuildCategory | 'all')[] = ['all', 'Core', 'Builder', 'Scholar', 'Watcher', 'Tool'];

  const handleAction = async (name: string, action: 'start' | 'stop' | 'restart') => {
    if (!bridge) return;
    setLoading(name);
    try {
      if (action === 'start' || action === 'restart') {
        await bridge.startGuild(name);
      } else {
        await bridge.stopGuild(name);
      }
      notify?.(`${action === 'start' ? 'Started' : action === 'restart' ? 'Restarted' : 'Stopped'} guild: ${name}`, 'info');
      onRefresh?.();
    } catch (e) {
      notify?.(`Failed: ${name} — ${e instanceof Error ? e.message : 'Unknown'}`, 'error');
    }
    setLoading(null);
  };

  const alwaysOn = filteredGuilds.filter(g => g.always_on);
  const onDemand = filteredGuilds.filter(g => !g.always_on);

  return (
    <div className="space-y-5 animate-in fade-in duration-500">
      {/* Header + category filter */}
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div className="flex items-center gap-2">
          <Cpu className="w-4 h-4 text-white/40" />
          <span className="text-xs text-white/50">{guilds.length} guilds</span>
          <span className="text-white/20">·</span>
          <span className="text-xs text-emerald-400">{guilds.filter(g => g.running).length} running</span>
        </div>
        <button
          type="button"
          onClick={() => setHideInactive(h => !h)}
          className="text-[10px] text-white/30 hover:text-white/60 px-2 py-1 rounded border border-white/10"
        >
          {hideInactive ? `Show all (${guilds.length})` : 'Hide inactive'}
        </button>
      </div>

      {/* Category pills */}
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
                  ? 'bg-white/10 text-white/80 border-white/20'
                  : CATEGORY_STYLE[cat as GuildCategory].cls
                : 'bg-transparent text-white/30 border-white/5 hover:border-white/15'
            )}
          >
            {cat === 'all' ? `All (${guilds.length})` : `${cat} (${categoryCounts[cat] || 0})`}
          </button>
        ))}
      </div>

      {/* Guild cards */}
      {filteredGuilds.length === 0 ? (
        <div className="text-center py-12 text-white/20">
          <Cpu className="w-10 h-10 mx-auto mb-3 opacity-30" />
          <p className="text-sm">{activeCategory === 'all' ? 'No guilds registered' : `No ${activeCategory} guilds`}</p>
        </div>
      ) : (
        <div className="space-y-6">
          {/* Always-On */}
          {alwaysOn.length > 0 && (
            <div className="space-y-3">
              <h3 className="text-xs font-semibold text-white/50 uppercase tracking-wider flex items-center gap-2">
                <Cpu className="w-3.5 h-3.5 text-emerald-400" /> Always-On
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                {alwaysOn.map(g => <GuildCard key={g.name} guild={g} loading={loading} onAction={handleAction} />)}
              </div>
            </div>
          )}
          {/* On-Demand */}
          {onDemand.length > 0 && (
            <div className="space-y-3">
              <h3 className="text-xs font-semibold text-white/50 uppercase tracking-wider flex items-center gap-2">
                <Zap className="w-3.5 h-3.5 text-blue-400" /> On-Demand
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                {onDemand.map(g => <GuildCard key={g.name} guild={g} loading={loading} onAction={handleAction} />)}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function GuildCard({ guild, loading, onAction }: { guild: Guild; loading: string | null; onAction: (name: string, action: 'start' | 'stop' | 'restart') => void }) {
  const status = resolveStatus(guild);
  const statusStyle = STATUS_STYLE[status];
  const category = getGuildCategory(guild.name);
  const catStyle = CATEGORY_STYLE[category];
  const isLoading = loading === guild.name;

  return (
    <div className={cn(
      'p-4 rounded-lg border bg-white/[0.02] transition-all duration-200',
      status === 'running' ? 'border-emerald-500/20' :
      status === 'crashed' ? 'border-red-500/20 bg-red-500/5' :
      status === 'down' ? 'border-red-500/10' :
      'border-white/5'
    )}>
      {/* Header */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2 min-w-0">
          <div className={cn('w-2 h-2 rounded-full shrink-0', statusStyle.dot)} />
          <span className="text-sm font-semibold text-white/80 truncate">{guild.name}</span>
        </div>
        <div className="flex items-center gap-1.5">
          <span className={cn('text-[8px] px-1.5 py-0.5 rounded font-bold border uppercase', catStyle.cls)}>
            {catStyle.label}
          </span>
          <span className={cn('text-[9px] px-2 py-0.5 rounded-full font-black border', statusStyle.badge)}>
            {statusStyle.label}
          </span>
        </div>
      </div>

      {/* Metrics */}
      <div className="text-[10px] text-white/30 space-y-0.5 mb-3 font-mono">
        <div className="flex justify-between">
          <span>Always-on</span>
          <span className={guild.always_on ? 'text-amber-400' : 'text-white/20'}>{guild.always_on ? 'Yes' : 'No'}</span>
        </div>
        <div className="flex justify-between">
          <span>Calls / Latency</span>
          <span className="text-white/40">{guild.total_calls || 0} / {guild.last_latency_ms ? `${guild.last_latency_ms}ms` : '—'}</span>
        </div>
        {(guild.restarts_5m ?? 0) > 0 && (
          <div className="flex justify-between text-red-400">
            <span>Restarts (5m)</span>
            <span className="font-bold">{guild.restarts_5m}</span>
          </div>
        )}
      </div>

      {/* Actions */}
      <div className="flex gap-2">
        {guild.running || (guild.restarts_5m ?? 0) > 0 ? (
          <>
            <button type="button" onClick={() => onAction(guild.name, 'stop')} disabled={isLoading}
              className="flex-1 flex items-center justify-center gap-1 px-2 py-1.5 bg-red-500/10 hover:bg-red-500/20 text-red-400 rounded text-xs transition-colors disabled:opacity-50">
              <Square className="w-3 h-3" /> Stop
            </button>
            <button type="button" onClick={() => onAction(guild.name, 'restart')} disabled={isLoading}
              className="px-2 py-1.5 bg-blue-500/10 hover:bg-blue-500/20 text-blue-400 rounded text-xs transition-colors disabled:opacity-50">
              <RefreshCw className={cn('w-3 h-3', isLoading && 'animate-spin')} />
            </button>
          </>
        ) : (
          <button type="button" onClick={() => onAction(guild.name, 'start')} disabled={isLoading}
            className="flex-1 flex items-center justify-center gap-1 px-2 py-1.5 bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400 rounded text-xs transition-colors disabled:opacity-50">
            <Play className="w-3 h-3" /> Start
          </button>
        )}
      </div>
    </div>
  );
}
