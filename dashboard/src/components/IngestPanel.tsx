import { useState, useEffect, useCallback, useRef } from 'react';
import { Upload, ChevronDown, ChevronUp, CheckCircle2, Layers } from 'lucide-react';
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
  const toggleBtnRef = useRef<HTMLButtonElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const modeList: Array<'text' | 'url' | 'file'> = ['text', 'url', 'file'];
  const modeRefs = useRef<Record<'text' | 'url' | 'file', HTMLButtonElement | null>>({
    text: null,
    url: null,
    file: null,
  });

  const handleModeKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, current: 'text' | 'url' | 'file') => {
    const idx = modeList.indexOf(current);
    let nextIdx = -1;
    if (e.key === 'ArrowRight') nextIdx = (idx + 1) % modeList.length;
    else if (e.key === 'ArrowLeft') nextIdx = (idx - 1 + modeList.length) % modeList.length;
    else if (e.key === 'Home') nextIdx = 0;
    else if (e.key === 'End') nextIdx = modeList.length - 1;
    if (nextIdx !== -1) {
      e.preventDefault();
      const nextMode = modeList[nextIdx];
      setMode(nextMode);
      modeRefs.current[nextMode]?.focus();
    }
  };

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
    <div
      className="rounded-lg border border-border bg-card shadow-sm overflow-hidden font-sans"
      onKeyDown={(e) => {
        if (e.key === 'Escape' && open) {
          e.stopPropagation();
          setOpen(false);
          toggleBtnRef.current?.focus();
        }
      }}
    >
      {/* Header — toggle */}
      <button
        ref={toggleBtnRef}
        type="button"
        aria-expanded={open}
        aria-controls="silva-ingest-panel-body"
        onClick={() => setOpen(v => !v)}
        className="w-full flex items-center justify-between px-4 py-2.5 text-xs font-semibold text-primary hover:bg-muted/50 transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
      >
        <div className="flex items-center gap-2">
          <Upload className="w-3.5 h-3.5" aria-hidden="true" />
          <span>Ingesta de Conocimiento en SilvaDB</span>
          {recent.length > 0 && (
            <span className="px-1.5 py-0.5 rounded-md bg-muted text-primary text-[10px] font-bold border border-border">
              {recent.length}
            </span>
          )}
        </div>
        {open ? <ChevronUp className="w-3.5 h-3.5" aria-hidden="true" /> : <ChevronDown className="w-3.5 h-3.5" aria-hidden="true" />}
      </button>

      {open && (
        <div id="silva-ingest-panel-body" className="px-4 pb-4 space-y-3 border-t border-border bg-card">
          {/* Mode Selector Tabs */}
          <div role="tablist" aria-label="Modo de ingesta" className="flex items-center gap-1.5 border-b border-border pb-2 mt-2.5">
            <button
              ref={(el) => { modeRefs.current.text = el; }}
              type="button"
              role="tab"
              aria-selected={mode === 'text'}
              tabIndex={mode === 'text' ? 0 : -1}
              onClick={() => setMode('text')}
              onKeyDown={(e) => handleModeKeyDown(e, 'text')}
              className={cn(
                "px-2.5 py-1 rounded-md text-xs font-semibold transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                mode === 'text'
                  ? "bg-primary/10 text-primary border border-primary/30"
                  : "text-muted-foreground hover:bg-muted/60 hover:text-foreground border border-transparent"
              )}
            >
              Texto
            </button>
            <button
              ref={(el) => { modeRefs.current.url = el; }}
              type="button"
              role="tab"
              aria-selected={mode === 'url'}
              tabIndex={mode === 'url' ? 0 : -1}
              onClick={() => setMode('url')}
              onKeyDown={(e) => handleModeKeyDown(e, 'url')}
              className={cn(
                "px-2.5 py-1 rounded-md text-xs font-semibold transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                mode === 'url'
                  ? "bg-primary/10 text-primary border border-primary/30"
                  : "text-muted-foreground hover:bg-muted/60 hover:text-foreground border border-transparent"
              )}
            >
              Enlace URL
            </button>
            <button
              ref={(el) => { modeRefs.current.file = el; }}
              type="button"
              role="tab"
              aria-selected={mode === 'file'}
              tabIndex={mode === 'file' ? 0 : -1}
              onClick={() => setMode('file')}
              onKeyDown={(e) => handleModeKeyDown(e, 'file')}
              className={cn(
                "px-2.5 py-1 rounded-md text-xs font-semibold transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                mode === 'file'
                  ? "bg-primary/10 text-primary border border-primary/30"
                  : "text-muted-foreground hover:bg-muted/60 hover:text-foreground border border-transparent"
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
                className="w-full bg-muted/40 border border-border rounded-md p-2.5 text-xs text-foreground placeholder-muted-foreground font-mono resize-y focus:ring-2 focus:ring-primary focus:outline-none"
              />

              {/* Controls row */}
              <div className="flex flex-wrap items-center gap-2">
                {/* Tags */}
                <input
                  type="text"
                  value={tags}
                  onChange={e => setTags(e.target.value)}
                  placeholder="etiquetas (separadas por coma)"
                  className="flex-1 min-w-[120px] bg-muted/40 border border-border rounded-md px-2.5 py-1.5 text-xs text-foreground placeholder-muted-foreground focus:ring-2 focus:ring-primary focus:outline-none"
                />

                {/* Bulk toggle */}
                <button
                  type="button"
                  onClick={() => setBulk(v => !v)}
                  className={cn(
                    'flex items-center gap-1.5 px-3 py-1.5 rounded-md border text-xs font-semibold transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none',
                    bulk
                      ? 'bg-primary text-primary-foreground border-primary'
                      : 'bg-muted/50 border-border text-muted-foreground hover:bg-muted hover:text-foreground'
                  )}
                >
                  <Layers className="w-3 h-3" />
                  Por bloques
                </button>
              </div>

              {/* Bulk preview */}
              {bulk && bulkChunks && bulkChunks.length > 0 && (
                <div className="text-[10px] text-muted-foreground font-mono">
                  {bulkChunks.length} bloque{bulkChunks.length !== 1 ? 's' : ''} detectado{bulkChunks.length !== 1 ? 's' : ''}
                  {bulkChunks.map((c, i) => (
                    <span key={i} className="ml-1 px-1.5 py-0.5 rounded-md bg-muted text-foreground border border-border">
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
                className="w-full bg-muted/40 border border-border rounded-md p-2.5 text-xs text-foreground placeholder-muted-foreground focus:ring-2 focus:ring-primary focus:outline-none"
              />
              <p className="text-[11px] text-muted-foreground">
                Introduzca una URL para rastrear e indexar su contenido automáticamente en SilvaDB.
              </p>
            </div>
          )}

          {mode === 'file' && (
            <div className="space-y-2">
              <input
                ref={fileInputRef}
                type="file"
                className="hidden"
                onChange={ev => {
                  if (ev.target.files && ev.target.files[0]) {
                    setSelectedFile(ev.target.files[0]);
                  }
                }}
              />
              <div
                tabIndex={0}
                role="button"
                aria-label="Cargar archivo para ingesta en SilvaDB"
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    fileInputRef.current?.click();
                  }
                }}
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
                  fileInputRef.current?.click();
                }}
                className={cn(
                  "border-2 border-dashed rounded-lg p-5 text-center cursor-pointer transition-all flex flex-col items-center justify-center gap-1.5 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                  isDragOver
                    ? "border-primary bg-primary/10"
                    : "border-border bg-muted/30 hover:bg-muted/60"
                )}
              >
                <Upload className="w-5 h-5 text-primary" />
                {selectedFile ? (
                  <div className="text-xs text-foreground">
                    <p className="font-bold text-primary">{selectedFile.name}</p>
                    <p className="text-[10px] text-muted-foreground">{(selectedFile.size / 1024).toFixed(1)} KB</p>
                  </div>
                ) : (
                  <div className="text-xs text-foreground">
                    <p className="font-bold">Arrastre un archivo aquí</p>
                    <p className="text-[11px] text-muted-foreground">o pulse Enter / haga clic para examinar</p>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Submit */}
          <div className="flex items-center justify-between pt-1">
            <span className="text-[11px] text-muted-foreground font-mono">
              {mode === 'text' && `${charCount} caracteres`}
              {mode === 'url' && 'Modo URL'}
              {mode === 'file' && (selectedFile ? `${selectedFile.name}` : 'Modo Archivo')}
            </span>
            <button
              type="button"
              onClick={handleSubmit}
              disabled={!canSubmit}
              className={cn(
                'flex items-center gap-1.5 px-3.5 py-1.5 rounded-md text-xs font-semibold transition-all cursor-pointer shadow-sm focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none',
                canSubmit
                  ? 'bg-primary hover:bg-primary/90 text-primary-foreground border border-primary'
                  : 'bg-muted text-muted-foreground border border-border cursor-not-allowed opacity-60'
              )}
            >
              {loading ? (
                <span>Ingestando...</span>
              ) : (
                <>
                  <Upload className="w-3.5 h-3.5" />
                  {mode === 'text' && bulk && bulkChunks && bulkChunks.length > 1 ? `Ingestar ${bulkChunks.length} bloques` : 'Ingestar'}
                </>
              )}
            </button>
          </div>

          {/* Recent nodes */}
          {recent.length > 0 && (
            <div className="border-t border-border pt-2.5 space-y-1.5">
              <p className="text-[10px] text-primary font-bold uppercase tracking-wider">Recientes</p>
              {recent.map(n => (
                <div key={n.node_id + n.ts} className="flex items-start gap-1.5 text-[10px] font-mono">
                  <CheckCircle2 className="w-3 h-3 text-emerald-600 dark:text-emerald-400 shrink-0 mt-px" />
                  <div className="flex-1 min-w-0">
                    <span className="text-foreground truncate block">{n.preview}</span>
                    <span className="text-muted-foreground">{n.triples} relaciones · {n.node_id.slice(0, 12)}…</span>
                  </div>
                  <span className="text-muted-foreground shrink-0">
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
