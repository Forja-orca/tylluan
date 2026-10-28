import React, { useState } from 'react';
import type { NexusBridge, NexusEvent } from '../lib/nexus-bridge';
import { Beaker, Cpu, Globe, Layers, Stethoscope, Terminal } from 'lucide-react';
import { cn } from '../lib/utils';
import LabConsolidated from './LabConsolidated';
import GuildsConsolidated from './GuildsConsolidated';
import DoctorPanel from './DoctorPanel';
import { LogsTab } from './LogsTab';
import { ScopesPanel } from './ScopesPanel';
import A2aPanel from './A2aPanel';

interface DiagnosticTabProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  events: NexusEvent[];
  onClearLogs?: () => void;
}

type SubDiag = 'doctor' | 'guilds' | 'lab' | 'logs' | 'scopes' | 'a2a';

export function DiagnosticTab({ bridge, notify, events, onClearLogs }: DiagnosticTabProps) {
  const [activeTab, setActiveTab] = useState<SubDiag>('doctor');

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-slate-800">
        <div>
          <h2 className="text-lg font-bold text-slate-100 flex items-center gap-2">
            <Beaker className="w-5 h-5 text-amber-400" aria-hidden="true" />
            Diagnóstico e Inspección Avanzada (Dev Mode)
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Herramientas internas de inspección, contratos de guilds, scopes de agentes, laboratorio A/B y telemetría de bajo nivel.
          </p>
        </div>

        <div role="group" aria-label="Sub-herramientas de diagnóstico" className="flex items-center gap-1 bg-slate-900 border border-slate-800 rounded-lg p-1 overflow-x-auto max-w-full">
          <button
            type="button"
            onClick={() => setActiveTab('doctor')}
            className={cn(
              "flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors shrink-0 cursor-pointer",
              activeTab === 'doctor' ? "bg-slate-800 text-slate-100 shadow-sm" : "text-slate-400 hover:text-slate-200"
            )}
          >
            <Stethoscope className="w-3.5 h-3.5" /> Doctor
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('guilds')}
            className={cn(
              "flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors shrink-0 cursor-pointer",
              activeTab === 'guilds' ? "bg-slate-800 text-slate-100 shadow-sm" : "text-slate-400 hover:text-slate-200"
            )}
          >
            <Cpu className="w-3.5 h-3.5" /> Guilds
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('lab')}
            className={cn(
              "flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors shrink-0 cursor-pointer",
              activeTab === 'lab' ? "bg-slate-800 text-slate-100 shadow-sm" : "text-slate-400 hover:text-slate-200"
            )}
          >
            <Beaker className="w-3.5 h-3.5" /> Lab
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('logs')}
            className={cn(
              "flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors shrink-0 cursor-pointer",
              activeTab === 'logs' ? "bg-slate-800 text-slate-100 shadow-sm" : "text-slate-400 hover:text-slate-200"
            )}
          >
            <Terminal className="w-3.5 h-3.5" /> Logs
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('scopes')}
            className={cn(
              "flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors shrink-0 cursor-pointer",
              activeTab === 'scopes' ? "bg-slate-800 text-slate-100 shadow-sm" : "text-slate-400 hover:text-slate-200"
            )}
          >
            <Layers className="w-3.5 h-3.5" /> Scopes
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('a2a')}
            className={cn(
              "flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors shrink-0 cursor-pointer",
              activeTab === 'a2a' ? "bg-slate-800 text-slate-100 shadow-sm" : "text-slate-400 hover:text-slate-200"
            )}
          >
            <Globe className="w-3.5 h-3.5" /> A2A
          </button>
        </div>
      </div>

      <div className="flex-1 min-h-0">
        {activeTab === 'doctor' && <DoctorPanel bridge={bridge} notify={notify} />}
        {activeTab === 'guilds' && <GuildsConsolidated bridge={bridge} notify={notify} events={events} />}
        {activeTab === 'lab' && <LabConsolidated bridge={bridge} notify={notify} />}
        {activeTab === 'logs' && <LogsTab events={events} onClear={onClearLogs} />}
        {activeTab === 'scopes' && <ScopesPanel bridge={bridge} notify={notify} />}
        {activeTab === 'a2a' && <A2aPanel notify={notify} />}
      </div>
    </div>
  );
}
