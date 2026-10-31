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
    <div className="flex-1 min-h-0 flex flex-col space-y-3 h-full font-sans">
      {/* Sub Navigation - Classic Win32 Property Sheet Tabs */}
      <div className="flex max-w-full overflow-x-auto border-b border-[#ACA899] dark:border-border gap-1 flex-shrink-0" role="group" aria-label="Vistas de memoria SilvaDB">
        <button
          type="button"
          aria-pressed={subTab === 'graph'}
          onClick={() => setSubTab('graph')}
          className={`flex min-w-max items-center gap-1.5 px-3 py-1 text-[11px] font-bold border transition-colors cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A] ${
            subTab === 'graph'
              ? 'bg-white dark:bg-card border-t-2 border-t-[#0A246A] border-x-[#ACA899] border-b-white dark:border-b-card -mb-px text-[#0A246A] dark:text-primary rounded-t-sm shadow-none'
              : 'bg-[#ECE9D8] dark:bg-muted/40 border-[#ACA899] dark:border-border text-[#444444] dark:text-muted-foreground hover:bg-[#F5F4EA] rounded-t-sm'
          }`}
        >
          <Network className="w-3.5 h-3.5" aria-hidden="true" />
          Grafo de Conocimiento
        </button>
        <button
          type="button"
          aria-pressed={subTab === 'nodes'}
          onClick={() => setSubTab('nodes')}
          className={`flex min-w-max items-center gap-1.5 px-3 py-1 text-[11px] font-bold border transition-colors cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A] ${
            subTab === 'nodes'
              ? 'bg-white dark:bg-card border-t-2 border-t-[#0A246A] border-x-[#ACA899] border-b-white dark:border-b-card -mb-px text-[#0A246A] dark:text-primary rounded-t-sm shadow-none'
              : 'bg-[#ECE9D8] dark:bg-muted/40 border-[#ACA899] dark:border-border text-[#444444] dark:text-muted-foreground hover:bg-[#F5F4EA] rounded-t-sm'
          }`}
        >
          <Radio className="w-3.5 h-3.5" aria-hidden="true" />
          Nodos de la Flota
        </button>
        <button
          type="button"
          aria-pressed={subTab === 'contradictions'}
          onClick={() => setSubTab('contradictions')}
          className={`flex min-w-max items-center gap-1.5 px-3 py-1 text-[11px] font-bold border transition-colors cursor-pointer focus-visible:outline-1 focus-visible:outline-[#0A246A] ${
            subTab === 'contradictions'
              ? 'bg-white dark:bg-card border-t-2 border-t-[#0A246A] border-x-[#ACA899] border-b-white dark:border-b-card -mb-px text-[#0A246A] dark:text-primary rounded-t-sm shadow-none'
              : 'bg-[#ECE9D8] dark:bg-muted/40 border-[#ACA899] dark:border-border text-[#444444] dark:text-muted-foreground hover:bg-[#F5F4EA] rounded-t-sm'
          }`}
        >
          <GitCompare className="w-3.5 h-3.5" aria-hidden="true" />
          Contradicciones Cognitivas
          {contradictedCount !== null && contradictedCount > 0 && (
            <span className="px-1.5 py-0.2 rounded-none bg-rose-500/20 text-rose-700 dark:text-rose-300 border border-rose-500/40 text-[10px] font-mono font-bold">
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
