import React, { useState } from 'react';
import { LaboratoryTab } from './LaboratoryTab';
import { VisionTab } from './VisionTab';
import { MaintenanceTab } from './MaintenanceTab';
import { LogsTab } from './LogsTab';
import { Beaker, Camera, ShieldAlert, Scroll } from 'lucide-react';

interface LabConsolidatedProps {
  bridge: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
  events: any[];
  onClearLogs: () => void;
}

export function LabConsolidated(props: LabConsolidatedProps) {
  const [subTab, setSubTab] = useState('laboratory');

  return (
    <div className="space-y-6">
      {/* Sub Navigation */}
      <div className="flex border-b border-slate-800 pb-2 gap-2 flex-wrap">
        <button
          onClick={() => setSubTab('laboratory')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'laboratory'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Beaker className="w-3.5 h-3.5" />
          Laboratory
        </button>
        <button
          onClick={() => setSubTab('vision')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'vision'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Camera className="w-3.5 h-3.5" />
          Vision
        </button>
        <button
          onClick={() => setSubTab('maintenance')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'maintenance'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <ShieldAlert className="w-3.5 h-3.5" />
          Maintenance
        </button>
        <button
          onClick={() => setSubTab('logs')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'logs'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Scroll className="w-3.5 h-3.5" />
          System Logs
        </button>
      </div>

      {/* Tab Panels */}
      <div>
        {subTab === 'laboratory' && (
          <LaboratoryTab
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
        {subTab === 'vision' && (
          <VisionTab
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
        {subTab === 'maintenance' && (
          <MaintenanceTab
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
        {subTab === 'logs' && (
          <LogsTab
            events={props.events}
            onClear={props.onClearLogs}
          />
        )}
      </div>
    </div>
  );
}
export default LabConsolidated;
