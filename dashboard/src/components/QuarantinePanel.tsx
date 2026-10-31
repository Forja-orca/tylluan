import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  ShieldAlert,
  AlertTriangle,
  RefreshCw,
  CheckCircle2,
  Unlock,
  Clock,
  User,
  Fingerprint,
  Info,
  Loader2,
  X,
} from 'lucide-react';
import {
  NexusBridge,
  type QuarantineNode,
  type QuarantineResponse,
} from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { withTimeout } from '../lib/timeout';

export interface QuarantinePanelProps {
  bridge: NexusBridge | null;
  className?: string;
}

export type QuarantineStatus =
  | 'ready'
  | 'loading'
  | 'empty'
  | 'forbidden'
  | 'pending_backend'
  | 'error';

export function QuarantinePanel({ bridge, className }: QuarantinePanelProps) {
  const [nodes, setNodes] = useState<QuarantineNode[]>([]);
  const [totalCount, setTotalCount] = useState<number>(0);
  const [status, setStatus] = useState<QuarantineStatus>('loading');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState<boolean>(false);

  // Modal / Confirmación de liberación
  const [confirmNode, setConfirmNode] = useState<QuarantineNode | null>(null);
  const [releasingId, setReleasingId] = useState<string | null>(null);
  const [releaseFeedback, setReleaseFeedback] = useState<{ id: string; success: boolean; msg: string } | null>(null);

  const abortControllerRef = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, []);

  const fetchQuarantine = useCallback(async (isManualRefresh = false) => {
    if (!bridge) {
      setStatus('pending_backend');
      setErrorMessage('Bridge no conectado.');
      return;
    }

    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    if (isManualRefresh) {
      setRefreshing(true);
    } else {
      setStatus('loading');
    }
    setErrorMessage(null);

    try {
      // Usamos bridge.getQuarantine si existe o bridge.fetchRaw fallback
      const fetchPromise = typeof bridge.getQuarantine === 'function'
        ? bridge.getQuarantine(50, 0)
        : bridge.fetchRaw('/api/v1/memory/quarantine?limit=50&offset=0', { signal: controller.signal });

      const res = (await withTimeout(
        fetchPromise,
        10000,
        'Request timed out after 10s al consultar /api/v1/memory/quarantine.'
      )) as QuarantineResponse;

      const items = Array.isArray(res?.nodes) ? res.nodes : [];
      const total = typeof res?.total === 'number' ? res.total : items.length;

      setNodes(items);
      setTotalCount(total);

      if (items.length === 0) {
        setStatus('empty');
      } else {
        setStatus('ready');
      }
    } catch (err: unknown) {
      if (err instanceof Error && err.name === 'AbortError') {
        return;
      }

      const msg = err instanceof Error ? err.message : String(err || '');
      const lower = msg.toLowerCase();

      if (msg.includes('403') || msg.includes('401') || lower.includes('forbidden') || lower.includes('unauthorized')) {
        setStatus('forbidden');
        setErrorMessage('La inspección y liberación de nodos en cuarentena exige rol de operador.');
      } else if (msg.includes('404') || lower.includes('not found') || lower.includes('not_found')) {
        setStatus('pending_backend');
        setErrorMessage('GET /api/v1/memory/quarantine no está disponible en el kernel en ejecución (esperando D-12 en el backend).');
      } else if (lower.includes('timeout') || lower.includes('timed out')) {
        setStatus('error');
        setErrorMessage('Tiempo de espera agotado al consultar /api/v1/memory/quarantine.');
      } else {
        setStatus('error');
        setErrorMessage(msg || 'Error al consultar /api/v1/memory/quarantine.');
      }
      setNodes([]);
      setTotalCount(0);
    } finally {
      if (abortControllerRef.current === controller) {
        abortControllerRef.current = null;
      }
      setRefreshing(false);
    }
  }, [bridge]);

  useEffect(() => {
    fetchQuarantine();
  }, [fetchQuarantine]);

  const handleReleaseNode = async (node: QuarantineNode) => {
    if (!bridge) return;
    setReleasingId(node.id);
    setReleaseFeedback(null);

    try {
      const releasePromise = typeof bridge.releaseQuarantine === 'function'
        ? bridge.releaseQuarantine(node.id)
        : bridge.fetchRaw(`/api/v1/memory/quarantine/${encodeURIComponent(node.id)}/release`, {
            method: 'POST',
          });

      const res = await withTimeout(
        releasePromise,
        10000,
        'Tiempo de espera agotado al liberar nodo de cuarentena.'
      ) as { released?: boolean; id?: string; error?: string };

      if (res?.released) {
        setReleaseFeedback({
          id: node.id,
          success: true,
          msg: `Nodo ${node.id} liberado exitosamente de la cuarentena.`,
        });
        // Remove locally from list
        setNodes(prev => prev.filter(n => n.id !== node.id));
        setTotalCount(prev => Math.max(0, prev - 1));
      } else {
        throw new Error(res?.error || 'No se pudo liberar el nodo.');
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err || '');
      setReleaseFeedback({
        id: node.id,
        success: false,
        msg: `Error al liberar: ${msg}`,
      });
    } finally {
      setReleasingId(null);
      setConfirmNode(null);
    }
  };

  const formatTimestamp = (ts: number): string => {
    if (!ts) return '—';
    try {
      const ms = ts > 1e11 ? ts : ts * 1000;
      const d = new Date(ms);
      return isNaN(d.getTime()) ? String(ts) : d.toLocaleString([], {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
      });
    } catch {
      return String(ts);
    }
  };

  return (
    <div
      role="region"
      aria-label="Panel de cuarentena de memoria"
      className={cn(
        "bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded p-5 transition-colors space-y-4",
        className
      )}
    >
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-slate-100 dark:border-slate-800/80 pb-3">
        <div className="flex items-center gap-2.5">
          <ShieldAlert className="w-5 h-5 text-amber-600 dark:text-amber-400" aria-hidden="true" />
          <div>
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100 flex items-center gap-2">
              Cuarentena de Memoria (ASI06 Judge)
              {totalCount > 0 && (
                <span className="px-2 py-0.5 text-xs font-mono font-bold rounded bg-amber-100 dark:bg-amber-950/60 text-amber-800 dark:text-amber-300 border border-amber-300 dark:border-amber-700/60">
                  {totalCount}
                </span>
              )}
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              Aislamiento de memorias sospechosas y liberación de falsos positivos (D-12)
            </p>
          </div>
        </div>

        <button
          type="button"
          onClick={() => fetchQuarantine(true)}
          disabled={status === 'loading' || refreshing}
          aria-label="Refrescar lista de cuarentena"
          className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 rounded text-xs font-medium text-slate-700 dark:text-slate-200 transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-1 focus-visible:ring-amber-500 focus-visible:outline-none self-start sm:self-auto"
        >
          <RefreshCw className={cn("w-3.5 h-3.5", (status === 'loading' || refreshing) && "animate-spin")} aria-hidden="true" />
          <span>{refreshing ? 'Actualizando...' : 'Refrescar'}</span>
        </button>
      </div>

      {/* Banner de Feedback de Liberación */}
      {releaseFeedback && (
        <div
          role="status"
          aria-live="polite"
          className={cn(
            "p-3 rounded border text-xs flex items-center justify-between gap-2",
            releaseFeedback.success
              ? "bg-emerald-50 dark:bg-emerald-950/40 border-emerald-300 dark:border-emerald-700/60 text-emerald-800 dark:text-emerald-200"
              : "bg-rose-50 dark:bg-rose-950/40 border-rose-300 dark:border-rose-700/60 text-rose-800 dark:text-rose-200"
          )}
        >
          <div className="flex items-center gap-2">
            {releaseFeedback.success ? (
              <CheckCircle2 className="w-4 h-4 text-emerald-600 dark:text-emerald-400 shrink-0" aria-hidden="true" />
            ) : (
              <AlertTriangle className="w-4 h-4 text-rose-600 dark:text-rose-400 shrink-0" aria-hidden="true" />
            )}
            <span>{releaseFeedback.msg}</span>
          </div>
          <button
            type="button"
            onClick={() => setReleaseFeedback(null)}
            aria-label="Cerrar notificación"
            className="p-1 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 rounded cursor-pointer"
          >
            <X className="w-3.5 h-3.5" aria-hidden="true" />
          </button>
        </div>
      )}

      {/* ESTADO 1: Cargando */}
      {status === 'loading' && (
        <div className="p-8 text-center space-y-3">
          <Loader2 className="w-6 h-6 animate-spin text-amber-600 dark:text-amber-400 mx-auto" aria-hidden="true" />
          <p className="text-xs text-slate-500 dark:text-slate-400">
            Cargando nodos en cuarentena...
          </p>
        </div>
      )}

      {/* ESTADO 2: 403 Requiere Operador */}
      {status === 'forbidden' && (
        <div
          role="alert"
          className="p-5 bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-900/60 rounded space-y-3"
        >
          <div className="flex items-center gap-2 text-rose-700 dark:text-rose-300 font-semibold text-sm">
            <AlertTriangle className="w-4 h-4 shrink-0" aria-hidden="true" />
            <span>requiere operador (HTTP 403)</span>
          </div>
          <p className="text-xs text-slate-600 dark:text-slate-400 leading-relaxed">
            {errorMessage || 'La inspección y liberación de nodos aislados por ASI06 exige permisos de operador. Verifica tu Bearer Token en la configuración.'}
          </p>
          <button
            type="button"
            onClick={() => fetchQuarantine(true)}
            className="px-3 py-1.5 bg-rose-100 hover:bg-rose-200 dark:bg-rose-900/40 dark:hover:bg-rose-800/60 text-rose-800 dark:text-rose-200 border border-rose-300 dark:border-rose-700/60 rounded text-xs font-medium cursor-pointer transition-colors focus-visible:ring-1 focus-visible:ring-rose-500 focus-visible:outline-none"
          >
            Reintentar verificación
          </button>
        </div>
      )}

      {/* ESTADO 3: Endpoint no disponible (404 / backend no implementado aún) */}
      {status === 'pending_backend' && (
        <div
          role="status"
          className="p-5 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900/60 rounded space-y-3"
        >
          <div className="flex items-center gap-2 text-amber-700 dark:text-amber-300 font-semibold text-sm">
            <Info className="w-4 h-4 shrink-0" aria-hidden="true" />
            <span>endpoint no disponible</span>
          </div>
          <p className="text-xs text-slate-600 dark:text-slate-400 leading-relaxed">
            {errorMessage || 'El endpoint GET /api/v1/memory/quarantine no está disponible en este kernel. Requiere la integración del módulo D-12 en backend.'}
          </p>
          <button
            type="button"
            onClick={() => fetchQuarantine(true)}
            className="px-3 py-1.5 bg-amber-100 hover:bg-amber-200 dark:bg-amber-900/40 dark:hover:bg-amber-800/60 text-amber-800 dark:text-amber-200 border border-amber-300 dark:border-amber-700/60 rounded text-xs font-medium cursor-pointer transition-colors focus-visible:ring-1 focus-visible:ring-amber-500 focus-visible:outline-none"
          >
            Reintentar
          </button>
        </div>
      )}

      {/* ESTADO 4: Error general */}
      {status === 'error' && (
        <div
          role="alert"
          className="p-5 bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-900/60 rounded space-y-3"
        >
          <div className="flex items-center gap-2 text-rose-700 dark:text-rose-300 font-semibold text-sm">
            <AlertTriangle className="w-4 h-4 shrink-0" aria-hidden="true" />
            <span>Error de consulta</span>
          </div>
          <p className="text-xs text-slate-600 dark:text-slate-400 leading-relaxed">
            {errorMessage || 'Ocurrió un error inesperado al consultar la lista de cuarentena.'}
          </p>
          <button
            type="button"
            onClick={() => fetchQuarantine(true)}
            className="px-3 py-1.5 bg-rose-100 hover:bg-rose-200 dark:bg-rose-900/40 dark:hover:bg-rose-800/60 text-rose-800 dark:text-rose-200 border border-rose-300 dark:border-rose-700/60 rounded text-xs font-medium cursor-pointer transition-colors focus-visible:ring-1 focus-visible:ring-rose-500 focus-visible:outline-none"
          >
            Reintentar
          </button>
        </div>
      )}

      {/* ESTADO 5: Vacío (Sin nodos en cuarentena) */}
      {status === 'empty' && (
        <div className="p-8 text-center bg-slate-50 dark:bg-slate-950/40 rounded border border-dashed border-slate-200 dark:border-slate-800 space-y-2">
          <CheckCircle2 className="w-8 h-8 text-emerald-500 mx-auto" aria-hidden="true" />
          <h3 className="text-sm font-semibold text-slate-800 dark:text-slate-200">
            sin nodos en cuarentena
          </h3>
          <p className="text-xs text-slate-500 dark:text-slate-400 max-w-sm mx-auto">
            El juez de ASI06 no tiene memorias aisladas actualmente. El grafo SilvaDB se encuentra limpio.
          </p>
        </div>
      )}

      {/* ESTADO 6: Con Nodos (Lista/Tabla de Cuarentena) */}
      {status === 'ready' && nodes.length > 0 && (
        <div
          tabIndex={0}
          role="region"
          aria-label="Lista de nodos en cuarentena"
          className="overflow-x-auto focus-visible:ring-1 focus-visible:ring-amber-500 focus-visible:outline-none rounded border border-slate-200 dark:border-slate-800"
        >
          <table className="w-full text-left text-xs border-collapse">
            <thead>
              <tr className="border-b border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/60 text-slate-500 dark:text-slate-400 font-semibold">
                <th className="py-2.5 px-3">Nodo ID</th>
                <th className="py-2.5 px-3">Contenido (Preview)</th>
                <th className="py-2.5 px-3">Motivo de Cuarentena</th>
                <th className="py-2.5 px-3">Origen / Procedencia</th>
                <th className="py-2.5 px-3">Agente</th>
                <th className="py-2.5 px-3">Fecha de Aislamiento</th>
                <th className="py-2.5 px-3 text-right">Acción</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100 dark:divide-slate-800/60 font-sans">
              {nodes.map((node) => (
                <tr
                  key={node.id}
                  className="hover:bg-slate-50/70 dark:hover:bg-slate-800/40 transition-colors"
                >
                  {/* ID */}
                  <td className="py-3 px-3 font-mono font-bold text-amber-700 dark:text-amber-400 whitespace-nowrap">
                    <span className="flex items-center gap-1.5" title={node.id}>
                      <Fingerprint className="w-3.5 h-3.5 opacity-70 shrink-0" aria-hidden="true" />
                      <span>{node.id.length > 16 ? `${node.id.slice(0, 16)}…` : node.id}</span>
                    </span>
                  </td>

                  {/* Content Preview */}
                  <td className="py-3 px-3 max-w-xs text-slate-700 dark:text-slate-300">
                    <p className="line-clamp-2" title={node.content_preview}>
                      {node.content_preview || '—'}
                    </p>
                  </td>

                  {/* Quarantine Reason */}
                  <td className="py-3 px-3 whitespace-nowrap">
                    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-medium bg-rose-50 dark:bg-rose-950/50 text-rose-700 dark:text-rose-300 border border-rose-200 dark:border-rose-800/60">
                      <AlertTriangle className="w-3 h-3 shrink-0" aria-hidden="true" />
                      <span>{node.quarantine_reason}</span>
                    </span>
                  </td>

                  {/* Provenance */}
                  <td className="py-3 px-3 font-mono text-[11px] text-slate-600 dark:text-slate-400 whitespace-nowrap">
                    {node.provenance || '—'}
                  </td>

                  {/* Agent ID */}
                  <td className="py-3 px-3 whitespace-nowrap text-slate-600 dark:text-slate-300">
                    <span className="flex items-center gap-1">
                      <User className="w-3 h-3 text-slate-400" aria-hidden="true" />
                      <span className="font-mono">{node.agent_id ? `@${node.agent_id}` : '—'}</span>
                    </span>
                  </td>

                  {/* Created At */}
                  <td className="py-3 px-3 whitespace-nowrap text-slate-500 dark:text-slate-400 text-[11px]">
                    <span className="flex items-center gap-1">
                      <Clock className="w-3 h-3 opacity-60" aria-hidden="true" />
                      <span>{formatTimestamp(node.created_at)}</span>
                    </span>
                  </td>

                  {/* Action: Release with Confirmation */}
                  <td className="py-3 px-3 text-right whitespace-nowrap">
                    <button
                      type="button"
                      onClick={() => setConfirmNode(node)}
                      disabled={releasingId === node.id}
                      aria-label={`Liberar nodo ${node.id}`}
                      className="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-semibold rounded bg-emerald-50 hover:bg-emerald-100 dark:bg-emerald-950/40 dark:hover:bg-emerald-900/60 text-emerald-700 dark:text-emerald-300 border border-emerald-300 dark:border-emerald-700/60 transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-1 focus-visible:ring-emerald-500 focus-visible:outline-none"
                    >
                      {releasingId === node.id ? (
                        <>
                          <Loader2 className="w-3 h-3 animate-spin" aria-hidden="true" />
                          <span>Liberando...</span>
                        </>
                      ) : (
                        <>
                          <Unlock className="w-3 h-3" aria-hidden="true" />
                          <span>Liberar</span>
                        </>
                      )}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* DIÁLOGO / MODAL ACCESIBLE DE CONFIRMACIÓN */}
      {confirmNode && (
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby="confirm-release-title"
          className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/70"
        >
          <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded max-w-md w-full p-6 shadow-none space-y-4 text-slate-900 dark:text-slate-100">
            <div className="flex items-center gap-3 text-amber-600 dark:text-amber-400">
              <AlertTriangle className="w-6 h-6 shrink-0" aria-hidden="true" />
              <h3 id="confirm-release-title" className="text-base font-bold">
                Confirmar Liberación de Nodo
              </h3>
            </div>

            <p className="text-xs text-slate-600 dark:text-slate-300 leading-relaxed">
              ¿Confirmas que este nodo fue aislado como un falso positivo y debe ser reincorporado al grafo SilvaDB activo?
            </p>

            <div className="bg-slate-50 dark:bg-slate-950/60 border border-slate-200 dark:border-slate-800 rounded p-3 space-y-1.5 text-xs">
              <div>
                <span className="text-slate-400">ID:</span>{' '}
                <span className="font-mono font-bold text-amber-600 dark:text-amber-400">{confirmNode.id}</span>
              </div>
              <div>
                <span className="text-slate-400">Motivo:</span>{' '}
                <span className="text-rose-600 dark:text-rose-400 font-medium">{confirmNode.quarantine_reason}</span>
              </div>
              <div>
                <span className="text-slate-400">Preview:</span>{' '}
                <span className="italic text-slate-700 dark:text-slate-300">"{confirmNode.content_preview}"</span>
              </div>
            </div>

            <div className="flex items-center justify-end gap-2.5 pt-2 border-t border-slate-100 dark:border-slate-800">
              <button
                type="button"
                onClick={() => setConfirmNode(null)}
                disabled={releasingId === confirmNode.id}
                className="px-3.5 py-1.5 rounded text-xs font-semibold text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800 transition cursor-pointer disabled:opacity-50 focus-visible:ring-1 focus-visible:ring-slate-400 focus-visible:outline-none"
              >
                Cancelar
              </button>
              <button
                type="button"
                onClick={() => handleReleaseNode(confirmNode)}
                disabled={releasingId === confirmNode.id}
                className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded text-xs font-semibold bg-emerald-600 hover:bg-emerald-500 text-white transition cursor-pointer disabled:opacity-50 focus-visible:ring-1 focus-visible:ring-emerald-400 focus-visible:outline-none"
              >
                {releasingId === confirmNode.id && (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
                )}
                <span>Confirmar liberación</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
