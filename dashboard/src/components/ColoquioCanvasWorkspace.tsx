import { useState, useEffect, useRef, useMemo } from 'react';
import {
  Monitor, FileText, PenTool, Network,
  Play, Copy, ChevronDown, ChevronRight,
  RotateCcw, Maximize2, Trash, Link2,
  History, Clock
} from 'lucide-react';
import { cn } from '../lib/utils';

import { ColoquioMessage, CanvasNode, CanvasEdge } from './coloquio-types';

type Tab = 'preview' | 'docs' | 'whiteboard' | 'knowledge';

interface CodeArtifact {
  id: string;
  label: string;
  lang: string;
  code: string;
  turn: number;
  author: string;
}

function extractCodeBlocks(messages: ColoquioMessage[]): CodeArtifact[] {
  const artifacts: CodeArtifact[] = [];
  const fence = /```(\w*)\n([\s\S]*?)```/g;
  for (const msg of messages) {
    let match: RegExpExecArray | null;
    fence.lastIndex = 0;
    while ((match = fence.exec(msg.content)) !== null) {
      const lang = match[1] || 'text';
      const code = match[2].trim();
      if (!code) continue;
      artifacts.push({
        id: `${msg.msg_id}-${artifacts.length}`,
        label: `T${msg.turn} · ${lang}`,
        lang,
        code,
        turn: msg.turn,
        author: msg.author_id,
      });
    }
  }
  return artifacts.reverse();
}

function buildSrcdoc(code: string, lang: string): string {
  if (['html', 'htm', ''].includes(lang.toLowerCase())) {
    return code;
  }
  if (['js', 'javascript', 'ts', 'typescript'].includes(lang.toLowerCase())) {
    return `<!doctype html><html><head><meta charset="utf-8">
<style>body{font-family:monospace;background:#0f172a;color:#e2e8f0;padding:1rem;}</style>
</head><body><script>
try { ${code} } catch(e) { document.body.innerHTML = '<pre style="color:#f87171">' + e + '</pre>'; }
<\/script></body></html>`;
  }
  if (['css'].includes(lang.toLowerCase())) {
    return `<!doctype html><html><head><meta charset="utf-8">
<style>body{background:#0f172a;color:#e2e8f0;font-family:monospace;padding:1rem;} ${code}</style>
</head><body><p>CSS preview — add HTML elements to see styles applied.</p></body></html>`;
  }
  // Generic: show as formatted code
  const escaped = code.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  return `<!doctype html><html><head><meta charset="utf-8">
<style>body{margin:0;background:#0f172a;} pre{padding:1rem;color:#e2e8f0;font-family:'Fira Code',monospace;font-size:13px;white-space:pre-wrap;word-break:break-all;}</style>
</head><body><pre>${escaped}</pre></body></html>`;
}

// ── PREVIEW TAB ──────────────────────────────────────────────────────────────
function PreviewTab({ messages }: { messages: ColoquioMessage[] }) {
  const artifacts = useMemo(() => extractCodeBlocks(messages), [messages]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(false);

  const selected = artifacts.find(a => a.id === selectedId) ?? artifacts[0] ?? null;
  const srcdoc = selected ? buildSrcdoc(selected.code, selected.lang) : null;

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Artifact picker */}
      <div className="shrink-0 border-b border-slate-700/60 bg-slate-900/80">
        <button
          onClick={() => setCollapsed(p => !p)}
          className="flex items-center gap-2 w-full px-3 py-1.5 text-[10px] font-bold text-slate-400 uppercase tracking-widest hover:text-slate-200 transition-colors cursor-pointer"
        >
          {collapsed ? <ChevronRight className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
          {artifacts.length} artifact{artifacts.length !== 1 ? 's' : ''} detected
        </button>
        {!collapsed && (
          <div className="flex flex-col max-h-32 overflow-y-auto px-2 pb-2 gap-1">
            {artifacts.length === 0 && (
              <p className="text-[11px] text-slate-500 px-1 py-2">
                No code blocks in the channel. Paste code in chat and it will appear here.
              </p>
            )}
            {artifacts.map(a => (
              <button
                key={a.id}
                onClick={() => setSelectedId(a.id)}
                className={cn(
                  'flex items-center gap-2 px-2 py-1 rounded text-left transition-colors cursor-pointer',
                  (selected?.id === a.id)
                    ? 'bg-indigo-950 border border-indigo-500/40 text-indigo-300'
                    : 'hover:bg-slate-800 text-slate-400 border border-transparent'
                )}
              >
                <Play className="w-2.5 h-2.5 shrink-0" />
                <span className="text-[10px] font-mono truncate flex-1">{a.label}</span>
                <span className="text-[9px] text-slate-600">{a.author}</span>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* iframe sandbox */}
      <div className="flex-1 relative overflow-hidden bg-white">
        {srcdoc ? (
          <iframe
            key={selected?.id}
            srcDoc={srcdoc}
            sandbox="allow-scripts allow-same-origin allow-forms allow-modals"
            className="w-full h-full border-none"
            title="Canvas Preview"
          />
        ) : (
          <div className="flex flex-col items-center justify-center h-full bg-[#06080d] gap-3">
            <Monitor className="w-10 h-10 text-slate-700" />
            <p className="text-xs text-slate-500 text-center px-6">
              Pega bloques de código HTML/JS/CSS en el chat.<br />
              Aparecerán aquí para ejecutar en vivo.
            </p>
          </div>
        )}
      </div>

      {/* Bottom bar */}
      {selected && (
        <div className="shrink-0 border-t border-slate-700/60 bg-slate-900/80 px-3 py-1.5 flex items-center gap-2">
          <span className="text-[10px] text-slate-500 font-mono flex-1 truncate">
            {selected.lang || 'text'} · {selected.code.split('\n').length} líneas
          </span>
          <button
            onClick={() => navigator.clipboard.writeText(selected.code)}
            className="text-[10px] text-slate-400 hover:text-slate-200 flex items-center gap-1 cursor-pointer"
          >
            <Copy className="w-3 h-3" /> copiar
          </button>
        </div>
      )}
    </div>
  );
}

interface DiffLine {
  type: 'added' | 'removed' | 'normal';
  value: string;
}

function computeLineDiff(oldText: string, newText: string): DiffLine[] {
  const oldLines = oldText ? oldText.split('\n') : [];
  const newLines = newText ? newText.split('\n') : [];
  const result: DiffLine[] = [];
  const m = oldLines.length;
  const n = newLines.length;
  
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (oldLines[i - 1] === newLines[j - 1]) {
        dp[i][j] = dp[i - 1][j - 1] + 1;
      } else {
        dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
      }
    }
  }
  
  let i = m, j = n;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && oldLines[i - 1] === newLines[j - 1]) {
      result.unshift({ type: 'normal', value: oldLines[i - 1] });
      i--;
      j--;
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      result.unshift({ type: 'added', value: newLines[j - 1] });
      j--;
    } else if (i > 0 && (j === 0 || dp[i - 1][j] > dp[i][j - 1])) {
      result.unshift({ type: 'removed', value: oldLines[i - 1] });
      i--;
    }
  }
  return result;
}

// ── DOCS TAB — collaborative live document ────────────────────────────────────
interface CollabDoc { doc_id: string; title: string; content: string; updated_by: string; version: number; updated_at: number; }

function DocsTab({ authorId = 'user' }: { authorId?: string }) {
  const [docs, setDocs] = useState<CollabDoc[]>([]);
  const [selectedDocId, setSelectedDocId] = useState<string | null>(null);
  const [content, setContent] = useState('');
  const [title, setTitle] = useState('');
  const [remoteVersion, setRemoteVersion] = useState(0);
  const [saveStatus, setSaveStatus] = useState<'saved' | 'saving' | 'unsaved' | 'conflict'>('saved');
  const [lastEditor, setLastEditor] = useState('');
  const [creating, setCreating] = useState(false);
  const [newTitle, setNewTitle] = useState('');
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const localEditRef = useRef(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Version History states
  const [showHistory, setShowHistory] = useState(false);
  const [versions, setVersions] = useState<{ version: number; updated_by: string; updated_at: number; title: string }[]>([]);
  const [selectedVersionIdx, setSelectedVersionIdx] = useState<number | null>(null);
  const [versionContent, setVersionContent] = useState<string>('');
  const [prevVersionContent, setPrevVersionContent] = useState<string>('');

  const BASE = '/api/v1/coloquio/documents';

  const loadDocs = async () => {
    try {
      const r = await fetch(BASE);
      const d = await r.json();
      setDocs(d.documents ?? []);
    } catch { /* network error, ignore */ }
  };

  const deleteDoc = async (id: string, title: string) => {
    if (!confirm(`¿Borrar "${title}"? Esta acción no se puede deshacer.`)) return;
    await fetch(`${BASE}/${id}`, { method: 'DELETE' });
    setDocs(prev => prev.filter(d => d.doc_id !== id));
    if (selectedDocId === id) { setSelectedDocId(null); setContent(''); setTitle(''); }
  };

  const loadDoc = async (id: string, force = false) => {
    try {
      const r = await fetch(`${BASE}/${id}`);
      if (!r.ok) return;
      const d: CollabDoc = (await r.json()).document;
      if (force || d.version > remoteVersion) {
        if (!force && localEditRef.current && d.version > remoteVersion + 1) {
          setSaveStatus('conflict');
        }
        setRemoteVersion(d.version);
        setLastEditor(d.updated_by);
        if (!localEditRef.current || force) {
          setContent(d.content);
          setTitle(d.title);
          setSaveStatus('saved');
        }
      }
    } catch { /* ignore */ }
  };

  const saveDoc = async (id: string, newTitle: string, newContent: string) => {
    setSaveStatus('saving');
    try {
      const r = await fetch(`${BASE}/${id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title: newTitle, content: newContent, updated_by: authorId }),
      });
      if (r.ok) {
        const d: CollabDoc = (await r.json()).document;
        setRemoteVersion(d.version);
        setLastEditor(authorId);
        setSaveStatus('saved');
        localEditRef.current = false;
      }
    } catch {
      setSaveStatus('unsaved');
    }
  };

  const createDoc = async () => {
    if (!newTitle.trim()) return;
    setCreating(false);
    try {
      const r = await fetch(BASE, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title: newTitle.trim(), created_by: authorId }),
      });
      const d: CollabDoc = (await r.json()).document;
      setDocs(prev => [d, ...prev]);
      setSelectedDocId(d.doc_id);
      setContent(d.content);
      setTitle(d.title);
      setRemoteVersion(d.version);
      setSaveStatus('saved');
      setNewTitle('');
    } catch { /* ignore */ }
  };

  // Fetch all snapshots/versions
  const loadVersions = async (docId: string) => {
    try {
      const r = await fetch(`${BASE}/${docId}/versions`);
      if (r.ok) {
        const data = await r.json();
        const vList = data.versions ?? [];
        vList.sort((a: any, b: any) => b.version - a.version);
        setVersions(vList);
        if (vList.length > 0) {
          setSelectedVersionIdx(0);
          loadVersionContent(docId, vList[0].version, vList);
        }
      }
    } catch { /* ignore */ }
  };

  // Load content of specific version + its predecessor for diffing
  const loadVersionContent = async (docId: string, versionNum: number, currentVersionsList?: typeof versions) => {
    try {
      const activeList = currentVersionsList ?? versions;
      const r = await fetch(`${BASE}/${docId}/versions/${versionNum}`);
      if (!r.ok) return;
      const data = await r.json();
      const currentSnap = data.snapshot?.content ?? '';
      setVersionContent(currentSnap);
      
      const idx = activeList.findIndex(v => v.version === versionNum);
      if (idx !== -1 && idx + 1 < activeList.length) {
        const prevVer = activeList[idx + 1].version;
        const rPrev = await fetch(`${BASE}/${docId}/versions/${prevVer}`);
        if (rPrev.ok) {
          const dPrev = await rPrev.json();
          setPrevVersionContent(dPrev.snapshot?.content ?? '');
        } else {
          setPrevVersionContent('');
        }
      } else {
        setPrevVersionContent('');
      }
    } catch { /* ignore */ }
  };

  const restoreVersion = async () => {
    if (selectedVersionIdx === null || !selectedDocId) return;
    const snap = versions[selectedVersionIdx];
    if (!confirm(`Restore the document to version ${snap.version} state?`)) return;
    await saveDoc(selectedDocId, snap.title, versionContent);
    await loadDocs();
    selectDoc(selectedDocId);
    setShowHistory(false);
  };

  // Initial load
  useEffect(() => { loadDocs(); }, []);

  // Select first doc when list loads
  useEffect(() => {
    if (!selectedDocId && docs.length > 0) {
      setSelectedDocId(docs[0].doc_id);
      loadDoc(docs[0].doc_id, true);
    }
  }, [docs]);

  // Poll for remote changes every 3s
  useEffect(() => {
    if (!selectedDocId || showHistory) return;
    if (pollRef.current) clearInterval(pollRef.current);
    pollRef.current = setInterval(() => {
      if (!localEditRef.current) loadDoc(selectedDocId);
    }, 3000);
    return () => { if (pollRef.current) clearInterval(pollRef.current); };
  }, [selectedDocId, remoteVersion, showHistory]);

  const onContentChange = (val: string) => {
    setContent(val);
    localEditRef.current = true;
    setSaveStatus('unsaved');
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    if (selectedDocId) {
      saveTimerRef.current = setTimeout(() => saveDoc(selectedDocId, title, val), 1500);
    }
  };

  const onTitleChange = (val: string) => {
    setTitle(val);
    localEditRef.current = true;
    setSaveStatus('unsaved');
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    if (selectedDocId) {
      saveTimerRef.current = setTimeout(() => saveDoc(selectedDocId, val, content), 1500);
    }
  };

  const selectDoc = (id: string) => {
    setSelectedDocId(id);
    localEditRef.current = false;
    setRemoteVersion(0);
    loadDoc(id, true);
  };

  const STATUS_COLOR: Record<string, string> = {
    saved: 'text-emerald-500',
    saving: 'text-amber-400',
    unsaved: 'text-slate-500',
    conflict: 'text-rose-400',
  };
  const STATUS_LABEL: Record<string, string> = {
    saved: 'guardado',
    saving: 'guardando...',
    unsaved: 'sin guardar',
    conflict: '⚠ conflicto',
  };

  if (showHistory && selectedDocId) {
    const diffLines = computeLineDiff(prevVersionContent, versionContent);
    const selectedSnap = selectedVersionIdx !== null ? versions[selectedVersionIdx] : null;

    return (
      <div className="flex h-full overflow-hidden bg-[#06080d] text-slate-200">
        {/* Sidebar Timeline */}
        <div className="w-40 shrink-0 border-r border-slate-700/60 bg-slate-900/60 flex flex-col">
          <div className="px-2.5 py-2 border-b border-slate-700/60 flex items-center justify-between">
            <span className="text-[9px] font-bold text-slate-400 uppercase tracking-wider">Historial</span>
            <button
              onClick={() => setShowHistory(false)}
              className="text-[9px] text-indigo-400 hover:text-indigo-300 font-bold cursor-pointer"
            >
              Cerrar
            </button>
          </div>
          <div className="flex-1 overflow-y-auto">
            {versions.map((v, idx) => (
              <button
                key={v.version}
                onClick={() => {
                  setSelectedVersionIdx(idx);
                  loadVersionContent(selectedDocId, v.version);
                }}
                className={cn(
                  'w-full text-left px-2.5 py-2 border-b border-slate-800/60 flex flex-col gap-0.5 transition-colors cursor-pointer',
                  selectedVersionIdx === idx ? 'bg-indigo-950/60' : 'hover:bg-slate-800/40'
                )}
              >
                <div className="flex justify-between items-center w-full">
                  <span className="text-[10px] font-bold font-mono text-indigo-300">v{v.version}</span>
                  <span className="text-[8px] text-slate-500">
                    {new Date(v.updated_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                  </span>
                </div>
                <span className="text-[9px] text-slate-400 truncate">por {v.updated_by}</span>
              </button>
            ))}
          </div>
        </div>

        {/* Diff View Area */}
        <div className="flex-1 flex flex-col overflow-hidden bg-[#030712]">
          {selectedSnap && (
            <div className="shrink-0 px-3 py-1.5 border-b border-slate-700/60 bg-slate-900/80 flex items-center justify-between">
              <span className="text-[10px] text-slate-400 font-mono">
                v{selectedSnap.version} · {selectedSnap.updated_by} ({new Date(selectedSnap.updated_at).toLocaleDateString()})
              </span>
              <button
                onClick={restoreVersion}
                className="bg-emerald-600 hover:bg-emerald-500 text-white text-[9px] font-bold px-2 py-0.5 rounded cursor-pointer"
              >
                Restore this version
              </button>
            </div>
          )}
          <div className="flex-1 overflow-y-auto p-4 font-mono text-xs leading-relaxed select-text space-y-px">
            {diffLines.map((line, idx) => (
              <div
                key={idx}
                className={cn(
                  'flex gap-2 px-2 py-0.5 rounded-sm whitespace-pre-wrap',
                  line.type === 'added'
                    ? 'bg-emerald-950/40 text-emerald-300 border-l border-emerald-500/80'
                    : line.type === 'removed'
                    ? 'bg-rose-950/40 text-rose-300 border-l border-rose-500/80 line-through'
                    : 'text-slate-400'
                )}
              >
                <span className="w-3 shrink-0 text-slate-600 text-[9px] select-none text-right">
                  {line.type === 'added' ? '+' : line.type === 'removed' ? '-' : ' '}
                </span>
                <span>{line.value || ' '}</span>
              </div>
            ))}
          </div>

          {/* Time Scrubber Slider */}
          {versions.length > 1 && (
            <div className="shrink-0 p-3 border-t border-slate-800 bg-slate-900/40 flex flex-col gap-1">
              <div className="flex justify-between text-[8px] font-mono text-slate-500">
                <span>v{versions[versions.length - 1].version} (inicial)</span>
                <span className="text-indigo-400 font-bold">Línea de tiempo: v{versions[selectedVersionIdx ?? 0]?.version}</span>
                <span>v{versions[0].version} (actual)</span>
              </div>
              <input
                type="range"
                min={0}
                max={versions.length - 1}
                value={versions.length - 1 - (selectedVersionIdx ?? 0)}
                onChange={e => {
                  const val = Number(e.target.value);
                  const idx = versions.length - 1 - val;
                  setSelectedVersionIdx(idx);
                  loadVersionContent(selectedDocId, versions[idx].version);
                }}
                className="w-full accent-indigo-500 cursor-pointer h-1 rounded"
              />
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full overflow-hidden">
      {/* Doc list sidebar */}
      <div className="w-36 shrink-0 border-r border-slate-700/60 bg-slate-900/60 flex flex-col">
        <div className="px-2 py-2 border-b border-slate-700/60 flex items-center justify-between">
          <span className="text-[9px] font-bold text-slate-500 uppercase tracking-wider">Documentos</span>
          <button onClick={() => setCreating(p => !p)} className="text-slate-500 hover:text-indigo-400 text-lg leading-none cursor-pointer" title="New doc">+</button>
        </div>
        {creating && (
          <div className="px-2 py-1.5 border-b border-slate-700/60 flex gap-1">
            <input autoFocus value={newTitle} onChange={e => setNewTitle(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && createDoc()}
              placeholder="Título..." className="flex-1 bg-slate-800 border border-indigo-500/40 rounded px-1.5 py-1 text-[10px] text-slate-200 outline-none min-w-0" />
            <button onClick={createDoc} className="text-[9px] bg-indigo-600 text-white px-1.5 rounded cursor-pointer">✓</button>
          </div>
        )}
        <div className="flex-1 overflow-y-auto">
          {docs.map(d => (
            <div key={d.doc_id}
              className={cn('group flex items-center border-b border-slate-800/60 transition-colors',
                selectedDocId === d.doc_id ? 'bg-indigo-950/60' : 'hover:bg-slate-800/60')}>
              <button onClick={() => selectDoc(d.doc_id)}
                className={cn('flex-1 text-left px-2 py-2 text-[10px] cursor-pointer truncate',
                  selectedDocId === d.doc_id ? 'text-indigo-300' : 'text-slate-400')}>
                {d.title || 'Untitled'}
              </button>
              <button onClick={() => deleteDoc(d.doc_id, d.title)}
                className="opacity-0 group-hover:opacity-100 pr-1.5 text-slate-600 hover:text-rose-400 cursor-pointer text-xs leading-none shrink-0 transition-opacity"
                title="Delete document">×</button>
            </div>
          ))}
          {docs.length === 0 && !creating && (
            <p className="text-[10px] text-slate-600 p-3 text-center">No documents.<br />Press + to create one.</p>
          )}
        </div>
      </div>

      {/* Editor */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {selectedDocId ? (
          <>
            <div className="shrink-0 px-3 py-1.5 border-b border-slate-700/60 bg-slate-900/80 flex items-center gap-2">
              <input value={title} onChange={e => onTitleChange(e.target.value)}
                className="flex-1 bg-transparent text-xs font-bold text-slate-200 outline-none placeholder-slate-600 min-w-0"
                placeholder="Document title..." />
              <span className={cn('text-[9px] font-mono shrink-0', STATUS_COLOR[saveStatus])}>{STATUS_LABEL[saveStatus]}</span>
              {lastEditor && saveStatus === 'saved' && (
                <span className="text-[9px] text-slate-600 shrink-0">by {lastEditor}</span>
              )}
              {saveStatus === 'conflict' && (
                <button onClick={() => loadDoc(selectedDocId, true)} className="text-[9px] text-rose-400 hover:text-rose-300 cursor-pointer shrink-0">reload</button>
              )}
              <button
                onClick={() => {
                  setShowHistory(true);
                  loadVersions(selectedDocId);
                }}
                className="text-[9px] text-slate-600 hover:text-slate-300 flex items-center gap-1 cursor-pointer shrink-0"
                title="Version history"
              >
                <History className="w-3.5 h-3.5" />
              </button>
              <button onClick={() => navigator.clipboard.writeText(content)} className="text-[9px] text-slate-600 hover:text-slate-300 flex items-center gap-1 cursor-pointer shrink-0">
                <Copy className="w-3 h-3" />
              </button>
            </div>
            <textarea value={content} onChange={e => onContentChange(e.target.value)} spellCheck={false}
              className="flex-1 resize-none bg-[#06080d] text-slate-200 font-mono text-xs p-4 outline-none border-none"
              placeholder="Empieza a escribir... Los cambios se guardan solos y todos los agentes ven el mismo documento." />
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center">
            <p className="text-[11px] text-slate-600 text-center">Selecciona o crea un documento<br />para empezar a colaborar.</p>
          </div>
        )}
      </div>
    </div>
  );
}


// ── WHITEBOARD TAB ────────────────────────────────────────────────────────────
function WhiteboardTab() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const drawing = useRef(false);
  const lastPos = useRef<{ x: number; y: number } | null>(null);
  const [color, setColor] = useState('#60a5fa');
  const [size, setSize] = useState(3);
  const COLORS = ['#60a5fa', '#34d399', '#f59e0b', '#f87171', '#a78bfa', '#ffffff'];

  const getPos = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const r = canvasRef.current!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  const onDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    drawing.current = true;
    lastPos.current = getPos(e);
  };

  const onMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!drawing.current || !canvasRef.current) return;
    const ctx = canvasRef.current.getContext('2d')!;
    const pos = getPos(e);
    ctx.beginPath();
    ctx.moveTo(lastPos.current!.x, lastPos.current!.y);
    ctx.lineTo(pos.x, pos.y);
    ctx.strokeStyle = color;
    ctx.lineWidth = size;
    ctx.lineCap = 'round';
    ctx.stroke();
    lastPos.current = pos;
  };

  const onUp = () => { drawing.current = false; };

  const clear = () => {
    if (!canvasRef.current) return;
    const ctx = canvasRef.current.getContext('2d')!;
    ctx.clearRect(0, 0, canvasRef.current.width, canvasRef.current.height);
  };

  useEffect(() => {
    const resize = () => {
      if (!canvasRef.current) return;
      const { width, height } = canvasRef.current.parentElement!.getBoundingClientRect();
      canvasRef.current.width = width;
      canvasRef.current.height = height;
    };
    resize();
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="shrink-0 px-3 py-1.5 border-b border-slate-700/60 bg-slate-900/80 flex items-center gap-2 flex-wrap">
        <PenTool className="w-3.5 h-3.5 text-violet-400" />
        <span className="text-[10px] font-bold text-slate-400 uppercase tracking-widest">Pizarra</span>
        <div className="flex gap-1 ml-2">
          {COLORS.map(c => (
            <button
              key={c}
              onClick={() => setColor(c)}
              className={cn('w-4 h-4 rounded-full border-2 cursor-pointer transition-transform', color === c ? 'border-white scale-125' : 'border-transparent')}
              style={{ background: c }}
            />
          ))}
        </div>
        <select
          value={size}
          onChange={e => setSize(Number(e.target.value))}
          className="ml-1 bg-slate-800 border border-slate-700 text-slate-300 text-[10px] rounded px-1 py-0.5 cursor-pointer"
        >
          {[2, 4, 8, 16].map(s => <option key={s} value={s}>{s}px</option>)}
        </select>
        <button onClick={clear} className="ml-auto text-[10px] text-slate-500 hover:text-rose-400 flex items-center gap-1 cursor-pointer">
          <Trash className="w-3 h-3" /> clear
        </button>
      </div>
      <div className="flex-1 relative bg-[#06080d]">
        <canvas
          ref={canvasRef}
          className="absolute inset-0 cursor-crosshair"
          onMouseDown={onDown}
          onMouseMove={onMove}
          onMouseUp={onUp}
          onMouseLeave={onUp}
        />
      </div>
    </div>
  );
}

// ── KNOWLEDGE TAB (upgraded Canvas Multitarea) ───────────────────────────────
const getNodeDims = (type: string): [number, number] => {
  if (type.startsWith('sticky-')) return [120, 120];
  if (type === 'text-box') return [140, 40];
  return [150, 50];
};

function KnowledgeTab({ channelId }: { channelId: string }) {
  const [nodes, setNodes] = useState<CanvasNode[]>([
    { id: '1', label: 'SilvaDB Core', x: 250, y: 150, type: 'concept' },
    { id: '2', label: 'Hybrid Routing', x: 120, y: 320, type: 'task' },
    { id: '3', label: 'Letta Agent Buffer', x: 380, y: 320, type: 'episode' },
  ]);
  const [edges, setEdges] = useState<CanvasEdge[]>([
    { id: 'e1-2', source: '1', target: '2' },
    { id: 'e1-3', source: '1', target: '3' },
  ]);
  const [camera, setCamera] = useState({ x: 0, y: 0, zoom: 1 });
  const [draggedId, setDraggedId] = useState<string | null>(null);
  const [dragOffset, setDragOffset] = useState({ x: 0, y: 0 });
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [newNodeLabel, setNewNodeLabel] = useState('');
  const [newNodeType, setNewNodeType] = useState<CanvasNode['type']>('concept');
  const [connectSourceId, setConnectSourceId] = useState<string | null>(null);
  const [wsStatus, setWsStatus] = useState<'connecting' | 'connected' | 'error'>('connecting');
  const [isPanning, setIsPanning] = useState(false);
  const [panStart, setPanStart] = useState({ x: 0, y: 0 });
  const containerRef = useRef<HTMLDivElement>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const nodesRef = useRef(nodes);
  const edgesRef = useRef(edges);

  // Editing state for inline text editing
  const [editingNodeId, setEditingNodeId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState('');

  useEffect(() => { nodesRef.current = nodes; }, [nodes]);
  useEffect(() => { edgesRef.current = edges; }, [edges]);

  useEffect(() => {
    const wsHost = window.location.port === '5173' ? `${window.location.hostname}:3030`
      : window.location.port === '5174' ? `${window.location.hostname}:3033`
      : window.location.host;
    const wsUrl = `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${wsHost}/api/v1/canvas/ws`;
    setWsStatus('connecting');
    const ws = new WebSocket(wsUrl);
    wsRef.current = ws;
    ws.onopen = () => { setWsStatus('connected'); ws.send(JSON.stringify({ type: 'request_sync', channelId })); };
    ws.onmessage = (event) => {
      try {
        if (typeof event.data !== 'string') return;
        const msg = JSON.parse(event.data);
        if (msg.channelId !== channelId) return;
        switch (msg.type) {
          case 'sync_response': if (msg.nodes) setNodes(msg.nodes); if (msg.edges) setEdges(msg.edges); break;
          case 'request_sync': ws.send(JSON.stringify({ type: 'sync_response', channelId, nodes: nodesRef.current, edges: edgesRef.current })); break;
          case 'node_moved': setNodes(p => p.map(n => n.id === msg.id ? { ...n, x: msg.x, y: msg.y } : n)); break;
          case 'node_added': setNodes(p => p.some(n => n.id === msg.node.id) ? p.map(n => n.id === msg.node.id ? msg.node : n) : [...p, msg.node]); break;
          case 'edge_added': setEdges(p => p.some(e => e.id === msg.edge.id) ? p : [...p, msg.edge]); break;
          case 'node_deleted': setNodes(p => p.filter(n => n.id !== msg.id)); setEdges(p => p.filter(e => e.source !== msg.id && e.target !== msg.id)); break;
        }
      } catch { /* ignore */ }
    };
    ws.onerror = () => setWsStatus('error');
    ws.onclose = () => setWsStatus('connecting');
    return () => ws.close();
  }, [channelId]);

  const send = (payload: object) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) wsRef.current.send(JSON.stringify({ ...payload, channelId }));
  };

  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const f = 1.1;
    setCamera(p => ({ ...p, zoom: e.deltaY < 0 ? Math.min(p.zoom * f, 3) : Math.max(p.zoom / f, 0.3) }));
  };

  const handleMouseDown = (e: React.MouseEvent, node?: CanvasNode) => {
    if (node) {
      e.stopPropagation();
      setDraggedId(node.id);
      setSelectedNodeId(node.id);
      setDragOffset({ x: (e.clientX / camera.zoom) - node.x, y: (e.clientY / camera.zoom) - node.y });
    } else {
      setIsPanning(true);
      setPanStart({ x: e.clientX - camera.x, y: e.clientY - camera.y });
    }
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (draggedId) {
      const x = Math.round((e.clientX / camera.zoom) - dragOffset.x);
      const y = Math.round((e.clientY / camera.zoom) - dragOffset.y);
      setNodes(p => p.map(n => n.id === draggedId ? { ...n, x, y } : n));
      send({ type: 'node_moved', id: draggedId, x, y });
    } else if (isPanning) {
      setCamera(p => ({ ...p, x: e.clientX - panStart.x, y: e.clientY - panStart.y }));
    }
  };

  const addNode = () => {
    if (!newNodeLabel.trim()) return;
    const node: CanvasNode = {
      id: Date.now().toString(), label: newNodeLabel.trim(),
      x: Math.round(-camera.x / camera.zoom + 200 + Math.random() * 100),
      y: Math.round(-camera.y / camera.zoom + 200 + Math.random() * 100),
      type: newNodeType
    };
    setNodes(p => [...p, node]);
    send({ type: 'node_added', node });
    setNewNodeLabel('');
  };

  const deleteNode = (id: string) => {
    setNodes(p => p.filter(n => n.id !== id));
    setEdges(p => p.filter(e => e.source !== id && e.target !== id));
    send({ type: 'node_deleted', id });
    if (selectedNodeId === id) setSelectedNodeId(null);
  };

  const createEdge = (targetId: string) => {
    if (!connectSourceId || connectSourceId === targetId) return;
    const edge: CanvasEdge = { id: `e-${connectSourceId}-${targetId}`, source: connectSourceId, target: targetId };
    setEdges(p => [...p, edge]);
    send({ type: 'edge_added', edge });
    setConnectSourceId(null);
  };

  const finishEditing = (node: CanvasNode) => {
    setEditingNodeId(null);
    const trimmed = editingText.trim();
    if (trimmed === node.label) return;
    const updated = { ...node, label: trimmed };
    setNodes(p => p.map(n => n.id === node.id ? updated : n));
    send({ type: 'node_added', node: updated });
  };

  const NODE_THEMES: Record<string, { color: string; border: string; bg: string }> = {
    concept: { color: 'text-sky-300', border: 'border-sky-500/40', bg: 'bg-sky-950/40' },
    task: { color: 'text-amber-300', border: 'border-amber-500/40', bg: 'bg-amber-950/40' },
    episode: { color: 'text-violet-300', border: 'border-violet-500/40', bg: 'bg-violet-950/40' },
    agent: { color: 'text-emerald-300', border: 'border-emerald-500/40', bg: 'bg-emerald-950/40' },
    'sticky-yellow': { color: 'text-yellow-950', border: 'border-yellow-300/80', bg: 'bg-yellow-200/95' },
    'sticky-green': { color: 'text-emerald-950', border: 'border-emerald-300/80', bg: 'bg-emerald-200/95' },
    'sticky-pink': { color: 'text-pink-950', border: 'border-pink-300/80', bg: 'bg-pink-200/95' },
    'sticky-blue': { color: 'text-blue-950', border: 'border-blue-300/80', bg: 'bg-blue-200/95' },
    'text-box': { color: 'text-slate-200', border: 'border-transparent', bg: 'bg-transparent' },
  };

  return (
    <div className="flex flex-col h-full overflow-hidden select-none relative bg-[#06080d]">
      <div className="flex items-center gap-3 px-3 py-1.5 border-b border-slate-700/60 bg-slate-900/60 shrink-0">
        <span className="text-[10px] font-bold text-slate-400 uppercase tracking-widest flex items-center gap-1.5">
          <Network className="w-3 h-3 text-indigo-400" /> Canvas Multitarea
        </span>
        <div className="flex items-center gap-1.5 ml-auto">
          <span className={cn('w-2 h-2 rounded-full', wsStatus === 'connected' ? 'bg-emerald-500' : wsStatus === 'connecting' ? 'bg-amber-500 animate-pulse' : 'bg-rose-500')} />
          <span className="text-[9px] text-slate-500 font-mono">{wsStatus}</span>
        </div>
      </div>
      <div
        ref={containerRef}
        onWheel={handleWheel}
        onMouseMove={handleMouseMove}
        onMouseDown={e => handleMouseDown(e)}
        onMouseUp={() => { setDraggedId(null); setIsPanning(false); }}
        onMouseLeave={() => { setDraggedId(null); setIsPanning(false); }}
        className={cn('flex-1 relative overflow-hidden', isPanning ? 'cursor-grabbing' : 'cursor-grab')}
        style={{
          backgroundImage: `radial-gradient(circle, #1e293b 1px, transparent 1px)`,
          backgroundSize: `${20 * camera.zoom}px ${20 * camera.zoom}px`,
          backgroundPosition: `${camera.x}px ${camera.y}px`,
        }}
      >
        <div style={{ transform: `translate(${camera.x}px,${camera.y}px) scale(${camera.zoom})`, transformOrigin: '0 0', position: 'absolute', inset: 0, pointerEvents: 'none' }}>
          <svg className="absolute inset-0 w-full h-full z-0 overflow-visible">
            <defs><marker id="arr" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#3b82f6" opacity="0.7" /></marker></defs>
            {edges.map(edge => {
              const s = nodes.find(n => n.id === edge.source);
              const t = nodes.find(n => n.id === edge.target);
              if (!s || !t) return null;
              const [w1, h1] = getNodeDims(s.type);
              const [w2, h2] = getNodeDims(t.type);
              const x1 = s.x + w1 / 2, y1 = s.y + h1 / 2, x2 = t.x + w2 / 2, y2 = t.y + h2 / 2;
              const dx = x2 - x1, dy = y2 - y1;
              const len = Math.sqrt(dx * dx + dy * dy);
              if (len === 0) return null;
              const r1 = 0.5 * Math.min(w1 / Math.abs(dx || 1), h1 / Math.abs(dy || 1));
              const r2 = 0.5 * Math.min(w2 / Math.abs(dx || 1), h2 / Math.abs(dy || 1));
              return <path key={edge.id} d={`M ${x1 + dx * r1} ${y1 + dy * r1} L ${x2 - dx * r2} ${y2 - dy * r2}`} stroke="#3b82f6" strokeWidth="2" strokeOpacity="0.6" fill="none" markerEnd="url(#arr)" />;
            })}
          </svg>
          {nodes.map(node => {
            const th = NODE_THEMES[node.type] || NODE_THEMES['concept'];
            const sel = selectedNodeId === node.id;
            const conn = connectSourceId === node.id;
            const [w, h] = getNodeDims(node.type);
            const isSticky = node.type.startsWith('sticky-');
            const isTextBox = node.type === 'text-box';

            return (
              <div
                key={node.id}
                onMouseDown={e => handleMouseDown(e, node)}
                onDoubleClick={e => {
                  e.stopPropagation();
                  setEditingNodeId(node.id);
                  setEditingText(node.label);
                }}
                className={cn(
                  'absolute px-3 py-2 border rounded-xl shadow-xl flex flex-col justify-center items-center backdrop-blur-md z-10 pointer-events-auto group text-slate-100',
                  th.bg,
                  th.border,
                  sel ? 'ring-2 ring-indigo-500' : '',
                  isSticky ? 'rounded-none shadow-yellow-950/20 aspect-square' : '',
                  isTextBox ? 'border-transparent shadow-none rounded-none' : ''
                )}
                style={{ left: node.x, top: node.y, width: `${w}px`, height: `${h}px` }}
              >
                <div className="absolute top-1 right-2 opacity-0 group-hover:opacity-100 z-20">
                  <button onClick={e => { e.stopPropagation(); deleteNode(node.id); }} className={cn('text-[12px] font-bold leading-none cursor-pointer', isSticky ? 'text-slate-600 hover:text-slate-900' : 'text-rose-400 hover:text-rose-300')}>×</button>
                </div>
                
                {!isSticky && !isTextBox && (
                  <div className={cn('text-[8px] font-bold uppercase tracking-widest mb-0.5 select-none shrink-0', th.color)}>{node.type}</div>
                )}

                {editingNodeId === node.id ? (
                  <textarea
                    autoFocus
                    value={editingText}
                    onChange={e => setEditingText(e.target.value)}
                    onBlur={() => finishEditing(node)}
                    onKeyDown={e => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        finishEditing(node);
                      }
                    }}
                    className="w-full h-full bg-slate-900/90 text-slate-100 text-[10px] p-1 border border-indigo-500 rounded outline-none resize-none font-mono"
                  />
                ) : (
                  <div className={cn(
                    'text-[10px] font-semibold text-center leading-normal break-words overflow-y-auto max-h-full py-0.5 select-text w-full',
                    isSticky ? 'text-slate-800' : 'text-slate-100'
                  )}>
                    {node.label}
                  </div>
                )}

                {!isTextBox && (
                  <div className="flex gap-1 mt-1.5 opacity-0 group-hover:opacity-100 transition-opacity z-20 shrink-0">
                    <button onClick={e => { e.stopPropagation(); setConnectSourceId(conn ? null : node.id); }}
                      className={cn('text-[8px] px-1.5 py-0.5 rounded border cursor-pointer', conn ? 'bg-indigo-950 border-indigo-500 text-indigo-300' : 'bg-slate-800 border-slate-700 text-slate-400')}>
                      {conn ? 'unir...' : 'conectar'}
                    </button>
                    {connectSourceId && connectSourceId !== node.id && (
                      <button onClick={e => { e.stopPropagation(); createEdge(node.id); }} className="text-[8px] px-1.5 py-0.5 rounded bg-emerald-950 border border-emerald-500 text-emerald-300 cursor-pointer">vincular</button>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
        {/* Floating add-node panel */}
        <div className="absolute top-3 left-3 bg-slate-900/90 border border-slate-700/80 p-3 rounded-xl shadow-xl w-52 z-20 flex flex-col gap-2 backdrop-blur-sm">
          <div className="flex items-center justify-between">
            <span className="text-[9px] font-bold text-slate-500 uppercase tracking-wider">Add node</span>
            <button onClick={() => setCamera({ x: 0, y: 0, zoom: 1 })} className="text-slate-500 hover:text-slate-200 cursor-pointer"><Maximize2 className="w-3 h-3" /></button>
          </div>
          <input type="text" placeholder="Contenido o etiqueta..." value={newNodeLabel} onChange={e => setNewNodeLabel(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && addNode()}
            className="bg-slate-800 border border-slate-700 rounded-lg px-2 py-1 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500 w-full" />
          
          <div className="flex flex-col gap-1">
            <span className="text-[8px] font-bold text-slate-500 uppercase tracking-wider">Tipo de elemento</span>
            <select
              value={newNodeType}
              onChange={e => setNewNodeType(e.target.value as any)}
              className="bg-slate-800 border border-slate-700 rounded-lg px-2 py-1 text-xs text-slate-200 focus:outline-none focus:border-indigo-500 w-full cursor-pointer"
            >
              <optgroup label="Grafo">
                <option value="concept">Concepto</option>
                <option value="task">Tarea</option>
                <option value="episode">Episodio</option>
                <option value="agent">Agente</option>
              </optgroup>
              <optgroup label="Notas Adhesivas">
                <option value="sticky-yellow">Nota Amarilla</option>
                <option value="sticky-green">Nota Verde</option>
                <option value="sticky-pink">Nota Rosa</option>
                <option value="sticky-blue">Nota Azul</option>
              </optgroup>
              <optgroup label="Otros">
                <option value="text-box">Caja de texto</option>
              </optgroup>
            </select>
          </div>

          <button onClick={addNode} disabled={!newNodeLabel.trim()}
            className="py-1.5 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-40 text-white text-xs font-bold rounded-lg cursor-pointer">
            Añadir
          </button>
        </div>
      </div>
    </div>
  );
}

// ── MAIN WORKSPACE ────────────────────────────────────────────────────────────
interface ColoquioCanvasWorkspaceProps {
  channelId: string;
  messages: ColoquioMessage[];
}

const TABS: { id: Tab; label: string; icon: React.ReactNode; badge?: (msgs: ColoquioMessage[]) => number }[] = [
  { id: 'preview', label: 'Preview', icon: <Monitor className="w-3 h-3" />, badge: msgs => extractCodeBlocks(msgs).length },
  { id: 'docs', label: 'Docs', icon: <FileText className="w-3 h-3" /> },
  { id: 'whiteboard', label: 'Pizarra', icon: <PenTool className="w-3 h-3" /> },
  { id: 'knowledge', label: 'Knowledge', icon: <Network className="w-3 h-3" /> },
];

export function ColoquioCanvasWorkspace({ channelId, messages }: ColoquioCanvasWorkspaceProps) {
  const [activeTab, setActiveTab] = useState<Tab>('preview');

  return (
    <div className="flex flex-col h-full overflow-hidden bg-[#06080d]">
      {/* Tab bar */}
      <div className="flex items-center shrink-0 border-b border-slate-700/60 bg-slate-900/80 px-2 gap-0.5">
        {TABS.map(tab => {
          const badge = tab.badge ? tab.badge(messages) : 0;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={cn(
                'flex items-center gap-1.5 px-3 py-2 text-[10px] font-bold uppercase tracking-widest transition-colors cursor-pointer border-b-2 -mb-px',
                activeTab === tab.id
                  ? 'border-indigo-500 text-indigo-300'
                  : 'border-transparent text-slate-500 hover:text-slate-300'
              )}
            >
              {tab.icon}
              {tab.label}
              {badge > 0 && (
                <span className="bg-indigo-600 text-white text-[8px] font-bold px-1 rounded-full leading-none py-0.5">
                  {badge}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-hidden">
        {activeTab === 'preview' && <PreviewTab messages={messages} />}
        {activeTab === 'docs' && <DocsTab />}
        {activeTab === 'whiteboard' && <WhiteboardTab />}
        {activeTab === 'knowledge' && <KnowledgeTab channelId={channelId} />}
      </div>
    </div>
  );
}
