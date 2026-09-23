import React, { useState, useEffect, useCallback, useMemo } from 'react';
import {
  GitCompare,
  CheckCircle2,
  AlertTriangle,
  Radio,
  RefreshCw,
  Search,
  Filter,
  BarChart3,
  Layers,
  ArrowUpDown,
  Zap,
  Info,
  X,
  Copy,
  Check,
  Cpu,
  Sliders,
  ShieldCheck,
  Flame,
  Activity,
  Terminal,
} from 'lucide-react';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import { MetricCard } from './ui/MetricPrimitives';

export interface GuildConfusionTally {
  Agrees: number;
  Differ: number;
}

export interface SchedulerConfusionData {
  total: number;
  agrees: number;
  differ: number;
  differ_with_cascade_fired: number;
  by_guild: Record<string, GuildConfusionTally>;
  note?: string;
}

export interface GuildConfusionMetrics {
  guild: string;
  agrees: number;
  differ: number;
  total: number;
  agreement_rate: number; // 0 to 100
  disagreement_rate: number; // 0 to 100
}

export interface SchedulerConfusionPanelProps {
  bridge?: {
    fetchRaw: (url: string, init?: RequestInit) => Promise<unknown>;
  } | null;
  notify?: (msg: string, type?: 'info' | 'error') => void;
}

export function computeDerivedMetrics(data: SchedulerConfusionData) {
  const total = Number(data.total) || 0;
  const agrees = Number(data.agrees) || 0;
  const differ = Number(data.differ) || 0;
  const differ_with_cascade_fired = Number(data.differ_with_cascade_fired) || 0;

  const agreement_rate = total > 0 ? (agrees / total) * 100 : 100;
  const disagreement_rate = total > 0 ? (differ / total) * 100 : 0;
  const cascade_fired_rate_on_differ = differ > 0 ? (differ_with_cascade_fired / differ) * 100 : 0;

  const guildMetrics: GuildConfusionMetrics[] = Object.entries(data.by_guild || {}).map(([guild, tally]) => {
    const gAgrees = Number(tally?.Agrees) || 0;
    const gDiffer = Number(tally?.Differ) || 0;
    const gTotal = gAgrees + gDiffer;
    const gAgrRate = gTotal > 0 ? (gAgrees / gTotal) * 100 : 100;
    const gDisRate = gTotal > 0 ? (gDiffer / gTotal) * 100 : 0;
    return {
      guild,
      agrees: gAgrees,
      differ: gDiffer,
      total: gTotal,
      agreement_rate: gAgrRate,
      disagreement_rate: gDisRate,
    };
  });

  return {
    total,
    agrees,
    differ,
    differ_with_cascade_fired,
    agreement_rate,
    disagreement_rate,
    cascade_fired_rate_on_differ,
    guildMetrics,
  };
}

// Realistic mock reference for development / preview / offline
export const MOCK_SCHEDULER_CONFUSION: SchedulerConfusionData = {
  total: 142,
  agrees: 128,
  differ: 14,
  differ_with_cascade_fired: 4,
  by_guild: {
    'bash': { Agrees: 48, Differ: 2 },
    'silva': { Agrees: 36, Differ: 1 },
    'coloquio': { Agrees: 24, Differ: 0 },
    'browser': { Agrees: 12, Differ: 4 },
    'vision_moondream': { Agrees: 5, Differ: 2 },
    'n8n_bridge': { Agrees: 3, Differ: 5 },
  },
  note: "observation-only tallies; cutover is a separate Tech Lead decision",
};

export function SchedulerConfusionPanel({ bridge, notify }: SchedulerConfusionPanelProps = {}) {
  const [data, setData] = useState<SchedulerConfusionData>(MOCK_SCHEDULER_CONFUSION);
  const [loading, setLoading] = useState(false);
  const [isLive, setIsLive] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [sortBy, setSortBy] = useState<'disagreement' | 'volume' | 'agreement' | 'name'>('disagreement');
  const [selectedGuild, setSelectedGuild] = useState<GuildConfusionMetrics | null>(null);
  const [copiedNote, setCopiedNote] = useState(false);

  const fetchConfusion = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      const res = (await bridge.fetchRaw('/api/v1/scheduler/confusion')) as SchedulerConfusionData;
      if (res && typeof res.total === 'number') {
        setData(res);
        setIsLive(true);
      }
    } catch (err) {
      console.warn('Failed to fetch scheduler confusion tallies:', err);
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  usePolling('scheduler-confusion-tallies', fetchConfusion, { interval: 'standard' });

  useEffect(() => {
    if (bridge) {
      fetchConfusion();
    }
  }, [bridge, fetchConfusion]);

  const metrics = useMemo(() => computeDerivedMetrics(data), [data]);

  // Filter and sort guild rows
  const filteredGuilds = useMemo(() => {
    return metrics.guildMetrics
      .filter(g => g.guild.toLowerCase().includes(searchQuery.toLowerCase()))
      .sort((a, b) => {
        if (sortBy === 'disagreement') {
          return b.disagreement_rate - a.disagreement_rate || b.differ - a.differ;
        }
        if (sortBy === 'volume') {
          return b.total - a.total;
        }
        if (sortBy === 'agreement') {
          return b.agreement_rate - a.agreement_rate || b.agrees - a.agrees;
        }
        return a.guild.localeCompare(b.guild);
      });
  }, [metrics.guildMetrics, searchQuery, sortBy]);

  const handleCopyJson = () => {
    navigator.clipboard.writeText(JSON.stringify(data, null, 2));
    setCopiedNote(true);
    if (notify) notify('Scheduler confusion JSON copied to clipboard', 'info');
    setTimeout(() => setCopiedNote(false), 2000);
  };

  const isDegradedOrEmpty = metrics.total === 0;
  const isStoreNotCreated = data.note && data.note.toLowerCase().includes('not created');
  const isStoreUnreadable = data.note && data.note.toLowerCase().includes('unreadable');

  return (
    <div className="flex-1 min-h-0 flex flex-col space-y-4 overflow-y-auto pr-1">
      {/* Header Banner */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 p-4 rounded-xl border border-slate-800 bg-slate-900/50 backdrop-blur-sm">
        <div className="flex items-start gap-3">
          <div className="p-2.5 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 mt-0.5">
            <GitCompare className="w-5 h-5" />
          </div>
          <div>
            <div className="flex items-center gap-2 flex-wrap">
              <h2 className="text-base font-bold text-slate-100 tracking-tight font-mono">
                Cognitive Scheduler Confusion Matrix
              </h2>
              <span className="px-2 py-0.5 text-[10px] font-mono uppercase rounded-full bg-emerald-950/60 border border-emerald-800/50 text-emerald-300">
                ADR-014 WS3
              </span>
              {isLive ? (
                <span className="flex items-center gap-1 px-2 py-0.5 text-[10px] font-mono uppercase rounded-full bg-emerald-950/60 border border-emerald-800/50 text-emerald-300">
                  <Radio className="w-2.5 h-2.5 text-emerald-400 animate-pulse" />
                  Live Kernel Stream
                </span>
              ) : (
                <span className="px-2 py-0.5 text-[10px] font-mono uppercase rounded-full bg-slate-800 border border-slate-700 text-slate-400">
                  Observation Preview
                </span>
              )}
            </div>
            <p className="text-xs text-slate-400 mt-0.5">
              Observation-only tallies comparing Cognitive Scheduler decision vs actual dispatch execution
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          {bridge && (
            <button
              onClick={fetchConfusion}
              disabled={loading}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-800 bg-slate-900/60 hover:bg-slate-800/80 text-slate-300 text-xs font-mono transition-all disabled:opacity-50"
            >
              <RefreshCw className={cn('w-3.5 h-3.5', loading && 'animate-spin text-emerald-400')} />
              <span>Refresh</span>
            </button>
          )}
          <button
            onClick={handleCopyJson}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-800 bg-slate-900/60 hover:bg-slate-800/80 text-slate-300 text-xs font-mono transition-all"
          >
            {copiedNote ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
            <span>{copiedNote ? 'Copied' : 'Copy JSON'}</span>
          </button>
          <button
            onClick={() => {
              setData(MOCK_SCHEDULER_CONFUSION);
              if (notify) notify('Loaded reference simulation tallies', 'info');
            }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-800 bg-slate-900/60 hover:bg-slate-800/80 text-slate-300 text-xs font-mono transition-all"
          >
            <Activity className="w-3.5 h-3.5 text-amber-400" />
            <span>Reset / Mock</span>
          </button>
        </div>
      </div>

      {/* Observation Note / Alert Banner */}
      {data.note && (
        <div className={cn(
          "flex items-start gap-2.5 px-3.5 py-2.5 rounded-lg border text-xs font-mono",
          isStoreNotCreated || isStoreUnreadable
            ? "bg-amber-950/30 border-amber-800/50 text-amber-300"
            : "bg-slate-900/60 border-slate-800 text-slate-300"
        )}>
          <Info className="w-4 h-4 text-amber-400 flex-shrink-0 mt-0.5" />
          <div className="flex-1">
            <span className="font-semibold uppercase tracking-wider text-[10px] text-amber-400 mr-2">
              Observation Mode:
            </span>
            <span>{data.note}</span>
          </div>
        </div>
      )}

      {/* KPI Metric Cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard
          label="Decisions Observed"
          value={metrics.total.toString()}
          sub="Total router dispatches logged"
          icon={Cpu}
        />
        <MetricCard
          label="Concordance Rate"
          value={`${metrics.agreement_rate.toFixed(1)}%`}
          sub={`${metrics.agrees} agreed / ${metrics.total} total`}
          icon={CheckCircle2}
          valueClass="text-emerald-400"
        />
        <MetricCard
          label="Confusion Rate"
          value={`${metrics.disagreement_rate.toFixed(1)}%`}
          sub={`${metrics.differ} disagreed verdicts`}
          icon={AlertTriangle}
          valueClass={metrics.differ > 0 ? "text-amber-400" : "text-slate-100"}
        />
        <MetricCard
          label="Cascade Fired on Differ"
          value={`${metrics.cascade_fired_rate_on_differ.toFixed(1)}%`}
          sub={`${metrics.differ_with_cascade_fired} / ${metrics.differ || 1} triggered cascade`}
          icon={Flame}
          valueClass="text-orange-400"
        />
      </div>

      {/* Spectrum Concordance Bar */}
      <div className="p-4 rounded-xl border border-slate-800 bg-slate-900/40 space-y-3">
        <div className="flex items-center justify-between text-xs font-mono">
          <span className="text-slate-300 font-semibold flex items-center gap-2">
            <Sliders className="w-3.5 h-3.5 text-emerald-400" />
            Verdict Concordance Distribution
          </span>
          <span className="text-slate-400 text-[11px]">
            {metrics.agrees} Concordant vs {metrics.differ} Divergent
          </span>
        </div>

        {/* Multi-segment distribution progress bar */}
        <div className="w-full h-3.5 rounded-full bg-slate-950 overflow-hidden flex border border-slate-800">
          {metrics.total > 0 ? (
            <>
              <div
                style={{ width: `${metrics.agreement_rate}%` }}
                className="h-full bg-emerald-500 transition-all duration-500 relative group"
                title={`Agrees: ${metrics.agrees} (${metrics.agreement_rate.toFixed(1)}%)`}
              />
              <div
                style={{
                  width: `${(metrics.differ_with_cascade_fired / metrics.total) * 100}%`,
                }}
                className="h-full bg-orange-500 transition-all duration-500"
                title={`Differ + Cascade: ${metrics.differ_with_cascade_fired}`}
              />
              <div
                style={{
                  width: `${((metrics.differ - metrics.differ_with_cascade_fired) / metrics.total) * 100}%`,
                }}
                className="h-full bg-rose-500 transition-all duration-500"
                title={`Differ (No Cascade): ${metrics.differ - metrics.differ_with_cascade_fired}`}
              />
            </>
          ) : (
            <div className="w-full h-full bg-slate-800 animate-pulse" />
          )}
        </div>

        <div className="flex items-center justify-between text-[11px] font-mono text-slate-400 flex-wrap gap-2 pt-1">
          <div className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-full bg-emerald-500 inline-block" />
            <span>Agrees ({metrics.agrees})</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-full bg-orange-500 inline-block" />
            <span>Differ + Cascade ({metrics.differ_with_cascade_fired})</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-full bg-rose-500 inline-block" />
            <span>Differ Only ({Math.max(0, metrics.differ - metrics.differ_with_cascade_fired)})</span>
          </div>
        </div>
      </div>

      {/* Guild Breakdown Table / Matrix */}
      <div className="p-4 rounded-xl border border-slate-800 bg-slate-900/40 space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <BarChart3 className="w-4 h-4 text-emerald-400" />
            <h3 className="text-sm font-bold text-slate-200 font-mono">
              Per-Guild Confusion Matrix ({filteredGuilds.length} Guilds)
            </h3>
          </div>

          <div className="flex items-center gap-2 flex-wrap">
            {/* Search Input */}
            <div className="relative">
              <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-500" />
              <input
                type="text"
                placeholder="Search guild..."
                value={searchQuery}
                onChange={e => setSearchQuery(e.target.value)}
                className="pl-8 pr-3 py-1 bg-slate-950/80 border border-slate-800 rounded-lg text-xs text-slate-200 placeholder-slate-500 font-mono focus:outline-none focus:border-emerald-500/50 w-36 sm:w-48 transition-all"
              />
            </div>

            {/* Sort Dropdown */}
            <div className="flex items-center gap-1.5 bg-slate-950/80 border border-slate-800 rounded-lg px-2.5 py-1 text-xs font-mono text-slate-300">
              <ArrowUpDown className="w-3 h-3 text-slate-400" />
              <select
                value={sortBy}
                onChange={e => setSortBy(e.target.value as any)}
                className="bg-transparent text-slate-200 font-mono focus:outline-none text-xs cursor-pointer"
              >
                <option value="disagreement" className="bg-slate-900">Highest Disagreement</option>
                <option value="volume" className="bg-slate-900">Total Volume</option>
                <option value="agreement" className="bg-slate-900">Highest Agreement</option>
                <option value="name" className="bg-slate-900">Guild Name</option>
              </select>
            </div>
          </div>
        </div>

        {/* Empty state when no data or no matching search */}
        {isDegradedOrEmpty && metrics.guildMetrics.length === 0 ? (
          <div className="p-8 rounded-lg border border-dashed border-slate-800 text-center space-y-2">
            <Cpu className="w-8 h-8 text-slate-600 mx-auto animate-pulse" />
            <p className="text-xs font-mono text-slate-300 font-medium">
              No Scheduler Confusion Observations Recorded Yet
            </p>
            <p className="text-[11px] font-mono text-slate-500 max-w-md mx-auto">
              The Cognitive Scheduler is running in passive observation mode. As dispatch intents execute through the kernel, concordance tallies will automatically accumulate here.
            </p>
            <button
              onClick={() => setData(MOCK_SCHEDULER_CONFUSION)}
              className="mt-2 inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono rounded-lg bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 hover:bg-emerald-500/20 transition-all"
            >
              <Activity className="w-3.5 h-3.5" />
              Preview Reference Dataset
            </button>
          </div>
        ) : filteredGuilds.length === 0 ? (
          <div className="p-6 text-center text-xs font-mono text-slate-500">
            No guilds matching query "{searchQuery}"
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {filteredGuilds.map(guildItem => {
              const isHighConfusion = guildItem.disagreement_rate > 20;
              const isModerateConfusion = guildItem.disagreement_rate > 5 && guildItem.disagreement_rate <= 20;

              return (
                <div
                  key={guildItem.guild}
                  onClick={() => setSelectedGuild(guildItem)}
                  className="p-3.5 rounded-lg border border-slate-800/80 bg-slate-950/60 hover:bg-slate-900/80 hover:border-slate-700 transition-all cursor-pointer group flex flex-col justify-between space-y-3"
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <div className="w-7 h-7 rounded-md bg-slate-900 border border-slate-800 flex items-center justify-center font-mono font-bold text-xs text-slate-300 group-hover:border-emerald-500/40 transition-colors">
                        <Terminal className="w-3.5 h-3.5 text-slate-400 group-hover:text-emerald-400" />
                      </div>
                      <div>
                        <h4 className="text-xs font-bold font-mono text-slate-200 group-hover:text-emerald-300 transition-colors">
                          {guildItem.guild}
                        </h4>
                        <span className="text-[10px] font-mono text-slate-500">
                          {guildItem.total} decision{guildItem.total === 1 ? '' : 's'} evaluated
                        </span>
                      </div>
                    </div>

                    <div className="text-right">
                      {isHighConfusion ? (
                        <span className="px-2 py-0.5 text-[10px] font-mono font-bold uppercase rounded-md bg-rose-950/60 border border-rose-800/50 text-rose-300 flex items-center gap-1">
                          <AlertTriangle className="w-2.5 h-2.5" />
                          {guildItem.disagreement_rate.toFixed(1)}% Differ
                        </span>
                      ) : isModerateConfusion ? (
                        <span className="px-2 py-0.5 text-[10px] font-mono font-bold uppercase rounded-md bg-amber-950/60 border border-amber-800/50 text-amber-300">
                          {guildItem.disagreement_rate.toFixed(1)}% Differ
                        </span>
                      ) : (
                        <span className="px-2 py-0.5 text-[10px] font-mono font-bold uppercase rounded-md bg-emerald-950/60 border border-emerald-800/50 text-emerald-300 flex items-center gap-1">
                          <CheckCircle2 className="w-2.5 h-2.5" />
                          {guildItem.agreement_rate.toFixed(1)}% Concordant
                        </span>
                      )}
                    </div>
                  </div>

                  {/* Progress Bar comparison */}
                  <div className="space-y-1">
                    <div className="w-full h-2 rounded-full bg-slate-900 overflow-hidden flex border border-slate-800/50">
                      <div
                        style={{ width: `${guildItem.agreement_rate}%` }}
                        className="h-full bg-emerald-500 transition-all duration-300"
                      />
                      <div
                        style={{ width: `${guildItem.disagreement_rate}%` }}
                        className="h-full bg-rose-500 transition-all duration-300"
                      />
                    </div>
                    <div className="flex items-center justify-between text-[10px] font-mono text-slate-400">
                      <span className="text-emerald-400">Agrees: {guildItem.agrees}</span>
                      <span className={guildItem.differ > 0 ? "text-rose-400" : "text-slate-500"}>
                        Differ: {guildItem.differ}
                      </span>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Guild Inspector Modal */}
      {selectedGuild && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm animate-in fade-in duration-150">
          <div className="w-full max-w-lg rounded-xl border border-slate-800 bg-slate-950 p-5 shadow-2xl space-y-4 font-mono">
            <div className="flex items-start justify-between border-b border-slate-800/80 pb-3">
              <div className="flex items-center gap-2.5">
                <div className="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400">
                  <Cpu className="w-5 h-5" />
                </div>
                <div>
                  <h3 className="text-sm font-bold text-slate-100">
                    Guild: {selectedGuild.guild}
                  </h3>
                  <p className="text-[11px] text-slate-400">
                    Cognitive Scheduler Observation Profile (ADR-014)
                  </p>
                </div>
              </div>
              <button
                onClick={() => setSelectedGuild(null)}
                aria-label="Close modal"
                className="p-1 rounded-lg text-slate-400 hover:text-slate-200 hover:bg-slate-900 transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <div className="grid grid-cols-2 gap-2 text-xs">
              <div className="p-3 rounded-lg bg-slate-900/60 border border-slate-800">
                <span className="text-slate-500 text-[10px] uppercase">Concordance (Agrees)</span>
                <p className="text-base font-bold text-emerald-400 mt-0.5">
                  {selectedGuild.agrees} <span className="text-xs text-slate-400">({selectedGuild.agreement_rate.toFixed(1)}%)</span>
                </p>
              </div>
              <div className="p-3 rounded-lg bg-slate-900/60 border border-slate-800">
                <span className="text-slate-500 text-[10px] uppercase">Divergence (Differ)</span>
                <p className="text-base font-bold text-rose-400 mt-0.5">
                  {selectedGuild.differ} <span className="text-xs text-slate-400">({selectedGuild.disagreement_rate.toFixed(1)}%)</span>
                </p>
              </div>
            </div>

            <div className="p-3.5 rounded-lg bg-slate-900/40 border border-slate-800/80 text-xs space-y-2 text-slate-300">
              <span className="text-slate-400 font-semibold block text-[11px] uppercase tracking-wider">
                Observation Insights
              </span>
              <p className="text-[11px] text-slate-400 leading-relaxed">
                When the router receives an intent targeting <span className="text-slate-200 font-bold">{selectedGuild.guild}</span>, the cognitive classifier and heuristic router agreed in {selectedGuild.agrees} out of {selectedGuild.total} executions.
              </p>
              {selectedGuild.disagreement_rate > 20 ? (
                <p className="text-[11px] text-amber-300 bg-amber-950/40 border border-amber-800/50 p-2 rounded">
                  ⚠️ Higher divergence observed ({selectedGuild.disagreement_rate.toFixed(1)}%). Consider evaluating execution class prompts or heuristic weights before scheduling cutover.
                </p>
              ) : (
                <p className="text-[11px] text-emerald-300 bg-emerald-950/40 border border-emerald-800/50 p-2 rounded">
                  ✓ High alignment with heuristic dispatcher. Ready for autonomous cutover consideration.
                </p>
              )}
            </div>

            <div className="flex justify-end pt-1">
              <button
                onClick={() => setSelectedGuild(null)}
                className="px-4 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono transition-colors"
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
