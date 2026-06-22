import React, { useState, useEffect, useCallback, useRef } from 'react';
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
  List
} from 'lucide-react';
import type { NexusBridge, GraphNode } from '../lib/nexus-bridge';
import type { MemoryStats } from '../hooks/useNexus';
import { useNexus } from '../hooks/useNexus';
import { cn } from '../lib/utils';
import { HippocampusGraph } from './HippocampusGraph';
import { IngestPanel } from './IngestPanel';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  memoryStats?: MemoryStats | null;
}

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
      setResults(res.nodes as any || []);
    } catch (e) {
      notify('Failed to load recent memories', 'error');
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
            <h2 className="text-sm font-bold text-white uppercase tracking-wider">Cortex Knowledge</h2>
            <p className="text-[10px] text-slate-500 font-mono">SilvaDB visualizer & search engine</p>
          </div>
        </div>

        <div className="flex bg-slate-900 rounded-lg p-1 border border-slate-800 gap-1 items-center shrink-0">
          <button 
            type="button" 
            onClick={() => setActiveSubView('graph')} 
            className={cn(
              "px-3 py-1.5 rounded-md text-xs font-bold flex items-center gap-1.5 transition-all cursor-pointer",
              activeSubView === 'graph' ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20" : "text-slate-500 hover:text-slate-300"
            )}
          >
            <Network className="w-3.5 h-3.5" /> GRAPH CANVAS
          </button>
          <button 
            type="button" 
            onClick={() => setActiveSubView('list')} 
            className={cn(
              "px-3 py-1.5 rounded-md text-xs font-bold flex items-center gap-1.5 transition-all cursor-pointer",
              activeSubView === 'list' ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20" : "text-slate-500 hover:text-slate-300"
            )}
          >
            <List className="w-3.5 h-3.5" /> LIST EXPLORER
          </button>
        </div>
      </div>

      {/* Main Workspace Panels */}
      {activeSubView === 'graph' ? (
        <div className="flex-1 min-h-0 flex flex-col gap-3 animate-in fade-in duration-300">
          {bridge ? (
            <>
              <div className="flex-1 min-h-0 flex flex-col">
                <HippocampusGraph bridge={bridge} events={events} />
              </div>
              <IngestPanel bridge={bridge} notify={notify} onIngestComplete={handleIngestComplete} />
            </>
          ) : (
            <div className="h-full rounded-xl border border-slate-800 bg-slate-950 flex items-center justify-center">
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
                <span className="text-[10px] font-mono text-slate-500 whitespace-nowrap">{results.length} patterns</span>
              )}
              <div className="flex-1 relative">
                <SearchIcon className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => handleSearchChange(e.target.value)}
                  onFocus={() => searchResults.length > 0 && setShowSearchPanel(true)}
                  placeholder="Search memory..."
                  className="w-full pl-10 pr-10 py-2 bg-slate-900/80 border border-slate-800 rounded-lg text-sm focus:ring-1 ring-emerald-500 transition-all"
                />
                {searchQuery && (
                  <button
                    onClick={() => { setSearchQuery(''); setSearchResults([]); }}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-white"
                  >
                    <X className="w-4 h-4" />
                  </button>
                )}
              </div>
              <button
                type="button"
                onClick={() => setShowRecentSidebar(!showRecentSidebar)}
                className={cn(
                  "p-2 rounded-lg border transition-all",
                  showRecentSidebar ? "bg-emerald-500/20 border-emerald-500 text-emerald-400" : "bg-slate-900 border-slate-800 text-slate-500 hover:text-white"
                )}
                title="Últimas 24h"
              >
                <Clock className="w-4 h-4" />
              </button>
              <button
                onClick={runClustering}
                disabled={loading}
                className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-blue-500/10 border border-blue-500/20 text-xs font-bold text-blue-400 hover:bg-blue-500/20 transition-all"
              >
                <Zap className="w-3.5 h-3.5" /> Detectar Comunidades
              </button>
              <button
                onClick={loadRecent}
                disabled={loading}
                className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-slate-900 border border-slate-800 text-xs text-slate-400 hover:text-slate-200 transition-colors"
              >
                <RefreshCw className={cn("w-3.5 h-3.5", loading && "animate-spin")} /> Actualizar
              </button>
              <button
                type="button"
                onClick={handleSearch}
                disabled={searching}
                className="px-4 py-2 bg-emerald-500/20 hover:bg-emerald-500/30 text-emerald-400 rounded-lg text-sm font-bold flex items-center gap-2"
              >
                {searching ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Database className="w-4 h-4" />} Explore
              </button>
            </div>
            <div className="flex bg-slate-900 rounded-lg p-1 border border-slate-800 shrink-0 gap-1 items-center">
              <button type="button" onClick={() => setView('grid')} className={cn("px-2 py-1 rounded text-[10px] font-bold cursor-pointer transition-colors", view === 'grid' ? "bg-slate-800 text-emerald-400" : "text-slate-500 hover:text-slate-300")}>GRID</button>
              <button type="button" onClick={() => setView('table')} className={cn("px-2 py-1 rounded text-[10px] font-bold cursor-pointer transition-colors", view === 'table' ? "bg-slate-800 text-emerald-400" : "text-slate-500 hover:text-slate-300")}>TABLE</button>
              <div className="w-px bg-slate-800 self-stretch my-0.5 mx-1"></div>
              <button 
                type="button" 
                onClick={() => setCompactMode(!compactMode)} 
                className={cn(
                  "px-2 py-1 rounded text-[10px] font-bold cursor-pointer transition-colors",
                  compactMode ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20" : "text-slate-500 hover:text-slate-300"
                )}
              >
                {compactMode ? "COMPACT" : "FULL VIEW"}
              </button>
            </div>
          </div>

          {showSearchPanel && searchResults.length > 0 && (
            <div className="absolute z-20 mt-12 w-80 max-h-96 overflow-y-auto bg-slate-900 border border-slate-700 rounded-xl shadow-2xl">
              <div className="sticky top-0 bg-slate-800 px-3 py-2 flex items-center justify-between">
                <span className="text-[10px] font-bold text-slate-400 uppercase">Resultados</span>
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
                    <span className="text-[8px] font-bold uppercase text-violet-400">{node.node_type || 'node'}</span>
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
          <div className="flex items-center gap-3 px-4 py-2.5 rounded-xl bg-slate-900/80 border border-slate-800">
            <Layers className="w-4 h-4 text-slate-500 flex-shrink-0" />
            <span className="text-[10px] font-bold uppercase tracking-widest text-slate-500">IVF Index</span>
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

          <div className="flex-1 min-h-0 overflow-y-auto">
            {view === 'grid' && (
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                {results.map((node, i) => {
                  const nodeType = (node as any).node_type || (node as any).type || 'entity';
                  const nodeContent = fixDoubleEncoding(node.content || (node as any).label || '—');
                  return (
                    <div key={i} className="group p-4 rounded-xl border border-slate-800 bg-slate-900/50 hover:bg-slate-800/50 transition-all relative overflow-hidden">
                      <div className="flex items-center gap-2 mb-3">
                        <div className={cn("w-2 h-2 rounded-full flex-shrink-0",
                          nodeType === 'lesson' ? "bg-violet-500" :
                          nodeType === 'identity' ? "bg-emerald-500" :
                          nodeType === 'concept' ? "bg-blue-500" : "bg-amber-500"
                        )}></div>
                        <span className="text-[10px] font-bold uppercase tracking-widest text-slate-500">{nodeType}</span>
                        {node.content?.startsWith('[DEPRECATED by') && (
                          <span className="px-1.5 py-0.5 rounded bg-red-500/10 text-[8px] font-extrabold uppercase text-red-400 border border-red-500/20 animate-pulse">DEPRECATED</span>
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
                          } else {
                            return (
                              <>
                                <span className={cn(compactMode && "block")}>{nodeContent}</span>
                                {compactMode && isExpanded && nodeContent.length > 500 && (
                                  <button
                                    type="button"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      setExpandedNodeIds(prev => ({ ...prev, [node.id]: false }));
                                    }}
                                    className="text-emerald-400 hover:text-emerald-300 mt-1 font-bold text-[10px] underline cursor-pointer"
                                  >
                                    [Ver menos]
                                  </button>
                                )}
                              </>
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
              <div className="rounded-xl border border-slate-800 bg-slate-900/50 overflow-hidden overflow-x-auto">
                <table className="w-full text-left border-collapse">
                  <thead>
                    <tr className="bg-slate-800/50 text-[10px] uppercase tracking-widest text-slate-500">
                      <th className="px-4 py-3 font-bold">Identifier</th>
                      <th className="px-4 py-3 font-bold">Type</th>
                      <th className="px-4 py-3 font-bold">Content</th>
                      <th className="px-4 py-3 font-bold text-right">Weight</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800">
                    {results.map((node, i) => {
                      const nodeType = (node as any).node_type || (node as any).type || 'entity';
                      const nodeContent = fixDoubleEncoding(node.content || (node as any).label || '—');
                      return (
                        <tr key={i} className="hover:bg-slate-800/30 transition-colors cursor-pointer" onClick={() => setQuery(node.id)}>
                          <td className="px-4 py-3 text-[10px] font-mono text-violet-400 max-w-[120px] truncate">{node.id}</td>
                          <td className="px-4 py-3">
                            <span className="px-1.5 py-0.5 rounded bg-slate-800 text-[9px] font-bold uppercase text-slate-400 border border-slate-700">{nodeType}</span>
                            {node.content?.startsWith('[DEPRECATED by') && (
                              <span className="ml-1.5 px-1.5 py-0.5 rounded bg-red-500/10 text-[8px] font-extrabold uppercase text-red-400 border border-red-500/20">DEPRECATED</span>
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
                                        className="text-emerald-400 hover:text-emerald-300 font-bold text-[10px] underline whitespace-nowrap cursor-pointer"
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
                          <td className="px-4 py-3 text-right text-[10px] font-bold text-emerald-400">{(node.weight || 0).toFixed(2)}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}

            {/* Recent Nodes Sidebar */}
            {showRecentSidebar && (
              <div className="absolute left-4 top-32 w-72 max-h-[60vh] bg-slate-900/95 backdrop-blur-md border border-slate-700 rounded-xl shadow-2xl z-10 overflow-hidden flex flex-col">
                <div className="px-4 py-3 bg-slate-800/50 border-b border-slate-700 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Clock className="w-4 h-4 text-emerald-400" />
                    <span className="text-[10px] font-bold text-slate-300 uppercase">Últimas 24h</span>
                  </div>
                  <button onClick={() => setShowRecentSidebar(false)} className="text-slate-500 hover:text-white">
                    <X className="w-4 h-4" />
                  </button>
                </div>
                <div className="flex-1 overflow-y-auto p-2">
                  {recentNodes.length === 0 ? (
                    <p className="text-xs text-slate-500 text-center py-4">Cargando...</p>
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
                              "text-[8px] font-bold uppercase px-1 py-0.5 rounded",
                              node.node_type === 'agent_memory' ? "bg-emerald-500/20 text-emerald-400" :
                              node.node_type === 'concept' ? "bg-blue-500/20 text-blue-400" :
                              node.node_type === 'fact' ? "bg-orange-500/20 text-orange-400" :
                              "bg-slate-700 text-slate-400"
                            )}>{node.node_type?.slice(0, 8) || 'node'}</span>
                            {node.content?.startsWith('[DEPRECATED by') && (
                              <span className="px-1 py-0.5 rounded bg-red-500/10 text-[8px] font-extrabold uppercase text-red-400 border border-red-500/20">DEPRECATED</span>
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
          </div>
        </div>
      )}
    </div>
  );
}
