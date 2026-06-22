import React, { useState, useEffect } from 'react';
import { Moon, Database, Clock, RefreshCw, Layers } from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';

interface DreamStatusWidgetProps {
  bridge: NexusBridge | null;
}

export function DreamStatusWidget({ bridge }: DreamStatusWidgetProps) {
  const [dreamStatus, setDreamStatus] = useState<any | null>(null);
  const [nextRunCountdown, setNextRunCountdown] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchDreamStatus = async () => {
      if (!bridge || document.visibilityState === 'hidden') return;
      try {
        const raw = await bridge.fetchRaw('/api/v1/dream/status', {}) as any;
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
    // Fetch every 30 seconds as specified in prompt (no real-time to avoid unnecessary load)
    const id = setInterval(fetchDreamStatus, 30000);
    return () => clearInterval(id);
  }, [bridge]);

  // Local 1-second countdown timer
  useEffect(() => {
    const timer = setInterval(() => {
      setNextRunCountdown(prev => (prev !== null && prev > 0 ? prev - 1 : prev));
    }, 1000);
    return () => clearInterval(timer);
  }, []);

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
      <div className="p-6 rounded-xl border border-slate-800 bg-slate-900/50 animate-pulse min-h-[280px] flex flex-col justify-between">
        <div className="flex items-center justify-between mb-4">
          <div className="h-4 w-32 bg-slate-800 rounded" />
          <div className="h-4 w-12 bg-slate-800 rounded" />
        </div>
        <div className="h-10 bg-slate-800 rounded mb-4" />
        <div className="h-6 bg-slate-800 rounded mb-4" />
        <div className="h-20 bg-slate-800 rounded" />
      </div>
    );
  }

  return (
    <div className="p-4 rounded-xl border border-slate-800 bg-slate-900/50 flex flex-col justify-between h-full transition-all hover:border-indigo-500/30">
      <div>
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-2 text-slate-400 text-[10px] uppercase tracking-widest font-bold">
            <Moon className="w-3.5 h-3.5 text-indigo-400 animate-pulse" /> Night Consolidation
          </div>
          <span className="text-[10px] text-indigo-400 font-mono bg-indigo-500/10 px-2 py-0.5 rounded-full border border-indigo-500/20">
            RUNS: {dreamStatus?.night_consolidation?.runs_completed ?? 0}
          </span>
        </div>

        {dreamStatus ? (
          <div className="space-y-4">
            {/* Countdown / Status */}
            <div className="flex justify-between items-center bg-slate-950/60 p-2.5 rounded-lg border border-slate-800/60 shadow-inner">
              <div className="flex items-center gap-1.5 text-[10px] text-slate-400 uppercase tracking-wider font-mono">
                <Clock className="w-3 h-3 text-indigo-400/80" /> Next run in
              </div>
              <span className="text-xs font-black text-indigo-300 font-mono">
                {formatCountdown(nextRunCountdown)}
              </span>
            </div>

            {/* Embedding Coverage */}
            <div className="flex justify-between items-center text-[10px]">
              <span className="text-slate-400 font-mono flex items-center gap-1">
                <Database className="w-3 h-3 text-slate-500" /> Embeddings
              </span>
              <span className="text-slate-300 font-mono">
                {dreamStatus.graph?.embedding_coverage ?? 0}% ({dreamStatus.graph?.nodes_with_embedding ?? 0} / {dreamStatus.graph?.nodes ?? 0})
              </span>
            </div>
            {dreamStatus.graph?.embedding_coverage > 50 ? (
              <div className="h-1.5 w-full bg-slate-950/80 rounded-full overflow-hidden p-0.5 border border-slate-800/40">
                <div className="h-full bg-emerald-500 rounded-full transition-all duration-700 ease-out shadow-[0_0_6px_rgba(52,211,153,0.4)]" style={{ width: `${dreamStatus.graph?.embedding_coverage ?? 0}%` }} />
              </div>
            ) : (
              <div className="h-1.5 w-full bg-slate-950/80 rounded-full overflow-hidden p-0.5 border border-slate-800/40">
                <div className="h-full bg-amber-500 rounded-full transition-all duration-700 ease-out shadow-[0_0_6px_rgba(245,158,11,0.4)]" style={{ width: `${dreamStatus.graph?.embedding_coverage ?? 0}%` }} />
              </div>
            )}

            {/* Topic-key Coverage (legacy) */}
            {dreamStatus.graph?.topic_key_coverage !== undefined && (
              <div className="flex justify-between items-center text-[10px] pr-1">
                <span className="text-slate-500 font-mono flex items-center gap-1 text-[8px]">
                  <Database className="w-2 h-2 text-slate-600" /> topic_key
                </span>
                <span className="text-slate-500 font-mono text-[8px]">
                  {dreamStatus.graph?.topic_key_coverage ?? 0}% ({dreamStatus.graph?.nodes_with_topic_key ?? 0})
                </span>
              </div>
            )}

            {/* Orphans Progress Bar */}
            <div className="space-y-1.5">
              <div className="flex justify-between items-center text-[10px]">
                <span className="text-slate-400 font-mono flex items-center gap-1">
                  <Database className="w-3 h-3 text-slate-500" /> Orphans
                </span>
                <span className="text-slate-300 font-mono">
                  {dreamStatus.graph?.orphan_pct ?? 0}% ({dreamStatus.graph?.orphans ?? 0} / {dreamStatus.graph?.nodes ?? 0})
                </span>
              </div>
              <div className="h-2 w-full bg-slate-950/80 rounded-full overflow-hidden p-0.5 border border-slate-800/40">
                <div 
                  className="h-full bg-indigo-500 rounded-full transition-all duration-700 ease-out shadow-[0_0_8px_rgba(99,102,241,0.5)]" 
                  style={{ width: `${dreamStatus.graph?.orphan_pct ?? 0}%` }} 
                />
              </div>
            </div>

            {/* Active Components list */}
            {dreamStatus.night_consolidation?.components && (
              <div className="space-y-1.5">
                <div className="flex items-center gap-1 text-[9px] text-slate-500 uppercase tracking-widest font-bold">
                  <Layers className="w-3 h-3 text-slate-500" /> Cognitive Process Flow
                </div>
                <div className="flex flex-wrap gap-1">
                  {dreamStatus.night_consolidation.components.map((c: string) => {
                    const name = c.split(' ')[0];
                    return (
                      <span 
                        key={name} 
                        title={c}
                        className="text-[8px] font-semibold font-mono bg-slate-950/45 hover:bg-slate-800/40 text-slate-400 px-1.5 py-0.5 rounded border border-slate-850 hover:text-slate-300 transition-colors cursor-help"
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
              <div className="text-[9px] text-slate-500 uppercase tracking-widest font-bold">
                Edges by Type
              </div>
              <div className="max-h-24 overflow-y-auto space-y-1 pr-1 font-mono text-[10px] border border-slate-850 rounded-lg p-2 bg-slate-950/40">
                {Object.entries(dreamStatus.graph?.edges_by_type ?? {}).length > 0 ? (
                  Object.entries(dreamStatus.graph.edges_by_type).map(([type, count]) => (
                    <div key={type} className="flex justify-between items-center py-0.5 border-b border-slate-900 last:border-b-0 text-slate-300">
                      <span className="truncate mr-2 text-slate-400 text-[9px]">{type}</span>
                      <span className="text-slate-200 font-bold font-mono">{count as number}</span>
                    </div>
                  ))
                ) : (
                  <div className="text-slate-650 italic text-[10px] text-center py-2">No edges present</div>
                )}
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
