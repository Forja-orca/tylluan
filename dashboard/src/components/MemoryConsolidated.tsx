import React, { useState } from 'react';
import { KnowledgeGraphTab } from './KnowledgeGraphTab';
import { NodesTab } from './NodesTab';
import { BlackboardTab } from './BlackboardTab';
import { Network, Radio, MessageSquare } from 'lucide-react';

interface MemoryConsolidatedProps {
  bridge: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
  memoryStats: any;
}

export function MemoryConsolidated(props: MemoryConsolidatedProps) {
  const [subTab, setSubTab] = useState('graph');

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
