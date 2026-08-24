import React, { useState, useEffect, useCallback, useRef, Suspense, lazy } from 'react';
import { 
  Search, 
  RefreshCw, 
  Database, 
  Save, 
  Search as SearchIcon,
  Clock,
  X,
  Zap,
  CheckCircle,
  AlertCircle,
  Layers,
  Network,
  List,
  ShieldAlert
} from 'lucide-react';
import type { NexusBridge, GraphNode } from '../lib/nexus-bridge';
import { LifecycleBadge } from '../lib/nexus-bridge';
import type { MemoryStats } from '../hooks/useNexus';
import { useNexus } from '../hooks/useNexus';
import { cn } from '../lib/utils';
import { IngestPanel } from './IngestPanel';

// three.js/WebGL is heavy — only load it when the Knowledge tab is actually opened.
const KnowledgeCortex3D = lazy(() =>
  import('./graph/KnowledgeCortex3D').then((m) => ({ default: m.KnowledgeCortex3D }))
);

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  memoryStats?: MemoryStats | null;
}

const getProvenanceLabel = (prov?: string) => {
  switch (prov) {
    case 'user_direct': return 'Fuente: Usuario directo';
    case 'agent_generated': return 'Fuente: Generado por agente';
    case 'federation_peer': return 'Fuente: Peer federado (sin verificar)';
    case 'guild_output': return 'Fuente: Salida de guild';
    case 'unverified': return 'Fuente: No verificado';
    default: return prov || 'Desconocido';
  }
};

function fixDoubleEncoding(str: string): string {
  if (!str || str.indexOf('\xC3') === -1) return str || '';
  try {
    const bytes = new Uint8Array(str.length);
    for (let i = 0; i < str.length; i++) bytes[i] = str.charCodeAt(i) & 0xFF;
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    return str;
  }
}

export function KnowledgeGraphTab({ bridge, notify, memoryStats }: Props) {
  const { events } = useNexus();
  const [activeSubView, setActiveSubView] = useState<'graph' | 'list'>('graph');
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<GraphNode[]>([]);
  const [edges, setEdges] = useState<any[]>([]);
  const [searching, setSearching] = useState(false);
  const [view, setView] = useState<'grid' | 'table'>('grid');
  const [isDragging, setIsDragging] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [loading, setLoading] = useState(false);
  const [compactMode, setCompactMode] = useState(true);
  const [expandedNodeIds, setExpandedNodeIds] = useState<Record<string, boolean>>({});

  const [searchResults, setSearchResults] = useState<GraphNode[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [showSearchPanel, setShowSearchPanel] = useState(false);
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // --- Recent Sidebar State ---
  const [showRecentSidebar, setShowRecentSidebar] = useState(false);
  const [recentNodes, setRecentNodes] = useState<GraphNode[]>([]);
  const [recentLoaded, setRecentLoaded] = useState(false);

  // --- Lifecycle Filter State ---
  const [lifecycleFilter, setLifecycleFilter] = useState<'all' | 'active' | 'quiet' | 'consolidated' | 'archived'>('all');

  const lifecycleCounts = React.useMemo(() => {
    const counts = { all: results.length, active: 0, quiet: 0, consolidated: 0, archived: 0 };
    for (const node of results) {
      const state = (((node as any).lifecycle_state || 'active').toLowerCase()) as keyof typeof counts;
      if (state in counts && state !== 'all') {
        counts[state]++;
      } else {
        counts.active++;
      }
    }
    return counts;
  }, [results]);

  const filteredResults = React.useMemo(() => {
    if (lifecycleFilter === 'all') return results;
    return results.filter(n => (((n as any).lifecycle_state || 'active').toLowerCase()) === lifecycleFilter);
  }, [results, lifecycleFilter]);

  const handleSearchChange = useCallback((value: string) => {
    setSearchQuery(value);
    if (searchTimeoutRef.current) clearTimeout(searchTimeoutRef.current);
    if (!value.trim()) {
      setSearchResults([]);
      return;
    }
    searchTimeoutRef.current = setTimeout(async () => {
      if (!bridge) return;
      try {
        const res = await bridge.recall(value, 10);
        setSearchResults(res);
      } catch (e) {
        console.error('Search failed:', e);
      }
    }, 400);
  }, [bridge]);

  const loadRecent = useCallback(async () => {
    if (!bridge) return;
    setSearching(true);
    try {
      const res = await bridge.getSilvaGraph(500, false);
      const loadedNodes = res.nodes as any || [];
      const loadedEdges = res.edges as any || [];
      // Empty DB is normal for fresh installs — show empty state, not fake data
      if (loadedNodes.length === 0) {
        notify('No memories found. Use tylluan_remember to add knowledge.', 'info');
      }
      setResults(loadedNodes);
      setEdges(loadedEdges);
    } catch (e) {
      notify('Failed to load knowledge graph', 'error');
      setResults([]);
      setEdges([]);
    }
    setSearching(false);
  }, [bridge, notify]);

  const runClustering = async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      await bridge.maintenance_reindex();
      notify('Reindexacion y clustering iniciados en background', 'info');
      setTimeout(() => {
        void loadRecent();
      }, 2000);
    } catch {
      notify('Fallo al iniciar reindexacion', 'error');
    }
    setLoading(false);
  };

  useEffect(() => {
    if (activeSubView === 'list') {
      loadRecent();
    }
  }, [activeSubView, loadRecent]);

  const loadRecentNodes = useCallback(async () => {
    if (!bridge) return;
    try {
      const res = await bridge.getRecentNodes(10);
      setRecentNodes(res);
    } catch (e) {
      console.error('Failed to load recent nodes:', e);
      setRecentNodes([]);
    } finally {
      setRecentLoaded(true);
    }
  }, [bridge]);

  useEffect(() => {
    if (showRecentSidebar) loadRecentNodes();
  }, [showRecentSidebar, loadRecentNodes]);

  const handleSearch = async () => {
    if (!query.trim() || !bridge) {
      loadRecent();
      return;
    }
    setSearching(true);
    try {
      const res = await bridge.fetchRaw('/api/v1/memory/search', {
        method: 'POST',
        body: JSON.stringify({ query, limit: 50 })
      });
      setResults(res.nodes || []);
    } catch (e) {
      setResults([]);
      notify('Memory search failed', 'error');
    }
    setSearching(false);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    const files = Array.from(e.dataTransfer.files);
    if (!files.length || !bridge) return;
    setUploading(true);
    let ok = 0, fail = 0;
    for (const file of files) {
      const form = new FormData();
      form.append('file', file);
      try {
        const res = await bridge.fetchRaw('/api/v1/ingest/upload', { method: 'POST', body: form });
        if (res.status === 'ingested') ok++; else fail++;
      } catch { fail++; }
    }
    setUploading(false);
    notify(`Ingested ${ok} file${ok !== 1 ? 's' : ''}${fail ? `, ${fail} failed` : ''}`, fail ? 'error' : 'info');
    if (ok > 0) loadRecent();
  };

  const handleIngestComplete = useCallback(() => {
    window.dispatchEvent(new CustomEvent('silva_graph_refresh'));
  }, []);

  return (
    <div className="flex-1 min-h-0 flex flex-col space-y-4 h-full">
      {/* Top Selector Bar */}
      <div className="flex items-center justify-between border-b border-slate-800 pb-3">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center">
            <Network className="w-5 h-5 text-emerald-400" />
          </div>
          <div>
            <h2 className="text-sm font-semibold text-slate-50">Cortex Knowledge</h2>
            <p className="text-[10px] text-slate-500">SilvaDB visualizer & search engine</p>
          </div>
        </div>

        <div className="flex bg-slate-900 rounded-lg p-1 gap-1 items-center shrink-0">
          <button 
            type="button" 
            onClick={() => setActiveSubView('graph')} 
            className={cn(
              "px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer",
              activeSubView === 'graph' ? "bg-emerald-500/10 text-emerald-400" : "text-slate-500 hover:text-slate-300"
            )}
          >
            <Network className="w-3.5 h-3.5" /> Graph Canvas
          </button>
          <button 
            type="button" 
            onClick={() => setActiveSubView('list')} 
            className={cn(
              "px-3 py-1.5 rounded-md text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer",
              activeSubView === 'list' ? "bg-emerald-500/10 text-emerald-400" : "text-slate-500 hover:text-slate-300"
            )}
          >
            <List className="w-3.5 h-3.5" /> List Explorer
          </button>
        </div>
      </div>

      {/* Main Workspace Panels */}
      {activeSubView === 'graph' ? (
        <div className="flex-1 min-h-0 flex flex-col gap-3 animate-in fade-in duration-300">
          {bridge ? (
            <>
              <div className="flex-1 min-h-0 flex flex-col">
                <Suspense fallback={
                  <div className="flex-1 min-h-0 rounded-xl bg-slate-900/60 flex items-center justify-center gap-2 text-xs text-slate-500">
                    <RefreshCw className="w-4 h-4 animate-spin" /> Cargando cortex 3D...
                  </div>
                }>
                  <KnowledgeCortex3D bridge={bridge} events={events} />
                </Suspense>
              </div>
              <IngestPanel bridge={bridge} notify={notify} onIngestComplete={handleIngestComplete} />
            </>
          ) : (
            <div className="h-full rounded-xl bg-slate-900/60 flex items-center justify-center">
              <div className="flex items-center gap-2 text-xs text-slate-600">
                <RefreshCw className="w-4 h-4 animate-spin" />
                Esperando conexion con SilvaDB
              </div>
            </div>
          )}
        </div>
      ) : (
        <div className="flex-1 min-h-0 flex flex-col space-y-4 animate-in fade-in duration-300">
          <div className="flex items-center justify-between gap-4">
            <div className="flex gap-2 flex-1 max-w-3xl items-center">
              {results.length > 0 && (
                <span className="text-[10px] font-medium text-slate-500 whitespace-nowrap">{results.length} patterns</span>
              )}
              <div className="flex-1 relative">
                <SearchIcon className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => handleSearchChange(e.target.value)}
                  onFocus={() => searchResults.length > 0 && setShowSearchPanel(true)}
                  placeholder="Buscar en memoria..."
                  className="w-full pl-10 pr-10 py-2 bg-slate-900/80 rounded-lg text-sm focus:ring-1 ring-emerald-500 transition-all"
                />
                {searchQuery && (
                  <button
                    onClick={() => { setSearchQuery(''); setSearchResults([]); }}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-50"
                  >
                    <X className="w-4 h-4" />
                  </button>
                )}
              </div>
              <button
                type="button"
                onClick={() => setShowRecentSidebar(!showRecentSidebar)}
                className={cn(
                  "p-2 rounded-lg transition-all",
                  showRecentSidebar ? "bg-emerald-500/20 text-emerald-400" : "bg-slate-900 text-slate-500 hover:text-slate-50"
                )}
                title="Últimas 24h"
              >
                <Clock className="w-4 h-4" />
              </button>
              <button
                onClick={runClustering}
                disabled={loading}
                className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-blue-500/10 text-xs font-medium text-blue-400 hover:bg-blue-500/20 transition-all"
              >
                <Zap className="w-3.5 h-3.5" /> Detectar Comunidades
              </button>
              <button
                onClick={loadRecent}
                disabled={loading}
                className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-slate-900 text-xs text-slate-400 hover:text-slate-200 transition-colors"
              >
                <RefreshCw className={cn("w-3.5 h-3.5", loading && "animate-spin")} /> Actualizar
              </button>
              <button
                type="button"
                onClick={handleSearch}
                disabled={searching}
                className="px-4 py-2 bg-emerald-500/20 hover:bg-emerald-500/30 text-emerald-400 rounded-lg text-sm font-medium flex items-center gap-2"
              >
                {searching ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Database className="w-4 h-4" />} Explore
              </button>
            </div>
            <div className="flex bg-slate-900 rounded-lg p-1 shrink-0 gap-1 items-center">
              <button type="button" onClick={() => setView('grid')} className={cn("px-2 py-1 rounded text-[10px] font-medium cursor-pointer transition-colors", view === 'grid' ? "bg-slate-800 text-emerald-400" : "text-slate-500 hover:text-slate-300")}>Grid</button>
              <button type="button" onClick={() => setView('table')} className={cn("px-2 py-1 rounded text-[10px] font-medium cursor-pointer transition-colors", view === 'table' ? "bg-slate-800 text-emerald-400" : "text-slate-500 hover:text-slate-300")}>Table</button>
              <div className="w-px bg-slate-800 self-stretch my-0.5 mx-1"></div>
              <button 
                type="button" 
                onClick={() => setCompactMode(!compactMode)} 
                className={cn(
                  "px-2 py-1 rounded text-[10px] font-medium cursor-pointer transition-colors",
                  compactMode ? "bg-emerald-500/10 text-emerald-400" : "text-slate-500 hover:text-slate-300"
                )}
              >
                {compactMode ? "Compact" : "Full View"}
              </button>
            </div>
          </div>

          {showSearchPanel && searchResults.length > 0 && (
            <div className="absolute z-20 mt-12 w-80 max-h-96 overflow-y-auto bg-slate-900 rounded-xl shadow-2xl">
              <div className="sticky top-0 bg-slate-800 px-3 py-2 flex items-center justify-between">
                <span className="text-[10px] font-medium text-slate-400">Resultados</span>
                <button onClick={() => setShowSearchPanel(false)}><X className="w-3 h-3 text-slate-500" /></button>
              </div>
              {searchResults.map((node, i) => (
                <div
                  key={i}
                  className="px-3 py-2 border-b border-slate-800 hover:bg-slate-800/50 cursor-pointer"
                  onClick={() => {
                    setQuery(node.content || node.id);
                    handleSearch();
                    setShowSearchPanel(false);
                  }}
                >
                  <div className="flex items-center gap-2 mb-1">
                    <span className="text-[8px] font-medium text-violet-400">{node.node_type || 'node'}</span>
                  </div>
                  <p className="text-xs text-slate-300 line-clamp-2">{node.content || node.label || node.id}</p>
                </div>
              ))}
            </div>
          )}

          <div
            onDragOver={(e) => { e.preventDefault(); setIsDragging(true); }}
            onDragLeave={() => setIsDragging(false)}
            onDrop={handleDrop}
            className={cn(
              "border-2 border-dashed rounded-xl px-6 py-4 flex items-center gap-4 transition-all cursor-default",
              isDragging ? "border-emerald-500 bg-emerald-500/10 scale-[1.01]" : "border-slate-800 hover:border-slate-600"
            )}
          >
            {uploading
              ? <RefreshCw className="w-5 h-5 text-emerald-400 animate-spin flex-shrink-0" />
              : <Save className="w-5 h-5 text-slate-600 flex-shrink-0" />}
            <div>
              <p className="text-xs font-bold text-slate-400">{isDragging ? 'Suelta para ingestar' : 'Arrastra archivos aquí para ingestar en SilvaDB'}</p>
              <p className="text-[10px] text-slate-600 mt-0.5">.md .txt .py .js .ts .rs .json .yaml .toml .pdf · .png .jpg .jpeg .webp</p>
            </div>
          </div>

          {/* IVF Index Status Widget */}
          <div className="flex items-center gap-3 px-4 py-2.5 rounded-lg bg-slate-900/80">
            <Layers className="w-4 h-4 text-slate-500 flex-shrink-0" />
            <span className="text-[10px] font-medium text-slate-500">IVF Index</span>
            <div className="flex items-center gap-1.5 ml-1">
              {memoryStats?.ivf_ready ? (
                <>
                  <CheckCircle className="w-3.5 h-3.5 text-emerald-400" />
                  <span className="text-[11px] font-semibold text-emerald-400">READY</span>
                </>
              ) : (
                <>
                  <AlertCircle className="w-3.5 h-3.5 text-amber-400" />
                  <span className="text-[11px] font-semibold text-amber-400">BUILDING…</span>
                </>
              )}
            </div>
            <div className="h-3 w-px bg-slate-700 mx-1" />
            <div className="flex items-center gap-1">
              <span className="text-[10px] text-slate-500">centroids:</span>
              <span className={cn(
                "text-[11px] font-mono font-bold",
                (memoryStats?.n_centroids ?? 0) > 0 ? "text-violet-400" : "text-slate-600"
              )}>
                {memoryStats?.n_centroids ?? '—'}
              </span>
            </div>
            <div className="h-3 w-px bg-slate-700 mx-1" />
            <div className="flex items-center gap-1">
              <span className="text-[10px] text-slate-500">last build:</span>
              <span className="text-[11px] font-mono text-slate-400">
                {memoryStats?.last_build != null
                  ? `rowid ${memoryStats.last_build}`
                  : '—'}
              </span>
            </div>
            {!memoryStats?.ivf_ready && (memoryStats?.node_count ?? 0) < 50 && (
              <span className="ml-auto text-[9px] text-slate-600 italic">&lt;50 embeddings — linear scan activo</span>
            )}
          </div>

          {/* Lifecycle Filter & Summary Bar */}
          <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 rounded-lg bg-slate-900/60 border border-slate-800/80">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-[10px] text-slate-500 font-bold uppercase tracking-wider">Ciclo de Vida:</span>
              <div className="flex items-center gap-1 bg-slate-950/60 p-0.5 rounded-md border border-slate-800">
                {(['all', 'active', 'quiet', 'consolidated', 'archived'] as const).map((filterKey) => {
                  const count = lifecycleCounts[filterKey];
                  const isActive = lifecycleFilter === filterKey;
                  return (
                    <button
                      key={filterKey}
                      type="button"
                      onClick={() => setLifecycleFilter(filterKey)}
                      className={cn(
                        "px-2 py-0.5 rounded text-[10px] font-medium font-mono uppercase transition-all flex items-center gap-1 cursor-pointer",
                        isActive 
                          ? filterKey === 'active' ? "bg-emerald-500/20 text-emerald-300 border border-emerald-500/40"
                          : filterKey === 'quiet' ? "bg-amber-500/20 text-amber-300 border border-amber-500/40"
                          : filterKey === 'consolidated' ? "bg-indigo-500/20 text-indigo-300 border border-indigo-500/40"
                          : filterKey === 'archived' ? "bg-slate-700/60 text-slate-300 border border-slate-600"
                          : "bg-slate-800 text-slate-200 border border-slate-700"
                          : "text-slate-500 hover:text-slate-300 hover:bg-slate-900 border border-transparent"
                      )}
                    >
                      <span>{filterKey}</span>
                      <span className={cn(
                        "px-1 text-[9px] rounded-full",
                        isActive ? "bg-slate-950/80 text-white font-bold" : "text-slate-600"
                      )}>{count}</span>
                    </button>
                  );
                })}
              </div>
            </div>
            {lifecycleFilter !== 'all' && (
              <span className="text-[10px] text-slate-500 italic">
                Mostrando {filteredResults.length} de {results.length} nodos
              </span>
            )}
          </div>

          <div className="flex-1 min-h-0 overflow-y-auto">
            {view === 'grid' && (
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                {filteredResults.map((node, i) => {
                  const nodeType = (node as any).node_type || (node as any).type || 'entity';
                  const nodeContent = fixDoubleEncoding(node.content || (node as any).label || '—');
                  return (
                    <div key={i} className={cn("group p-4 rounded-lg transition-all relative overflow-hidden", 
                      (node.provenance === 'federation_peer' || node.provenance === 'unverified') ? "bg-slate-900/50 hover:bg-slate-800/50" :
                      nodeType === 'consolidated_summary' ? "bg-indigo-950/10 hover:bg-indigo-900/20" : "bg-slate-900/50 hover:bg-slate-800/50"
                    )}>
                      <div className="flex items-center gap-2 mb-3">
                        <div className={cn("w-2 h-2 rounded-full flex-shrink-0",
                          nodeType === 'consolidated_summary' ? "bg-indigo-400" :
                          nodeType === 'lesson' ? "bg-violet-500" :
                          nodeType === 'identity' ? "bg-emerald-500" :
                          nodeType === 'concept' ? "bg-blue-500" : "bg-amber-500"
                        )}></div>
                        <span className="text-[10px] font-medium text-slate-500">{nodeType}</span>
                        <LifecycleBadge state={(node as any).lifecycle_state || 'active'} />
                        {node.content?.startsWith('[DEPRECATED by') && (
                          <span className="px-1.5 py-0.5 rounded bg-red-500/10 text-[9px] font-medium text-red-400 border border-red-500/20 animate-pulse">DEPRECATED</span>
                        )}
                        <span className="text-[9px] font-mono text-slate-600 ml-auto">{node.id.split(':').pop()?.slice(0, 8)}</span>
                      </div>
                      <div className="text-xs text-slate-300 leading-relaxed mb-4 min-h-[4.5rem]">
                        {(() => {
                          const isExpanded = expandedNodeIds[node.id];
                          if (compactMode && !isExpanded && nodeContent.length > 500) {
                            return (
                              <>
                                <span className="line-clamp-4 block">{nodeContent.slice(0, 500)}...</span>
                                <button
                                  type="button"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    setExpandedNodeIds(prev => ({ ...prev, [node.id]: true }));
                                  }}
                                  className="text-emerald-400 hover:text-emerald-300 mt-1 font-bold text-[10px] underline cursor-pointer"
                                >
                                  [Ver completo: {node.id.split(':').pop()?.slice(0, 8)}]
                                </button>
                              </>
                            );
                          } else if (compactMode && isExpanded) {
                            return (
                              <div className="flex flex-col gap-2">
                                <span className="whitespace-pre-wrap">{nodeContent}</span>
                                {nodeType === 'consolidated_summary' && (
                                  <div className="mt-2 p-2 rounded-md bg-indigo-500/5 border border-indigo-500/20">
                                    <span className="text-[10px] font-bold text-indigo-400 uppercase block mb-1">Fuentes Originales:</span>
                                    <ul className="space-y-1">
                                      {edges.filter(e => e.target === node.id && (e.type === 'consolidated_into' || e.edge_type === 'consolidated_into')).length > 0 ? (
                                        edges.filter(e => e.target === node.id && (e.type === 'consolidated_into' || e.edge_type === 'consolidated_into')).map((edge, j) => (
                                          <li key={j} className="text-[10px] text-slate-400 font-mono flex items-center gap-1.5 cursor-pointer hover:text-indigo-300" onClick={() => setQuery(edge.source)}>
                                            <Network className="w-3 h-3 text-indigo-500" />
                                            {edge.source}
                                          </li>
                                        ))
                                      ) : (
                                        <li className="text-[10px] text-slate-500 italic">No se encontraron fuentes cargadas</li>
                                      )}
                                    </ul>
                                  </div>
                                )}
                                <button
                                  type="button"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    setExpandedNodeIds(prev => ({ ...prev, [node.id]: false }));
                                  }}
                                   className="text-slate-500 hover:text-slate-300 mt-1 font-medium text-[10px] underline cursor-pointer self-start"
                                >
                                  [Minimizar]
                                </button>
                              </div>
                            );
                          } else {
                            return (
                              <div className="flex flex-col gap-2">
                                <span className={cn(compactMode && "block")}>{nodeContent}</span>
                                {nodeType === 'consolidated_summary' && (
                                  <div className="mt-2 p-2 rounded-md bg-indigo-500/5 border border-indigo-500/20">
                                    <span className="text-[10px] font-bold text-indigo-400 uppercase block mb-1">Fuentes Originales:</span>
                                    <ul className="space-y-1">
                                      {edges.filter(e => e.target === node.id && (e.type === 'consolidated_into' || e.edge_type === 'consolidated_into')).length > 0 ? (
                                        edges.filter(e => e.target === node.id && (e.type === 'consolidated_into' || e.edge_type === 'consolidated_into')).map((edge, j) => (
                                          <li key={j} className="text-[10px] text-slate-400 font-mono flex items-center gap-1.5 cursor-pointer hover:text-indigo-300" onClick={() => setQuery(edge.source)}>
                                            <Network className="w-3 h-3 text-indigo-500" />
                                            {edge.source}
                                          </li>
                                        ))
                                      ) : (
                                        <li className="text-[10px] text-slate-500 italic">No se encontraron fuentes cargadas</li>
                                      )}
                                    </ul>
                                  </div>
                                )}
                                {compactMode && isExpanded && nodeContent.length > 500 && (
                                  <button
                                    type="button"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      setExpandedNodeIds(prev => ({ ...prev, [node.id]: false }));
                                    }}
                                    className="text-emerald-400 hover:text-emerald-300 mt-1 font-medium text-[10px] underline cursor-pointer"
                                  >
                                    [Ver menos]
                                  </button>
                                )}
                              </div>
                            );
                          }
                        })()}
                      </div>
                      <div className="flex items-center justify-between border-t border-slate-800/50 pt-3">
                        <div className="flex items-center gap-3">
                          <div className="flex flex-col">
                            <span className="text-[8px] text-slate-600 uppercase">Weight</span>
                            <span className="text-[10px] font-bold text-emerald-500">{(node.weight || 0).toFixed(2)}</span>
                          </div>
                          {node.provenance && (
                            <div className="flex flex-col border-l border-slate-700/50 pl-3">
                              <span className="text-[8px] text-slate-600 uppercase">Provenance</span>
                               <span className={cn("text-[10px] font-medium flex items-center gap-1", 
                                (node.provenance === 'federation_peer' || node.provenance === 'unverified') ? "text-amber-500" : "text-slate-400"
                              )}>
                                {(node.provenance === 'federation_peer' || node.provenance === 'unverified') && <ShieldAlert className="w-3 h-3" />}
                                {getProvenanceLabel(node.provenance)}
                              </span>
                            </div>
                          )}
                        </div>
                        <button type="button" title="Search related" onClick={() => setQuery(node.id)} className="p-1 hover:bg-slate-700 rounded transition-colors">
                          <Search className="w-3 h-3 text-slate-500" />
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}

            {view === 'table' && (
              <div className="rounded-lg bg-slate-900/50 overflow-hidden overflow-x-auto">
                <table className="w-full text-left border-collapse">
                  <thead>
                    <tr className="bg-slate-800/50 text-[11px] text-slate-500">
                      <th className="px-4 py-3 font-medium">Identifier</th>
                      <th className="px-4 py-3 font-medium">Type</th>
                      <th className="px-4 py-3 font-medium">Content</th>
                      <th className="px-4 py-3 font-medium text-right">Weight</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800">
                    {filteredResults.map((node, i) => {
                      const nodeType = (node as any).node_type || (node as any).type || 'entity';
                      const nodeContent = fixDoubleEncoding(node.content || (node as any).label || '—');
                      return (
                        <tr key={i} className="hover:bg-slate-800/30 transition-colors cursor-pointer" onClick={() => setQuery(node.id)}>
                          <td className="px-4 py-3 text-[10px] font-mono text-violet-400 max-w-[120px] truncate">{node.id}</td>
                          <td className="px-4 py-3">
                            <span className={cn("px-1.5 py-0.5 rounded text-[9px] font-medium",
                              nodeType === 'consolidated_summary' ? "bg-indigo-500/20 text-indigo-400" : "bg-slate-800 text-slate-400"
                            )}>
                              {nodeType === 'consolidated_summary' ? 'SYNTHESIS' : nodeType}
                            </span>
                            <LifecycleBadge state={(node as any).lifecycle_state || 'active'} className="ml-1.5" />
                            {node.content?.startsWith('[DEPRECATED by') && (
                              <span className="ml-1.5 px-1.5 py-0.5 rounded bg-red-500/10 text-[9px] font-medium text-red-400 border border-red-500/20">DEPRECATED</span>
                            )}
                            {(node.provenance === 'federation_peer' || node.provenance === 'unverified') && (
                              <span className="ml-1.5 px-1.5 py-0.5 rounded bg-amber-500/10 text-[9px] font-medium text-amber-500 border border-amber-500/20 inline-flex items-center gap-1">
                                <ShieldAlert className="w-2.5 h-2.5" /> EXTERNAL
                              </span>
                            )}
                          </td>
                          <td className="px-4 py-3 text-xs text-slate-400 max-w-md">
                            {(() => {
                              const isExpanded = expandedNodeIds[node.id];
                              if (compactMode && !isExpanded && nodeContent.length > 100) {
                                  return (
                                    <div className="flex items-center gap-1.5">
                                      <span className="truncate max-w-xs block">{nodeContent.slice(0, 100)}...</span>
                                      <button
                                        type="button"
                                        onClick={(e) => {
                                          e.stopPropagation();
                                          setExpandedNodeIds(prev => ({ ...prev, [node.id]: true }));
                                        }}
                                         className="text-emerald-400 hover:text-emerald-300 font-medium text-[10px] underline whitespace-nowrap cursor-pointer"
                                      >
                                        [Ver completo]
                                      </button>
                                    </div>
                                  );
                              } else {
                                  return (
                                    <div className="whitespace-normal break-words">
                                      <span>{nodeContent}</span>
                                      {compactMode && isExpanded && nodeContent.length > 100 && (
                                        <button
                                          type="button"
                                          onClick={(e) => {
                                            e.stopPropagation();
                                            setExpandedNodeIds(prev => ({ ...prev, [node.id]: false }));
                                          }}
                                          className="text-emerald-400 hover:text-emerald-300 ml-1.5 font-bold text-[10px] underline whitespace-nowrap cursor-pointer"
                                        >
                                          [Ver menos]
                                        </button>
                                      )}
                                    </div>
                                  );
                              }
                            })()}
                          </td>
                          <td className="px-4 py-3 text-right text-[10px] font-medium text-emerald-400">{(node.weight || 0).toFixed(2)}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}

            {/* Recent Nodes Sidebar */}
            {showRecentSidebar && (
              <div className="absolute left-4 top-32 w-72 max-h-[60vh] bg-slate-900/95 backdrop-blur-md rounded-xl shadow-2xl z-10 overflow-hidden flex flex-col">
                <div className="px-4 py-3 bg-slate-800/50 border-b border-slate-700 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Clock className="w-4 h-4 text-emerald-400" />
                    <span className="text-[10px] font-medium text-slate-300">Últimas 24h</span>
                  </div>
                  <button onClick={() => setShowRecentSidebar(false)} className="text-slate-500 hover:text-slate-50">
                    <X className="w-4 h-4" />
                  </button>
                </div>
                <div className="flex-1 overflow-y-auto p-2">
                  {!recentLoaded ? (
                    <p className="text-xs text-slate-500 text-center py-4">Cargando...</p>
                  ) : recentNodes.length === 0 ? (
                    <p className="text-xs text-slate-500 text-center py-4">Sin datos recientes</p>
                  ) : (
                    recentNodes.map((node, i) => {
                      const timeAgo = node.created_at 
                        ? (() => {
                            const diff = Date.now() - new Date(node.created_at).getTime();
                            const mins = Math.floor(diff / 60000);
                            if (mins < 60) return `hace ${mins}m`;
                            const hours = Math.floor(mins / 60);
                            if (hours < 24) return `hace ${hours}h`;
                            return `${Math.floor(hours / 24)}d`;
                          })()
                        : '';
                      return (
                        <div
                          key={i}
                          className="px-3 py-2 mb-1 rounded-lg hover:bg-slate-800/50 cursor-pointer transition-colors"
                          onClick={() => {
                            setQuery(node.content || node.id);
                            handleSearch();
                            setShowRecentSidebar(false);
                          }}
                        >
                          <div className="flex items-center gap-2 mb-1">
                            <span className={cn(
                              "text-[9px] font-medium px-1 py-0.5 rounded",
                              node.node_type === 'agent_memory' ? "bg-emerald-500/20 text-emerald-400" :
                              node.node_type === 'concept' ? "bg-blue-500/20 text-blue-400" :
                              node.node_type === 'fact' ? "bg-orange-500/20 text-orange-400" :
                              "bg-slate-700 text-slate-400"
                            )}>{node.node_type?.slice(0, 8) || 'node'}</span>
                            <LifecycleBadge state={(node as any).lifecycle_state || 'active'} className="ml-1" />
                            {node.content?.startsWith('[DEPRECATED by') && (
                              <span className="px-1 py-0.5 rounded bg-red-500/10 text-[9px] font-medium text-red-400 border border-red-500/20">DEPRECATED</span>
                            )}
                            <span className="text-[9px] text-slate-600 ml-auto">{timeAgo}</span>
                          </div>
                          <p className="text-xs text-slate-300 line-clamp-2">{node.content?.slice(0, 80) || node.id}</p>
                        </div>
                      );
                    })
                  )}
                </div>
              </div>
            )}

            {results.length === 0 && !searching && (
              <div className="py-20 text-center flex flex-col items-center">
                <Database className="w-12 h-12 text-slate-800 mb-4" />
                <p className="text-slate-600 font-medium">No neural patterns match your scan</p>
                <button type="button" onClick={loadRecent} className="mt-4 text-xs text-emerald-500 hover:underline">Reset Scan</button>
              </div>
            )}

            {results.length > 0 && filteredResults.length === 0 && !searching && (
              <div className="py-16 text-center flex flex-col items-center">
                <Database className="w-10 h-10 text-slate-800 mb-3" />
                <p className="text-slate-500 text-xs font-medium">No hay nodos en estado <span className="font-mono font-bold text-slate-400 uppercase">"{lifecycleFilter}"</span></p>
                <button type="button" onClick={() => setLifecycleFilter('all')} className="mt-3 text-xs text-emerald-400 hover:underline">Ver todos los nodos ({results.length})</button>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
