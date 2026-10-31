import React, { useState, useEffect, useCallback, useMemo } from 'react';
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
  Plug
} from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import type { McpSession } from '../lib/nexus-bridge';
import { cn, relativeTime } from '../lib/utils';

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

export function ConnectorsTab({ notify }: { notify: (msg: string, type?: 'info' | 'error') => void }) {
  const { bridge, sessions: globalSessions, loading: globalLoading, refreshData } = useNexus();
  const [capabilities, setCapabilities] = useState<CapabilitiesData | null>(null);
  const [localLoading, setLocalLoading] = useState(false);
  const [activeSubTab, setActiveSubTab] = useState<'sessions' | 'catalog' | 'guides'>('sessions');
  const [guideTab, setGuideTab] = useState<'cursor' | 'vscode' | 'claude' | 'custom'>('cursor');
  const [revokingIds, setRevokingIds] = useState<Set<string>>(new Set());
  const [copiedText, setCopiedText] = useState<string | null>(null);
  const [hint, setHint] = useState<SetupHintData | null>(null);

  const fetchCapabilities = useCallback(async () => {
    if (!bridge) return;
    setLocalLoading(true);
    try {
      const [data, hintData] = await Promise.allSettled([
        bridge.getCapabilities(),
        bridge.fetchRaw('/api/v1/setup-hint'),
      ]);
      if (data.status === 'fulfilled') setCapabilities(data.value as CapabilitiesData);
      if (hintData.status === 'fulfilled') setHint(hintData.value as SetupHintData);
    } catch (e) {
      console.error('Failed to fetch capabilities:', e);
      notify('Failed to load server capabilities', 'error');
    } finally {
      setLocalLoading(false);
    }
  }, [bridge, notify]);

  useEffect(() => {
    fetchCapabilities();
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
    notify(`${label} copied to clipboard`, 'info');
    setTimeout(() => setCopiedText(null), 2000);
  };

  // Snippets are built from the kernel's setup-hint (URL derived from the Host
  // the dashboard used to reach it — correct for legacy 3030, local 4000, or any
  // custom port). Never hardcode the port.
  const sseUrl = hint?.mcp_clients?.claude_desktop?.config?.mcpServers?.tylluan?.url
    ?? `${bridge?.getBaseUrl() ?? window.location.origin}/sse`;

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

  const restCurl = `curl -X POST ${hint?.verify?.dashboard ?? `${bridge?.getBaseUrl() ?? window.location.origin}`}/api/v1/do \\
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
    <div className="space-y-6 p-6">
      {/* Header Banner */}
      <div className="bg-card p-6 rounded border border-border flex flex-col md:flex-row md:items-center justify-between gap-6">
        <div className="space-y-2">
          <div className="flex items-center gap-3">
            <div className="p-2.5 bg-primary/10 border border-primary/20 rounded flex items-center justify-center">
              <Link2 className="w-5 h-5 text-primary" />
            </div>
            <div>
              <h1 className="text-2xl font-bold font-sans text-foreground tracking-tight">Connectors & Discovery</h1>
              <p className="text-xs text-muted-foreground font-mono">Milestone M24 Sovereign Integrations</p>
            </div>
          </div>
          <p className="text-sm text-muted-foreground max-w-xl">
            TylluanNexus acts as a Sovereign MCP Hub. External client LLMs connect via standard MCP or REST APIs.
            The client LLM discovers capabilities dynamically via Prompts & Resources, keeping contexts light.
          </p>
        </div>

        <div className="flex items-center gap-3 shrink-0 self-end md:self-center">
          <button 
            onClick={handleRefresh}
            disabled={globalLoading || localLoading}
            className="flex items-center gap-2 px-4 py-2.5 bg-slate-900 border border-slate-800 rounded hover:bg-slate-800 hover:border-slate-700 text-xs font-bold text-slate-300 transition-colors disabled:opacity-50"
          >
            <RefreshCw className={cn("w-4 h-4 text-emerald-400", (globalLoading || localLoading) && "animate-spin")} />
            <span>Reload Status</span>
          </button>
        </div>
      </div>

      {/* Tabs Selector */}
      <div className="flex border-b border-slate-800 gap-6">
        <button 
          onClick={() => setActiveSubTab('sessions')}
          className={cn(
            "px-3 py-1 text-[11px] font-bold border transition-all cursor-pointer rounded-t-sm -mb-px",
            activeSubTab === 'sessions' 
              ? "bg-white dark:bg-card border-[#ACA899] dark:border-border border-b-transparent text-[#01285F] dark:text-primary shadow-none" 
              : "bg-[#D8D4C4] dark:bg-muted/40 border-transparent text-[#555555] dark:text-muted-foreground hover:bg-[#EAE7DC] hover:text-[#000000]"
          )}
        >
          <div className="flex items-center gap-1.5">
            <Users className="w-3.5 h-3.5 text-[#0A246A]" aria-hidden="true" />
            <span>Sesiones Activas</span>
            {activeSessions.length > 0 && (
              <span className="px-1.5 py-0.2 rounded-none bg-emerald-500/15 text-[10px] font-mono text-emerald-700 dark:text-emerald-400 border border-emerald-500/30">
                {activeSessions.length}
              </span>
            )}
          </div>
        </button>

        <button 
          onClick={() => setActiveSubTab('catalog')}
          className={cn(
            "px-3 py-1 text-[11px] font-bold border transition-all cursor-pointer rounded-t-sm -mb-px",
            activeSubTab === 'catalog' 
              ? "bg-white dark:bg-card border-[#ACA899] dark:border-border border-b-transparent text-[#01285F] dark:text-primary shadow-none" 
              : "bg-[#D8D4C4] dark:bg-muted/40 border-transparent text-[#555555] dark:text-muted-foreground hover:bg-[#EAE7DC] hover:text-[#000000]"
          )}
        >
          <div className="flex items-center gap-1.5">
            <Cpu className="w-3.5 h-3.5 text-[#0A246A]" aria-hidden="true" />
            <span>Catálogo de Capacidades</span>
            {capabilities && (
              <span className="px-1.5 py-0.2 rounded-none bg-blue-500/15 text-[10px] font-mono text-blue-700 dark:text-blue-400 border border-blue-500/30">
                {capabilities.guilds.length} Guilds
              </span>
            )}
          </div>
        </button>

        <button 
          onClick={() => setActiveSubTab('guides')}
          className={cn(
            "px-3 py-1 text-[11px] font-bold border transition-all cursor-pointer rounded-t-sm -mb-px",
            activeSubTab === 'guides' 
              ? "bg-white dark:bg-card border-[#ACA899] dark:border-border border-b-transparent text-[#01285F] dark:text-primary shadow-none" 
              : "bg-[#D8D4C4] dark:bg-muted/40 border-transparent text-[#555555] dark:text-muted-foreground hover:bg-[#EAE7DC] hover:text-[#000000]"
          )}
        >
          <div className="flex items-center gap-1.5">
            <BookOpen className="w-3.5 h-3.5 text-[#0A246A]" aria-hidden="true" />
            <span>Guías de Configuración</span>
          </div>
        </button>
      </div>

      {/* Tab Panels */}
      <div className="space-y-4">
        {activeSubTab === 'sessions' && (
          <div className="space-y-3">
            <div className="bg-white dark:bg-card rounded-none border border-[#ACA899] dark:border-border overflow-x-auto">
              <table className="w-full text-left border-collapse text-[11px] font-sans">
                <thead>
                  <tr className="bg-[#ECE9D8] dark:bg-muted/60 border-b border-[#ACA899] dark:border-border text-[#01285F] dark:text-foreground font-bold">
                    <th className="px-3 py-1.5 border-r border-[#ACA899] dark:border-border">Cliente / Proceso</th>
                    <th className="px-3 py-1.5 border-r border-[#ACA899] dark:border-border">Identificación del Agente</th>
                    <th className="px-3 py-1.5 text-center border-r border-[#ACA899] dark:border-border">Herramientas Invocadas</th>
                    <th className="px-3 py-1.5 border-r border-[#ACA899] dark:border-border">Guild Activo</th>
                    <th className="px-3 py-1.5 border-r border-[#ACA899] dark:border-border">Última Actividad</th>
                    <th className="px-3 py-1.5 text-right">Revocar Acceso</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/60">
                  {activeSessions.map((session) => (
                    <tr key={session.id} className="group hover:bg-slate-850/20 transition-colors">
                      <td className="px-6 py-4">
                        <div className="flex items-center gap-3">
                          <div className="w-9 h-9 rounded bg-slate-800/50 flex items-center justify-center border border-slate-700/50 text-slate-400 group-hover:text-emerald-400 transition-colors">
                            <User className="w-4.5 h-4.5" />
                          </div>
                          <div>
                            <div className="text-sm font-bold text-slate-200">{session.client_name}</div>
                            <div className="text-[11px] font-mono text-slate-500">ID: {session.id.slice(0, 16)}...</div>
                          </div>
                        </div>
                      </td>
                      <td className="px-6 py-4">
                        {session.agent_id ? (
                          <div className="flex items-center gap-2">
                            <Shield className="w-3.5 h-3.5 text-emerald-400/80" />
                            <span className="text-xs font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20 truncate max-w-[180px]">
                              {session.agent_id}
                            </span>
                          </div>
                        ) : (
                          <span className="text-xs text-slate-600 italic">Unidentified agent</span>
                        )}
                      </td>
                      <td className="px-6 py-4 text-center">
                        <span className="px-2 py-1 rounded bg-slate-900 border border-slate-800 text-xs font-mono text-slate-300">
                          {session.tool_count}
                        </span>
                      </td>
                      <td className="px-6 py-4">
                        {session.last_guild ? (
                          <div className="flex items-center gap-2">
                            <Activity className="w-3.5 h-3.5 text-blue-400/80" />
                            <span className="text-xs font-mono text-blue-400 bg-blue-500/10 px-2.5 py-0.5 rounded border border-blue-500/20">
                              {session.last_guild}
                            </span>
                          </div>
                        ) : (
                          <span className="text-xs text-slate-700 font-mono">—</span>
                        )}
                      </td>
                      <td className="px-6 py-4">
                        <div className="flex items-center gap-2 text-xs text-slate-400 font-mono">
                          <Clock className="w-3.5 h-3.5 text-slate-600" />
                          {relativeTime(session.last_active_unix)}
                        </div>
                      </td>
                      <td className="px-6 py-4 text-right">
                        <button
                          onClick={() => handleRevoke(session.id)}
                          disabled={revokingIds.has(session.id)}
                          className={cn(
                            "p-2 rounded transition-colors border",
                            revokingIds.has(session.id) 
                              ? "bg-slate-800 border-slate-700 cursor-not-allowed text-slate-500" 
                              : "bg-red-500/10 hover:bg-red-500/25 text-red-400 border-red-500/20 hover:border-red-500/40"
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
                  <Plug className="w-8 h-8 mb-2 opacity-30 text-[#0A246A] dark:text-primary" />
                  <p className="text-xs font-bold text-[#01285F] dark:text-foreground">Sin sesiones de clientes activas</p>
                  <p className="text-[11px] text-[#444444] dark:text-muted-foreground max-w-sm mt-0.5">
                    Conecte un cliente externo compatible con MCP (Claude Desktop, Cursor, Code) para interactuar con las herramientas soberanas del microkernel.
                  </p>
                </div>
              )}
            </div>
          </div>
        )}

        {activeSubTab === 'catalog' && (
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-3">
            {/* Left Col: Sovereign Contract */}
            <div className="space-y-3">
              <fieldset className="p-3 bg-white dark:bg-card border border-[#ACA899] dark:border-border rounded-none">
                <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">
                  Contrato Soberano (6 Herramientas)
                </legend>
                <p className="text-[11px] text-[#444444] dark:text-muted-foreground leading-normal mb-2.5">
                  CONTRACT-01 garantiza que los clientes expongan exactamente estas 6 herramientas soberanas. El trabajo especializado se canaliza a través de ellas evitando la saturación de contexto.
                </p>

                <div className="space-y-2.5">
                  {capabilities?.sovereign_contract.tools.map((t) => (
                    <div key={t.name} className="p-3 bg-slate-950/60 border border-slate-850 rounded space-y-1">
                      <div className="text-xs font-mono font-bold text-emerald-400">{t.name}</div>
                      <div className="text-[11px] text-slate-400 leading-normal">{t.description}</div>
                    </div>
                  )) ?? (
                    <div className="text-xs text-slate-500 italic">No contract tools loaded</div>
                  )}
                </div>
              </fieldset>

              {/* MCP Handshake Catalog */}
              <fieldset className="p-3 bg-white dark:bg-card border border-[#ACA899] dark:border-border rounded-none space-y-2.5">
                <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">
                  Registro de Descubrimiento MCP
                </legend>
                <div className="space-y-2.5">
                  <div>
                    <div className="text-[11px] font-bold text-[#01285F] dark:text-foreground mb-1">Catálogo de Plantillas (Prompts)</div>
                    {capabilities?.mcp.prompts.map((p) => (
                      <div key={p.name} className="p-2 bg-[#F5F4EA] dark:bg-muted/40 border border-[#ACA899] dark:border-border rounded-none space-y-0.5">
                        <div className="text-[11px] font-mono font-bold text-[#0A246A] dark:text-primary">{p.name}</div>
                        <div className="text-[10px] text-[#444444] dark:text-muted-foreground">{p.description}</div>
                      </div>
                    ))}
                  </div>
                  <div>
                    <div className="text-[11px] font-bold text-[#01285F] dark:text-foreground mb-1">Catálogo de Recursos (Resources)</div>
                    {capabilities?.mcp.resources.map((r) => (
                      <div key={r.uri} className="p-2 bg-[#F5F4EA] dark:bg-muted/40 border border-[#ACA899] dark:border-border rounded-none space-y-0.5">
                        <div className="text-[11px] font-mono font-bold text-[#0A246A] dark:text-primary truncate">{r.uri}</div>
                        <div className="text-[10px] text-[#000000] dark:text-foreground font-bold">{r.name}</div>
                        <div className="text-[10px] text-[#444444] dark:text-muted-foreground">{r.description}</div>
                      </div>
                    ))}
                  </div>
                </div>
              </fieldset>
            </div>

            {/* Right Col: Active Guilds and schemas */}
            <div className="lg:col-span-2 space-y-3">
              <fieldset className="p-3 bg-white dark:bg-card border border-[#ACA899] dark:border-border rounded-none">
                <legend className="text-[11px] font-bold text-[#0A246A] dark:text-primary px-1">
                  Módulos de Guilds Activos
                </legend>
                <div className="flex items-center justify-between mb-2">
                  <span className="text-[11px] text-[#444444] dark:text-muted-foreground font-mono">Total de herramientas subyacentes: {capabilities?.all_guild_tools.length ?? 0}</span>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  {capabilities?.guilds.map((g) => (
                    <div 
                      key={g.name} 
                      className={cn(
                        "p-4 rounded border transition-colors flex flex-col justify-between gap-3 bg-slate-950/40",
                        g.running 
                          ? "border-emerald-500/20" 
                          : "border-slate-800"
                      )}
                    >
                      <div className="flex justify-between items-start">
                        <div>
                          <div className="flex items-center gap-2">
                            <span className="font-bold text-sm text-slate-200">{g.name}</span>
                            <span className={cn(
                              "w-1.5 h-1.5 rounded-full",
                              g.running ? "bg-emerald-400 animate-pulse" : "bg-slate-600"
                            )} />
                          </div>
                          <span className="text-[11px] font-mono text-slate-500 uppercase">{g.launcher_type} launcher</span>
                        </div>
                        <span className={cn(
                          "px-2 py-0.5 rounded text-[11px] font-bold font-mono border",
                          g.running 
                            ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20" 
                            : "bg-slate-800 text-slate-500 border-slate-700"
                        )}>
                          {g.running ? 'Running' : 'On-Demand'}
                        </span>
                      </div>

                      <div className="flex justify-between items-center text-xs text-slate-400">
                        <span>Tools exposed:</span>
                        <span className="font-mono font-bold text-slate-200 bg-slate-850 px-2 py-0.5 rounded border border-slate-850">
                          {g.tools_count}
                        </span>
                      </div>
                    </div>
                  )) ?? (
                    <div className="col-span-2 py-10 text-center text-xs text-slate-500 italic">No guild modules loaded</div>
                  )}
                </div>

                {/* Guild tools index */}
                <div className="mt-6 space-y-3">
                  <div className="text-xs font-bold text-slate-400 border-b border-slate-850 pb-2">Exposed Skill Schemas</div>
                  <div className="max-h-[350px] overflow-y-auto space-y-2.5 pr-2 custom-scrollbar">
                    {capabilities?.all_guild_tools.map((t) => (
                      <div key={t.name} className="p-3 bg-slate-950/60 border border-slate-850 rounded hover:border-slate-800 transition-colors">
                        <div className="flex justify-between items-start mb-1">
                          <span className="text-xs font-mono font-bold text-slate-200">{t.name}</span>
                        </div>
                        <p className="text-[11px] text-slate-400 leading-normal">{t.description}</p>
                      </div>
                    ))}
                  </div>
                </div>

              </fieldset>
            </div>
          </div>
        )}

        {activeSubTab === 'guides' && (
          <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
            {/* Guide Tabs */}
            <div className="space-y-2">
              <button 
                onClick={() => setGuideTab('cursor')}
                className={cn(
                  "w-full text-left p-4 rounded border transition-colors font-semibold text-xs flex items-center justify-between",
                  guideTab === 'cursor' 
                    ? "bg-primary/10 border-primary text-primary" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>Cursor Setup</span>
                <ExternalLink className="w-3.5 h-3.5 opacity-50" />
              </button>

              <button 
                onClick={() => setGuideTab('vscode')}
                className={cn(
                  "w-full text-left p-4 rounded border transition-colors font-semibold text-xs flex items-center justify-between",
                  guideTab === 'vscode' 
                    ? "bg-primary/10 border-primary text-primary" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>VS Code (Cline / Roo)</span>
                <ExternalLink className="w-3.5 h-3.5 opacity-50" />
              </button>

              <button 
                onClick={() => setGuideTab('claude')}
                className={cn(
                  "w-full text-left p-4 rounded border transition-colors font-semibold text-xs flex items-center justify-between",
                  guideTab === 'claude' 
                    ? "bg-primary/10 border-primary text-primary" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>Claude Desktop</span>
                <ExternalLink className="w-3.5 h-3.5 opacity-50" />
              </button>

              <button 
                onClick={() => setGuideTab('custom')}
                className={cn(
                  "w-full text-left p-4 rounded border transition-colors font-semibold text-xs flex items-center justify-between",
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
                    <h3 className="text-base font-bold font-sans text-slate-50 tracking-tight">Cursor MCP Integration</h3>
                    <p className="text-xs text-slate-400">Configure Cursor to connect to Tylluan's sovereign hub using standard MCP Stdio Client.</p>
                  </div>

                  <div className="p-4 rounded bg-amber-500/5 border border-amber-500/10 flex gap-3 text-xs text-slate-400 leading-normal">
                    <Info className="w-4 h-4 text-amber-500/80 shrink-0 mt-0.5" />
                    <div>
                      <span className="font-bold text-amber-400">Security Key:</span> Bearer authentication is managed by setting the <code>TYLLUAN_TOKEN</code> environment variable. The default token is <code>TU_TOKEN_AQUI</code>.
                    </div>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Add this entry inside <code>project.json</code> or global Cursor config:</span>
                      <button 
                        onClick={() => handleCopy(cursorConfig, 'Cursor Config')}
                        className="flex items-center gap-1 hover:text-slate-50 transition-colors"
                      >
                        {copiedText === 'Cursor Config' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copy Code</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {cursorConfig}
                    </pre>
                  </div>

                  <div className="text-xs text-slate-500 leading-relaxed">
                    <span className="font-bold text-slate-400">How Auto-Discovery works:</span> Upon initialization, the Cursor agent automatically fetches the Prompts/Resources catalog from Tylluan. The agent will read <code>tylluan://metadata/guilds</code> and load it directly into its prompt context, allowing it to know exactly which commands to call inside <code>tylluan_do</code>.
                  </div>
                </div>
              )}

              {guideTab === 'vscode' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold font-sans text-slate-50 tracking-tight">VS Code (Cline / Roo Code) Configuration</h3>
                    <p className="text-xs text-slate-400">Add TylluanNexus as a custom MCP server inside Cline or Roo Code extension settings.</p>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Add this configuration to your <code>cline_mcp_settings.json</code>:</span>
                      <button 
                        onClick={() => handleCopy(clineConfig, 'VS Code Config')}
                        className="flex items-center gap-1 hover:text-slate-50 transition-colors"
                      >
                        {copiedText === 'VS Code Config' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copy Code</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {clineConfig}
                    </pre>
                  </div>

                  <div className="text-xs text-slate-500 leading-relaxed space-y-2">
                    <p>
                      The client bridge file is located at <code>C:/Users/<usuario>/.gemini/antigravity/mcp/tylluan_mcp_client.js</code> and orchestrates stdio transport mapping to Tylluan's HTTP REST endpoints.
                    </p>
                    <p>
                      <span className="font-bold text-slate-400">HANDSHAKE CONTRACT:</span> The Cline agent will see exactly 6 sovereign tools. It will call <code>tylluan_recall</code> or <code>tylluan_graph</code> for memory indexing, and route all filesystem, git, and bash intents dynamically via <code>tylluan_do</code>.
                    </p>
                  </div>
                </div>
              )}

              {guideTab === 'claude' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold font-sans text-slate-50 tracking-tight">Claude Desktop Integration</h3>
                    <p className="text-xs text-slate-400">Integrate TylluanNexus into the official Anthropic Claude Desktop client.</p>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Add this server entry inside your <code>claude_desktop_config.json</code>:</span>
                      <button 
                        onClick={() => handleCopy(claudeConfig, 'Claude Desktop Config')}
                        className="flex items-center gap-1 hover:text-slate-50 transition-colors"
                      >
                        {copiedText === 'Claude Desktop Config' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copy Code</span>
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
                    <h3 className="text-base font-bold font-sans text-slate-50 tracking-tight">Custom REST Client / Script Integration</h3>
                    <p className="text-xs text-slate-400">Call TylluanNexus directly via HTTP REST API from custom shell scripts, Python programs, or fetch calls.</p>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Example cURL request to execute an intent:</span>
                      <button 
                        onClick={() => handleCopy(restCurl, 'cURL command')}
                        className="flex items-center gap-1 hover:text-slate-50 transition-colors"
                      >
                        {copiedText === 'cURL command' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copy Code</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {restCurl}
                    </pre>
                  </div>

                  <div className="p-4 rounded bg-slate-950 border border-slate-850 space-y-2.5">
                    <div className="text-xs font-bold text-slate-400">HTTP Headers Required:</div>
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
