import React, { useState, useEffect, useCallback } from 'react';
import { ShieldCheck, CheckCircle2, XCircle, AlertCircle, RefreshCw } from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';

interface CanaryStatusWidgetProps {
  bridge: NexusBridge | null;
}

interface CanaryProbe {
  name: string;
  pass: boolean;
  detail: string;
}

interface CanaryData {
  status: 'healthy' | 'degraded' | 'critical' | string;
  score: number;
  passed: number;
  total: number;
  probes: CanaryProbe[];
  version?: string;
}

export function CanaryStatusWidget({ bridge }: CanaryStatusWidgetProps) {
  const [canary, setCanary] = useState<CanaryData | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);

  const fetchCanary = async () => {
    if (!bridge || document.visibilityState === 'hidden') return;
    setRefreshing(true);
    try {
      const raw = await bridge.fetchRaw('/api/v1/canary', {}) as CanaryData;
      if (raw && raw.status) {
        setCanary(raw);
      }
    } catch (e) {
      console.error('Failed to fetch canary status:', e);
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  useEffect(() => {
    fetchCanary();
  }, [bridge]);

  // Polling via centralized coordinator (replaces 1 scattered setInterval)
  usePolling('canary-status', fetchCanary, { interval: 'slow', enabled: !!bridge });

  if (loading && !canary) {
    return (
      <div className="p-4 rounded-xl bg-slate-900/50 animate-pulse min-h-[280px] flex flex-col justify-between">
        <div className="flex items-center justify-between mb-4">
          <div className="h-4 w-32 bg-slate-800 rounded" />
          <div className="h-4 w-12 bg-slate-800 rounded" />
        </div>
        <div className="h-12 bg-slate-800 rounded mb-4" />
        <div className="h-24 bg-slate-800 rounded" />
      </div>
    );
  }

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'healthy':
        return 'text-emerald-400 border-emerald-500/20 bg-emerald-500/10';
      case 'degraded':
        return 'text-amber-400 border-amber-500/20 bg-amber-500/10';
      case 'critical':
      default:
        return 'text-rose-400 border-rose-500/20 bg-rose-500/10';
    }
  };

  const getStatusText = (status: string) => {
    switch (status) {
      case 'healthy':
        return 'SYSTEM SECURE';
      case 'degraded':
        return 'DEGRADED';
      case 'critical':
      default:
        return 'CRITICAL ALERT';
    }
  };

  return (
    <div className="p-4 rounded-xl bg-slate-900/50 flex flex-col justify-between h-full transition-all hover:bg-slate-900/70">
      <div>
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2 text-slate-400 text-[11px] font-medium">
            <ShieldCheck className="w-3.5 h-3.5 text-emerald-400 animate-pulse" /> Canary Assertions
          </div>
          <button 
            onClick={fetchCanary}
            disabled={refreshing}
            className="p-1 hover:bg-slate-800 rounded text-slate-500 hover:text-slate-300 transition-colors disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background focus-visible:outline-none"
          >
            <RefreshCw className={cn("w-3 h-3", refreshing && "animate-spin")} />
          </button>
        </div>

        {canary ? (
          <div className="space-y-3">
            {/* Health Score Overview */}
            <div className="flex items-center justify-between bg-slate-950/60 p-2.5 rounded-lg">
              <div className="flex flex-col">
                <span className="text-[10px] text-slate-500 font-mono">Assertion Score</span>
                <span className="text-xl font-bold text-slate-200 font-mono">
                  {canary.score.toFixed(0)}%
                </span>
              </div>
              <span className={cn("text-[10px] font-mono font-semibold px-2 py-0.5 rounded-md", getStatusColor(canary.status))}>
                {getStatusText(canary.status)}
              </span>
            </div>

            {/* Invariants Probe List */}
            <div className="space-y-1">
              <div className="text-[10px] text-slate-500 font-medium mb-1">
                Security Invariants
              </div>
              <div className="max-h-[170px] overflow-y-auto space-y-1.5 rounded-lg p-2 bg-slate-950/40">
                {canary.probes.map((probe) => (
                  <div key={probe.name} className="flex items-start gap-2 py-1 border-b border-slate-900 last:border-b-0">
                    {probe.pass ? (
                      <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 shrink-0 mt-0.5" />
                    ) : (
                      <XCircle className="w-3.5 h-3.5 text-rose-500 shrink-0 mt-0.5" />
                    )}
                    <div className="flex-1 min-w-0">
                      <div className="flex justify-between items-center text-[10px] font-mono text-slate-400 font-medium">
                        <span className="truncate">{probe.name}</span>
                        <span className={probe.pass ? "text-emerald-500" : "text-rose-500"}>
                          {probe.pass ? "PASS" : "FAIL"}
                        </span>
                      </div>
                      <p className="text-[10px] text-slate-500 truncate font-mono mt-0.5" title={probe.detail}>
                        {probe.detail}
                      </p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        ) : (
          <div className="text-[10px] text-slate-500 text-center py-4">No data received.</div>
        )}
      </div>
    </div>
  );
}
