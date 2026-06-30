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
  online?: boolean;
}

export function LabConsolidated(props: LabConsolidatedProps) {
  const [subTab, setSubTab] = useState('laboratory');

  if (!props.online) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-8 text-center bg-slate-900/10 rounded-2xl border border-slate-800/60 max-w-md mx-auto my-12 animate-in fade-in duration-300">
        <div className="w-16 h-16 rounded-2xl bg-red-500/5 border border-red-500/20 flex items-center justify-center mb-6">
          <Beaker className="w-8 h-8 text-red-500/60" />
        </div>
        <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider font-mono">Laboratory Offline</h3>
        <p className="text-slate-400 text-xs mt-2 max-w-sm leading-relaxed">
          El área de pruebas cognitivas, inspección y mantenimiento del kernel está desconectada.
        </p>
        <div className="mt-6 p-4 bg-slate-950/60 border border-slate-900 rounded-xl text-[10px] font-mono text-slate-500 text-left w-full">
          <span className="text-emerald-400 font-bold block mb-1">CÓMO ARRANCAR EL SERVICIO:</span>
          1. Abre tu terminal local.<br/>
          2. Ejecuta: <code className="text-slate-300 font-bold">tylluan-cli start</code><br/>
          3. Recarga esta página.
        </div>
        <button
          onClick={() => window.location.reload()}
          className="mt-6 flex items-center gap-2 px-4 py-2 bg-slate-900 border border-slate-800 hover:border-slate-700 text-slate-400 hover:text-white rounded-xl text-xs font-bold transition-all hover:bg-slate-800/50 cursor-pointer"
        >
          <Beaker className="w-3.5 h-3.5" /> Reintentar Conexión
        </button>
      </div>
    );
  }

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
