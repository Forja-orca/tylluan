import React, { useState } from 'react';
import { FleetTab } from './FleetTab';
import { ColoquioTab } from './ColoquioTab';
import { CollectiveTab } from './CollectiveTab';
import { Users, MessageSquare, Shield } from 'lucide-react';

interface TeamConsolidatedProps {
  bridge: any;
}

export function TeamConsolidated(props: TeamConsolidatedProps) {
  const [subTab, setSubTab] = useState('fleet');

  return (
    <div className="flex-1 min-h-0 flex flex-col space-y-4 h-full">
      {/* Sub Navigation */}
      <div className="flex border-b border-slate-800 pb-2 gap-2 flex-shrink-0">
        <button
          onClick={() => setSubTab('fleet')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
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
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
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
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
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
      </div>
    </div>
  );
}
export default TeamConsolidated;
