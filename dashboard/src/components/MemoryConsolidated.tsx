import React, { useState } from 'react';
import { KnowledgeGraphTab } from './KnowledgeGraphTab';
import { NodesTab } from './NodesTab';
import { BlackboardTab } from './BlackboardTab';
import { Network, Radio, MessageSquare, Database, RefreshCw } from 'lucide-react';

interface MemoryConsolidatedProps {
  bridge: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
  memoryStats: any;
  online?: boolean;
}

export function MemoryConsolidated(props: MemoryConsolidatedProps) {
  const [subTab, setSubTab] = useState('graph');

  if (!props.online) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-8 text-center bg-slate-900/10 rounded-2xl border border-slate-800/60 max-w-md mx-auto my-12 animate-in fade-in duration-300">
        <div className="w-16 h-16 rounded-2xl bg-red-500/5 border border-red-500/20 flex items-center justify-center mb-6">
          <Database className="w-8 h-8 text-red-500/60" />
        </div>
        <h3 className="text-sm font-bold text-slate-200 uppercase tracking-wider font-mono">Memory Hub Desconectado</h3>
        <p className="text-slate-400 text-xs mt-2 max-w-sm leading-relaxed">
          La base de conocimiento de SilvaDB, el grafo semántico y el Blackboard no están disponibles porque el microkernel está desconectado.
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
          <RefreshCw className="w-3.5 h-3.5" /> Reintentar Conexión
        </button>
      </div>
    );
  }

  return (
    <div className="flex-1 min-h-0 flex flex-col space-y-4 h-full">
      {/* Sub Navigation */}
      <div className="flex border-b border-slate-800 pb-2 gap-2 flex-shrink-0">
        <button
          onClick={() => setSubTab('graph')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'graph'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Network className="w-3.5 h-3.5" />
          Knowledge Graph
        </button>
        <button
          onClick={() => setSubTab('nodes')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'nodes'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Radio className="w-3.5 h-3.5" />
          Nodes
        </button>
        <button
          onClick={() => setSubTab('blackboard')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'blackboard'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <MessageSquare className="w-3.5 h-3.5" />
          Blackboard
        </button>
      </div>

      {/* Tab Panels */}
      <div className="flex-1 min-h-0 flex flex-col">
        {subTab === 'graph' && (
          <KnowledgeGraphTab
            bridge={props.bridge}
            notify={props.notify}
            memoryStats={props.memoryStats}
          />
        )}
        {subTab === 'nodes' && (
          <div className="flex-1 overflow-y-auto">
            <NodesTab
              bridge={props.bridge}
              notify={props.notify}
            />
          </div>
        )}
        {subTab === 'blackboard' && (
          <div className="flex-1 overflow-y-auto">
            <BlackboardTab
              bridge={props.bridge}
            />
          </div>
        )}
      </div>
    </div>
  );
}
export default MemoryConsolidated;
