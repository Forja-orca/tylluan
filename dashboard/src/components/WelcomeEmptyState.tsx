import React, { useState, useEffect } from 'react';
import { Terminal, MessageSquare, PlusCircle, CheckCircle2, Circle, Copy, Check } from 'lucide-react';
import { cn } from '../lib/utils';
import { useNexus } from '../hooks/useNexus';
import { TylluanLogo } from './TylluanLogo';
import type { NexusBridge } from '../lib/nexus-bridge';
import { FirstBootConsentSection } from './first-boot/FirstBootConsentSection';

interface WelcomeEmptyStateProps {
  bridge: NexusBridge | null;
  sysStatus: { embeddings_loaded?: boolean; silva_healthy?: boolean; [key: string]: unknown } | null;
  sessions: unknown[];
  memoryStats: Record<string, unknown> | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onRefresh: () => void;
}

export function WelcomeEmptyState({
  bridge,
  sysStatus,
  sessions,
  memoryStats: _memoryStats,
  notify,
  onRefresh
}: WelcomeEmptyStateProps) {
  const { agentProfiles, bridge: ctxBridge, sessions: hookSessions } = useNexus();
  const [firstQueryText, setFirstQueryText] = useState('¿Qué es Tylluan?');
  const [noteText, setNoteText] = useState('Tylluan ejecuta RAG local.');
  const [copied, setCopied] = useState(false);
  const [addingNote, setAddingNote] = useState(false);
  const [querying, setQuerying] = useState(false);
  const [sseUrl, setSseUrl] = useState<string | null>(null);

  // Kernel is the source of truth for its own address (setup-hint derives the URL
  // from the request Host header). Never hardcode the port here.
  useEffect(() => {
    const b = bridge ?? ctxBridge ?? null;
    if (!b?.fetchRaw) return;
    try {
      const res = b.fetchRaw('/api/v1/setup-hint');
      if (res && typeof (res as Promise<unknown>).then === 'function') {
        (res as Promise<{ mcp_clients?: { claude_desktop?: { config?: { mcpServers?: { tylluan?: { url?: string } } } } } }>)
          .then((hint) => {
            if (hint?.mcp_clients?.claude_desktop?.config?.mcpServers?.tylluan?.url) {
              setSseUrl(hint.mcp_clients.claude_desktop.config.mcpServers.tylluan.url);
            }
          })
          .catch(() => {});
      }
    } catch {
      // Safe fallback
    }
  }, [bridge, ctxBridge]);

  // Decisión de consentimiento de primer arranque guardada en localStorage
  const [firstBootChoice, setFirstBootChoice] = useState<string | null>(
    () => localStorage.getItem('tylluan_first_boot_choice')
  );
  const [localModelReady, setLocalModelReady] = useState(false);

  // States for checklist (using localStorage and api telemetry)
  const isInstalled = true; // By definition if they see the dashboard
  const isProfileResolved = agentProfiles && agentProfiles.length > 0;
  
  // D-166 / T3898: No considerar el modelo cargado sólo porque SQLite (silva_healthy) esté inicializada.
  // La búsqueda vectorial densa requiere que embeddings_loaded sea true o que haya un modelo no vacío distinto de 'none'.
  const isModelLoaded = localModelReady || Boolean(
    sysStatus?.embeddings_loaded || 
    (typeof sysStatus?.embedding_model === 'string' && sysStatus.embedding_model !== 'none' && sysStatus.embedding_model.trim() !== '')
  );
  const isEmbeddingsStepDone = isModelLoaded || firstBootChoice === 'bm25_offline';
  
  // MCP is connected if there is at least one session, or if they checked the box
  const [mcpChecked, setMcpChecked] = useState(() => localStorage.getItem('tylluan_wizard_mcp') === 'true');
  const effectiveSessions = React.useMemo(() => {
    return (sessions && sessions.length > 0) ? sessions : (hookSessions ?? []);
  }, [sessions, hookSessions]);
  const isMcpConnected = effectiveSessions.length > 0 || mcpChecked;

  // First query completed state
  const [firstQueryDone, setFirstQueryDone] = useState(() => localStorage.getItem('tylluan_wizard_query') === 'true');

  useEffect(() => {
    if (effectiveSessions.length > 0 && !mcpChecked) {
      setMcpChecked(true);
      localStorage.setItem('tylluan_wizard_mcp', 'true');
    }
  }, [effectiveSessions, mcpChecked]);

  const handleCopyMcp = () => {
    const config = {
      mcpServers: {
        tylluan: {
          type: "sse",
          url: sseUrl || `${bridge?.getBaseUrl?.() ?? window.location.origin}/sse`
        }
      }
    };
    navigator.clipboard.writeText(JSON.stringify(config, null, 2));
    setCopied(true);
    notify('MCP Configuration copied to clipboard', 'info');
    setMcpChecked(true);
    localStorage.setItem('tylluan_wizard_mcp', 'true');
    setTimeout(() => setCopied(false), 2000);
  };

  const handleAddNote = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!noteText.trim() || !bridge) return;
    setAddingNote(true);
    try {
      await bridge.fetchRaw('/api/v1/memory/write', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content: noteText })
      });
      notify('First note stored in SilvaDB!', 'info');
      setNoteText('');
      onRefresh();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unknown error';
      notify(`Failed to add note: ${msg}`, 'error');
    } finally {
      setAddingNote(false);
    }
  };

  const handleQuery = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!firstQueryText.trim() || !bridge) return;
    setQuerying(true);
    try {
      const res = await bridge.fetchRaw('/api/v1/memory/search', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: firstQueryText.trim(), limit: 5 })
      });
      const hits = res?.results?.length ?? res?.nodes?.length ?? 0;
      notify(hits > 0 ? `Query completed — ${hits} results in memory` : 'Query completed — no results in memory', 'info');
      setFirstQueryDone(true);
      localStorage.setItem('tylluan_wizard_query', 'true');
      setFirstQueryText('');
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unknown error';
      notify(`Query failed: ${msg}`, 'error');
    } finally {
      setQuerying(false);
    }
  };

  const mcpConfigJson = `{
  "mcpServers": {
    "tylluan": {
      "type": "sse",
      "url": "${sseUrl || `${bridge?.getBaseUrl?.() ?? window.location.origin}/sse`}"
    }
  }
}`;

  return (
    <div className="max-w-5xl mx-auto space-y-8 py-8 animate-in fade-in duration-500 font-sans">
      {/* Hero Section */}
      <div className="text-center space-y-3">
        <div className="flex justify-center mb-2">
          <TylluanLogo size="xl" animated={false} showText={false} />
        </div>
        <h2 className="text-3xl font-extrabold tracking-tight text-foreground uppercase font-mono">
          Bienvenido a <span className="text-primary">Tylluan</span>
        </h2>
        <p className="text-muted-foreground text-sm max-w-md mx-auto">
          Tu memoria soberana para IA está lista. SilvaDB inicializada y escuchando localmente.
        </p>
      </div>

      {/* Panel interactivo de Primer Arranque y Consentimiento de Descarga */}
      {!isModelLoaded && firstBootChoice !== 'bm25_offline' && (
        <FirstBootConsentSection
          bridge={bridge ?? ctxBridge ?? null}
          sysStatus={sysStatus}
          notify={notify}
          onContinueOffline={() => {
            setFirstBootChoice('bm25_offline');
          }}
          onModelReady={(_modelId) => {
            setLocalModelReady(true);
            onRefresh();
          }}
        />
      )}

      {/* Banner discreto si el operador eligió modo BM25 offline */}
      {firstBootChoice === 'bm25_offline' && !isModelLoaded && (
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 p-3.5 rounded-xl bg-card border border-border text-xs text-muted-foreground animate-in fade-in">
          <div className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-emerald-500" />
            <span>Operando en modo puramente léxico BM25 sin descargas de red ni dependencias externas.</span>
          </div>
          <button
            type="button"
            onClick={() => {
              localStorage.removeItem('tylluan_first_boot_choice');
              setFirstBootChoice(null);
            }}
            className="text-primary hover:underline font-semibold text-[11px] cursor-pointer shrink-0 text-left sm:text-right"
          >
            Activar búsqueda semántica vectorial
          </button>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
        {/* Onboarding Tasks Checklist */}
        <div className="lg:col-span-1 bg-card border border-border rounded-2xl p-5 space-y-4">
          <h3 className="text-[11px] font-medium text-muted-foreground border-b border-border pb-2">
            Lista de Inicio
          </h3>
          <div className="space-y-3">
            {[
              { id: 'install', label: 'Instalar Tylluan', done: isInstalled },
              { id: 'profile', label: 'Crear Perfil de Agente', done: isProfileResolved },
              { 
                id: 'model', 
                label: isModelLoaded 
                  ? 'Motor de Embeddings (Activo)' 
                  : (firstBootChoice === 'bm25_offline' ? 'Búsqueda Léxica (BM25 Soberano)' : 'Motor de Embeddings'), 
                done: isEmbeddingsStepDone 
              },
              { id: 'mcp', label: 'Conectar Cliente MCP', done: isMcpConnected },
              { id: 'query', label: 'Primera Consulta / Acción', done: firstQueryDone }
            ].map(task => (
              <div key={task.id} className="flex items-center gap-3 text-xs">
                {task.done ? (
                  <CheckCircle2 className="w-4 h-4 text-primary shrink-0" />
                ) : (
                  <Circle className="w-4 h-4 text-muted-foreground/60 shrink-0" />
                )}
                <span className={cn(task.done ? "text-muted-foreground line-through decoration-muted-foreground/60" : "text-foreground font-medium")}>
                  {task.label}
                </span>
              </div>
            ))}
          </div>

          <div className="pt-2">
            <button
              onClick={() => {
                localStorage.setItem('tylluan_wizard_query', 'true');
                localStorage.setItem('tylluan_wizard_mcp', 'true');
                setFirstQueryDone(true);
                setMcpChecked(true);
                notify('Guía inicial omitida', 'info');
              }}
              className="w-full text-center text-[11px] text-muted-foreground hover:text-foreground font-medium mt-2 cursor-pointer transition-colors focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none rounded"
            >
              Omitir Guía
            </button>
          </div>
        </div>

        {/* 3 Core Action Cards */}
        <div className="lg:col-span-3 grid grid-cols-1 md:grid-cols-3 gap-6">
          
          {/* Card 1: Add First Note */}
          <div className="bg-card border border-border rounded-2xl p-5 flex flex-col justify-between transition-all group">
            <div className="space-y-3">
              <div className="p-2 w-9 h-9 rounded-xl bg-primary/10 border border-primary/20 text-primary flex items-center justify-center">
                <PlusCircle className="w-5 h-5" />
              </div>
              <h3 className="text-sm font-semibold text-foreground">1. Guardar Nota</h3>
              <p className="text-[11px] text-muted-foreground leading-relaxed">
                Almacena una memoria o dato clave en tu base local SilvaDB.
              </p>
              
              <form onSubmit={handleAddNote} className="space-y-2 pt-2">
                <input
                  type="text"
                  value={noteText}
                  onChange={(e) => setNoteText(e.target.value)}
                  placeholder="ej. Tylluan ejecuta RAG local."
                  className="w-full bg-background border border-border rounded-lg px-2.5 py-1.5 text-xs text-foreground focus:outline-none focus:border-primary font-sans"
                  required
                />
                <button
                  type="submit"
                  disabled={addingNote}
                  className="w-full py-1.5 bg-primary text-primary-foreground rounded-lg text-[11px] font-semibold transition-all duration-200 cursor-pointer hover:opacity-90 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                >
                  {addingNote ? 'Guardando...' : 'Recordar'}
                </button>
              </form>
            </div>
          </div>

          {/* Card 2: Connect Tools */}
          <div className="bg-card border border-border rounded-2xl p-5 flex flex-col justify-between transition-all">
            <div className="space-y-3">
              <div className="p-2 w-9 h-9 rounded-xl bg-primary/10 border border-primary/20 text-primary flex items-center justify-center">
                <Terminal className="w-5 h-5" />
              </div>
              <h3 className="text-sm font-semibold text-foreground">2. Conectar Cliente</h3>
              <p className="text-[11px] text-muted-foreground leading-relaxed">
                Configura tu cliente MCP para conectarse a Tylluan.
              </p>

              <div className="relative mt-2">
                <pre
                  tabIndex={0}
                  role="region"
                  aria-label="Configuración MCP generada"
                  className="bg-background border border-border rounded-lg p-2.5 text-[11px] font-mono text-muted-foreground leading-normal overflow-x-auto select-all max-h-[85px] focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
                >
                  {mcpConfigJson}
                </pre>
                <button
                  onClick={handleCopyMcp}
                  className="absolute top-1.5 right-1.5 p-1 bg-card hover:bg-muted text-muted-foreground hover:text-foreground rounded transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none border border-border"
                  title="Copiar configuración"
                >
                  {copied ? <Check className="w-3.5 h-3.5 text-primary" /> : <Copy className="w-3.5 h-3.5" />}
                </button>
              </div>
            </div>
          </div>

          {/* Card 3: Query Memory */}
          <div className="bg-card border border-border rounded-2xl p-5 flex flex-col justify-between transition-all">
            <div className="space-y-3">
              <div className="p-2 w-9 h-9 rounded-xl bg-primary/10 border border-primary/20 text-primary flex items-center justify-center">
                <MessageSquare className="w-5 h-5" />
              </div>
              <h3 className="text-sm font-semibold text-foreground">3. Probar Consulta</h3>
              <p className="text-[11px] text-muted-foreground leading-relaxed">
                Ejecuta una consulta de prueba sobre los nodos de SilvaDB.
              </p>

              <form onSubmit={handleQuery} className="space-y-2 pt-2">
                <input
                  type="text"
                  value={firstQueryText}
                  onChange={(e) => setFirstQueryText(e.target.value)}
                  placeholder="Pregunta algo..."
                  className="w-full bg-background border border-border rounded-lg px-2.5 py-1.5 text-xs text-foreground focus:outline-none focus:border-primary font-sans"
                  required
                />
                <button
                  type="submit"
                  disabled={querying}
                  className="w-full py-1.5 bg-primary text-primary-foreground rounded-lg text-[11px] font-semibold transition-all duration-200 cursor-pointer hover:opacity-90 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                >
                  {querying ? 'Consultando...' : 'Consultar'}
                </button>
              </form>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
