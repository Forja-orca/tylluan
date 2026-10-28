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
    <div className="space-y-6">
      {/* Subheader and Filter Selector */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-slate-800">
        <div>
          <h2 className="text-lg font-bold text-slate-100 flex items-center gap-2">
            <ShieldCheck className="w-5 h-5 text-emerald-400" aria-hidden="true" />
            Aprobaciones, Seguridad y Cuarentena
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Gobernanza soberana HITL, sandbox ASI06, verificación de contratos e historial inmutable de auditoría.
          </p>
        </div>

        <div role="group" aria-label="Filtro de seguridad" className="flex items-center gap-1 bg-slate-900 border border-slate-800 rounded-lg p-1 self-start sm:self-auto">
          <button
            type="button"
            onClick={() => setActiveSection('all')}
            className={cn(
              "px-2.5 py-1 rounded text-xs font-medium transition-colors cursor-pointer",
              activeSection === 'all'
                ? "bg-slate-800 text-slate-100 shadow-sm"
                : "text-slate-400 hover:text-slate-200"
            )}
          >
            Vista Completa
          </button>
          <button
            type="button"
            onClick={() => setActiveSection('trust')}
            className={cn(
              "px-2.5 py-1 rounded text-xs font-medium transition-colors cursor-pointer",
              activeSection === 'trust'
                ? "bg-slate-800 text-slate-100 shadow-sm"
                : "text-slate-400 hover:text-slate-200"
            )}
          >
            Trust Console
          </button>
          <button
            type="button"
            onClick={() => setActiveSection('quarantine')}
            className={cn(
              "px-2.5 py-1 rounded text-xs font-medium transition-colors cursor-pointer",
              activeSection === 'quarantine'
                ? "bg-slate-800 text-slate-100 shadow-sm"
                : "text-slate-400 hover:text-slate-200"
            )}
          >
            Cuarentena
          </button>
          <button
            type="button"
            onClick={() => setActiveSection('audit')}
            className={cn(
              "px-2.5 py-1 rounded text-xs font-medium transition-colors cursor-pointer",
              activeSection === 'audit'
                ? "bg-slate-800 text-slate-100 shadow-sm"
                : "text-slate-400 hover:text-slate-200"
            )}
          >
            Auditoría
          </button>
          <button
            type="button"
            onClick={() => setActiveSection('decision')}
            className={cn(
              "px-2.5 py-1 rounded text-xs font-medium transition-colors cursor-pointer",
              activeSection === 'decision'
                ? "bg-slate-800 text-slate-100 shadow-sm"
                : "text-slate-400 hover:text-slate-200"
            )}
          >
            Decisión
          </button>
        </div>
      </div>

      {/* Panels */}
      <div className="space-y-6">
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
