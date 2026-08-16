import { useState, useEffect, useCallback, useRef } from 'react';
import { Search, Database, Upload, RefreshCw, Clock, FileText, Trash2 } from 'lucide-react';
import { cn } from '../lib/utils';
import type { NexusBridge, GraphNode } from '../lib/api-client';
import type { MemoryStats } from '../hooks/useNexus';

interface Props {
  bridge: NexusBridge | null;
  memoryStats: MemoryStats | null;
  notify?: (msg: string, type?: 'info' | 'error') => void;
}

export function MemoryOverview({ bridge, memoryStats, notify }: Props) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<GraphNode[]>([]);
  const [recentNodes, setRecentNodes] = useState<GraphNode[]>([]);
  const [searching, setSearching] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [uploading, setUploading] = useState(false);
  const searchTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadRecent = useCallback(async () => {
    if (!bridge) return;
    try {
      const nodes = await bridge.getRecentNodes(20);
      setRecentNodes(nodes);
    } catch { /* non-critical */ }
  }, [bridge]);

  useEffect(() => { loadRecent(); }, [loadRecent]);

  const handleSearch = useCallback((value: string) => {
    setQuery(value);
    if (searchTimeout.current) clearTimeout(searchTimeout.current);
    if (!value.trim()) {
      setResults([]);
      return;
    }
    searchTimeout.current = setTimeout(async () => {
      if (!bridge) return;
      setSearching(true);
      try {
        const res = await bridge.recall(value, 10);
        setResults(res);
      } catch {
        notify?.('Search failed', 'error');
      }
      setSearching(false);
    }, 400);
  }, [bridge, notify]);

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
    notify?.(`Ingested ${ok} file${ok !== 1 ? 's' : ''}${fail ? `, ${fail} failed` : ''}`, fail ? 'error' : 'info');
    if (ok > 0) loadRecent();
  };

  const displayNodes = results.length > 0 ? results : recentNodes;

  return (
    <div className="space-y-5 animate-in fade-in duration-500">
      {/* Stats row */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <StatCard label="Nodos" value={memoryStats?.node_count ?? '—'} icon={Database} color="text-violet-400" />
        <StatCard label="Aristas" value={memoryStats?.edge_count ?? '—'} icon={Database} color="text-cyan-400" />
        <StatCard label="Documentos" value={memoryStats?.document_count ?? '—'} icon={FileText} color="text-amber-400" />
        <StatCard label="Disco" value={memoryStats?.disk_usage_bytes ? `${(memoryStats.disk_usage_bytes / 1024 / 1024).toFixed(1)}MB` : '—'} icon={Database} color="text-emerald-400" />
      </div>

      {/* Search */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-white/30" />
        <input
          type="text"
          value={query}
          onChange={e => handleSearch(e.target.value)}
          placeholder="Buscar en memoria..."
          className="w-full pl-10 pr-4 py-2.5 bg-white/5 border border-white/10 rounded-xl text-sm text-white placeholder:text-white/30 focus:outline-none focus:border-violet-500/50 transition-colors"
        />
        {searching && <RefreshCw className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-white/30 animate-spin" />}
      </div>

      {/* Ingest dropzone */}
      <div
        onDragOver={e => { e.preventDefault(); setIsDragging(true); }}
        onDragLeave={() => setIsDragging(false)}
        onDrop={handleDrop}
        className={cn(
          'border-2 border-dashed rounded-xl p-6 text-center transition-all',
          isDragging ? 'border-violet-500 bg-violet-500/10 scale-[1.02]' : 'border-white/10 hover:border-white/20'
        )}
      >
        {uploading ? (
          <RefreshCw className="w-6 h-6 mx-auto animate-spin text-violet-400" />
        ) : (
          <>
            <Upload className="w-8 h-8 mx-auto mb-2 text-white/20" />
            <p className="text-sm text-white/40">Arrastra archivos para agregar a la memoria</p>
            <p className="text-[10px] text-white/20 mt-1">.md .txt .py .js .json .yaml</p>
          </>
        )}
      </div>

      {/* Nodes list */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <h3 className="text-xs font-semibold text-white/50 uppercase tracking-wider">
            {results.length > 0 ? `${results.length} resultados` : 'Nodos recientes'}
          </h3>
          {results.length > 0 && (
            <button type="button" onClick={() => { setResults([]); setQuery(''); }} className="text-[10px] text-white/30 hover:text-white/60">
              Limpiar
            </button>
          )}
        </div>
        {displayNodes.length === 0 ? (
          <div className="text-center py-8 text-white/20 text-sm">
            {query ? 'Sin resultados' : 'Sin nodos en memoria'}
          </div>
        ) : (
          <div className="space-y-1.5">
            {displayNodes.map((node: any) => (
              <div key={node.id || node.title} className="flex items-start gap-3 p-3 rounded-lg bg-white/[0.02] border border-white/5 hover:border-white/10 transition-colors">
                <div className="w-2 h-2 rounded-full bg-violet-400 mt-1.5 shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm text-white/80 truncate">{node.title || node.id || 'Untitled'}</p>
                  {node.content && <p className="text-[11px] text-white/30 mt-0.5 line-clamp-2">{node.content}</p>}
                  <div className="flex items-center gap-3 mt-1">
                    {node.source && <span className="text-[9px] text-white/20">{node.source}</span>}
                    {node.timestamp && <span className="text-[9px] text-white/20 flex items-center gap-1"><Clock className="w-2.5 h-2.5" />{new Date(node.timestamp * 1000).toLocaleDateString()}</span>}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function StatCard({ label, value, icon: Icon, color }: { label: string; value: React.ReactNode; icon: React.ElementType; color: string }) {
  return (
    <div className="glass-card p-3 flex items-center gap-3">
      <Icon className={`w-5 h-5 ${color} shrink-0`} />
      <div>
        <p className="text-[10px] text-white/40 uppercase tracking-wider">{label}</p>
        <p className={`text-lg font-bold ${color}`}>{value}</p>
      </div>
    </div>
  );
}
