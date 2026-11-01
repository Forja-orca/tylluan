import React, { useState, useRef } from 'react';
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
  const subTabOrder: Array<'graph' | 'nodes' | 'contradictions'> = ['graph', 'nodes', 'contradictions'];
  const tabRefs = useRef<Record<'graph' | 'nodes' | 'contradictions', HTMLButtonElement | null>>({
    graph: null,
    nodes: null,
    contradictions: null,
  });

  const handleSubTabKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, current: 'graph' | 'nodes' | 'contradictions') => {
    const idx = subTabOrder.indexOf(current);
    let nextIdx = -1;
    if (e.key === 'ArrowRight') nextIdx = (idx + 1) % subTabOrder.length;
    else if (e.key === 'ArrowLeft') nextIdx = (idx - 1 + subTabOrder.length) % subTabOrder.length;
    else if (e.key === 'Home') nextIdx = 0;
    else if (e.key === 'End') nextIdx = subTabOrder.length - 1;
    if (nextIdx !== -1) {
      e.preventDefault();
      const nextTab = subTabOrder[nextIdx];
      setSubTab(nextTab);
      tabRefs.current[nextTab]?.focus();
    }
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col space-y-3 h-full font-sans">
      {/* Sub Navigation - Sovereign Palette Tabs */}
      <div className="flex max-w-full overflow-x-auto border-b border-border pb-2 gap-1.5 flex-shrink-0" role="tablist" aria-label="Vistas de memoria SilvaDB">
        <button
          ref={(el) => { tabRefs.current.graph = el; }}
          type="button"
          role="tab"
          aria-selected={subTab === 'graph'}
          tabIndex={subTab === 'graph' ? 0 : -1}
          onClick={() => setSubTab('graph')}
          onKeyDown={(e) => handleSubTabKeyDown(e, 'graph')}
          className={`flex min-w-max items-center gap-1.5 px-3 py-1.5 text-xs font-semibold rounded-md border transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none ${
            subTab === 'graph'
              ? 'bg-primary/10 text-primary border-primary/30 shadow-sm'
              : 'bg-card text-muted-foreground border-border hover:bg-muted/60 hover:text-foreground'
          }`}
        >
          <Network className="w-3.5 h-3.5" aria-hidden="true" />
          Grafo de Conocimiento
        </button>
        <button
          ref={(el) => { tabRefs.current.nodes = el; }}
          type="button"
          role="tab"
          aria-selected={subTab === 'nodes'}
          tabIndex={subTab === 'nodes' ? 0 : -1}
          onClick={() => setSubTab('nodes')}
          onKeyDown={(e) => handleSubTabKeyDown(e, 'nodes')}
          className={`flex min-w-max items-center gap-1.5 px-3 py-1.5 text-xs font-semibold rounded-md border transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none ${
            subTab === 'nodes'
              ? 'bg-primary/10 text-primary border-primary/30 shadow-sm'
              : 'bg-card text-muted-foreground border-border hover:bg-muted/60 hover:text-foreground'
          }`}
        >
          <Radio className="w-3.5 h-3.5" aria-hidden="true" />
          Nodos de la Flota
        </button>
        <button
          ref={(el) => { tabRefs.current.contradictions = el; }}
          type="button"
          role="tab"
          aria-selected={subTab === 'contradictions'}
          tabIndex={subTab === 'contradictions' ? 0 : -1}
          onClick={() => setSubTab('contradictions')}
          onKeyDown={(e) => handleSubTabKeyDown(e, 'contradictions')}
          className={`flex min-w-max items-center gap-1.5 px-3 py-1.5 text-xs font-semibold rounded-md border transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none ${
            subTab === 'contradictions'
              ? 'bg-primary/10 text-primary border-primary/30 shadow-sm'
              : 'bg-card text-muted-foreground border-border hover:bg-muted/60 hover:text-foreground'
          }`}
        >
          <GitCompare className="w-3.5 h-3.5" aria-hidden="true" />
          Contradicciones Cognitivas
          {contradictedCount !== null && contradictedCount > 0 && (
            <span className="px-1.5 py-0.5 rounded-md bg-rose-500/15 text-rose-700 dark:text-rose-300 border border-rose-500/30 text-[10px] font-mono font-bold">
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
