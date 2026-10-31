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

  const containerRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && loading) {
        const active = document.activeElement;
        const target = e.target as Node;
        const isInside = containerRef.current && (
          containerRef.current.contains(active) || containerRef.current.contains(target)
        );
        if (!isInside) return;

        if (abortControllerRef.current) {
          abortControllerRef.current.abort();
          abortControllerRef.current = null;
        }
        setLoading(false);
        setError('Consulta cancelada por el usuario.');
        setEndpointStatus('error');
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [loading]);

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
      ref={containerRef}
      aria-labelledby="contradictions-panel-heading"
      className="bg-[#ECE9D8] dark:bg-card border border-[#ACA899] dark:border-border rounded-sm p-3 space-y-3 font-sans min-w-0 max-w-full overflow-hidden"
    >
      {/* Header - Windows 2000/XP Style */}
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-3 border-b border-[#ACA899] dark:border-border pb-3 min-w-0">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2 mb-1">
            <span className="px-1.5 py-0.2 text-[10px] font-mono font-bold bg-white dark:bg-muted text-rose-700 dark:text-rose-400 border border-[#7F9DB9] dark:border-border">
              Auditoría Epistémica SilvaDB
            </span>
            <span className="px-1.5 py-0.2 text-[10px] font-mono font-bold bg-white dark:bg-muted text-[#000000] dark:text-foreground border border-[#7F9DB9] dark:border-border">
              Total en conflicto: {totalConflicted}<span className="sr-only"> (Total conflicted: {totalConflicted})</span>
            </span>
          </div>
          <h2
            id="contradictions-panel-heading"
            className="text-xs font-bold text-[#0A246A] dark:text-primary flex items-center gap-1.5 flex-wrap"
          >
            <GitCompare className="w-4 h-4 text-rose-700 shrink-0" aria-hidden="true" />
            <span>Nodos en Conflicto Cognitivo <span className="text-[11px] font-mono font-normal text-[#555555] dark:text-muted-foreground">(/api/v1/silva/conflicted)</span></span>
          </h2>
          <p className="text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5">
            Vista de operador de nodos SilvaDB con conflicted = 1 (extracto &le; 300 caracteres).
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2 min-w-0">
          <label htmlFor="conflicted-limit-select" className="text-[11px] font-bold text-[#01285F] dark:text-foreground flex items-center gap-1">
            <SlidersHorizontal className="w-3 h-3 text-[#555555]" aria-hidden="true" />
            Límite:
          </label>
          <select
            id="conflicted-limit-select"
            aria-label="Número de nodos en conflicto a consultar"
            value={limit}
            onChange={(e) => setLimit(Math.min(200, Number(e.target.value)))}
            className="bg-white dark:bg-muted border border-[#7F9DB9] dark:border-border rounded-none px-2 py-0.5 text-[11px] font-mono text-[#000000] dark:text-foreground focus:outline-none"
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
            aria-label="Actualizar lista de nodos en conflicto"
            className="flex items-center gap-1 px-2.5 py-1 bg-white dark:bg-muted hover:bg-[#F5F4EA] text-[11px] font-bold text-[#000000] dark:text-foreground rounded-sm border border-[#ACA899] dark:border-border transition-colors disabled:opacity-50 cursor-pointer"
          >
            <RefreshCw className={cn('w-3 h-3 text-[#0A246A]', loading && 'animate-spin')} aria-hidden="true" />
            <span>{loading ? 'Cargando...' : 'Actualizar'}</span>
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
              className="flex items-center gap-1 px-2 py-1 bg-white hover:bg-rose-50 border border-rose-300 text-rose-700 text-[11px] font-bold rounded-sm transition-colors cursor-pointer"
            >
              Cancelar
            </button>
          )}
        </div>
      </div>

      {/* Mandatory Calibration Warning Banner */}
      <div
        role="note"
        aria-label="Advertencia sobre falsos positivos en detección estructural de contradicciones"
        className="p-2.5 rounded-sm bg-white dark:bg-muted/40 border border-[#ACA899] dark:border-border flex items-start gap-2 text-[11px]"
      >
        <AlertTriangle className="w-4 h-4 text-[#B8860B] shrink-0 mt-0.5" aria-hidden="true" />
        <div className="min-w-0 text-[#000000] dark:text-foreground leading-normal space-y-0.5 break-words">
          <p>
            <span className="font-bold text-[#0A246A] dark:text-primary mr-1">
              Nota de calibración:
            </span>
            La detección actual es estructural; sin contradicciones reales en la muestra etiquetada a mano por Buffy (<code className="font-mono">benchmarks/human_truth_conflict_pairs_v1.json</code>: 100 pares = 91 consistentes, 9 no relacionados, 0 que se contradigan).
          </p>
          <p className="text-[#444444] dark:text-muted-foreground text-[10px]">
            Este panel consulta exclusivamente <code className="font-mono">GET /api/v1/silva/conflicted?limit=N</code> (nodos con <code className="font-mono">conflicted = 1</code>). Los nodos obsoletos con marcador <code className="font-mono">[DEPRECATED by …]</code> se excluyen deliberadamente.
          </p>
        </div>
      </div>

      {/* Honest Pending / Forbidden / Error Banner (sin aria-live en contenido sondeado) */}
      {error && (
        <div
          className={cn(
            'p-4 rounded border text-xs flex items-center justify-between gap-3',
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
            className="px-3.5 py-1.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-200 font-mono text-xs cursor-pointer focus-visible:ring-1 focus-visible:ring-amber-400 focus-visible:outline-none shrink-0"
          >
            Reintentar
          </button>
        </div>
      )}

      {/* Search Bar */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3 bg-slate-950/60 p-3.5 rounded border border-slate-800">
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
            placeholder="Buscar por identificador, texto o tipo de nodo (Search by node ID, content excerpt, or node_type)..."
            className="w-full pl-8 pr-2.5 py-1 bg-white dark:bg-card border border-[#7F9DB9] dark:border-border rounded-none text-[11px] text-[#000000] dark:text-foreground placeholder-[#888888] font-mono focus:outline-none"
          />
        </div>
        <span className="text-[11px] font-mono text-[#555555] dark:text-muted-foreground">
          Mostrando {filteredNodes.length} de {nodes.length} cargados (total en conflicto: {totalConflicted})
        </span>
      </div>

      {/* Node List */}
      <div className="space-y-2">
        {paginatedNodes.length > 0 ? (
          paginatedNodes.map((node) => (
            <div
              key={node.id}
              className="p-2.5 rounded-none bg-white dark:bg-card border border-[#ACA899] dark:border-border hover:bg-[#F5F4EA] dark:hover:bg-muted/30 transition-colors space-y-1.5"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-1.5 font-mono text-[11px]">
                  <span className="font-bold text-[#0A246A] dark:text-primary break-all">{node.id}</span>
                  <span className="px-1.5 py-0.2 rounded-none bg-[#ECE9D8] dark:bg-muted text-[#000000] dark:text-foreground border border-[#ACA899]">
                    {node.node_type}
                  </span>
                  <span className="px-1.5 py-0.2 rounded-none bg-rose-500/15 text-rose-700 dark:text-rose-300 border border-rose-500/30 font-bold">
                    conflicted=1
                  </span>
                </div>
                {node.conflicted_since && (
                  <span className="text-[10px] font-mono text-[#555555] dark:text-muted-foreground flex items-center gap-1">
                    <Clock className="w-3 h-3" aria-hidden="true" />
                    {String(node.conflicted_since)}
                  </span>
                )}
              </div>
              <p className="text-[11px] text-[#222222] dark:text-slate-200 font-mono leading-relaxed break-words bg-[#ECE9D8] dark:bg-muted/40 p-2 rounded-none border border-[#ACA899]">
                {node.content_excerpt || '(Extracto vacío)'}
              </p>
            </div>
          ))
        ) : (
          <div className="py-8 px-4 text-center rounded-none bg-white dark:bg-card border border-[#ACA899] dark:border-border space-y-2 max-w-lg mx-auto">
            <GitCompare className="w-6 h-6 text-[#7F9DB9] mx-auto" aria-hidden="true" />
            <p className="text-xs font-bold text-[#0A246A] dark:text-foreground">
              {loading ? (
                <span className="inline-flex items-center gap-2">
                  <RefreshCw className="w-3.5 h-3.5 text-[#0A246A] animate-spin" aria-hidden="true" />
                  Consultando nodos en conflicto en SilvaDB...
                </span>
              ) : endpointStatus === 'pending_backend' ? (
                'Esperando montaje de GET /api/v1/silva/conflicted en el kernel'
              ) : endpointStatus === 'forbidden' ? (
                'Requiere credencial de operador (HTTP 403)'
              ) : (
                'No se encontraron nodos en conflicto en SilvaDB'
              )}
            </p>
            <p className="text-[11px] text-[#444444] dark:text-muted-foreground max-w-md mx-auto">
              Solo se muestran nodos con conflicted = 1. Los registros obsoletos con [DEPRECATED by …] se excluyen deliberadamente.
            </p>
            {!loading && (
              <button
                type="button"
                onClick={fetchConflicted}
                aria-label="Reintentar consulta de nodos en conflicto"
                className="inline-flex items-center gap-1.5 px-3 py-1 rounded-sm bg-[#ECE9D8] hover:bg-[#F5F4EA] text-[#000000] dark:text-foreground font-bold text-[11px] border border-[#ACA899] transition-colors cursor-pointer"
              >
                <RefreshCw className="w-3 h-3 text-[#0A246A]" aria-hidden="true" />
                Reintentar
              </button>
            )}
          </div>
        )}
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between pt-2 border-t border-[#ACA899] dark:border-border text-[11px] font-mono text-[#444444] dark:text-muted-foreground">
          <span>
            Página {page} de {totalPages} ({filteredNodes.length} elementos)
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={page <= 1}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              className="px-3 py-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-40 cursor-pointer"
            >
              Prev
            </button>
            <button
              type="button"
              disabled={page >= totalPages}
              onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
              className="px-3 py-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-40 cursor-pointer"
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
