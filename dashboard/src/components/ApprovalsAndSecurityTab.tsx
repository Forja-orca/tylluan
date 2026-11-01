import React, { useState } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { ShieldCheck } from 'lucide-react';
import { cn } from '../lib/utils';
import { TrustConsoleTab } from './TrustConsoleTab';
import { QuarantinePanel } from './QuarantinePanel';
import AuditTrailPanel from './AuditTrailPanel';
import CoherenceGatePanel from './CoherenceGatePanel';
import FrictionPanel from './FrictionPanel';
import { DecisionServicePanel } from './DecisionServicePanel';

interface ApprovalsAndSecurityTabProps {
  bridge: NexusBridge | null;
}

type SubSection = 'all' | 'trust' | 'quarantine' | 'audit' | 'decision';

export function ApprovalsAndSecurityTab({ bridge }: ApprovalsAndSecurityTabProps) {
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
              Gobernanza soberana HITL, aislamiento ASI06, verificación de contratos e historial inmutable de auditoría.
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
