import { useState, useEffect, useCallback } from 'react';
import { Upload, Layers, ChevronDown, ChevronUp, Clock, Tag, AlertTriangle, CheckCircle2 } from 'lucide-react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onIngestComplete?: () => void;
}

interface RecentNode {
  node_id: string;
  preview: string;
  triples: number;
  ts: number;
}

const NODE_TYPES = ['fact', 'concept', 'event', 'person', 'place', 'tool', 'note'] as const;

export function IngestPanel({ bridge, notify, onIngestComplete }: Props) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState('');
  const [nodeType, setNodeType] = useState<string>('note');
  const [tags, setTags] = useState('');
  const [importance, setImportance] = useState(0.5);
  const [bulk, setBulk] = useState(false);
  const [loading, setLoading] = useState(false);
  const [recent, setRecent] = useState<RecentNode[]>([]);

  // Multi-mode states (R12-3)
  const [mode, setMode] = useState<'text' | 'url' | 'file'>('text');
  const [urlText, setUrlText] = useState('');
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);

  const charCount = text.trim().length;
  const bulkChunks = bulk ? text.split(/\n---+\n/).map(s => s.trim()).filter(Boolean) : null;
  
  const canSubmit =
    !loading &&
    !!bridge &&
    (
      (mode === 'text' && charCount > 0) ||
      (mode === 'url' && urlText.trim().length > 0) ||
      (mode === 'file' && !!selectedFile)
    );

  const fetchRecentNodes = useCallback(async () => {
    if (!bridge) return;
    try {
      const nodesList = await bridge.getRecentNodes(15);
      // Filter for nodes that are likely from the ingest/code pipeline
      const filtered = (nodesList || []).filter((n: any) => 
        n.node_type === 'episode' || 
        n.node_type === 'document' || 
        n.node_type === 'code_entity' ||
        n.node_type === 'note'
      );

      const mapped: RecentNode[] = filtered.map((n: any) => {
        let meta: any = {};
        try {
          meta = typeof n.metadata === 'string' ? JSON.parse(n.metadata) : (n.metadata || {});
        } catch {}

        let preview = n.content || '';
        if (n.node_type === 'code_entity') {
          preview = `[Code] ${meta.kind || ''} ${meta.name || n.id}`;
        } else if (meta.source_file) {
          preview = `${meta.source_file} (Chunk ${meta.chunk_index + 1}/${meta.chunk_total}): ${n.content}`;
        } else if (meta.url) {
          preview = `[URL] ${meta.url}: ${n.content}`;
        }

        if (preview.length > 80) {
          preview = preview.slice(0, 80) + '…';
        }

        return {
          node_id: n.id,
          preview,
          triples: meta.triples_extracted || 0,
          ts: n.updated_at ? new Date(n.updated_at).getTime() : Date.now(),
        };
      });

      setRecent(mapped.slice(0, 8));
    } catch (e) {
      console.error('Failed to fetch recent nodes:', e);
    }
  }, [bridge]);

  useEffect(() => {
    if (!open) return;
    fetchRecentNodes();
    const interval = setInterval(fetchRecentNodes, 10000);
    return () => clearInterval(interval);
  }, [open, fetchRecentNodes]);

  const handleSubmit = useCallback(async () => {
    if (!bridge || !canSubmit) return;
    setLoading(true);
    try {
      if (mode === 'text') {
        const chunks = bulkChunks ?? [text.trim()];
        let succeeded = 0;
        let totalTriples = 0;
        for (const chunk of chunks) {
          const res = await bridge.fetchRaw('/api/v1/ingest', {
            method: 'POST',
            body: JSON.stringify({
              text: chunk,
              source: 'panel',
              tags: tags.split(',').map(t => t.trim()).filter(Boolean)
            })
          });
          totalTriples += res.triples_extracted ?? 0;
          succeeded++;
        }
        notify(
          chunks.length > 1
            ? `Ingested ${succeeded}/${chunks.length} chunks — ${totalTriples} triples extracted`
            : `Node ingested — ${totalTriples} triples extracted`,
          'info'
        );
        setText('');
        onIngestComplete?.();
        fetchRecentNodes();
      } else if (mode === 'url') {
        const trimmedUrl = urlText.trim();
        const res = await bridge.ingestUrl(trimmedUrl, tags);
        
        let parsedResult: any = null;
        try {
          const responseText = res.response || '';
          const match = responseText.match(/text:\s*"(.*)"\s*}/s);
          if (match) {
            const cleanJson = match[1].replace(/\\"/g, '"').replace(/\\\\/g, '\\');
            parsedResult = JSON.parse(cleanJson);
          } else {
            parsedResult = JSON.parse(responseText);
          }
        } catch {}

        const chunks = parsedResult?.chunks ?? 0;
        const nodesCreated = parsedResult?.nodes_created ?? 0;

        notify(`URL ingested successfully via tylluan_do — ${nodesCreated} nodes created (${chunks} chunks)`, 'info');
        setUrlText('');
        onIngestComplete?.();
        fetchRecentNodes();
      } else if (mode === 'file') {
        if (!selectedFile) return;
        const res = await bridge.uploadFile(selectedFile);
        notify(`File ${selectedFile.name} uploaded successfully via ${res.pipeline || 'ingest'} pipeline`, 'info');
        setSelectedFile(null);
        onIngestComplete?.();
        fetchRecentNodes();
      }
    } catch (e: any) {
      notify(`Ingest failed: ${e?.message ?? 'unknown error'}`, 'error');
    } finally {
      setLoading(false);
    }
  }, [bridge, canSubmit, mode, text, bulkChunks, tags, urlText, selectedFile, notify, onIngestComplete, fetchRecentNodes]);

  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900/40 overflow-hidden">
      {/* Header — toggle */}
      <button
        onClick={() => setOpen(v => !v)}
        className="w-full flex items-center justify-between px-4 py-3 text-xs font-bold uppercase tracking-wider text-slate-400 hover:text-slate-200 hover:bg-slate-800/40 transition-colors"
      >
        <div className="flex items-center gap-2">
          <Upload className="w-3.5 h-3.5 text-emerald-500" />
          <span>Ingest Knowledge</span>
          {recent.length > 0 && (
            <span className="px-1.5 py-0.5 rounded-full bg-emerald-500/15 text-emerald-400 text-[10px] font-bold border border-emerald-500/25">
              {recent.length}
            </span>
          )}
        </div>
        {open ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
      </button>

      {open && (
        <div className="px-4 pb-4 space-y-3 border-t border-slate-800">
          {/* Mode Selector Tabs (R12-3) */}
          <div className="flex items-center gap-1.5 border-b border-slate-800/60 pb-1.5 mt-3">
            <button
              onClick={() => setMode('text')}
              className={cn(
                "px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider transition-colors",
                mode === 'text'
                  ? "bg-slate-800 text-emerald-400 border border-slate-700"
                  : "text-slate-500 hover:text-slate-300"
              )}
            >
              Text
            </button>
            <button
              onClick={() => setMode('url')}
              className={cn(
                "px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider transition-colors",
                mode === 'url'
                  ? "bg-slate-800 text-emerald-400 border border-slate-700"
                  : "text-slate-500 hover:text-slate-300"
              )}
            >
              URL
            </button>
            <button
              onClick={() => setMode('file')}
              className={cn(
                "px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider transition-colors",
                mode === 'file'
                  ? "bg-slate-800 text-emerald-400 border border-slate-700"
                  : "text-slate-500 hover:text-slate-300"
              )}
            >
              File Drag & Drop
            </button>
          </div>

          {/* Mode contents */}
          {mode === 'text' && (
            <>
              {/* Textarea */}
              <textarea
                value={text}
                onChange={e => setText(e.target.value)}
                placeholder={bulk
                  ? 'Paste multiple chunks separated by lines of ---\nChunk 1 text\n---\nChunk 2 text'
                  : 'Paste text, facts, or knowledge to store in SilvaDB…'}
                rows={6}
                className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2.5 text-xs text-slate-200 placeholder-slate-600 font-mono resize-y focus:outline-none focus:border-emerald-500/60 transition-colors"
              />

              {/* Controls row */}
              <div className="flex flex-wrap items-center gap-3">
                {/* Node type */}
                <div className="flex items-center gap-1.5">
                  <Tag className="w-3 h-3 text-slate-500" />
                  <select
                    value={nodeType}
                    onChange={e => setNodeType(e.target.value)}
                    className="bg-slate-950 border border-slate-700 rounded-md px-2 py-1 text-[11px] text-slate-300 focus:outline-none focus:border-emerald-500/60"
                  >
                    {NODE_TYPES.map(t => (
                      <option key={t} value={t}>{t}</option>
                    ))}
                  </select>
                </div>

                {/* Tags */}
                <input
                  type="text"
                  value={tags}
                  onChange={e => setTags(e.target.value)}
                  placeholder="tags (comma-sep)"
                  className="flex-1 min-w-[120px] bg-slate-950 border border-slate-700 rounded-md px-2 py-1 text-[11px] text-slate-300 placeholder-slate-600 focus:outline-none focus:border-emerald-500/60"
                />

                {/* Importance slider */}
                <div className="flex items-center gap-1.5 shrink-0">
                  <span className="text-[10px] text-slate-500 uppercase tracking-wider">Imp</span>
                  <input
                    type="range"
                    min={0} max={1} step={0.1}
                    value={importance}
                    onChange={e => setImportance(Number(e.target.value))}
                    className="w-20 accent-emerald-500"
                  />
                  <span className="text-[10px] text-slate-400 w-6 text-right">{importance.toFixed(1)}</span>
                </div>

                {/* Bulk toggle */}
                <button
                  onClick={() => setBulk(v => !v)}
                  className={cn(
                    'flex items-center gap-1 px-2.5 py-1 rounded-md border text-[10px] font-bold uppercase tracking-wider transition-colors',
                    bulk
                      ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-400'
                      : 'bg-slate-950 border-slate-700 text-slate-500 hover:border-slate-600 hover:text-slate-300'
                  )}
                >
                  <Layers className="w-3 h-3" />
                  Bulk
                </button>
              </div>

              {/* Bulk preview */}
              {bulk && bulkChunks && bulkChunks.length > 0 && (
                <div className="text-[10px] text-slate-500 font-mono">
                  {bulkChunks.length} chunk{bulkChunks.length !== 1 ? 's' : ''} detected
                  {bulkChunks.map((c, i) => (
                    <span key={i} className="ml-2 px-1.5 py-0.5 rounded bg-slate-800 text-slate-400">
                      {c.slice(0, 30)}{c.length > 30 ? '…' : ''}
                    </span>
                  ))}
                </div>
              )}
            </>
          )}

          {mode === 'url' && (
            <div className="space-y-3">
              <input
                type="text"
                value={urlText}
                onChange={e => setUrlText(e.target.value)}
                placeholder="https://example.com/article"
                className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2.5 text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-emerald-500/60 transition-colors"
              />
              <p className="text-[10px] text-slate-550">
                Provide a URL to crawl and ingest its content automatically into SilvaDB.
              </p>
            </div>
          )}

          {mode === 'file' && (
            <div className="space-y-3">
              <div
                onDragOver={e => { e.preventDefault(); setIsDragOver(true); }}
                onDragLeave={() => setIsDragOver(false)}
                onDrop={e => {
                  e.preventDefault();
                  setIsDragOver(false);
                  if (e.dataTransfer.files && e.dataTransfer.files[0]) {
                    setSelectedFile(e.dataTransfer.files[0]);
                  }
                }}
                onClick={() => {
                  const input = document.createElement('input');
                  input.type = 'file';
                  input.onchange = ev => {
                    const target = ev.target as HTMLInputElement;
                    if (target.files && target.files[0]) {
                      setSelectedFile(target.files[0]);
                    }
                  };
                  input.click();
                }}
                className={cn(
                  "border-2 border-dashed rounded-lg p-6 text-center cursor-pointer transition-all flex flex-col items-center justify-center gap-2",
                  isDragOver
                    ? "border-emerald-500 bg-emerald-500/10 scale-[1.01]"
                    : "border-slate-800 bg-slate-950 hover:border-slate-700"
                )}
              >
                <Upload className={cn("w-6 h-6 transition-colors", selectedFile ? "text-emerald-400" : "text-slate-500")} />
                {selectedFile ? (
                  <div className="text-xs text-slate-200">
                    <p className="font-bold text-emerald-400">{selectedFile.name}</p>
                    <p className="text-[10px] text-slate-500">{(selectedFile.size / 1024).toFixed(1)} KB</p>
                  </div>
                ) : (
                  <div className="text-xs text-slate-400">
                    <p className="font-bold">Arrastra un archivo aquí</p>
                    <p className="text-[10px] text-slate-600">o haz clic para seleccionar</p>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Submit */}
          <div className="flex items-center justify-between">
            <span className="text-[10px] text-slate-600 font-mono">
              {mode === 'text' && `${charCount} chars`}
              {mode === 'url' && 'URL Mode'}
              {mode === 'file' && (selectedFile ? `${selectedFile.name}` : 'File Mode')}
            </span>
            <button
              onClick={handleSubmit}
              disabled={!canSubmit}
              className={cn(
                'flex items-center gap-2 px-4 py-1.5 rounded-lg text-xs font-bold uppercase tracking-wider transition-all',
                canSubmit
                  ? 'bg-emerald-600 hover:bg-emerald-500 text-white shadow-lg shadow-emerald-500/10'
                  : 'bg-slate-800 text-slate-600 cursor-not-allowed'
              )}
            >
              {loading ? (
                <>
                  <div className="w-3 h-3 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                  Ingesting…
                </>
              ) : (
                <>
                  <Upload className="w-3 h-3" />
                  {mode === 'text' && bulk && bulkChunks && bulkChunks.length > 1 ? `Ingest ${bulkChunks.length} chunks` : 'Ingest'}
                </>
              )}
            </button>
          </div>

          {/* Recent nodes */}
          {recent.length > 0 && (
            <div className="border-t border-slate-800 pt-3 space-y-1.5">
              <p className="text-[10px] uppercase tracking-widest text-slate-600 font-bold">Recent</p>
              {recent.map(n => (
                <div key={n.node_id + n.ts} className="flex items-start gap-2 text-[10px] font-mono">
                  <CheckCircle2 className="w-3 h-3 text-emerald-500 shrink-0 mt-px" />
                  <div className="flex-1 min-w-0">
                    <span className="text-slate-300 truncate block">{n.preview}</span>
                    <span className="text-slate-600">{n.triples} triples · {n.node_id.slice(0, 12)}…</span>
                  </div>
                  <span className="text-slate-700 shrink-0">
                    <Clock className="w-2.5 h-2.5 inline mr-0.5" />
                    {new Date(n.ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
