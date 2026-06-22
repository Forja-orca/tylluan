import React, { useState } from 'react';
import type { NexusBridge, NexusEvent } from '../lib/nexus-bridge';
import { MaintenanceTab } from './MaintenanceTab';
import { LogsTab } from './LogsTab';
import { ModelConfigPanel } from './ModelConfigPanel';
import { Wrench, Terminal, Cpu } from 'lucide-react';
import { cn } from '../lib/utils';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  events: NexusEvent[];
  onClearLogs?: () => void;
}

export function SystemTab({ bridge, notify, events, onClearLogs }: Props) {
  const [view, setView] = useState<'maintenance' | 'logs' | 'models'>('maintenance');

  return (
    <div className="flex flex-col h-full space-y-4">
      {/* Sub-navigation */}
      <div className="flex items-center gap-2 p-1 bg-slate-900 border border-slate-800 rounded-xl w-max">
        <button
          type="button"
          onClick={() => setView('maintenance')}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors",
            view === 'maintenance'
              ? "bg-slate-800 text-slate-200 shadow-sm"
              : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50"
          )}
        >
          <Wrench className="w-4 h-4" />
          Maintenance
        </button>
        <button
          type="button"
          onClick={() => setView('logs')}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors",
            view === 'logs'
              ? "bg-slate-800 text-slate-200 shadow-sm"
              : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50"
          )}
        >
          <Terminal className="w-4 h-4" />
          Kernel Logs
        </button>
        <button
          type="button"
          onClick={() => setView('models')}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold transition-colors",
            view === 'models'
              ? "bg-slate-800 text-slate-200 shadow-sm"
              : "text-slate-500 hover:text-slate-300 hover:bg-slate-800/50"
          )}
        >
          <Cpu className="w-4 h-4" />
          Models
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 min-h-0">
        {view === 'maintenance' ? (
          <MaintenanceTab bridge={bridge} notify={notify} />
        ) : view === 'logs' ? (
          <LogsTab events={events} onClear={onClearLogs} />
        ) : (
          <ModelConfigPanel bridge={bridge} />
        )}
      </div>
    </div>
  );
}
