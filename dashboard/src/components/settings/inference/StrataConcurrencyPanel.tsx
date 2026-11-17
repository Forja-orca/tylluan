import React, { useCallback, useEffect, useState } from 'react';
import { Activity, AlertTriangle, Loader2, RefreshCw } from 'lucide-react';

export interface StrataAgentMetric {
  agent_id: string;
  ttft_ms: number;
  tok_per_sec: number;
  in_flight?: number;
}

export interface StrataStatusPayload {
  concurrency: {
    serving: number;
    requested: number;
  };
  agents: StrataAgentMetric[];
}

export interface StrataConcurrencyPanelProps {
  /** Mock opcional para simular GET /v1/status de Strata sin red real. */
  fetchStatus?: () => Promise<StrataStatusPayload>;
  /** Datos iniciales opcionales para render directo sin esperar promesa. */
  initialData?: StrataStatusPayload;
}

const DEFAULT_MOCK_STATUS: StrataStatusPayload = {
  concurrency: {
    serving: 2,
    requested: 4,
  },
  agents: [
    { agent_id: 'deep', ttft_ms: 142, tok_per_sec: 48.6, in_flight: 1 },
    { agent_id: 'dsh', ttft_ms: 185, tok_per_sec: 41.2, in_flight: 1 },
    { agent_id: 'hermes', ttft_ms: 210, tok_per_sec: 36.8, in_flight: 0 },
  ],
};

/**
 * Panel «Concurrencia» en Inferencia con datos MOCK de GET /v1/status de Strata.
 * Muestra concurrency.serving / concurrency.requested, TTFT y tok/s por agente.
 */
export function StrataConcurrencyPanel({
  fetchStatus,
  initialData,
}: StrataConcurrencyPanelProps = {}) {
  const [data, setData] = useState<StrataStatusPayload | null>(
    initialData ?? (fetchStatus ? null : DEFAULT_MOCK_STATUS),
  );
  const [loading, setLoading] = useState<boolean>(Boolean(fetchStatus && !initialData));
  const [error, setError] = useState<string | null>(null);

  const loadStatus = useCallback(async () => {
    if (!fetchStatus) {
      setError(null);
      setData(initialData ?? DEFAULT_MOCK_STATUS);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const result = await fetchStatus();
      setData(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error al consultar /v1/status (mock).');
    } finally {
      setLoading(false);
    }
  }, [fetchStatus, initialData]);

  useEffect(() => {
    if (fetchStatus && !initialData) {
      void loadStatus();
    }
  }, [fetchStatus, initialData, loadStatus]);

  const serving = data?.concurrency?.serving ?? 0;
  const requested = data?.concurrency?.requested ?? 0;
  const agents = data?.agents ?? [];

  return (
    <section
      aria-labelledby="strata-concurrency-heading"
      className="bg-card border border-border rounded-lg p-4 shadow-sm space-y-3 font-sans"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 pb-2 border-b border-border">
        <div className="flex items-center gap-2">
          <Activity className="w-4 h-4 text-primary" aria-hidden="true" />
          <div>
            <h4 id="strata-concurrency-heading" className="text-xs font-bold text-foreground">
              Concurrencia (Strata)
            </h4>
            <p className="text-[11px] text-muted-foreground">
              Telemetría simulada de <code className="font-mono">GET /v1/status</code> (serving, requested, TTFT y tok/s).
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className="px-2 py-0.5 rounded text-[10px] font-mono font-semibold bg-amber-500/15 text-amber-800 dark:text-amber-300 border border-amber-500/30">
            MOCK · /v1/status
          </span>
          <button
            type="button"
            disabled={loading}
            aria-busy={loading}
            onClick={() => void loadStatus()}
            className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-semibold border border-border bg-muted/40 hover:bg-muted text-foreground cursor-pointer disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            {loading ? (
              <Loader2 className="w-3 h-3 animate-spin" aria-hidden="true" />
            ) : (
              <RefreshCw className="w-3 h-3" aria-hidden="true" />
            )}
            <span>{loading ? 'Consultando...' : 'Actualizar'}</span>
          </button>
        </div>
      </div>

      {loading && (
        <div
          role="status"
          className="p-4 rounded-md border border-border bg-muted/30 flex items-center gap-2 text-xs text-muted-foreground font-mono"
        >
          <Loader2 className="w-4 h-4 animate-spin text-primary" aria-hidden="true" />
          <span>Cargando métricas de concurrencia desde /v1/status (mock)...</span>
        </div>
      )}

      {!loading && error && (
        <div
          role="alert"
          className="p-3 rounded-md border border-rose-500/30 bg-rose-500/10 flex flex-wrap items-center justify-between gap-2 text-xs text-rose-800 dark:text-rose-300"
        >
          <div className="flex items-center gap-2">
            <AlertTriangle className="w-4 h-4 shrink-0" aria-hidden="true" />
            <span>Error al leer /v1/status: {error}</span>
          </div>
          <button
            type="button"
            onClick={() => void loadStatus()}
            className="px-2.5 py-1 rounded text-[11px] font-semibold bg-rose-700 hover:bg-rose-600 text-white cursor-pointer focus-visible:ring-2 focus-visible:ring-rose-500 focus-visible:outline-none"
          >
            Reintentar
          </button>
        </div>
      )}

      {!loading && !error && data && (
        <>
          <div className="grid grid-cols-2 gap-3">
            <div className="p-3 rounded-md border border-border bg-muted/20">
              <span className="text-[11px] text-muted-foreground block">Concurrencia (serving)</span>
              <span
                data-testid="strata-serving-count"
                className="text-base font-mono font-bold text-foreground"
              >
                {serving}
              </span>
            </div>
            <div className="p-3 rounded-md border border-border bg-muted/20">
              <span className="text-[11px] text-muted-foreground block">Solicitadas (requested)</span>
              <span
                data-testid="strata-requested-count"
                className="text-base font-mono font-bold text-foreground"
              >
                {requested}
              </span>
            </div>
          </div>

          {agents.length === 0 ? (
            <div
              role="status"
              className="p-4 rounded-md border border-dashed border-border text-center text-xs text-muted-foreground"
            >
              Sin agentes activos en Strata en este momento.
            </div>
          ) : (
            <div className="overflow-x-auto rounded-md border border-border">
              <table
                aria-label="Métricas de concurrencia por agente en Strata"
                className="w-full text-left text-xs font-mono"
              >
                <thead className="bg-muted/40 text-muted-foreground border-b border-border text-[11px]">
                  <tr>
                    <th scope="col" className="px-3 py-2 font-semibold">Agente</th>
                    <th scope="col" className="px-3 py-2 font-semibold">TTFT (ms)</th>
                    <th scope="col" className="px-3 py-2 font-semibold">tok/s</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border">
                  {agents.map((ag) => (
                    <tr key={ag.agent_id} className="hover:bg-muted/20">
                      <td className="px-3 py-2 font-semibold text-foreground">@{ag.agent_id}</td>
                      <td className="px-3 py-2 text-foreground">{ag.ttft_ms} ms</td>
                      <td className="px-3 py-2 text-foreground">{ag.tok_per_sec.toFixed(1)} tok/s</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </section>
  );
}
