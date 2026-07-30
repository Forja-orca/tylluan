import { useState, useCallback, useRef } from 'react';
import {
  Activity, AlertTriangle, RefreshCw, Route, Clock, RotateCcw,
  Users, Zap, ArrowUp, ArrowDown, Minus, Gauge,
} from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import type { FrictionStats } from '../lib/api/security';

export interface FrictionPanelProps {
  bridge: NexusBridge | null;
}

// ── helpers ──────────────────────────────────────────────────────────────────

function formatVal(v: number | undefined): string {
  if (v === undefined || v === null) return '\u2014';
  return v.toLocaleString();
}

function formatScore(v: number): string {
  if (v === 0) return '0.0';
  return v < 0.1 ? '<0.1' : v.toFixed(1);
}

function TrendBadge({ current, previous }: { current: number; previous: number | null }) {
  if (previous === null || (current === 0 && previous === 0)) {
    return (
      <span className="inline-flex items-center gap-0.5 text-[10px] font-mono text-slate-500">
        <Minus className="w-3 h-3" /> sin datos
      </span>
    );
  }
  const delta = current - previous;
  if (delta > 0) {
    return (
      <span className="inline-flex items-center gap-0.5 text-[10px] font-mono text-red-400">
        <ArrowUp className="w-3 h-3" /> +{delta < 10 ? delta.toFixed(1) : Math.round(delta)}
      </span>
    );
  }
  if (delta < 0) {
    return (
      <span className="inline-flex items-center gap-0.5 text-[10px] font-mono text-emerald-400">
        <ArrowDown className="w-3 h-3" /> {delta > -10 ? delta.toFixed(1) : Math.round(delta)}
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-0.5 text-[10px] font-mono text-slate-500">
      <Minus className="w-3 h-3" /> 0
    </span>
  );
}

function ScoreRing({ score, max }: { score: number; max: number }) {
  const pct = max > 0 ? Math.min(100, (score / max) * 100) : 0;
  const r = 28;
  const circ = 2 * Math.PI * r;
  const offset = circ - (pct / 100) * circ;
  const color = pct < 30 ? '#10b981' : pct < 60 ? '#f59e0b' : '#ef4444';
  return (
    <div className="relative w-[72px] h-[72px]">
      <svg className="w-full h-full -rotate-90" viewBox="0 0 72 72">
        <circle cx="36" cy="36" r={r} fill="none" stroke="#1e293b" strokeWidth="5" />
        <circle
          cx="36" cy="36" r={r} fill="none" stroke={color}
          strokeWidth="5" strokeDasharray={circ} strokeDashoffset={offset}
          strokeLinecap="round" className="transition-all duration-700"
        />
      </svg>
      <div className="absolute inset-0 flex items-center justify-center">
        <span className="text-sm font-bold font-mono text-slate-100">{formatScore(score)}</span>
      </div>
    </div>
  );
}

// ── stat card ────────────────────────────────────────────────────────────────

function StatTile({
  icon: Icon,
  label,
  value,
  accent,
  subtitle,
}: {
  icon: React.ElementType;
  label: string;
  value: string | number;
  accent: string;
  subtitle?: string;
}) {
  return (
    <div className="flex-1 min-w-[140px] p-3.5 bg-slate-900/60 rounded-xl group transition-all">
      <div className="flex items-center gap-1.5 text-slate-400 text-[11px] font-medium font-mono mb-1.5">
        <Icon className={cn('w-3.5 h-3.5', accent)} />
        {label}
      </div>
      <div className="text-xl font-bold text-slate-100 font-mono tracking-tight">{value}</div>
      {subtitle && <div className="mt-0.5 text-[10px] text-slate-500 font-mono">{subtitle}</div>}
    </div>
  );
}

// ── main panel ───────────────────────────────────────────────────────────────

export default function FrictionPanel({ bridge }: FrictionPanelProps) {
  const [stats, setStats] = useState<FrictionStats | null>(null);
  const [prevScore, setPrevScore] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchStats = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    try {
      const data = await bridge.getFrictionStats();
      if (stats) setPrevScore(stats.total_friction_score);
      setStats(data);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg.includes('not_found') ? 'Endpoint no disponible (kernel sin friction stats)' : msg);
    } finally {
      setLoading(false);
    }
  }, [bridge, stats]);

  usePolling('friction-stats', fetchStats, { interval: 'standard', enabled: !!bridge });

  const totalEventTypes =
    (stats?.manual_interventions ?? 0) +
    (stats?.routing_errors ?? 0) +
    (stats?.routing_ambiguous ?? 0) +
    (stats?.timeouts ?? 0) +
    (stats?.retries ?? 0) +
    (stats?.guild_errors ?? 0);

  return (
    <div className="space-y-6 font-sans">
      {/* Header */}
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 p-5 bg-slate-900/60 rounded-2xl">
        <div>
          <div className="flex items-center gap-2">
            <span className="px-2 py-0.5 text-[10px] font-medium bg-amber-500/10 text-amber-400 rounded-md">
              Friction Log
            </span>
          </div>
          <h2 className="text-xl font-bold tracking-tight text-slate-100 mt-2 flex items-center gap-2 font-mono">
            <Activity className="w-5 h-5 text-amber-400" />
            Friction Telemetry
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            System friction observability: routing errors, manual interventions, retries, and workflow round-trips.
          </p>
        </div>
        <button
          onClick={fetchStats}
          disabled={loading}
          className="flex items-center gap-2 px-3.5 py-2 bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 font-mono font-medium rounded-xl transition-all disabled:opacity-50"
        >
          <RefreshCw className={cn('w-3.5 h-3.5 text-amber-400', loading && 'animate-spin')} />
          <span>{loading ? 'Polling...' : 'Sync Stats'}</span>
        </button>
      </div>

      {error && (
        <div className="p-4 bg-amber-500/10 border border-amber-500/30 text-amber-400 rounded-xl text-xs font-mono">
          ⚠️ {error}
        </div>
      )}

      {/* Friction Score — hero tile */}
      <div className="flex items-center gap-6 p-5 bg-slate-900/60 rounded-2xl">
        <ScoreRing score={stats?.total_friction_score ?? 0} max={Math.max(10, stats?.total_friction_score ?? 10)} />
        <div className="flex-1 space-y-1">
          <div className="flex items-center gap-3">
            <span className="text-xs font-medium text-slate-400 font-mono">Total Friction Score</span>
            <TrendBadge current={stats?.total_friction_score ?? 0} previous={prevScore} />
          </div>
          <div className="text-3xl font-bold text-slate-100 font-mono">
            {formatScore(stats?.total_friction_score ?? 0)}
          </div>
          <p className="text-[11px] text-slate-500 font-mono">
            Compuesto: manual interventions, routing errors, timeouts, retries, ambiguous routes, guild errors, round-trips.
          </p>
        </div>
      </div>

      {/* Stat tiles — event type counters */}
      <div className="space-y-2">
        <h3 className="text-[11px] font-medium text-slate-400 font-mono flex items-center gap-2">
          <Gauge className="w-3.5 h-3.5 text-amber-400" />
          Event Counters
        </h3>
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3">
          <StatTile
            icon={AlertTriangle}
            label="Routing Errors"
            value={formatVal(stats?.routing_errors)}
            accent="text-red-400"
            subtitle="Wrong guild targeted"
          />
          <StatTile
            icon={Users}
            label="Manual Interventions"
            value={formatVal(stats?.manual_interventions)}
            accent="text-amber-400"
            subtitle="Agent specified guild manually"
          />
          <StatTile
            icon={Route}
            label="Ambiguous Routes"
            value={formatVal(stats?.routing_ambiguous)}
            accent="text-violet-400"
            subtitle="Semantic tiebreaker needed"
          />
          <StatTile
            icon={Clock}
            label="Timeouts"
            value={formatVal(stats?.timeouts)}
            accent="text-orange-400"
            subtitle="Commands timed out"
          />
          <StatTile
            icon={RotateCcw}
            label="Retries"
            value={formatVal(stats?.retries)}
            accent="text-blue-400"
            subtitle="Automatic retry attempts"
          />
          <StatTile
            icon={Zap}
            label="Guild Errors"
            value={formatVal(stats?.guild_errors)}
            accent="text-amber-400"
            subtitle="Guild execution failures"
          />
        </div>
      </div>

      {/* Workflow stats */}
      <div className="space-y-2">
        <h3 className="text-[11px] font-medium text-slate-400 font-mono flex items-center gap-2">
          <Activity className="w-3.5 h-3.5 text-amber-400" />
          Workflow Aggregates
        </h3>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
          <StatTile
            icon={Activity}
            label="Total Sessions"
            value={formatVal(stats?.total_sessions)}
            accent="text-slate-400"
            subtitle="Friction tracking sessions"
          />
          <StatTile
            icon={Activity}
            label="Total Workflows"
            value={formatVal(stats?.total_workflows)}
            accent="text-slate-400"
            subtitle="Tracked workflows"
          />
          <StatTile
            icon={Activity}
            label="Total Events"
            value={formatVal(totalEventTypes)}
            accent="text-slate-400"
            subtitle="Friction events logged"
          />
          <StatTile
            icon={Route}
            label="Avg Round-Trips"
            value={stats?.avg_round_trips_per_workflow != null ? stats.avg_round_trips_per_workflow.toFixed(1) : '\u2014'}
            accent="text-amber-400"
            subtitle="Per workflow"
          />
        </div>
      </div>

      {/* Coloquio roundtrips */}
      {((stats?.coloquio_roundtrips ?? 0) > 0) && (
        <div className="p-4 bg-slate-900/60 rounded-2xl flex items-center gap-4">
          <div className="w-10 h-10 rounded-xl bg-indigo-500/15 flex items-center justify-center">
            <Users className="w-5 h-5 text-indigo-400" />
          </div>
          <div>
            <div className="text-xs font-bold text-slate-200 font-mono">Coloquio Roundtrips</div>
            <div className="text-2xl font-bold text-slate-100 font-mono">{formatVal(stats?.coloquio_roundtrips)}</div>
            <div className="text-[10px] text-slate-500 font-mono">Multi-agent coordination cycles</div>
          </div>
        </div>
      )}
    </div>
  );
}
