import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { 
  Users, 
  RefreshCw, 
  Trash2, 
  Clock, 
  Shield, 
  Activity, 
  User, 
  Copy, 
  Check, 
  Terminal, 
  Info, 
  ExternalLink,
  BookOpen,
  Cpu,
  Link2,
  Plug,
  AlertTriangle
} from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import type { McpSession } from '../lib/nexus-bridge';
import { cn, relativeTime } from '../lib/utils';

export interface ToolCollision {
  name: string;
  kept_server: string;
  shadowed_servers: string[];
}

export interface ExternalMcpServer {
  name: string;
  active: boolean;
  running: boolean;
  command?: string | null;
  args?: string[] | null;
  cwd?: string | null;
  env?: Record<string, string> | null;
  headers?: Record<string, string> | null;
  tools?: Array<{ name: string; description?: string }>;
  url?: string | null;
  sse_url?: string | null;
  timeout_ms?: number | null;
}

interface CapabilitiesData {
  status: string;
  version: string;
  sovereign_contract: {
    tools: Array<{
      name: string;
      description: string;
      inputSchema?: Record<string, unknown>;
    }>;
  };
  guilds: Array<{
    name: string;
    running: boolean;
    always_on: boolean;
    tools_count: number;
    launcher_type: string;
  }>;
  all_guild_tools: Array<{
    name: string;
    description: string;
    inputSchema?: Record<string, unknown>;
  }>;
  mcp: {
    prompts: Array<{ name: string; description: string }>;
    resources: Array<{ uri: string; name: string; description: string; mimeType: string }>;
  };
  sessions: McpSession[];
  tool_collisions?: ToolCollision[];
}

interface SetupHintData {
  mcp_clients?: {
    claude_desktop?: {
      config?: {
        mcpServers?: {
          tylluan?: {
            url?: string;
          };
        };
      };
    };
    [key: string]: unknown;
  };
  verify?: {
    dashboard?: string;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

export type ConnectorSubTab = 'sessions' | 'external' | 'catalog' | 'guides';

export function ConnectorsTab({ notify }: { notify: (msg: string, type?: 'info' | 'error') => void }) {
  const { bridge, sessions: globalSessions, loading: globalLoading, refreshData } = useNexus();
  const [capabilities, setCapabilities] = useState<CapabilitiesData | null>(null);
  const [externalServers, setExternalServers] = useState<ExternalMcpServer[]>([]);
  const [localLoading, setLocalLoading] = useState(false);
  const [activeSubTab, setActiveSubTab] = useState<ConnectorSubTab>('sessions');
  const [guideTab, setGuideTab] = useState<'cursor' | 'vscode' | 'claude' | 'custom'>('cursor');
  const [revokingIds, setRevokingIds] = useState<Set<string>>(new Set());
  const [copiedText, setCopiedText] = useState<string | null>(null);
  const [hint, setHint] = useState<SetupHintData | null>(null);

  const subTabList: ConnectorSubTab[] = ['sessions', 'external', 'catalog', 'guides'];
  const subTabRefs = useRef<Record<ConnectorSubTab, HTMLButtonElement | null>>({
    sessions: null,
    external: null,
    catalog: null,
    guides: null,
  });

  const handleSubTabKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, current: ConnectorSubTab) => {
    const idx = subTabList.indexOf(current);
    let nextIdx = -1;
    if (e.key === 'ArrowRight') nextIdx = (idx + 1) % subTabList.length;
    else if (e.key === 'ArrowLeft') nextIdx = (idx - 1 + subTabList.length) % subTabList.length;
    else if (e.key === 'Home') nextIdx = 0;
    else if (e.key === 'End') nextIdx = subTabList.length - 1;
    if (nextIdx !== -1) {
      e.preventDefault();
      const nextTab = subTabList[nextIdx];
      setActiveSubTab(nextTab);
      subTabRefs.current[nextTab]?.focus();
    }
  };

  const guideTabList: Array<'cursor' | 'vscode' | 'claude' | 'custom'> = ['cursor', 'vscode', 'claude', 'custom'];
  const guideTabRefs = useRef<Record<'cursor' | 'vscode' | 'claude' | 'custom', HTMLButtonElement | null>>({
    cursor: null,
    vscode: null,
    claude: null,
    custom: null,
  });

  const handleGuideTabKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, current: 'cursor' | 'vscode' | 'claude' | 'custom') => {
    const idx = guideTabList.indexOf(current);
    let nextIdx = -1;
    if (e.key === 'ArrowDown' || e.key === 'ArrowRight') nextIdx = (idx + 1) % guideTabList.length;
    else if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') nextIdx = (idx - 1 + guideTabList.length) % guideTabList.length;
    else if (e.key === 'Home') nextIdx = 0;
    else if (e.key === 'End') nextIdx = guideTabList.length - 1;
    if (nextIdx !== -1) {
      e.preventDefault();
      const nextTab = guideTabList[nextIdx];
      setGuideTab(nextTab);
      guideTabRefs.current[nextTab]?.focus();
    }
  };

  const abortControllerRef = useRef<AbortController | null>(null);

  const fetchCapabilities = useCallback(async () => {
    if (!bridge) return;
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    setLocalLoading(true);
    try {
      const [data, hintData, extData] = await Promise.allSettled([
        bridge.getCapabilities(),
        bridge.fetchRaw('/api/v1/setup-hint', { signal: controller.signal }),
        bridge.fetchRaw('/api/v1/mcp/external', { signal: controller.signal }),
      ]);
      if (data.status === 'fulfilled') setCapabilities(data.value as CapabilitiesData);
      if (hintData.status === 'fulfilled') setHint(hintData.value as SetupHintData);
      if (extData.status === 'fulfilled' && Array.isArray(extData.value)) {
        setExternalServers(extData.value as ExternalMcpServer[]);
      }
    } catch (e) {
      if (e instanceof Error && e.name === 'AbortError') return;
      console.error('Failed to fetch capabilities:', e);
      notify('Failed to load server capabilities', 'error');
    } finally {
      setLocalLoading(false);
    }
  }, [bridge, notify]);

  useEffect(() => {
    fetchCapabilities();
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, [fetchCapabilities]);

  const handleRefresh = async () => {
    await Promise.all([
      refreshData(),
      fetchCapabilities()
    ]);
    notify('Connectors status updated', 'info');
  };

  const handleRevoke = async (id: string) => {
    if (!bridge) return;
    setRevokingIds(prev => {
      const next = new Set(prev);
      next.add(id);
      return next;
    });

    try {
      await bridge.revokeSession(id);
      notify(`Session ${id.slice(0, 8)} revoked`, 'info');
      // Refresh local view immediately
      fetchCapabilities();
    } catch (e) {
      notify(`Failed to revoke session: ${e instanceof Error ? e.message : 'Unknown error'}`, 'error');
    } finally {
      setRevokingIds(prev => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const handleCopy = (text: string, label: string) => {
    navigator.clipboard.writeText(text);
    setCopiedText(label);
    notify(`${label} copiado al portapapeles`, 'info');
    setTimeout(() => setCopiedText(null), 2000);
  };

  // Snippets are built from the kernel's setup-hint (URL derived from the Host
  // the dashboard used to reach it — correct for legacy 3030, local 4000, or any
  // custom port). Never hardcode the port.
  const originUrl = typeof window !== 'undefined' ? window.location.origin : 'http://localhost:47004';
  const baseUrl = typeof bridge?.getBaseUrl === 'function' ? bridge.getBaseUrl() : originUrl;
  const sseUrl = hint?.mcp_clients?.claude_desktop?.config?.mcpServers?.tylluan?.url
    ?? `${baseUrl}/sse`;

  // Bridge is mcp-remote (real, published on npm) — NOT '@modelcontextprotocol/client-cli',
  // which does not exist on the npm registry (verified via `npm view`, 404). That fake
  // package name previously broke Qwen Desktop/Claude Desktop/Codex silently; see
  // docs/guides/CLIENT_INTEGRATION_GUIDE.md and commit 7f12879. URL is a positional
  // arg, not an env var; --allow-http is required for a plain-HTTP local server.
  const cursorConfig = `{
  "mcpServers": {
    "tylluan-nexus": {
      "command": "npx",
      "args": ["mcp-remote@latest", "${sseUrl}", "--allow-http"]
    }
  }
}`;

  // Generic, machine-independent config (previously pointed at a machine-specific
  // path: C:/Users/<usuario>/...). Same mcp-remote bridge as the other editors.
  const clineConfig = `{
  "mcpServers": {
    "tylluan-nexus": {
      "command": "npx",
      "args": ["mcp-remote@latest", "${sseUrl}", "--allow-http"],
      "disabled": false,
      "alwaysOn": true
    }
  }
}`;

  const claudeConfig = `{
  "mcpServers": {
    "tylluan-nexus": {
      "command": "npx",
      "args": ["mcp-remote@latest", "${sseUrl}", "--allow-http"]
    }
  }
}`;

  const restCurl = `curl -X POST ${hint?.verify?.dashboard ?? baseUrl}/api/v1/do \\
  -H "Authorization: Bearer TU_TOKEN_AQUI" \\
  -H "Content-Type: application/json" \\
  -d '{
    "intent": "Read the contents of README.md and summarize it",
    "agent_id": "curl-client"
  }'`;

  // Merge local capabilities sessions list with global hook sessions list
  const activeSessions = useMemo(() => {
    if (capabilities?.sessions) {
      return capabilities.sessions;
    }
    return globalSessions || [];
  }, [capabilities, globalSessions]);

  // WCAG-A pre-flight 2026-08-12 (dark theme, exact token vars):
  //  slate-50 on slate-900 17.16:1 · header gradient worst (emerald-700/30): 12.91:1 · hover slate-850 15.98:1
  //  violet-400 (purple->violet map): 7.14:1 on slate-950 · emerald-400 on emerald-500/10: 8.14-9.00:1
  return (
    <div className="space-y-4">
      {/* Header Banner */}
      <div className="bg-card p-5 rounded-lg border border-border shadow-sm flex flex-col md:flex-row md:items-center justify-between gap-6">
        <div className="space-y-2">
          <div className="flex items-center gap-3">
            <div className="p-2.5 bg-primary/10 border border-primary/20 rounded-lg flex items-center justify-center">
              <Link2 className="w-5 h-5 text-primary" />
            </div>
            <div>
              <h1 className="text-xl font-bold font-sans text-foreground tracking-tight">Connectors & Discovery</h1>
              <p className="text-xs text-muted-foreground font-mono">Milestone M24 Sovereign Integrations</p>
            </div>
          </div>
          <p className="text-xs text-muted-foreground max-w-xl leading-relaxed">
            TylluanNexus acts as a Sovereign MCP Hub. External client LLMs connect via standard MCP or REST APIs.
            The client LLM discovers capabilities dynamically via Prompts & Resources, keeping contexts light.
          </p>
        </div>

        <div className="flex items-center gap-3 shrink-0 self-end md:self-center">
          <button 
            type="button"
            onClick={handleRefresh}
            disabled={globalLoading || localLoading}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-muted/60 hover:bg-muted border border-border rounded-md text-xs font-semibold text-foreground shadow-sm transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            <RefreshCw className={cn("w-3.5 h-3.5 text-primary", (globalLoading || localLoading) && "animate-spin")} />
            <span>Actualizar Estado</span>
          </button>
        </div>
      </div>

      {/* Tabs Selector */}
      <div role="tablist" aria-label="Secciones de conectores" className="flex flex-wrap border-b border-border pb-2 gap-1.5">
        <button 
          ref={(el) => { subTabRefs.current.sessions = el; }}
          type="button"
          role="tab"
          aria-selected={activeSubTab === 'sessions'}
          tabIndex={activeSubTab === 'sessions' ? 0 : -1}
          onClick={() => setActiveSubTab('sessions')}
          onKeyDown={(e) => handleSubTabKeyDown(e, 'sessions')}
          className={cn(
            "px-3 py-1.5 text-xs font-semibold border transition-all cursor-pointer rounded-md focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
            activeSubTab === 'sessions' 
              ? "bg-primary/10 border-primary/30 text-primary shadow-sm" 
              : "bg-card border-border text-muted-foreground hover:bg-muted/60 hover:text-foreground"
          )}
        >
          <div className="flex items-center gap-1.5">
            <Users className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
            <span>Sesiones Activas</span>
            {activeSessions.length > 0 && (
              <span className="px-1.5 py-0.5 rounded-md bg-emerald-500/15 text-[10px] font-mono text-emerald-700 dark:text-emerald-400 border border-emerald-500/30">
                {activeSessions.length}
              </span>
            )}
          </div>
        </button>

        <button 
          ref={(el) => { subTabRefs.current.external = el; }}
          type="button"
          role="tab"
          aria-selected={activeSubTab === 'external'}
          tabIndex={activeSubTab === 'external' ? 0 : -1}
          onClick={() => setActiveSubTab('external')}
          onKeyDown={(e) => handleSubTabKeyDown(e, 'external')}
          className={cn(
            "px-3 py-1.5 text-xs font-semibold border transition-all cursor-pointer rounded-md focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
            activeSubTab === 'external' 
              ? "bg-primary/10 border-primary/30 text-primary shadow-sm" 
              : "bg-card border-border text-muted-foreground hover:bg-muted/60 hover:text-foreground"
          )}
        >
          <div className="flex items-center gap-1.5">
            <Plug className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
            <span>Servidores MCP Externos</span>
            {externalServers.length > 0 && (
              <span className="px-1.5 py-0.5 rounded-md bg-purple-500/15 text-[10px] font-mono text-purple-700 dark:text-purple-400 border border-purple-500/30">
                {externalServers.length}
              </span>
            )}
          </div>
        </button>

        <button 
          ref={(el) => { subTabRefs.current.catalog = el; }}
          type="button"
          role="tab"
          aria-selected={activeSubTab === 'catalog'}
          tabIndex={activeSubTab === 'catalog' ? 0 : -1}
          onClick={() => setActiveSubTab('catalog')}
          onKeyDown={(e) => handleSubTabKeyDown(e, 'catalog')}
          className={cn(
            "px-3 py-1.5 text-xs font-semibold border transition-all cursor-pointer rounded-md focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
            activeSubTab === 'catalog' 
              ? "bg-primary/10 border-primary/30 text-primary shadow-sm" 
              : "bg-card border-border text-muted-foreground hover:bg-muted/60 hover:text-foreground"
          )}
        >
          <div className="flex items-center gap-1.5">
            <Cpu className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
            <span>Catálogo de Capacidades</span>
            {capabilities && (
              <span className="px-1.5 py-0.5 rounded-md bg-blue-500/15 text-[10px] font-mono text-blue-700 dark:text-blue-400 border border-blue-500/30">
                {capabilities.guilds.length} Guilds
              </span>
            )}
          </div>
        </button>

        <button 
          ref={(el) => { subTabRefs.current.guides = el; }}
          type="button"
          role="tab"
          aria-selected={activeSubTab === 'guides'}
          tabIndex={activeSubTab === 'guides' ? 0 : -1}
          onClick={() => setActiveSubTab('guides')}
          onKeyDown={(e) => handleSubTabKeyDown(e, 'guides')}
          className={cn(
            "px-3 py-1.5 text-xs font-semibold border transition-all cursor-pointer rounded-md focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
            activeSubTab === 'guides' 
              ? "bg-primary/10 border-primary/30 text-primary shadow-sm" 
              : "bg-card border-border text-muted-foreground hover:bg-muted/60 hover:text-foreground"
          )}
        >
          <div className="flex items-center gap-1.5">
            <BookOpen className="w-3.5 h-3.5 text-primary" aria-hidden="true" />
            <span>Guías de Configuración</span>
          </div>
        </button>
      </div>

      {/* Tab Panels */}
      <div className="space-y-4">
        {activeSubTab === 'sessions' && (
          <div className="space-y-3">
            <div className="bg-card rounded-lg border border-border shadow-sm overflow-x-auto">
              <table className="w-full text-left border-collapse text-xs font-sans">
                <thead>
                  <tr className="bg-muted/60 border-b border-border text-foreground font-semibold">
                    <th className="px-3.5 py-2.5 border-r border-border/60">Cliente / Proceso</th>
                    <th className="px-3.5 py-2.5 border-r border-border/60">Identificación del Agente</th>
                    <th className="px-3.5 py-2.5 text-center border-r border-border/60">Herramientas Invocadas</th>
                    <th className="px-3.5 py-2.5 border-r border-border/60">Guild Activo</th>
                    <th className="px-3.5 py-2.5 border-r border-border/60">Última Actividad</th>
                    <th className="px-3.5 py-2.5 text-right">Revocar Acceso</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border">
                  {activeSessions.map((session) => (
                    <tr key={session.id} className="group hover:bg-muted/30 transition-colors">
                      <td className="px-4 py-3">
                        <div className="flex items-center gap-3">
                          <div className="w-8 h-8 rounded-md bg-muted flex items-center justify-center border border-border text-muted-foreground group-hover:text-primary transition-colors">
                            <User className="w-4 h-4" />
                          </div>
                          <div>
                            <div className="text-xs font-bold text-foreground">{session.client_name}</div>
                            <div className="text-[11px] font-mono text-muted-foreground">ID: {session.id.slice(0, 16)}...</div>
                          </div>
                        </div>
                      </td>
                      <td className="px-4 py-3">
                        {session.agent_id ? (
                          <div className="flex items-center gap-2">
                            <Shield className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
                            <span className="text-xs font-mono text-emerald-700 dark:text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded-md border border-emerald-500/20 truncate max-w-[180px]">
                              {session.agent_id}
                            </span>
                          </div>
                        ) : (
                          <span className="text-xs text-muted-foreground italic">Unidentified agent</span>
                        )}
                      </td>
                      <td className="px-4 py-3 text-center">
                        <span className="px-2 py-0.5 rounded-md bg-muted border border-border text-xs font-mono text-foreground">
                          {session.tool_count}
                        </span>
                      </td>
                      <td className="px-4 py-3">
                        {session.last_guild ? (
                          <div className="flex items-center gap-2">
                            <Activity className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400" />
                            <span className="text-xs font-mono text-blue-700 dark:text-blue-400 bg-blue-500/10 px-2.5 py-0.5 rounded-md border border-blue-500/20">
                              {session.last_guild}
                            </span>
                          </div>
                        ) : (
                          <span className="text-xs text-muted-foreground font-mono">—</span>
                        )}
                      </td>
                      <td className="px-4 py-3">
                        <div className="flex items-center gap-2 text-xs text-muted-foreground font-mono">
                          <Clock className="w-3.5 h-3.5 text-muted-foreground" />
                          {relativeTime(session.last_active_unix)}
                        </div>
                      </td>
                      <td className="px-4 py-3 text-right">
                        <button
                          type="button"
                          aria-label={`Revocar sesión ${session.id.slice(0, 8)}`}
                          onClick={() => handleRevoke(session.id)}
                          disabled={revokingIds.has(session.id)}
                          className={cn(
                            "p-1.5 rounded-md transition-colors border cursor-pointer focus-visible:ring-2 focus-visible:ring-rose-500 focus-visible:outline-none",
                            revokingIds.has(session.id) 
                              ? "bg-muted border-border cursor-not-allowed text-muted-foreground" 
                              : "bg-rose-500/10 hover:bg-rose-500/25 text-rose-700 dark:text-rose-400 border-rose-500/20 hover:border-rose-500/40"
                          )}
                          title="Revocar token de cliente y desconectar"
                        >
                          {revokingIds.has(session.id) ? (
                            <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                          ) : (
                            <Trash2 className="w-3.5 h-3.5" />
                          )}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>

              {activeSessions.length === 0 && (
                <div className="py-12 flex flex-col items-center justify-center text-center p-4">
                  <Plug className="w-8 h-8 mb-2 opacity-40 text-primary" />
                  <p className="text-xs font-bold text-foreground">Sin sesiones de clientes activas</p>
                  <p className="text-xs text-muted-foreground max-w-sm mt-1">
                    Conecte un cliente externo compatible con MCP (Claude Desktop, Cursor, Code) para interactuar con las herramientas soberanas del microkernel.
                  </p>
                </div>
              )}
            </div>
          </div>
        )}

        {activeSubTab === 'external' && (
          <div className="space-y-4">
            {/* Tool Collisions Banner if present */}
            {capabilities?.tool_collisions && capabilities.tool_collisions.length > 0 && (
              <div role="alert" className="p-4 bg-amber-500/10 border border-amber-500/30 rounded-lg text-amber-900 dark:text-amber-200 space-y-2">
                <div className="flex items-center gap-2 font-bold text-xs">
                  <AlertTriangle className="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0" />
                  <span>Colisiones de Herramientas MCP Detectadas ({capabilities.tool_collisions.length})</span>
                </div>
                <p className="text-[11px] text-muted-foreground leading-relaxed">
                  Múltiples guilds o servidores MCP declaran herramientas con el mismo nombre. Tylluan aplica precedencia determinista en orden alfabético: el primer servidor retiene la herramienta activa y los restantes quedan enmascarados.
                </p>
                <div className="overflow-x-auto mt-2">
                  <table className="w-full text-left text-xs border-collapse">
                    <thead>
                      <tr className="border-b border-border/60 text-muted-foreground font-semibold">
                        <th className="py-1 px-2.5">Herramienta</th>
                        <th className="py-1 px-2.5">Servidor Activo (Retenido)</th>
                        <th className="py-1 px-2.5">Servidores Enmascarados</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-border/40 font-mono">
                      {capabilities.tool_collisions.map((c) => (
                        <tr key={c.name} className="hover:bg-amber-500/5">
                          <td className="py-1.5 px-2.5 font-bold text-foreground">{c.name}</td>
                          <td className="py-1.5 px-2.5">
                            <span className="px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border border-emerald-500/30 text-[11px]">
                              {c.kept_server}
                            </span>
                          </td>
                          <td className="py-1.5 px-2.5">
                            <span className="px-2 py-0.5 rounded bg-amber-500/15 text-amber-800 dark:text-amber-300 border border-amber-500/30 text-[11px]">
                              {c.shadowed_servers.join(', ')}
                            </span>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}

            {capabilities?.tool_collisions && capabilities.tool_collisions.length === 0 && (
              <div className="flex items-center gap-2 p-3 bg-emerald-500/10 border border-emerald-500/20 rounded-md text-emerald-800 dark:text-emerald-300 text-xs font-mono">
                <Check className="w-4 h-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
                <span>Sin colisiones de herramientas en los conectores MCP registrados</span>
              </div>
            )}

            <div className="bg-card p-4 rounded-lg border border-border shadow-sm">
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-border">
                <div>
                  <h3 className="text-sm font-bold text-foreground flex items-center gap-2">
                    <Plug className="w-4 h-4 text-primary" />
                    <span>Servidores MCP Externos Configurados</span>
                  </h3>
                  <p className="text-xs text-muted-foreground mt-0.5">
                    Procesos stdio y servicios remotos integrados mediante el protocolo MCP. Sanitización A0 activa en variables de entorno y cabeceras.
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <span className="px-2.5 py-1 rounded bg-muted text-xs font-mono text-muted-foreground border border-border flex items-center gap-1.5">
                    <Shield className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
                    <span>A0 Sanitizado</span>
                  </span>
                </div>
              </div>

              {externalServers.length === 0 ? (
                <div className="py-12 flex flex-col items-center justify-center text-center p-4">
                  <Plug className="w-8 h-8 mb-2 opacity-40 text-primary" />
                  <p className="text-xs font-bold text-foreground">Sin servidores MCP externos registrados</p>
                  <p className="text-xs text-muted-foreground max-w-sm mt-1">
                    Puede registrar servidores externos en <code>tylluan.toml</code> bajo la sección <code>[[external_mcp]]</code> o importarlos mediante la API administrativa.
                  </p>
                </div>
              ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
                  {externalServers.map((server) => (
                    <div key={server.name} className="p-4 bg-muted/30 border border-border rounded-lg space-y-3">
                      <div className="flex items-start justify-between gap-2">
                        <div>
                          <h4 className="text-sm font-bold text-foreground font-mono">{server.name}</h4>
                          {server.cwd && (
                            <p className="text-[11px] text-muted-foreground font-mono truncate max-w-xs">
                              CWD: {server.cwd}
                            </p>
                          )}
                        </div>
                        <div className="flex items-center gap-1.5 flex-wrap justify-end">
                          <span className={cn(
                            "px-2 py-0.5 rounded text-[10px] font-mono border",
                            server.active 
                              ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border-emerald-500/30"
                              : "bg-muted text-muted-foreground border-border"
                          )}>
                            {server.active ? 'Activo' : 'Inactivo'}
                          </span>
                          <span className={cn(
                            "px-2 py-0.5 rounded text-[10px] font-mono border",
                            server.running 
                              ? "bg-blue-500/15 text-blue-700 dark:text-blue-400 border-blue-500/30"
                              : "bg-muted text-muted-foreground border-border"
                          )}>
                            {server.running ? 'En Ejecución' : 'Detenido'}
                          </span>
                        </div>
                      </div>

                      {server.command && (
                        <div className="p-2.5 bg-card border border-border rounded text-xs font-mono text-foreground break-all">
                          <div className="text-[10px] text-muted-foreground mb-1 uppercase font-semibold">Comando Stdio</div>
                          <code>{server.command} {server.args?.join(' ')}</code>
                        </div>
                      )}

                      {(server.url || server.sse_url) && (
                        <div className="p-2.5 bg-card border border-border rounded text-xs font-mono text-muted-foreground break-all">
                          <div className="text-[10px] uppercase font-semibold text-muted-foreground mb-1">URL Remota</div>
                          <code>{server.url || server.sse_url}</code>
                        </div>
                      )}

                      {server.env && Object.keys(server.env).length > 0 && (
                        <div className="space-y-1.5">
                          <div className="text-[10px] uppercase font-semibold text-muted-foreground flex items-center justify-between">
                            <span>Variables de Entorno (Redactadas)</span>
                            <span className="text-emerald-600 dark:text-emerald-400 flex items-center gap-1">
                              <Shield className="w-3 h-3" /> Protegido A0
                            </span>
                          </div>
                          <div className="flex flex-wrap gap-1.5">
                            {Object.entries(server.env).map(([k, v]) => (
                              <span key={k} className="px-2 py-0.5 bg-card border border-border rounded text-[11px] font-mono text-muted-foreground">
                                <span className="text-foreground font-semibold">{k}</span>={v}
                              </span>
                            ))}
                          </div>
                        </div>
                      )}

                      {server.headers && Object.keys(server.headers).length > 0 && (
                        <div className="space-y-1.5">
                          <div className="text-[10px] uppercase font-semibold text-muted-foreground">
                            Encabezados HTTP Sanitizados
                          </div>
                          <div className="flex flex-wrap gap-1.5">
                            {Object.entries(server.headers).map(([k, v]) => (
                              <span key={k} className="px-2 py-0.5 bg-card border border-border rounded text-[11px] font-mono text-muted-foreground">
                                <span className="text-foreground font-semibold">{k}</span>={v}
                              </span>
                            ))}
                          </div>
                        </div>
                      )}

                      {server.tools && server.tools.length > 0 && (
                        <div className="space-y-1 pt-1 border-t border-border/60">
                          <div className="text-[10px] uppercase font-semibold text-muted-foreground">
                            Herramientas Expuestas ({server.tools.length})
                          </div>
                          <div className="flex flex-wrap gap-1">
                            {server.tools.map((t) => (
                              <span key={t.name} className="px-1.5 py-0.5 bg-primary/10 border border-primary/20 rounded text-[10px] font-mono text-primary">
                                {t.name}
                              </span>
                            ))}
                          </div>
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}

        {activeSubTab === 'catalog' && (
          <div className="space-y-4">
            {capabilities?.tool_collisions && capabilities.tool_collisions.length > 0 && (
              <div role="alert" className="p-4 bg-amber-500/10 border border-amber-500/30 rounded-lg text-amber-900 dark:text-amber-200 space-y-2">
                <div className="flex items-center gap-2 font-bold text-xs">
                  <AlertTriangle className="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0" />
                  <span>Colisiones de Herramientas MCP Detectadas ({capabilities.tool_collisions.length})</span>
                </div>
                <p className="text-[11px] text-muted-foreground leading-relaxed">
                  Múltiples guilds declaran herramientas con el mismo nombre. Se aplica precedencia determinista en orden alfabético: el servidor retenido ejecuta las llamadas y los demás quedan enmascarados.
                </p>
              </div>
            )}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
            {/* Left Col: Sovereign Contract */}
            <div className="space-y-4">
              <fieldset className="p-4 bg-card border border-border rounded-lg shadow-sm">
                <legend className="text-xs font-bold text-primary px-1">
                  Contrato Soberano (6 Herramientas)
                </legend>
                <p className="text-xs text-muted-foreground leading-normal mb-3">
                  CONTRACT-01 garantiza que los clientes expongan exactamente estas 6 herramientas soberanas. El trabajo especializado se canaliza a través de ellas evitando la saturación de contexto.
                </p>

                <div className="space-y-2.5">
                  {capabilities?.sovereign_contract.tools.map((t) => (
                    <div key={t.name} className="p-3 bg-muted/40 border border-border rounded-md space-y-1">
                      <div className="text-xs font-mono font-bold text-emerald-700 dark:text-emerald-400">{t.name}</div>
                      <div className="text-[11px] text-muted-foreground leading-normal">{t.description}</div>
                    </div>
                  )) ?? (
                    <div className="text-xs text-muted-foreground italic">Sin herramientas soberanas cargadas</div>
                  )}
                </div>
              </fieldset>

              {/* MCP Handshake Catalog */}
              <fieldset className="p-4 bg-card border border-border rounded-lg shadow-sm space-y-2.5">
                <legend className="text-xs font-bold text-primary px-1">
                  Registro de Descubrimiento MCP
                </legend>
                <div className="space-y-3">
                  <div>
                    <div className="text-xs font-bold text-foreground mb-1.5">Catálogo de Plantillas (Prompts)</div>
                    {capabilities?.mcp.prompts.map((p) => (
                      <div key={p.name} className="p-2.5 bg-muted/40 border border-border rounded-md space-y-0.5">
                        <div className="text-xs font-mono font-bold text-primary">{p.name}</div>
                        <div className="text-[11px] text-muted-foreground">{p.description}</div>
                      </div>
                    ))}
                  </div>
                  <div>
                    <div className="text-xs font-bold text-foreground mb-1.5">Catálogo de Recursos (Resources)</div>
                    {capabilities?.mcp.resources.map((r) => (
                      <div key={r.uri} className="p-2.5 bg-muted/40 border border-border rounded-md space-y-0.5">
                        <div className="text-xs font-mono font-bold text-primary truncate">{r.uri}</div>
                        <div className="text-[11px] text-foreground font-bold">{r.name}</div>
                        <div className="text-[11px] text-muted-foreground">{r.description}</div>
                      </div>
                    ))}
                  </div>
                </div>
              </fieldset>
            </div>

            {/* Right Col: Active Guilds and schemas */}
            <div className="lg:col-span-2 space-y-4">
              <fieldset className="p-4 bg-card border border-border rounded-lg shadow-sm">
                <legend className="text-xs font-bold text-primary px-1">
                  Módulos de Guilds Activos
                </legend>
                <div className="flex items-center justify-between mb-3">
                  <span className="text-xs text-muted-foreground font-mono">Total de herramientas subyacentes: {capabilities?.all_guild_tools.length ?? 0}</span>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  {capabilities?.guilds.map((g) => (
                    <div 
                      key={g.name} 
                      className={cn(
                        "p-3.5 rounded-lg border transition-colors flex flex-col justify-between gap-3 bg-muted/30",
                        g.running 
                          ? "border-emerald-500/30" 
                          : "border-border"
                      )}
                    >
                      <div className="flex justify-between items-start">
                        <div>
                          <div className="flex items-center gap-2">
                            <span className="font-bold text-xs text-foreground">{g.name}</span>
                            <span className={cn(
                              "w-1.5 h-1.5 rounded-full",
                              g.running ? "bg-emerald-500 animate-pulse" : "bg-muted-foreground/50"
                            )} />
                          </div>
                          <span className="text-[10px] font-mono text-muted-foreground uppercase">{g.launcher_type} launcher</span>
                        </div>
                        <span className={cn(
                          "px-2 py-0.5 rounded-md text-[10px] font-bold font-mono border",
                          g.running 
                            ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-500/30" 
                            : "bg-muted text-muted-foreground border-border"
                        )}>
                          {g.running ? 'Running' : 'On-Demand'}
                        </span>
                      </div>

                      <div className="flex justify-between items-center text-xs text-muted-foreground">
                        <span>Herramientas expuestas:</span>
                        <span className="font-mono font-bold text-foreground bg-card px-2 py-0.5 rounded-md border border-border">
                          {g.tools_count}
                        </span>
                      </div>
                    </div>
                  )) ?? (
                    <div className="col-span-2 py-10 text-center text-xs text-muted-foreground italic">Sin módulos de guilds cargados</div>
                  )}
                </div>

                {/* Guild tools index */}
                <div className="mt-5 space-y-3">
                  <div className="text-xs font-bold text-foreground border-b border-border pb-2">Esquemas de Habilidades Expuestas</div>
                  <div className="max-h-[350px] overflow-y-auto space-y-2 pr-2 custom-scrollbar">
                    {capabilities?.all_guild_tools.map((t) => (
                      <div key={t.name} className="p-3 bg-muted/40 border border-border rounded-md hover:border-primary/30 transition-colors">
                        <div className="flex justify-between items-start mb-1">
                          <span className="text-xs font-mono font-bold text-foreground">{t.name}</span>
                        </div>
                        <p className="text-[11px] text-muted-foreground leading-normal">{t.description}</p>
                      </div>
                    ))}
                  </div>
                </div>

              </fieldset>
            </div>
          </div>
        </div>
      )}

        {activeSubTab === 'guides' && (
          <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
            {/* Guide Tabs */}
            <div role="tablist" aria-label="Guías de configuración de clientes" aria-orientation="vertical" className="space-y-2">
              <button 
                ref={(el) => { guideTabRefs.current.cursor = el; }}
                type="button"
                role="tab"
                aria-selected={guideTab === 'cursor'}
                tabIndex={guideTab === 'cursor' ? 0 : -1}
                onClick={() => setGuideTab('cursor')}
                onKeyDown={(e) => handleGuideTabKeyDown(e, 'cursor')}
                className={cn(
                  "w-full text-left p-4 rounded border transition-colors font-semibold text-xs flex items-center justify-between cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                  guideTab === 'cursor' 
                    ? "bg-primary/10 border-primary text-primary" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>Cursor Setup</span>
                <ExternalLink className="w-3.5 h-3.5 opacity-50" />
              </button>

              <button 
                ref={(el) => { guideTabRefs.current.vscode = el; }}
                type="button"
                role="tab"
                aria-selected={guideTab === 'vscode'}
                tabIndex={guideTab === 'vscode' ? 0 : -1}
                onClick={() => setGuideTab('vscode')}
                onKeyDown={(e) => handleGuideTabKeyDown(e, 'vscode')}
                className={cn(
                  "w-full text-left p-4 rounded border transition-colors font-semibold text-xs flex items-center justify-between cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                  guideTab === 'vscode' 
                    ? "bg-primary/10 border-primary text-primary" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>VS Code (Cline / Roo)</span>
                <ExternalLink className="w-3.5 h-3.5 opacity-50" />
              </button>

              <button 
                ref={(el) => { guideTabRefs.current.claude = el; }}
                type="button"
                role="tab"
                aria-selected={guideTab === 'claude'}
                tabIndex={guideTab === 'claude' ? 0 : -1}
                onClick={() => setGuideTab('claude')}
                onKeyDown={(e) => handleGuideTabKeyDown(e, 'claude')}
                className={cn(
                  "w-full text-left p-4 rounded border transition-colors font-semibold text-xs flex items-center justify-between cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                  guideTab === 'claude' 
                    ? "bg-primary/10 border-primary text-primary" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>Claude Desktop</span>
                <ExternalLink className="w-3.5 h-3.5 opacity-50" />
              </button>

              <button 
                ref={(el) => { guideTabRefs.current.custom = el; }}
                type="button"
                role="tab"
                aria-selected={guideTab === 'custom'}
                tabIndex={guideTab === 'custom' ? 0 : -1}
                onClick={() => setGuideTab('custom')}
                onKeyDown={(e) => handleGuideTabKeyDown(e, 'custom')}
                className={cn(
                  "w-full text-left p-4 rounded border transition-colors font-semibold text-xs flex items-center justify-between cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                  guideTab === 'custom' 
                    ? "bg-primary/10 border-primary text-primary" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>Custom API Client</span>
                <Terminal className="w-3.5 h-3.5 opacity-50" />
              </button>
            </div>

            {/* Guide Content */}
            <div className="lg:col-span-3 bg-slate-900/30 border border-slate-800 rounded p-6 space-y-6">
              {guideTab === 'cursor' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold font-sans text-slate-50 tracking-tight">Integración MCP para Cursor</h3>
                    <p className="text-xs text-slate-400">Configure Cursor para conectarse al concentrador soberano de Tylluan mediante cliente MCP Stdio estándar.</p>
                  </div>

                  <div className="p-4 rounded bg-amber-500/5 border border-amber-500/10 flex gap-3 text-xs text-slate-400 leading-normal">
                    <Info className="w-4 h-4 text-amber-500/80 shrink-0 mt-0.5" />
                    <div>
                      <span className="font-bold text-amber-400">Clave de Seguridad:</span> La autenticación Bearer se gestiona mediante la variable de entorno <code>TYLLUAN_TOKEN</code>. El token por defecto es <code>TU_TOKEN_AQUI</code>.
                    </div>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Añada esta entrada en <code>project.json</code> o en la configuración global de Cursor:</span>
                      <button 
                        type="button"
                        onClick={() => handleCopy(cursorConfig, 'Configuración de Cursor')}
                        className="flex items-center gap-1 hover:text-slate-50 transition-colors cursor-pointer rounded px-1.5 py-0.5 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                      >
                        {copiedText === 'Configuración de Cursor' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copiar Código</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {cursorConfig}
                    </pre>
                  </div>

                  <div className="text-xs text-slate-500 leading-relaxed">
                    <span className="font-bold text-slate-400">Descubrimiento Automático:</span> Al iniciar, el agente de Cursor obtiene automáticamente el catálogo de plantillas y recursos de Tylluan. El agente leerá <code>tylluan://metadata/guilds</code> y lo cargará directamente en su contexto, conociendo exactamente los comandos a invocar mediante <code>tylluan_do</code>.
                  </div>
                </div>
              )}

              {guideTab === 'vscode' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold font-sans text-slate-50 tracking-tight">Configuración para VS Code (Cline / Roo Code)</h3>
                    <p className="text-xs text-slate-400">Añada TylluanNexus como servidor MCP personalizado en la configuración de Cline o Roo Code.</p>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Añada esta configuración a su archivo <code>cline_mcp_settings.json</code>:</span>
                      <button 
                        type="button"
                        onClick={() => handleCopy(clineConfig, 'Configuración de VS Code')}
                        className="flex items-center gap-1 hover:text-slate-50 transition-colors cursor-pointer rounded px-1.5 py-0.5 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                      >
                        {copiedText === 'Configuración de VS Code' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copiar Código</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {clineConfig}
                    </pre>
                  </div>

                  <div className="text-xs text-slate-500 leading-relaxed space-y-2">
                    <p>
                      El puente de conexión cliente (<code className="font-mono">tylluan_mcp_client.js</code>) orquesta el mapeo de transporte stdio hacia los endpoints HTTP REST de Tylluan.
                    </p>
                    <p>
                      <span className="font-bold text-slate-400">CONTRATO SOBERANO:</span> El agente Cline verá exactamente 6 herramientas soberanas. Llamará a <code>tylluan_recall</code> o <code>tylluan_graph</code> para indexación de memoria, y enrutará acciones de archivos, git y terminal mediante <code>tylluan_do</code>.
                    </p>
                  </div>
                </div>
              )}

              {guideTab === 'claude' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold font-sans text-slate-50 tracking-tight">Integración con Claude Desktop</h3>
                    <p className="text-xs text-slate-400">Integre TylluanNexus en el cliente oficial Claude Desktop de Anthropic.</p>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Añada esta entrada de servidor dentro de <code>claude_desktop_config.json</code>:</span>
                      <button 
                        type="button"
                        onClick={() => handleCopy(claudeConfig, 'Configuración de Claude Desktop')}
                        className="flex items-center gap-1 hover:text-slate-50 transition-colors cursor-pointer rounded px-1.5 py-0.5 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                      >
                        {copiedText === 'Configuración de Claude Desktop' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copiar Código</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {claudeConfig}
                    </pre>
                  </div>
                </div>
              )}

              {guideTab === 'custom' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold font-sans text-slate-50 tracking-tight">Integración mediante Cliente REST Personalizado</h3>
                    <p className="text-xs text-slate-400">Invoque TylluanNexus directamente vía API HTTP REST desde scripts de shell, programas en Python o peticiones fetch.</p>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Ejemplo de petición cURL para ejecutar una intención:</span>
                      <button 
                        type="button"
                        onClick={() => handleCopy(restCurl, 'Comando cURL')}
                        className="flex items-center gap-1 hover:text-slate-50 transition-colors cursor-pointer rounded px-1.5 py-0.5 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                      >
                        {copiedText === 'Comando cURL' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copiar Código</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {restCurl}
                    </pre>
                  </div>

                  <div className="p-4 rounded bg-slate-950 border border-slate-850 space-y-2.5">
                    <div className="text-xs font-bold text-slate-400">Cabeceras HTTP Requeridas:</div>
                    <div className="grid grid-cols-3 gap-2 text-xs font-mono">
                      <div className="text-slate-500 font-bold">Authorization</div>
                      <div className="col-span-2 text-emerald-400">Bearer TU_TOKEN_AQUI</div>

                      <div className="text-slate-500 font-bold">Content-Type</div>
                      <div className="col-span-2 text-slate-300">application/json</div>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
