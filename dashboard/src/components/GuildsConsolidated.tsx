import React, { useState } from 'react';
import { GuildsTab } from './GuildsTab';
import { ConnectorsTab } from './ConnectorsTab';
import { McpRegistryPanel } from './McpRegistryPanel';
import { FederationTab } from './FederationTab';
import { Cpu, Link2, Plug, Network } from 'lucide-react';

interface GuildsConsolidatedProps {
  bridge: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
  events: any[];
}

export function GuildsConsolidated(props: GuildsConsolidatedProps) {
  const [subTab, setSubTab] = useState('guilds');

  return (
    <div className="space-y-6">
      {/* Sub Navigation */}
      <div className="flex border-b border-slate-800 pb-2 gap-2 flex-wrap">
        <button
          onClick={() => setSubTab('guilds')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'guilds'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Cpu className="w-3.5 h-3.5" />
          Guilds
        </button>
        <button
          onClick={() => setSubTab('connectors')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'connectors'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Link2 className="w-3.5 h-3.5" />
          Connectors
        </button>
        <button
          onClick={() => setSubTab('mcp')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'mcp'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Plug className="w-3.5 h-3.5" />
          MCP Registry
        </button>
        <button
          onClick={() => setSubTab('federation')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'federation'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Network className="w-3.5 h-3.5" />
          Federation
        </button>
      </div>

      {/* Tab Panels */}
      <div>
        {subTab === 'guilds' && (
          <GuildsTab
            bridge={props.bridge}
            notify={props.notify}
            events={props.events}
          />
        )}
        {subTab === 'connectors' && (
          <ConnectorsTab
            notify={props.notify}
          />
        )}
        {subTab === 'mcp' && (
          <McpRegistryPanel
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
        {subTab === 'federation' && (
          <FederationTab
            bridge={props.bridge}
            notify={props.notify}
          />
        )}
      </div>
    </div>
  );
}
export default GuildsConsolidated;
