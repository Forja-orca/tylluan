import React from 'react';
import {
  Binary,
  Gauge,
  History
} from 'lucide-react';
import type { ModelCatalogEntry } from './model-catalog-contract';

export interface DecisionPlaneCardProps {
  decisionModels: ModelCatalogEntry[];
  activeDecisionModelId?: string;
  onSelectDecisionModel: (modelId: string) => void;
  autoRoutingEnabled: boolean;
  onToggleAutoRouting: (enabled: boolean) => void;
  confidenceThreshold: number;
  onUpdateConfidenceThreshold: (val: number) => void;
  ramBudget: {
    total_allowed_mb: number;
    current_used_private_mb?: number;
    current_used_mb?: number;
    current_peak_private_mb?: number;
  };
  lastDecisionTrace?: {
    timestamp: number;
    chosenModel: string;
    confidence: number;
    latencyMs: number;
    intentCategory: string;
  };
  className?: string;
}

export const DecisionPlaneCard: React.FC<DecisionPlaneCardProps> = ({
  decisionModels,
  activeDecisionModelId,
  onSelectDecisionModel,
  autoRoutingEnabled,
  onToggleAutoRouting,
  confidenceThreshold,
  onUpdateConfidenceThreshold,
  ramBudget,
  lastDecisionTrace,
  className = ''
}) => {
  const ramBudgetCompat = ramBudget as unknown as { total_allowed_mb?: number; limit_private_mb?: number; current_used_private_mb?: number; current_used_mb?: number };
  const totalAllowed = ramBudget.total_allowed_mb ?? ramBudgetCompat.limit_private_mb ?? 3072;
  const usedMb = ramBudget.current_used_private_mb ?? ramBudgetCompat.current_used_mb ?? 0;
  const percentUsed = Math.min(100, Math.round((usedMb / totalAllowed) * 100));
  const isOverBudget = usedMb > totalAllowed;

  return (
    <div
      className={`rounded border border-border bg-card p-4 space-y-4 text-foreground ${className}`}
      data-testid="decision-plane-card"
    >
      {/* Cabecera y Toggle Auto-Routing */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-border/60 pb-3">
        <div className="flex items-center gap-2.5">
          <div className="p-2 rounded bg-amber-500/15 text-amber-400 border border-amber-500/30">
            <Binary className="w-4 h-4" />
          </div>
          <div>
            <h4 className="text-sm font-semibold font-mono">Plano de Decisión y Enrutamiento (T3144)</h4>
            <p className="text-[11px] font-mono text-muted-foreground">
              Clasificación rápida en un pase · Cascada: Decisión calibrada → LLM solo si es necesario
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <label className="flex items-center gap-2 text-xs font-mono font-semibold cursor-pointer">
            <input
              type="checkbox"
              checked={autoRoutingEnabled}
              onChange={e => onToggleAutoRouting(e.target.checked)}
              className="rounded border-border text-amber-500 focus:ring-amber-400 focus:ring-offset-0"
            />
            <span>Enrutamiento Automático (Auto)</span>
          </label>
        </div>
      </div>

      {/* Selector de Modelo de Decisión y Umbral de Confianza */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs font-mono">
        <div className="space-y-1.5 p-3 rounded bg-muted/30 border border-border">
          <label className="text-[11px] text-muted-foreground block font-semibold">
            Modelo de Decisión Activo:
          </label>
          <select
            value={activeDecisionModelId}
            onChange={e => onSelectDecisionModel(e.target.value)}
            disabled={!autoRoutingEnabled}
            className="w-full px-2.5 py-1.5 text-xs font-mono rounded bg-background border border-border focus:ring-1 focus:ring-amber-400 focus:outline-none disabled:opacity-50"
          >
            {decisionModels.map(dm => (
              <option key={dm.id} value={dm.id}>
                {dm.name} ({dm.provider}) · ~{dm.decision_metadata?.typical_latency_ms || 20}ms
              </option>
            ))}
          </select>
          <p className="text-[10px] text-muted-foreground pt-1">
            Evalúa intención y complejidad sin generar texto token a token.
          </p>
        </div>

        <div className="space-y-1.5 p-3 rounded bg-muted/30 border border-border">
          <div className="flex items-center justify-between">
            <label className="text-[11px] text-muted-foreground font-semibold">
              Umbral Mínimo de Confianza:
            </label>
            <span className="text-amber-300 font-bold font-mono">
              {Math.round(confidenceThreshold * 100)}%
            </span>
          </div>
          <input
            type="range"
            min="0.50"
            max="0.95"
            step="0.05"
            value={confidenceThreshold}
            onChange={e => onUpdateConfidenceThreshold(parseFloat(e.target.value))}
            disabled={!autoRoutingEnabled}
            className="w-full accent-amber-500 cursor-pointer disabled:opacity-50"
          />
          <p className="text-[10px] text-muted-foreground pt-0.5">
            Si la confianza es inferior, escala automáticamente al modelo generativo superior.
          </p>
        </div>
      </div>

      {/* Presupuesto de RAM Soberano (Directiva José T3168: máx 3.072 MB RAM Privada) */}
      <div className="p-3 rounded bg-muted/20 border border-border/80 space-y-2 text-xs font-mono">
        <div className="flex items-center justify-between text-[11px]">
          <span className="text-muted-foreground flex items-center gap-1.5">
            <Gauge className="w-3.5 h-3.5 text-amber-400" />
            Presupuesto RAM Privada Suelo Tylluan (Tope: 3.072 MB):
          </span>
          <span className={`font-semibold ${isOverBudget ? 'text-rose-400' : 'text-emerald-400'}`}>
            {usedMb} MB / {ramBudget.total_allowed_mb} MB ({percentUsed}%)
          </span>
        </div>

        <div className="h-2 w-full rounded-full bg-muted overflow-hidden border border-border">
          <div
            className={`h-full transition-all duration-300 ${
              isOverBudget ? 'bg-rose-500' : percentUsed > 80 ? 'bg-amber-500' : 'bg-emerald-500'
            }`}
            style={{ width: `${percentUsed}%` }}
          />
        </div>

        <p className="text-[10px] text-muted-foreground">
          Medición estricta de RAM Privada (<code>PrivateMemorySize64</code>: ONNX residentes + servidores efímeros del Suelo). Excluye Working Set compartido y proceso kernel. El Techo del Operador (<code>origin: user</code>) no computa contra este presupuesto.
        </p>
      </div>

      {/* Observabilidad / Trazabilidad de Última Decisión */}
      {lastDecisionTrace && (
        <div className="p-2.5 rounded bg-amber-500/5 border border-amber-500/20 text-[11px] font-mono flex items-center justify-between gap-2 flex-wrap">
          <span className="flex items-center gap-1 text-muted-foreground">
            <History className="w-3.5 h-3.5 text-amber-400" />
            Último enrutado:
          </span>
          <span className="text-amber-300 font-semibold">{lastDecisionTrace.chosenModel}</span>
          <span className="text-muted-foreground">Categoría: {lastDecisionTrace.intentCategory}</span>
          <span className="text-muted-foreground">Confianza: {Math.round(lastDecisionTrace.confidence * 100)}%</span>
          <span className="text-muted-foreground">Latencia: {lastDecisionTrace.latencyMs}ms</span>
        </div>
      )}
    </div>
  );
};
