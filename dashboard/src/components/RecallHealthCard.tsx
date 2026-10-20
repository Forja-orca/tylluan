import React from 'react';
import { Activity, Clock, Database, AlertCircle, HelpCircle } from 'lucide-react';
import { cn } from '../lib/utils';

export interface RecallTelemetryData {
  calls: number;
  empty_calls: number;
  empty_ratio: number | null;
  mean_latency_ms: number | null;
  latency_histogram: Record<string, number>;
}

const LATENCY_BUCKETS = [
  '<=5ms',
  '<=10ms',
  '<=25ms',
  '<=50ms',
  '<=100ms',
  '<=250ms',
  '<=500ms',
  '<=1000ms',
  '<=2500ms',
  '>2500ms',
] as const;

export interface RecallHealthCardProps {
  recall?: RecallTelemetryData | null;
  isAbsent?: boolean;
  className?: string;
}

export const RecallHealthCard: React.FC<RecallHealthCardProps> = ({
  recall,
  isAbsent = false,
  className,
}) => {
  // Estado 1: Ausente (el kernel en vivo aún no implementa D-5 o el campo recall falta en components)
  if (isAbsent || recall === undefined) {
    return (
      <div
        role="region"
        aria-label="Salud del recall"
        className={cn(
          "bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-5 shadow-xs transition-colors",
          className
        )}
      >
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 mb-3">
          <div className="flex items-center gap-2.5">
            <Activity className="w-5 h-5 text-amber-600 dark:text-amber-400" aria-hidden="true" />
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              Salud del recall
            </h2>
          </div>
          <span
            className="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-semibold rounded-full border bg-amber-50 dark:bg-amber-950/40 text-amber-700 dark:text-amber-300 border-amber-300 dark:border-amber-700/60 self-start sm:self-auto"
          >
            <AlertCircle className="w-3.5 h-3.5" aria-hidden="true" />
            <span>kernel sin telemetría de recall (actualiza)</span>
          </span>
        </div>
        <p className="text-xs text-slate-600 dark:text-slate-400 leading-relaxed">
          El kernel en ejecución no expone contadores de telemetría en{' '}
          <code className="font-mono text-slate-800 dark:text-slate-200 bg-slate-100 dark:bg-slate-800 px-1 py-0.5 rounded">
            components.recall
          </code>
          . Requiere un binario con D-5 compilado y en ejecución para visualizar latencias, ratio de vacías e histograma.
        </p>
      </div>
    );
  }

  // Si no hay datos (null inicial sin haber cargado)
  if (recall === null) {
    return (
      <div
        role="region"
        aria-label="Salud del recall"
        className={cn(
          "bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-5 shadow-xs transition-colors",
          className
        )}
      >
        <div className="flex items-center gap-2.5 mb-2">
          <Activity className="w-5 h-5 text-slate-400" aria-hidden="true" />
          <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
            Salud del recall
          </h2>
        </div>
        <p className="text-xs text-slate-500 dark:text-slate-400">
          Cargando telemetría de recall...
        </p>
      </div>
    );
  }

  const hasCalls = recall.calls > 0;
  const emptyRatioText = recall.empty_ratio !== null
    ? `${(recall.empty_ratio * 100).toFixed(1)}%`
    : 'sin datos';

  const meanLatencyText = recall.mean_latency_ms !== null
    ? `${recall.mean_latency_ms.toFixed(1)} ms`
    : 'sin datos';

  const histogram = recall.latency_histogram || {};
  const maxBucketCount = Math.max(1, ...Object.values(histogram));

  return (
    <div
      role="region"
      aria-label="Salud del recall"
      className={cn(
        "bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-5 shadow-xs transition-colors space-y-4",
        className
      )}
    >
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 border-b border-slate-100 dark:border-slate-800/80 pb-3">
        <div className="flex items-center gap-2.5">
          <Activity className="w-5 h-5 text-emerald-600 dark:text-emerald-400" aria-hidden="true" />
          <div>
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              Salud del recall
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              Telemetría en memoria (D-5) · GET /health?verbose=true
            </p>
          </div>
        </div>
        <span
          className={cn(
            "inline-flex items-center gap-1.5 px-2.5 py-0.5 text-xs font-semibold rounded-full border self-start sm:self-auto",
            hasCalls
              ? "bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-300 border-emerald-300 dark:border-emerald-700/60"
              : "bg-slate-100 dark:bg-slate-800/60 text-slate-600 dark:text-slate-300 border-slate-300 dark:border-slate-700"
          )}
        >
          {hasCalls ? 'Activo' : 'Sin llamadas'}
        </span>
      </div>

      {/* Grid de Métricas Principales */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
        {/* Total Calls */}
        <div className="bg-slate-50 dark:bg-slate-950/60 border border-slate-200/80 dark:border-slate-800/80 rounded-lg p-3">
          <span className="text-[11px] font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-wider block">
            Total Calls
          </span>
          <div className="text-lg font-mono font-bold text-slate-900 dark:text-slate-100 mt-1">
            {recall.calls.toLocaleString()}
          </div>
          <span className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5 block">
            consultas procesadas
          </span>
        </div>

        {/* Empty Calls */}
        <div className="bg-slate-50 dark:bg-slate-950/60 border border-slate-200/80 dark:border-slate-800/80 rounded-lg p-3">
          <span className="text-[11px] font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-wider block">
            Empty Calls
          </span>
          <div className="text-lg font-mono font-bold text-slate-900 dark:text-slate-100 mt-1">
            {recall.empty_calls.toLocaleString()}
          </div>
          <span className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5 block">
            sin memorias devueltas
          </span>
        </div>

        {/* Empty Ratio */}
        <div className="bg-slate-50 dark:bg-slate-950/60 border border-slate-200/80 dark:border-slate-800/80 rounded-lg p-3">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-wider block">
              Empty Ratio
            </span>
            <span title="Proporción de llamadas que devolvieron vacío">
              <HelpCircle className="w-3 h-3 text-slate-400" aria-hidden="true" />
            </span>
          </div>
          <div
            className={cn(
              "text-lg font-mono font-bold mt-1",
              recall.empty_ratio === null
                ? "text-slate-500 dark:text-slate-400 italic"
                : recall.empty_ratio > 0.5
                ? "text-rose-600 dark:text-rose-400"
                : "text-emerald-600 dark:text-emerald-400"
            )}
          >
            {emptyRatioText}
          </div>
          <span className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5 block">
            {recall.empty_ratio !== null ? (recall.empty_ratio > 0.5 ? 'alerta: búsqueda vacía' : 'ratio saludable') : 'esperando llamadas'}
          </span>
        </div>

        {/* Latencia Media */}
        <div className="bg-slate-50 dark:bg-slate-950/60 border border-slate-200/80 dark:border-slate-800/80 rounded-lg p-3">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-wider block">
              Latencia Media
            </span>
            <Clock className="w-3 h-3 text-slate-400" aria-hidden="true" />
          </div>
          <div
            className={cn(
              "text-lg font-mono font-bold mt-1",
              recall.mean_latency_ms === null
                ? "text-slate-500 dark:text-slate-400 italic"
                : recall.mean_latency_ms > 500
                ? "text-amber-600 dark:text-amber-400"
                : "text-emerald-600 dark:text-emerald-400"
            )}
          >
            {meanLatencyText}
          </div>
          <span className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5 block">
            {recall.mean_latency_ms !== null ? 'tiempo promedio por consulta' : 'esperando llamadas'}
          </span>
        </div>
      </div>

      {/* Histograma de Latencia (10 cubos D-5) */}
      <div className="mt-4 pt-3 border-t border-slate-100 dark:border-slate-800/80">
        <div className="flex items-center justify-between mb-2.5">
          <span className="text-xs font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-1.5">
            <Database className="w-3.5 h-3.5 text-slate-500" aria-hidden="true" />
            Histograma de latencia (10 cubos)
          </span>
          <span className="text-[11px] text-slate-500 dark:text-slate-400 font-mono">
            {hasCalls ? `${recall.calls} llamadas sumadas` : 'sin llamadas'}
          </span>
        </div>

        {!hasCalls ? (
          <div className="p-4 bg-slate-50 dark:bg-slate-950/40 rounded-lg text-center text-xs text-slate-500 dark:text-slate-400">
            Sin datos de latencia acumulados hasta la primera llamada de recall.
          </div>
        ) : (
          <div
            tabIndex={0}
            role="region"
            aria-label="Distribución de histograma de latencia"
            className="grid grid-cols-2 sm:grid-cols-5 gap-2 font-mono text-xs focus-visible:ring-1 focus-visible:ring-emerald-500 focus-visible:outline-none rounded-lg p-1"
          >
            {LATENCY_BUCKETS.map((bucketKey) => {
              const count = histogram[bucketKey] ?? 0;
              const pct = hasCalls ? (count / recall.calls) * 100 : 0;
              const barHeightPct = Math.max(8, (count / maxBucketCount) * 100);

              return (
                <div
                  key={bucketKey}
                  className="bg-slate-50 dark:bg-slate-950/60 border border-slate-200/80 dark:border-slate-800/80 rounded-md p-2 flex flex-col justify-between"
                >
                  <div className="flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400">
                    <span>{bucketKey}</span>
                    <span className="font-semibold text-slate-800 dark:text-slate-200">{count}</span>
                  </div>
                  <div className="w-full bg-slate-200 dark:bg-slate-800 h-1.5 rounded-full mt-2 overflow-hidden">
                    <div
                      className={cn(
                        "h-full rounded-full transition-all duration-300",
                        bucketKey === '>2500ms'
                          ? "bg-rose-500"
                          : bucketKey.includes('1000ms') || bucketKey.includes('2500ms')
                          ? "bg-amber-500"
                          : "bg-emerald-500"
                      )}
                      style={{ width: `${pct > 0 ? barHeightPct : 0}%` }}
                    />
                  </div>
                  <div className="text-[10px] text-slate-400 dark:text-slate-500 text-right mt-1">
                    {pct.toFixed(0)}%
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
};
