import React, { useState } from 'react';
import type { Approval, NexusBridge } from '../lib/nexus-bridge';
import { ShieldCheck, Clock } from 'lucide-react';
import { cn } from '../lib/utils';
import { useAppStore } from '../stores/useAppStore';
import { TrustConsoleTab } from './TrustConsoleTab';
import { QuarantinePanel } from './QuarantinePanel';
import AuditTrailPanel from './AuditTrailPanel';
import CoherenceGatePanel from './CoherenceGatePanel';
import FrictionPanel from './FrictionPanel';
import { DecisionServicePanel } from './DecisionServicePanel';

interface ApprovalsAndSecurityTabProps {
  bridge: NexusBridge | null;
  approvals?: Approval[];
}

type SubSection = 'all' | 'trust' | 'quarantine' | 'audit' | 'decision';

export function ApprovalsAndSecurityTab({ bridge, approvals = [] }: ApprovalsAndSecurityTabProps) {
  const [activeSection, setActiveSection] = useState<SubSection>('all');

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

      {/* Pendiente de aprobación (HITL / acciones bloqueadas) */}
      <section
        aria-labelledby="pending-approvals-heading"
        className="border border-border bg-card p-3.5 rounded-lg shadow-sm space-y-2.5"
      >
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 id="pending-approvals-heading" className="text-xs font-bold text-foreground flex items-center gap-1.5">
            <Clock className="w-3.5 h-3.5 text-amber-500" aria-hidden="true" />
            <span>Pendiente de tu aprobación</span>
            <span className="px-1.5 py-0.5 rounded bg-muted text-[10px] font-mono text-muted-foreground border border-border">
              {approvals.length}
            </span>
          </h3>
          <button
            type="button"
            onClick={() => useAppStore.getState().setActiveTab('coloquio')}
            className="text-[11px] font-semibold text-primary hover:underline cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none rounded px-1"
          >
            Revisar planes en Coloquio
          </button>
        </div>

        {approvals.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            Sin acciones pendientes de aprobación en este momento.
          </p>
        ) : (
          <ul className="space-y-2" aria-label="Lista de acciones pendientes de aprobación">
            {approvals.map((a) => (
              <li
                key={a.id}
                className="p-2.5 rounded-md border border-amber-500/30 bg-amber-500/10 flex flex-col sm:flex-row sm:items-center justify-between gap-1.5"
              >
                <div className="min-w-0">
                  <div className="text-xs font-semibold text-foreground truncate">
                    {a.tool || 'Acción solicitada'}
                    {a.guild ? <span className="text-muted-foreground font-normal"> · {a.guild}</span> : null}
                  </div>
                  <div className="text-[11px] font-mono text-muted-foreground truncate">{a.id}</div>
                </div>
                <span className="self-start sm:self-auto px-2 py-0.5 rounded text-[10px] font-semibold bg-amber-500/20 text-amber-700 dark:text-amber-300 border border-amber-500/30 shrink-0">
                  {a.status || 'pendiente'}
                </span>
              </li>
            ))}
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
