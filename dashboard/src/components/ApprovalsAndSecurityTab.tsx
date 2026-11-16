import React, { useState, useEffect, useCallback } from 'react';
import type { Approval, NexusBridge } from '../lib/nexus-bridge';
import { ShieldCheck, Clock, RefreshCw, AlertTriangle } from 'lucide-react';
import { cn } from '../lib/utils';
import { useAppStore } from '../stores/useAppStore';
import { TrustConsoleTab } from './TrustConsoleTab';
import { QuarantinePanel } from './QuarantinePanel';
import AuditTrailPanel from './AuditTrailPanel';
import CoherenceGatePanel from './CoherenceGatePanel';
import FrictionPanel from './FrictionPanel';
import { DecisionServicePanel } from './DecisionServicePanel';

export interface UnifiedApprovalItem extends Approval {
  kind?: 'grant' | 'plan' | string;
  tool_name?: string;
  agent_id?: string;
  intent?: string;
  summary?: string;
  state?: 'pending' | 'held' | string;
  held?: boolean;
  can_approve?: boolean;
  can_hold?: boolean;
  can_withdraw?: boolean;
}

interface ApprovalsApiResponse {
  approvals?: UnifiedApprovalItem[];
  count?: number;
  judgement?: {
    held?: number;
    withdrawn?: number;
  };
}

interface ApprovalsAndSecurityTabProps {
  bridge: NexusBridge | null;
  approvals?: UnifiedApprovalItem[];
  onOpenAuthModal?: () => void;
}

type SubSection = 'all' | 'trust' | 'quarantine' | 'audit' | 'decision';

export function ApprovalsAndSecurityTab({
  bridge,
  approvals = [],
  onOpenAuthModal,
}: ApprovalsAndSecurityTabProps) {
  const [activeSection, setActiveSection] = useState<SubSection>('all');
  const [remoteApprovals, setRemoteApprovals] = useState<UnifiedApprovalItem[] | null>(null);
  const [judgement, setJudgement] = useState<{ held?: number; withdrawn?: number } | null>(null);
  const [loadingApprovals, setLoadingApprovals] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [authRequired, setAuthRequired] = useState(false);
  const [holdTargetId, setHoldTargetId] = useState<string | null>(null);
  const [holdReason, setHoldReason] = useState('');
  const [holdError, setHoldError] = useState<string | null>(null);
  const [actionBusyId, setActionBusyId] = useState<string | null>(null);
  const [actionFeedback, setActionFeedback] = useState<string | null>(null);
  const [devModeActive, setDevModeActive] = useState(false);

  useEffect(() => {
    let cancelled = false;
    if (!bridge) {
      setDevModeActive(false);
      return;
    }
    void (async () => {
      try {
        const res = await bridge.fetchRaw('/api/v1/setup-hint');
        if (!res || !res.ok) return;
        const data = (await res.json()) as { auth_required?: boolean };
        if (!cancelled && typeof data?.auth_required === 'boolean') {
          setDevModeActive(data.auth_required === false);
        }
      } catch {
        // Ignorar fallo de lectura auxiliar
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [bridge]);

  const handleOpenTokenModal = () => {
    if (onOpenAuthModal) {
      onOpenAuthModal();
      return;
    }
    const headerBtn = document.querySelector<HTMLButtonElement>('[aria-label="Configurar o copiar Bearer Token"]');
    headerBtn?.click();
  };

  const loadApprovals = useCallback(async () => {
    if (!bridge) {
      setRemoteApprovals(null);
      setAuthRequired(false);
      return;
    }
    setLoadingApprovals(true);
    setFetchError(null);
    setAuthRequired(false);
    try {
      const res = await bridge.fetchRaw('/api/v1/approvals');
      if (res.status === 401) {
        setAuthRequired(true);
        setFetchError('Autenticación requerida (HTTP 401): configura tu Bearer Token para consultar las aprobaciones pendientes.');
        return;
      }
      if (!res.ok) {
        throw new Error(`HTTP ${res.status} al consultar /api/v1/approvals`);
      }
      const data = (await res.json()) as ApprovalsApiResponse;
      if (Array.isArray(data?.approvals)) {
        setRemoteApprovals(data.approvals);
      }
      if (data?.judgement) {
        setJudgement(data.judgement);
      }
    } catch (err) {
      setFetchError(err instanceof Error ? err.message : 'Error al consultar /api/v1/approvals');
    } finally {
      setLoadingApprovals(false);
    }
  }, [bridge]);

  useEffect(() => {
    void loadApprovals();
  }, [loadApprovals]);

  const items = remoteApprovals ?? approvals;

  const runAction = async (id: string, action: 'approve' | 'hold' | 'withdraw', body?: Record<string, unknown>) => {
    if (!bridge) return;
    setActionBusyId(id);
    setActionFeedback(null);
    try {
      const res = await bridge.fetchRaw(`/api/v1/approvals/${encodeURIComponent(id)}/${action}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: body ? JSON.stringify(body) : undefined,
      });
      if (!res.ok) {
        if (res.status === 403) {
          setActionFeedback(`Permiso denegado (HTTP 403) al ejecutar ${action} sobre ${id}.`);
        } else if (res.status === 409) {
          setActionFeedback(`Conflicto de estado (HTTP 409) al ejecutar ${action} sobre ${id}: la acción ya fue procesada o cambió de estado.`);
        } else {
          setActionFeedback(`Error HTTP ${res.status} al ejecutar ${action} sobre ${id}.`);
        }
        return;
      }
      if (action === 'hold') {
        setHoldTargetId(null);
        setHoldReason('');
        setHoldError(null);
      }
      await loadApprovals();
    } catch (err) {
      setActionFeedback(err instanceof Error ? err.message : `Error al ejecutar ${action}.`);
    } finally {
      setActionBusyId(null);
    }
  };

  const handleConfirmHold = (id: string) => {
    const trimmed = holdReason.trim();
    if (!trimmed) {
      setHoldError('El motivo es obligatorio para poner en duda.');
      return;
    }
    setHoldError(null);
    void runAction(id, 'hold', { reason: trimmed });
  };

  return (
    <div className="space-y-3 font-sans">
      {/* Subheader and Property Sheet Tabs */}
      <div className="border border-border bg-card p-3.5 rounded-lg shadow-sm">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2.5 border-b border-border">
          <div>
            <h2 className="text-xs font-bold text-foreground flex items-center gap-1.5">
              <ShieldCheck className="w-4 h-4 text-primary" aria-hidden="true" />
              Aprobaciones, Seguridad y Cuarentena
            </h2>
            <p className="text-[11px] text-muted-foreground mt-0.5">
              Revisa acciones que piden tu permiso, elementos aislados por seguridad y el historial de actividad.
            </p>
          </div>
        </div>

        {/* Property Sheet Tabs */}
        <div role="tablist" aria-label="Secciones de seguridad" className="flex flex-wrap items-center gap-1.5 mt-2.5 pt-1 border-t border-border">
          {[
            { id: 'all' as SubSection, label: 'Vista Completa' },
            { id: 'trust' as SubSection, label: 'Consola de Confianza' },
            { id: 'quarantine' as SubSection, label: 'Cuarentena' },
            { id: 'audit' as SubSection, label: 'Registro de Auditoría' },
            { id: 'decision' as SubSection, label: 'Servicio de Decisión' },
          ].map((tab) => {
            const isActive = activeSection === tab.id;
            return (
              <button
                key={tab.id}
                role="tab"
                aria-selected={isActive}
                type="button"
                onClick={() => setActiveSection(tab.id)}
                className={cn(
                  "px-3.5 py-1.5 text-[11px] font-bold border transition-all cursor-pointer rounded-md",
                  isActive
                    ? "bg-primary/10 border-primary/40 text-primary shadow-sm"
                    : "bg-muted/50 border-border text-muted-foreground hover:bg-muted hover:text-foreground"
                )}
              >
                {tab.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* Aviso de solo lectura cuando dev_mode=true (auth_required === false en /api/v1/setup-hint) */}
      {devModeActive && (
        <div
          role="status"
          aria-label="Aviso de modo desarrollo"
          className="p-3 rounded-lg border border-amber-500/40 bg-amber-500/10 flex items-start gap-2.5 text-xs"
        >
          <AlertTriangle className="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" aria-hidden="true" />
          <div className="space-y-0.5">
            <p className="font-semibold text-amber-800 dark:text-amber-200">
              Modo desarrollo activo (dev_mode = true)
            </p>
            <p className="text-amber-700 dark:text-amber-300">
              Este kernel acepta peticiones sin token Bearer. No uses esta configuración en un entorno compartido o expuesto.
            </p>
          </div>
        </div>
      )}

      {/* Pendiente de aprobación (HITL / acciones bloqueadas) */}
      <section
        aria-labelledby="pending-approvals-heading"
        className="border border-border bg-card p-3.5 rounded-lg shadow-sm space-y-2.5"
      >
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 id="pending-approvals-heading" className="text-xs font-bold text-foreground flex flex-wrap items-center gap-1.5">
            <Clock className="w-3.5 h-3.5 text-amber-500" aria-hidden="true" />
            <span>Pendiente de tu aprobación</span>
            <span className="px-1.5 py-0.5 rounded bg-muted text-[10px] font-mono text-muted-foreground border border-border">
              {items.length}
            </span>
            {judgement && (
              <span className="text-[10px] font-mono text-muted-foreground">
                (en duda: {judgement.held ?? 0} · retirados: {judgement.withdrawn ?? 0})
              </span>
            )}
          </h3>
          <div className="flex flex-wrap items-center gap-2">
            {bridge && (
              <button
                type="button"
                onClick={() => void loadApprovals()}
                disabled={loadingApprovals}
                aria-label="Refrescar aprobaciones pendientes"
                className="inline-flex items-center gap-1 text-[11px] font-semibold text-muted-foreground hover:text-foreground cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none rounded px-1.5 py-0.5 border border-border bg-muted/40 disabled:opacity-50"
              >
                <RefreshCw className={cn('w-3 h-3', loadingApprovals && 'animate-spin')} aria-hidden="true" />
                <span>Refrescar</span>
              </button>
            )}
            <button
              type="button"
              onClick={() => useAppStore.getState().setActiveTab('coloquio')}
              className="text-[11px] font-semibold text-primary hover:underline cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none rounded px-1"
            >
              Revisar planes en Coloquio
            </button>
          </div>
        </div>

        {fetchError && (
          <div
            role="alert"
            className="p-2.5 rounded-md border border-rose-500/30 bg-rose-500/10 flex flex-wrap items-center justify-between gap-2"
          >
            <p className="text-xs text-rose-700 dark:text-rose-300">{fetchError}</p>
            {authRequired && (
              <button
                type="button"
                onClick={handleOpenTokenModal}
                className="px-2.5 py-1 rounded text-[11px] font-semibold bg-rose-600 hover:bg-rose-500 text-white cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none shrink-0"
              >
                Configurar Token
              </button>
            )}
          </div>
        )}
        {actionFeedback && (
          <p role="status" className="text-xs text-amber-700 dark:text-amber-300">
            {actionFeedback}
          </p>
        )}

        {items.length === 0 ? (
          <div className="p-3 rounded-md border border-border bg-muted/30 space-y-1.5 text-xs">
            <p className="font-semibold text-foreground">
              Sin acciones pendientes de aprobación en este momento.
            </p>
            <p className="text-muted-foreground">
              <strong className="text-foreground">Qué es:</strong> aquí aparecen permisos puntuales (grants) y planes de trabajo (plans) que requieren revisión humana antes de ejecutarse.
            </p>
            <p className="text-muted-foreground">
              <strong className="text-foreground">Cómo aparece uno:</strong> cuando un agente solicita una acción sensible vía <code className="font-mono text-[11px]">tylluan_do</code> o deja un plan en espera.
            </p>
            <p className="text-muted-foreground">
              <strong className="text-foreground">Qué hacer:</strong> pulsa Refrescar cuando un agente pida permiso, o abre Coloquio para revisar los planes de la flota.
            </p>
          </div>
        ) : (
          <ul className="space-y-2" aria-label="Lista de acciones pendientes de aprobación">
            {items.map((a) => {
              const displayTool = a.tool || a.tool_name || 'Acción solicitada';
              const displayState = a.state || a.status || 'pendiente';
              const isHoldOpen = holdTargetId === a.id;
              const isBusy = actionBusyId === a.id;
              return (
                <li
                  key={a.id}
                  className="p-2.5 rounded-md border border-amber-500/30 bg-amber-500/10 flex flex-col gap-2"
                >
                  <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-1.5">
                    <div className="min-w-0">
                      <div className="text-xs font-semibold text-foreground truncate flex flex-wrap items-center gap-1.5">
                        {a.kind && (
                          <span className="px-1.5 py-0.5 rounded text-[10px] font-mono uppercase bg-card border border-border text-muted-foreground">
                            {a.kind}
                          </span>
                        )}
                        <span>{displayTool}</span>
                        {a.guild ? <span className="text-muted-foreground font-normal">· {a.guild}</span> : null}
                        {a.agent_id ? <span className="text-muted-foreground font-mono font-normal">@{a.agent_id}</span> : null}
                      </div>
                      {(a.summary || a.intent) && (
                        <div className="text-[11px] text-foreground/80 truncate mt-0.5">
                          {a.summary || a.intent}
                        </div>
                      )}
                      <div className="text-[11px] font-mono text-muted-foreground truncate">{a.id}</div>
                    </div>
                    <div className="flex flex-wrap items-center gap-1.5 shrink-0">
                      <span className="px-2 py-0.5 rounded text-[10px] font-semibold bg-amber-500/20 text-amber-700 dark:text-amber-300 border border-amber-500/30 shrink-0">
                        {displayState}
                      </span>
                      {a.can_approve && (
                        <button
                          type="button"
                          disabled={isBusy}
                          onClick={() => void runAction(a.id, 'approve')}
                          className="px-2 py-0.5 rounded text-[11px] font-semibold bg-emerald-600 hover:bg-emerald-500 text-white cursor-pointer disabled:opacity-50"
                        >
                          Aprobar
                        </button>
                      )}
                      {a.can_hold && (
                        <button
                          type="button"
                          disabled={isBusy}
                          onClick={() => {
                            setHoldTargetId(isHoldOpen ? null : a.id);
                            setHoldReason('');
                            setHoldError(null);
                          }}
                          className="px-2 py-0.5 rounded text-[11px] font-semibold bg-amber-600 hover:bg-amber-500 text-white cursor-pointer disabled:opacity-50"
                        >
                          Poner en duda
                        </button>
                      )}
                      {a.can_withdraw && (
                        <button
                          type="button"
                          disabled={isBusy}
                          onClick={() => void runAction(a.id, 'withdraw')}
                          className="px-2 py-0.5 rounded text-[11px] font-semibold border border-border bg-card hover:bg-muted text-foreground cursor-pointer disabled:opacity-50"
                        >
                          Retirar
                        </button>
                      )}
                    </div>
                  </div>
                  {isHoldOpen && (
                    <div className="pt-2 border-t border-amber-500/20 flex flex-col gap-1.5">
                      <div className="flex flex-wrap items-center gap-2">
                        <input
                          type="text"
                          value={holdReason}
                          onChange={(e) => {
                            setHoldReason(e.target.value);
                            if (holdError) setHoldError(null);
                          }}
                          placeholder="Motivo obligatorio para poner en duda..."
                          aria-label="Motivo obligatorio para poner en duda"
                          className="flex-1 min-w-[180px] px-2.5 py-1 text-xs rounded border border-border bg-background text-foreground"
                        />
                        <button
                          type="button"
                          disabled={isBusy}
                          onClick={() => handleConfirmHold(a.id)}
                          className="px-2.5 py-1 rounded text-[11px] font-semibold bg-amber-700 hover:bg-amber-600 text-white cursor-pointer disabled:opacity-50"
                        >
                          Confirmar duda
                        </button>
                      </div>
                      {holdError && (
                        <p role="alert" className="text-[11px] text-rose-600 dark:text-rose-400">
                          {holdError}
                        </p>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {/* Panels */}
      <div className="space-y-3">
        {(activeSection === 'all' || activeSection === 'trust') && (
          <section aria-labelledby="trust-console-heading">
            <TrustConsoleTab />
          </section>
        )}

        {(activeSection === 'all' || activeSection === 'quarantine') && (
          <section aria-labelledby="quarantine-heading">
            <QuarantinePanel bridge={bridge} />
          </section>
        )}

        {(activeSection === 'all' || activeSection === 'audit') && (
          <section aria-labelledby="audit-heading">
            <AuditTrailPanel bridge={bridge} />
          </section>
        )}

        {(activeSection === 'all' || activeSection === 'decision') && (
          <section aria-labelledby="decision-heading" className="space-y-6">
            <DecisionServicePanel bridge={bridge} />
            <CoherenceGatePanel bridge={bridge} />
            <FrictionPanel bridge={bridge} />
          </section>
        )}
      </div>
    </div>
  );
}
