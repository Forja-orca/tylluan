import React, { useState, useEffect } from 'react';
import type { NexusBridge, AutoResearchSummary } from '../lib/nexus-bridge';
import { 
  Beaker, Play, Square, GitBranch, GitCommit, TrendingUp, Zap, Clock, ShieldCheck, 
  Cpu, RotateCcw, AlertTriangle, ArrowRight, Settings
} from 'lucide-react';
import { cn } from '../lib/utils';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

export function LaboratoryTab({ bridge, notify }: Props) {
  const [loading, setLoading] = useState(false);
  const [data, setData] = useState<AutoResearchSummary>({
    status: "Idle",
    current_mutation: null,
    progress: {
      current_step: 0,
      total_steps: 100,
      last_improvement_at: 0
    },
    metrics: {
      baseline: { recall_1: 0.65, recall_5: 0.90, latency_ms: 202.0 },
      current: { recall_1: 0.65, recall_5: 0.90, latency_ms: 202.0 }
    },
    lineage: [
      {
        step: 1,
        target: "retrolink_orphans_score",
        val: 0.22,
        recall_1: 0.67,
        status: "Committed"
      },
      {
        step: 2,
        target: "Jina_threshold",
        val: 0.75,
        recall_1: 0.65,
        status: "Reverted"
      }
    ]
  });

  const fetchData = async () => {
    if (!bridge) return;
    try {
      const summary = await bridge.getAutoResearchSummary();
      setData(summary);
    } catch (err) {
      console.error("Failed to fetch AutoResearch summary", err);
    }
  };

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000);
    return () => clearInterval(interval);
  }, [bridge]);

  const handleStart = async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      const res = await bridge.startAutoResearch();
      if (res.active) {
        notify("AutoResearch background optimization loop initiated", "info");
        await fetchData();
      } else {
        notify("Failed to start AutoResearch loop", "error");
      }
    } catch (err) {
      notify("Error starting AutoResearch: " + (err as Error).message, "error");
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      const res = await bridge.stopAutoResearch();
      if (!res.active) {
        notify("AutoResearch background loop stopped safely", "info");
        await fetchData();
      } else {
        notify("Failed to stop AutoResearch loop", "error");
      }
    } catch (err) {
      notify("Error stopping AutoResearch: " + (err as Error).message, "error");
    } finally {
      setLoading(false);
    }
  };

  const handleSimulateMutation = async () => {
    if (!bridge) return;
    setLoading(true);
    notify("Running local mutation and benchmark on SilvaDB...", "info");
    try {
      const res = await bridge.evaluateAutoResearch();
      if (res.experiment_run) {
        notify("Calibration cycle completed successfully", "info");
      } else {
        notify("Error running calibration cycle", "error");
      }
    } catch (err) {
      notify("Calibration error: " + (err as Error).message, "error");
    } finally {
      setLoading(false);
    }
  };

  const recallImprovement = data.metrics.current.recall_1 - data.metrics.baseline.recall_1;

  return (
    <div className="space-y-6">
      {/* Header Widget */}
      <div className="flex flex-col md:flex-row md:items-center justify-between p-6 bg-gradient-to-r from-slate-900 via-indigo-950/20 to-slate-900 border border-slate-800 rounded-3xl gap-4">
        <div className="flex items-start gap-4">
          <div className="w-12 h-12 bg-indigo-500/10 rounded-2xl flex items-center justify-center border border-indigo-500/30 text-indigo-400">
            <Beaker className="w-6 h-6 animate-pulse" />
          </div>
          <div>
            <h2 className="text-lg font-bold text-white tracking-tight flex items-center gap-2">
              AutoResearch Calibration Lab
              <span className={cn(
                "text-[10px] uppercase font-mono tracking-wider px-2 py-0.5 rounded-full border",
                data.status === "Running" 
                  ? "bg-purple-950/50 border-purple-500/40 text-purple-400 animate-pulse"
                  : "bg-slate-900 border-slate-800 text-slate-500"
              )}>
                {data.status}
              </span>
            </h2>
            <p className="text-xs text-slate-400 mt-1 max-w-xl">
              Uses idle CPU/RAM cycles to mutate SilvaDB parameters and auto-calibrate the orphan linker against the LongMemEval-S test suite.
            </p>
          </div>
        </div>
        
        <div className="flex items-center gap-3">
          {data.status !== "Running" ? (
            <button
              onClick={handleStart}
              disabled={loading}
              className="flex items-center gap-2 bg-emerald-500 hover:bg-emerald-600 disabled:opacity-50 text-slate-950 text-xs font-bold px-4 py-2.5 rounded-xl shadow-lg shadow-emerald-500/10 transition-all"
            >
              <Play className="w-4 h-4 fill-current" />
              Activar AutoResearch
            </button>
          ) : (
            <button
              onClick={handleStop}
              disabled={loading}
              className="flex items-center gap-2 bg-red-500/20 hover:bg-red-500/30 disabled:opacity-50 border border-red-500/30 text-red-400 text-xs font-bold px-4 py-2.5 rounded-xl transition-all"
            >
              <Square className="w-4 h-4 fill-current" />
              Detener Motor
            </button>
          )}
          
          <button
            onClick={handleSimulateMutation}
            disabled={loading}
            className="flex items-center gap-2 bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-300 text-xs font-bold px-4 py-2.5 rounded-xl transition-all"
          >
            <Zap className="w-4 h-4" />
            Mutar e Evaluar
          </button>
        </div>
      </div>

      {/* Main Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        
        {/* Metric Comparison Panel */}
        <div className="lg:col-span-2 space-y-6">
          <div className="p-6 bg-slate-900/60 border border-slate-800 rounded-2xl">
            <h3 className="text-sm font-bold text-slate-300 uppercase tracking-wider mb-4 flex items-center gap-2">
              <TrendingUp className="w-4 h-4 text-emerald-400" />
              Métricas de Recuperación: Baseline vs Calibrado
            </h3>
            
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {/* Recall@1 Card */}
              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl relative overflow-hidden">
                <span className="text-[10px] font-bold text-slate-500 uppercase">Recall @ 1 (Precisión)</span>
                <div className="flex items-baseline justify-between mt-2">
                  <div className="text-2xl font-bold font-mono text-white">
                    {(data.metrics.current.recall_1 * 100).toFixed(1)}%
                  </div>
                  <div className={cn(
                    "text-xs font-mono font-bold flex items-center",
                    recallImprovement > 0 ? "text-emerald-400" : "text-slate-500"
                  )}>
                    {recallImprovement > 0 ? `+${(recallImprovement * 100).toFixed(1)}%` : '0.0%'}
                  </div>
                </div>
                <div className="text-[10px] text-slate-500 font-mono mt-1">
                  Baseline: {(data.metrics.baseline.recall_1 * 100).toFixed(1)}%
                </div>
                <div className="absolute right-2 bottom-2 text-slate-800 opacity-20">
                  <ShieldCheck className="w-12 h-12" />
                </div>
              </div>

              {/* Recall@5 Card */}
              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl relative overflow-hidden">
                <span className="text-[10px] font-bold text-slate-500 uppercase">Recall @ 5 (Contexto)</span>
                <div className="flex items-baseline justify-between mt-2">
                  <div className="text-2xl font-bold font-mono text-white">
                    {(data.metrics.current.recall_5 * 100).toFixed(1)}%
                  </div>
                  <div className="text-xs font-mono font-bold text-emerald-400">
                    {data.metrics.current.recall_5 > data.metrics.baseline.recall_5 ? `+${((data.metrics.current.recall_5 - data.metrics.baseline.recall_5) * 100).toFixed(1)}%` : '0.0%'}
                  </div>
                </div>
                <div className="text-[10px] text-slate-500 font-mono mt-1">
                  Baseline: {(data.metrics.baseline.recall_5 * 100).toFixed(1)}%
                </div>
                <div className="absolute right-2 bottom-2 text-slate-800 opacity-20">
                  <TrendingUp className="w-12 h-12" />
                </div>
              </div>

              {/* Latency Card */}
              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl relative overflow-hidden">
                <span className="text-[10px] font-bold text-slate-500 uppercase">Latencia Recuperación</span>
                <div className="flex items-baseline justify-between mt-2">
                  <div className="text-2xl font-bold font-mono text-white">
                    {data.metrics.current.latency_ms} ms
                  </div>
                  <div className={cn(
                    "text-xs font-mono font-bold",
                    data.metrics.current.latency_ms < data.metrics.baseline.latency_ms ? "text-emerald-400" : "text-slate-500"
                  )}>
                    {data.metrics.current.latency_ms < data.metrics.baseline.latency_ms 
                      ? `${(data.metrics.current.latency_ms - data.metrics.baseline.latency_ms).toFixed(1)}ms` 
                      : '0.0ms'}
                  </div>
                </div>
                <div className="text-[10px] text-slate-500 font-mono mt-1">
                  Baseline: {data.metrics.baseline.latency_ms} ms
                </div>
                <div className="absolute right-2 bottom-2 text-slate-800 opacity-20">
                  <Clock className="w-12 h-12" />
                </div>
              </div>
            </div>
          </div>

          {/* Active Mutation Widget */}
          {data.status === "Running" && data.current_mutation && (
            <div className="p-6 bg-purple-950/20 border border-purple-500/20 rounded-2xl animate-pulse">
              <h3 className="text-sm font-bold text-purple-300 uppercase tracking-wider mb-4 flex items-center gap-2">
                <Cpu className="w-4 h-4 text-purple-400" />
                Mutación en Progreso Activo
              </h3>
              
              <div className="flex flex-col md:flex-row md:items-center justify-between bg-slate-950/60 p-4 border border-purple-500/10 rounded-xl gap-4">
                <div className="space-y-1">
                  <div className="text-xs font-mono text-purple-400 font-bold">{data.current_mutation.id}</div>
                  <div className="text-sm text-white font-bold">{data.current_mutation.target}</div>
                </div>
                
                <div className="flex items-center gap-4 text-sm font-mono bg-slate-900 px-4 py-2 rounded-lg border border-slate-800">
                  <div className="text-slate-500">original: <span className="text-white font-bold">{data.current_mutation.original_val}</span></div>
                  <ArrowRight className="w-4 h-4 text-purple-400" />
                  <div className="text-purple-400 font-bold">trial: {data.current_mutation.mutated_val}</div>
                </div>
              </div>
            </div>
          )}

          {/* Lineage Tree Visualization */}
          <div className="p-6 bg-slate-900/60 border border-slate-800 rounded-2xl">
            <h3 className="text-sm font-bold text-slate-300 uppercase tracking-wider mb-4 flex items-center gap-2">
              <GitBranch className="w-4 h-4 text-indigo-400" />
              Historial de Calibración & Lineaje (Commit/Revert)
            </h3>
            
            <div className="relative border-l border-slate-800 ml-4 pl-6 space-y-4">
              {(data.lineage || []).map((item, idx) => (
                <div key={idx} className="relative group">
                  {/* Node icon dot */}
                  <div className={cn(
                    "absolute -left-[31px] top-1 w-4 h-4 rounded-full border-4 bg-slate-950 flex items-center justify-center transition-transform group-hover:scale-125",
                    item.status === "Committed" 
                      ? "border-emerald-500 shadow-sm shadow-emerald-500/30" 
                      : "border-red-500"
                  )} />
                  
                  <div className="bg-slate-950/60 p-3 rounded-xl border border-slate-800 flex items-center justify-between text-xs">
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="font-bold text-slate-300">Paso #{item.step}</span>
                        <span className="text-slate-500 font-mono">{item.target}</span>
                      </div>
                      <div className="text-[10px] text-slate-400 font-mono mt-1">
                        Valor Ensayo: <span className="text-white">{item.val}</span> | Recall@1: <span className="text-white font-bold">{(item.recall_1 * 100).toFixed(1)}%</span>
                      </div>
                    </div>
                    
                    <span className={cn(
                      "text-[9px] uppercase font-mono tracking-wider font-bold px-2 py-0.5 rounded-md border",
                      item.status === "Committed"
                        ? "bg-emerald-950/40 border-emerald-500/20 text-emerald-400"
                        : "bg-red-950/40 border-red-500/20 text-red-400"
                    )}>
                      {item.status}
                    </span>
                  </div>
                </div>
              ))}
              
              {(!data.lineage || data.lineage.length === 0) && (
                <div className="text-xs text-slate-500 font-mono py-2">
                  No hay calibraciones registradas. Inicia el motor para comenzar la optimización autónoma.
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Configurations & Targets Panel */}
        <div className="space-y-6">
          <div className="p-6 bg-slate-900/60 border border-slate-800 rounded-2xl">
            <h3 className="text-sm font-bold text-slate-300 uppercase tracking-wider mb-4 flex items-center gap-2">
              <Settings className="w-4 h-4 text-indigo-400" />
              Parámetros Bajo Calibración
            </h3>
            
            <div className="space-y-4">
              {/* Parameter 1 */}
              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-bold text-slate-300">Candidate Pool Multiplier</span>
                  <span className="text-xs font-mono font-bold text-indigo-400">
                    {data.current_params?.candidate_pool_mult ?? 20}x
                  </span>
                </div>
                <p className="text-[10px] text-slate-500">
                  Factor multiplicador de candidatos recuperados antes del filtro Reranker (10x - 40x).
                </p>
                <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
                  <div 
                    className="h-full bg-indigo-500 transition-all duration-500" 
                    style={{ width: `${Math.min(100, ((data.current_params?.candidate_pool_mult ?? 20) / 40) * 100)}%` }}
                  />
                </div>
              </div>

              {/* Parameter 2 */}
              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-bold text-slate-300">Rerank Window Size</span>
                  <span className="text-xs font-mono font-bold text-indigo-400">
                    {data.current_params?.rerank_window ?? 40} docs
                  </span>
                </div>
                <p className="text-[10px] text-slate-500">
                  Cantidad de candidatos enviados al motor de Re-ranking Jina Cross-Encoder (20 - 80).
                </p>
                <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
                  <div 
                    className="h-full bg-indigo-500 transition-all duration-500" 
                    style={{ width: `${Math.min(100, ((data.current_params?.rerank_window ?? 40) / 80) * 100)}%` }}
                  />
                </div>
              </div>

              {/* Parameter 3 */}
              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-bold text-slate-300">Semantic Weight (RRF)</span>
                  <span className="text-xs font-mono font-bold text-indigo-400">
                    {data.current_params?.semantic_weight ?? 70}%
                  </span>
                </div>
                <p className="text-[10px] text-slate-500">
                  Peso de la búsqueda semántica vectorizada BGE-M3 (vs BM25 léxico) en fusión RRF.
                </p>
                <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
                  <div 
                    className="h-full bg-indigo-500 transition-all duration-500" 
                    style={{ width: `${data.current_params?.semantic_weight ?? 70}%` }}
                  />
                </div>
              </div>

              {/* Parameter 4 */}
              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-bold text-slate-300">Deduplication Cosine Threshold</span>
                  <span className="text-xs font-mono font-bold text-indigo-400">
                    0.{(data.current_params?.dedup_cosine ?? 92)}
                  </span>
                </div>
                <p className="text-[10px] text-slate-500">
                  Minimum cosine similarity threshold for merging new nodes in DreamCycle (0.80 - 0.98).
                </p>
                <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
                  <div 
                    className="h-full bg-indigo-500 transition-all duration-500" 
                    style={{ width: `${data.current_params?.dedup_cosine ?? 92}%` }}
                  />
                </div>
              </div>
            </div>
          </div>

          <div className="p-6 bg-slate-900/60 border border-slate-800 rounded-2xl space-y-4">
            <h3 className="text-sm font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
              <Clock className="w-4 h-4 text-emerald-400" />
              Gating de Homeostasia
            </h3>
            
            <div className="text-xs text-slate-400 leading-relaxed space-y-3">
              <p>
                Para evitar degradar la interacción del usuario, el motor solo realiza evaluaciones cuando se cumplen los siguientes criterios:
              </p>
              
              <ul className="space-y-2 font-mono text-[10px] bg-slate-950 p-3 rounded-lg border border-slate-800">
                <li className="flex items-center gap-2">
                  <span className="w-1.5 h-1.5 bg-emerald-500 rounded-full" />
                  IDLE_SECS &gt; 180s (OK)
                </li>
                <li className="flex items-center gap-2">
                  <span className="w-1.5 h-1.5 bg-emerald-500 rounded-full" />
                  REST_TRAFFIC == 0 (OK)
                </li>
                <li className="flex items-center gap-2">
                  <span className="w-1.5 h-1.5 bg-emerald-500 rounded-full" />
                  COLOQUIO_QUEUED == 0 (OK)
                </li>
              </ul>
            </div>
          </div>
        </div>
        
      </div>
    </div>
  );
}
