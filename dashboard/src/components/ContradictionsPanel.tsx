import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import {
  AlertTriangle,
  RefreshCw,
  Search,
  GitCompare,
  Clock,
  ShieldAlert,
  SlidersHorizontal
} from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { withTimeout } from '../lib/timeout';

export interface ConflictedNodeItem {
  id: string;
  node_type: string;
  content_excerpt: string;
  conflicted_since?: string | number | null;
}

export interface SilvaConflictedResponse {
  nodes: ConflictedNodeItem[];
  total_conflicted: number;
  limit: number;
}

export interface ContradictionsPanelProps {
  bridge: NexusBridge | null;
  notify?: (msg: string, type?: 'info' | 'error') => void;
}

const PAGE_SIZE = 20;

export function ContradictionsPanel({ bridge, notify }: ContradictionsPanelProps) {
  const [nodes, setNodes] = useState<ConflictedNodeItem[]>([]);
  const [totalConflicted, setTotalConflicted] = useState<number>(0);
  const [limit, setLimit] = useState<number>(50);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [endpointStatus, setEndpointStatus] = useState<'ready' | 'pending_backend' | 'forbidden' | 'error'>('ready');
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [page, setPage] = useState<number>(1);
  const abortControllerRef = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, []);

  const fetchConflicted = useCallback(async () => {
    if (!bridge) return;
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;
    setLoading(true);
    setError(null);
    try {
      const safeLimit = Math.min(200, Math.max(1, limit));
      const res = (await withTimeout(
        bridge.fetchRaw(`/api/v1/silva/conflicted?limit=${safeLimit}`, { signal: controller.signal }),
        10000,
        'Request timed out after 10s. GET /api/v1/silva/conflicted took too long.'
      )) as SilvaConflictedResponse;
      const validNodes = Array.isArray(res?.nodes)
        ? res.nodes.map((n) => ({
            id: String(n.id),
            node_type: String(n.node_type || 'Semantic'),
            content_excerpt: String(n.content_excerpt ?? '').slice(0, 300),
            conflicted_since: n.conflicted_since ?? null,
          }))
        : [];
      setNodes(validNodes);
      setTotalConflicted(
        typeof res?.total_conflicted === 'number' ? res.total_conflicted : validNodes.length
      );
      setEndpointStatus('ready');
      setPage(1);
    } catch (err: unknown) {
      if (err instanceof Error && err.name === 'AbortError') {
        setError('Consulta cancelada por el usuario.');
        setEndpointStatus('error');
        setNodes([]);
        setTotalConflicted(0);
        return;
      }
      const msg = err instanceof Error ? err.message : String(err || '');
      if (msg.includes('404') || msg.toLowerCase().includes('not found')) {
        setEndpointStatus('pending_backend');
        setError(
          'GET /api/v1/silva/conflicted no está montado aún en el kernel en ejecución.'
        );
      } else if (msg.includes('403') || msg.includes('401') || msg.toLowerCase().includes('forbidden') || msg.toLowerCase().includes('unauthorized')) {
        setEndpointStatus('forbidden');
        setError('Requiere credencial de operador (HTTP 403). Token maestro requerido para consultar GET /api/v1/silva/conflicted.');
      } else if (msg.includes('timed out') || msg.includes('timeout')) {
        setEndpointStatus('error');
        setError('Request timed out after 10s. La consulta a /api/v1/silva/conflicted excedió el tiempo límite.');
      } else {
        setEndpointStatus('error');
        setError(msg || 'Error al consultar GET /api/v1/silva/conflicted');
        if (notify) notify('Error consultando nodos en conflicto en SilvaDB', 'error');
      }
      setNodes([]);
      setTotalConflicted(0);
    } finally {
      if (abortControllerRef.current === controller) {
        abortControllerRef.current = null;
      }
      setLoading(false);
    }
  }, [bridge, limit, notify]);

  useEffect(() => {
    fetchConflicted();
  }, [fetchConflicted]);

  const filteredNodes = useMemo(() => {
    if (!searchQuery.trim()) return nodes;
    const q = searchQuery.toLowerCase();
    return nodes.filter(
      (item) =>
        item.id.toLowerCase().includes(q) ||
        item.content_excerpt.toLowerCase().includes(q) ||
        item.node_type.toLowerCase().includes(q)
    );
  }, [nodes, searchQuery]);

  const totalPages = Math.max(1, Math.ceil(filteredNodes.length / PAGE_SIZE));
  const paginatedNodes = useMemo(() => {
    const start = (page - 1) * PAGE_SIZE;
    return filteredNodes.slice(start, start + PAGE_SIZE);
  }, [filteredNodes, page]);

  return (
    <section
      aria-labelledby="contradictions-panel-heading"
      className="bg-slate-900/70 border border-slate-800 rounded-2xl p-6 space-y-5 font-sans"
    >
      {/* Header */}
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 border-b border-slate-800 pb-4">
        <div>
          <div className="flex flex-wrap items-center gap-2 mb-1.5">
            <span className="px-2 py-0.5 text-xs font-mono font-semibold bg-rose-500/15 text-rose-300 border border-rose-500/30 rounded-md">
              SilvaDB Epistemic Audit
            </span>
            <span className="px-2 py-0.5 text-xs font-mono text-slate-300 bg-slate-800 border border-slate-700 rounded-md">
              Total conflicted: {totalConflicted}
            </span>
          </div>
          <h2
            id="contradictions-panel-heading"
            className="text-xl font-bold text-slate-100 flex items-center gap-2"
          >
            <GitCompare className="w-5 h-5 text-rose-400" aria-hidden="true" />
            Conflicted Nodes (`/api/v1/silva/conflicted`)
          </h2>
          <p className="text-xs text-slate-300 mt-1">
            Read-only operator view of SilvaDB nodes with `conflicted = 1` (`get_all_conflicted`, excerpt &le; 300 chars).
          </p>
        </div>

        <div className="flex items-center gap-2">
          <label htmlFor="conflicted-limit-select" className="text-xs font-mono text-slate-300 flex items-center gap-1">
            <SlidersHorizontal className="w-3.5 h-3.5 text-slate-400" aria-hidden="true" />
            Limit:
          </label>
          <select
            id="conflicted-limit-select"
            aria-label="Number of conflicted nodes to fetch (max 200)"
            value={limit}
            onChange={(e) => setLimit(Math.min(200, Number(e.target.value)))}
            className="bg-slate-950 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs font-mono text-slate-200 focus:border-amber-400 focus:outline-none"
          >
            <option value={25}>25</option>
            <option value={50}>50</option>
            <option value={100}>100</option>
            <option value={200}>200</option>
          </select>

          <button
            type="button"
            onClick={fetchConflicted}
            disabled={loading}
            aria-label="Refresh conflicted nodes list"
            className="flex items-center gap-1.5 px-3.5 py-2 bg-slate-800 hover:bg-slate-700 focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none text-xs text-slate-200 font-medium rounded-xl transition-colors disabled:opacity-50 cursor-pointer"
          >
            <RefreshCw className={cn('w-3.5 h-3.5 text-rose-400', loading && 'animate-spin')} aria-hidden="true" />
            <span>{loading ? 'Loading...' : 'Refresh'}</span>
          </button>
          {loading && (
            <button
              type="button"
              onClick={() => {
                if (abortControllerRef.current) {
                  abortControllerRef.current.abort();
                  abortControllerRef.current = null;
                }
                setLoading(false);
                setError('Consulta cancelada por el usuario.');
                setEndpointStatus('error');
              }}
              aria-label="Cancel conflicted nodes query"
              className="flex items-center gap-1 px-3 py-2 bg-rose-500/10 hover:bg-rose-500/20 border border-rose-500/30 text-rose-300 text-xs font-medium rounded-xl transition-colors cursor-pointer"
            >
              Cancel
            </button>
          )}
        </div>
      </div>

      {/* Mandatory High-FP Warning Banner (D2) */}
      <div
        role="note"
        aria-label="Advertencia sobre falsos positivos en detección estructural de contradicciones"
        className="p-4 rounded-xl bg-amber-500/10 border border-amber-500/30 flex items-start gap-3"
      >
        <AlertTriangle className="w-5 h-5 text-amber-400 shrink-0 mt-0.5" aria-hidden="true" />
        <div className="text-xs text-slate-200 leading-relaxed space-y-1">
          <p>
            <span className="font-bold text-amber-300 uppercase tracking-wide mr-1.5">
              Nota de calibración:
            </span>
            La detección actual es estructural; sin contradicciones reales en la muestra etiquetada a mano por Buffy (`benchmarks/human_truth_conflict_pairs_v1.json`: 100 pares = 91 consistentes, 9 no relacionados, 0 que se contradigan).
          </p>
          <p className="text-slate-300">
            Este panel consulta exclusivamente `GET /api/v1/silva/conflicted?limit=N` (nodos con `conflicted = 1`). Los nodos obsoletos con marcador `[DEPRECATED by …]` de `/api/v1/silva/contradictions` no son contradicciones y se excluyen deliberadamente.
          </p>
        </div>
      </div>

      {/* Honest Pending / Forbidden / Error Banner (sin aria-live en contenido sondeado) */}
      {error && (
        <div
          className={cn(
            'p-4 rounded-xl border text-xs flex items-center justify-between gap-3',
            endpointStatus === 'forbidden'
              ? 'bg-amber-500/10 border-amber-500/40 text-amber-200'
              : endpointStatus === 'pending_backend'
              ? 'bg-slate-950/90 border-slate-700 text-slate-300'
              : 'bg-rose-500/10 border-rose-500/30 text-rose-300'
          )}
        >
          <div className="flex items-center gap-2.5">
            <ShieldAlert className={cn('w-4 h-4 shrink-0', endpointStatus === 'forbidden' ? 'text-amber-400' : 'text-rose-400')} aria-hidden="true" />
            <span className="font-mono text-slate-100">{error}</span>
          </div>
          <button
            type="button"
            onClick={fetchConflicted}
            aria-label="Reintentar consulta de nodos en conflicto"
            className="px-3.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 font-mono text-xs cursor-pointer focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none shrink-0"
          >
            Reintentar
          </button>
        </div>
      )}

      {/* Search Bar */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3 bg-slate-950/60 p-3.5 rounded-xl border border-slate-800">
        <div className="relative flex-1 max-w-md">
          <Search className="w-3.5 h-3.5 text-slate-400 absolute left-3 top-2.5" aria-hidden="true" />
          <input
            type="text"
            aria-label="Filter conflicted nodes by ID, excerpt, or node type"
            value={searchQuery}
            onChange={(e) => {
              setSearchQuery(e.target.value);
              setPage(1);
            }}
            placeholder="Search by node ID, content excerpt, or node_type..."
            className="w-full pl-9 pr-3 py-1.5 bg-slate-900 border border-slate-800 rounded-lg text-xs text-slate-200 placeholder-slate-400 font-mono focus:border-amber-400 focus:outline-none"
          />
        </div>
        <span className="text-xs font-mono text-slate-400">
          Showing {filteredNodes.length} of {nodes.length} loaded (total_conflicted: {totalConflicted})
        </span>
      </div>

      {/* Node List */}
      <div className="space-y-2.5">
        {paginatedNodes.length > 0 ? (
          paginatedNodes.map((node) => (
            <div
              key={node.id}
              className="p-4 rounded-xl bg-slate-950/70 border border-slate-800 hover:border-slate-700 transition-colors space-y-2"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2 font-mono text-xs">
                  <span className="font-bold text-amber-300">{node.id}</span>
                  <span className="px-2 py-0.5 rounded bg-slate-900 text-slate-300 border border-slate-800">
                    {node.node_type}
                  </span>
                  <span className="px-2 py-0.5 rounded bg-rose-500/15 text-rose-300 border border-rose-500/30 font-semibold">
                    conflicted=1
                  </span>
                </div>
                {node.conflicted_since && (
                  <span className="text-xs font-mono text-slate-400 flex items-center gap-1">
                    <Clock className="w-3 h-3" aria-hidden="true" />
                    {String(node.conflicted_since)}
                  </span>
                )}
              </div>
              <p className="text-xs text-slate-200 font-mono leading-relaxed break-words bg-slate-900/60 p-3 rounded-lg border border-slate-800/80">
                {node.content_excerpt || '(Empty excerpt)'}
              </p>
            </div>
          ))
        ) : (
          <div className="py-10 px-4 text-center rounded-xl bg-slate-950/40 border border-slate-800/70 space-y-3 max-w-lg mx-auto">
            <GitCompare className="w-8 h-8 text-slate-400 mx-auto" aria-hidden="true" />
            <p className="text-sm font-semibold text-slate-200">
              {loading ? (
                <span className="inline-flex items-center gap-2">
                  <RefreshCw className="w-4 h-4 text-rose-400 animate-spin" aria-hidden="true" />
                  Consultando nodos en conflicto en SilvaDB...
                </span>
              ) : endpointStatus === 'pending_backend' ? (
                'Esperando montaje de GET /api/v1/silva/conflicted en el kernel'
              ) : endpointStatus === 'forbidden' ? (
                'Requiere credencial de operador (HTTP 403)'
              ) : (
                'No se encontraron nodos en conflicto'
              )}
            </p>
            <p className="text-xs text-slate-400 max-w-lg mx-auto">
              Solo se muestran nodos con `conflicted = 1` de `GET /api/v1/silva/conflicted`. Los registros obsoletos con `[DEPRECATED by …]` se excluyen deliberadamente.
            </p>
            {!loading && (
              <button
                type="button"
                onClick={fetchConflicted}
                aria-label="Reintentar consulta de nodos en conflicto"
                className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 font-mono text-xs transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none"
              >
                <RefreshCw className="w-3.5 h-3.5 text-rose-400" aria-hidden="true" />
                Reintentar
              </button>
            )}
          </div>
        )}
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between pt-2 border-t border-slate-800 text-xs font-mono text-slate-300">
          <span>
            Page {page} of {totalPages} ({filteredNodes.length} items)
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={page <= 1}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              className="px-3 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 disabled:opacity-40 cursor-pointer"
            >
              Prev
            </button>
            <button
              type="button"
              disabled={page >= totalPages}
              onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
              className="px-3 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 disabled:opacity-40 cursor-pointer"
            >
              Next
            </button>
          </div>
        </div>
      )}
    </section>
  );
}

export default ContradictionsPanel;
