import React, { useState, useEffect, useCallback } from 'react';
import { ShieldAlert, RefreshCw, Gauge, Ban, TrendingDown, Cpu, Activity, CheckCircle2, Lock } from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';
import type { CoherenceGateStats, RecallFeedbackStats } from '../lib/api/security';

export interface CoherenceGatePanelProps {
  bridge: NexusBridge | null;
}

const POLL_INTERVAL_MS = 15_000;

export function calculateProgressPct(resolved: number, threshold: number): number {
  if (!threshold || threshold <= 0) return 0;
  return Math.min(100, Math.max(0, (resolved / threshold) * 100));
}

export function formatStatValue(val: number | undefined): string {
  if (val === undefined || val === null) return '—';
  return val.toLocaleString();
}

function TacticalStatCard({
  icon: Icon,
  label,
  value,
  accentColor,
  subtitle,
}: {
  icon: any;
  label: string;
  value: string | number;
  accentColor: string;
  subtitle?: string;
}) {
  return (
    <div className="flex-1 min-w-[160px] p-4 bg-slate-900/60 rounded-xl relative overflow-hidden group hover:bg-slate-900/80 transition-all">
      <div className="flex items-center justify-between text-slate-400 text-[11px] font-medium mb-2">
        <span className="flex items-center gap-1.5">
          <Icon className={cn('w-3.5 h-3.5', accentColor)} />
          {label}
        </span>
      </div>
      <div className="text-2xl font-bold text-slate-100 font-mono tracking-tight">{value}</div>
      {subtitle && <div className="mt-1 text-[10px] text-slate-500">{subtitle}</div>}
    </div>
  );
}

export default function CoherenceGatePanel({ bridge }: CoherenceGatePanelProps) {
  const [gate, setGate] = useState<CoherenceGateStats | null>(null);
  const [signal, setSignal] = useState<RecallFeedbackStats | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchStats = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    try {
      const [gateStats, signalStats] = await Promise.all([
        bridge.getCoherenceGateStats(),
        bridge.getRecallFeedbackStats(),
      ]);
      setGate(gateStats);
      setSignal(signalStats);
    } catch (err: any) {
      setError(err.message || 'Failed to reach Tylluan Kernel on :4000');
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => {
    fetchStats();
  }, [fetchStats]);

  // Polling via centralized coordinator (replaces 1 scattered setInterval)
  usePolling('coherence-stats', fetchStats, { interval: 'slow', enabled: !!bridge });

  const resolvedCount = signal?.resolved ?? 0;
  const threshold = signal?.threshold ?? 5000;
  const progressPct = calculateProgressPct(resolvedCount, threshold);

  return (
    <div className="space-y-6 font-sans">
      {/* Header */}
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 p-5 bg-slate-900/60 rounded-2xl">
        <div>
          <div className="flex items-center gap-2">
            <span className="px-2 py-0.5 text-[10px] font-semibold bg-amber-500/10 text-amber-400 rounded-md">
              ADR-011 Active
            </span>
            <span className="px-2 py-0.5 text-[10px] font-semibold bg-rose-500/10 text-rose-400 rounded-md">
              3-Layer Defense
            </span>
          </div>
          <h2 className="text-xl font-semibold tracking-tight text-slate-100 mt-2 flex items-center gap-2">
            <ShieldAlert className="w-5 h-5 text-amber-400" />
            Coherence Gate &amp; Signal Loop Telemetry
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Sovereign recall-path memory poisoning defense and implicit Jaccard utility feedback loop.
          </p>
        </div>
        <button
          onClick={fetchStats}
          disabled={loading}
          className="flex items-center gap-2 px-3.5 py-2 bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 font-medium rounded-xl transition-all disabled:opacity-50"
        >
          <RefreshCw className={cn('w-3.5 h-3.5 text-amber-400', loading && 'animate-spin')} />
          <span>{loading ? 'Polling...' : 'Sync Stats'}</span>
        </button>
      </div>

      {error && (
        <div className="p-4 bg-rose-500/10 border border-rose-500/20 text-rose-400 rounded-xl text-xs">
          ⚠️ {error}
        </div>
      )}

      {/* Coherence Gate Telemetry */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-[11px] font-medium text-slate-400 flex items-center gap-2">
            <Lock className="w-3.5 h-3.5 text-amber-400" />
            Coherence Gate — Live Memory Protection
          </h3>
          <span className="text-[10px] text-slate-500">Live Session Counters</span>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          <TacticalStatCard
            icon={Gauge}
            label="Nodes Evaluated"
            value={formatStatValue(gate?.total_seen)}
            accentColor="text-slate-400"
            subtitle="Total candidate nodes processed"
          />
          <TacticalStatCard
            icon={Ban}
            label="Layer 1 Eliminated"
            value={formatStatValue(gate?.total_eliminated)}
            accentColor="text-[#FF2E93]"
            subtitle="Prompt injection patterns blocked"
          />
          <TacticalStatCard
            icon={TrendingDown}
            label="Layer 2/3 Penalized"
            value={formatStatValue(gate?.total_penalized)}
            accentColor="text-amber-400"
            subtitle="Provenance & drift penalties"
          />
        </div>

        {/* 3-Layer Architecture Cards */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mt-3">
          <div className="p-3.5 bg-slate-900/40 rounded-xl">
            <div className="flex items-center gap-2 text-xs font-semibold text-slate-200 mb-1">
              <span className="w-2 h-2 rounded-full bg-rose-400" />
              Layer 1: Pattern Interception
            </div>
            <p className="text-[11px] text-slate-400">
              Eliminates known prompt injection signatures before memory is passed to LLM context.
            </p>
          </div>
          <div className="p-3.5 bg-slate-900/40 rounded-xl">
            <div className="flex items-center gap-2 text-xs font-semibold text-slate-200 mb-1">
              <span className="w-2 h-2 rounded-full bg-amber-400" />
              Layer 2: Trust Provenance
            </div>
            <p className="text-[11px] text-slate-400">
              Applies rank penalty to unverified federation peers and untrusted external sources.
            </p>
          </div>
          <div className="p-3.5 bg-slate-900/40 rounded-xl">
            <div className="flex items-center gap-2 text-xs font-semibold text-slate-200 mb-1">
              <span className="w-2 h-2 rounded-full bg-indigo-400" />
              Layer 3: Semantic Drift
            </div>
            <p className="text-[11px] text-slate-400">
              Penalizes candidates whose cosine distance diverges significantly from prompt query.
            </p>
          </div>
        </div>
      </div>

      {/* Signal Loop Training Progress */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-[11px] font-medium text-slate-400 flex items-center gap-2">
            <Activity className="w-3.5 h-3.5 text-amber-400" />
            Signal Loop — LightReranker FFN Progress (ADR-011 §3)
          </h3>
          <span className="text-[10px] text-slate-500">Threshold: 5,000 Resolved Rows</span>
        </div>

        <div className="p-5 bg-slate-900/60 rounded-2xl space-y-4">
          <div className="flex justify-between items-baseline">
            <div className="space-y-1">
              <div className="text-xs text-slate-400">
                <span className="text-slate-100 font-bold text-sm">{formatStatValue(signal?.resolved)}</span> / {formatStatValue(threshold)} resolved rows
              </div>
              <div className="text-[11px] text-slate-500">
                Pending resolution: <span className="text-slate-300 font-semibold">{formatStatValue(signal?.pending)}</span>
              </div>
            </div>
            <div className="text-right">
              <div className="text-xl font-bold text-amber-400">{progressPct.toFixed(2)}%</div>
              <div className="text-[10px] text-slate-500 font-medium">
                {resolvedCount >= threshold ? 'Ready to Train' : 'Accumulating Signal'}
              </div>
            </div>
          </div>

          {/* Progress Bar */}
          <div className="w-full h-2.5 bg-slate-800 rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-amber-600 to-amber-400 transition-all duration-500"
              style={{ width: `${Math.max(2, progressPct)}%` }}
            />
          </div>

          <div className="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-2 pt-2 border-t border-slate-800/60 text-[11px] text-slate-400">
            <div className="flex items-center gap-1.5">
              <Cpu className="w-3.5 h-3.5 text-slate-400" />
              <span>Learned LightReranker (FFN 4→16→1):</span>
              <span className="text-amber-400 font-semibold">
                {resolvedCount >= threshold ? 'Cutover Ready' : 'Standby (Hybrid RRF Active)'}
              </span>
            </div>
            <div className="flex items-center gap-1.5 text-slate-500">
              <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
              <span>Jaccard Resolution Phase active in NightConsolidation</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
