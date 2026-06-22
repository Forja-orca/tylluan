import React, { useState, useId } from 'react';
import { 
  AlertTriangle, 
  RefreshCw, 
  Search,
  Image as ImageIcon,
  FileJson
} from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import type { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

interface Props {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

interface AnalysisResult {
  description?: string;
  error?: string;
  node_id?: string | null;
  triples_extracted?: number;
  status?: string;
  ocr?: string;
}

export function VisionTab({ bridge, notify }: Props) {
  const { guilds, refreshData } = useNexus();
  const [imagePath, setImagePath] = useState('');
  const [prompt, setPrompt] = useState('Describe this image in detail.');
  const promptId = useId();
  const [result, setResult] = useState('');
  const [parsedResult, setParsedResult] = useState<AnalysisResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [starting, setStarting] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [imagePreview, setImagePreview] = useState<string | null>(null);
  const [kernelConfig, setKernelConfig] = useState<any>(null);

  React.useEffect(() => {
    bridge?.fetchRaw('/api/v1/config').then(setKernelConfig).catch(() => {});
  }, [bridge]);

  const visionGuild = guilds.find(g => g.name === 'vision');
  const visionRunning = visionGuild?.running ?? false;

  const analyze = async (mode: 'analyze' | 'ocr' | 'extract') => {
    if (!bridge || !imagePath.trim()) return;
    if (!visionRunning) {
      notify('Vision guild is not running. Start it from the Guilds tab first.', 'error');
      return;
    }
    setLoading(true);
    setResult('');
    setParsedResult(null);
    try {
      let res;
      const endpoint = mode === 'analyze' ? 'vision_analyze' : mode === 'ocr' ? 'vision_ocr' : 'vision_extract';
      const body = mode === 'analyze' ? { image_path: imagePath, prompt } : 
                   mode === 'ocr' ? { image_path: imagePath } : 
                   { image_path: imagePath, schema: prompt };

      res = await bridge.fetchRaw(`/api/v1/guilds/vision/call/${endpoint}`, {
        method: 'POST',
        body: JSON.stringify(body)
      });
      // MCP response: { content: [{type:"text", text:"..."}] }
      // Extract innermost text regardless of nesting
      let text: string;
      if (Array.isArray(res?.content) && res.content.length > 0) {
        text = res.content.map((c: any) => c.text ?? '').join('\n').trim();
      } else {
        text = res?.result ?? res?.output ?? JSON.stringify(res, null, 2);
      }
      setResult(text);

      // Parse inner JSON from guild response
      try {
        const parsed: AnalysisResult = JSON.parse(text);
        setParsedResult(parsed);
      } catch {
        setParsedResult(null);
      }
      
      try {
        const p = JSON.parse(text) as AnalysisResult;
        if (p.status === 'degraded' || p.status === 'error') {
          notify(`Vision ${mode}: ${p.status} — ${p.description ?? p.error ?? ''}`, 'error');
        } else {
          notify(`Vision ${mode} complete`, 'info');
        }
      } catch { notify(`Vision ${mode} complete`, 'info'); }
    } catch (e) {
      notify(`Vision ${mode} failed: ${e instanceof Error ? e.message : 'Unknown error'}`, 'error');
      setResult(`Error: ${e instanceof Error ? e.message : 'Unknown error'}`);
      setParsedResult(null);
    }
    setLoading(false);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file || !file.type.startsWith('image/')) return;

    setImagePreview(URL.createObjectURL(file));

    // Browser never exposes the real local path — upload to kernel first
    if (!bridge) { notify('No bridge connection', 'error'); return; }
    try {
      const form = new FormData();
      form.append('file', file, file.name);
      const res = await fetch('/api/v1/ingest/upload', { method: 'POST', body: form });
      const json = await res.json() as { file?: string; status?: string };
      if (json.file) {
        // Kernel saves to data/ingest/ relative to workspace root
        const serverPath = `data/ingest/${json.file}`;
        setImagePath(serverPath);
        notify(`Uploaded: ${file.name} → ${serverPath}`, 'info');
      } else {
        notify(`Upload failed: ${JSON.stringify(json)}`, 'error');
      }
    } catch (err) {
      notify(`Upload error: ${err instanceof Error ? err.message : String(err)}`, 'error');
    }
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };

  return (
    <div className="space-y-4">
      {/* Guild Status Banner */}
      {!visionRunning && (
        <div className="bg-amber-900/30 border border-amber-700/50 rounded-lg p-3 text-xs text-amber-300">
          <div className="flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5" />
            <div className="flex-1">
              <p className="font-bold">Vision guild inactivo</p>
              <p className="mt-1 text-[11px] opacity-80">
                Para activar:
              </p>
              <code className="block mt-1 px-2 py-1 bg-amber-950/50 rounded text-[10px] font-mono">
                pip install transformers torch pillow
              </code>
              <p className="mt-1 text-[11px] opacity-80">y reiniciar el kernel.</p>
            </div>
            <button
              type="button"
              disabled={starting || !bridge}
              onClick={async () => {
                if (!bridge) return;
                setStarting(true);
                try {
                  await bridge.startGuild('vision');
                  notify('Vision guild started', 'info');
                  await refreshData();
                } catch (e) {
                  notify(`Vision start failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
                } finally {
                  setStarting(false);
                }
              }}
              className="px-3 py-1 bg-amber-500/20 hover:bg-amber-500/30 disabled:opacity-50 rounded font-bold uppercase tracking-wider text-[10px] flex items-center gap-1 shrink-0"
            >
              {starting && <RefreshCw className="w-3 h-3 animate-spin" />}
              {starting ? 'Loading vision model...' : 'Warm-up Vision Guild'}
            </button>
          </div>
        </div>
      )}

      {/* Model availability banner */}
      {visionRunning && (
        <div className="bg-blue-900/20 border border-blue-700/30 rounded-lg p-3 text-xs text-blue-300">
          <div className="flex items-center gap-2">
            <ImageIcon className="w-4 h-4 flex-shrink-0" />
            <span>Modelo SmolVLM2 cargado — análisis en modo degradado (fallback OCR disponible)</span>
          </div>
        </div>
      )}

      <div className="rounded-lg border border-slate-800 bg-slate-900/50 overflow-hidden">
        <div className="px-4 py-2 border-b border-slate-800 bg-slate-900/80 flex items-center justify-between">
          <span className="text-xs font-medium uppercase tracking-widest text-slate-400">Sovereign Visual Intelligence</span>
          <div className={cn("flex items-center gap-1.5 text-[10px] font-bold",
            visionRunning ? "text-emerald-400" : "text-slate-600"
          )}>
            <div className={cn("w-1.5 h-1.5 rounded-full", visionRunning ? "bg-emerald-500 animate-pulse" : "bg-slate-700")} />
            {visionRunning ? 'GUILD ONLINE' : 'GUILD OFFLINE'}
          </div>
        </div>
        <div className="p-4 space-y-4">
          {/* Drag & Drop zone */}
          <div
            onDrop={handleDrop}
            onDragOver={handleDragOver}
            onDragLeave={() => setIsDragging(false)}
            className={cn(
              "border-2 border-dashed rounded-xl p-8 text-center transition-all cursor-pointer relative overflow-hidden",
              isDragging ? "border-emerald-500 bg-emerald-500/10 scale-[1.01]" : "border-slate-600 hover:border-emerald-500"
            )}
          >
            {imagePreview && (
              <img src={imagePreview} className="absolute inset-0 w-full h-full object-contain opacity-10 pointer-events-none" alt="" />
            )}
            <ImageIcon className={cn("w-8 h-8 mx-auto mb-2 relative z-10 transition-colors", isDragging ? "text-emerald-500" : "text-slate-600")} />
            <p className="text-slate-400 text-sm relative z-10 font-medium">{isDragging ? 'Suelta la imagen aquí' : 'Arrastra una imagen aquí'}</p>
            <p className="text-slate-600 text-xs mt-1 relative z-10">o escribe la ruta abajo</p>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <div className="md:col-span-3">
              <label className="block text-[10px] text-slate-500 uppercase mb-1" htmlFor="vision-image-path">Image Path (Absolute)</label>
              <input
                id="vision-image-path"
                type="text"
                value={imagePath}
                onChange={(e) => { setImagePath(e.target.value); setImagePreview(null); }}
                placeholder="e:/path/to/image.jpg"
                className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded text-sm font-mono text-slate-300"
              />
            </div>
            <div>
              <div className="block text-[10px] text-slate-500 uppercase mb-1">Inference Model</div>
              <div className="px-3 py-2 bg-slate-900 border border-slate-800 rounded text-xs text-emerald-400 font-mono">
                {kernelConfig?.vision?.model_path?.split('/').pop() ?? 'SmolVLM2-256M'}
              </div>
            </div>
          </div>

          <div>
            <label className="block text-[10px] text-slate-500 uppercase mb-1" htmlFor="vision-prompt">Prompt / Schema</label>
            <textarea
              id="vision-prompt"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={3}
              className="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded text-sm font-mono text-slate-300"
            />
          </div>

          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => analyze('analyze')}
              disabled={loading || !imagePath || !visionRunning}
              className="px-4 py-2 bg-emerald-500/20 hover:bg-emerald-500/30 disabled:opacity-40 text-emerald-400 rounded text-xs font-medium flex items-center gap-2"
            >
              {loading ? <RefreshCw className="w-3 h-3 animate-spin" /> : <Search className="w-3 h-3" />} Analyze Image
            </button>
            <button
              type="button"
              onClick={() => analyze('ocr')}
              disabled={loading || !imagePath || !visionRunning}
              className="px-4 py-2 bg-blue-500/20 hover:bg-blue-500/30 disabled:opacity-40 text-blue-400 rounded text-xs font-medium"
            >
              OCR (Extract Text)
            </button>
            <button
              type="button"
              onClick={() => analyze('extract')}
              disabled={loading || !imagePath || !visionRunning}
              className="px-4 py-2 bg-violet-500/20 hover:bg-violet-500/30 disabled:opacity-40 text-violet-400 rounded text-xs font-medium"
            >
              Extract Structured JSON
            </button>
          </div>
        </div>
      </div>

      {/* Result section */}
      <div className="rounded-lg border border-slate-800 bg-slate-950 overflow-hidden">
        <div className="px-4 py-1.5 border-b border-slate-800 text-[10px] text-slate-500 uppercase tracking-widest flex items-center gap-2">
          <FileJson className="w-3 h-3" />
          Analysis Result
        </div>
        {loading ? (
          <div className="p-8 min-h-[100px] flex flex-col items-center justify-center gap-3">
            <RefreshCw className="w-8 h-8 text-emerald-500 animate-spin" />
            <p className="text-xs text-slate-500 animate-pulse font-sans">Running SmolVLM2 on CPU...</p>
          </div>
        ) : parsedResult ? (
          <div className="p-4 space-y-3">
            {/* Status badge */}
            {parsedResult.status && (
              <div className="flex items-center gap-2">
                <span className={cn(
                  "px-2 py-0.5 rounded-full text-[10px] font-bold uppercase tracking-wider",
                  parsedResult.status === 'ok' ? "bg-emerald-500/20 text-emerald-400 border border-emerald-500/30" :
                  parsedResult.status === 'degraded' ? "bg-amber-500/20 text-amber-400 border border-amber-500/30" :
                  parsedResult.status === 'error' ? "bg-red-500/20 text-red-400 border border-red-500/30" :
                  "bg-slate-500/20 text-slate-400 border border-slate-500/30"
                )}>
                  {parsedResult.status}
                </span>
                {/* SilvaDB node_id badge */}
                {parsedResult.node_id && (
                  <span className="px-2 py-0.5 rounded-full text-[10px] font-mono bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                    Nodo SilvaDB: {parsedResult.node_id.slice(0, 12)}...
                  </span>
                )}
                {/* Triples count */}
                {parsedResult.triples_extracted !== undefined && parsedResult.triples_extracted > 0 && (
                  <span className="px-2 py-0.5 rounded-full text-[10px] font-mono bg-blue-500/10 text-blue-400 border border-blue-500/20">
                    Triples: {parsedResult.triples_extracted}
                  </span>
                )}
              </div>
            )}
            {/* Description */}
            {parsedResult.description && (
              <div>
                <label className="block text-[10px] text-slate-500 uppercase mb-1">Description</label>
                <textarea
                  readOnly
                  value={parsedResult.description}
                  rows={6}
                  className="w-full px-3 py-2 bg-slate-900 border border-slate-800 rounded text-xs font-mono text-slate-300 resize-y"
                />
              </div>
            )}
            {/* Error */}
            {parsedResult.error && (
              <div>
                <label className="block text-[10px] text-red-500 uppercase mb-1">Error</label>
                <p className="px-3 py-2 bg-red-950/30 border border-red-900/30 rounded text-xs text-red-400 font-mono">
                  {parsedResult.error}
                </p>
              </div>
            )}
            {/* OCR fallback */}
            {parsedResult.ocr && (
              <div>
                <label className="block text-[10px] text-slate-500 uppercase mb-1">OCR</label>
                <textarea
                  readOnly
                  value={parsedResult.ocr}
                  rows={4}
                  className="w-full px-3 py-2 bg-slate-900 border border-slate-800 rounded text-xs font-mono text-slate-300 resize-y"
                />
              </div>
            )}
          </div>
        ) : result ? (
          <div className="p-4 min-h-[100px] whitespace-pre-wrap font-mono text-xs text-slate-300 leading-relaxed">
            {result}
          </div>
        ) : (
          <div className="p-8 min-h-[100px] flex items-center justify-center">
            <span className="text-slate-600 font-sans text-sm">Waiting for input...</span>
          </div>
        )}
      </div>
    </div>
  );
}
