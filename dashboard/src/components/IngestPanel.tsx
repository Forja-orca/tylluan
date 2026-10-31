import { useState, useEffect, useCallback } from 'react';
import { Upload, ChevronDown, ChevronUp, Clock, CheckCircle2, Layers } from 'lucide-react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { usePolling } from '../hooks/usePolling';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onIngestComplete?: () => void;
}

interface IngestRawNode {
  id: string;
  node_type?: string;
  content?: string;
  metadata?: string | Record<string, unknown>;
  updated_at?: string | number;
}

interface IngestParsedMeta {
  kind?: string;
  name?: string;
  source_file?: string;
  chunk_index?: number;
  chunk_total?: number;
  url?: string;
  triples_extracted?: number;
}

interface RecentNode {
  node_id: string;
  preview: string;
  triples: number;
  ts: number;
}

export function IngestPanel({ bridge, notify, onIngestComplete }: Props) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState('');
  const [tags, setTags] = useState('');
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
      const filtered = ((nodesList || []) as IngestRawNode[]).filter((n) => 
        n.node_type === 'episode' || 
        n.node_type === 'document' || 
        n.node_type === 'code_entity' ||
        n.node_type === 'note'
      );

      const mapped: RecentNode[] = filtered.map((n) => {
        let meta: IngestParsedMeta = {};
        try {
          meta = typeof n.metadata === 'string' ? JSON.parse(n.metadata) : ((n.metadata || {}) as IngestParsedMeta);
        } catch {}

        let preview = n.content || '';
        if (n.node_type === 'code_entity') {
          preview = `[Code] ${meta.kind || ''} ${meta.name || n.id}`;
        } else if (meta.source_file) {
          preview = `${meta.source_file} (Chunk ${(meta.chunk_index ?? 0) + 1}/${meta.chunk_total ?? 1}): ${n.content}`;
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
  }, [open, fetchRecentNodes]);

  // Polling via centralized coordinator (replaces 1 scattered setInterval)
  usePolling('ingest-nodes', fetchRecentNodes, { interval: 'medium', enabled: !!open });

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
        
        let parsedResult: { chunks?: number; nodes_created?: number } | null = null;
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
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e ?? 'unknown error');
      notify(`Ingest failed: ${msg}`, 'error');
    } finally {
      setLoading(false);
    }
  }, [bridge, canSubmit, mode, text, bulkChunks, tags, urlText, selectedFile, notify, onIngestComplete, fetchRecentNodes]);

  return (
    <div className="rounded-sm border border-[#ACA899] dark:border-border bg-[#ECE9D8] dark:bg-card overflow-hidden font-sans">
      {/* Header — toggle */}
      <button
        onClick={() => setOpen(v => !v)}
        className="w-full flex items-center justify-between px-3 py-1.5 text-[11px] font-bold text-[#0A246A] dark:text-primary hover:bg-[#F5F4EA] transition-colors cursor-pointer"
      >
        <div className="flex items-center gap-2">
          <Upload className="w-3.5 h-3.5" />
          <span>Ingesta de Conocimiento en SilvaDB</span>
          {recent.length > 0 && (
            <span className="px-1.5 py-0.2 rounded-none bg-white dark:bg-muted text-[#0A246A] text-[10px] font-bold border border-[#7F9DB9]">
              {recent.length}
            </span>
          )}
        </div>
        {open ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
      </button>

      {open && (
        <div className="px-3 pb-3 space-y-2.5 border-t border-[#ACA899] dark:border-border bg-white dark:bg-card">
          {/* Mode Selector Tabs */}
          <div className="flex items-center gap-1 border-b border-[#ACA899] pb-1 mt-2">
            <button
              onClick={() => setMode('text')}
              className={cn(
                "px-2 py-0.5 rounded-sm text-[11px] font-bold transition-colors cursor-pointer",
                mode === 'text'
                  ? "bg-[#0A246A] text-white"
                  : "text-[#444444] hover:bg-[#ECE9D8]"
              )}
            >
              Texto
            </button>
            <button
              onClick={() => setMode('url')}
              className={cn(
                "px-2 py-0.5 rounded-sm text-[11px] font-bold transition-colors cursor-pointer",
                mode === 'url'
                  ? "bg-[#0A246A] text-white"
                  : "text-[#444444] hover:bg-[#ECE9D8]"
              )}
            >
              Enlace URL
            </button>
            <button
              onClick={() => setMode('file')}
              className={cn(
                "px-2 py-0.5 rounded-sm text-[11px] font-bold transition-colors cursor-pointer",
                mode === 'file'
                  ? "bg-[#0A246A] text-white"
                  : "text-[#444444] hover:bg-[#ECE9D8]"
              )}
            >
              Archivo
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
                  ? 'Pegue varios bloques separados por líneas de ---\nTexto del bloque 1\n---\nTexto del bloque 2'
                  : 'Pegue texto, hechos o conocimiento para almacenar en SilvaDB...'}
                rows={5}
                className="w-full bg-white dark:bg-muted border border-[#7F9DB9] dark:border-border rounded-none p-2 text-[11px] text-[#000000] dark:text-foreground placeholder-[#888888] font-mono resize-y focus:outline-none"
              />

              {/* Controls row */}
              <div className="flex flex-wrap items-center gap-2">
                {/* Tags */}
                <input
                  type="text"
                  value={tags}
                  onChange={e => setTags(e.target.value)}
                  placeholder="etiquetas (separadas por coma)"
                  className="flex-1 min-w-[120px] bg-white dark:bg-muted border border-[#7F9DB9] dark:border-border rounded-none px-2 py-1 text-[11px] text-[#000000] dark:text-foreground placeholder-[#888888] focus:outline-none"
                />

                {/* Bulk toggle */}
                <button
                  onClick={() => setBulk(v => !v)}
                  className={cn(
                    'flex items-center gap-1 px-2.5 py-1 rounded-sm border text-[11px] font-bold transition-colors cursor-pointer',
                    bulk
                      ? 'bg-[#0A246A] text-white border-[#0A246A]'
                      : 'bg-[#ECE9D8] border-[#ACA899] text-[#444444] hover:bg-[#F5F4EA]'
                  )}
                >
                  <Layers className="w-3 h-3" />
                  Por bloques
                </button>
              </div>

              {/* Bulk preview */}
              {bulk && bulkChunks && bulkChunks.length > 0 && (
                <div className="text-[10px] text-[#555555] font-mono">
                  {bulkChunks.length} bloque{bulkChunks.length !== 1 ? 's' : ''} detectado{bulkChunks.length !== 1 ? 's' : ''}
                  {bulkChunks.map((c, i) => (
                    <span key={i} className="ml-1 px-1 py-0.2 rounded-none bg-[#ECE9D8] text-[#000000]">
                      {c.slice(0, 25)}{c.length > 25 ? '…' : ''}
                    </span>
                  ))}
                </div>
              )}
            </>
          )}

          {mode === 'url' && (
            <div className="space-y-2">
              <input
                type="text"
                value={urlText}
                onChange={e => setUrlText(e.target.value)}
                placeholder="https://ejemplo.com/articulo"
                className="w-full bg-white dark:bg-muted border border-[#7F9DB9] dark:border-border rounded-none p-2 text-[11px] text-[#000000] dark:text-foreground placeholder-[#888888] focus:outline-none"
              />
              <p className="text-[10px] text-[#555555]">
                Introduzca una URL para rastrear e indexar su contenido automáticamente en SilvaDB.
              </p>
            </div>
          )}

          {mode === 'file' && (
            <div className="space-y-2">
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
                  "border-2 border-dashed rounded-none p-4 text-center cursor-pointer transition-all flex flex-col items-center justify-center gap-1",
                  isDragOver
                    ? "border-[#0A246A] bg-[#ECE9D8]"
                    : "border-[#ACA899] bg-[#ECE9D8] hover:bg-[#F5F4EA]"
                )}
              >
                <Upload className="w-5 h-5 text-[#0A246A]" />
                {selectedFile ? (
                  <div className="text-[11px] text-[#000000]">
                    <p className="font-bold text-[#0A246A]">{selectedFile.name}</p>
                    <p className="text-[10px] text-[#555555]">{(selectedFile.size / 1024).toFixed(1)} KB</p>
                  </div>
                ) : (
                  <div className="text-[11px] text-[#444444]">
                    <p className="font-bold">Arrastre un archivo aquí</p>
                    <p className="text-[10px] text-[#666666]">o haga clic para examinar</p>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Submit */}
          <div className="flex items-center justify-between pt-1">
            <span className="text-[10px] text-[#555555] font-mono">
              {mode === 'text' && `${charCount} caracteres`}
              {mode === 'url' && 'Modo URL'}
              {mode === 'file' && (selectedFile ? `${selectedFile.name}` : 'Modo Archivo')}
            </span>
            <button
              onClick={handleSubmit}
              disabled={!canSubmit}
              className={cn(
                'flex items-center gap-1.5 px-3 py-1 rounded-sm text-[11px] font-bold transition-all cursor-pointer',
                canSubmit
                  ? 'bg-[#0A246A] hover:bg-[#081C52] text-white border border-[#0A246A]'
                  : 'bg-[#ECE9D8] text-[#888888] border border-[#ACA899] cursor-not-allowed'
              )}
            >
              {loading ? (
                <span>Ingestando...</span>
              ) : (
                <>
                  <Upload className="w-3 h-3" />
                  {mode === 'text' && bulk && bulkChunks && bulkChunks.length > 1 ? `Ingestar ${bulkChunks.length} bloques` : 'Ingestar'}
                </>
              )}
            </button>
          </div>

          {/* Recent nodes */}
          {recent.length > 0 && (
            <div className="border-t border-[#ACA899] pt-2 space-y-1">
              <p className="text-[10px] text-[#01285F] font-bold uppercase">Recientes</p>
              {recent.map(n => (
                <div key={n.node_id + n.ts} className="flex items-start gap-1.5 text-[10px] font-mono">
                  <CheckCircle2 className="w-3 h-3 text-[#008000] shrink-0 mt-px" />
                  <div className="flex-1 min-w-0">
                    <span className="text-[#000000] dark:text-foreground truncate block">{n.preview}</span>
                    <span className="text-[#555555]">{n.triples} relaciones · {n.node_id.slice(0, 12)}…</span>
                  </div>
                  <span className="text-[#666666] shrink-0">
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
