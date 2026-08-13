import React, { useState } from 'react';
import type { NexusBridge, NexusEvent } from '../lib/nexus-bridge';
import { MaintenanceTab } from './MaintenanceTab';
import { LogsTab } from './LogsTab';
import { ModelsTab } from './ModelsTab';
import DoctorPanel from './DoctorPanel';
import { ScopesPanel } from './ScopesPanel';
import A2aPanel from './A2aPanel';
import { Wrench, Terminal, Cpu, Stethoscope, Layers, Globe } from 'lucide-react';
import { cn } from '../lib/utils';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  events: NexusEvent[];
  onClearLogs?: () => void;
}

export function SystemTab({ bridge, notify, events, onClearLogs }: Props) {
  const [view, setView] = useState<'doctor' | 'maintenance' | 'logs' | 'models' | 'scopes' | 'a2a'>('doctor');

  return (
    <div className="flex flex-col h-full space-y-4">
      {/* Sub-navigation */}
      <div className="flex items-center gap-2 p-1 bg-slate-900 border border-slate-800 rounded-xl w-max">
        <button
          type="button"
          onClick={() => setView('doctor')}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors focus-visible:outline-none",
            view === 'doctor'
              ? "bg-slate-800 text-slate-200 shadow-sm focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
              : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
          )}
        >
          <Stethoscope className="w-4 h-4" />
          Doctor
        </button>
        <button
          type="button"
          onClick={() => setView('maintenance')}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors focus-visible:outline-none",
            view === 'maintenance'
              ? "bg-slate-800 text-slate-200 shadow-sm focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
              : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
          )}
        >
          <Wrench className="w-4 h-4" />
          Maintenance
        </button>
        <button
          type="button"
          onClick={() => setView('logs')}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors focus-visible:outline-none",
            view === 'logs'
              ? "bg-slate-800 text-slate-200 shadow-sm focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
              : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
          )}
        >
          <Terminal className="w-4 h-4" />
          Kernel Logs
        </button>
        <button
          type="button"
          onClick={() => setView('models')}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors focus-visible:outline-none",
            view === 'models'
              ? "bg-slate-800 text-slate-200 shadow-sm focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
              : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
          )}
        >
          <Cpu className="w-4 h-4" />
          Models
        </button>
        <button
          type="button"
          onClick={() => setView('scopes')}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors focus-visible:outline-none",
            view === 'scopes'
              ? "bg-slate-800 text-slate-200 shadow-sm focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
              : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
          )}
        >
          <Layers className="w-4 h-4" />
          Scopes
        </button>
        <button
          type="button"
          onClick={() => setView('a2a')}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors focus-visible:outline-none",
            view === 'a2a'
              ? "bg-slate-800 text-slate-200 shadow-sm focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
              : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50 focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
          )}
        >
          <Globe className="w-4 h-4" />
          A2A Interop
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 min-h-0">
        {view === 'doctor' ? (
          <DoctorPanel bridge={bridge} notify={notify} />
        ) : view === 'maintenance' ? (
          <MaintenanceTab bridge={bridge} notify={notify} />
        ) : view === 'logs' ? (
          <LogsTab events={events} onClear={onClearLogs} />
        ) : view === 'models' ? (
          <ModelsTab bridge={bridge} />
        ) : view === 'scopes' ? (
          <ScopesPanel bridge={bridge} notify={notify} />
        ) : (
          <A2aPanel notify={notify} />
        )}
      </div>
    </div>
  );
}
