import React, { useState, lazy, Suspense } from 'react';
import { Users, MessageSquare, Shield, Loader2 } from 'lucide-react';

const FleetTab = lazy(() => import('./FleetTab').then(m => ({ default: m.FleetTab })));
const ColoquioTab = lazy(() => import('./ColoquioTab').then(m => ({ default: m.ColoquioTab })));
const CollectiveTab = lazy(() => import('./CollectiveTab').then(m => ({ default: m.CollectiveTab })));

interface TeamConsolidatedProps {
  bridge: any;
}

export function TeamConsolidated(props: TeamConsolidatedProps) {
  const [subTab, setSubTab] = useState('fleet');

  return (
    <div className="flex-1 min-h-0 flex flex-col space-y-4 h-full">
      {/* Sub Navigation */}
      <div className="flex max-w-full overflow-x-auto border-b border-slate-800 pb-2 gap-2 flex-shrink-0 scrollbar-thin">
        <button
          onClick={() => setSubTab('fleet')}
          className={`flex min-w-max items-center gap-1.5 sm:gap-2 px-3 py-1.5 sm:px-4 sm:py-2 text-[11px] sm:text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'fleet'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Users className="w-3.5 h-3.5" />
          Fleet Status
        </button>
        <button
          onClick={() => setSubTab('coloquio')}
          className={`flex min-w-max items-center gap-1.5 sm:gap-2 px-3 py-1.5 sm:px-4 sm:py-2 text-[11px] sm:text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'coloquio'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <MessageSquare className="w-3.5 h-3.5" />
          Coloquio Chat
        </button>
        <button
          onClick={() => setSubTab('agents')}
          className={`flex min-w-max items-center gap-1.5 sm:gap-2 px-3 py-1.5 sm:px-4 sm:py-2 text-[11px] sm:text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'agents'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Shield className="w-3.5 h-3.5" />
          Agent Collective
        </button>
      </div>

      {/* Tab Panels */}
      <div className="flex-1 min-h-0 flex flex-col">
        <Suspense fallback={
          <div className="flex-1 flex items-center justify-center py-12 text-slate-500 text-xs font-mono gap-2">
            <Loader2 className="w-4 h-4 animate-spin text-emerald-400" />
            <span>Cargando módulo de equipo...</span>
          </div>
        }>
          {subTab === 'fleet' && (
            <div className="flex-1 overflow-y-auto">
              <FleetTab />
            </div>
          )}
          {subTab === 'coloquio' && (
            <ColoquioTab
              bridge={props.bridge}
            />
          )}
          {subTab === 'agents' && (
            <div className="flex-1 overflow-y-auto">
              <CollectiveTab />
            </div>
          )}
        </Suspense>
      </div>
    </div>
  );
}
export default TeamConsolidated;
