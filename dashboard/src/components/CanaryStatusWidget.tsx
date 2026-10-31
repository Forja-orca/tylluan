import React, { useState, useEffect, useCallback } from 'react';
import { CheckCircle2, XCircle, RefreshCw } from 'lucide-react';
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

  const fetchCanary = useCallback(async () => {
    if (!bridge || document.visibilityState === 'hidden') return;
    setRefreshing(true);
    try {
      const raw = await bridge.fetchRaw('/api/v1/canary', {}) as CanaryData;
      if (raw && raw.status) {
        setCanary(raw);
      }
    } catch (e: unknown) {
      console.error('Failed to fetch canary status:', e);
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, [bridge]);

  useEffect(() => {
    fetchCanary();
  }, [fetchCanary]);

  // Polling via centralized coordinator (replaces 1 scattered setInterval)
  usePolling('canary-status', fetchCanary, { interval: 'slow', enabled: !!bridge });

  if (loading && !canary) {
    return (
      <div className="p-4 rounded border border-border bg-card animate-pulse min-h-[280px] flex flex-col justify-between font-sans">
        <div className="flex items-center justify-between mb-4">
          <div className="h-4 w-32 bg-muted rounded" />
          <div className="h-4 w-12 bg-muted rounded" />
        </div>
        <div className="h-12 bg-muted rounded mb-4" />
        <div className="h-24 bg-muted rounded" />
      </div>
    );
  }

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'healthy':
        return 'text-emerald-600 border-emerald-500/30 bg-emerald-500/10';
      case 'degraded':
        return 'text-amber-600 border-amber-500/30 bg-amber-500/10';
      case 'critical':
      default:
        return 'text-red-600 border-red-500/30 bg-red-500/10';
    }
  };

  const getStatusText = (status: string) => {
    switch (status) {
      case 'healthy':
        return 'SISTEMA SEGURO';
      case 'degraded':
        return 'DEGRADADO';
      case 'critical':
      default:
        return 'ALERTA CRÍTICA';
    }
  };

  return (
    <div className="p-4 rounded border border-border bg-card flex flex-col justify-between h-full font-sans">
      <div>
        <div className="flex items-center justify-between mb-3">
          <div className="text-xs font-bold text-foreground">
            Afirmaciones Canario
          </div>
          <button 
            onClick={fetchCanary}
            disabled={refreshing}
            aria-label="Actualizar afirmaciones canario"
            className="p-1 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            <RefreshCw className={cn("w-3.5 h-3.5", refreshing && "animate-spin")} />
          </button>
        </div>

        {canary ? (
          <div className="space-y-3">
            {/* Health Score Overview */}
            <div className="flex items-center justify-between bg-muted/40 p-2.5 rounded border border-border">
              <div className="flex flex-col">
                <span className="text-[11px] text-muted-foreground font-mono">Puntuación de Aserción</span>
                <span className="text-xl font-bold text-foreground font-mono">
                  {canary.score.toFixed(0)}%
                </span>
              </div>
              <span className={cn("text-[11px] font-mono font-semibold px-2 py-0.5 rounded border", getStatusColor(canary.status))}>
                {getStatusText(canary.status)}
              </span>
            </div>

            {/* Invariants Probe List */}
            <div className="space-y-1">
              <div className="text-xs font-bold text-foreground mb-1">
                Invariantes de Seguridad
              </div>
              <div 
                tabIndex={0}
                role="region"
                aria-label="Lista de sondas de seguridad"
                className="max-h-[170px] overflow-y-auto space-y-1.5 rounded p-2 bg-muted/30 border border-border focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
              >
                {canary.probes.map((probe) => (
                  <div key={probe.name} className="flex items-start gap-2 py-1 border-b border-border/60 last:border-b-0">
                    {probe.pass ? (
                      <CheckCircle2 className="w-3.5 h-3.5 text-emerald-600 shrink-0 mt-0.5" />
                    ) : (
                      <XCircle className="w-3.5 h-3.5 text-red-600 shrink-0 mt-0.5" />
                    )}
                    <div className="flex-1 min-w-0">
                      <div className="flex justify-between items-center text-[11px] font-mono text-muted-foreground font-medium">
                        <span className="truncate">{probe.name}</span>
                        <span className={probe.pass ? "text-emerald-600 font-bold" : "text-red-600 font-bold"}>
                          {probe.pass ? "PASS" : "FAIL"}
                        </span>
                      </div>
                      <p className="text-[11px] text-muted-foreground truncate font-mono mt-0.5" title={probe.detail}>
                        {probe.detail}
                      </p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        ) : (
          <div className="text-xs text-muted-foreground text-center py-4">Sin datos recibidos.</div>
        )}
      </div>
    </div>
  );
}
