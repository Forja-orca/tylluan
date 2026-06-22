import { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import { 
  Network, Plus, Trash, Maximize2, RotateCcw, Sparkles,
  Play, FileText, Palette, RefreshCw, Eye, ExternalLink, BookOpen, Layers, Code,
  Save, Edit, Lock, Users, PlusCircle, AlertTriangle, RefreshCw as RefreshIcon
} from 'lucide-react';
import { cn } from '../lib/utils';
import { CanvasNode, CanvasEdge, ColoquioMessage } from './coloquio-types';
import { NexusBridge } from '../lib/nexus-bridge';

interface CollabDoc {
  doc_id: string;
  title: string;
  content: string;
  created_by: string;
  updated_by: string;
  version: number;
  created_at: number;
  updated_at: number;
}

interface ColoquioGraphPanelProps {
  channelId: string;
  messages: ColoquioMessage[];
  bridge: NexusBridge | null;
}

type TabType = 'preview' | 'docs' | 'whiteboard' | 'knowledge';

export function ColoquioGraphPanel({ channelId, messages = [], bridge }: ColoquioGraphPanelProps) {
  const [activeTab, setActiveTab] = useState<TabType>('preview');
  
  // ─── COLLABORATIVE DOCS STATE ─────────────────────────────────────────────
  const [docs, setDocs] = useState<CollabDoc[]>([]);
  const [currentDocId, setCurrentDocId] = useState<string | null>(null);
  const [editingDoc, setEditingDoc] = useState(false);
  const [docContent, setDocContent] = useState('');
  const [docTitle, setDocTitle] = useState('');
  const [docVersion, setDocVersion] = useState<number>(0);
  const [docsLoading, setDocsLoading] = useState(true);
  const [newDocTitle, setNewDocTitle] = useState('');
  const [showNewDocInput, setShowNewDocInput] = useState(false);
  const [conflictDoc, setConflictDoc] = useState<string | null>(null);
  const saveTimerRef = useRef<number | null>(null);
  const lastSavedContentRef = useRef('');
  const signalRef = useRef<EventSource | null>(null);
  
  const activeDoc = docs.find(d => d.doc_id === currentDocId) || null;

  // Fetch documents list
  const fetchDocs = useCallback(async () => {
    if (!bridge) return;
    try {
      const data = await bridge.fetchRaw('/api/v1/coloquio/documents');
      if (data?.documents) {
        setDocs(data.documents);
        if (!currentDocId && data.documents.length > 0) {
          setCurrentDocId(data.documents[0].doc_id);
          setDocContent(data.documents[0].content);
          setDocTitle(data.documents[0].title);
          setDocVersion(data.documents[0].version ?? 0);
        }
      }
    } catch (e) {
      console.warn('Failed to fetch docs:', e);
    } finally {
      setDocsLoading(false);
    }
  }, [bridge, currentDocId]);

  useEffect(() => { fetchDocs(); }, [fetchDocs]);

  // When currentDocId changes, load that doc content
  const selectDoc = useCallback(async (docId: string) => {
    if (!bridge) return;
    setEditingDoc(false);
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    try {
      const data = await bridge.fetchRaw(`/api/v1/coloquio/documents/${docId}`);
      if (data?.document) {
        setCurrentDocId(docId);
        setDocContent(data.document.content);
        setDocTitle(data.document.title);
        setDocVersion(data.document.version ?? 0);
        lastSavedContentRef.current = data.document.content;
      }
    } catch (e) {
      console.warn('Failed to fetch doc:', e);
    }
  }, [bridge]);

  // Auto-save with debounce — sends version for optimistic locking
  const saveDoc = useCallback(async (content: string, title: string) => {
    if (!bridge || !currentDocId) return;
    if (content === lastSavedContentRef.current) return;
    try {
      const res = await bridge.fetchRaw(`/api/v1/coloquio/documents/${currentDocId}`, {
        method: 'PUT',
        body: JSON.stringify({ title, content, updated_by: 'system', expected_version: docVersion })
      });
      if (res?.document) {
        setDocVersion(res.document.version ?? docVersion + 1);
        lastSavedContentRef.current = content;
      }
    } catch (e: any) {
      if (e?.status === 409 || e?.response?.status === 409 || (typeof e === 'object' && e?.error === 'version_conflict')) {
        setConflictDoc(currentDocId);
      } else {
        console.warn('Failed to save doc:', e);
      }
    }
  }, [bridge, currentDocId, docVersion]);

  const resolveConflict = useCallback(async () => {
    if (!bridge || !currentDocId || !conflictDoc) return;
    setConflictDoc(null);
    // Fetch latest version and replace local state
    const data = await bridge.fetchRaw(`/api/v1/coloquio/documents/${currentDocId}`);
    if (data?.document) {
      setDocContent(data.document.content);
      setDocTitle(data.document.title);
      setDocVersion(data.document.version ?? 0);
      lastSavedContentRef.current = data.document.content;
      setEditingDoc(true); // re-open editor so user sees the merged state
    }
  }, [bridge, currentDocId, conflictDoc]);

  const handleDocContentChange = (value: string) => {
    setDocContent(value);
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = window.setTimeout(() => {
      saveDoc(value, docTitle);
    }, 2000);
  };

  const handleDocTitleChange = (value: string) => {
    setDocTitle(value);
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = window.setTimeout(() => {
      saveDoc(docContent, value);
    }, 2000);
  };

  const createNewDoc = useCallback(async () => {
    if (!bridge || !newDocTitle.trim()) return;
    try {
      const data = await bridge.fetchRaw('/api/v1/coloquio/documents', {
        method: 'POST',
        body: JSON.stringify({ title: newDocTitle.trim(), created_by: 'user' })
      });
      if (data?.document) {
        setDocs(prev => [data.document, ...prev]);
        setCurrentDocId(data.document.doc_id);
        setDocContent(data.document.content);
        setDocTitle(data.document.title);
        setNewDocTitle('');
        setShowNewDocInput(false);
        setEditingDoc(true);
        lastSavedContentRef.current = data.document.content;
      }
    } catch (e) {
      console.warn('Failed to create doc:', e);
    }
  }, [bridge, newDocTitle]);

  // Listen for SSE doc:updated events from other agents
  useEffect(() => {
    if (!bridge) return;
    const es = new EventSource('/api/v1/events');
    signalRef.current = es;
    es.addEventListener('doc:updated', (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data);
        if (data.doc_id === currentDocId && data.updated_by !== 'user' && !editingDoc) {
          // Another agent saved — refresh content if we're not editing
          fetchDocs();
        }
        // Update list badge counts
        if (data.doc_id) {
          setDocs(prev => prev.map(d => 
            d.doc_id === data.doc_id 
              ? { ...d, version: data.version ?? d.version, updated_by: data.updated_by ?? d.updated_by }
              : d
          ));
        }
      } catch {}
    });
    es.addEventListener('doc:created', (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data);
        if (data.doc_id) fetchDocs();
      } catch {}
    });
    return () => { es.close(); signalRef.current = null; };
  }, [bridge, currentDocId, editingDoc, fetchDocs]);

  // ─── STATE FOR GRAPH / WHITEBOARD ──────────────────────────────────────────
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

  // Viewport panning
  const [isPanning, setIsPanning] = useState(false);
  const [panStart, setPanStart] = useState({ x: 0, y: 0 });

  const containerRef = useRef<HTMLDivElement>(null);
  const wsRef = useRef<WebSocket | null>(null);

  // Maintain refs for values to prevent stale WebSocket closures and redundant reconnections
  const nodesRef = useRef(nodes);
  const edgesRef = useRef(edges);

  useEffect(() => { nodesRef.current = nodes; }, [nodes]);
  useEffect(() => { edgesRef.current = edges; }, [edges]);

  // Setup WebSocket connection (depends only on channelId)
  useEffect(() => {
    const wsHost = window.location.port === '5173' ? `${window.location.hostname}:3030` : window.location.host;
    const wsUrl = `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${wsHost}/api/v1/canvas/ws`;

    setWsStatus('connecting');
    const ws = new WebSocket(wsUrl);
    wsRef.current = ws;

    ws.onopen = () => {
      setWsStatus('connected');
      ws.send(JSON.stringify({ type: 'request_sync', channelId }));
    };

    ws.onmessage = (event) => {
      try {
        if (typeof event.data === 'string') {
          const msg = JSON.parse(event.data);
          if (msg.channelId !== channelId) return;

          switch (msg.type) {
            case 'sync_response':
              if (msg.nodes) setNodes(msg.nodes);
              if (msg.edges) setEdges(msg.edges);
              break;
            case 'request_sync':
              ws.send(JSON.stringify({
                type: 'sync_response',
                channelId,
                nodes: nodesRef.current,
                edges: edgesRef.current
              }));
              break;
            case 'node_moved':
              setNodes(prev => prev.map(n => n.id === msg.id ? { ...n, x: msg.x, y: msg.y } : n));
              break;
            case 'node_added':
              setNodes(prev => prev.some(n => n.id === msg.node.id) ? prev : [...prev, msg.node]);
              break;
            case 'edge_added':
              setEdges(prev => prev.some(e => e.id === msg.edge.id) ? prev : [...prev, msg.edge]);
              break;
            case 'node_deleted':
              setNodes(prev => prev.filter(n => n.id !== msg.id));
              setEdges(prev => prev.filter(e => e.source !== msg.id && e.target !== msg.id));
              break;
            default:
              break;
          }
        }
      } catch (err) {
        console.warn('Canvas socket parse warning:', err);
      }
    };

    ws.onerror = () => { setWsStatus('error'); };
    ws.onclose = () => { setWsStatus('connecting'); };

    return () => { ws.close(); };
  }, [channelId]);

  const sendWsUpdate = (payload: any) => {
    if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ ...payload, channelId }));
    }
  };

  // ─── EXTRACT HTML ARTIFACTS FOR PREVIEW ───────────────────────────────────
  const htmlArtifacts = useMemo(() => {
    const list: { turn: number; author: string; code: string; title: string }[] = [];
    messages.forEach(msg => {
      // Look for code blocks of type html
      const regex = /```html\n([\s\S]*?)\n```/gi;
      let match;
      while ((match = regex.exec(msg.content)) !== null) {
        const code = match[1];
        const lines = msg.content.split('\n');
        const titleLine = lines.find(l => l.startsWith('#') || l.startsWith('**')) || '';
        const cleanTitle = titleLine.replace(/[#*\[\]]/g, '').trim() || `Turno #${msg.turn}`;
        list.push({
          turn: msg.turn,
          author: msg.author_id,
          code,
          title: cleanTitle
        });
      }
      
      // Also capture inline document structures starting with doctype
      if (msg.content.includes("<!DOCTYPE html>") || msg.content.includes("<html")) {
        if (!msg.content.includes("```html")) {
          list.push({
            turn: msg.turn,
            author: msg.author_id,
            code: msg.content,
            title: `Documento Completo T#${msg.turn}`
          });
        }
      }
    });
    return list.reverse(); // Newest first
  }, [messages]);

  const [selectedArtifactIndex, setSelectedArtifactIndex] = useState<number>(0);
  const currentArtifact = htmlArtifacts[selectedArtifactIndex] || null;
  const [iframeKey, setIframeKey] = useState(0);

  const handleReloadIframe = () => setIframeKey(k => k + 1);

  // ─── EXTRACT MARKDOWN DOCUMENTS ──────────────────────────────────────────
  const markdownDocs = useMemo(() => {
    const list: { turn: number; author: string; content: string; title: string }[] = [];
    messages.forEach(msg => {
      // Exclude messages that are mostly code blocks to keep text readable
      const hasHeading = msg.content.includes("##") || msg.content.includes("###");
      const hasPlan = msg.content.toLowerCase().includes("[plan]") || msg.content.toLowerCase().includes("[cierre");
      if ((hasHeading || hasPlan) && !msg.content.includes("```html")) {
        const lines = msg.content.split('\n');
        const titleLine = lines.find(l => l.startsWith('#') || l.startsWith('**')) || '';
        const cleanTitle = titleLine.replace(/[#*\[\]]/g, '').trim() || `Documento T#${msg.turn}`;
        list.push({
          turn: msg.turn,
          author: msg.author_id,
          content: msg.content,
          title: cleanTitle
        });
      }
    });
    return list.reverse();
  }, [messages]);

  const [selectedDocIndex, setSelectedDocIndex] = useState<number>(0);
  const currentDoc = markdownDocs[selectedDocIndex] || null;

  // Simple MD renderer helper for sandbox
  const renderDocContent = (text: string) => {
    let html = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    html = html.replace(/```(\w*)\n([\s\S]*?)\n```/g, (_, lang, code) =>
      `<pre class="bg-slate-950/90 border border-slate-800 rounded-xl p-3 my-3 font-mono text-[10px] overflow-x-auto text-slate-300">` +
      `<div class="text-[8px] text-slate-600 uppercase tracking-widest font-bold mb-1.5 border-b border-slate-900 pb-1">${lang || 'code'}</div>` +
      `<code>${code}</code></pre>`
    );
    html = html.replace(/`([^`]+)`/g, '<code class="bg-slate-900 border border-slate-800 px-1 py-0.5 rounded font-mono text-[10px] text-indigo-400">$1</code>');
    html = html.replace(/\*\*([^*]+)\*\*/g, '<strong class="text-slate-100">$1</strong>');
    html = html.replace(/###\s*(.*)/g, '<h4 class="text-xs font-bold text-slate-200 mt-4 mb-2">$1</h4>');
    html = html.replace(/##\s*(.*)/g, '<h3 class="text-sm font-bold text-indigo-300 mt-5 mb-2">$1</h3>');
    html = html.replace(/(?:^|\n)&gt;\s?(.*)/g, (_, q) =>
      `<blockquote class="border-l-2 border-indigo-500/30 pl-3 italic text-slate-400 my-2">${q}</blockquote>`
    );
    html = html.replace(/(?:^|\n)[-*]\s(.*)/g, (_, item) =>
      `<li class="list-disc ml-4 my-1 text-slate-300">${item}</li>`
    );
    return { __html: html };
  };

  // ─── CANVAS GRAPH OPERATIONS ──────────────────────────────────────────────
  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const zoomFactor = 1.1;
    let newZoom = camera.zoom;
    if (e.deltaY < 0) {
      newZoom = Math.min(newZoom * zoomFactor, 3);
    } else {
      newZoom = Math.max(newZoom / zoomFactor, 0.3);
    }
    setCamera(prev => ({ ...prev, zoom: newZoom }));
  };

  const handleMouseDown = (e: React.MouseEvent, node?: CanvasNode) => {
    if (node) {
      e.stopPropagation();
      setDraggedId(node.id);
      setSelectedNodeId(node.id);
      setDragOffset({
        x: (e.clientX / camera.zoom) - node.x,
        y: (e.clientY / camera.zoom) - node.y,
      });
    } else {
      setIsPanning(true);
      setPanStart({ x: e.clientX - camera.x, y: e.clientY - camera.y });
    }
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (draggedId) {
      const x = Math.round((e.clientX / camera.zoom) - dragOffset.x);
      const y = Math.round((e.clientY / camera.zoom) - dragOffset.y);
      setNodes(prev => prev.map(n => n.id === draggedId ? { ...n, x, y } : n));
      sendWsUpdate({ type: 'node_moved', id: draggedId, x, y });
    } else if (isPanning) {
      const x = e.clientX - panStart.x;
      const y = e.clientY - panStart.y;
      setCamera(prev => ({ ...prev, x, y }));
    }
  };

  const handleMouseUp = () => {
    setDraggedId(null);
    setIsPanning(false);
  };

  const addNode = () => {
    if (!newNodeLabel.trim()) return;
    const node: CanvasNode = {
      id: Date.now().toString(),
      label: newNodeLabel.trim(),
      x: Math.round(-camera.x / camera.zoom + 200 + Math.random() * 100),
      y: Math.round(-camera.y / camera.zoom + 200 + Math.random() * 100),
      type: newNodeType
    };
    setNodes(prev => [...prev, node]);
    sendWsUpdate({ type: 'node_added', node });
    setNewNodeLabel('');
  };

  const deleteNode = (id: string) => {
    setNodes(prev => prev.filter(n => n.id !== id));
    setEdges(prev => prev.filter(e => e.source !== id && e.target !== id));
    sendWsUpdate({ type: 'node_deleted', id });
    if (selectedNodeId === id) setSelectedNodeId(null);
  };

  const createEdge = (targetId: string) => {
    if (!connectSourceId || connectSourceId === targetId) return;
    const edge: CanvasEdge = {
      id: `e-${connectSourceId}-${targetId}`,
      source: connectSourceId,
      target: targetId,
    };
    setEdges(prev => [...prev, edge]);
    sendWsUpdate({ type: 'edge_added', edge });
    setConnectSourceId(null);
  };

  const resetCamera = () => { setCamera({ x: 0, y: 0, zoom: 1 }); };

  const NODE_THEMES: Record<CanvasNode['type'], { color: string; border: string; bg: string }> = {
    concept: { color: 'text-sky-300', border: 'border-sky-500/40', bg: 'bg-sky-950/40' },
    task: { color: 'text-amber-300', border: 'border-amber-500/40', bg: 'bg-amber-950/40' },
    episode: { color: 'text-violet-300', border: 'border-violet-500/40', bg: 'bg-violet-950/40' },
    agent: { color: 'text-emerald-300', border: 'border-emerald-500/40', bg: 'bg-emerald-950/40' },
  };

  const syncWithSilvaDB = async () => {
    if (!bridge) return;
    try {
      const data = await bridge.fetchRaw('/api/v1/silva/graph', {
        method: 'POST',
        body: JSON.stringify({ command: 'expand', node_id: 'SilvaDB Core', depth: 2 })
      });
      
      const contentStr = data.content?.[0]?.text;
      if (contentStr) {
        const payload = JSON.parse(contentStr);
        if (payload.nodes && payload.edges) {
          const mappedNodes: CanvasNode[] = payload.nodes.map((n: any, idx: number) => ({
            id: n.id,
            label: n.id,
            type: n.type || 'concept',
            x: 100 + (idx % 4) * 160,
            y: 150 + Math.floor(idx / 4) * 120
          }));

          const mappedEdges: CanvasEdge[] = payload.edges.map((e: any) => ({
            id: `e-${e.source}-${e.target}`,
            source: e.source,
            target: e.target
          }));

          setNodes(prev => {
            const next = [...prev];
            mappedNodes.forEach(mn => {
              if (!next.some(n => n.id === mn.id)) next.push(mn);
            });
            return next;
          });

          setEdges(prev => {
            const next = [...prev];
            mappedEdges.forEach(me => {
              if (!next.some(e => e.id === me.id)) next.push(me);
            });
            return next;
          });
        }
      }

      for (const node of nodes) {
        if (!node.id.startsWith('task-') && node.type === 'concept') {
          await bridge.fetchRaw('/api/v1/silva/graph', {
            method: 'POST',
            body: JSON.stringify({
              command: 'add_triple',
              subject: node.label,
              predicate: 'defines',
              object: 'blackboard_concept',
              agent_id: 'user'
            })
          });
        }
      }
    } catch (err) {
      console.warn('SilvaDB Sync Warning:', err);
    }
  };

  return (
    <div className="flex flex-col h-full overflow-hidden relative bg-[#06080d] text-slate-200">
      {/* Workspace Tabs Header */}
      <div className="flex items-center justify-between border-b border-slate-700/60 bg-slate-900/80 px-4 py-2 shrink-0 z-30">
        <div className="flex items-center gap-1">
          <button
            onClick={() => setActiveTab('preview')}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer",
              activeTab === 'preview' ? "bg-indigo-600 text-white shadow-lg shadow-indigo-900/20" : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            )}
          >
            <Play className="w-3.5 h-3.5" /> Preview {htmlArtifacts.length > 0 && <span className="ml-1 px-1.5 py-0.25 bg-white/10 rounded-full text-[9px]">{htmlArtifacts.length}</span>}
          </button>
          <button
            onClick={() => setActiveTab('docs')}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer",
              activeTab === 'docs' ? "bg-indigo-600 text-white shadow-lg shadow-indigo-900/20" : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            )}
          >
            <FileText className="w-3.5 h-3.5" /> Documentos {markdownDocs.length > 0 && <span className="ml-1 px-1.5 py-0.25 bg-white/10 rounded-full text-[9px]">{markdownDocs.length}</span>}
          </button>
          <button
            onClick={() => setActiveTab('whiteboard')}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer",
              activeTab === 'whiteboard' ? "bg-indigo-600 text-white shadow-lg shadow-indigo-900/20" : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            )}
          >
            <Palette className="w-3.5 h-3.5" /> Pizarra
          </button>
          <button
            onClick={() => setActiveTab('knowledge')}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-bold transition-all cursor-pointer",
              activeTab === 'knowledge' ? "bg-indigo-600 text-white shadow-lg shadow-indigo-900/20" : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            )}
          >
            <Layers className="w-3.5 h-3.5" /> Grafo
          </button>
        </div>
        <div className="flex items-center gap-2">
          {activeTab === 'whiteboard' && (
            <button
              onClick={syncWithSilvaDB}
              className="text-[10px] font-bold px-2 py-1 border border-indigo-500/30 bg-indigo-950/20 hover:bg-indigo-900/30 text-indigo-300 rounded-lg flex items-center gap-1 cursor-pointer"
            >
              <RotateCcw className="w-2.5 h-2.5" /> Sincronizar SilvaDB
            </button>
          )}
          <div className="flex items-center gap-1.5">
            <span className={cn('w-2 h-2 rounded-full', wsStatus === 'connected' ? 'bg-emerald-500' : 'bg-rose-500')} />
            <span className="text-[9px] text-slate-500 font-mono hidden sm:inline">
              {wsStatus === 'connected' ? 'sincronizado' : 'offline'}
            </span>
          </div>
        </div>
      </div>

      {/* ─── TAB PANELS CONTENT ────────────────────────────────────────────────── */}
      <div className="flex-1 overflow-hidden relative bg-[#090b11]">

        {/* 1. PREVIEW TAB (Interactive Sandbox) */}
        {activeTab === 'preview' && (
          <div className="absolute inset-0 flex flex-col p-4 bg-[#06080d]">
            {currentArtifact ? (
              <div className="flex-1 flex flex-col overflow-hidden bg-slate-900/40 border border-slate-800 rounded-2xl shadow-xl">
                <div className="flex items-center justify-between px-4 py-2.5 border-b border-slate-800/80 bg-slate-900/60 shrink-0">
                  <div className="flex items-center gap-2 min-w-0">
                    <Code className="w-3.5 h-3.5 text-indigo-400" />
                    <span className="text-xs font-bold text-slate-200 truncate">{currentArtifact.title}</span>
                    <span className="text-[10px] text-slate-500 font-mono">Turno #{currentArtifact.turn} (@{currentArtifact.author})</span>
                  </div>
                  <div className="flex items-center gap-1.5">
                    {htmlArtifacts.length > 1 && (
                      <select
                        value={selectedArtifactIndex}
                        onChange={(e) => setSelectedArtifactIndex(Number(e.target.value))}
                        className="bg-slate-800 border border-slate-700/60 rounded px-2 py-0.75 text-[10px] font-bold text-slate-300 focus:outline-none focus:border-indigo-500"
                      >
                        {htmlArtifacts.map((art, idx) => (
                          <option key={idx} value={idx}>{art.title} (T#{art.turn})</option>
                        ))}
                      </select>
                    )}
                    <button
                      onClick={handleReloadIframe}
                      className="p-1 hover:bg-slate-800 rounded text-slate-400 hover:text-slate-200 cursor-pointer"
                      title="Recargar iframe"
                    >
                      <RefreshCw className="w-3 h-3" />
                    </button>
                    <button
                      onClick={() => {
                        const w = window.open();
                        if (w) w.document.write(currentArtifact.code);
                      }}
                      className="p-1 hover:bg-slate-800 rounded text-slate-400 hover:text-slate-200 cursor-pointer"
                      title="Open in new tab"
                    >
                      <ExternalLink className="w-3 h-3" />
                    </button>
                  </div>
                </div>
                <div className="flex-1 bg-white relative">
                  <iframe
                    key={iframeKey}
                    srcDoc={currentArtifact.code}
                    title={currentArtifact.title}
                    className="w-full h-full border-0"
                    sandbox="allow-scripts allow-modals allow-forms"
                  />
                </div>
              </div>
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center gap-3 text-slate-500 border border-dashed border-slate-800 rounded-2xl bg-slate-900/10">
                <Play className="w-8 h-8 opacity-25 animate-pulse" />
                <div className="text-center max-w-sm px-4">
                  <h4 className="text-xs font-bold text-slate-400 mb-1">Empty Application Sandbox</h4>
                  <p className="text-[10px] text-slate-600 leading-relaxed">Write or request interactive HTML code in the coloquio (e.g., ```html code block). The canvas will detect it automatically and create an interactive view here.</p>
                </div>
              </div>
            )}
          </div>
        )}

        {/* 2. DOCS TAB (Collaborative Document Editor) */}
        {activeTab === 'docs' && (
          <div className="absolute inset-0 flex bg-[#06080d]">
            {/* Sidebar: document list */}
            <div className="w-56 border-r border-slate-800/60 bg-slate-900/40 flex flex-col shrink-0">
              <div className="p-3 border-b border-slate-800/60 flex items-center justify-between">
                <span className="text-[10px] font-bold text-slate-400 uppercase tracking-wider flex items-center gap-1.5">
                  <BookOpen className="w-3 h-3" /> Documentos
                </span>
                <button
                  onClick={() => setShowNewDocInput(true)}
                  className="p-1 hover:bg-slate-800 rounded text-slate-400 hover:text-slate-200 cursor-pointer"
                  title="New document"
                >
                  <PlusCircle className="w-3.5 h-3.5" />
                </button>
              </div>
              {showNewDocInput && (
                <div className="px-3 py-2 border-b border-slate-800/60 flex gap-1">
                  <input
                    type="text"
                    value={newDocTitle}
                    onChange={(e) => setNewDocTitle(e.target.value)}
                    onKeyDown={(e) => { if (e.key === 'Enter') createNewDoc(); if (e.key === 'Escape') setShowNewDocInput(false); }}
                    placeholder="Doc name..."
                    className="flex-1 bg-slate-800 border border-slate-700 rounded px-2 py-1 text-[10px] text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500"
                    autoFocus
                  />
                  <button onClick={createNewDoc} className="px-1.5 py-1 bg-indigo-600 hover:bg-indigo-500 text-white text-[9px] font-bold rounded cursor-pointer">OK</button>
                </div>
              )}
              <div className="flex-1 overflow-y-auto">
                {docsLoading ? (
                  <div className="p-3 text-[10px] text-slate-500">Loading...</div>
                ) : docs.length === 0 ? (
                  <div className="p-3 text-[10px] text-slate-500 text-center mt-8">
                    No documents yet.<br/>Create one.
                  </div>
                ) : (
                  docs.map(doc => (
                    <button
                      key={doc.doc_id}
                      onClick={() => selectDoc(doc.doc_id)}
                      className={cn(
                        "w-full text-left px-3 py-2 border-b border-slate-800/30 hover:bg-slate-800/40 transition-colors cursor-pointer",
                        currentDocId === doc.doc_id ? "bg-indigo-950/30 border-l-2 border-l-indigo-500" : ""
                      )}
                    >
                      <div className="text-[11px] font-bold text-slate-200 truncate">{doc.title}</div>
                      <div className="flex items-center gap-2 mt-0.5">
                        <span className="text-[8px] text-slate-500 font-mono">v{doc.version}</span>
                        <span className="text-[8px] text-slate-600 truncate">@{doc.updated_by}</span>
                      </div>
                    </button>
                  ))
                )}
              </div>
            </div>

            {/* Editor panel */}
            <div className="flex-1 flex flex-col overflow-hidden">
              {activeDoc ? (
                <>
                  {/* Conflict banner */}
                  {conflictDoc === currentDocId && (
                    <div className="flex items-center gap-2 px-4 py-1.5 bg-amber-900/30 border-b border-amber-700/40 shrink-0">
                      <AlertTriangle className="w-3 h-3 text-amber-400 shrink-0" />
                      <span className="text-[10px] text-amber-300 flex-1">
                        Conflicto: otro agente editó este documento. Tus cambios locales se conservan.
                      </span>
                      <button
                        onClick={resolveConflict}
                        className="flex items-center gap-1 px-2 py-0.5 bg-amber-600/30 hover:bg-amber-600/50 text-amber-300 text-[9px] font-bold rounded cursor-pointer border border-amber-600/40"
                      >
                        <RefreshIcon className="w-2.5 h-2.5" /> Reload version
                      </button>
                    </div>
                  )}
                  {/* Toolbar */}
                  <div className="flex items-center justify-between px-4 py-2 border-b border-slate-800/60 bg-slate-900/40 shrink-0">
                    <div className="flex items-center gap-2 min-w-0 flex-1">
                      {editingDoc ? (
                        <input
                          type="text"
                          value={docTitle}
                          onChange={(e) => handleDocTitleChange(e.target.value)}
                          className="bg-transparent border-b border-indigo-500/50 px-1 py-0.5 text-xs font-bold text-slate-200 focus:outline-none min-w-0 max-w-[200px]"
                        />
                      ) : (
                        <span className="text-xs font-bold text-slate-200 truncate">{docTitle}</span>
                      )}
                      <span className="text-[9px] text-slate-500 font-mono shrink-0">v{activeDoc.version}</span>
                      <span className="text-[9px] text-slate-600 shrink-0">@{activeDoc.updated_by}</span>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <button
                        onClick={() => {
                          setEditingDoc(!editingDoc);
                          if (!editingDoc) {
                            lastSavedContentRef.current = docContent;
                          } else {
                            saveDoc(docContent, docTitle);
                          }
                        }}
                        className={cn(
                          "flex items-center gap-1 px-2 py-1 rounded text-[10px] font-bold transition-all cursor-pointer",
                          editingDoc
                            ? "bg-emerald-600/20 border border-emerald-500/40 text-emerald-400"
                            : "bg-slate-800 border border-slate-700 text-slate-400 hover:text-slate-200"
                        )}
                      >
                        {editingDoc ? <><Lock className="w-2.5 h-2.5" /> Editando</> : <><Edit className="w-2.5 h-2.5" /> Editar</>}
                      </button>
                      <button
                        onClick={() => saveDoc(docContent, docTitle)}
                        className="p-1 hover:bg-slate-800 rounded text-slate-400 hover:text-slate-200 cursor-pointer"
                        title="Save now"
                      >
                        <Save className="w-3 h-3" />
                      </button>
                    </div>
                  </div>
                  {/* Content area */}
                  <div className="flex-1 overflow-hidden">
                    {editingDoc ? (
                      <textarea
                        value={docContent}
                        onChange={(e) => handleDocContentChange(e.target.value)}
                        className="w-full h-full bg-transparent text-[12px] text-slate-200 font-mono p-4 resize-none focus:outline-none leading-relaxed"
                        placeholder="Escribe el documento aquí... Pueden editar varios agentes simultáneamente."
                      />
                    ) : (
                      <div className="w-full h-full overflow-y-auto">
                        <div 
                          className="px-6 py-5 select-text leading-relaxed max-w-3xl mx-auto"
                          dangerouslySetInnerHTML={renderDocContent(docContent || '_The document is empty._')}
                        />
                      </div>
                    )}
                  </div>
                </>
              ) : (
                <div className="flex-1 flex flex-col items-center justify-center gap-3 text-slate-500">
                  <FileText className="w-8 h-8 opacity-25" />
                  <div className="text-center max-w-sm px-4">
                    <h4 className="text-xs font-bold text-slate-400 mb-1">Selecciona o crea un documento</h4>
                    <p className="text-[10px] text-slate-600 leading-relaxed">Los documentos colaborativos son editables por todos los agentes y por ti en tiempo real.</p>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}

        {/* 3. WHITEBOARD TAB (Node Graph SVG canvas) */}
        {(activeTab === 'whiteboard' || activeTab === 'knowledge') && (
          <div className="absolute inset-0 flex overflow-hidden">
            <div
              ref={containerRef}
              onWheel={handleWheel}
              onMouseMove={handleMouseMove}
              onMouseDown={(e) => handleMouseDown(e)}
              onMouseUp={handleMouseUp}
              onMouseLeave={handleMouseUp}
              className={cn(
                "flex-1 h-full relative overflow-hidden outline-none transition-colors duration-100",
                isPanning ? "cursor-grabbing" : "cursor-grab"
              )}
              style={{
                backgroundImage: `radial-gradient(circle, #1e293b 1px, transparent 1px)`,
                backgroundSize: `${20 * camera.zoom}px ${20 * camera.zoom}px`,
                backgroundPosition: `${camera.x}px ${camera.y}px`,
              }}
            >
              {/* World transform viewport */}
              <div
                style={{
                  transform: `translate(${camera.x}px, ${camera.y}px) scale(${camera.zoom})`,
                  transformOrigin: '0 0',
                  position: 'absolute',
                  inset: 0,
                  pointerEvents: 'none'
                }}
              >
                {/* SVG connections */}
                <svg className="absolute inset-0 w-full h-full z-0 overflow-visible">
                  <defs>
                    <marker id="arrow" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
                      <path d="M 0 0 L 10 5 L 0 10 z" fill="#3b82f6" opacity="0.7" />
                    </marker>
                  </defs>
                  {edges.map(edge => {
                    const srcNode = nodes.find(n => n.id === edge.source);
                    const tgtNode = nodes.find(n => n.id === edge.target);
                    if (!srcNode || !tgtNode) return null;

                    const w = 150;
                    const h = 50;
                    const x1 = srcNode.x + w / 2;
                    const y1 = srcNode.y + h / 2;
                    const x2 = tgtNode.x + w / 2;
                    const y2 = tgtNode.y + h / 2;

                    const dx = x2 - x1;
                    const dy = y2 - y1;
                    const len = Math.sqrt(dx * dx + dy * dy);

                    if (len === 0) return null;

                    const ratio = 0.5 * Math.min(w / Math.abs(dx || 1), h / Math.abs(dy || 1));
                    const ix1 = x1 + dx * ratio;
                    const iy1 = y1 + dy * ratio;
                    const ix2 = x2 - dx * ratio;
                    const iy2 = y2 - dy * ratio;

                    return (
                      <path
                        key={edge.id}
                        d={`M ${ix1} ${iy1} L ${ix2} ${iy2}`}
                        stroke="#3b82f6"
                        strokeWidth="2"
                        strokeOpacity="0.6"
                        fill="none"
                        markerEnd="url(#arrow)"
                      />
                    );
                  })}
                </svg>

                {/* Render Nodes */}
                {nodes.map(node => {
                  const theme = NODE_THEMES[node.type];
                  const isSelected = selectedNodeId === node.id;
                  const isConnecting = connectSourceId === node.id;

                  return (
                    <div
                      key={node.id}
                      onMouseDown={(e) => handleMouseDown(e, node)}
                      className={cn(
                        'absolute w-[150px] min-h-[50px] px-3 py-2 border rounded-xl shadow-xl flex flex-col justify-center items-center backdrop-blur-md z-10 pointer-events-auto group transition-shadow',
                        theme.bg, theme.border,
                        isSelected ? 'ring-2 ring-indigo-500 border-indigo-400 shadow-indigo-500/20' : ''
                      )}
                      style={{ left: node.x, top: node.y }}
                    >
                      <div className="absolute top-1 right-2 flex gap-1 opacity-0 hover:opacity-100 group-hover:opacity-100 transition-opacity">
                        <button onClick={(e) => { e.stopPropagation(); deleteNode(node.id); }} className="text-[10px] text-rose-400 hover:text-rose-300 font-bold cursor-pointer">×</button>
                      </div>
                      <div className={cn('text-[8px] font-bold uppercase tracking-widest mb-0.5', theme.color)}>
                        {node.type}
                      </div>
                      <div className="text-[11px] font-semibold text-slate-100 text-center w-full leading-tight select-text">
                        {node.label}
                      </div>
                      <div className="flex gap-1 mt-1.5 shrink-0">
                        <button
                          onClick={(e) => { e.stopPropagation(); setConnectSourceId(isConnecting ? null : node.id); }}
                          className={cn('text-[8px] px-1.5 py-0.5 rounded border transition-colors cursor-pointer',
                            isConnecting ? 'bg-indigo-950 border-indigo-500 text-indigo-300' : 'bg-slate-800 border-slate-700 text-slate-400 hover:text-slate-200')}
                        >
                          {isConnecting ? 'unir...' : 'conectar'}
                        </button>
                        {connectSourceId && connectSourceId !== node.id && (
                          <button
                            onClick={(e) => { e.stopPropagation(); createEdge(node.id); }}
                            className="text-[8px] px-1.5 py-0.5 rounded bg-emerald-950 border border-emerald-500 text-emerald-300 cursor-pointer"
                          >
                            vincular
                          </button>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>

            {/* Sidebar Toolbar panel */}
            <div className="absolute top-4 left-4 bg-slate-900/90 border border-slate-700/80 p-3.5 rounded-2xl shadow-2xl w-60 z-20 flex flex-col gap-3 backdrop-blur-sm">
              <div className="text-[10px] font-bold text-slate-500 uppercase tracking-wider flex items-center justify-between">
                <span>Controles del Canvas</span>
                <button onClick={resetCamera} title="Re-centrar cámara" className="text-slate-400 hover:text-slate-200 p-0.5">
                  <Maximize2 className="w-3.5 h-3.5" />
                </button>
              </div>
              <div className="flex flex-col gap-2">
                <input
                  type="text"
                  placeholder="New concept..."
                  value={newNodeLabel}
                  onChange={(e) => setNewNodeLabel(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && addNode()}
                  className="bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500 w-full"
                />
                <div className="flex gap-1">
                  {(['concept', 'task', 'episode', 'agent'] as const).map(t => (
                    <button
                      key={t}
                      onClick={() => setNewNodeType(t)}
                      className={cn('text-[9px] font-bold uppercase tracking-wider py-1 flex-1 rounded border capitalize transition-all',
                        newNodeType === t ? 'bg-indigo-950 border-indigo-500 text-indigo-300' : 'bg-slate-800 border-slate-700 text-slate-500 hover:text-slate-300')}
                    >
                      {t[0]}
                    </button>
                  ))}
                </div>
                <button
                  onClick={addNode}
                  disabled={!newNodeLabel.trim()}
                  className="py-1.5 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-40 text-white text-xs font-bold rounded-lg transition-colors w-full cursor-pointer"
                >
                  Añadir al Canvas
                </button>
              </div>
              {selectedNodeId && (
                <div className="border-t border-slate-800 pt-2 flex flex-col gap-1.5">
                  <div className="text-[9px] font-bold text-slate-600 uppercase tracking-widest">Acciones Nodo</div>
                  <button
                    onClick={() => deleteNode(selectedNodeId)}
                    className="py-1 bg-rose-950/30 hover:bg-rose-900/40 border border-rose-500/30 text-rose-400 hover:text-rose-300 text-[10px] font-bold rounded-lg transition-colors cursor-pointer"
                  >
                    Eliminar
                  </button>
                </div>
              )}
            </div>
          </div>
        )}

      </div>
    </div>
  );
}
