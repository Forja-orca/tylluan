import React, { useState, useEffect, useCallback, useMemo } from 'react';
import {
  Scale,
  AlertTriangle,
  RefreshCw,
  CheckCircle2,
  XCircle,
  MinusCircle,
  Clock,
  Cpu,
  ShieldAlert,
  SlidersHorizontal
} from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { usePolling } from '../hooks/usePolling';
import { cn } from '../lib/utils';
import { withTimeout } from '../lib/timeout';

export interface DecisionShadowRow {
  id: number | string;
  ts: string | number;
  contract: string;
  request_hash: string;
  rules_value: string | null;
  model_value: string | null;
  provider: string | null;
  latency_ms: number | null;
  agreed: number | boolean | null;
  mode: string;
  model_identity: string | null;
}

export interface DecisionContractSummary {
  contract: string;
  n: number;
  n_agreed: number;
  n_disagreed: number;
  n_null: number;
}

export interface DecisionShadowResponse {
  rows: DecisionShadowRow[];
  summary?: DecisionContractSummary[];
  limit?: number;
  total?: number;
}

export interface DecisionServicePanelProps {
  bridge: NexusBridge | null;
}

const KNOWN_CONTRACTS = [
  { id: 'recall_gate', label: 'recall_gate', description: 'Retrieve vs abstain on query evidence' },
  { id: 'write_relevance', label: 'write_relevance', description: 'Persist vs discard memory candidate' },
  { id: 'conflict_verdict', label: 'conflict_verdict', description: 'Contradiction vs compatible node pair' },
  { id: 'tool_choice', label: 'tool_choice', description: 'Guild/tool routing selection' },
];

function normalizeAgreed(val: number | boolean | null | undefined): 'agreed' | 'disagreed' | 'null' {
  if (val === null || val === undefined) return 'null';
  if (val === true || val === 1) return 'agreed';
  if (val === false || val === 0) return 'disagreed';
  return 'null';
}

function formatTimestamp(ts: string | number): string {
  if (!ts) return '—';
  try {
    const d = typeof ts === 'number' ? new Date(ts > 1e12 ? ts : ts * 1000) : new Date(ts);
    if (isNaN(d.getTime())) return String(ts);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    return String(ts);
  }
}

export function DecisionServicePanel({ bridge }: DecisionServicePanelProps) {
  const [rows, setRows] = useState<DecisionShadowRow[]>([]);
  const [rawSummary, setRawSummary] = useState<DecisionContractSummary[]>([]);
  const [totalCount, setTotalCount] = useState<number>(0);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [endpointStatus, setEndpointStatus] = useState<'ready' | 'pending_backend' | 'forbidden' | 'error'>('ready');
  const [limit, setLimit] = useState<number>(50);
  const [contractFilter, setContractFilter] = useState<string>('all');

  const fetchShadowLog = useCallback(async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    try {
      const res = (await withTimeout(
        bridge.fetchRaw(`/api/v1/decide/shadow?limit=${limit}`),
        10000,
        'Request timed out after 10s. GET /api/v1/decide/shadow took too long.'
      )) as DecisionShadowResponse;
      const validRows = Array.isArray(res?.rows) ? res.rows : [];
      setRows(validRows);
      setTotalCount(typeof res?.total === 'number' ? res.total : validRows.length);

      if (Array.isArray(res?.summary)) {
        setRawSummary(
          res.summary.map((item) => ({
            contract: String(item.contract),
            n: Number(item.n ?? 0),
            n_agreed: Number(item.n_agreed ?? 0),
            n_disagreed: Number(item.n_disagreed ?? 0),
            n_null: Number(item.n_null ?? 0),
          }))
        );
      } else {
        setRawSummary([]);
      }
      setEndpointStatus('ready');
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err || '');
      if (msg.includes('404') || msg.toLowerCase().includes('not found')) {
        setEndpointStatus('pending_backend');
        setError('GET /api/v1/decide/shadow no está montado aún en el kernel en ejecución (endpoint de sombra Fase 2).');
      } else if (msg.includes('403') || msg.includes('401') || msg.toLowerCase().includes('forbidden') || msg.toLowerCase().includes('unauthorized')) {
        setEndpointStatus('forbidden');
        setError('Requiere credencial de operador (HTTP 403). Token maestro requerido para inspeccionar decision_shadow_log.');
      } else if (msg.includes('timed out') || msg.includes('timeout')) {
        setEndpointStatus('error');
        setError('Request timed out after 10s. El endpoint tardó más de 10 segundos en responder.');
      } else {
        setEndpointStatus('error');
        setError(msg || 'Error al consultar /api/v1/decide/shadow en :47004');
      }
      setRows([]);
      setRawSummary([]);
      setTotalCount(0);
    } finally {
      setLoading(false);
    }
  }, [bridge, limit]);

  useEffect(() => {
    fetchShadowLog();
  }, [fetchShadowLog]);

  usePolling('decision-shadow-log', fetchShadowLog, { interval: 'slow', enabled: !!bridge });

  // Derive per-contract summary from backend summary OR fallback from rows
  const contractSummaries = useMemo<DecisionContractSummary[]>(() => {
    const map = new Map<string, DecisionContractSummary>();
    for (const c of KNOWN_CONTRACTS) {
      map.set(c.id, { contract: c.id, n: 0, n_agreed: 0, n_disagreed: 0, n_null: 0 });
    }

    if (rawSummary.length > 0) {
      for (const s of rawSummary) {
        map.set(s.contract, s);
      }
      return Array.from(map.values());
    }

    for (const r of rows) {
      const key = r.contract || 'unknown';
      const entry = map.get(key) ?? { contract: key, n: 0, n_agreed: 0, n_disagreed: 0, n_null: 0 };
      entry.n += 1;
      const state = normalizeAgreed(r.agreed);
      if (state === 'agreed') entry.n_agreed += 1;
      else if (state === 'disagreed') entry.n_disagreed += 1;
      else entry.n_null += 1;
      map.set(key, entry);
    }
    return Array.from(map.values());
  }, [rawSummary, rows]);

  const filteredRows = useMemo(() => {
    if (contractFilter === 'all') return rows;
    return rows.filter((r) => r.contract === contractFilter);
  }, [rows, contractFilter]);

  const activeProviderSummary = useMemo(() => {
    const firstWithModel = rows.find((r) => r.provider || r.model_identity);
    if (!firstWithModel) {
      return { mode: 'rules (deterministic)', provider: 'none', model: 'No shadow model active' };
    }
    return {
      mode: firstWithModel.mode || 'shadow',
      provider: firstWithModel.provider || 'http_json',
      model: firstWithModel.model_identity || 'local loopback model',
    };
  }, [rows]);

  return (
    <section
      aria-labelledby="decision-service-heading"
      className="bg-slate-900/70 border border-slate-800 rounded-2xl p-6 space-y-6 font-sans"
    >
      {/* Header */}
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 border-b border-slate-800 pb-4">
        <div>
          <div className="flex flex-wrap items-center gap-2 mb-1.5">
            <span className="px-2 py-0.5 text-xs font-mono font-semibold bg-amber-500/15 text-amber-300 border border-amber-500/30 rounded-md">
              ADR-021 · tylluan_decide
            </span>
            <span className="px-2 py-0.5 text-xs font-mono font-semibold bg-emerald-500/15 text-emerald-300 border border-emerald-500/30 rounded-md">
              Mode: {activeProviderSummary.mode}
            </span>
            <span className="px-2 py-0.5 text-xs font-mono text-slate-300 bg-slate-800 border border-slate-700 rounded-md flex items-center gap-1.5">
              <Cpu className="w-3.5 h-3.5 text-amber-400" />
              {activeProviderSummary.model}
            </span>
          </div>
          <h2
            id="decision-service-heading"
            className="text-xl font-bold text-slate-100 flex items-center gap-2.5"
          >
            <Scale className="w-5 h-5 text-amber-400" />
            Decision Service — Contracts &amp; Shadow Telemetry
          </h2>
          <p className="text-xs text-slate-300 mt-1">
            Deterministic rules + optional loopback model in shadow mode (`decision_shadow_log` v29). No raw query payloads are stored or exposed.
          </p>
        </div>

        <div className="flex items-center gap-2 self-end md:self-auto">
          <div className="flex items-center gap-1.5 bg-slate-950 border border-slate-800 rounded-xl px-2.5 py-1.5">
            <SlidersHorizontal className="w-3.5 h-3.5 text-slate-400" />
            <label htmlFor="shadow-limit-select" className="sr-only">
              Shadow log row limit
            </label>
            <select
              id="shadow-limit-select"
              aria-label="Shadow log row limit"
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
              className="bg-transparent text-xs font-mono text-slate-200 focus:outline-none"
            >
              <option value={25} className="bg-slate-900">25 rows</option>
              <option value={50} className="bg-slate-900">50 rows</option>
              <option value={100} className="bg-slate-900">100 rows</option>
            </select>
          </div>

          <button
            type="button"
            onClick={fetchShadowLog}
            disabled={loading}
            aria-label="Refresh Decision Service shadow log"
            className="flex items-center gap-1.5 px-3.5 py-2 bg-slate-800 hover:bg-slate-700 focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none text-xs text-slate-200 font-medium rounded-xl transition-colors disabled:opacity-50 cursor-pointer"
          >
            <RefreshCw className={cn('w-3.5 h-3.5 text-amber-400', loading && 'animate-spin')} />
            <span>{loading ? 'Syncing...' : 'Refresh'}</span>
          </button>
          {loading && (
            <button
              type="button"
              onClick={() => {
                setLoading(false);
                setError('Consulta cancelada por el usuario.');
                setEndpointStatus('error');
              }}
              aria-label="Cancel shadow log query"
              className="flex items-center gap-1 px-3 py-2 bg-rose-500/10 hover:bg-rose-500/20 border border-rose-500/30 text-rose-300 text-xs font-medium rounded-xl transition-colors cursor-pointer"
            >
              Cancel
            </button>
          )}
        </div>
      </div>

      {/* Prominent Epistemic Warning Banner (Mandatory per D1) */}
      <div
        role="note"
        aria-label="Advertencia epistemológica sobre acuerdo reglas y modelo"
        className="p-4 rounded-xl bg-amber-500/10 border border-amber-500/30 flex items-start gap-3"
      >
        <AlertTriangle className="w-5 h-5 text-amber-400 shrink-0 mt-0.5" aria-hidden="true" />
        <div className="text-xs text-slate-200 leading-relaxed">
          <span className="font-bold text-amber-300 uppercase tracking-wide mr-1.5">
            Sin verdad humana: acuerdo con reglas no es precisión.
          </span>
          Coincidir con las reglas deterministas solo mide consistencia entre el clasificador en sombra y las heurísticas actuales. Cuando las reglas se abstienen (`abstain`), el modelo en sombra no se consulta para evitar decisiones no auditables.
        </div>
      </div>

      {/* Endpoint status banner when offline / 404 / 403 (sin aria-live en contenido sondeado) */}
      {error && (
        <div
          className={cn(
            'p-4 rounded-xl border flex flex-col sm:flex-row sm:items-center justify-between gap-3 text-xs',
            endpointStatus === 'forbidden'
              ? 'bg-amber-500/10 border-amber-500/40 text-amber-200'
              : endpointStatus === 'pending_backend'
              ? 'bg-slate-950/90 border-slate-700 text-slate-300'
              : 'bg-rose-500/10 border-rose-500/30 text-rose-300'
          )}
        >
          <div className="flex items-start gap-2.5">
            <ShieldAlert className={cn('w-4 h-4 shrink-0 mt-0.5', endpointStatus === 'forbidden' ? 'text-amber-400' : 'text-rose-400')} aria-hidden="true" />
            <div>
              <p className="font-semibold text-slate-100">{error}</p>
              <p className="text-slate-300 mt-0.5">
                {endpointStatus === 'forbidden'
                  ? 'Configura la credencial de operador en tu sesión para consultar el decision_shadow_log.'
                  : endpointStatus === 'pending_backend'
                  ? 'Mostrando los contratos canónicos de decisión; las filas de sombra se poblarán al responder GET /api/v1/decide/shadow.'
                  : 'Comprueba el token Bearer de operador o el estado del kernel en :47004.'}
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={fetchShadowLog}
            aria-label="Reintentar consulta de log en sombra"
            className="self-start sm:self-auto px-3.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 font-mono text-xs transition-colors shrink-0 cursor-pointer focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none"
          >
            Reintentar
          </button>
        </div>
      )}

      {/* Per-Contract Agreement Summary Grid */}
      <div>
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-xs font-mono font-bold uppercase tracking-wider text-slate-300">
            Agreement &amp; Disagreement by Contract
          </h3>
          {contractFilter !== 'all' && (
            <button
              type="button"
              onClick={() => setContractFilter('all')}
              className="text-xs font-mono text-amber-400 hover:text-amber-300 underline cursor-pointer"
            >
              Show all contracts
            </button>
          )}
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
          {contractSummaries.map((c) => {
            const meta = KNOWN_CONTRACTS.find((k) => k.id === c.contract);
            const isSelected = contractFilter === c.contract;
            const evaluated = c.n_agreed + c.n_disagreed;
            const agreePct = evaluated > 0 ? Math.round((c.n_agreed / evaluated) * 100) : null;

            return (
              <button
                key={c.contract}
                type="button"
                onClick={() => setContractFilter(isSelected ? 'all' : c.contract)}
                aria-pressed={isSelected}
                className={cn(
                  'text-left p-4 rounded-xl border transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none',
                  isSelected
                    ? 'bg-amber-500/15 border-amber-500/50'
                    : 'bg-slate-950/70 border-slate-800 hover:border-slate-700'
                )}
              >
                <div className="flex items-center justify-between gap-2 mb-1">
                  <span className="font-mono text-xs font-bold text-amber-300 truncate">
                    {c.contract}
                  </span>
                  <span className="font-mono text-xs px-2 py-0.5 rounded bg-slate-900 text-slate-300 border border-slate-800">
                    n={c.n}
                  </span>
                </div>
                {meta && (
                  <p className="text-xs text-slate-400 mb-3 line-clamp-1">{meta.description}</p>
                )}

                <div className="grid grid-cols-3 gap-1.5 text-xs font-mono pt-2 border-t border-slate-800/80">
                  <div className="p-1.5 rounded bg-emerald-500/10 border border-emerald-500/20 text-center">
                    <span className="block text-emerald-300 font-bold">{c.n_agreed}</span>
                    <span className="text-[11px] text-slate-300">Agreed</span>
                  </div>
                  <div className="p-1.5 rounded bg-rose-500/10 border border-rose-500/20 text-center">
                    <span className="block text-rose-300 font-bold">{c.n_disagreed}</span>
                    <span className="text-[11px] text-slate-300">Disagreed</span>
                  </div>
                  <div className="p-1.5 rounded bg-slate-900 border border-slate-800 text-center">
                    <span className="block text-slate-300 font-bold">{c.n_null}</span>
                    <span className="text-[11px] text-slate-400">Abstain/Null</span>
                  </div>
                </div>

                <div className="mt-2.5 flex items-center justify-between text-xs font-mono text-slate-300">
                  <span>Shadow concordance:</span>
                  <span className="font-bold text-slate-100">
                    {agreePct !== null ? `${agreePct}%` : '—'}
                  </span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {/* Shadow Log Table */}
      <div className="bg-slate-950/60 border border-slate-800 rounded-xl overflow-hidden">
        <div className="px-4 py-3 border-b border-slate-800 flex flex-col sm:flex-row sm:items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <span className="text-xs font-mono font-bold uppercase tracking-wider text-slate-200">
              Recent Shadow Evaluations (`decision_shadow_log`)
            </span>
            {contractFilter !== 'all' && (
              <span className="px-2 py-0.5 rounded text-xs font-mono bg-amber-500/15 text-amber-300 border border-amber-500/30">
                contract: {contractFilter}
              </span>
            )}
          </div>
          <span className="text-xs font-mono text-slate-400">
            Showing {filteredRows.length} of {rows.length} rows (total: {totalCount})
          </span>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse text-xs">
            <thead>
              <tr className="border-b border-slate-800 bg-slate-900/70 font-mono text-slate-300">
                <th className="py-2.5 px-3">Contract</th>
                <th className="py-2.5 px-3">Mode</th>
                <th className="py-2.5 px-3">Rules Verdict</th>
                <th className="py-2.5 px-3">Shadow Model Verdict</th>
                <th className="py-2.5 px-3 text-center">Comparison</th>
                <th className="py-2.5 px-3">Provider / Model</th>
                <th className="py-2.5 px-3 text-right">Latency</th>
                <th className="py-2.5 px-3">Req Hash</th>
                <th className="py-2.5 px-3 text-right">Time</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/60 font-mono">
              {filteredRows.length > 0 ? (
                filteredRows.map((row, idx) => {
                  const cmp = normalizeAgreed(row.agreed);
                  return (
                    <tr key={row.id ?? idx} className="hover:bg-slate-900/40 transition-colors">
                      <td className="py-2.5 px-3 font-bold text-amber-300">{row.contract}</td>
                      <td className="py-2.5 px-3 text-slate-300">{row.mode || 'shadow'}</td>
                      <td className="py-2.5 px-3">
                        <span className="px-2 py-0.5 rounded bg-slate-900 text-slate-200 border border-slate-700">
                          {row.rules_value ?? 'abstain'}
                        </span>
                      </td>
                      <td className="py-2.5 px-3">
                        <span className="px-2 py-0.5 rounded bg-slate-900 text-slate-300 border border-slate-800">
                          {row.model_value ?? '— (not called)'}
                        </span>
                      </td>
                      <td className="py-2.5 px-3 text-center">
                        {cmp === 'agreed' && (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
                            <CheckCircle2 className="w-3 h-3" aria-hidden="true" />
                            AGREED
                          </span>
                        )}
                        {cmp === 'disagreed' && (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-rose-500/15 text-rose-300 border border-rose-500/30">
                            <XCircle className="w-3 h-3" aria-hidden="true" />
                            DISAGREED
                          </span>
                        )}
                        {cmp === 'null' && (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-slate-800 text-slate-300 border border-slate-700">
                            <MinusCircle className="w-3 h-3" aria-hidden="true" />
                            NULL / ABSTAIN
                          </span>
                        )}
                      </td>
                      <td className="py-2.5 px-3 text-slate-300">
                        {row.provider || 'rules'}
                        {row.model_identity ? ` · ${row.model_identity}` : ''}
                      </td>
                      <td className="py-2.5 px-3 text-right text-slate-300">
                        {row.latency_ms !== null && row.latency_ms !== undefined
                          ? `${row.latency_ms} ms`
                          : '—'}
                      </td>
                      <td className="py-2.5 px-3 text-slate-400" title={row.request_hash}>
                        {row.request_hash ? `${row.request_hash.slice(0, 10)}…` : '—'}
                      </td>
                      <td className="py-2.5 px-3 text-right text-slate-400">
                        <span className="inline-flex items-center gap-1">
                          <Clock className="w-3 h-3" aria-hidden="true" />
                          {formatTimestamp(row.ts)}
                        </span>
                      </td>
                    </tr>
                  );
                })
              ) : (
                <tr>
                  <td colSpan={9} className="py-8 px-4 text-center text-slate-400 font-sans text-xs">
                    {loading ? (
                      <div className="flex items-center justify-center gap-2">
                        <RefreshCw className="w-4 h-4 text-amber-400 animate-spin" aria-hidden="true" />
                        <span>Cargando evaluaciones en sombra...</span>
                      </div>
                    ) : (
                      <div className="flex flex-col items-center justify-center gap-3 max-w-md mx-auto">
                        <p className="text-slate-300">
                          {endpointStatus === 'forbidden'
                            ? 'Requiere credencial de operador para acceder al log de decisiones en sombra.'
                            : 'No hay evaluaciones en sombra registradas aún. Las evaluaciones aparecerán aquí cuando [decision] registre comparaciones.'}
                        </p>
                        <button
                          type="button"
                          onClick={fetchShadowLog}
                          aria-label="Reintentar consulta de evaluaciones en sombra"
                          className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 font-mono text-xs transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none"
                        >
                          <RefreshCw className="w-3.5 h-3.5 text-amber-400" aria-hidden="true" />
                          Reintentar
                        </button>
                      </div>
                    )}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

export default DecisionServicePanel;
