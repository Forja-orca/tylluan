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
  online?: boolean;
}

export function GuildsConsolidated(props: GuildsConsolidatedProps) {
  const [subTab, setSubTab] = useState('guilds');

  if (!props.online) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-8 text-center bg-slate-900/10 rounded-2xl border border-slate-800/60 max-w-md mx-auto my-12 animate-in fade-in duration-300">
        <div className="w-16 h-16 rounded-2xl bg-red-500/5 border border-red-500/20 flex items-center justify-center mb-6">
          <Cpu className="w-8 h-8 text-red-500/60" />
        </div>
        <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider font-mono">Guilds Registry Offline</h3>
        <p className="text-slate-400 text-xs mt-2 max-w-sm leading-relaxed">
          Las herramientas de agentes y los Gremios (Guilds) no están disponibles. Arranca el microkernel local para gestionar las capacidades integradas y la ejecución de sandbox.
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
          <Cpu className="w-3.5 h-3.5" /> Reintentar Conexión
        </button>
      </div>
    );
  }

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
