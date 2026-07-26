import React, { useState, useEffect, useCallback } from 'react';
import { ShieldAlert, RefreshCw, Gauge, Ban, TrendingDown } from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import type { CoherenceGateStats, RecallFeedbackStats } from '../lib/api/security';

export interface CoherenceGatePanelProps {
  bridge: NexusBridge | null;
}

const POLL_INTERVAL_MS = 30_000;

function StatCard({ icon: Icon, label, value, accent }: { icon: any; label: string; value: string | number; accent: string }) {
  return (
    <div className="flex-1 min-w-[140px] p-4 bg-slate-900/60 border border-slate-850 rounded-2xl">
      <div className="flex items-center gap-2 text-slate-400 text-[10px] uppercase font-bold tracking-wider font-mono mb-2">
        <Icon className={cn('w-3.5 h-3.5', accent)} />
        {label}
      </div>
      <div className="text-2xl font-bold text-slate-100 font-mono">{value}</div>
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
      setError(err.message || 'Failed to reach kernel');
    } finally {
      setLoading(false);
    }
  }, [bridge]);

  useEffect(() => {
    fetchStats();
    const interval = setInterval(fetchStats, POLL_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [fetchStats]);

  return (
    <div className="space-y-6">
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-slate-50 flex items-center gap-2">
            <ShieldAlert className="w-5 h-5 text-amber-500" />
            Coherence Gate &amp; Signal Loop (ADR-011)
          </h2>
          <p className="text-xs text-slate-400">
            Recall-path memory poisoning defense and the implicit feedback loop feeding the future learned reranker.
          </p>
        </div>
        <button
          onClick={fetchStats}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-900 border border-slate-800 hover:border-slate-700 text-xs text-slate-300 font-medium rounded-xl transition-all disabled:opacity-50"
        >
          <RefreshCw className={cn('w-3.5 h-3.5', loading && 'animate-spin')} />
          Refresh
        </button>
      </div>

      {error && (
        <div className="p-3 bg-red-500/10 border border-red-500/20 text-red-400 rounded-2xl text-xs font-mono">
          {error}
        </div>
      )}

      <div>
        <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider font-mono mb-3">
          Coherence Gate — since last kernel start
        </h3>
        <div className="flex flex-wrap gap-3">
          <StatCard icon={Gauge} label="Nodes Seen" value={gate?.total_seen ?? '—'} accent="text-slate-400" />
          <StatCard icon={Ban} label="Eliminated (Layer 1)" value={gate?.total_eliminated ?? '—'} accent="text-red-400" />
          <StatCard icon={TrendingDown} label="Penalized (Layer 2/3)" value={gate?.total_penalized ?? '—'} accent="text-amber-400" />
        </div>
        <p className="mt-2 text-[10px] text-slate-500 font-mono">
          {gate?.note ?? 'Counters reset on kernel restart — this is live observability, not a persisted historical log.'}
        </p>
      </div>

      <div>
        <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider font-mono mb-3">
          Signal Loop — LightReranker training progress
        </h3>
        <div className="p-4 bg-slate-900/60 border border-slate-850 rounded-2xl">
          <div className="flex justify-between items-baseline mb-2">
            <span className="text-xs text-slate-400 font-mono">
              {signal?.resolved ?? 0} / {signal?.threshold ?? 5000} resolved rows
            </span>
            <span className="text-xs text-slate-300 font-mono font-bold">
              {(signal?.pct ?? 0).toFixed(2)}%
            </span>
          </div>
          <div className="w-full h-2 bg-slate-950 rounded-full overflow-hidden border border-slate-800">
            <div
              className="h-full bg-emerald-500 transition-all"
              style={{ width: `${Math.min(signal?.pct ?? 0, 100)}%` }}
            />
          </div>
          <p className="mt-3 text-[10px] text-slate-500 font-mono">
            {signal && signal.resolved === 0
              ? 'No usable signal yet — needs real recall traffic in production before the reranker can train (ADR-011 §2, Fase 3).'
              : `${signal?.pending ?? 0} rows still pending resolution.`}
          </p>
        </div>
      </div>
    </div>
  );
}
