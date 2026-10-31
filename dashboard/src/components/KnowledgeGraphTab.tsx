import React, { useState, useEffect, useCallback, useRef, Suspense } from 'react';
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
import type { NexusBridge, GraphNode, NexusEvent } from '../lib/nexus-bridge';
import { LifecycleBadge } from '../lib/nexus-bridge';
import type { MemoryStats } from '../hooks/useNexus';
import { useNexus } from '../hooks/useNexus';
import { cn } from '../lib/utils';
import { IngestPanel } from './IngestPanel';

// The graph canvas is heavy enough to warrant code-splitting — only load it
// when the Knowledge tab is actually opened. 2D canvas renderer (see
// same semantics, a fraction of the cost).
function LightGraphPlaceholder({ bridge: _bridge }: { bridge: NexusBridge; events: NexusEvent[] }) {
  return (
    <div className="flex-1 min-h-[300px] rounded bg-slate-900/60 border border-slate-800 flex flex-col items-center justify-center p-6 text-center">
      <Network className="w-10 h-10 text-emerald-400/40 mb-3" />
      <h4 className="text-sm font-semibold text-slate-200">Explorador Estructural SilvaDB</h4>
      <p className="text-xs text-slate-400 max-w-md mt-1">
        Consulta recuerdos estructurados y relaciones directamente desde la pestana de Nodos para maxima velocidad y cero consumo de GPU/WebGL.
      </p>
    </div>
  );
}

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  memoryStats?: MemoryStats | null;
}

const getProvenanceLabel = (prov?: string) => {
  switch (prov) {
    case 'user_direct': return 'Source: Direct user';
    case 'agent_generated': return 'Source: Agent generated';
    case 'federation_peer': return 'Source: Federated peer (unverified)';
    case 'guild_output': return 'Source: Guild output';
    case 'unverified': return 'Source: Unverified';
    default: return prov || 'Unknown';
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

interface GraphEdge {
  source: string;
  target: string;
  type?: string;
  edge_type?: string;
}

interface ExtendedGraphNode extends Omit<GraphNode, 'lifecycle_state'> {
  lifecycle_state?: GraphNode['lifecycle_state'] | string;
  type?: string;
  label?: string;
}

export function KnowledgeGraphTab({ bridge, notify, memoryStats }: Props) {
  const { events } = useNexus();
  const [activeSubView, setActiveSubView] = useState<'graph' | 'list'>('graph');
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<ExtendedGraphNode[]>([]);
  const [edges, setEdges] = useState<GraphEdge[]>([]);
  const [searching, setSearching] = useState(false);
  const [fetchState, setFetchState] = useState<'idle' | 'loading' | 'success' | 'error'>('idle');
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [isBackgroundRefreshing, setIsBackgroundRefreshing] = useState(false);
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
  const [recentNodes, setRecentNodes] = useState<ExtendedGraphNode[]>([]);
  const [recentLoaded, setRecentLoaded] = useState(false);

  // --- Lifecycle Filter State ---
  const [lifecycleFilter, setLifecycleFilter] = useState<'all' | 'active' | 'quiet' | 'consolidated' | 'archived'>('all');

  const lifecycleCounts = React.useMemo(() => {
    const counts = { all: results.length, active: 0, quiet: 0, consolidated: 0, archived: 0 };
    for (const node of results) {
      const state = ((node.lifecycle_state || 'active').toLowerCase()) as keyof typeof counts;
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
    return results.filter(n => ((n.lifecycle_state || 'active').toLowerCase()) === lifecycleFilter);
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

  const loadRecent = useCallback(async (isBackground = false) => {
    if (!bridge) return;
    if (isBackground) {
      setIsBackgroundRefreshing(true);
    } else {
      setSearching(true);
      setFetchState('loading');
    }
    setFetchError(null);
    try {
      const res = await bridge.getSilvaGraph(500, false);
      const loadedNodes = (res.nodes || []) as ExtendedGraphNode[];
      const loadedEdges = (res.edges || []) as GraphEdge[];
      // Empty DB is normal for fresh installs — show empty state, not fake data
      if (loadedNodes.length === 0) {
        notify('No memories found. Use tylluan_remember to add knowledge.', 'info');
      }
      setResults(loadedNodes);
      setEdges(loadedEdges);
      setFetchState('success');
    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err || 'Failed to load knowledge graph');
      notify('Failed to load knowledge graph', 'error');
      setFetchError(errMsg);
      setFetchState('error');
    } finally {
      setSearching(false);
      setIsBackgroundRefreshing(false);
    }
  }, [bridge, notify]);

  const runClustering = async () => {
    if (!bridge) return;
    setLoading(true);
    try {
      await bridge.maintenance_reindex();
      notify('Reindexing and clustering started in background', 'info');
      setTimeout(() => {
        void loadRecent();
      }, 2000);
    } catch {
      notify('Failed to initiate reindexing', 'error');
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
      setRecentNodes(res as ExtendedGraphNode[]);
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
    } catch {
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
          <div className="w-8 h-8 rounded bg-primary/10 border border-primary/20 flex items-center justify-center">
            <Network className="w-5 h-5 text-primary" />
          </div>
          <div>
            <h2 className="text-sm font-semibold text-slate-50">Cortex Knowledge</h2>
            <p className="text-[11px] text-slate-500">SilvaDB visualizer & search engine</p>
          </div>
        </div>

        <div role="group" aria-label="Knowledge View Modes" className="flex bg-slate-900 border border-border rounded p-1 gap-1 items-center shrink-0">
          <button 
            type="button" 
            aria-pressed={activeSubView === 'graph'}
            onClick={() => setActiveSubView('graph')} 
            className={cn(
              "px-3 py-1.5 rounded text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none",
              activeSubView === 'graph' ? "bg-primary/10 text-primary border border-primary/30" : "text-slate-400 hover:text-slate-200"
            )}
          >
            <Network className="w-3.5 h-3.5" aria-hidden="true" /> Graph Canvas
          </button>
          <button 
            type="button" 
            aria-pressed={activeSubView === 'list'}
            onClick={() => setActiveSubView('list')} 
            className={cn(
              "px-3 py-1.5 rounded text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none",
              activeSubView === 'list' ? "bg-primary/10 text-primary border border-primary/30" : "text-slate-400 hover:text-slate-200"
            )}
          >
            <List className="w-3.5 h-3.5" aria-hidden="true" /> List Explorer
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
                  <div className="flex-1 min-h-0 rounded bg-slate-900/60 border border-border flex items-center justify-center gap-2 text-xs text-slate-400">
                    <RefreshCw className="w-4 h-4 animate-spin" /> Loading graph canvas...
                  </div>
                }>
                  <LightGraphPlaceholder bridge={bridge} events={events} />
                </Suspense>
              </div>
              <IngestPanel bridge={bridge} notify={notify} onIngestComplete={handleIngestComplete} />
            </>
          ) : (
            <div className="h-full rounded bg-slate-900/60 border border-border flex items-center justify-center">
              <div className="flex items-center gap-2 text-xs text-slate-400">
                <RefreshCw className="w-4 h-4 animate-spin" />
                Waiting for SilvaDB connection
              </div>
            </div>
          )}
        </div>
      ) : (
        <div className="flex-1 min-h-0 flex flex-col space-y-3 font-sans">
          <div className="flex items-center justify-between gap-3 bg-[#ECE9D8] dark:bg-card border border-[#ACA899] dark:border-border p-2 rounded-sm">
            <div className="flex gap-2 flex-1 max-w-3xl items-center">
              {results.length > 0 && (
                <span className="text-[11px] font-bold text-[#0A246A] dark:text-primary whitespace-nowrap">{results.length} recuerdos</span>
              )}
              <div className="flex-1 relative">
                <SearchIcon className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-[#555555]" aria-hidden="true" />
                <input
                  type="text"
                  aria-label="Buscar nodos en memoria SilvaDB"
                  value={searchQuery}
                  onChange={(e) => handleSearchChange(e.target.value)}
                  onFocus={() => searchResults.length > 0 && setShowSearchPanel(true)}
                  placeholder="Buscar en memoria SilvaDB..."
                  className="w-full pl-8 pr-8 py-1 bg-white dark:bg-muted border border-[#7F9DB9] dark:border-border rounded-none text-[11px] text-foreground focus:outline-none"
                />
                {searchQuery && (
                  <button
                    type="button"
                    aria-label="Borrar búsqueda"
                    onClick={() => { setSearchQuery(''); setSearchResults([]); }}
                    className="absolute right-2 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600"
                  >
                    <X className="w-3.5 h-3.5" aria-hidden="true" />
                  </button>
                )}
              </div>
              <button
                type="button"
                aria-label="Alternar nodos recientes (últimas 24h)"
                aria-pressed={showRecentSidebar}
                onClick={() => setShowRecentSidebar(!showRecentSidebar)}
                className={cn(
                  "p-1.5 rounded-sm border transition-all cursor-pointer",
                  showRecentSidebar ? "bg-[#0A246A] text-white border-[#0A246A]" : "bg-white dark:bg-muted text-foreground border-[#ACA899] hover:bg-[#F5F4EA]"
                )}
                title="Últimas 24 h"
              >
                <Clock className="w-3.5 h-3.5" aria-hidden="true" />
              </button>
              <button
                type="button"
                onClick={runClustering}
                disabled={loading}
                className="flex items-center gap-1.5 px-2.5 py-1 rounded-sm bg-white dark:bg-muted hover:bg-[#F5F4EA] text-[11px] font-bold text-[#0A246A] dark:text-primary border border-[#ACA899] transition-all cursor-pointer"
              >
                <Zap className="w-3.5 h-3.5" aria-hidden="true" /> Detectar comunidades
              </button>
              <button
                type="button"
                onClick={() => void loadRecent(false)}
                disabled={loading || isBackgroundRefreshing}
                className="flex items-center gap-1.5 px-2.5 py-1 rounded-sm bg-white dark:bg-muted hover:bg-[#F5F4EA] text-[11px] font-bold text-foreground border border-[#ACA899] transition-colors cursor-pointer"
              >
                <RefreshCw className={cn("w-3 h-3 text-[#0A246A]", (loading || isBackgroundRefreshing) && "animate-spin")} aria-hidden="true" /> Actualizar
              </button>
              <button
                type="button"
                onClick={handleSearch}
                disabled={searching}
                className="px-3 py-1 bg-[#0A246A] hover:bg-[#081C52] text-white text-[11px] font-bold border border-[#0A246A] rounded-sm flex items-center gap-1.5 cursor-pointer"
              >
                {searching ? <RefreshCw className="w-3 h-3 animate-spin" aria-hidden="true" /> : <Database className="w-3 h-3" aria-hidden="true" />} Explorar
              </button>
            </div>
            <div className="flex bg-white dark:bg-muted border border-[#ACA899] rounded-sm p-0.5 shrink-0 gap-0.5 items-center">
              <button type="button" aria-pressed={view === 'grid'} onClick={() => setView('grid')} className={cn("px-2 py-0.5 rounded-sm text-[10px] font-bold cursor-pointer transition-colors", view === 'grid' ? "bg-[#0A246A] text-white" : "text-[#444444] dark:text-muted-foreground hover:bg-[#F5F4EA]")}>Cuadrícula</button>
              <button type="button" aria-pressed={view === 'table'} onClick={() => setView('table')} className={cn("px-2 py-0.5 rounded-sm text-[10px] font-bold cursor-pointer transition-colors", view === 'table' ? "bg-[#0A246A] text-white" : "text-[#444444] dark:text-muted-foreground hover:bg-[#F5F4EA]")}>Tabla</button>
              <div className="w-px bg-[#ACA899] self-stretch my-0.5 mx-0.5"></div>
              <button 
                type="button" 
                aria-pressed={compactMode}
                onClick={() => setCompactMode(!compactMode)} 
                className={cn(
                  "px-2 py-0.5 rounded-sm text-[10px] font-bold cursor-pointer transition-colors",
                  compactMode ? "bg-[#ECE9D8] text-[#0A246A]" : "text-[#444444] hover:bg-[#F5F4EA]"
                )}
              >
                {compactMode ? "Compacto" : "Completo"}
              </button>
            </div>
          </div>

          {showSearchPanel && searchResults.length > 0 && (
            <div className="absolute z-20 mt-12 w-80 max-h-96 overflow-y-auto bg-slate-900 border border-border rounded shadow-none">
              <div className="sticky top-0 bg-slate-800 px-3 py-2 flex items-center justify-between">
                <span className="text-[11px] font-medium text-slate-400">Results</span>
                <button type="button" aria-label="Close search results" onClick={() => setShowSearchPanel(false)}><X className="w-3 h-3 text-slate-400" aria-hidden="true" /></button>
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
                    <span className="text-[11px] font-medium text-violet-400">{node.node_type || 'node'}</span>
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
              "border-2 border-dashed rounded px-6 py-4 flex items-center gap-4 transition-all cursor-default",
              isDragging ? "border-primary bg-primary/10 scale-[1.01]" : "border-slate-800 hover:border-slate-600"
            )}
          >
            {uploading
              ? <RefreshCw className="w-5 h-5 text-primary animate-spin flex-shrink-0" aria-hidden="true" />
              : <Save className="w-5 h-5 text-slate-400 flex-shrink-0" aria-hidden="true" />}
            <div>
              <p className="text-xs font-bold text-slate-300">{isDragging ? 'Drop to ingest' : 'Drag files here to ingest into SilvaDB'}</p>
              <p className="text-[11px] text-slate-400 mt-0.5">.md .txt .py .js .ts .rs .json .yaml .toml .pdf · .png .jpg .jpeg .webp</p>
            </div>
          </div>

          {/* IVF Index Status Widget */}
          <div role="status" aria-live="polite" className="flex items-center gap-3 px-4 py-2.5 rounded border border-border bg-slate-900/80">
            <Layers className="w-4 h-4 text-slate-400 flex-shrink-0" aria-hidden="true" />
            <span className="text-[11px] font-medium text-slate-400">IVF Index</span>
            <div className="flex items-center gap-1.5 ml-1">
              {memoryStats?.ivf_ready ? (
                <>
                  <CheckCircle className="w-3.5 h-3.5 text-emerald-400" aria-hidden="true" />
                  <span className="text-[11px] font-semibold text-emerald-400">READY</span>
                </>
              ) : (
                <>
                  <AlertCircle className="w-3.5 h-3.5 text-amber-400" aria-hidden="true" />
                  <span className="text-[11px] font-semibold text-amber-400">BUILDING…</span>
                </>
              )}
            </div>
            <div className="h-3 w-px bg-slate-700 mx-1" />
            <div className="flex items-center gap-1">
              <span className="text-[11px] text-slate-400">centroids:</span>
              <span className={cn(
                "text-[11px] font-mono font-bold",
                (memoryStats?.n_centroids ?? 0) > 0 ? "text-violet-400" : "text-slate-400"
              )}>
                {memoryStats?.n_centroids ?? '—'}
              </span>
            </div>
            <div className="h-3 w-px bg-slate-700 mx-1" />
            <div className="flex items-center gap-1">
              <span className="text-[11px] text-slate-400">last build:</span>
              <span className="text-[11px] font-mono text-slate-300">
                {memoryStats?.last_build != null
                  ? `rowid ${memoryStats.last_build}`
                  : '—'}
              </span>
            </div>
            {!memoryStats?.ivf_ready && (memoryStats?.node_count ?? 0) < 50 && (
              <span className="ml-auto text-[11px] text-slate-400 italic">&lt;50 embeddings — linear scan active</span>
            )}
          </div>

          {/* Lifecycle Filter & Summary Bar */}
          <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 rounded bg-slate-900/60 border border-slate-800/80">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs font-semibold text-muted-foreground">Lifecycle:</span>
              <div role="group" aria-label="Lifecycle Filter" className="flex items-center gap-1 bg-slate-950/60 p-0.5 rounded border border-slate-800">
                {(['all', 'active', 'quiet', 'consolidated', 'archived'] as const).map((filterKey) => {
                  const count = lifecycleCounts[filterKey];
                  const isActive = lifecycleFilter === filterKey;
                  return (
                    <button
                      key={filterKey}
                      type="button"
                      aria-pressed={isActive}
                      onClick={() => setLifecycleFilter(filterKey)}
                      className={cn(
                        "px-2 py-0.5 rounded text-[11px] font-medium font-mono uppercase transition-all flex items-center gap-1 cursor-pointer focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none",
                        isActive 
                          ? filterKey === 'active' ? "bg-emerald-500/20 text-emerald-300 border border-emerald-500/40"
                          : filterKey === 'quiet' ? "bg-amber-500/20 text-amber-300 border border-amber-500/40"
                          : filterKey === 'consolidated' ? "bg-indigo-500/20 text-indigo-300 border border-indigo-500/40"
                          : filterKey === 'archived' ? "bg-slate-700/60 text-slate-300 border border-slate-600"
                          : "bg-slate-800 text-slate-200 border border-slate-700"
                          : "text-slate-400 hover:text-slate-200 hover:bg-slate-900 border border-transparent"
                      )}
                    >
                      <span>{filterKey}</span>
                      <span className={cn(
                        "px-1 text-[11px] rounded",
                        isActive ? "bg-slate-950/80 text-white font-bold" : "text-slate-400"
                      )}>{count}</span>
                    </button>
                  );
                })}
              </div>
            </div>
            {lifecycleFilter !== 'all' && (
              <span className="text-[11px] text-slate-400 italic">
                Showing {filteredResults.length} of {results.length} nodes
              </span>
            )}
          </div>

          <div className="flex-1 min-h-0 overflow-y-auto">
            {fetchState === 'loading' && results.length === 0 ? (
              <div className="h-64 flex flex-col items-center justify-center gap-3 text-slate-400">
                <RefreshCw className="w-6 h-6 animate-spin text-primary" />
                <span className="text-xs">Loading SilvaDB memories...</span>
              </div>
            ) : fetchState === 'error' && results.length === 0 ? (
              <div className="rounded border border-dashed border-rose-900/60 bg-rose-950/20 p-8 text-center flex flex-col items-center justify-center my-6">
                <div className="w-10 h-10 rounded bg-rose-900/40 border border-rose-800 flex items-center justify-center mb-3">
                  <AlertCircle className="w-5 h-5 text-rose-400" />
                </div>
                <h3 className="text-xs font-semibold text-rose-300">
                  Failed to load SilvaDB memories
                </h3>
                <p className="text-[11px] text-slate-400 max-w-sm mt-1 leading-relaxed">
                  {fetchError || 'Unable to connect to the SilvaDB knowledge graph endpoint.'}
                </p>
                <button
                  type="button"
                  onClick={() => void loadRecent(false)}
                  className="mt-4 px-3 py-1.5 rounded bg-rose-500/20 text-rose-300 text-xs font-medium hover:bg-rose-500/30 transition-all border border-rose-500/40 flex items-center gap-1.5 focus-visible:ring-1 focus-visible:ring-rose-400 focus-visible:outline-none cursor-pointer"
                >
                  <RefreshCw className="w-3.5 h-3.5" /> Retry
                </button>
              </div>
            ) : filteredResults.length === 0 ? (
              <div className="rounded border border-dashed border-slate-800 bg-slate-900/30 p-8 text-center flex flex-col items-center justify-center my-6">
                <div className="w-10 h-10 rounded bg-slate-800/80 border border-slate-700 flex items-center justify-center mb-3">
                  <Database className="w-5 h-5 text-slate-400" />
                </div>
                <h3 className="text-xs font-semibold text-slate-300">
                  {results.length === 0 ? "No memories found in SilvaDB" : `No nodes match filter "${lifecycleFilter}"`}
                </h3>
                <p className="text-[11px] text-slate-500 max-w-sm mt-1 leading-relaxed">
                  {results.length === 0 
                    ? "Use the sovereign tool tylluan_remember or the ingestion panel to add knowledge to the graph."
                    : `Try selecting a different lifecycle filter or "all" to view the ${results.length} available nodes.`}
                </p>
                {results.length === 0 ? (
                  <button
                    type="button"
                    onClick={() => void loadRecent(false)}
                    className="mt-4 px-3 py-1.5 rounded bg-primary/10 text-primary text-xs font-medium hover:bg-primary/20 transition-all border border-primary/30 flex items-center gap-1.5 focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none cursor-pointer"
                  >
                    <RefreshCw className="w-3.5 h-3.5" /> Retry
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => setLifecycleFilter('all')}
                    className="mt-4 px-3 py-1.5 rounded bg-slate-800 text-slate-200 text-xs font-medium hover:bg-slate-700 transition-all border border-slate-700 focus-visible:ring-1 focus-visible:ring-slate-400 focus-visible:outline-none cursor-pointer"
                  >
                    Show all ({results.length})
                  </button>
                )}
              </div>
            ) : view === 'grid' ? (
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                {filteredResults.map((node, i) => {
                  const nodeType = node.node_type || node.type || 'entity';
                  const nodeContent = fixDoubleEncoding(node.content || node.label || '—');
                  return (
                    <div key={i} className={cn("group p-4 rounded border border-border transition-all relative overflow-hidden", 
                      (node.provenance === 'federation_peer' || node.provenance === 'unverified') ? "bg-slate-900/50 hover:bg-slate-800/50" :
                      nodeType === 'consolidated_summary' ? "bg-indigo-950/10 hover:bg-indigo-900/20" : "bg-slate-900/50 hover:bg-slate-800/50"
                    )}>
                      <div className="flex items-center gap-2 mb-3">
                        <div aria-hidden="true" className={cn("w-2 h-2 rounded-full flex-shrink-0",
                          nodeType === 'consolidated_summary' ? "bg-indigo-400" :
                          nodeType === 'lesson' ? "bg-violet-500" :
                          nodeType === 'identity' ? "bg-emerald-500" :
                          nodeType === 'concept' ? "bg-blue-500" : "bg-amber-500"
                        )}></div>
                        <span className="text-[11px] font-medium text-slate-500">{nodeType}</span>
                        <LifecycleBadge state={node.lifecycle_state || 'active'} />
                        {node.content?.startsWith('[DEPRECATED by') && (
                          <span className="px-1.5 py-0.5 rounded bg-red-500/10 text-[11px] font-medium text-red-400 border border-red-500/20 animate-pulse">DEPRECATED</span>
                        )}
                        <span className="text-[11px] font-mono text-slate-600 ml-auto">{node.id.split(':').pop()?.slice(0, 8)}</span>
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
                                  className="text-amber-400 hover:text-amber-300 mt-1 font-semibold text-[11px] underline cursor-pointer"
                                >
                                  [View full: {node.id.split(':').pop()?.slice(0, 8)}]
                                </button>
                              </>
                            );
                          } else if (compactMode && isExpanded) {
                            return (
                              <div className="flex flex-col gap-2">
                                <span className="whitespace-pre-wrap">{nodeContent}</span>
                                {nodeType === 'consolidated_summary' && (
                                  <div className="mt-2 p-2 rounded bg-indigo-500/5 border border-indigo-500/20">
                                    <span className="text-[11px] font-bold text-indigo-400 uppercase block mb-1">Original Sources:</span>
                                    <ul className="space-y-1">
                                      {edges.filter(e => e.target === node.id && (e.type === 'consolidated_into' || e.edge_type === 'consolidated_into')).length > 0 ? (
                                        edges.filter(e => e.target === node.id && (e.type === 'consolidated_into' || e.edge_type === 'consolidated_into')).map((edge, j) => (
                                          <li key={j} className="text-[11px] text-slate-400 font-mono flex items-center gap-1.5 cursor-pointer hover:text-indigo-300" onClick={() => setQuery(edge.source)}>
                                            <Network className="w-3 h-3 text-indigo-500" />
                                            {edge.source}
                                          </li>
                                        ))
                                      ) : (
                                        <li className="text-[11px] text-slate-500 italic">No loaded sources found</li>
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
                                   className="text-slate-500 hover:text-slate-300 mt-1 font-medium text-[11px] underline cursor-pointer self-start"
                                >
                                  [Collapse]
                                </button>
                              </div>
                            );
                          } else {
                            return (
                              <div className="flex flex-col gap-2">
                                <span className={cn(compactMode && "block")}>{nodeContent}</span>
                                {nodeType === 'consolidated_summary' && (
                                  <div className="mt-2 p-2 rounded bg-indigo-500/5 border border-indigo-500/20">
                                    <span className="text-[11px] font-bold text-indigo-400 uppercase block mb-1">Original Sources:</span>
                                    <ul className="space-y-1">
                                      {edges.filter(e => e.target === node.id && (e.type === 'consolidated_into' || e.edge_type === 'consolidated_into')).length > 0 ? (
                                        edges.filter(e => e.target === node.id && (e.type === 'consolidated_into' || e.edge_type === 'consolidated_into')).map((edge, j) => (
                                          <li key={j} className="text-[11px] text-slate-400 font-mono flex items-center gap-1.5 cursor-pointer hover:text-indigo-300" onClick={() => setQuery(edge.source)}>
                                            <Network className="w-3 h-3 text-indigo-500" />
                                            {edge.source}
                                          </li>
                                        ))
                                      ) : (
                                        <li className="text-[11px] text-slate-500 italic">No loaded sources found</li>
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
                                    className="text-amber-400 hover:text-amber-300 mt-1 font-medium text-[11px] underline cursor-pointer"
                                  >
                                    [Show less]
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
                            <span className="text-[11px] text-slate-600 uppercase">Weight</span>
                            <span className="text-[11px] font-mono font-semibold text-amber-400">{(node.weight || 0).toFixed(2)}</span>
                          </div>
                          {node.provenance && (
                            <div className="flex flex-col border-l border-slate-700/50 pl-3">
                              <span className="text-[11px] text-slate-600 uppercase">Provenance</span>
                               <span className={cn("text-[11px] font-medium flex items-center gap-1", 
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
            ) : (
              <div className="rounded border border-border bg-slate-900/50 overflow-hidden overflow-x-auto">
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
                      const nodeType = node.node_type || node.type || 'entity';
                      const nodeContent = fixDoubleEncoding(node.content || node.label || '—');
                      return (
                        <tr key={i} className="hover:bg-slate-800/30 transition-colors cursor-pointer" onClick={() => setQuery(node.id)}>
                          <td className="px-4 py-3 text-[11px] font-mono text-violet-400 max-w-[120px] truncate">{node.id}</td>
                          <td className="px-4 py-3">
                            <span className={cn("px-1.5 py-0.5 rounded text-[11px] font-medium",
                              nodeType === 'consolidated_summary' ? "bg-indigo-500/20 text-indigo-400" : "bg-slate-800 text-slate-400"
                            )}>
                              {nodeType === 'consolidated_summary' ? 'SYNTHESIS' : nodeType}
                            </span>
                            <LifecycleBadge state={node.lifecycle_state || 'active'} className="ml-1.5" />
                            {node.content?.startsWith('[DEPRECATED by') && (
                              <span className="ml-1.5 px-1.5 py-0.5 rounded bg-red-500/10 text-[11px] font-medium text-red-400 border border-red-500/20">DEPRECATED</span>
                            )}
                            {(node.provenance === 'federation_peer' || node.provenance === 'unverified') && (
                              <span className="ml-1.5 px-1.5 py-0.5 rounded bg-amber-500/10 text-[11px] font-medium text-amber-500 border border-amber-500/20 inline-flex items-center gap-1">
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
                                          className="text-amber-400 hover:text-amber-300 font-medium text-[11px] underline whitespace-nowrap cursor-pointer"
                                      >
                                        [View full]
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
                                          className="text-amber-400 hover:text-amber-300 ml-1.5 font-semibold text-[11px] underline whitespace-nowrap cursor-pointer"
                                        >
                                          [Show less]
                                        </button>
                                      )}
                                    </div>
                                  );
                              }
                            })()}
                          </td>
                          <td className="px-4 py-3 text-right text-[11px] font-mono font-medium text-amber-400">{(node.weight || 0).toFixed(2)}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}

            {/* Recent Nodes Sidebar */}
            {showRecentSidebar && (
              <div className="absolute left-4 top-32 w-72 max-h-[60vh] bg-slate-900 border border-slate-700 rounded z-10 overflow-hidden flex flex-col">
                <div className="px-4 py-3 bg-slate-800/50 border-b border-slate-700 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Clock className="w-4 h-4 text-primary" />
                    <span className="text-[11px] font-medium text-slate-300">Past 24h</span>
                  </div>
                  <button onClick={() => setShowRecentSidebar(false)} className="text-slate-500 hover:text-slate-50">
                    <X className="w-4 h-4" />
                  </button>
                </div>
                <div className="flex-1 overflow-y-auto p-2">
                  {!recentLoaded ? (
                    <p className="text-xs text-slate-500 text-center py-4">Loading...</p>
                  ) : recentNodes.length === 0 ? (
                    <p className="text-xs text-slate-500 text-center py-4">No recent data</p>
                  ) : (
                    recentNodes.map((node, i) => {
                      const timeAgo = node.created_at 
                        ? (() => {
                            const diff = Date.now() - new Date(node.created_at).getTime();
                            const mins = Math.floor(diff / 60000);
                            if (mins < 60) return `${mins}m ago`;
                            const hours = Math.floor(mins / 60);
                            if (hours < 24) return `${hours}h ago`;
                            return `${Math.floor(hours / 24)}d ago`;
                          })()
                        : '';
                      return (
                        <div
                          key={i}
                          className="px-3 py-2 mb-1 rounded hover:bg-slate-800/50 cursor-pointer transition-colors"
                          onClick={() => {
                            setQuery(node.content || node.id);
                            handleSearch();
                            setShowRecentSidebar(false);
                          }}
                        >
                          <div className="flex items-center gap-2 mb-1">
                            <span className={cn(
                              "text-[11px] font-medium px-1 py-0.5 rounded",
                              node.node_type === 'agent_memory' ? "bg-emerald-500/20 text-emerald-400" :
                              node.node_type === 'concept' ? "bg-blue-500/20 text-blue-400" :
                              node.node_type === 'fact' ? "bg-orange-500/20 text-orange-400" :
                              "bg-slate-700 text-slate-400"
                            )}>{node.node_type?.slice(0, 8) || 'node'}</span>
                            <LifecycleBadge state={node.lifecycle_state || 'active'} className="ml-1" />
                            {node.content?.startsWith('[DEPRECATED by') && (
                              <span className="px-1 py-0.5 rounded bg-red-500/10 text-[11px] font-medium text-red-400 border border-red-500/20">DEPRECATED</span>
                            )}
                            <span className="text-[11px] text-slate-600 ml-auto">{timeAgo}</span>
                          </div>
                          <p className="text-xs text-slate-300 line-clamp-2">{node.content?.slice(0, 80) || node.id}</p>
                        </div>
                      );
                    })
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
