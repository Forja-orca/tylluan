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
      <div className="border border-[#ACA899] dark:border-border bg-[#ECE9D8] dark:bg-card p-2.5 rounded-sm">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-[#ACA899] dark:border-border">
          <div>
            <h2 className="text-xs font-bold text-[#01285F] dark:text-foreground flex items-center gap-1.5">
              <ShieldCheck className="w-4 h-4 text-[#0A246A] dark:text-primary" aria-hidden="true" />
              Aprobaciones, Seguridad y Cuarentena
            </h2>
            <p className="text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5">
              Gobernanza soberana HITL, aislamiento ASI06, verificación de contratos e historial inmutable de auditoría.
            </p>
          </div>
        </div>

        {/* Property Sheet Tabs */}
        <div role="tablist" aria-label="Secciones de seguridad" className="flex flex-wrap items-center gap-1 mt-2 pt-1 border-t border-[#ACA899] dark:border-border">
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
                  "px-3 py-1 text-[11px] font-bold border transition-all cursor-pointer rounded-t-sm -mb-px",
                  isActive
                    ? "bg-white dark:bg-card border-[#ACA899] dark:border-border border-b-transparent text-[#01285F] dark:text-primary shadow-none"
                    : "bg-[#D8D4C4] dark:bg-muted/40 border-transparent text-[#555555] dark:text-muted-foreground hover:bg-[#EAE7DC] hover:text-[#000000]"
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
