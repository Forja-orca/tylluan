import React, { useState } from 'react';
import { KnowledgeGraphTab } from './KnowledgeGraphTab';
import { NodesTab } from './NodesTab';
import { ContradictionsPanel } from './ContradictionsPanel';
import { Network, Radio, GitCompare } from 'lucide-react';
import type { NexusBridge, MemoryStats } from '../lib/nexus-bridge';

interface MemoryConsolidatedProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  memoryStats: MemoryStats | null;
}

export function MemoryConsolidated(props: MemoryConsolidatedProps) {
  const [subTab, setSubTab] = useState<'graph' | 'nodes' | 'contradictions'>('graph');
  const stats = props.memoryStats as (MemoryStats & { contradicted_count?: number; conflicted_count?: number }) | null;
  const contradictedCount = stats?.contradicted_count ?? stats?.conflicted_count ?? null;

  return (
    <div className="flex-1 min-h-0 flex flex-col space-y-4 h-full">
      {/* Sub Navigation */}
      <div className="flex max-w-full overflow-x-auto border-b border-border pb-2 gap-2 flex-shrink-0 scrollbar-thin" role="group" aria-label="SilvaDB Memory Views">
        <button
          type="button"
          aria-pressed={subTab === 'graph'}
          onClick={() => setSubTab('graph')}
          className={`flex min-w-max items-center gap-1.5 sm:gap-2 px-3 py-1.5 sm:px-4 sm:py-2 text-xs font-semibold rounded border transition-colors cursor-pointer ${
            subTab === 'graph'
              ? 'bg-primary/10 border-primary/30 text-primary'
              : 'bg-muted/40 border-border text-muted-foreground hover:text-foreground'
          }`}
        >
          <Network className="w-3.5 h-3.5" aria-hidden="true" />
          Knowledge Graph
        </button>
        <button
          type="button"
          aria-pressed={subTab === 'nodes'}
          onClick={() => setSubTab('nodes')}
          className={`flex min-w-max items-center gap-1.5 sm:gap-2 px-3 py-1.5 sm:px-4 sm:py-2 text-xs font-semibold rounded border transition-colors cursor-pointer ${
            subTab === 'nodes'
              ? 'bg-primary/10 border-primary/30 text-primary'
              : 'bg-muted/40 border-border text-muted-foreground hover:text-foreground'
          }`}
        >
          <Radio className="w-3.5 h-3.5" aria-hidden="true" />
          Nodes
        </button>
        <button
          type="button"
          aria-pressed={subTab === 'contradictions'}
          onClick={() => setSubTab('contradictions')}
          className={`flex min-w-max items-center gap-1.5 sm:gap-2 px-3 py-1.5 sm:px-4 sm:py-2 text-xs font-semibold rounded border transition-colors cursor-pointer ${
            subTab === 'contradictions'
              ? 'bg-rose-500/15 border-rose-500/40 text-rose-300'
              : 'bg-muted/40 border-border text-muted-foreground hover:text-foreground'
          }`}
        >
          <GitCompare className="w-3.5 h-3.5" aria-hidden="true" />
          Contradictions
          {contradictedCount !== null && contradictedCount > 0 && (
            <span className="px-1.5 py-0.5 rounded bg-rose-500/20 text-rose-300 border border-rose-500/30 text-xs">
              {contradictedCount}
            </span>
          )}
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
        {subTab === 'contradictions' && (
          <div className="flex-1 overflow-y-auto">
            <ContradictionsPanel
              bridge={props.bridge}
              notify={props.notify}
            />
          </div>
        )}
      </div>
    </div>
  );
}
export default MemoryConsolidated;
