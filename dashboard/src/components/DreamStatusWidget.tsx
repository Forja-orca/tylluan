import React, { useState, useEffect } from 'react';
import { Database, Clock } from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { usePolling } from '../hooks/usePolling';

interface DreamStatusWidgetProps {
  bridge: NexusBridge | null;
}

interface DreamStatusData {
  status: string;
  night_consolidation?: {
    next_run_in_secs?: number;
    runs_completed?: number;
    components?: string[];
  };
  graph?: {
    embedding_coverage?: number;
    nodes_with_embedding?: number;
    nodes?: number;
    topic_key_coverage?: number;
    nodes_with_topic_key?: number;
    orphan_pct?: number;
    orphans?: number;
    edges_by_type?: Record<string, number>;
  };
}

export function DreamStatusWidget({ bridge }: DreamStatusWidgetProps) {
  const [dreamStatus, setDreamStatus] = useState<DreamStatusData | null>(null);
  const [nextRunCountdown, setNextRunCountdown] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchDreamStatus = async () => {
      if (!bridge || document.visibilityState === 'hidden') return;
      try {
        const raw = (await bridge.fetchRaw('/api/v1/dream/status', {})) as DreamStatusData;
        if (raw && raw.status === 'ok') {
          setDreamStatus(raw);
          if (raw.night_consolidation && raw.night_consolidation.next_run_in_secs !== undefined) {
            setNextRunCountdown(raw.night_consolidation.next_run_in_secs);
          }
        }
      } catch (e) {
        console.error('Failed to fetch dream status:', e);
      } finally {
        setLoading(false);
      }
    };
    
    fetchDreamStatus();
  }, [bridge]);

  // Polling via centralized coordinator (replaces 2 scattered setInterval calls)
  usePolling('dream-status', async () => {
    if (!bridge) return;
    try {
      const raw = (await bridge.fetchRaw('/api/v1/dream/status', {})) as DreamStatusData;
      if (raw && raw.status === 'ok') {
        setDreamStatus(raw);
        if (raw.night_consolidation?.next_run_in_secs !== undefined) {
          setNextRunCountdown(raw.night_consolidation.next_run_in_secs);
        }
      }
    } catch {}
  }, { interval: 'slow', enabled: !!bridge });
  usePolling('dream-countdown', () => {
    setNextRunCountdown(prev => (prev !== null && prev > 0 ? prev - 1 : prev));
  }, { interval: 'fast', enabled: true });

  const formatCountdown = (seconds: number | null) => {
    if (seconds === null) return '--:--';
    if (seconds <= 0) return 'Running...';
    const hrs = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    const secs = seconds % 60;
    if (hrs > 0) return `${hrs}h ${mins}m ${secs}s`;
    if (mins > 0) return `${mins}m ${secs}s`;
    return `${secs}s`;
  };

  if (loading && !dreamStatus) {
    return (
      <div className="p-4 rounded border border-border bg-card animate-pulse min-h-[280px] flex flex-col justify-between">
        <div className="flex items-center justify-between mb-4">
          <div className="h-4 w-32 bg-muted rounded" />
          <div className="h-4 w-12 bg-muted rounded" />
        </div>
        <div className="h-10 bg-muted rounded mb-4" />
        <div className="h-6 bg-muted rounded mb-4" />
        <div className="h-20 bg-muted rounded" />
      </div>
    );
  }

  return (
    <div className="p-4 rounded border border-border bg-card flex flex-col justify-between h-full font-sans">
      <div>
        <div className="flex items-center justify-between mb-4">
          <div className="text-xs font-bold text-foreground">
            Consolidación Nocturna
          </div>
          <span className="text-[11px] text-primary font-mono bg-primary/10 px-2 py-0.5 rounded border border-primary/30">
            CICLOS: {dreamStatus?.night_consolidation?.runs_completed ?? 0}
          </span>
        </div>

        {dreamStatus ? (
          <div className="space-y-4">
            {/* Countdown / Status */}
            <div className="flex justify-between items-center bg-muted/40 p-2.5 rounded border border-border">
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground font-mono">
                <Clock className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" /> Próxima ejecución
              </div>
              <span className="text-xs font-bold text-foreground font-mono">
                {formatCountdown(nextRunCountdown)}
              </span>
            </div>

            {/* Embedding Coverage */}
            <div className="flex justify-between items-center text-xs">
              <span className="text-muted-foreground font-mono flex items-center gap-1">
                <Database className="w-3 h-3 text-muted-foreground" aria-hidden="true" /> Cobertura Embeddings
              </span>
              <span className="text-foreground font-mono">
                {dreamStatus.graph?.embedding_coverage ?? 0}% ({dreamStatus.graph?.nodes_with_embedding ?? 0} / {dreamStatus.graph?.nodes ?? 0})
              </span>
            </div>
            {(dreamStatus.graph?.embedding_coverage ?? 0) > 50 ? (
              <div className="h-2 w-full bg-muted rounded overflow-hidden border border-border">
                <div className="h-full bg-emerald-600 rounded transition-all duration-500 ease-out" style={{ width: `${dreamStatus.graph?.embedding_coverage ?? 0}%` }} />
              </div>
            ) : (
              <div className="h-2 w-full bg-muted rounded overflow-hidden border border-border">
                <div className="h-full bg-amber-600 rounded transition-all duration-500 ease-out" style={{ width: `${dreamStatus.graph?.embedding_coverage ?? 0}%` }} />
              </div>
            )}

            {/* Topic-key Coverage (legacy) */}
            {dreamStatus.graph?.topic_key_coverage !== undefined && (
              <div className="flex justify-between items-center text-[11px] pr-1">
                <span className="text-muted-foreground font-mono flex items-center gap-1 text-[11px]">
                  <Database className="w-2.5 h-2.5 text-muted-foreground" aria-hidden="true" /> topic_key
                </span>
                <span className="text-muted-foreground font-mono text-[11px]">
                  {dreamStatus.graph?.topic_key_coverage ?? 0}% ({dreamStatus.graph?.nodes_with_topic_key ?? 0})
                </span>
              </div>
            )}

            {/* Orphans Progress Bar */}
            <div className="space-y-1.5">
              <div className="flex justify-between items-center text-xs">
                <span className="text-muted-foreground font-mono flex items-center gap-1">
                  <Database className="w-3 h-3 text-muted-foreground" aria-hidden="true" /> Nodos huérfanos
                </span>
                <span className="text-foreground font-mono">
                  {dreamStatus.graph?.orphan_pct ?? 0}% ({dreamStatus.graph?.orphans ?? 0} / {dreamStatus.graph?.nodes ?? 0})
                </span>
              </div>
              <div className="h-2 w-full bg-muted rounded overflow-hidden border border-border">
                <div 
                  className="h-full bg-primary rounded transition-all duration-500 ease-out" 
                  style={{ width: `${dreamStatus.graph?.orphan_pct ?? 0}%` }} 
                />
              </div>
            </div>

            {/* Active Components list */}
            {dreamStatus.night_consolidation?.components && (
              <div className="space-y-1.5">
                <div className="text-xs font-bold text-foreground">
                  Flujo de Procesamiento Cognitivo
                </div>
                <div className="flex flex-wrap gap-1">
                  {dreamStatus.night_consolidation.components.map((c: string) => {
                    const name = c.split(' ')[0];
                    return (
                      <span 
                        key={name} 
                        title={c}
                        className="text-[11px] font-semibold font-mono bg-muted text-foreground px-1.5 py-0.5 rounded border border-border cursor-help"
                      >
                        {name}
                      </span>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Edges by Type Compact Table */}
            <div className="space-y-1.5">
              <div className="text-xs font-bold text-foreground">
                Aristas por Tipo
              </div>
              <div
                tabIndex={0}
                role="region"
                aria-label="Grafo de consolidación: aristas por tipo"
                className="max-h-24 overflow-y-auto space-y-1 pr-1 font-mono text-[11px] border border-border rounded p-2 bg-muted/40 focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
              >
                {Object.entries(dreamStatus.graph?.edges_by_type ?? {}).length > 0 ? (
                  Object.entries(dreamStatus.graph?.edges_by_type ?? {}).map(([type, count]) => (
                    <div key={type} className="flex justify-between items-center py-0.5 border-b border-border/60 last:border-b-0 text-foreground">
                      <span className="truncate mr-2 text-muted-foreground text-[11px]">{type}</span>
                      <span className="text-foreground font-bold font-mono">{count as number}</span>
                    </div>
                  ))
                ) : (
                  <div className="text-muted-foreground italic text-[11px] text-center py-2">Sin aristas registradas</div>
                )}
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
